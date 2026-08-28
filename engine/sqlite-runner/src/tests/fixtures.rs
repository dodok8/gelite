extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use schema_model::{Field, ObjectType, ScalarField, ScalarType, SchemaCatalog, SingleCardinality};
use sqlite_schema_plan::{SQLiteValuePlan, plan_initial_schema};
use sqlite_schema_sqlgen::{RenderedSchemaStatement, render_initial_schema};

use crate::{SQLiteRunner, SQLiteRunnerError, SQLiteTransactionRunner};

pub const VERSION_ID: &str = "9b496060-9a5c-4c7e-9f32-210f698fe497";
pub const APPLIED_AT: &str = "2026-08-28T12:34:56.789Z";

#[derive(Debug, PartialEq, Eq)]
pub enum RecordedCall {
    Execute(String),
    ExecuteWithValues(String, Vec<SQLiteValuePlan>),
}

#[derive(Default)]
pub struct RecordingRunner {
    calls: Vec<RecordedCall>,
    fails_on_sql: Option<String>,
}

impl RecordingRunner {
    pub fn calls(&self) -> &[RecordedCall] {
        &self.calls
    }

    pub fn fail_on_sql(sql: impl Into<String>) -> Self {
        Self {
            calls: Vec::new(),
            fails_on_sql: Some(sql.into()),
        }
    }

    fn should_fail_sql(&self, sql: &str) -> bool {
        self.fails_on_sql.as_deref() == Some(sql)
    }
}

impl SQLiteRunner for RecordingRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.calls.push(RecordedCall::Execute(sql.to_string()));

        if self.should_fail_sql(sql) {
            return Err(SQLiteRunnerError::execution_failed(
                "recorded runner failure",
            ));
        }

        Ok(())
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        self.calls.push(RecordedCall::ExecuteWithValues(
            sql.to_string(),
            values.to_vec(),
        ));

        if self.should_fail_sql(sql) {
            return Err(SQLiteRunnerError::execution_failed(
                "recorded runner failure",
            ));
        }

        Ok(())
    }
}

impl SQLiteTransactionRunner for RecordingRunner {
    fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("BEGIN")
    }

    fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("COMMIT")
    }

    fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("ROLLBACK")
    }
}

pub fn post_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![Field::Scalar(ScalarField::new(
            "title",
            ScalarType::Str,
            SingleCardinality::Required,
        ))],
    )])
    .expect("test catalog should be valid")
}

pub fn rendered_post_schema_statements() -> Vec<RenderedSchemaStatement> {
    let catalog = post_catalog();
    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    render_initial_schema(&plan)
}
