# Delete Runtime Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Execute compiled delete statements through the native runner, expose them through the REPL/CLI with affected-row results, verify referential behavior, and document the completed MVP workflow.

**Architecture:** Reuse the runner's existing prepared DML execution in a shared private helper and expose explicit update/delete methods. Add `QueryKind::Delete` so REPL compilation and CLI execution stay semantically visible while sharing the same affected-row result shape. Exercise the complete parser-to-runner path in query pipeline tests and leave referential cleanup to SQLite foreign keys.

**Tech Stack:** Rust 2024, SQLite through `sqlite3-sys`, existing staged query crates, Cargo tests, Markdown documentation.

## Global Constraints

- Follow issue #36 and `plan/delete-sqlite-design.md`.
- Preserve the `AST -> Semantic IR -> SQLite Plan -> SQL -> runner` pipeline.
- Omitted filters delete all rows; do not add a confirmation prompt.
- Return `affected_rows` instead of deleted objects or identities.
- Preserve SQLite `ON DELETE RESTRICT` errors and `ON DELETE CASCADE` cleanup.
- Add no dependencies and no general top-level query enum.
- Add `Assisted-by: Codex:gpt-5.6-sol` to every commit.

---

### Task 1: Execute deletes in the native runner

**Files:**
- Modify: `engine/sqlite-runner/src/native.rs`
- Modify: `engine/sqlite-runner/src/tests/native.rs`

**Interfaces:**
- Consumes: `sqlite_query_sqlgen::SQLiteStatement`.
- Produces: `NativeSQLiteRunner::execute_delete(&SQLiteStatement) -> Result<i64, SQLiteRunnerError>`.

- [ ] **Step 1: Write failing native runner tests**

Add tests that execute a bound delete, verify restrictive foreign keys fail, and verify cascading join rows disappear:

```rust
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
        vec![sqlite_query_sqlgen::SQLiteBindValue::String("Draft".to_string())],
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
        vec![sqlite_query_sqlgen::SQLiteBindValue::String("user-1".to_string())],
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
        vec![sqlite_query_sqlgen::SQLiteBindValue::String("post-1".to_string())],
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
```

- [ ] **Step 2: Run native delete tests to verify RED**

Run:

```bash
cargo test -p sqlite-runner native_runner_ -- --nocapture
```

Expected: compilation fails because `execute_delete` does not exist.

- [ ] **Step 3: Share prepared DML execution and add `execute_delete`**

Keep `execute_update` public and add a sibling method. Both call one private helper so binding, row counting, and error behavior cannot diverge:

```rust
pub fn execute_update(
    &mut self,
    statement: &sqlite_query_sqlgen::SQLiteStatement,
) -> Result<i64, SQLiteRunnerError> {
    self.execute_mutation(statement, "UPDATE")
}

pub fn execute_delete(
    &mut self,
    statement: &sqlite_query_sqlgen::SQLiteStatement,
) -> Result<i64, SQLiteRunnerError> {
    self.execute_mutation(statement, "DELETE")
}

fn execute_mutation(
    &mut self,
    statement: &sqlite_query_sqlgen::SQLiteStatement,
    operation: &str,
) -> Result<i64, SQLiteRunnerError> {
    let prepared = self
        .connection
        .prepare_v2(statement.sql())
        .map_err(|_| self.connection_error(&format!("prepare {operation}")))?;

    self.bind_query_values(&prepared, statement.bind_values())?;

    match prepared.step() {
        Ok(ResultCode::DONE) => Ok(self.connection.changes64()),
        Ok(result) => Err(self.result_error(&format!("step {operation}"), result)),
        Err(result) => Err(self.result_error(&format!("step {operation}"), result)),
    }
}
```

- [ ] **Step 4: Verify runner GREEN**

Run:

```bash
cargo test -p sqlite-runner
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass.

- [ ] **Step 5: Commit runner execution**

```bash
git add engine/sqlite-runner/src/native.rs engine/sqlite-runner/src/tests/native.rs
git commit -m "Execute delete mutations" -m "Assisted-by: Codex:gpt-5.6-sol"
```

---

### Task 2: Integrate delete into pipeline, REPL, and CLI

**Files:**
- Modify: `tests/query-pipeline/tests/select_execution.rs`
- Modify: `tools/repl/src/lib.rs`
- Modify: `tools/gelite-cli/src/main.rs`

**Interfaces:**
- Consumes: `parse_delete`, `resolve_delete`, `plan_delete`, `render_delete`, and `execute_delete`.
- Produces: `QueryKind::Delete`, compiled delete statements, and the existing `affected_rows` result shape.

- [ ] **Step 1: Write failing end-to-end delete tests**

Import `render_delete` and add an `execute_delete` test helper that performs every compiler stage before calling the runner:

```rust
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
```

Add tests for a root filter, a related filter, and multi-link cleanup:

```rust
#[test]
fn delete_pipeline_executes_root_filter_from_query_text() {
    let mut runner = setup_blog_database();

    let affected_rows = execute_delete(
        &mut runner,
        r#"delete Post filter .id = "post-1""#,
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
fn delete_pipeline_cascades_multi_link_rows() {
    let mut runner = setup_blog_database();

    execute_delete(
        &mut runner,
        r#"delete Post filter .id = "post-1""#,
    );

    let result = runner
        .execute_select(&SQLiteStatement::new(
            "SELECT target_id FROM user__posts ORDER BY target_id",
            vec![],
        ))
        .expect("remaining join rows should be readable");
    assert_eq!(
        result.rows(),
        &[vec![SQLiteCellValue::Text("post-2".to_string())]]
    );
}
```

- [ ] **Step 2: Write a failing REPL compilation test**

Add:

```rust
#[test]
fn compile_query_dispatches_delete_pipeline() {
    let catalog = build_development_schema();

    let (kind, statement) = compile_query(
        &catalog,
        r#"delete Post filter .title = "Draft""#,
        false,
    )
    .expect("delete should compile");

    assert_eq!(kind, QueryKind::Delete);
    assert_eq!(
        statement.sql(),
        "DELETE FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );
}
```

- [ ] **Step 3: Run focused tests to verify RED**

Run:

```bash
cargo test -p query-pipeline-tests delete_pipeline
cargo test -p repl compile_query_dispatches_delete_pipeline
```

Expected: pipeline compilation fails until imports/helper are complete, and REPL compilation fails because `QueryKind::Delete` and the delete dispatch arm do not exist.

- [ ] **Step 4: Compile delete statements in the REPL**

Import `parse_delete`, add `Delete` to `QueryKind`, and add this `compile_query` arm:

```rust
Some("delete") => {
    let query = parse_delete(query_text).map_err(|error| {
        eprintln!("failed to parse query: {error:#?}");
        ReplError
    })?;
    let resolved = query_resolver::resolve_delete(catalog, &query).map_err(|error| {
        eprintln!("failed to resolve query: {error:#?}");
        ReplError
    })?;
    let plan = sqlite_query_plan::plan_delete(&resolved);

    (QueryKind::Delete, sqlite_query_sqlgen::render_delete(&plan))
}
```

Update the unsupported-query message to list `delete`.

- [ ] **Step 5: Execute delete statements in the CLI**

Add a `QueryKind::Delete` match arm beside update. Call `runner.execute_delete(statement)` and return the same result shape:

```rust
Ok(sqlite_runner::SQLiteQueryResult::new(
    vec!["affected_rows".to_string()],
    vec![vec![sqlite_runner::SQLiteCellValue::Integer(affected_rows)]],
))
```

- [ ] **Step 6: Verify the full runtime pipeline**

Run:

```bash
cargo test -p query-pipeline-tests delete_pipeline
cargo test -p repl
cargo test -p gelite-cli
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass.

- [ ] **Step 7: Commit runtime integration**

```bash
git add tests/query-pipeline/tests/select_execution.rs tools/repl/src/lib.rs tools/gelite-cli/src/main.rs
git commit -m "Integrate delete query execution" -m "Assisted-by: Codex:gpt-5.6-sol"
```

---

### Task 3: Document the implemented delete MVP and verify the branch

**Files:**
- Modify: `spec/query.md`
- Modify: `spec/sqlite-query-plan.md`
- Modify: `spec/storage-sqlite.md`
- Modify: `README.md`
- Modify: `README.ko.md`

**Interfaces:**
- Consumes: the committed parser, resolver, planner, sqlgen, runner, and CLI behavior.
- Produces: user and contributor documentation matching the implemented MVP.

- [ ] **Step 1: Update language and SQLite specifications**

State explicitly that the implemented delete subset supports only an optional filter, returns an affected-row count at runtime, renders root filters directly, renders relation filters through an identity subquery, and delegates cleanup/restriction to SQLite foreign keys.

- [ ] **Step 2: Add English and Korean workflows**

Add one filtered delete example and this warning-equivalent fact without introducing a prompt:

```text
delete Post
```

An omitted filter deletes every `Post`. Successful CLI/REPL execution reports `affected_rows`.

- [ ] **Step 3: Check documentation consistency**

Run:

```bash
rg -n "delete Post|affected_rows|unfiltered|filter" README.md README.ko.md spec/query.md spec/sqlite-query-plan.md spec/storage-sqlite.md
git diff --check
```

Expected: both READMEs describe the same behavior and no whitespace errors are reported.

- [ ] **Step 4: Run final verification**

Run:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass with no warnings.

- [ ] **Step 5: Commit documentation**

```bash
git add spec/query.md spec/sqlite-query-plan.md spec/storage-sqlite.md README.md README.ko.md
git commit -m "Document delete mutation workflow" -m "Assisted-by: Codex:gpt-5.6-sol"
```

- [ ] **Step 6: Report branch state**

Run:

```bash
git status --short
git log --oneline --decorate -12
```

Expected: the worktree is clean and the delete feature is represented by small, sequential commits.
