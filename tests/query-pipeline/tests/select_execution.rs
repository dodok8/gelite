use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use gelite_commands::{apply_schema, plan_schema};
use sqlite_query_sqlgen::{
    SQLiteBindValue, SQLiteStatement, render_delete, render_insert, render_select, render_update,
};
use sqlite_runner::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteRunner, SQLiteRunnerError, apply_schema_statements,
    native::NativeSQLiteRunner,
};
use sqlite_schema_plan::SQLiteValuePlan;

const BLOG_SCHEMA_SOURCE: &str = r#"
type User {
  required unique email: str
  required score: int64
  link best_friend: User
  multi link posts: Post
}

type Post {
  required title: str
  required view_count: int64
  required link author: User
}
"#;

#[test]
fn schema_apply_records_preview_content_and_preserves_the_initial_baseline() {
    let preview = plan_schema(BLOG_SCHEMA_SOURCE).expect("preview");
    let expected = preview.statements().last().unwrap().values().unwrap();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema(BLOG_SCHEMA_SOURCE, &mut runner).expect("initial apply");
    let statement = SQLiteStatement::new(
        "SELECT version_id, checksum, applied_at, schema_snapshot, version_number FROM _engine_schema_versions",
        vec![],
    );
    let stored = runner.execute_select(&statement).expect("stored version");
    let [row] = stored.rows() else {
        panic!("initial apply must store exactly one version row");
    };
    let [
        SQLiteCellValue::Text(id),
        SQLiteCellValue::Text(checksum),
        SQLiteCellValue::Text(applied_at),
        SQLiteCellValue::Text(snapshot),
        SQLiteCellValue::Integer(1),
    ] = row.as_slice()
    else {
        panic!("version values must be text with initial version number 1");
    };
    assert_ne!(id, "<version-id-on-apply>");
    assert_ne!(applied_at, "<applied-at-on-apply>");
    assert_eq!(SQLiteValuePlan::Text(checksum.clone()), expected[1]);
    assert_eq!(SQLiteValuePlan::Text(snapshot.clone()), expected[3]);
    runner
        .verify_schema_version()
        .expect("applied logical schema should verify");

    assert!(apply_schema(BLOG_SCHEMA_SOURCE, &mut runner).is_err());
    let after = runner
        .execute_select(&statement)
        .expect("original baseline");
    assert_eq!(after.rows(), stored.rows());
    runner
        .verify_schema_version()
        .expect("failed reapplication must preserve a verifiable baseline");
}

#[test]
fn schema_apply_stores_identical_content_despite_comments_and_whitespace() {
    let commented = BLOG_SCHEMA_SOURCE
        .lines()
        .map(|line| format!("\t{line}  # same logical schema\r\n"))
        .collect::<String>();
    let statement = SQLiteStatement::new(
        "SELECT checksum, schema_snapshot FROM _engine_schema_versions",
        vec![],
    );
    let stored = [BLOG_SCHEMA_SOURCE, commented.as_str()].map(|source| {
        let mut runner = NativeSQLiteRunner::open_in_memory().expect("database");
        apply_schema(source, &mut runner).expect("schema should apply");
        runner
            .verify_schema_version()
            .expect("equivalent source should produce a verifiable baseline");
        runner.execute_select(&statement).expect("stored content")
    });

    assert_eq!(stored[0].rows().len(), 1);
    assert_eq!(stored[0].rows(), stored[1].rows());
}

#[test]
fn schema_apply_detects_tampered_version_content_and_logical_catalog() {
    // Raw SQL corrupts internal metadata that schema commands do not allow users to edit.
    for mutation in [
        "UPDATE _engine_schema_versions SET checksum = 'tampered'",
        "UPDATE _engine_schema_versions SET schema_snapshot = schema_snapshot || ' '",
        "UPDATE _engine_catalog_fields SET cardinality = 'optional' WHERE name = 'title'",
    ] {
        let mut runner = NativeSQLiteRunner::open_in_memory().expect("database");
        apply_schema(BLOG_SCHEMA_SOURCE, &mut runner).expect("schema should apply");
        runner
            .verify_schema_version()
            .expect("original baseline should verify");
        runner
            .execute(mutation)
            .expect("metadata should be changed");

        let error = runner
            .verify_schema_version()
            .expect_err("modified metadata must fail verification");
        assert!(
            matches!(error, SQLiteRunnerError::SchemaVerificationFailed { .. }),
            "{mutation}: {error:?}"
        );
    }
}

#[test]
fn schema_version_verifies_after_reopening_database_without_source_file() {
    let source_path = write_temp_geli_schema(BLOG_SCHEMA_SOURCE);
    let database_path = source_path.with_extension("db");
    let source = fs::read_to_string(&source_path).expect("source should load");
    let mut runner = NativeSQLiteRunner::open(database_path.to_str().expect("UTF-8 path"))
        .expect("database should open");
    apply_schema(&source, &mut runner).expect("schema should apply");
    drop(runner);
    drop(source);
    fs::remove_file(source_path).expect("original source should be removed");

    let mut reopened = NativeSQLiteRunner::open(database_path.to_str().expect("UTF-8 path"))
        .expect("database should reopen");
    let result = reopened.verify_schema_version();
    drop(reopened);
    fs::remove_file(database_path).expect("test database should be removed");

    result.expect("stored snapshot should verify without the source or original connection");
}

static TEMP_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse_blog_catalog_from_geli_file() -> schema_model::SchemaCatalog {
    let path = write_temp_geli_schema(BLOG_SCHEMA_SOURCE);
    let source = fs::read_to_string(&path).expect("temporary .geli schema should be readable");
    let catalog = schema_parser::parse_schema(&source).expect("schema source should parse");
    fs::remove_file(&path).expect("temporary .geli schema should be removed");

    catalog
}

fn write_temp_geli_schema(source: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gelite-query-pipeline-{}-{}.geli",
        std::process::id(),
        unique_suffix()
    ));

    fs::write(&path, source).expect("temporary .geli schema should be writable");

    path
}

fn unique_suffix() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    let counter = TEMP_SCHEMA_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{timestamp}-{counter}")
}

fn setup_blog_database() -> NativeSQLiteRunner {
    let catalog = parse_blog_catalog_from_geli_file();
    let schema_plan = sqlite_schema_plan::plan_initial_schema(
        &catalog,
        "9b496060-9a5c-4c7e-9f32-210f698fe497",
        "2026-08-28T12:34:56.789Z",
    )
    .expect("schema snapshot should serialize");
    let schema_statements = sqlite_schema_sqlgen::render_initial_schema(&schema_plan);
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &schema_statements)
        .expect("schema statements should apply");

    insert_blog_fixture_rows(&mut runner, &catalog);

    runner
}

fn insert_blog_fixture_rows(
    runner: &mut NativeSQLiteRunner,
    catalog: &schema_model::SchemaCatalog,
) {
    execute_insert(
        runner,
        catalog,
        r#"insert User {
            email := "carol@example.com",
            score := 50,
            best_friend := null,
        }"#,
        "user-3",
    );
    execute_insert(
        runner,
        catalog,
        r#"insert User {
            email := "blocked@example.com",
            score := 0,
            best_friend := "user-3",
        }"#,
        "user-2",
    );
    execute_insert(
        runner,
        catalog,
        r#"insert User {
            email := "alice@example.com",
            score := 100,
            best_friend := "user-2",
        }"#,
        "user-1",
    );
    execute_insert(
        runner,
        catalog,
        r#"insert Post {
            title := "Draft",
            view_count := 5,
            author := "user-1",
        }"#,
        "post-1",
    );
    execute_insert(
        runner,
        catalog,
        r#"insert Post {
            title := "Published",
            view_count := 20,
            author := "user-1",
        }"#,
        "post-2",
    );
    execute_insert(
        runner,
        catalog,
        r#"insert Post {
            title := "Archived",
            view_count := 100,
            author := "user-2",
        }"#,
        "post-3",
    );

    // Multi-link mutation syntax is outside the first insert milestone, so the
    // join-table fixture rows remain explicit storage setup for select tests.
    runner
        .execute_with_values(
            "INSERT INTO user__posts (source_id, target_id, position) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-1".to_string()),
                SQLiteValuePlan::Text("post-1".to_string()),
                SQLiteValuePlan::Integer(0),
            ],
        )
        .expect("first multi-link fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO user__posts (source_id, target_id, position) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-1".to_string()),
                SQLiteValuePlan::Text("post-2".to_string()),
                SQLiteValuePlan::Integer(1),
            ],
        )
        .expect("second multi-link fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO user__posts (source_id, target_id, position) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-2".to_string()),
                SQLiteValuePlan::Text("post-3".to_string()),
                SQLiteValuePlan::Integer(0),
            ],
        )
        .expect("third multi-link fixture row should insert");
}

fn execute_insert(
    runner: &mut NativeSQLiteRunner,
    catalog: &schema_model::SchemaCatalog,
    source: &str,
    generated_id: &str,
) {
    let ast = query_parser::parse_insert(source).expect("fixture insert should parse");
    let ir = query_resolver::resolve_insert(catalog, &ast).expect("fixture insert should resolve");
    let plan = sqlite_query_plan::plan_insert(&ir);
    let statement = render_insert(&plan, generated_id);

    runner
        .execute_insert(&statement)
        .expect("fixture insert should execute");
}

fn render_query(source: &str) -> SQLiteStatement {
    let catalog = parse_blog_catalog_from_geli_file();
    let ast = query_parser::parse_select(source).expect("query should parse");
    let ir = query_resolver::resolve_select(&catalog, &ast).expect("query should resolve");
    let plan = sqlite_query_plan::plan_select(&ir);

    render_select(&plan)
}

fn execute_query(source: &str) -> SQLiteQueryResult {
    let mut runner = setup_blog_database();
    let statement = render_query(source);

    runner
        .execute_select(&statement)
        .expect("select statement should execute")
}

fn execute_command_query(runner: &mut NativeSQLiteRunner, source: &str) -> SQLiteQueryResult {
    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");
    let query = gelite_commands::compile_query(&catalog, source).expect("query should compile");

    gelite_commands::execute_query(runner, query).expect("query should execute")
}

fn execute_update(runner: &mut NativeSQLiteRunner, source: &str) -> i64 {
    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");
    let ast = query_parser::parse_update(source).expect("update should parse");
    let ir = query_resolver::resolve_update(&catalog, &ast).expect("update should resolve");
    let plan = sqlite_query_plan::plan_update(&ir);
    let statement = render_update(&plan);

    runner
        .execute_update(&statement)
        .expect("update statement should execute")
}

fn execute_delete(runner: &mut NativeSQLiteRunner, source: &str) -> i64 {
    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");
    let ast = query_parser::parse_delete(source).expect("delete should parse");
    let ir = query_resolver::resolve_delete(&catalog, &ast).expect("delete should resolve");
    let plan = sqlite_query_plan::plan_delete(&ir);
    let statement = render_delete(&plan);

    runner
        .execute_delete(&statement)
        .expect("delete statement should execute")
}

fn affected_rows(result: &SQLiteQueryResult) -> i64 {
    let [row] = result.rows() else {
        panic!("mutation result should contain one row");
    };
    let [SQLiteCellValue::Integer(rows)] = row.as_slice() else {
        panic!("mutation result should contain one affected_rows value");
    };

    *rows
}

#[test]
fn multi_link_mutation_pipeline_adds_and_removes_target_sets_idempotently() {
    let mut runner = setup_blog_database();
    let add = r#"update User
        filter .email = "carol@example.com"
        set {
            posts += (
                select Post { id }
                filter .view_count >= 20
            )
        }"#;
    let remove = r#"update User
        filter .email = "alice@example.com"
        set {
            posts -= (
                select Post { id }
                filter .view_count >= 20
            )
        }"#;

    assert_eq!(affected_rows(&execute_command_query(&mut runner, add)), 2);
    assert_eq!(affected_rows(&execute_command_query(&mut runner, add)), 0);
    assert_eq!(
        affected_rows(&execute_command_query(&mut runner, remove)),
        1
    );
    assert_eq!(
        affected_rows(&execute_command_query(&mut runner, remove)),
        0
    );
}

#[test]
fn multi_link_mutation_pipeline_treats_missing_sources_and_targets_as_noops() {
    let mut runner = setup_blog_database();

    for source in [
        r#"update User filter .email = "missing@example.com" set {
            posts += (select Post { id })
        }"#,
        r#"update User filter .email = "carol@example.com" set {
            posts -= (select Post { id } filter .title = "Missing")
        }"#,
    ] {
        assert_eq!(
            affected_rows(&execute_command_query(&mut runner, source)),
            0
        );
    }
}

#[test]
fn multi_link_mutation_pipeline_batches_multiple_sources_and_targets() {
    let mut runner = setup_blog_database();
    let result = execute_command_query(
        &mut runner,
        r#"update User filter .score >= 0 set {
            posts += (select Post { id } filter .view_count >= 20)
        }"#,
    );

    assert_eq!(affected_rows(&result), 4);
}

#[test]
fn multi_link_mutation_pipeline_rolls_back_with_failing_transaction_script() {
    let mut runner = setup_blog_database();
    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");
    let script = gelite_commands::compile_script(
        &catalog,
        r#"start transaction;
        update User filter .email = "carol@example.com" set {
            posts += (select Post { id } filter .view_count >= 20)
        };
        insert User { email := "carol@example.com", score := 1 };
        commit;"#,
    )
    .expect("transaction script should compile");

    gelite_commands::execute_script(&mut runner, script)
        .expect_err("duplicate email should fail and roll back");

    let result = execute_command_query(
        &mut runner,
        r#"update User filter .email = "carol@example.com" set {
            posts += (select Post { id } filter .view_count >= 20)
        }"#,
    );
    assert_eq!(affected_rows(&result), 2);
}

#[test]
fn select_pipeline_renders_in_filter_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .title in ["Draft", "Published"] order by .title asc limit 20"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" IN (?, ?) ORDER BY \"root\".\"title\" ASC LIMIT 20"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Draft".to_string()),
            SQLiteBindValue::String("Published".to_string()),
        ]
    );
}

#[test]
fn select_pipeline_renders_not_in_filter_through_single_link_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .author.email not in ["blocked@example.com"]"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"email\" NOT IN (?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("blocked@example.com".to_string())]
    );
}

#[test]
fn select_pipeline_renders_comparison_filter_from_query_text() {
    let statement = render_query(r#"select Post { title } filter .view_count >= 10"#);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"view_count\" >= ?"
    );
    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(10)]);
}

#[test]
fn select_pipeline_renders_arithmetic_order_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .title != "Archived" order by .view_count + 1 desc"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" != ? ORDER BY (\"root\".\"view_count\" + ?) DESC"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Archived".to_string()),
            SQLiteBindValue::Int64(1),
        ]
    );
}

#[test]
fn select_pipeline_renders_numeric_cast_filter_from_query_text() {
    let statement = render_query(r#"select Post { title } filter f64(.view_count) / 2.0 >= 10.5"#);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (CAST(\"root\".\"view_count\" AS REAL) / ?) >= ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::Float64(2.0),
            SQLiteBindValue::Float64(10.5),
        ]
    );
}

#[test]
fn select_pipeline_renders_string_function_filter_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter concat(.title, "!", str(.view_count)) = "Draft!5""#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"title\" || ? || CAST(\"root\".\"view_count\" AS TEXT)) = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("!".to_string()),
            SQLiteBindValue::String("Draft!5".to_string()),
        ]
    );
}

#[test]
fn select_pipeline_renders_computed_projection_from_query_text() {
    let statement = render_query(r#"select Post { score := .view_count + 1 }"#);

    assert_eq!(
        statement.sql(),
        "SELECT (\"root\".\"view_count\" + ?) AS \"__gelite_value_0\" FROM \"post\" AS \"root\""
    );
    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(1)]);
}

#[test]
fn select_pipeline_renders_path_only_computed_projection_from_query_text() {
    let statement = render_query(r#"select Post { title_copy := .title }"#);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" AS \"__gelite_value_0\" FROM \"post\" AS \"root\""
    );
    assert!(statement.bind_values().is_empty());
}

#[test]
fn select_pipeline_executes_root_scalar_comparison_filter() {
    let result =
        execute_query(r#"select Post { title } filter .view_count >= 10 order by .title asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_computed_projection() {
    let result =
        execute_query(r#"select Post { score := .view_count + 1 } order by .view_count asc"#);

    assert_eq!(result.columns(), &["score".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Integer(6)],
            vec![SQLiteCellValue::Integer(21)],
            vec![SQLiteCellValue::Integer(101)],
        ]
    );
}

#[test]
fn select_pipeline_executes_path_only_computed_projection() {
    let result = execute_query(r#"select Post { title_copy := .title } order by .view_count asc"#);

    assert_eq!(result.columns(), &["title_copy".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
            vec![SQLiteCellValue::Text("Archived".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_nested_selected_single_link_shape() {
    let result = execute_query(
        r#"select Post {
  title,
  author: {
    email,
    best_friend: {
      email
    }
  }
}
filter .title = "Draft""#,
    );

    assert_eq!(
        result.columns(),
        &["title".to_string(), "author".to_string()]
    );
    assert_eq!(
        result.rows(),
        &[vec![
            SQLiteCellValue::Text("Draft".to_string()),
            SQLiteCellValue::Object(vec![
                (
                    "email".to_string(),
                    SQLiteCellValue::Text("alice@example.com".to_string()),
                ),
                (
                    "best_friend".to_string(),
                    SQLiteCellValue::Object(vec![(
                        "email".to_string(),
                        SQLiteCellValue::Text("blocked@example.com".to_string()),
                    )]),
                ),
            ]),
        ]]
    );
}

#[test]
fn select_pipeline_preserves_explicit_nested_id() {
    let result = execute_query(
        r#"select Post {
  author: {
    id,
    email
  }
}
filter .title = "Draft""#,
    );

    assert_eq!(result.columns(), &["author".to_string()]);
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Object(vec![
            (
                "id".to_string(),
                SQLiteCellValue::Text("user-1".to_string()),
            ),
            (
                "email".to_string(),
                SQLiteCellValue::Text("alice@example.com".to_string()),
            ),
        ])]]
    );
}

#[test]
fn select_pipeline_executes_repeated_nested_selected_single_link_names() {
    let result = execute_query(
        r#"select User {
  email,
  best_friend: {
    email,
    best_friend: {
      email
    }
  }
}
filter .email = "alice@example.com""#,
    );

    assert_eq!(
        result.columns(),
        &["email".to_string(), "best_friend".to_string()]
    );
    assert_eq!(
        result.rows(),
        &[vec![
            SQLiteCellValue::Text("alice@example.com".to_string()),
            SQLiteCellValue::Object(vec![
                (
                    "email".to_string(),
                    SQLiteCellValue::Text("blocked@example.com".to_string()),
                ),
                (
                    "best_friend".to_string(),
                    SQLiteCellValue::Object(vec![(
                        "email".to_string(),
                        SQLiteCellValue::Text("carol@example.com".to_string()),
                    )]),
                ),
            ]),
        ]]
    );
}

#[test]
fn select_pipeline_shapes_missing_optional_single_link_as_null() {
    let result = execute_query(
        r#"select User {
  email,
  best_friend: { email }
}
filter .email = "carol@example.com""#,
    );

    assert_eq!(
        result.columns(),
        &["email".to_string(), "best_friend".to_string()]
    );
    assert_eq!(
        result.rows(),
        &[vec![
            SQLiteCellValue::Text("carol@example.com".to_string()),
            SQLiteCellValue::Null,
        ]]
    );
}

#[test]
fn select_pipeline_preserves_nested_computed_output_name_and_order() {
    let result = execute_query(
        r#"select Post {
  author: {
    email,
    rank := .score + 1
  }
}
filter .title = "Draft""#,
    );

    assert_eq!(result.columns(), &["author".to_string()]);
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Object(vec![
            (
                "email".to_string(),
                SQLiteCellValue::Text("alice@example.com".to_string()),
            ),
            ("rank".to_string(), SQLiteCellValue::Integer(101)),
        ])]]
    );
}

#[test]
fn select_pipeline_executes_unary_arithmetic_computed_projection() {
    let result =
        execute_query(r#"select Post { neg_views := -.view_count } order by +.view_count asc"#);

    assert_eq!(result.columns(), &["neg_views".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Integer(-5)],
            vec![SQLiteCellValue::Integer(-20)],
            vec![SQLiteCellValue::Integer(-100)],
        ]
    );
}

#[test]
fn select_pipeline_executes_root_scalar_arithmetic_filter() {
    let result =
        execute_query(r#"select Post { title } filter .view_count + 6 > 25 order by .title asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_root_scalar_numeric_cast_filter() {
    let result = execute_query(
        r#"select Post { title } filter f64(.view_count) / 2.0 >= 10.0 order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_string_function_filter() {
    let result = execute_query(
        r#"select Post { title } filter concat(.title, "!", str(.view_count)) = "Draft!5""#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Text("Draft".to_string())]]
    );
}

#[test]
fn select_pipeline_executes_single_link_unary_arithmetic_filter() {
    let result =
        execute_query(r#"select Post { title } filter -.author.score < 0 order by .title asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_root_scalar_arithmetic_order() {
    let result = execute_query(r#"select Post { title } order by .view_count + 1 desc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
            vec![SQLiteCellValue::Text("Draft".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_single_link_arithmetic_order() {
    let result = execute_query(r#"select Post { title } order by .author.score + .view_count asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_membership_filter_with_unary_arithmetic_items() {
    let result = execute_query(
        r#"select Post { title } filter .view_count in [-5 + 10, +20] order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_membership_filter_with_arithmetic_items() {
    let result = execute_query(
        r#"select Post { title } filter .view_count in [5 + 0, 10 + 10] order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_single_link_membership_filter() {
    let result = execute_query(
        r#"select Post { title } filter .author.email not in ["blocked@example.com"] order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_membership_select_for_zero_one_and_multiple_rows() {
    let zero = execute_query(
        r#"select Post { title }
        filter .author.id in (select User { id } filter .score > 1000)
        order by .title asc"#,
    );
    assert!(zero.rows().is_empty());

    let one = execute_query(
        r#"select Post { title }
        filter .author.id in (select User { id } filter .email = "blocked@example.com")
        order by .title asc"#,
    );
    assert_eq!(
        one.rows(),
        &[vec![SQLiteCellValue::Text("Archived".to_string())]]
    );

    let multiple = execute_query(
        r#"select Post { title }
        filter .author.id in (select User { id } filter .score >= 0)
        order by .title asc"#,
    );
    assert_eq!(
        multiple.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_shapes_multi_links_for_zero_one_many_and_multiple_parents() {
    let mut runner = setup_blog_database();
    let result = execute_command_query(
        &mut runner,
        "select User { email, posts: { title } } order by .email asc",
    );

    assert_eq!(
        result.columns(),
        &["email".to_string(), "posts".to_string()]
    );
    let lengths = result
        .rows()
        .iter()
        .map(|row| match (&row[0], &row[1]) {
            (SQLiteCellValue::Text(email), SQLiteCellValue::List(posts)) => {
                assert!(posts.iter().all(|post| match post {
                    SQLiteCellValue::Object(fields) => {
                        fields.len() == 1 && fields[0].0 == "title"
                    }
                    _ => false,
                }));
                (email.as_str(), posts.len())
            }
            values => panic!("expected email and posts collection, got {values:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        lengths,
        [
            ("alice@example.com", 2),
            ("blocked@example.com", 1),
            ("carol@example.com", 0),
        ]
    );
    assert!(result.parent_identities().iter().all(Option::is_none));
    assert!(
        result
            .follow_up_parent_identities()
            .iter()
            .all(Vec::is_empty)
    );
}

#[test]
fn select_pipeline_recursively_shapes_multi_link_target_fields() {
    let mut runner = setup_blog_database();
    let result = execute_command_query(
        &mut runner,
        r#"select User {
  posts: {
    title,
    score := .view_count + 1,
    author: {
      email,
      posts: { title }
    }
  }
}
filter .email = "alice@example.com""#,
    );

    let SQLiteCellValue::List(posts) = &result.rows()[0][0] else {
        panic!("posts should be a collection");
    };
    assert_eq!(posts.len(), 2);
    for post in posts {
        let SQLiteCellValue::Object(fields) = post else {
            panic!("post should be an object");
        };
        assert_eq!(fields[0].0, "title");
        assert_eq!(fields[1].0, "score");
        assert!(matches!(fields[1].1, SQLiteCellValue::Integer(_)));
        let SQLiteCellValue::Object(author) = &fields[2].1 else {
            panic!("author should be an object");
        };
        assert_eq!(author[0].0, "email");
        assert_eq!(
            author[0].1,
            SQLiteCellValue::Text("alice@example.com".to_string())
        );
        let SQLiteCellValue::List(nested_posts) = &author[1].1 else {
            panic!("nested posts should be a collection");
        };
        assert_eq!(nested_posts.len(), 2);
    }
}

#[test]
fn update_pipeline_executes_root_filter_from_query_text() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_update(
        &mut runner,
        r#"update Post filter .id = "post-1" set { title := "Reviewed" }"#,
    );

    assert_eq!(affected_rows, 1);

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT title FROM post WHERE id = 'post-1'",
            vec![],
        ))
        .expect("updated post should be readable");
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Text("Reviewed".to_string())]]
    );
}

#[test]
fn update_pipeline_executes_related_filter_from_query_text() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_update(
        &mut runner,
        r#"update Post filter .author.email = "alice@example.com" set { title := "Reviewed" }"#,
    );

    assert_eq!(affected_rows, 2);

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT id, title FROM post ORDER BY id",
            vec![],
        ))
        .expect("updated posts should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![
                SQLiteCellValue::Text("post-1".to_string()),
                SQLiteCellValue::Text("Reviewed".to_string()),
            ],
            vec![
                SQLiteCellValue::Text("post-2".to_string()),
                SQLiteCellValue::Text("Reviewed".to_string()),
            ],
            vec![
                SQLiteCellValue::Text("post-3".to_string()),
                SQLiteCellValue::Text("Archived".to_string()),
            ],
        ]
    );
}

#[test]
fn update_pipeline_executes_membership_select_filter() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_update(
        &mut runner,
        r#"update Post
        filter .author.id in (
            select User { id }
            filter .email = "alice@example.com"
        )
        set { title := "Reviewed" }"#,
    );

    assert_eq!(affected_rows, 2);
    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT id, title FROM post ORDER BY id",
            vec![],
        ))
        .expect("updated posts should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![
                SQLiteCellValue::Text("post-1".to_string()),
                SQLiteCellValue::Text("Reviewed".to_string()),
            ],
            vec![
                SQLiteCellValue::Text("post-2".to_string()),
                SQLiteCellValue::Text("Reviewed".to_string()),
            ],
            vec![
                SQLiteCellValue::Text("post-3".to_string()),
                SQLiteCellValue::Text("Archived".to_string()),
            ],
        ]
    );
}

#[test]
fn insert_pipeline_executes_link_select_and_rejects_missing_required_link() {
    let mut runner = setup_blog_database();
    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");

    execute_insert(
        &mut runner,
        &catalog,
        r#"insert Post {
            title := "Linked",
            view_count := 1,
            author := (
                select User { id }
                filter .email = "carol@example.com"
            ),
        }"#,
        "post-4",
    );

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT author_id FROM post WHERE id = 'post-4'",
            vec![],
        ))
        .expect("inserted link should be readable");
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Text("user-3".to_string())]]
    );

    let ast = query_parser::parse_insert(
        r#"insert Post {
            title := "Missing",
            view_count := 1,
            author := (
                select User { id }
                filter .email = "missing@example.com"
            ),
        }"#,
    )
    .expect("required missing-link insert should parse");
    let ir = query_resolver::resolve_insert(&catalog, &ast)
        .expect("required missing-link insert should resolve");
    let plan = sqlite_query_plan::plan_insert(&ir);
    let statement = render_insert(&plan, "post-5");

    runner
        .execute_insert(&statement)
        .expect_err("required link should reject a zero-row select");
}

#[test]
fn update_pipeline_executes_link_select_and_clears_optional_link_on_no_rows() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_update(
        &mut runner,
        r#"update Post filter .id = "post-3" set {
            author := (
                select User { id }
                filter .email = "alice@example.com"
            ),
        }"#,
    );
    assert_eq!(affected_rows, 1);

    let affected_rows = execute_update(
        &mut runner,
        r#"update User filter .id = "user-1" set {
            best_friend := (
                select User { id }
                filter .email = "missing@example.com"
            ),
        }"#,
    );
    assert_eq!(affected_rows, 1);

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT author_id FROM post WHERE id = 'post-3' UNION ALL SELECT best_friend_id FROM user WHERE id = 'user-1'",
            vec![],
        ))
        .expect("updated links should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("user-1".to_string())],
            vec![SQLiteCellValue::Null],
        ]
    );
}

#[test]
fn delete_pipeline_executes_root_filter_from_query_text() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_delete(&mut runner, r#"delete Post filter .id = "post-1""#);

    assert_eq!(affected_rows, 1);
    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT id FROM post ORDER BY id",
            vec![],
        ))
        .expect("remaining posts should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("post-2".to_string())],
            vec![SQLiteCellValue::Text("post-3".to_string())],
        ]
    );
}

#[test]
fn delete_pipeline_executes_related_filter_from_query_text() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_delete(
        &mut runner,
        r#"delete Post filter .author.email = "alice@example.com""#,
    );

    assert_eq!(affected_rows, 2);
    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT id FROM post ORDER BY id",
            vec![],
        ))
        .expect("remaining posts should be readable");
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Text("post-3".to_string())]]
    );
}

#[test]
fn delete_pipeline_executes_not_in_membership_select_filter() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_delete(
        &mut runner,
        r#"delete Post
        filter .author.id not in (
            select User { id }
            filter .email = "alice@example.com"
        )"#,
    );

    assert_eq!(affected_rows, 1);
    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT id FROM post ORDER BY id",
            vec![],
        ))
        .expect("remaining posts should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("post-1".to_string())],
            vec![SQLiteCellValue::Text("post-2".to_string())],
        ]
    );
}

#[test]
fn delete_pipeline_cascades_multi_link_rows() {
    let mut runner = setup_blog_database();

    execute_delete(&mut runner, r#"delete Post filter .id = "post-1""#);

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT target_id FROM user__posts ORDER BY target_id",
            vec![],
        ))
        .expect("remaining join rows should be readable");
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("post-2".to_string())],
            vec![SQLiteCellValue::Text("post-3".to_string())],
        ]
    );
}
