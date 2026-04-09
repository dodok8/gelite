# Semantic IR MVP Spec

## Goal

Define the minimum semantic intermediate representation between parsed query AST
and SQLite-specific lowering.

The Semantic IR exists to:

- preserve the meaning of a query after schema resolution
- carry type and cardinality information
- represent nested result shapes explicitly
- separate query semantics from SQLite table/column details

## Role of Semantic IR

The MVP pipeline is:

1. parse query text into AST
2. resolve schema names and validate semantics
3. produce Semantic IR
4. lower Semantic IR to a SQLite execution plan
5. execute SQLite queries and shape results

Semantic IR is the stage where a query becomes:

- typed
- schema-resolved
- cardinality-aware
- independent from backend-specific naming

## Non-Goals

The Semantic IR MVP does not attempt to model:

- SQL table or column names
- join strategy details
- cost-based optimization
- polymorphism
- inheritance-aware type expansion
- backlinks or inferred inverse traversal
- computed fields
- access policies or rewrites
- function overloading machinery beyond what the MVP query language needs

These concerns belong either to earlier validation or to later backend-specific
planning.

## Core Principles

- Semantic IR must use resolved schema references, not raw identifier strings.
- Semantic IR must be backend-independent.
- Semantic IR must make result shape explicit.
- Semantic IR must preserve cardinality information on paths and fields.
- Semantic IR must be small enough to support the MVP query language without
  importing Gel's full complexity.

## Core Node Types

The MVP Semantic IR should define at least these top-level node categories:

- `SelectQuery`
- `InsertQuery`
- `UpdateQuery`
- `DeleteQuery`
- `ResolvedShape`
- `ResolvedShapeField`
- `ResolvedPath`
- `PathStep`
- `FieldRef`
- `ObjectTypeRef`
- `ScalarTypeRef`
- `LiteralExpr`
- `PathExpr`
- `CompareExpr`
- `AndExpr`
- `OrExpr`
- `NotExpr`
- `OrderExpr`
- `Assignment`

These names are descriptive, not binding implementation names.

## Type References

Type references should be resolved objects, not plain strings.

### `ObjectTypeRef`

Minimum fields:

- stable type id
- type name

### `ScalarTypeRef`

Minimum fields:

- scalar kind id or enum value
- scalar name

The MVP only needs built-in scalar types:

- `str`
- `int64`
- `float64`
- `bool`
- `uuid`
- `datetime`

## Field References

`FieldRef` is a core Semantic IR concept. It should describe the schema field
that a query refers to after resolution.

Minimum fields:

- stable field id
- owning object type
- field name
- field kind: `scalar` or `link`
- target type
- cardinality: `optional`, `required`, or `multi`
- `is_implicit`

This allows the compiler to distinguish:

- explicit scalar fields
- explicit link fields
- implicit fields such as `id`

## Query Nodes

### `SelectQuery`

Minimum fields:

- root object type
- output shape
- optional filter expression
- zero or more order expressions
- optional limit
- optional offset

### `InsertQuery`

Minimum fields:

- root object type
- assignments

### `UpdateQuery`

Minimum fields:

- root object type
- optional filter expression
- assignments

### `DeleteQuery`

Minimum fields:

- root object type
- optional filter expression

## Shape Model

Nested result shaping is a primary responsibility of Semantic IR.

### `ResolvedShape`

Minimum fields:

- source object type
- ordered list of shape fields

### `ResolvedShapeField`

Minimum fields:

- output name
- field reference
- field cardinality
- optional child shape

Rules:

- scalar fields must not have a child shape
- link fields selected in result output must have a child shape
- `multi` links are represented in shape like single links, but retain `multi`
  cardinality metadata for later runtime shaping

## Path Model

Paths are needed for filters, ordering, and resolved field access.

### `ResolvedPath`

Minimum fields:

- root object type
- ordered list of path steps
- result type
- result cardinality

### `PathStep`

Minimum fields:

- field reference
- field kind
- target or result type
- step cardinality

The Semantic IR path model only needs to support:

- root field access
- traversal through declared single links
- terminal scalar access

The Semantic IR MVP does not support:

- backlinks
- inferred inverse relations
- alias scope traversal
- arbitrary subquery paths

## Expression Model

The MVP expression system is intentionally small.

### `LiteralExpr`

Supported literal kinds:

- string
- integer
- float
- boolean
- null

### `PathExpr`

Wraps a resolved path used in a filter or ordering context.

### `CompareExpr`

Minimum fields:

- left expression
- comparison operator
- right expression

Supported operators:

- `=`
- `!=`
- `>`
- `>=`
- `<`
- `<=`

### Boolean Expressions

The MVP also needs:

- `AndExpr`
- `OrExpr`
- `NotExpr`

## Ordering Model

### `OrderExpr`

Minimum fields:

- resolved path
- direction: `asc` or `desc`

## Mutation Model

### `Assignment`

Minimum fields:

- field reference
- value expression

MVP constraints:

- assignments may target declared scalar fields
- assignments may target declared single `link` fields
- assignments may not target implicit `id`
- assignments may not target `multi` links

## Boundary With SQLite Planning

Semantic IR must stop before backend-specific physical details.

Semantic IR includes:

- resolved types
- resolved fields
- cardinality
- shape tree
- filter expressions
- ordering
- mutation targets

Semantic IR does not include:

- SQLite table names
- SQLite column names
- join conditions
- SQL snippets
- statement batching strategy
- index selection

Those belong to a separate SQLite planning layer.

## Worked Example

For:

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

The Semantic IR should capture at least:

- root object type `Post`
- shape fields `id`, `title`, and `author`
- `author` resolved as a declared single `link` from `Post` to `User`
- nested child shape on `User` containing `id` and `name`
- filter path `Post.author.id`
- ordering path `Post.title`
- result cardinalities for all selected fields and paths

It should not yet contain:

- `post` table name
- `author_id` column name
- concrete SQL join text

## Relationship To Gel

This MVP follows Gel's high-level design principle of compiling queries into a
typed, schema-resolved intermediate form before backend lowering.

It intentionally does not adopt Gel's full IR complexity. In particular, the
MVP defers:

- separate scope-tree modeling
- advanced path identity machinery
- polymorphic and inheritance-heavy typing
- backend-specific execution overlays

The goal is to borrow Gel's staging discipline without copying its full
implementation surface.
