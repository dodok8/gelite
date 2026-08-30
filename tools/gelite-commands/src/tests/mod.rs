mod fixtures;

use schema_model::SchemaCatalog;
use sqlite_query_sqlgen::SQLiteBindValue;
use sqlite_query_sqlgen::SQLiteStatement;
use sqlite_runner::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteQueryRunner, SQLiteRunner, SQLiteRunnerError,
    SQLiteSchemaReader, SQLiteStoredSchema, SQLiteTransactionRunner,
};
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{
    CompiledScriptStatement, QueryKind, SchemaPlanStatement, apply_schema, compile_query,
    compile_script, execute_query, execute_script, format_query_result, plan_schema,
};
use fixtures::blog_schema_source;

fn blog_catalog() -> SchemaCatalog {
    schema_parser::parse_schema(blog_schema_source()).expect("blog schema should parse")
}

#[derive(Default)]
struct RecordingRunner {
    calls: Vec<String>,
    inserts: Vec<(String, Vec<SQLiteValuePlan>)>,
    stored_schema: Option<SQLiteStoredSchema>,
}

impl RecordingRunner {
    fn with_stored_schema(catalog: SchemaCatalog, version_number: i64) -> Self {
        Self {
            stored_schema: Some(SQLiteStoredSchema {
                catalog,
                version_number,
            }),
            ..Self::default()
        }
    }
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
        self.inserts.push((sql.to_string(), values.to_vec()));
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

impl SQLiteSchemaReader for RecordingRunner {
    fn load_verified_schema(&mut self) -> Result<Option<SQLiteStoredSchema>, SQLiteRunnerError> {
        Ok(self.stored_schema.clone())
    }
}

#[derive(Default)]
struct RecordingQueryRunner {
    calls: Vec<&'static str>,
    fail: bool,
}

struct MultiLinkQueryRunner {
    results: Vec<SQLiteQueryResult>,
    calls: Vec<(String, Vec<SQLiteBindValue>)>,
}

impl SQLiteQueryRunner for MultiLinkQueryRunner {
    fn execute_select(
        &mut self,
        statement: &SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        self.calls.push((
            statement.sql().to_string(),
            statement.bind_values().to_vec(),
        ));
        Ok(self.results.remove(0))
    }

    fn execute_insert(&mut self, _statement: &SQLiteStatement) -> Result<(), SQLiteRunnerError> {
        unreachable!()
    }

    fn execute_update(&mut self, _statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError> {
        unreachable!()
    }

    fn execute_delete(&mut self, _statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError> {
        unreachable!()
    }
}

impl SQLiteQueryRunner for RecordingQueryRunner {
    fn execute_select(
        &mut self,
        _statement: &SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        self.calls.push("select");
        if self.fail {
            return Err(SQLiteRunnerError::execution_failed("test failure"));
        }
        Ok(SQLiteQueryResult::new(
            vec!["title".to_string()],
            vec![vec![SQLiteCellValue::Text("Case File".to_string())]],
        ))
    }

    fn execute_insert(&mut self, _statement: &SQLiteStatement) -> Result<(), SQLiteRunnerError> {
        self.calls.push("insert");
        Ok(())
    }

    fn execute_update(&mut self, _statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError> {
        self.calls.push("update");
        Ok(2)
    }

    fn execute_delete(&mut self, _statement: &SQLiteStatement) -> Result<i64, SQLiteRunnerError> {
        self.calls.push("delete");
        Ok(3)
    }
}

impl SQLiteTransactionRunner for RecordingQueryRunner {
    fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.calls.push("begin");
        Ok(())
    }

    fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.calls.push("commit");
        Ok(())
    }

    fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.calls.push("rollback");
        Ok(())
    }
}

#[test]
fn schema_plan_command_renders_initial_schema_from_source() {
    let output = plan_schema(blog_schema_source()).expect("schema plan command should succeed");
    let statements = output.statements();

    assert_eq!(statements.len(), 14);
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
    assert_eq!(
        statements[13].sql(),
        "INSERT INTO \"_engine_schema_versions\" (\"version_id\", \"checksum\", \"applied_at\", \"schema_snapshot\", \"version_number\") VALUES (?, ?, ?, ?, ?)"
    );
    let values = statements[13].values().expect("version bindings");
    assert_eq!(values[4], SQLiteValuePlan::Integer(1));
    assert_eq!(
        values[0],
        SQLiteValuePlan::Text("<version-id-on-apply>".into())
    );
    assert_eq!(
        values[2],
        SQLiteValuePlan::Text("<applied-at-on-apply>".into())
    );
    assert_eq!(output, plan_schema(blog_schema_source()).unwrap());
}

#[test]
fn schema_preview_content_ignores_comments_layout_and_declaration_order() {
    let original = plan_schema(blog_schema_source()).expect("original schema");
    let reordered = plan_schema(
        "# same logical schema\ntype Post { required link author: User # author\n required title: str }\n\n\ttype User { required email: str } # end",
    )
    .expect("equivalent schema");
    assert_eq!(original.statements().last(), reordered.statements().last());
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
    let before = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());

    apply_schema(blog_schema_source(), &mut runner).expect("schema apply command should succeed");
    let after = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());

    assert_eq!(runner.calls.len(), 16);
    assert_eq!(runner.calls.first().map(String::as_str), Some("BEGIN"));
    assert_eq!(runner.calls.last().map(String::as_str), Some("COMMIT"));
    assert!(
        runner.calls[1].starts_with("CREATE TABLE \"_engine_schema_versions\""),
        "metadata table should be created first"
    );
    assert!(
        runner
            .calls
            .iter()
            .any(|call| call.contains("INSERT INTO \"_engine_catalog_objects\"")),
        "catalog object metadata should be inserted"
    );
    let preview = plan_schema(blog_schema_source()).expect("preview");
    let expected = preview.statements().last().expect("version INSERT");
    let (sql, values) = runner.inserts.last().expect("applied version INSERT");
    assert_eq!(sql, expected.sql());
    assert_eq!(values[1], expected.values().unwrap()[1]);
    assert_eq!(values[3], expected.values().unwrap()[3]);
    let [
        SQLiteValuePlan::Text(id),
        _,
        SQLiteValuePlan::Text(applied_at),
        _,
        SQLiteValuePlan::Integer(1),
    ] = values.as_slice()
    else {
        panic!(
            "version identifier and timestamp must be text and the initial version number must be 1"
        );
    };
    let parsed_id = uuid::Uuid::parse_str(id).expect("valid UUID");
    assert_eq!(parsed_id.get_version_num(), 4);
    assert_eq!(*id, parsed_id.hyphenated().to_string());
    let timestamp = chrono::DateTime::parse_from_rfc3339(applied_at).expect("RFC3339 timestamp");
    assert_eq!(applied_at.len(), 24, "exactly three fractional digits");
    assert!(applied_at.ends_with('Z'));
    assert!(timestamp.timestamp_millis() >= before.timestamp_millis());
    assert!(timestamp.timestamp_millis() <= after.timestamp_millis());

    let mut second = RecordingRunner::default();
    apply_schema(blog_schema_source(), &mut second).expect("another apply attempt");
    assert_ne!(values[0], second.inserts.last().unwrap().1[0]);
}

#[test]
fn schema_apply_reparses_changed_source_and_rejects_invalid_input_before_execution() {
    let original = plan_schema(blog_schema_source()).expect("preview");
    let changed = "type User { required name: str }";
    let expected = plan_schema(changed).expect("changed preview");
    let mut runner = RecordingRunner::default();
    apply_schema(changed, &mut runner).expect("apply changed schema");
    let (_, values) = runner.inserts.last().expect("version INSERT");
    assert_eq!(
        values[3],
        expected.statements().last().unwrap().values().unwrap()[3]
    );
    assert_ne!(
        values[3],
        original.statements().last().unwrap().values().unwrap()[3]
    );

    let mut invalid = RecordingRunner::default();
    assert!(apply_schema("type User { link missing: Unknown }", &mut invalid).is_err());
    assert!(invalid.calls.is_empty());
}

#[test]
fn schema_apply_command_applies_migrations_and_appends_the_next_version() {
    let current = schema_parser::parse_schema("type User { name: str }").unwrap();
    let desired = "type User { name: str nickname: str }";
    let desired_catalog = schema_parser::parse_schema(desired).unwrap();
    let mut runner = RecordingRunner::with_stored_schema(current, 7);

    apply_schema(desired, &mut runner).expect("supported migration should apply");

    assert_eq!(runner.calls.first().map(String::as_str), Some("BEGIN"));
    assert_eq!(runner.calls.last().map(String::as_str), Some("COMMIT"));
    assert!(
        runner
            .calls
            .iter()
            .any(|call| { call == "ALTER TABLE \"user\" ADD COLUMN \"nickname\" TEXT NULL" })
    );
    let (_, values) = runner
        .inserts
        .last()
        .expect("version INSERT should be last");
    assert_eq!(values[4], SQLiteValuePlan::Integer(8));
    assert_eq!(
        values[3],
        SQLiteValuePlan::Text(
            sqlite_schema_plan::serialize_schema_snapshot(&desired_catalog).unwrap()
        )
    );
}

#[test]
fn schema_apply_command_skips_empty_migrations() {
    let source = "type User { name: str }";
    let catalog = schema_parser::parse_schema(source).unwrap();
    let mut runner = RecordingRunner::with_stored_schema(catalog, 3);

    apply_schema(source, &mut runner).expect("identical schema should be a no-op");

    assert!(runner.calls.is_empty());
    assert!(runner.inserts.is_empty());
}

#[test]
fn schema_apply_command_rejects_unsupported_changes_and_version_overflow_before_writes() {
    let current = schema_parser::parse_schema("type User { name: str }").unwrap();
    let mut unsupported = RecordingRunner::with_stored_schema(current.clone(), 1);
    let error = apply_schema("type User {}", &mut unsupported)
        .expect_err("field removal should be unsupported");
    assert!(error.message().contains("FieldRemoval"));
    assert!(unsupported.calls.is_empty());

    let mut overflow = RecordingRunner::with_stored_schema(current, i64::MAX);
    let error = apply_schema("type User { name: str nickname: str }", &mut overflow)
        .expect_err("version overflow should fail");
    assert_eq!(error.message(), "schema version number exceeds i64 range");
    assert!(overflow.calls.is_empty());
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
fn query_command_batches_and_merges_multi_link_execution() {
    let catalog = schema_parser::parse_schema(
        "type User {
  required email: str
  multi link posts: Post
}

type Post {
  required title: str
}",
    )
    .expect("multi-link schema should parse");
    let root = SQLiteQueryResult::with_identities(
        vec!["email".to_string(), "posts".to_string()],
        vec![
            vec![
                SQLiteCellValue::Text("sheri@example.com".to_string()),
                SQLiteCellValue::List(vec![]),
            ],
            vec![
                SQLiteCellValue::Text("emma@example.com".to_string()),
                SQLiteCellValue::List(vec![]),
            ],
        ],
        vec![None, None],
        vec![
            vec![Some("user-1".to_string())],
            vec![Some("user-2".to_string())],
        ],
    );
    let posts = SQLiteQueryResult::with_identities(
        vec!["title".to_string()],
        vec![
            vec![SQLiteCellValue::Text("Case File".to_string())],
            vec![SQLiteCellValue::Text("Archive".to_string())],
        ],
        vec![Some("user-1".to_string()), Some("user-2".to_string())],
        vec![vec![], vec![]],
    );
    let mut runner = MultiLinkQueryRunner {
        results: vec![root, posts],
        calls: vec![],
    };

    let query = compile_query(&catalog, "select User { email, posts: { title } }")
        .expect("multi-link select should compile");
    let result = execute_query(&mut runner, query).expect("multi-link select should execute");

    assert_eq!(
        result.rows(),
        &[
            vec![
                SQLiteCellValue::Text("sheri@example.com".to_string()),
                SQLiteCellValue::List(vec![SQLiteCellValue::Object(vec![(
                    "title".to_string(),
                    SQLiteCellValue::Text("Case File".to_string()),
                )])]),
            ],
            vec![
                SQLiteCellValue::Text("emma@example.com".to_string()),
                SQLiteCellValue::List(vec![SQLiteCellValue::Object(vec![(
                    "title".to_string(),
                    SQLiteCellValue::Text("Archive".to_string()),
                )])]),
            ],
        ]
    );
    assert_eq!(runner.calls.len(), 2);
    assert_eq!(
        runner.calls[1].1,
        [
            SQLiteBindValue::String("user-1".to_string()),
            SQLiteBindValue::String("user-2".to_string()),
        ]
    );
}

#[test]
fn query_command_reports_deferred_multi_link_plans() {
    let catalog = schema_parser::parse_schema(
        "type User {\n  multi link posts: Post\n}\n\ntype Post {\n  required title: str\n}",
    )
    .expect("multi-link schema should parse");

    let query = compile_query(&catalog, "select User { posts: { title } }")
        .expect("multi-link select should compile");

    assert_eq!(
        query.deferred_follow_up_plan_message().as_deref(),
        Some(
            "Deferred follow-up plans: 1 (query batches are determined after parent identities are known)"
        )
    );
}

#[test]
fn query_command_chunks_multi_link_parent_ids_within_sqlite_bind_limit() {
    let catalog = schema_parser::parse_schema(
        "type User {\n  multi link posts: Post\n}\n\ntype Post {\n  required view_count: int64\n}",
    )
    .expect("multi-link schema should parse");
    let parent_count = 1_000;
    let root = SQLiteQueryResult::with_identities(
        vec!["posts".to_string()],
        (0..parent_count)
            .map(|_| vec![SQLiteCellValue::List(vec![])])
            .collect(),
        vec![None; parent_count],
        (0..parent_count)
            .map(|index| vec![Some(format!("user-{index}"))])
            .collect(),
    );
    let empty_posts = || SQLiteQueryResult::new(vec!["score".to_string()], vec![]);
    let mut runner = MultiLinkQueryRunner {
        results: vec![root, empty_posts(), empty_posts()],
        calls: vec![],
    };

    let query = compile_query(
        &catalog,
        "select User { posts: { score := .view_count + 1 } }",
    )
    .expect("multi-link select should compile");
    execute_query(&mut runner, query).expect("multi-link select should execute");

    assert_eq!(
        runner
            .calls
            .iter()
            .skip(1)
            .map(|(_, bind_values)| bind_values.len())
            .collect::<Vec<_>>(),
        [999, 3]
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
        (
            "start transaction",
            "transaction commands require a database-backed interactive REPL",
        ),
        ("select", "failed to parse query"),
        ("select Post { title } delete Post", "failed to parse query"),
        ("select Missing { id }", "failed to resolve query"),
    ] {
        let error = match compile_query(&blog_catalog(), source) {
            Ok(_) => panic!("query should fail"),
            Err(error) => error,
        };

        assert!(error.message().contains(expected), "{}", error.message());
    }
}

#[test]
fn query_command_executes_all_supported_statement_kinds() {
    let catalog = blog_catalog();
    let mut runner = RecordingQueryRunner::default();

    let select = execute_query(
        &mut runner,
        compile_query(&catalog, "select Post { title }").unwrap(),
    )
    .unwrap();
    assert_eq!(
        select.rows(),
        &[vec![SQLiteCellValue::Text("Case File".to_string())]]
    );
    assert_eq!(format_query_result(&select), "title\nCase File");

    let insert =
        compile_query(&catalog, r#"insert User { email := "sheri@example.com" }"#).unwrap();
    let QueryKind::Insert { generated_id } = &insert.kind else {
        panic!("expected insert query kind");
    };
    let generated_id = generated_id.clone();
    assert_eq!(
        execute_query(&mut runner, insert).unwrap().rows(),
        &[vec![SQLiteCellValue::Text(generated_id)]]
    );

    let update = compile_query(&catalog, r#"update Post set { title := "Reviewed" }"#).unwrap();
    assert_eq!(
        execute_query(&mut runner, update).unwrap().rows(),
        &[vec![SQLiteCellValue::Integer(2)]]
    );

    let delete = compile_query(&catalog, "delete Post").unwrap();
    assert_eq!(
        execute_query(&mut runner, delete).unwrap().rows(),
        &[vec![SQLiteCellValue::Integer(3)]]
    );

    assert_eq!(runner.calls, ["select", "insert", "update", "delete"]);
}

#[test]
fn format_query_result_renders_nested_objects_and_null() {
    let result = SQLiteQueryResult::new(
        vec!["title".to_string(), "author".to_string()],
        vec![
            vec![
                SQLiteCellValue::Text("Draft".to_string()),
                SQLiteCellValue::Object(vec![(
                    "email".to_string(),
                    SQLiteCellValue::Text("alice@example.com".to_string()),
                )]),
            ],
            vec![
                SQLiteCellValue::Text("Orphaned".to_string()),
                SQLiteCellValue::Null,
            ],
        ],
    );

    assert_eq!(
        format_query_result(&result),
        "title\tauthor\nDraft\t{email: alice@example.com}\nOrphaned\tNULL"
    );
}

#[test]
fn query_command_reports_execution_errors() {
    let mut runner = RecordingQueryRunner {
        fail: true,
        ..Default::default()
    };
    let query = compile_query(&blog_catalog(), "select Post { title }").unwrap();

    let error = execute_query(&mut runner, query).expect_err("execution should fail");

    assert_eq!(error.message(), "test failure");
}

#[test]
fn query_script_compiles_and_executes_in_order() {
    let script = compile_script(
        &blog_catalog(),
        "start transaction; insert User { email := \"sheri@example.com\" }; commit; select Post { title };",
    )
    .expect("script should compile");

    assert_eq!(script.statements().len(), 4);
    assert_eq!(script.statements()[0].sql(), "BEGIN TRANSACTION");
    assert!(matches!(
        script.statements()[1],
        CompiledScriptStatement::Query(_)
    ));

    let mut runner = RecordingQueryRunner::default();
    let results = execute_script(&mut runner, script).expect("script should execute");

    assert_eq!(runner.calls, ["begin", "insert", "commit", "select"]);
    assert!(results[0].is_none());
    assert!(results[1].is_some());
    assert!(results[2].is_none());
    assert!(results[3].is_some());
}

#[test]
fn query_script_validates_all_transactions_before_execution() {
    for (source, expected) in [
        ("commit;", "no transaction is active"),
        (
            "start transaction; start transaction; commit;",
            "nested transactions are not supported",
        ),
        (
            "start transaction; select Post { title };",
            "transaction is still active at end of script",
        ),
    ] {
        let error = match compile_script(&blog_catalog(), source) {
            Ok(_) => panic!("script should fail"),
            Err(error) => error,
        };
        assert!(error.message().contains(expected), "{}", error.message());
        assert!(error.message().contains("line 1, column"));
    }
}

#[test]
fn query_script_rolls_back_an_active_transaction_after_runtime_failure() {
    let script = compile_script(
        &blog_catalog(),
        "start transaction; select Post { title }; commit;",
    )
    .expect("script should compile");
    let mut runner = RecordingQueryRunner {
        fail: true,
        ..Default::default()
    };

    let error = execute_script(&mut runner, script).expect_err("script should fail");

    assert_eq!(error.message(), "statement 2: test failure");
    assert_eq!(runner.calls, ["begin", "select", "rollback"]);
}
