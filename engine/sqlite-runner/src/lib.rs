#![no_std]

//! Runner-facing contracts for executing rendered SQLite statements.
//!
//! This crate does not choose a concrete SQLite binding. Its contracts let
//! command code apply rendered schemas and execute rendered data statements
//! without knowing whether the backend is native, embedded, or WASM-based.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
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
    parent_identities: Vec<Option<String>>,
    follow_up_parent_identities: Vec<Vec<Option<String>>>,
}

impl SQLiteQueryResult {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<SQLiteCellValue>>) -> Self {
        let row_count = rows.len();

        Self {
            columns,
            rows,
            parent_identities: vec![None; row_count],
            follow_up_parent_identities: vec![vec![]; row_count],
        }
    }

    pub fn with_identities(
        columns: Vec<String>,
        rows: Vec<Vec<SQLiteCellValue>>,
        parent_identities: Vec<Option<String>>,
        follow_up_parent_identities: Vec<Vec<Option<String>>>,
    ) -> Self {
        Self {
            columns,
            rows,
            parent_identities,
            follow_up_parent_identities,
        }
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn rows(&self) -> &[Vec<SQLiteCellValue>] {
        &self.rows
    }

    pub fn rows_mut(&mut self) -> &mut [Vec<SQLiteCellValue>] {
        &mut self.rows
    }

    pub fn parent_identities(&self) -> &[Option<String>] {
        &self.parent_identities
    }

    pub fn follow_up_parent_identities(&self) -> &[Vec<Option<String>>] {
        &self.follow_up_parent_identities
    }

    pub fn into_parent_rows(self) -> Vec<(Option<String>, Vec<SQLiteCellValue>)> {
        self.parent_identities.into_iter().zip(self.rows).collect()
    }

    pub fn clear_internal_identities(&mut self) {
        self.parent_identities.fill(None);
        self.follow_up_parent_identities
            .iter_mut()
            .for_each(Vec::clear);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteCellValue {
    Integer(i64),
    Real(f64),
    Text(String),
    Object(Vec<(String, SQLiteCellValue)>),
    List(Vec<SQLiteCellValue>),
    Null,
}

#[cfg(feature = "native")]
fn shape_fields_with_identities(
    shape: &SQLiteResultShape,
    row: &mut [SQLiteCellValue],
    follow_up_parent_identities: &mut [Option<String>],
) -> Result<Vec<SQLiteCellValue>, SQLiteRunnerError> {
    shape
        .fields()
        .iter()
        .map(|field| {
            match (
                field.column_index(),
                field.nested_shape(),
                field.follow_up_fetch_index(),
            ) {
                (Some(index), None, None) => row
                    .get_mut(index)
                    .map(|value| core::mem::replace(value, SQLiteCellValue::Null))
                    .ok_or_else(|| {
                        SQLiteRunnerError::execution_failed(
                            "result shape column index exceeds SQLite column count",
                        )
                    }),
                (None, Some(nested_shape), None) => {
                    shape_object(nested_shape, row, follow_up_parent_identities)
                }
                (None, None, Some(fetch_index)) => {
                    let identity = identity_at(shape.identity_column_index(), row)?;
                    let target = follow_up_parent_identities
                        .get_mut(fetch_index)
                        .ok_or_else(|| {
                            SQLiteRunnerError::execution_failed(
                                "follow-up fetch index exceeds result identity metadata",
                            )
                        })?;
                    *target = identity;
                    Ok(SQLiteCellValue::List(vec![]))
                }
                _ => Err(SQLiteRunnerError::execution_failed(
                    "result shape field must contain a column, nested shape, or follow-up fetch",
                )),
            }
        })
        .collect()
}

#[cfg(feature = "native")]
fn shape_object(
    shape: &SQLiteResultShape,
    row: &mut [SQLiteCellValue],
    follow_up_parent_identities: &mut [Option<String>],
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
            .zip(shape_fields_with_identities(
                shape,
                row,
                follow_up_parent_identities,
            )?)
            .map(|(field, value)| (field.output_name().into(), value))
            .collect(),
    ))
}

#[cfg(feature = "native")]
fn follow_up_fetch_count(shape: &SQLiteResultShape) -> usize {
    shape.fields().iter().fold(0, |count, field| {
        let own_count = field.follow_up_fetch_index().map_or(0, |index| index + 1);
        let nested_count = field.nested_shape().map_or(0, follow_up_fetch_count);

        count.max(own_count).max(nested_count)
    })
}

#[cfg(feature = "native")]
fn identity_at(
    index: Option<usize>,
    row: &[SQLiteCellValue],
) -> Result<Option<String>, SQLiteRunnerError> {
    let Some(index) = index else {
        return Ok(None);
    };

    match row.get(index) {
        Some(SQLiteCellValue::Text(identity)) => Ok(Some(identity.clone())),
        Some(SQLiteCellValue::Null) => Ok(None),
        Some(_) => Err(SQLiteRunnerError::execution_failed(
            "result shape identity is not text",
        )),
        None => Err(SQLiteRunnerError::execution_failed(
            "parent identity index exceeds SQLite column count",
        )),
    }
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
