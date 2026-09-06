use sqlite_schema_plan::SQLiteValuePlan;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::{SQLiteRunner, wasm::WasmSQLiteRunner};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn wasm_runner_opens_and_closes_in_memory_database() {
    let runner = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner.close().expect("in-memory database should close");
}

#[wasm_bindgen_test]
fn wasm_runner_executes_raw_and_prepared_sql() {
    let mut runner = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");

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
        .expect("prepared insert should execute");

    assert_eq!(runner.table_exists("metadata"), Ok(true));
    assert_eq!(
        runner
            .first_three_column_row(
                "SELECT changes(), name, target_object_id
                 FROM metadata
                 WHERE object_id = 1",
            )
            .expect("inserted row should be readable"),
        Some((1, "Post".to_string(), None)),
    );
}

#[wasm_bindgen_test]
fn wasm_runner_reports_sql_errors() {
    let mut runner = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");

    let raw_error = runner
        .execute("THIS IS NOT SQL")
        .expect_err("invalid raw SQL should fail");
    assert!(raw_error.message().contains("execute SQL"));

    let prepared_error = runner
        .execute_with_values("INSERT INTO missing VALUES (?)", &[SQLiteValuePlan::Null])
        .expect_err("invalid prepared SQL should fail");
    assert!(prepared_error.message().contains("prepare SQL"));
}
