extern crate alloc;
extern crate std;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_query_sqlgen::{SQLiteResultField, SQLiteResultShape, SQLiteStatement};
use sqlite_schema_plan::SQLiteValuePlan;
use sqlite_schema_sqlgen::RenderedSchemaStatement;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    SQLiteRunner, SQLiteRunnerError, apply_schema_statements,
    native::NativeSQLiteRunner,
    tests::fixtures::{
        APPLIED_AT, VERSION_ID, native_runner_with_post_schema, rendered_post_schema_statements,
    },
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

    let version = runner
        .first_three_column_row(
            "SELECT COUNT(*), MIN(version_id), NULL FROM _engine_schema_versions",
        )
        .expect("applied version should be readable");
    assert_eq!(version, Some((1, VERSION_ID.to_string(), None)));

    let row = runner
        .first_three_column_row(
            "SELECT object_id, name, NULL FROM _engine_catalog_objects WHERE name = 'Post'",
        )
        .expect("catalog object row should be readable");

    assert_eq!(row, Some((1, "Post".to_string(), None)));
}

#[test]
fn native_schema_apply_rolls_back_after_ddl_failure() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let mut statements = rendered_post_schema_statements();

    let duplicate_ddl = statements[0].sql().to_string();
    statements.push(RenderedSchemaStatement::Sql(duplicate_ddl));

    let error = apply_schema_statements(&mut runner, &statements)
        .expect_err("duplicate table creation should fail");

    assert!(error.message().contains("already exists"));

    [
        "_engine_schema_versions",
        "_engine_catalog_objects",
        "_engine_catalog_fields",
        "post",
    ]
    .iter()
    .for_each(|table| {
        assert_eq!(runner.table_exists(table), Ok(false), "{table} remains");
    });
}

#[test]
fn native_schema_apply_rolls_back_after_commit_failure() {
    let path = temporary_database_path("schema-commit-failure");
    let path_str = path.to_str().expect("temporary path should be UTF-8");
    let mut writer = NativeSQLiteRunner::open(path_str).expect("database should open");
    writer
        .execute("PRAGMA journal_mode = DELETE")
        .expect("rollback journal should be enabled");
    let mut reader = NativeSQLiteRunner::open(path_str).expect("reader should open");
    reader
        .begin_transaction()
        .expect("read transaction should begin");
    // Keep a read lock after the query finishes so the writer cannot commit.
    assert_eq!(reader.table_exists("post"), Ok(false));

    let statements = rendered_post_schema_statements();
    let result = apply_schema_statements(&mut writer, &statements);
    reader
        .rollback_transaction()
        .expect("read lock should be released");
    let remaining_tables = [
        "_engine_schema_versions",
        "_engine_catalog_objects",
        "_engine_catalog_fields",
        "post",
    ]
    .map(|table| (table, writer.table_exists(table)));
    let retry = apply_schema_statements(&mut writer, &statements);

    // Close both connections and remove the file before regression assertions.
    drop(reader);
    drop(writer);
    std::fs::remove_file(path).expect("temporary database should be removed");

    let error = result.expect_err("read lock should make schema commit fail");
    assert!(error.message().contains("database is locked"), "{error:?}");
    remaining_tables.iter().for_each(|(table, exists)| {
        assert_eq!(*exists, Ok(false), "{table} remains after commit failure");
    });
    retry.expect("schema apply should succeed on the same connection after releasing the reader");
}

#[test]
fn native_schema_apply_preserves_original_error_after_rollback_failure() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let mut statements = rendered_post_schema_statements();
    // Raw SQL forces automatic rollback; schema plans do not expose conflict policies.
    statements.push(RenderedSchemaStatement::Sql(
        "INSERT OR ROLLBACK INTO _engine_catalog_objects (object_id, name) VALUES (1, 'Post')"
            .to_string(),
    ));

    let error = apply_schema_statements(&mut runner, &statements)
        .expect_err("duplicate catalog object should fail and roll back automatically");

    assert_eq!(runner.table_exists("_engine_catalog_objects"), Ok(false));
    assert!(
        error.message().contains("UNIQUE constraint failed"),
        "original constraint error should be preserved: {error:?}"
    );
    assert!(
        error.message().contains("no transaction is active"),
        "rollback failure context should be included: {error:?}"
    );
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
fn native_schema_verification_accepts_stored_version_without_changing_it() {
    let mut runner = native_runner_with_post_schema();
    let query = SQLiteStatement::new("SELECT * FROM _engine_schema_versions", vec![]);
    let version_before = runner.execute_select(&query).expect("version should load");
    let catalog_before = runner.load_schema_catalog().expect("catalog should load");

    runner
        .verify_schema_version()
        .expect("stored checksum and logical catalog should match without a source file");
    runner
        .verify_schema_version()
        .expect("verification should be repeatable");

    assert_eq!(runner.execute_select(&query), Ok(version_before));
    assert_eq!(runner.load_schema_catalog(), Ok(catalog_before));
    runner
        .begin_transaction()
        .expect("successful verification should not leave a transaction open");
    runner
        .rollback_transaction()
        .expect("transaction should end");
}

#[test]
fn native_schema_verification_accepts_empty_catalog() {
    let catalog = schema_model::SchemaCatalog::try_new(vec![]).expect("empty catalog is valid");
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("empty snapshot should serialize");
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");
    apply_schema_statements(
        &mut runner,
        &sqlite_schema_sqlgen::render_initial_schema(&plan),
    )
    .expect("empty schema should apply");

    runner
        .verify_schema_version()
        .expect("an empty catalog still has a valid initial version");
}

#[test]
fn native_schema_verification_rejects_unsupported_snapshot_format() {
    let catalog = schema_model::SchemaCatalog::try_new(vec![]).expect("empty catalog is valid");
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("empty snapshot should serialize");
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");
    apply_schema_statements(
        &mut runner,
        &sqlite_schema_sqlgen::render_initial_schema(&plan),
    )
    .expect("empty schema should apply");
    // SHA-256 of the exact unsupported-format bytes; corruption must not fail only on checksum.
    runner
        .execute_with_values(
            "UPDATE _engine_schema_versions SET schema_snapshot = ?, checksum = ?",
            &[
                SQLiteValuePlan::Text(r#"{"format_version":2,"objects":[]}"#.to_string()),
                SQLiteValuePlan::Text(
                    "bf6ae26e250cb16a187eaf0e60d10d508cf06d3ddbc9223357181c8bc6a5b93b".to_string(),
                ),
            ],
        )
        .expect("unsupported snapshot and matching checksum should be stored");

    let error = runner
        .verify_schema_version()
        .expect_err("unsupported format versions must not be reinterpreted as format v1");
    assert!(matches!(
        error,
        SQLiteRunnerError::SchemaVerificationFailed { .. }
    ));
}

#[test]
fn native_schema_verification_rejects_checksum_tampering_and_ends_transaction() {
    let mut runner = native_runner_with_post_schema();
    // Raw metadata writes model corruption; normal schema application cannot produce it.
    runner
        .execute_with_values(
            "UPDATE _engine_schema_versions SET checksum = ?",
            &[SQLiteValuePlan::Text("0".repeat(64))],
        )
        .expect("checksum should be corrupted");
    let query = SQLiteStatement::new("SELECT * FROM _engine_schema_versions", vec![]);
    let corrupted = runner.execute_select(&query).expect("version should load");

    let error = runner
        .verify_schema_version()
        .expect_err("checksum corruption should be rejected");

    assert!(
        matches!(error, SQLiteRunnerError::SchemaVerificationFailed { .. }),
        "checksum corruption should be a verification failure: {error:?}"
    );
    assert!(
        !error.message().is_empty(),
        "verification error should explain the failure"
    );
    assert_eq!(runner.execute_select(&query), Ok(corrupted));
    runner
        .begin_transaction()
        .expect("failed verification should not leave a transaction open");
    runner
        .rollback_transaction()
        .expect("transaction should end");
}

#[test]
fn native_schema_verification_hashes_exact_stored_snapshot_bytes() {
    let mut runner = native_runner_with_post_schema();
    // Preserve the JSON meaning while changing the stored bytes without updating the hash.
    runner
        .execute("UPDATE _engine_schema_versions SET schema_snapshot = schema_snapshot || ' '")
        .expect("snapshot whitespace should be corrupted");

    let error = runner
        .verify_schema_version()
        .expect_err("verification must not normalize snapshot bytes before hashing");
    assert_eq!(error.message(), "stored schema snapshot checksum mismatch");
}

#[test]
fn native_schema_verification_rejects_valid_snapshot_for_another_catalog() {
    let mut runner = native_runner_with_post_schema();
    let other_catalog =
        schema_model::SchemaCatalog::try_new(vec![]).expect("empty catalog is valid");
    let other_plan =
        sqlite_schema_plan::plan_initial_schema(&other_catalog, VERSION_ID, APPLIED_AT)
            .expect("other snapshot should serialize");
    // Replace only the version with a correctly hashed snapshot; keep the Post catalog.
    runner
        .execute("DELETE FROM _engine_schema_versions")
        .expect("version should be removed");
    for insert in sqlite_schema_plan::plan_schema_version_insert(&other_plan) {
        let rendered = sqlite_schema_sqlgen::render_insert(&insert);
        runner
            .execute_with_values(rendered.sql(), rendered.values())
            .expect("other version should be stored");
    }

    let error = runner
        .verify_schema_version()
        .expect_err("a valid checksum alone does not prove that the logical catalog matches");
    assert!(matches!(
        error,
        SQLiteRunnerError::SchemaVerificationFailed { .. }
    ));
}

#[test]
fn native_schema_verification_rejects_logical_catalog_tampering() {
    let mut runner = native_runner_with_post_schema();
    // This remains a valid catalog, but differs from the recorded required field.
    runner
        .execute("UPDATE _engine_catalog_fields SET cardinality = 'optional' WHERE name = 'title'")
        .expect("catalog cardinality should change");
    runner
        .load_schema_catalog()
        .expect("changed catalog remains valid");

    runner
        .verify_schema_version()
        .expect_err("logical catalog changes should be detected even when the stored hash matches");
}

#[test]
fn native_schema_verification_rejects_invalid_catalog_metadata() {
    let mut runner = native_runner_with_post_schema();
    // Invalid metadata has no valid schema-planning representation.
    runner
        .execute("UPDATE _engine_catalog_fields SET scalar_type = 'unknown' WHERE name = 'title'")
        .expect("catalog scalar type should be corrupted");

    runner
        .verify_schema_version()
        .expect_err("catalog loading errors should become verification errors, not panics");
}

#[test]
fn native_schema_verification_rejects_incompatible_field_metadata() {
    // Metadata writes bypass the typed schema planner.
    for sql in [
        "UPDATE _engine_catalog_fields SET target_object_id = 1 WHERE name = 'title'",
        "UPDATE _engine_catalog_fields SET is_unique = 'true' WHERE name = 'title'",
    ] {
        let mut runner = native_runner_with_post_schema();
        runner
            .execute(sql)
            .expect("field metadata should be corrupted");
        runner
            .verify_schema_version()
            .expect_err("invalid field metadata must not be normalized away");
    }
}

#[test]
fn native_schema_verification_rejects_database_without_version_table() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");

    runner
        .verify_schema_version()
        .expect_err("an unapplied database has no stored version to verify");

    assert_eq!(runner.table_exists("_engine_schema_versions"), Ok(false));
    runner
        .begin_transaction()
        .expect("missing-table failure should not leave a transaction open");
    runner
        .rollback_transaction()
        .expect("transaction should end");
}

#[test]
fn native_schema_verification_rejects_missing_version_row() {
    let mut runner = native_runner_with_post_schema();
    // Normal initial application always inserts a version row.
    runner
        .execute("DELETE FROM _engine_schema_versions")
        .expect("version should be removed");

    runner
        .verify_schema_version()
        .expect_err("missing baseline must not be treated as successful verification");
}

#[test]
fn native_schema_verification_preserves_caller_transaction() {
    let mut runner = native_runner_with_post_schema();
    runner
        .begin_transaction()
        .expect("caller transaction should begin");
    runner
        .execute("INSERT INTO post (id, title) VALUES ('post-1', 'Draft')")
        .expect("caller write should execute");

    runner
        .verify_schema_version()
        .expect_err("verification requires its own read transaction");
    runner
        .commit_transaction()
        .expect("verification must not roll back or commit caller work");
    assert_eq!(
        runner.first_three_column_row("SELECT COUNT(*), MIN(title), NULL FROM post"),
        Ok(Some((1, "Draft".to_string(), None)))
    );
}

#[test]
fn native_schema_verification_checks_only_latest_version() {
    let mut runner = native_runner_with_post_schema();
    // Raw history fixtures stand in for future non-initial migrations.
    runner.execute(
        "INSERT INTO _engine_schema_versions (version_id, checksum, applied_at, schema_snapshot, version_number)
         SELECT '11111111-1111-4111-8111-111111111111', checksum, applied_at, schema_snapshot, 2
         FROM _engine_schema_versions",
    ).expect("latest version should be stored");
    runner.execute("UPDATE _engine_schema_versions SET checksum = 'old corruption' WHERE version_number = 1")
        .expect("older history should be corrupted");

    runner
        .verify_schema_version()
        .expect("valid latest version should match the catalog");
    runner.execute("UPDATE _engine_schema_versions SET checksum = 'latest corruption' WHERE version_number = 2")
        .expect("latest checksum should be corrupted");
    let error = runner
        .verify_schema_version()
        .expect_err("latest corruption should be detected");
    assert_eq!(error.message(), "stored schema snapshot checksum mismatch");
}

#[test]
fn native_schema_verification_rejects_missing_or_corrupt_implicit_identity() {
    // Metadata corruption must not disappear when ObjectType recreates the implicit id.
    for sql in [
        "DELETE FROM _engine_catalog_fields WHERE name = 'id'",
        "UPDATE _engine_catalog_fields SET scalar_type = 'str' WHERE name = 'id'",
        "UPDATE _engine_catalog_fields SET is_unique = 1 WHERE name = 'id'",
        "INSERT INTO _engine_catalog_fields SELECT object_id, 99, 'extra_id', field_kind, cardinality, scalar_type, target_object_id, is_implicit, is_unique, inverse_field_name FROM _engine_catalog_fields WHERE name = 'id'",
    ] {
        let mut runner = native_runner_with_post_schema();
        runner
            .execute(sql)
            .expect("implicit identity should be corrupted");
        let error = runner
            .verify_schema_version()
            .expect_err("corrupt implicit identity must be rejected");
        assert!(error.message().contains("implicit UUID id"), "{error:?}");
        runner
            .begin_transaction()
            .expect("verification failure should release its transaction");
        runner
            .rollback_transaction()
            .expect("test transaction should end");
    }
}

#[test]
fn native_schema_verification_rejects_orphaned_catalog_fields() {
    let mut runner = native_runner_with_post_schema();
    // External writers can bypass SQLite foreign-key enforcement.
    runner
        .execute("PRAGMA foreign_keys = OFF")
        .expect("disable foreign keys for corruption fixture");
    runner
        .execute("UPDATE _engine_catalog_fields SET object_id = 99 WHERE name = 'title'")
        .expect("orphan field should be stored");

    let error = runner
        .verify_schema_version()
        .expect_err("orphan fields must not be silently ignored");
    assert!(error.message().contains("unknown owner object"));
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
    let statement = SQLiteStatement::new(
        "SELECT 'Draft', 'user-1', 'alice@example.com' UNION ALL SELECT 'Orphaned', NULL, NULL",
        vec![],
    )
    .with_result_shape(SQLiteResultShape::new(
        None,
        vec![
            SQLiteResultField::value("title", 0),
            SQLiteResultField::nested(
                "author",
                SQLiteResultShape::new(Some(1), vec![SQLiteResultField::value("email", 2)]),
            ),
        ],
    ));

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
fn native_runner_preserves_hidden_multi_link_parent_identities() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let statement = SQLiteStatement::new(
        "SELECT 'user-1', 'alice@example.com' UNION ALL SELECT 'user-2', 'bob@example.com'",
        vec![],
    )
    .with_result_shape(SQLiteResultShape::new(
        Some(0),
        vec![
            SQLiteResultField::value("email", 1),
            SQLiteResultField::follow_up("posts", 0),
        ],
    ));

    let result = runner
        .execute_select(&statement)
        .expect("select should retain follow-up identities");

    assert_eq!(
        result.rows(),
        &[
            vec![
                crate::SQLiteCellValue::Text("alice@example.com".to_string()),
                crate::SQLiteCellValue::List(vec![]),
            ],
            vec![
                crate::SQLiteCellValue::Text("bob@example.com".to_string()),
                crate::SQLiteCellValue::List(vec![]),
            ],
        ]
    );
    assert_eq!(
        result.follow_up_parent_identities(),
        &[
            vec![Some("user-1".to_string())],
            vec![Some("user-2".to_string())]
        ]
    );
}

#[test]
fn native_runner_preserves_follow_up_row_grouping_identity() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let statement = SQLiteStatement::new("SELECT 'user-1', 'post-1', 'Draft'", vec![])
        .with_result_shape(SQLiteResultShape::new(
            Some(1),
            vec![SQLiteResultField::value("title", 2)],
        ))
        .with_parent_identity_column_index(0);

    let result = runner
        .execute_select(&statement)
        .expect("follow-up select should retain its grouping identity");

    assert_eq!(result.parent_identities(), &[Some("user-1".to_string())]);
    assert_eq!(
        result.rows(),
        &[vec![crate::SQLiteCellValue::Text("Draft".to_string())]]
    );
}

#[test]
fn result_shaping_moves_selected_text_without_cloning() {
    let text = String::from("alice@example.com");
    let allocation = text.as_ptr();
    let mut row = vec![crate::SQLiteCellValue::Text(text)];
    let shape = SQLiteResultShape::new(None, vec![SQLiteResultField::value("email", 0)]);
    let mut follow_up_parent_identities = vec![];

    let result =
        crate::shape_fields_with_identities(&shape, &mut row, &mut follow_up_parent_identities)
            .expect("result should be shaped");

    assert_eq!(row, [crate::SQLiteCellValue::Null]);
    let crate::SQLiteCellValue::Text(value) = &result[0] else {
        panic!("selected text should remain text");
    };
    assert_eq!(value.as_ptr(), allocation);
}

#[test]
fn native_runner_rejects_out_of_range_result_field_column() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let statement = SQLiteStatement::new("SELECT 'Draft'", vec![]).with_result_shape(
        SQLiteResultShape::new(None, vec![SQLiteResultField::value("title", 1)]),
    );

    let error = runner
        .execute_select(&statement)
        .expect_err("out-of-range result field should fail");

    assert_eq!(
        error.message(),
        "result shape column index exceeds SQLite column count"
    );
}

#[test]
fn native_runner_rejects_out_of_range_nested_identity_column() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");
    let statement =
        SQLiteStatement::new("SELECT 'Draft'", vec![]).with_result_shape(SQLiteResultShape::new(
            None,
            vec![SQLiteResultField::nested(
                "author",
                SQLiteResultShape::new(Some(1), vec![]),
            )],
        ));

    let error = runner
        .execute_select(&statement)
        .expect_err("out-of-range nested identity should fail");

    assert_eq!(
        error.message(),
        "result shape identity index exceeds SQLite column count"
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

#[test]
fn inverse_catalog_round_trips_and_rejects_corrupt_metadata() {
    use schema_model::{Cardinality, Field, LinkField, ObjectType, SchemaCatalog};
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "Department",
            vec![Field::Link(LinkField::with_inverse(
                "employees",
                "Employee",
                Cardinality::Many,
                "department",
            ))],
        ),
        ObjectType::new(
            "Employee",
            vec![Field::Link(LinkField::new(
                "department",
                "Department",
                Cardinality::Optional,
            ))],
        ),
    ])
    .expect("valid schema");
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let statements = sqlite_schema_sqlgen::render_initial_schema(&plan);
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema_statements(&mut runner, &statements).expect("apply schema");
    assert_eq!(
        runner.load_schema_catalog().expect("reload catalog"),
        catalog
    );
    runner
        .verify_schema_version()
        .expect("inverse links should match the stored snapshot");
    for metadata in ["field_kind = 'scalar'", "is_implicit = 1", "is_unique = 1"] {
        runner
            .execute(&format!(
                "UPDATE _engine_catalog_fields SET {metadata} WHERE name = 'employees'"
            ))
            .expect("corrupt inverse metadata");
        let error = runner
            .load_schema_catalog()
            .expect_err("invalid inverse metadata must be rejected before field reconstruction");
        assert_eq!(error.message(), "invalid inverse field metadata");
        runner
            .execute(
                "UPDATE _engine_catalog_fields SET field_kind = 'link', is_implicit = 0, is_unique = 0 WHERE name = 'employees'",
            )
            .expect("restore inverse metadata");
    }
    runner.execute("UPDATE _engine_catalog_fields SET inverse_field_name = 'missing' WHERE name = 'employees'").expect("corrupt metadata fixture");
    assert!(runner.load_schema_catalog().is_err());
    runner.execute("UPDATE _engine_catalog_fields SET inverse_field_name = 'department' WHERE name = 'employees'").expect("restore source");
    runner
        .execute(
            "UPDATE _engine_catalog_fields SET cardinality = 'optional' WHERE name = 'employees'",
        )
        .expect("corrupt cardinality");
    assert!(runner.load_schema_catalog().is_err());
}

#[test]
fn legacy_catalog_without_inverse_column_still_loads() {
    let statements = rendered_post_schema_statements();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("database");
    apply_schema_statements(&mut runner, &statements).expect("apply schema");
    let catalog = runner.load_schema_catalog().expect("catalog");
    // Raw metadata DDL recreates the pre-inverse format, not a user schema mutation.
    runner
        .execute("ALTER TABLE _engine_catalog_fields DROP COLUMN inverse_field_name")
        .expect("legacy metadata fixture");
    assert_eq!(
        runner.load_schema_catalog().expect("legacy reload"),
        catalog
    );
}
