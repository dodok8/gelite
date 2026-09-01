#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use rusqlite::{
    Connection, Error, params,
    types::{Type, Value},
};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn rusqlite_supports_the_required_browser_connection_surface() {
    let mut connection = Connection::open_in_memory().expect("in-memory database should open");

    connection
        .execute_batch(
            "CREATE TABLE entry (
                id INTEGER PRIMARY KEY,
                rating REAL NOT NULL,
                title TEXT NOT NULL,
                note TEXT NULL
            );",
        )
        .expect("DDL should execute");

    let affected = connection
        .prepare("INSERT INTO entry (id, rating, title, note) VALUES (?, ?, ?, ?)")
        .expect("insert should prepare")
        .execute(params![7_i64, 4.5_f64, "Case File", Option::<String>::None])
        .expect("bound insert should execute");
    assert_eq!(affected, 1);

    let mut statement = connection
        .prepare("SELECT id, rating, title, note FROM entry")
        .expect("select should prepare");
    assert_eq!(statement.column_names(), ["id", "rating", "title", "note"]);

    let mut rows = statement.query([]).expect("select should execute");
    let row = rows
        .next()
        .expect("row should step")
        .expect("inserted row should exist");
    assert_eq!(
        row.get_ref(0).expect("id should be readable").data_type(),
        Type::Integer
    );
    assert_eq!(
        row.get_ref(1)
            .expect("rating should be readable")
            .data_type(),
        Type::Real
    );
    assert_eq!(
        row.get_ref(2)
            .expect("title should be readable")
            .data_type(),
        Type::Text
    );
    assert_eq!(
        row.get_ref(3).expect("note should be readable").data_type(),
        Type::Null
    );
    assert_eq!(
        row.get::<_, Value>(0).expect("id should be owned"),
        Value::Integer(7)
    );
    assert_eq!(
        row.get::<_, Value>(1).expect("rating should be owned"),
        Value::Real(4.5)
    );
    assert_eq!(
        row.get::<_, Value>(2).expect("title should be owned"),
        Value::Text("Case File".into())
    );
    assert_eq!(
        row.get::<_, Value>(3).expect("note should be owned"),
        Value::Null
    );
    assert!(rows.next().expect("rows should finish").is_none());
    drop(rows);
    drop(statement);

    let transaction = connection.transaction().expect("transaction should begin");
    transaction
        .execute(
            "INSERT INTO entry (id, rating, title, note) VALUES (?, ?, ?, ?)",
            params![8_i64, 3.0_f64, "Committed", "kept"],
        )
        .expect("transaction insert should execute");
    transaction.commit().expect("transaction should commit");

    let transaction = connection.transaction().expect("transaction should begin");
    transaction
        .execute(
            "INSERT INTO entry (id, rating, title, note) VALUES (?, ?, ?, ?)",
            params![9_i64, 2.0_f64, "Rolled Back", "discarded"],
        )
        .expect("transaction insert should execute");
    transaction
        .rollback()
        .expect("transaction should roll back");

    let ids = connection
        .prepare("SELECT id FROM entry ORDER BY id")
        .expect("verification select should prepare")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("verification select should execute")
        .collect::<Result<Vec<_>, _>>()
        .expect("ids should be readable");
    assert_eq!(ids, [7, 8]);

    let error = connection
        .execute("THIS IS NOT SQL", [])
        .expect_err("invalid SQL should fail");
    assert!(matches!(error, Error::SqliteFailure(_, _)));

    assert!(connection.close().is_ok());
}
