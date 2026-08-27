extern crate alloc;

mod fixtures;
#[cfg(feature = "native")]
mod native;

use alloc::string::ToString;
use alloc::vec;
use sqlite_schema_plan::{SQLiteValuePlan, plan_initial_schema};
use sqlite_schema_sqlgen::{RenderedSchemaStatement, render_initial_schema};

use crate::{SQLiteRunnerError, apply_schema_statements};
use fixtures::{RecordedCall, RecordingRunner, post_catalog, rendered_post_schema_statements};

#[test]
fn apply_schema_statements_executes_sql_and_insert_statements_in_order() {
    let catalog = post_catalog();
    let plan = plan_initial_schema(&catalog);
    let statements = render_initial_schema(&plan);
    let mut runner = RecordingRunner::default();

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    assert!(matches!(
        runner.calls().first(),
        Some(RecordedCall::Execute(sql)) if sql.starts_with("CREATE TABLE \"_engine_schema_versions\"")
    ));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::Execute(sql) if sql.starts_with("CREATE TABLE \"post\"")
    )));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::ExecuteWithValues(sql, values)
            if sql == "INSERT INTO \"_engine_catalog_objects\" (\"object_id\", \"name\") VALUES (?, ?)"
                && values == &vec![
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Text("Post".to_string()),
                ]
    )));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::ExecuteWithValues(sql, values)
            if sql == "INSERT INTO \"_engine_catalog_fields\" (\"object_id\", \"field_id\", \"name\", \"field_kind\", \"cardinality\", \"scalar_type\", \"target_object_id\", \"is_implicit\", \"is_unique\", \"inverse_field_name\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                && values == &vec![
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Text("id".to_string()),
                    SQLiteValuePlan::Text("scalar".to_string()),
                    SQLiteValuePlan::Text("required".to_string()),
                    SQLiteValuePlan::Text("uuid".to_string()),
                    SQLiteValuePlan::Null,
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Integer(0),
            SQLiteValuePlan::Null,
                ]
    )));
}

#[test]
fn apply_schema_statements_stops_after_insert_failure() {
    let statements = rendered_post_schema_statements();

    let failing_sql = statements
        .iter()
        .find_map(|statement| match statement {
            RenderedSchemaStatement::Insert(insert) => Some(insert.sql().to_string()),
            RenderedSchemaStatement::Sql(_) => None,
        })
        .expect("rendered schema should contain metadata insert");

    let mut runner = RecordingRunner::fail_on_sql(failing_sql.clone());

    let result = apply_schema_statements(&mut runner, &statements);

    assert_eq!(
        result,
        Err(SQLiteRunnerError::execution_failed(
            "recorded runner failure"
        ))
    );

    let failing_call_index = runner
        .calls()
        .iter()
        .position(|call| match call {
            RecordedCall::ExecuteWithValues(sql, _) => sql == &failing_sql,
            RecordedCall::Execute(_) => false,
        })
        .expect("failing insert should be attempted");

    assert_eq!(runner.calls().len(), failing_call_index + 1);
}

#[test]
fn apply_schema_statements_preserves_empty_statement_list() {
    let statements = [];
    let mut runner = RecordingRunner::default();

    let result = apply_schema_statements(&mut runner, &statements);

    assert_eq!(result, Ok(()));
    assert!(runner.calls().is_empty());
}

#[test]
fn apply_schema_statements_stops_after_execute_failure() {
    let statements = rendered_post_schema_statements();

    let failing_sql = statements
        .iter()
        .find_map(|statement| match statement {
            RenderedSchemaStatement::Sql(sql) => Some(sql.clone()),
            RenderedSchemaStatement::Insert(_) => None,
        })
        .expect("rendered schema should contain raw SQL statement");

    let mut runner = RecordingRunner::fail_on_sql(failing_sql.clone());

    let result = apply_schema_statements(&mut runner, &statements);

    assert_eq!(
        result,
        Err(SQLiteRunnerError::execution_failed(
            "recorded runner failure"
        ))
    );
    assert_eq!(runner.calls(), &[RecordedCall::Execute(failing_sql)]);
}
