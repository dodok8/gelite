#![no_std]

//! Runner-facing contracts for executing rendered SQLite statements.
//!
//! This crate does not choose a concrete SQLite binding. Its contracts let
//! command code apply rendered schemas and execute rendered data statements
//! without knowing whether the backend is native, embedded, or WASM-based.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
#[cfg(feature = "native")]
use sqlite_query_sqlgen::SQLiteResultShape;
use sqlite_query_sqlgen::SQLiteStatement;
use sqlite_schema_plan::SQLiteValuePlan;
use sqlite_schema_sqlgen::RenderedSchemaStatement;

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wasm")]
pub mod wasm;

/// Error type returned by runner operations.
///
/// The first version only needs a binding-neutral execution failure. Concrete
/// backends can convert their driver errors into this type without exposing the
/// driver through public planner or command APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SQLiteRunnerError {
    ExecutionFailed { message: String },
}

impl SQLiteRunnerError {
    pub fn execution_failed(message: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::ExecutionFailed { message } => message,
        }
    }
}

/// Minimal SQLite execution contract used by schema application.
///
/// `execute` is for raw SQL statements such as DDL. `execute_with_values` is
/// for prepared statements whose values must stay separate from SQL text.
pub trait SQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError>;

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteQueryResult {
    columns: Vec<String>,
    rows: Vec<Vec<SQLiteCellValue>>,
}

impl SQLiteQueryResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<SQLiteCellValue>>) -> Self {
        Self { columns, rows }
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn rows(&self) -> &[Vec<SQLiteCellValue>] {
        &self.rows
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteCellValue {
    Integer(i64),
    Real(f64),
    Text(String),
    Object(Vec<(String, SQLiteCellValue)>),
    Null,
}

#[cfg(feature = "native")]
fn shape_query_result(
    shape: &SQLiteResultShape,
    rows: Vec<Vec<SQLiteCellValue>>,
) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
    let columns = shape
        .fields()
        .iter()
        .map(|field| field.output_name().into())
        .collect();
    let rows = rows
        .iter()
        .map(|row| shape_fields(shape, row))
        .collect::<Result<_, _>>()?;

    Ok(SQLiteQueryResult::new(columns, rows))
}

#[cfg(feature = "native")]
fn shape_fields(
    shape: &SQLiteResultShape,
    row: &[SQLiteCellValue],
) -> Result<Vec<SQLiteCellValue>, SQLiteRunnerError> {
    shape
        .fields()
        .iter()
        .map(|field| match (field.column_index(), field.nested_shape()) {
            (Some(index), None) => row.get(index).cloned().ok_or_else(|| {
                SQLiteRunnerError::execution_failed(
                    "result shape column index exceeds SQLite column count",
                )
            }),
            (None, Some(nested_shape)) => shape_object(nested_shape, row),
            _ => Err(SQLiteRunnerError::execution_failed(
                "result shape field must contain either a column or a nested shape",
            )),
        })
        .collect()
}

#[cfg(feature = "native")]
fn shape_object(
    shape: &SQLiteResultShape,
    row: &[SQLiteCellValue],
) -> Result<SQLiteCellValue, SQLiteRunnerError> {
    if let Some(index) = shape.identity_column_index() {
        match row.get(index) {
            Some(SQLiteCellValue::Null) => return Ok(SQLiteCellValue::Null),
            Some(_) => {}
            None => {
                return Err(SQLiteRunnerError::execution_failed(
                    "result shape identity index exceeds SQLite column count",
                ));
            }
        }
    }

    Ok(SQLiteCellValue::Object(
        shape
            .fields()
            .iter()
            .zip(shape_fields(shape, row)?)
            .map(|(field, value)| (field.output_name().into(), value))
            .collect(),
    ))
}

/// Binding-neutral execution contract for rendered data statements.
pub trait SQLiteQueryRunner {
    fn execute_select(
        &mut self,
        statement: &SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError>;

    fn execute_insert(&mut self, statement: &SQLiteStatement) -> Result<(), SQLiteRunnerError>;

    fn execute_update(&mut self, statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError>;

    fn execute_delete(&mut self, statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError>;
}

/// Binding-neutral transaction contract for runners that keep one connection.
pub trait SQLiteTransactionRunner {
    fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError>;

    fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError>;

    fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError>;
}

/// Applies rendered schema statements through a runner implementation.
///
/// Statement order is preserved. Raw SQL statements are sent through
/// `SQLiteRunner::execute`; metadata inserts are sent through
/// `SQLiteRunner::execute_with_values` with their bind values unchanged.
pub fn apply_schema_statements(
    runner: &mut impl SQLiteRunner,
    statements: &[RenderedSchemaStatement],
) -> Result<(), SQLiteRunnerError> {
    for statement in statements {
        match statement {
            RenderedSchemaStatement::Sql(sql) => runner.execute(sql)?,
            RenderedSchemaStatement::Insert(insert) => {
                runner.execute_with_values(insert.sql(), insert.values())?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
