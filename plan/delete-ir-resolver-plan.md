# Delete IR and Resolver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lower parsed `delete` statements into backend-independent Semantic IR with the same filter semantics already used by `select` and `update`.

**Architecture:** Add a two-field `query_ir::DeleteQuery` containing a resolved object type and optional resolved filter. Add `query_resolver::resolve_delete` as a thin composition of existing object-type lookup and `resolve_expr`; do not introduce delete-specific expression logic or SQLite concepts.

**Tech Stack:** Rust workspace crates `query-ast`, `query-ir`, `query-resolver`, and `schema-model`.

## Global Constraints

- Preserve the parser -> AST -> resolver -> Semantic IR -> SQLite plan boundary.
- Keep SQLite table names, columns, aliases, and SQL out of `query-ir` and `query-resolver`.
- Reuse the existing filter expression resolver without adding a second delete-specific path.
- Preserve an omitted filter as `None`; unfiltered-delete safety prompts are outside this step.
- Add no dependencies, generic query enum, result shaping, ordering, pagination, or assignment model.

---

### Task 1: Add resolved delete Semantic IR

**Files:**
- Modify: `spec/ir.md`
- Modify: `engine/query-ir/src/lib.rs`
- Modify: `engine/query-ir/src/tests/mod.rs`

**Interfaces:**
- Consumes: `schema_model::ObjectTypeRef` and `query_ir::Expr`.
- Produces: `query_ir::DeleteQuery::new(ObjectTypeRef, Option<Expr>)`, `target_object_type(&self) -> &ObjectTypeRef`, and `filter(&self) -> Option<&Expr>`.

- [ ] **Step 1: Tighten the IR contract**

Update the `DeleteQuery` section in `spec/ir.md` to state that the object type is a stable schema reference, the filter is already type-checked, and `None` means every object of the target type.

- [ ] **Step 2: Write the failing IR test**

Add `DeleteQuery` to the imports in `engine/query-ir/src/tests/mod.rs` and add:

```rust
#[test]
fn resolved_delete_query_stores_target_and_filter() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let query = DeleteQuery::new(post_type(), Some(filter.clone()));

    assert_eq!(query.target_object_type().name(), "Post");
    assert_eq!(query.filter(), Some(&filter));
}
```

- [ ] **Step 3: Verify the IR test is RED**

Run:

```sh
cargo test -p query-ir resolved_delete_query_stores_target_and_filter
```

Expected: compilation fails because `query_ir::DeleteQuery` does not exist.

- [ ] **Step 4: Implement the minimum IR node**

Add to `engine/query-ir/src/lib.rs` beside `UpdateQuery`:

```rust
/// Resolved delete query.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteQuery {
    target_object_type: ObjectTypeRef,
    filter: Option<Expr>,
}

impl DeleteQuery {
    pub fn new(target_object_type: ObjectTypeRef, filter: Option<Expr>) -> Self {
        Self {
            target_object_type,
            filter,
        }
    }

    pub fn target_object_type(&self) -> &ObjectTypeRef {
        &self.target_object_type
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }
}
```

- [ ] **Step 5: Verify the IR test is GREEN**

Run:

```sh
cargo test -p query-ir resolved_delete_query_stores_target_and_filter
```

Expected: one matching test passes.

- [ ] **Step 6: Commit the IR contract**

```sh
git add spec/ir.md engine/query-ir/src/lib.rs engine/query-ir/src/tests/mod.rs
git commit -m "Add delete query IR" -m "Assisted-by: Codex:gpt-5.5"
```

---

### Task 2: Resolve parsed delete queries

**Files:**
- Create: `engine/query-resolver/src/tests/delete.rs`
- Modify: `engine/query-resolver/src/tests/mod.rs`
- Modify: `engine/query-resolver/src/lib.rs`

**Interfaces:**
- Consumes: `query_ast::DeleteQuery`, `schema_model::SchemaCatalog`, and the existing private `resolve_expr` function.
- Produces: `resolve_delete(&SchemaCatalog, &query_ast::DeleteQuery) -> Result<query_ir::DeleteQuery, ResolveError>`.

- [ ] **Step 1: Register the delete resolver test module**

Add `mod delete;` to `engine/query-resolver/src/tests/mod.rs`.

- [ ] **Step 2: Write the failing success-path tests**

Create `engine/query-resolver/src/tests/delete.rs` with these imports and helper:

```rust
use alloc::string::ToString;
use alloc::vec;

use query_ast::{CompareExpr, CompareOp, DeleteQuery, Expr, Literal, Path, PathStep};

use crate::tests::fixtures::{post_with_title_catalog, user_only_catalog};
use crate::{ResolveError, resolve_delete};

fn equality_filter(field: &str, literal: Literal) -> Expr {
    Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new(field)])),
        CompareOp::Eq,
        Expr::Literal(literal),
    ))
}
```

Add the filtered and unfiltered cases:

```rust
#[test]
fn resolves_delete_target_and_filter() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new(
        "Post",
        Some(equality_filter(
            "title",
            Literal::String("Draft".to_string()),
        )),
    );

    let resolved = resolve_delete(&catalog, &query).expect("delete query should resolve");

    assert_eq!(resolved.target_object_type().name(), "Post");
    assert!(resolved.filter().is_some());
}

#[test]
fn resolves_unfiltered_delete() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new("Post", None);

    let resolved = resolve_delete(&catalog, &query).expect("delete query should resolve");

    assert_eq!(resolved.target_object_type().name(), "Post");
    assert!(resolved.filter().is_none());
}
```

- [ ] **Step 3: Write the failing diagnostic tests**

Add:

```rust
#[test]
fn rejects_delete_unknown_target_type() {
    let catalog = user_only_catalog();
    let query = DeleteQuery::new("Missing", None);

    let error = resolve_delete(&catalog, &query).expect_err("unknown target should fail");

    assert_eq!(
        error,
        ResolveError::UnknownObjectType {
            name: "Missing".to_string(),
        }
    );
}

#[test]
fn rejects_delete_unknown_filter_field() {
    let catalog = post_with_title_catalog();
    let query = DeleteQuery::new(
        "Post",
        Some(equality_filter(
            "missing",
            Literal::String("value".to_string()),
        )),
    );

    let error = resolve_delete(&catalog, &query).expect_err("unknown filter field should fail");

    assert_eq!(
        error,
        ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        }
    );
}
```

- [ ] **Step 4: Verify the resolver tests are RED**

Run:

```sh
cargo test -p query-resolver delete
```

Expected: compilation fails because `resolve_delete` is not yet available to the resolver tests.

- [ ] **Step 5: Implement the minimum resolver entry point**

Add to `engine/query-resolver/src/lib.rs` beside `resolve_update`:

```rust
/// Resolves a parsed delete query against a validated schema catalog.
pub fn resolve_delete(
    catalog: &schema_model::SchemaCatalog,
    query: &query_ast::DeleteQuery,
) -> Result<query_ir::DeleteQuery, ResolveError> {
    let target_object_type = catalog
        .find_type_ref(query.target_type_name())
        .ok_or_else(|| ResolveError::UnknownObjectType {
            name: query.target_type_name().to_string(),
        })?;
    let filter = query
        .filter()
        .map(|expr| resolve_expr(catalog, &target_object_type, expr))
        .transpose()?;

    Ok(query_ir::DeleteQuery::new(target_object_type, filter))
}
```

- [ ] **Step 6: Verify the resolver tests are GREEN**

Run:

```sh
cargo test -p query-resolver delete
```

Expected: all four delete resolver tests pass.

- [ ] **Step 7: Run the full validation gate**

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

Expected: every command exits successfully with no failed tests, formatting differences, warnings, or whitespace errors.

- [ ] **Step 8: Commit the resolver**

```sh
git add engine/query-resolver/src/lib.rs engine/query-resolver/src/tests/mod.rs engine/query-resolver/src/tests/delete.rs
git commit -m "Add delete query resolver" -m "Assisted-by: Codex:gpt-5.5"
```
