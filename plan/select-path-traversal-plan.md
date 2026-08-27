# Select Path Traversal Plan

Status update for #66: the original single-link milestone below is complete.
Stored and declared inverse multi paths now extend filter comparisons against
literals with independent existence semantics. Ordering and computed values
retain the single-path restriction. See `spec/query.md` for the current
contract; same-target predicate scopes remain deferred to #68.

## Purpose

This document describes the next select-query expansion after the basic
`parser -> resolver -> ir -> sqlite-query-plan -> sqlite-query-sqlgen -> repl` pipeline.

The current pipeline can parse multi-step paths, but the resolver and IR only
preserve enough information for root scalar fields. The next milestone is to
support filter and order paths that traverse declared single links.

Examples:

```text
select Post { title } filter .author.name = "Sheri"
select Post { title } order by .author.name asc
```

This should be implemented before larger features such as functions, `count`,
`in`, aggregation, or schema-definition parsing. Path traversal is a core
semantic capability that those later features will depend on.

## Current State

### Parser

The parser can already read multi-step filter and order paths:

```text
.author.name
.author.id
```

`query-ast::Path` stores the ordered path steps as field-name strings. It does
not record whether the path had a leading root-relative dot.

### Resolver

The resolver currently resolves filter and order paths as single-step scalar
fields only.

This is enough for:

```text
filter .title = "Hello"
order by .title desc
```

It is not enough for:

```text
filter .author.name = "Sheri"
order by .author.name
```

Returning only `FieldRef(User.name)` would lose the traversal path from
`Post.author` to `User.name`, so sqlite planning would not know which join is
needed.

### IR

The current IR has:

```text
ValueExpr::Field(FieldRef)
ValueExpr::Literal(Literal)
Expr::Compare(...)
Expr::IsNull(...)
OrderExpr { value: ValueExpr, direction }
```

This represents root scalar fields well, but it does not preserve path steps.

### SQLite Planning

`sqlite-query-plan` can already produce joins for selected single-link nested shapes.

It does not yet produce joins for:

- filter traversal through a single link
- ordering traversal through a single link

### SQL Generation

`sqlite-query-sqlgen` now renders joins that are already present in the SQLite plan.
It should not invent new joins. Join planning belongs to `sqlite-query-plan`.

## Target Semantics

The MVP should support traversal through declared single links and terminate on
a scalar field.

Supported:

```text
filter .author.name = "Sheri"
filter .author.id = "user-1"
order by .author.name asc
```

Still unsupported:

- traversal through `multi link`
- traversal through scalar fields
- backlinks or inferred inverse traversal
- arbitrary alias-scoped paths
- functions over paths
- aggregation over paths

## Recommended IR Change

Introduce a resolved path representation.

Suggested shape:

```rust
pub struct ResolvedPath {
    root_object_type: ObjectTypeRef,
    steps: Vec<ResolvedPathStep>,
    result_cardinality: Cardinality,
}

pub struct ResolvedPathStep {
    field: FieldRef,
    kind: ResolvedPathStepKind,
    cardinality: Cardinality,
}

pub enum ResolvedPathStepKind {
    Scalar,
    Link { target_object_type: ObjectTypeRef },
}
```

Then change value expressions from root-field-only references to path-aware
references:

```rust
pub enum ValueExpr {
    Path(ResolvedPath),
    Literal(Literal),
}
```

Alternative:

```rust
pub enum ValueExpr {
    Field(FieldRef),
    Path(ResolvedPath),
    Literal(Literal),
}
```

The first option is cleaner because a root field is just a one-step path. The
second option may reduce migration churn. Prefer the first option if the change
is still manageable.

## Resolver Plan

Add path resolution that walks each `query_ast::PathStep` against the schema
catalog.

Resolution rules:

- Start from the select root object type.
- Each non-terminal step must be a single link.
- A non-terminal scalar field is invalid.
- A non-terminal multi link is unsupported in the MVP.
- The terminal step used in filter/order must be scalar.
- Unknown steps return the existing unknown-field diagnostic or a more precise
  path diagnostic.

Recommended first tests:

```text
resolves_filter_path_through_single_link_to_scalar_field
resolves_order_path_through_single_link_to_scalar_field
rejects_filter_path_traversal_through_scalar_field
rejects_filter_path_traversal_through_multi_link
rejects_filter_path_with_unknown_nested_field
```

## SQLite Plan

`sqlite-query-plan` should lower `ResolvedPath` into:

- a column reference for the terminal scalar field
- zero or more joins for link traversal

For:

```text
filter .author.name = "Sheri"
```

expected physical shape:

```sql
INNER JOIN user AS author ON root.author_id = author.id
WHERE author.name = ?
```

For:

```text
order by .author.name asc
```

expected physical shape:

```sql
INNER JOIN user AS author ON root.author_id = author.id
ORDER BY author.name ASC
```

Join aliasing can start with the link field output/name, such as `author`.
This is temporary. Later, alias generation must become collision-resistant for
repeated paths or multiple links to the same target type.

The planner must not lower a traversed path by looking only at the final field.
For example, `Post.author.name` is not `root.name`; it is `author.name` plus the
join that makes the `author` alias valid.

The core lowering should therefore be modeled as path planning, not merely as
column conversion:

```text
ResolvedPath -> PlannedPath { column, joins }
```

Suggested internal shape:

```rust
struct PlannedPath {
    column: SQLiteColumnRef,
    joins: Vec<SQLiteJoin>,
}
```

The first supported traversal should cover any chain of required or optional
single links that ends in a scalar field:

```text
Post.title
=> column root.title
=> joins []

Post.author.name
=> column author.name
=> joins [root.author_id -> author.id]

Post.author.organization.name
=> column author_organization.name
=> joins [
     root.author_id -> author.id,
     author.organization_id -> author_organization.id
   ]
```

`multi` link traversal must not be treated as a direct foreign-key join. It
requires a join table and should stay unsupported until the SQLite storage model
and planner tests cover it explicitly.

## Join Deduplication

The first implementation may produce duplicate joins if the same link path is
used in shape, filter, and order.

However, the planner should quickly move toward deduplicating joins by logical
path:

```text
root.author
root.author.organization
```

Do not deduplicate merely by target table name. Two different links can point to
the same table but require different aliases and join conditions.

## SQLite SQL Generation

`sqlite-query-sqlgen` should continue to render only the joins present in
`SQLiteSelectPlan`.

No resolver or path traversal logic should be added to SQL generation.

## Suggested Implementation Order

1. Add IR tests for `ResolvedPath`.
2. Add `ResolvedPath` and `ResolvedPathStep` types.
3. Migrate root field value expressions to one-step resolved paths.
4. Update resolver root scalar filter/order tests.
5. Add resolver tests for `.author.name`.
6. Teach resolver to resolve single-link path traversal.
7. Add a sqlite-query-plan failing test proving that `Post.author.name` must lower to
   `author.name`, not `root.name`.
8. Add a sqlite-query-plan failing test proving that filter/order path traversal adds
   the required join.
9. Introduce internal `PlannedPath` / `plan_resolved_path` logic that returns
   both terminal column and joins.
10. Update sqlite-query-plan to collect joins from shape, filter, and order paths.
11. Add join deduplication by logical traversal path, not merely target table.
12. Update sqlite-query-sqlgen tests only if join rendering or ordering output
    changes.
13. Add REPL smoke examples for traversed filter/order paths.

## Why Not `count` Or `in` First

`count` requires aggregate expressions, result cardinality decisions, and SQL
aggregation lowering.

`in` requires list literals, bind expansion, or subquery modeling.

Both features depend on a stable expression/path model. Implementing path
traversal first reduces the risk of building those features on top of an
insufficient IR.
