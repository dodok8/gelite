extern crate alloc;

use alloc::string::ToString;
use alloc::vec;
use sqlite_schema_plan::SQLiteValuePlan;

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
fn native_runner_reports_execution_errors() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    let error = runner
        .execute("CREATE TABLE")
        .expect_err("invalid SQL should fail");

    assert!(error.message().contains("execute SQL"));
    assert!(!error.message().is_empty());
}
