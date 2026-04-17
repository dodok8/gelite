# Implementation Start Plan

## Goal

Start implementation with the smallest slice that validates the architecture
decisions already captured in the product, schema, query, storage, Semantic IR,
and SQLite Plan specs.

The first implementation goal is not "a full engine". It is:

- a Rust workspace
- a schema catalog model
- a minimal query AST
- a Semantic IR model
- a resolver that lowers one `select` query from AST to Semantic IR

This is the first useful proof that the document stack is coherent.

## First Coding Milestone

The first milestone should prove this end-to-end path:

1. define a schema catalog in Rust
2. construct a small query AST for `select`
3. resolve the AST against the schema catalog
4. produce Semantic IR
5. inspect the IR in tests

This milestone should not yet require:

- a parser
- SQLite execution
- SQL generation
- migrations
- HTTP APIs
- CLI workflows

## Why Start Here

This starting point is recommended because it tests the most important
contracts first:

- scalar vs `link` field distinction
- implicit `id` handling
- cardinality propagation
- nested shape rules
- path validation in filters and ordering

If these rules are unstable, everything downstream will be unstable too.

## Recommended Initial Module Boundaries

For the first Rust implementation pass, use a small workspace with narrowly
scoped crates:

- `schema`
- `query-ast`
- `ir`
- `resolver`
- `dev-cli`

### `schema`

Responsibilities:

- object type definitions
- field definitions
- built-in scalar type enum
- cardinality enum
- implicit `id` support
- catalog lookup APIs

### `query-ast`

Responsibilities:

- minimal AST for the MVP query language
- start with `select`
- shape items
- filter expressions
- order expressions
- literals

This crate does not need parsing yet. The AST may be constructed directly in
tests at first.

### `ir`

Responsibilities:

- Semantic IR types from [ir.md](/home/dodok8/Development/gelite/spec/ir.md)
- resolved query nodes
- resolved shape/path/expression nodes
- type and field references

### `resolver`

Responsibilities:

- AST validation against schema catalog
- AST to Semantic IR lowering
- semantic diagnostics

This crate is the most important first implementation target.

### `dev-cli`

Responsibilities:

- temporary binary for local experiments
- load a hardcoded schema catalog
- build a hardcoded AST
- print the resolved Semantic IR

This is a development tool, not the final user-facing CLI.

## Deferred Initial Crates

These should wait until the first milestone is done:

- `sqlite-plan`
- `sqlgen`
- `runtime`
- `server`
- `web`
- `migrations`

The planner layer should come next, but only after the Semantic IR resolver is
stable enough to trust.

## First Data Structures To Implement

### In `schema`

- `ObjectType`
- `Field`
- `FieldKind`
- `ScalarType`
- `Cardinality`
- `SchemaCatalog`

Suggested minimum behavior:

- lookup type by name
- lookup field by `(object_type, field_name)`
- enumerate declared fields
- inject implicit `id`

## Recommended Test Plan For `schema`

Because this project is also a learning project, implementation should start by
writing tests that lock down the schema model contracts before expanding the
API surface.

The `schema` crate should be implemented in three test layers.

### Layer 1: model representation tests

These tests verify that the Rust data model can represent the schema semantics
described in [schema.md](/home/dodok8/Development/gelite/spec/schema.md).

Recommended tests:

- `object_type_exposes_declared_scalar_fields`
- `object_type_exposes_declared_link_fields`
- `object_type_enumerates_declared_fields_in_definition_order`
- `implicit_id_field_exists_on_every_object_type`
- `implicit_id_field_is_required_uuid`
- `declared_fields_do_not_include_implicit_id`

What these tests should prove:

- scalar fields preserve scalar type and cardinality
- link fields preserve target type and cardinality
- declared field order is deterministic
- every object type exposes an implicit `id`
- implicit `id` is available in lookup but not mixed into declared fields

### Layer 2: catalog lookup tests

These tests verify that `SchemaCatalog` is usable as a stable input to the
future resolver layer.

Recommended tests:

- `catalog_can_lookup_type_by_name`
- `catalog_returns_none_for_unknown_type`
- `catalog_can_lookup_field_by_type_and_name`
- `catalog_field_lookup_can_find_implicit_id`
- `catalog_returns_none_for_unknown_field`
- `catalog_preserves_type_iteration_order`
- `catalog_preserves_field_iteration_order_within_type`

What these tests should prove:

- type lookup works by name
- field lookup works by `(object_type, field_name)`
- implicit fields are visible through lookup APIs
- iteration order is deterministic for both types and fields

### Layer 3: structural validation tests

These tests verify the minimum semantic validation rules that belong in the
schema catalog layer even before a parser exists.

Recommended tests:

- `rejects_duplicate_type_names`
- `rejects_duplicate_field_names_within_type`
- `rejects_explicit_id_field_declaration`
- `rejects_unknown_link_target`
- `rejects_reserved_scalar_type_name_as_object_type_name`

These should be added after the basic model and lookup tests are passing.

### Tests To Defer

The following validation cases are real requirements from the schema spec, but
they may be better enforced later by parser/AST design or by more explicit type
construction APIs:

- `rejects_multi_on_scalar_field`
- `rejects_link_with_scalar_target`
- `rejects_object_type_target_without_link`
- `rejects_reserved_keyword_identifiers`

If the Rust constructors make these invalid states unrepresentable, they do not
need to be tested as runtime validation in the first pass.

### Suggested Test Writing Order

To keep failures easy to interpret, write tests in this order:

1. `object_type_exposes_declared_scalar_fields`
2. `object_type_exposes_declared_link_fields`
3. `implicit_id_field_exists_on_every_object_type`
4. `implicit_id_field_is_required_uuid`
5. `declared_fields_do_not_include_implicit_id`
6. `catalog_can_lookup_type_by_name`
7. `catalog_can_lookup_field_by_type_and_name`
8. `catalog_field_lookup_can_find_implicit_id`
9. `catalog_preserves_type_iteration_order`
10. `rejects_duplicate_type_names`
11. `rejects_duplicate_field_names_within_type`
12. `rejects_explicit_id_field_declaration`
13. `rejects_unknown_link_target`

This order is recommended because it separates:

- data model design failures
- lookup API failures
- validation rule failures

### Suggested Test Module Shape

For the first pass, keep tests in `schema/src/lib.rs` or a nearby unit-test
module grouped by concern:

```rust
#[cfg(test)]
mod tests {
    mod object_type_model {}
    mod catalog_lookup {}
    mod validation {}
}
```

The main goal is that the test names themselves describe the schema contract
clearly enough that work can be resumed later without rediscovering intent.

### In `query-ast`

- `SelectQuery`
- `Shape`
- `ShapeItem`
- `Path`
- `PathStep`
- `Expr`
- `Literal`
- `OrderExpr`

### In `ir`

- `SelectQuery`
- `ResolvedShape`
- `ResolvedShapeField`
- `ResolvedPath`
- `FieldRef`
- `ObjectTypeRef`
- `ScalarTypeRef`
- `Expr`
- `OrderExpr`

### In `resolver`

Start with support for:

- root object selection
- scalar shape fields
- single `link` shape fields
- nested shapes
- filter path resolution
- order path resolution
- limit/offset passthrough

Do not start with:

- insert/update/delete
- `multi link` result fetching
- SQLite planning

## First Example To Hardcode

Use this schema shape:

```text
type User {
  required name: str
}

type Post {
  required title: str
  body: str
  required link author: User
}
```

Use this query shape:

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
filter .author.id = "00000000-0000-0000-0000-000000000001"
order by .title asc
limit 10
```

This one example is enough to validate:

- implicit `id`
- scalar selection
- single-link nested shape
- filter path traversal
- ordering path traversal

## Suggested Development Sequence

1. create the Rust workspace
2. define `schema` types and catalog lookup
3. define the minimal query AST
4. define Semantic IR types
5. implement AST-to-IR resolver for `select`
6. add tests for valid and invalid cases
7. add the temporary development CLI binary

Only after this works should implementation move on to:

1. SQLite Plan types
2. Semantic IR to SQLite Plan lowering
3. SQL generation
4. row shaping runtime

## Suggested Early Tests

Add tests for:

- selecting an unknown type
- selecting an unknown field
- selecting a scalar field with nested shape
- selecting a `link` field without nested shape
- resolving implicit `id`
- resolving `.author.id` through a declared single `link`
- rejecting backlink-like traversal

These tests will protect the architecture better than broad end-to-end work too
early.

## Commands To Create The Initial Workspace

Run these commands from the repository root:

```bash
mkdir -p engine/crates
cargo new --lib engine/crates/schema
cargo new --lib engine/crates/query-ast
cargo new --lib engine/crates/ir
cargo new --lib engine/crates/resolver
cargo new --bin engine/crates/dev-cli
```

Then create `engine/Cargo.toml` as a workspace manifest:

```toml
[workspace]
members = [
  "crates/schema",
  "crates/query-ast",
  "crates/ir",
  "crates/resolver",
  "crates/dev-cli",
]

resolver = "2"
```

Then wire dependencies approximately like this:

- `resolver` depends on `schema`, `query-ast`, and `ir`
- `dev-cli` depends on `schema`, `query-ast`, `ir`, and `resolver`

## Recommended First Work Session

If starting immediately, the best order is:

1. create the workspace and crates
2. implement the `schema` crate first
3. implement the `ir` crate second
4. implement the `query-ast` crate third
5. implement the `resolver` crate fourth
6. add one successful `select` resolution test
7. add a few failing semantic tests

## Definition of Done For This Phase

This phase is complete when:

- the workspace builds
- a hardcoded schema catalog can be created
- a hardcoded `select` AST can be lowered to Semantic IR
- tests cover the basic success and failure cases
- no SQLite execution code is needed yet

At that point, the next planning conversation should focus on the
`Semantic IR -> SQLite Plan` lowering implementation.
