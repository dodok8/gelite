extern crate alloc;
extern crate std;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_query_sqlgen::{SQLiteResultField, SQLiteResultShape, SQLiteStatement};
use sqlite_schema_plan::SQLiteValuePlan;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    SQLiteRunner, apply_schema_statements, native::NativeSQLiteRunner,
    tests::fixtures::rendered_post_schema_statements,
};

#[test]
fn native_runner_can_open_in_memory_database() {
    let runner = NativeSQLiteRunner::open_in_memory();

    assert!(runner.is_ok());
}

#[test]
fn native_runner_commits_explicit_transaction() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    runner
        .execute("CREATE TABLE entry (id TEXT PRIMARY KEY)")
        .expect("table should be created");

    runner
        .begin_transaction()
        .expect("transaction should begin");
    runner
        .execute("INSERT INTO entry VALUES ('entry-1')")
        .expect("insert should execute");
    runner
        .commit_transaction()
        .expect("transaction should commit");

    assert_eq!(entry_ids(&mut runner), vec!["entry-1".to_string()]);
}

#[test]
fn native_runner_rolls_back_explicit_transaction() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    runner
        .execute("CREATE TABLE entry (id TEXT PRIMARY KEY)")
        .expect("table should be created");

    runner
        .begin_transaction()
        .expect("transaction should begin");
    runner
        .execute("INSERT INTO entry VALUES ('entry-1')")
        .expect("insert should execute");
    runner
        .rollback_transaction()
        .expect("transaction should roll back");

    assert!(entry_ids(&mut runner).is_empty());
}

#[test]
fn native_runner_rejects_invalid_transaction_transitions() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    let commit_error = runner
        .commit_transaction()
        .expect_err("commit without transaction should fail");
    assert!(commit_error.message().contains("no transaction is active"));

    let rollback_error = runner
        .rollback_transaction()
        .expect_err("rollback without transaction should fail");
    assert!(
        rollback_error
            .message()
            .contains("no transaction is active")
    );

    runner
        .begin_transaction()
        .expect("transaction should begin");
    let nested_error = runner
        .begin_transaction()
        .expect_err("nested transaction should fail");
    assert!(
        nested_error
            .message()
            .contains("cannot start a transaction within a transaction")
    );
}

#[test]
fn native_runner_rolls_back_transaction_when_connection_closes() {
    let path = temporary_database_path("uncommitted-transaction");
    let path_str = path.to_str().expect("temporary path should be UTF-8");
    let mut runner = NativeSQLiteRunner::open(path_str).expect("database should open");
    runner
        .execute("CREATE TABLE entry (id TEXT PRIMARY KEY)")
        .expect("table should be created");
    runner
        .begin_transaction()
        .expect("transaction should begin");
    runner
        .execute("INSERT INTO entry VALUES ('entry-1')")
        .expect("insert should execute");
    drop(runner);

    let mut reopened = NativeSQLiteRunner::open(path_str).expect("database should reopen");
    assert!(entry_ids(&mut reopened).is_empty());
    drop(reopened);
    std::fs::remove_file(path).expect("temporary database should be removed");
}

fn entry_ids(runner: &mut NativeSQLiteRunner) -> Vec<String> {
    runner
        .execute_select(&sqlite_query_sqlgen::SQLiteStatement::new(
            "SELECT id FROM entry ORDER BY id",
            vec![],
        ))
        .expect("entries should be readable")
        .rows()
        .iter()
        .map(|row| match &row[0] {
            crate::SQLiteCellValue::Text(value) => value.clone(),
            value => panic!("expected text id, got {value:?}"),
        })
        .collect()
}

fn temporary_database_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should follow Unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("gelite-{name}-{}-{nonce}.db", std::process::id()))
}

#[test]
fn native_runner_can_execute_create_table_statement() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY)")
        .expect("create table should execute");

    assert_eq!(runner.table_exists("post"), Ok(true));
    assert_eq!(runner.table_exists("missing"), Ok(false));
}

#[test]
fn native_runner_can_execute_insert_statement_with_bind_values() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute(
            "CREATE TABLE metadata (
                object_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                target_object_id INTEGER NULL
            )",
        )
        .expect("create table should execute");

    runner
        .execute_with_values(
            "INSERT INTO metadata (object_id, name, target_object_id) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Integer(1),
                SQLiteValuePlan::Text("Post".to_string()),
                SQLiteValuePlan::Null,
            ],
        )
        .expect("insert should execute");

    let row = runner
        .first_three_column_row(
            "SELECT object_id, name, target_object_id FROM metadata WHERE object_id = 1",
        )
        .expect("row should be readable");

    assert_eq!(row, Some((1, "Post".to_string(), None)));
}

#[test]
fn native_runner_can_execute_query_insert_statement_with_bind_values() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute(
            "CREATE TABLE entry (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                view_count INTEGER NOT NULL,
                rating REAL NOT NULL,
                published INTEGER NOT NULL,
                subtitle TEXT NULL
            )",
        )
        .expect("create table should execute");

    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "INSERT INTO entry (id, title, view_count, rating, published, subtitle) VALUES (?, ?, ?, ?, ?, ?)",
        vec![
            sqlite_query_sqlgen::SQLiteBindValue::String("entry-1".to_string()),
            sqlite_query_sqlgen::SQLiteBindValue::String("Case File".to_string()),
            sqlite_query_sqlgen::SQLiteBindValue::Int64(7),
            sqlite_query_sqlgen::SQLiteBindValue::Float64(4.5),
            sqlite_query_sqlgen::SQLiteBindValue::Bool(true),
            sqlite_query_sqlgen::SQLiteBindValue::Null,
        ],
    );

    runner
        .execute_insert(&statement)
        .expect("query insert should execute");

    let select = sqlite_query_sqlgen::SQLiteStatement::new(
        "SELECT id, title, view_count, rating, published, subtitle FROM entry",
        vec![],
    );
    let result = runner
        .execute_select(&select)
        .expect("inserted row should be readable");

    assert_eq!(
        result.rows(),
        &[vec![
            crate::SQLiteCellValue::Text("entry-1".to_string()),
            crate::SQLiteCellValue::Text("Case File".to_string()),
            crate::SQLiteCellValue::Integer(7),
            crate::SQLiteCellValue::Real(4.5),
            crate::SQLiteCellValue::Integer(1),
            crate::SQLiteCellValue::Null,
        ]]
    );
}

#[test]
fn native_runner_enforces_foreign_keys_for_query_inserts() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE author (id TEXT PRIMARY KEY)")
        .expect("author table should be created");
    runner
        .execute(
            "CREATE TABLE post (
                id TEXT PRIMARY KEY,
                author_id TEXT NOT NULL,
                FOREIGN KEY (author_id) REFERENCES author(id)
            )",
        )
        .expect("post table should be created");

    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "INSERT INTO post (id, author_id) VALUES (?, ?)",
        vec![
            sqlite_query_sqlgen::SQLiteBindValue::String("post-1".to_string()),
            sqlite_query_sqlgen::SQLiteBindValue::String("missing-author".to_string()),
        ],
    );

    runner
        .execute_insert(&statement)
        .expect_err("missing foreign-key target should reject insert");
}

#[test]
fn native_runner_executes_update_and_returns_affected_rows() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .expect("post table should be created");
    runner
        .execute("INSERT INTO post VALUES ('post-1', 'Draft'), ('post-2', 'Draft')")
        .expect("posts should be inserted");

    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "UPDATE post SET title = ? WHERE id = ?",
        vec![
            sqlite_query_sqlgen::SQLiteBindValue::String("Closed".to_string()),
            sqlite_query_sqlgen::SQLiteBindValue::String("post-1".to_string()),
        ],
    );

    let affected_rows = runner
        .execute_update(&statement)
        .expect("update should execute");

    assert_eq!(affected_rows, 1);

    let select =
        sqlite_query_sqlgen::SQLiteStatement::new("SELECT title FROM post ORDER BY id", vec![]);
    let result = runner
        .execute_select(&select)
        .expect("updated rows should be readable");

    assert_eq!(
        result.rows(),
        &[
            vec![crate::SQLiteCellValue::Text("Closed".to_string())],
            vec![crate::SQLiteCellValue::Text("Draft".to_string())],
        ]
    );
}

#[test]
fn native_runner_executes_delete_and_returns_affected_rows() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .expect("post table should be created");
    runner
        .execute("INSERT INTO post VALUES ('post-1', 'Draft'), ('post-2', 'Published')")
        .expect("posts should be inserted");
    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "DELETE FROM post WHERE title = ?",
        vec![sqlite_query_sqlgen::SQLiteBindValue::String(
            "Draft".to_string(),
        )],
    );

    let affected_rows = runner
        .execute_delete(&statement)
        .expect("delete should execute");

    assert_eq!(affected_rows, 1);
}

#[test]
fn native_runner_preserves_delete_restrict_errors() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE user (id TEXT PRIMARY KEY)")
        .expect("user table should be created");
    runner
        .execute(
            "CREATE TABLE post (
                id TEXT PRIMARY KEY,
                author_id TEXT NOT NULL,
                FOREIGN KEY (author_id) REFERENCES user(id) ON DELETE RESTRICT
            )",
        )
        .expect("post table should be created");
    runner
        .execute("INSERT INTO user VALUES ('user-1')")
        .expect("user should be inserted");
    runner
        .execute("INSERT INTO post VALUES ('post-1', 'user-1')")
        .expect("post should be inserted");
    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "DELETE FROM user WHERE id = ?",
        vec![sqlite_query_sqlgen::SQLiteBindValue::String(
            "user-1".to_string(),
        )],
    );

    runner
        .execute_delete(&statement)
        .expect_err("referenced user delete should fail");
}

#[test]
fn native_runner_delete_cascades_join_table_rows() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE user (id TEXT PRIMARY KEY)")
        .expect("user table should be created");
    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY)")
        .expect("post table should be created");
    runner
        .execute(
            "CREATE TABLE user__posts (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                FOREIGN KEY (source_id) REFERENCES user(id) ON DELETE CASCADE,
                FOREIGN KEY (target_id) REFERENCES post(id) ON DELETE CASCADE
            )",
        )
        .expect("join table should be created");
    runner
        .execute("INSERT INTO user VALUES ('user-1')")
        .expect("user should be inserted");
    runner
        .execute("INSERT INTO post VALUES ('post-1')")
        .expect("post should be inserted");
    runner
        .execute("INSERT INTO user__posts VALUES ('user-1', 'post-1')")
        .expect("join row should be inserted");
    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "DELETE FROM post WHERE id = ?",
        vec![sqlite_query_sqlgen::SQLiteBindValue::String(
            "post-1".to_string(),
        )],
    );

    runner
        .execute_delete(&statement)
        .expect("post delete should execute");
    let result = runner
        .execute_select(&sqlite_query_sqlgen::SQLiteStatement::new(
            "SELECT source_id FROM user__posts",
            vec![],
        ))
        .expect("join table should remain readable");

    assert!(result.rows().is_empty());
}

#[test]
fn native_runner_can_apply_rendered_initial_schema() {
    let statements = rendered_post_schema_statements();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    assert_eq!(runner.table_exists("_engine_schema_versions"), Ok(true));
    assert_eq!(runner.table_exists("_engine_catalog_objects"), Ok(true));
    assert_eq!(runner.table_exists("_engine_catalog_fields"), Ok(true));
    assert_eq!(runner.table_exists("post"), Ok(true));

    let row = runner
        .first_three_column_row(
            "SELECT object_id, name, NULL FROM _engine_catalog_objects WHERE name = 'Post'",
        )
        .expect("catalog object row should be readable");

    assert_eq!(row, Some((1, "Post".to_string(), None)));
}

#[test]
fn native_runner_can_load_schema_catalog_from_metadata() {
    let statements = rendered_post_schema_statements();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");

    assert!(catalog.find_type("Post").is_some());
    assert!(catalog.find_field("Post", "title").is_some());
    assert!(catalog.find_field("Post", "id").is_some());
}

#[test]
fn native_runner_can_execute_select_statement_with_bind_values() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .expect("create table should execute");
    runner
        .execute_with_values(
            "INSERT INTO post (id, title) VALUES (?, ?)",
            &[
                SQLiteValuePlan::Text("post-1".to_string()),
                SQLiteValuePlan::Text("Hello".to_string()),
            ],
        )
        .expect("insert should execute");

    let statement = sqlite_query_sqlgen::SQLiteStatement::new(
        "SELECT title FROM post WHERE title = ?",
        vec![sqlite_query_sqlgen::SQLiteBindValue::String(
            "Hello".to_string(),
        )],
    );
    let result = runner
        .execute_select(&statement)
        .expect("select should execute");

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[vec![crate::SQLiteCellValue::Text("Hello".to_string())]]
    );
}

#[test]
fn native_runner_shapes_present_and_missing_single_links() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let statement = SQLiteStatement::with_result_shape(
        "SELECT 'Draft', 'user-1', 'alice@example.com' UNION ALL SELECT 'Orphaned', NULL, NULL",
        vec![],
        SQLiteResultShape::new(
            None,
            vec![
                SQLiteResultField::value("title", 0),
                SQLiteResultField::nested(
                    "author",
                    SQLiteResultShape::new(Some(1), vec![SQLiteResultField::value("email", 2)]),
                ),
            ],
        ),
    );

    let result = runner
        .execute_select(&statement)
        .expect("select should shape single links");

    assert_eq!(
        result.columns(),
        &["title".to_string(), "author".to_string()]
    );
    assert_eq!(
        result.rows(),
        &[
            vec![
                crate::SQLiteCellValue::Text("Draft".to_string()),
                crate::SQLiteCellValue::Object(vec![(
                    "email".to_string(),
                    crate::SQLiteCellValue::Text("alice@example.com".to_string()),
                )]),
            ],
            vec![
                crate::SQLiteCellValue::Text("Orphaned".to_string()),
                crate::SQLiteCellValue::Null,
            ],
        ]
    );
}

#[test]
fn native_runner_reports_execution_errors() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    let error = runner
        .execute("CREATE TABLE")
        .expect_err("invalid SQL should fail");

    assert!(error.message().contains("execute SQL"));
    assert!(!error.message().is_empty());
}
