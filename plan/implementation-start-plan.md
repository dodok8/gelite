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

## Recommended Next Step After `schema`

Once the `schema` crate can model valid schemas, expose lookup APIs, and reject
invalid schema structure, the next implementation target should be `query-ast`.

This is the recommended next step because:

- `resolver` needs a concrete input model before it can lower anything
- `schema` is now stable enough to serve as the validation target
- implementing `ir` alone first would leave no producer for the data

The goal of `query-ast` is not parsing yet. The first goal is to define a small
Rust AST that can be constructed directly in tests and experiments.

## Recommended Scope For `query-ast`

The first `query-ast` pass should support only the minimum AST needed for one
resolved `select` query.

Include:

- root object selection
- output shape
- nested single-link shape
- filter expressions over paths and literals
- order expressions over paths
- limit
- offset

Do not include yet:

- parser tokens or spans
- pretty-printing
- insert/update/delete
- functions beyond the minimum needed for filters
- computed fields
- advanced query operators

## Recommended Data Structures For `query-ast`

The first pass only needs a small set of AST nodes.

### Core query nodes

- `SelectQuery`
- `Shape`
- `ShapeItem`
- `Path`
- `PathStep`
- `Expr`
- `Literal`
- `OrderExpr`

### Suggested early shape

This is descriptive, not binding API:

- `SelectQuery`
  - root type name
  - shape
  - optional filter expression
  - zero or more order expressions
  - optional limit
  - optional offset
- `Shape`
  - ordered shape items
- `ShapeItem`
  - output field name or selected path step
  - optional nested child shape
- `Path`
  - ordered path steps
- `Expr`
  - literal
  - path expression
  - compare expression
  - boolean combinations if needed
- `Literal`
  - string
  - integer
  - float
  - bool
  - null if needed by the query spec
- `OrderExpr`
  - path
  - direction

## Recommended Test Plan For `query-ast`

As with `schema`, start by testing data-model behavior before adding more API
surface.

### Layer 1: query shape representation tests

Recommended tests:

- `select_query_can_store_root_type_name`
- `shape_can_contain_scalar_field_selection`
- `shape_can_contain_nested_link_selection`
- `shape_preserves_item_definition_order`

What these tests should prove:

- the root selected type is stored explicitly
- scalar shape selections can be represented
- nested link selections can be represented
- shape order is deterministic

### Layer 2: expression and path tests

Recommended tests:

- `path_can_represent_single_step_field_access`
- `path_can_represent_multi_step_link_traversal`
- `literal_expr_can_store_string_values`
- `compare_expr_can_reference_path_and_literal`
- `order_expr_can_reference_a_path`

What these tests should prove:

- path segments are explicit and ordered
- filters can target schema fields through paths
- literals preserve value kinds
- ordering can be attached to a path

### Layer 3: full query assembly tests

Recommended tests:

- `select_query_can_store_filter_order_and_limit`
- `select_query_can_store_nested_shape_with_filter_inputs`

What these tests should prove:

- one complete `select` query can be assembled in Rust
- the AST contains all inputs the resolver will need

## Suggested Test Writing Order For `query-ast`

To keep the first pass small and easy to reason about, write tests in this
order:

1. `select_query_can_store_root_type_name`
2. `shape_can_contain_scalar_field_selection`
3. `shape_can_contain_nested_link_selection`
4. `shape_preserves_item_definition_order`
5. `path_can_represent_single_step_field_access`
6. `compare_expr_can_reference_path_and_literal`
7. `order_expr_can_reference_a_path`
8. `select_query_can_store_filter_order_and_limit`

This order is recommended because it moves from:

- data shape representation
- to path/expression inputs
- to one full resolver-ready query

## First Hardcoded `query-ast` Example

The first AST built in tests should match the first resolver milestone:

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

The `query-ast` crate does not need to parse this string yet. It only needs to
represent this query faithfully as Rust data.

### In `ir`

- `SelectQuery`
- `ResolvedShape`
- `ResolvedShapeField`
- `Expr`
- `CompareExpr`
- `ValueExpr`
- `OrderExpr`
- `ObjectTypeRef` and `FieldRef` from the `schema` crate

## Current Implementation Status

The initial `schema`, `query-ast`, and `ir` passes are now implemented as
small Rust crates.

### Implemented in `schema`

- object type, scalar field, and link field representation
- scalar and link cardinality modeling
- implicit `id` field exposure
- catalog lookup by type name and field name
- schema validation for duplicate type names, duplicate field names, explicit
  `id`, unknown link targets, and reserved scalar type names
- shared reference types:
  - `ObjectTypeId`
  - `ObjectTypeRef`
  - `FieldId`
  - `FieldRef`

### Implemented in `query-ast`

- root object selection
- ordered shape items
- nested shape representation
- paths
- literals
- compare expressions
- order expressions
- filter/order/limit/offset query assembly

The query AST remains parser-independent. Tests construct AST values directly.

### Implemented in `ir`

- `SelectQuery`
- `ResolvedShape`
- `ResolvedShapeField`
- `ValueExpr::Field`
- `ValueExpr::Literal`
- `Expr::Compare`
- `CompareExpr`
- `CompareOp::Eq`
- `OrderExpr`
- `OrderDirection::{Asc, Desc}`
- `limit` and `offset` passthrough

The IR currently models resolved semantic structure only. It does not yet
perform validation by itself. For example, whether a selected field is scalar
or link, whether a link target exists, and whether a filter comparison is type
compatible should be checked by `resolver`.

`ObjectTypeRef` and `FieldRef` are intentionally owned by `schema`, not `ir`.
This keeps resolved references shared across the resolver and downstream IR
consumers without duplicating identity types.

### Next in `resolver`

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

## Resolver Implementation Plan

The resolver is the next implementation target. Its job is to lower
`query-ast` into `ir` by validating names and paths against `schema`.

The resolver should not parse query text, execute queries, generate SQL, or
own schema identity types. It should connect the already implemented crates:

```text
query-ast + schema -> resolver -> ir
```

### Resolver responsibilities

- resolve the selected root type name into `schema::ObjectTypeRef`
- resolve each shape item name against the current source object type
- convert resolved scalar selections into `ir::ResolvedShapeField`
- convert resolved link selections into nested `ir::ResolvedShapeField`
- preserve shape item order from `query-ast`
- propagate field cardinality from `schema` into `ir`
- resolve filter paths into `ir::ValueExpr::Field`
- resolve order paths into `ir::ValueExpr::Field`
- pass `limit` and `offset` through unchanged
- return structured errors for semantic failures

### Resolver non-responsibilities

- parsing query text
- checking schema catalog structural validity
- deciding SQLite table or column names
- generating SQL
- executing queries
- row shaping
- supporting insert/update/delete
- supporting computed expressions or functions

### Suggested public API

Start with a small API that is easy to test:

```rust
pub fn resolve_select(
    catalog: &schema::SchemaCatalog,
    query: &query_ast::SelectQuery,
) -> Result<ir::SelectQuery, ResolveError>
```

Keep `ResolveError` minimal at first. The first useful variants are:

- `UnknownObjectType`
- `UnknownField`
- `NestedShapeOnScalarField`
- `MissingShapeOnLinkField`
- `UnsupportedPath`

The exact payloads can start small, but they should include the relevant type
or field name so failing tests can assert the specific error cause.

### Test writing order for `resolver`

Write resolver tests in this order. Each test should add only one new semantic
contract.

1. `resolves_select_root_object_type`
2. `rejects_unknown_root_object_type`
3. `resolves_scalar_shape_field`
4. `rejects_unknown_shape_field`
5. `resolves_implicit_id_shape_field`
6. `rejects_nested_shape_on_scalar_field`
7. `resolves_link_shape_with_child_shape`
8. `rejects_link_shape_without_child_shape`
9. `preserves_shape_field_order`
10. `resolves_filter_compare_path_to_field`
11. `rejects_filter_path_with_unknown_field`
12. `resolves_order_path_to_field`
13. `passes_limit_and_offset_through`

This order intentionally starts with root type resolution, then shape
resolution, then filter/order resolution. Shape resolution should be stable
before paths in filters and ordering are added.

### First resolver success case

The first successful resolver test should use a smaller query than the full
hardcoded example:

```text
select Post {
  title
}
```

Expected result:

- `ir::SelectQuery.root_object_type()` is `Post`
- `ir::ResolvedShape.source_object_type()` is `Post`
- the shape has one field
- that field references `Post.title`
- cardinality is copied from the schema field
- the field has no child shape

This test proves the minimum end-to-end path without requiring nested links,
filters, ordering, or pagination.

### Shape resolution rules

For each `query-ast::ShapeItem`, the resolver should inspect the field found in
the current source object type.

- scalar field without child shape: valid
- scalar field with child shape: invalid
- link field with child shape: valid
- link field without child shape: invalid for the first resolver pass
- implicit `id`: valid as a scalar field exposed by the catalog

If a link field is valid, the resolver should find the link target object type
and recursively resolve the child shape with that target as the new source
object type.

### Filter and order path rules

For the first resolver pass, keep paths intentionally limited.

- filter paths may resolve to scalar fields
- order paths may resolve to scalar fields
- single-step scalar paths such as `.title` should be implemented first
- link traversal such as `.author.id` can be added after single-step paths are
  stable
- unknown path steps should return a resolver error

The resolver should convert:

```text
query-ast path `.title`
```

into:

```text
ir::ValueExpr::Field(FieldRef(Post.title))
```

`query-ast::Literal` should be converted into `ir::Literal`. At first, string
literals are enough because the current IR only models `Literal::String`.

For the first pass, `query-ast::CompareExpr` is intentionally asymmetric:

```text
left: Path
right: Literal
```

That means the resolver may lower the left side as a resolved field and the
right side as a literal for now. This is not the final expression model.

Later, compare expressions should be generalized so both sides can be value
expressions:

```text
left: ValueExpr
right: ValueExpr
```

That future model should allow cases such as:

- `"Hello" = .title`
- `.created_at = .updated_at`
- computed expressions if the query language grows to support them

Do not implement this generalization during the first resolver pass. The first
goal is to resolve the current AST contract correctly and keep the parser-free
pipeline moving.

### Error handling guidelines

Do not return strings as the primary error representation. Use an enum so tests
can assert exact semantic failures:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownObjectType { name: String },
    UnknownField { object_type: String, field: String },
    NestedShapeOnScalarField { object_type: String, field: String },
    MissingShapeOnLinkField { object_type: String, field: String },
    UnsupportedPath,
}
```

The enum can grow later. Keep the initial variants focused on the tests being
written.

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

The detailed follow-up plan is tracked in
[sqlite-plan-implementation-plan.md](sqlite-plan-implementation-plan.md).
