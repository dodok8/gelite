use rusqlite::Connection;
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{
    SQLiteRunner, SQLiteRunnerError,
    rusqlite_support::{
        execute, execute_with_values, first_three_column_row, sqlite_error, table_exists,
    },
};

/// Browser WASM SQLite runner backed by an owned in-memory connection.
pub struct WasmSQLiteRunner {
    connection: Connection,
}

impl WasmSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| sqlite_error("open in-memory SQLite database", error))?;
        let runner = Self { connection };
        execute(&runner.connection, "PRAGMA foreign_keys = ON")?;

        Ok(runner)
    }

    pub fn close(self) -> Result<(), SQLiteRunnerError> {
        self.connection
            .close()
            .map_err(|(_, error)| sqlite_error("close SQLite database", error))
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool, SQLiteRunnerError> {
        table_exists(&self.connection, table_name)
    }

    /// Reads the first row as owned values for backend smoke tests.
    pub fn first_three_column_row(
        &self,
        sql: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
        first_three_column_row(&self.connection, sql)
    }
}

impl SQLiteRunner for WasmSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        execute(&self.connection, sql)
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        execute_with_values(&self.connection, sql, values)
    }
}
