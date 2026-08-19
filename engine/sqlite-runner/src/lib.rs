#![no_std]

//! Runner-facing contracts for executing rendered SQLite statements.
//!
//! This crate does not choose a concrete SQLite binding. Its contracts let
//! command code apply rendered schemas and execute rendered data statements
//! without knowing whether the backend is native, embedded, or WASM-based.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
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
    Null,
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
