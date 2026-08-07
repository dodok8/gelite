mod fixtures;

use schema_model::SchemaCatalog;
use sqlite_query_sqlgen::SQLiteBindValue;
use sqlite_runner::{SQLiteRunner, SQLiteRunnerError};
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{QueryKind, SchemaPlanStatement, apply_schema, compile_query, plan_schema};
use fixtures::blog_schema_source;

fn blog_catalog() -> SchemaCatalog {
    schema_parser::parse_schema(blog_schema_source()).expect("blog schema should parse")
}

#[derive(Default)]
struct RecordingRunner {
    calls: Vec<String>,
}

impl SQLiteRunner for RecordingRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.calls.push(sql.to_string());
        Ok(())
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        self.calls.push(format!("{sql} {values:?}"));
        Ok(())
    }
}

#[test]
fn schema_plan_command_renders_initial_schema_from_source() {
    let output = plan_schema(blog_schema_source()).expect("schema plan command should succeed");
    let statements = output.statements();

    assert_eq!(statements.len(), 13);
    assert!(
        statements[0]
            .sql()
            .starts_with("CREATE TABLE \"_engine_schema_versions\"")
    );
    assert!(
        statements[1]
            .sql()
            .starts_with("CREATE TABLE \"_engine_catalog_objects\"")
    );
    assert!(
        statements[2]
            .sql()
            .starts_with("CREATE TABLE \"_engine_catalog_fields\"")
    );
    assert!(statements[3].sql().starts_with("CREATE TABLE \"user\""));
    assert!(statements[4].sql().starts_with("CREATE TABLE \"post\""));
    assert_eq!(
        statements[12].sql(),
        "CREATE INDEX \"post__author_id_idx\" ON \"post\" (\"author_id\")"
    );
}

#[test]
fn schema_plan_command_preserves_metadata_bind_values() {
    let output = plan_schema(blog_schema_source()).expect("schema plan command should succeed");

    let post_object_insert = output
        .statements()
        .iter()
        .find(|statement| {
            matches!(
                statement,
                SchemaPlanStatement::Insert { values, .. }
                    if values == &[
                        SQLiteValuePlan::Integer(2),
                        SQLiteValuePlan::Text("Post".into()),
                    ]
            )
        })
        .expect("Post catalog object insert should exist");

    assert_eq!(
        post_object_insert.sql(),
        "INSERT INTO \"_engine_catalog_objects\" (\"object_id\", \"name\") VALUES (?, ?)"
    );
    assert_eq!(
        post_object_insert.values(),
        Some(
            [
                SQLiteValuePlan::Integer(2),
                SQLiteValuePlan::Text("Post".into()),
            ]
            .as_slice()
        )
    );
}

#[test]
fn schema_plan_command_returns_parse_error_for_invalid_schema() {
    let error = plan_schema(
        "type Post {
  required link author: Missing
}",
    )
    .expect_err("invalid schema should fail");

    assert!(error.message().contains("failed to parse schema"));
    assert!(!error.message().is_empty());
}

#[test]
fn schema_apply_command_executes_rendered_schema_statements() {
    let mut runner = RecordingRunner::default();

    apply_schema(blog_schema_source(), &mut runner).expect("schema apply command should succeed");

    assert_eq!(runner.calls.len(), 13);
    assert!(
        runner.calls[0].starts_with("CREATE TABLE \"_engine_schema_versions\""),
        "metadata table should be created first"
    );
    assert!(
        runner
            .calls
            .iter()
            .any(|call| call.contains("INSERT INTO \"_engine_catalog_objects\"")),
        "catalog object metadata should be inserted"
    );
}

#[test]
fn query_command_compiles_select() {
    let compiled =
        compile_query(&blog_catalog(), "select Post { title }").expect("select should compile");

    assert_eq!(compiled.kind, QueryKind::Select);
    assert_eq!(
        compiled.statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\""
    );
}

#[test]
fn query_command_compiles_insert_with_generated_id() {
    let compiled = compile_query(
        &blog_catalog(),
        r#"insert User { email := "sheri@example.com" }"#,
    )
    .expect("insert should compile");
    let QueryKind::Insert { generated_id } = &compiled.kind else {
        panic!("expected insert query kind");
    };

    assert_eq!(
        uuid::Uuid::parse_str(generated_id)
            .expect("generated id should be a UUID")
            .get_version(),
        Some(uuid::Version::Random)
    );
    assert_eq!(
        compiled.statement.bind_values().first(),
        Some(&SQLiteBindValue::String(generated_id.clone()))
    );
}

#[test]
fn query_command_compiles_update() {
    let compiled = compile_query(
        &blog_catalog(),
        r#"update Post filter .title = "Draft" set { title := "Reviewed" }"#,
    )
    .expect("update should compile");

    assert_eq!(compiled.kind, QueryKind::Update);
    assert_eq!(
        compiled.statement.sql(),
        "UPDATE \"post\" AS \"root\" SET \"title\" = ? WHERE \"root\".\"title\" = ?"
    );
}

#[test]
fn query_command_compiles_delete() {
    let compiled = compile_query(&blog_catalog(), r#"delete Post filter .title = "Draft""#)
        .expect("delete should compile");

    assert_eq!(compiled.kind, QueryKind::Delete);
    assert_eq!(
        compiled.statement.sql(),
        "DELETE FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );
}

#[test]
fn query_command_reports_dispatch_parse_and_resolution_errors() {
    for (source, expected) in [
        ("truncate Post", "query must start"),
        ("select", "failed to parse query"),
        ("select Missing { id }", "failed to resolve query"),
    ] {
        let error = match compile_query(&blog_catalog(), source) {
            Ok(_) => panic!("query should fail"),
            Err(error) => error,
        };

        assert!(error.message().contains(expected), "{}", error.message());
    }
}
