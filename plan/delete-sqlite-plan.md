# SQLite Delete Planning and SQL Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lower resolved delete queries into SQLite plans and render executable delete statements for absent, root, and relation filters.

**Architecture:** Add a dedicated `SQLiteDeletePlan` with the same target and predicate representation used by updates. Extract only the mutation-filter planning and rendering branches that now have two callers; keep assignment handling update-only and leave execution to the runner.

**Tech Stack:** Rust 2024, `no_std` engine crates with `alloc`, existing query IR/planner/sqlgen types, Cargo tests.

## Global Constraints

- Follow `spec/query.md`, `spec/storage-sqlite.md`, `spec/sqlite-query-plan.md`, and `plan/delete-sqlite-design.md`.
- Preserve the staged `AST -> Semantic IR -> SQLite Plan -> SQL` pipeline.
- Add no dependencies and no delete-specific expression representation.
- Omitted filters delete all target rows; command-layer safety prompts are out of scope.
- Let SQLite foreign keys enforce `ON DELETE CASCADE` and `ON DELETE RESTRICT` behavior.
- Add `Assisted-by: Codex:gpt-5.5` to every commit.

---

### Task 1: Add SQLite delete planning

**Files:**
- Create: `engine/sqlite-query-plan/src/tests/delete.rs`
- Modify: `engine/sqlite-query-plan/src/tests/mod.rs`
- Modify: `engine/sqlite-query-plan/src/lib.rs`

**Interfaces:**
- Consumes: `query_ir::DeleteQuery`, `query_ir::Expr`, and existing predicate planning functions.
- Produces: `plan_delete(&query_ir::DeleteQuery) -> SQLiteDeletePlan` and getters for target, filter, and joins.

- [ ] **Step 1: Register the delete planner tests**

Add this module to `engine/sqlite-query-plan/src/tests/mod.rs`:

```rust
mod delete;
```

- [ ] **Step 2: Write failing planner tests**

Create `engine/sqlite-query-plan/src/tests/delete.rs`:

```rust
use alloc::string::ToString;

use query_ir::{CompareExpr, CompareOp, DeleteQuery, Expr, Literal};

use super::fixtures::{
    post_author_name_path_value, post_title_path_value, post_type,
};
use crate::{SQLiteJoinKind, SQLiteValueExpr, SQLiteWhereExpr, plan_delete};

#[test]
fn sqlite_delete_plan_targets_one_root_table() {
    let plan = plan_delete(&DeleteQuery::new(post_type(), None));

    assert_eq!(plan.target().object_type().name(), "Post");
    assert_eq!(plan.target().table_name(), "post");
    assert_eq!(plan.target().alias(), "root");
    assert_eq!(plan.target().id_column(), "id");
    assert!(plan.filter().is_none());
    assert!(plan.joins().is_empty());
}

#[test]
fn sqlite_delete_plan_reuses_root_filter_planning() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));

    let plan = plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };
    let SQLiteValueExpr::Column(column) = compare.left() else {
        panic!("expected root column");
    };
    assert_eq!(column.source_alias(), "root");
    assert_eq!(column.column_name(), "title");
    assert!(plan.joins().is_empty());
}

#[test]
fn sqlite_delete_plan_uses_join_for_related_filter_path() {
    let filter = Expr::Compare(CompareExpr::new(
        post_author_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));

    let plan = plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    assert_eq!(plan.joins().len(), 1);
    assert_eq!(plan.joins()[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(plan.joins()[0].source_alias(), "root");
    assert_eq!(plan.joins()[0].target_table(), "user");
    assert_eq!(plan.joins()[0].target_alias(), "author");

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };
    let SQLiteValueExpr::Column(column) = compare.left() else {
        panic!("expected related column");
    };
    assert_eq!(column.source_alias(), "author");
    assert_eq!(column.column_name(), "name");
}
```

- [ ] **Step 3: Run the planner tests to verify RED**

Run:

```bash
cargo test -p sqlite-query-plan delete
```

Expected: compilation fails because `plan_delete` and `SQLiteDeletePlan` do not exist.

- [ ] **Step 4: Extract shared predicate planning and add the delete plan**

Replace the inline filter planning in `plan_update` with:

```rust
let (filter, joins) = plan_mutation_filter(ir.filter());
```

Add:

```rust
/// Lowers a resolved delete query to a structured SQLite delete plan.
pub fn plan_delete(ir: &query_ir::DeleteQuery) -> SQLiteDeletePlan {
    let target_object_type = ir.target_object_type().clone();
    let (filter, joins) = plan_mutation_filter(ir.filter());

    SQLiteDeletePlan {
        target: SQLiteObjectSource {
            table_name: sqlite_table_name(&target_object_type),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: target_object_type,
        },
        filter,
        joins,
    }
}

fn plan_mutation_filter(
    filter: Option<&query_ir::Expr>,
) -> (Option<SQLiteWhereExpr>, Vec<SQLiteJoin>) {
    let mut reserved_aliases = vec![];
    if let Some(filter) = filter {
        collect_root_path_aliases_from_expr(filter, &mut reserved_aliases);
    }
    let mut join_aliases = SQLiteJoinAliasAllocator::new(reserved_aliases);

    match filter {
        Some(expr) => {
            let planned = plan_where_expr(expr, &mut join_aliases);
            (Some(planned.expr), dedup_joins(planned.joins))
        }
        None => (None, vec![]),
    }
}

/// Structured SQLite plan for deleting resolved objects.
pub struct SQLiteDeletePlan {
    target: SQLiteObjectSource,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
}

impl SQLiteDeletePlan {
    pub fn target(&self) -> &SQLiteObjectSource {
        &self.target
    }

    pub fn filter(&self) -> Option<&SQLiteWhereExpr> {
        self.filter.as_ref()
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }
}
```

- [ ] **Step 5: Verify planner GREEN and existing update behavior**

Run:

```bash
cargo test -p sqlite-query-plan delete
cargo test -p sqlite-query-plan update
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass.

- [ ] **Step 6: Commit the planner**

```bash
git add engine/sqlite-query-plan/src/lib.rs engine/sqlite-query-plan/src/tests/mod.rs engine/sqlite-query-plan/src/tests/delete.rs
git commit -m "Add SQLite delete planning" -m "Assisted-by: Codex:gpt-5.5"
```

---

### Task 2: Render SQLite delete statements

**Files:**
- Create: `engine/sqlite-query-sqlgen/src/tests/delete.rs`
- Modify: `engine/sqlite-query-sqlgen/src/tests/mod.rs`
- Modify: `engine/sqlite-query-sqlgen/src/lib.rs`

**Interfaces:**
- Consumes: `SQLiteDeletePlan`, `SQLiteObjectSource`, `SQLiteWhereExpr`, and `SQLiteJoin`.
- Produces: `render_delete(&SQLiteDeletePlan) -> SQLiteStatement`.

- [ ] **Step 1: Register the SQL generation tests**

Add this module to `engine/sqlite-query-sqlgen/src/tests/mod.rs`:

```rust
mod delete;
```

- [ ] **Step 2: Write failing SQL generation tests**

Create `engine/sqlite-query-sqlgen/src/tests/delete.rs`:

```rust
use alloc::string::ToString;

use query_ir::{CompareExpr, CompareOp, DeleteQuery, Expr, Literal};

use super::fixtures::{
    post_author_name_path_value, post_title_path_value, post_type,
};
use crate::{SQLiteBindValue, render_delete};

#[test]
fn sqlite_sqlgen_can_render_unfiltered_delete() {
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), None));

    let statement = render_delete(&plan);

    assert_eq!(statement.sql(), "DELETE FROM \"post\"");
    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_filtered_delete() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    let statement = render_delete(&plan);

    assert_eq!(
        statement.sql(),
        "DELETE FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Draft".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_selects_joined_delete_targets_in_subquery() {
    let filter = Expr::Compare(CompareExpr::new(
        post_author_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    let statement = render_delete(&plan);

    assert_eq!(
        statement.sql(),
        "DELETE FROM \"post\" WHERE \"id\" IN (SELECT \"root\".\"id\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"name\" = ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}
```

- [ ] **Step 3: Run the SQL generation tests to verify RED**

Run:

```bash
cargo test -p sqlite-query-sqlgen delete
```

Expected: compilation fails because `render_delete` does not exist.

- [ ] **Step 4: Share mutation predicate rendering and add delete rendering**

Import `SQLiteDeletePlan`, `SQLiteJoin`, and `SQLiteObjectSource`. Move the filter branch in `render_update` into:

```rust
fn append_mutation_filter(
    sql: &mut String,
    target: &SQLiteObjectSource,
    filter: Option<&SQLiteWhereExpr>,
    joins: &[SQLiteJoin],
    bind_values: &mut Vec<SQLiteBindValue>,
) {
    let Some(filter) = filter else {
        return;
    };
    let filter_sql = render_where_expr(filter, bind_values);

    if joins.is_empty() {
        sql.push_str(&format!(" WHERE {filter_sql}"));
    } else {
        let joins = render_joins(joins).join(" ");
        sql.push_str(&format!(
            " WHERE {} IN (SELECT {} FROM {} AS {} {joins} WHERE {filter_sql})",
            quote_identifier(target.id_column()),
            render_qualified_identifier(target.alias(), target.id_column()),
            quote_identifier(target.table_name()),
            quote_identifier(target.alias()),
        ));
    }
}
```

Call it from `render_update`, then add:

```rust
/// Renders a structured SQLite delete plan into SQL text and bind values.
pub fn render_delete(plan: &SQLiteDeletePlan) -> SQLiteStatement {
    let target = plan.target();
    let mut sql = if plan.filter().is_some() && plan.joins().is_empty() {
        format!(
            "DELETE FROM {} AS {}",
            quote_identifier(target.table_name()),
            quote_identifier(target.alias())
        )
    } else {
        format!("DELETE FROM {}", quote_identifier(target.table_name()))
    };
    let mut bind_values = Vec::new();

    append_mutation_filter(
        &mut sql,
        target,
        plan.filter(),
        plan.joins(),
        &mut bind_values,
    );

    SQLiteStatement { sql, bind_values }
}
```

- [ ] **Step 5: Verify SQL generation GREEN and existing update behavior**

Run:

```bash
cargo test -p sqlite-query-sqlgen delete
cargo test -p sqlite-query-sqlgen update
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass.

- [ ] **Step 6: Run complete verification**

Run:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Expected: all commands pass with no warnings.

- [ ] **Step 7: Commit SQL generation**

```bash
git add engine/sqlite-query-sqlgen/src/lib.rs engine/sqlite-query-sqlgen/src/tests/mod.rs engine/sqlite-query-sqlgen/src/tests/delete.rs
git commit -m "Render SQLite delete statements" -m "Assisted-by: Codex:gpt-5.5"
```

---

### Task 3: Define the next integration slice

**Files:**
- Inspect: `engine/sqlite-runner/src/lib.rs`
- Inspect: `tests/query-pipeline/tests/select_execution.rs`
- Inspect: `tools/repl/src/lib.rs`

**Interfaces:**
- Consumes: `render_delete(&SQLiteDeletePlan) -> SQLiteStatement`.
- Produces: a separately reviewed plan for runner execution, pipeline coverage, and user-facing command integration.

- [ ] **Step 1: Confirm the committed planner and SQL generator leave the worktree clean**

Run:

```bash
git status --short
git log --oneline -6
```

Expected: no worktree changes and separate planner/sqlgen commits are visible.

- [ ] **Step 2: Trace the existing update execution path**

Read the runner, query pipeline test, command, and REPL call sites that execute updates. Record only the files and interfaces required to add delete execution; do not implement them in the SQLite planning commits.

- [ ] **Step 3: Write the next plan**

Create a new plan covering delete execution, affected-row reporting, foreign-key restriction tests, end-to-end parsing through execution, and CLI/REPL exposure. Keep runtime and user-interface commits independently testable.
