# SQLite Plan MVP Spec

## Goal

Define the minimum SQLite-specific planning layer between Semantic IR and SQL
generation/runtime execution.

The SQLite Plan exists to:

- translate Semantic IR into SQLite-aware access patterns
- choose the tables, columns, joins, and follow-up fetches needed for execution
- preserve enough result-shaping metadata to reconstruct nested results
- keep backend-specific concerns out of Semantic IR

## Role of SQLite Plan

The MVP pipeline is:

1. parse query text into AST
2. resolve schema names and validate semantics
3. produce Semantic IR
4. lower Semantic IR to SQLite Plan
5. generate SQL statements from SQLite Plan
6. execute SQL and shape results

The SQLite Plan is the first backend-specific layer.

It knows:

- physical table names
- physical column names
- join edges
- join-table access for `multi link`
- how many SQL statements are needed

It does not yet need to be raw SQL text.

## Non-Goals

The SQLite Plan MVP does not attempt to model:

- cost-based optimization
- backend portability
- query-plan caching policy
- index selection heuristics
- SQLite virtual tables or FTS planning
- generalized optimizer rewrite passes
- alternative join orders chosen by statistics

The MVP planner should prefer predictability and simplicity over cleverness.

## Core Principles

- SQLite Plan must be derived from valid Semantic IR.
- SQLite Plan may use physical storage names and layout details.
- SQLite Plan should stay structured; do not collapse immediately into SQL
  strings.
- SQLite Plan should make result-shaping responsibilities explicit.
- SQLite Plan should optimize for correctness and explainability first.

## Relationship To Other Specs

- [ir.md](/home/dodok8/Development/gelite/spec/ir.md) defines the backend-independent
  semantic layer.
- [storage-sqlite.md](/home/dodok8/Development/gelite/spec/storage-sqlite.md) defines the
  physical SQLite storage model.

SQLite Plan sits between them:

- it takes resolved semantic meaning from Semantic IR
- it uses the physical mapping rules from the storage spec
- it produces an execution structure that SQL generation can serialize

## Plan Families

The MVP needs at least these top-level plan families:

- `SQLiteSelectPlan`
- `SQLiteInsertPlan`
- `SQLiteUpdatePlan`
- `SQLiteDeletePlan`
- `SQLiteMultiLinkMutationPlan`
- `SQLiteFollowUpFetchPlan`

These names are descriptive, not binding implementation names.

## Common Planning Inputs

Every SQLite plan is derived from:

- resolved root object type
- storage naming rules
- field-to-column or field-to-join-table mapping
- cardinality information from Semantic IR
- shape requirements for the result

## Common Planning Concepts

### Physical Object Source

The planner needs a way to refer to the root object table for a query.

Minimum fields:

- object type
- table name
- row identity column

### Physical Field Access

The planner needs a resolved physical access form for a schema field.

Possible forms in the MVP:

- direct scalar column
- single-link foreign key column
- multi-link join table access

Minimum fields:

- schema field reference
- access kind
- table name or join table name
- column names involved

### Result Slot

The planner should track which physical values are fetched for later shaping.

Minimum fields:

- stable slot name or id
- source table alias
- column name
- logical output role

Examples:

- root object id
- root scalar field
- nested single-link object id
- nested single-link scalar field

## `SQLiteSelectPlan`

This is the most important plan node in the MVP.

Minimum fields:

- root source
- base table alias
- selected value slots
- joins
- optional predicate tree
- order expressions
- optional limit
- optional offset
- zero or more follow-up fetch plans
- result shape plan

## Join Model

The MVP only needs predictable join shapes.

### `SQLiteJoin`

Minimum fields:

- join kind
- source alias
- target table
- target alias
- join condition
- logical reason for the join

Supported join reasons in the MVP:

- selected single `link`
- filter traversal through single `link`
- ordering traversal through single `link`

Recommended join kinds:

- `inner` for required links when the semantics demand presence
- `left` for optional links when preserving parent rows matters

The exact choice may be normalized later, but the plan should record it
explicitly.

## Predicate Model

The SQLite planner needs a backend-specific predicate tree.

### `SQLitePredicate`

Minimum supported forms:

- value expression compared to value expression
- value expression `in` or `not in` a value expression list or nested select
- `is null`
- `is not null`
- boolean `and`
- boolean `or`
- boolean `not`

The planner may simplify some Semantic IR paths:

- `.author.id = <uuid>` may lower directly to `post.author_id = ?`

This optimization belongs in SQLite planning, not Semantic IR.

### `SQLiteValueExpr`

Predicates and later order/projection planning use structured SQLite value
expressions. The plan should keep these expressions as data until
`sqlite-query-sqlgen` serializes them.

Minimum supported forms for the current value-expression milestones:

- column reference
- literal
- arithmetic expression
- unary arithmetic expression
- cast expression
- string function expression

Membership list items use the same value expression structure. The SQLite
planner receives only resolver-accepted list items, so list items are non-null
scalar expressions that do not depend on the current row in this milestone.
The planner should preserve their tree shape and bind order for SQL generation.

Membership right-hand sides distinguish a value-expression list from a nested
`SQLiteSelectPlan`. The planner lowers the resolved select through the existing
select planner rather than embedding SQL text. Its table and join aliases are
allocated in a separate scope, so inner aliases may reuse names from the outer
plan without collision.

SQL generation renders the nested plan as `IN (SELECT ...)` or
`NOT IN (SELECT ...)`. It renders the left value first and then appends nested
select bind values in the order their placeholders appear. Bind values from
later outer predicate nodes follow the nested select binds.

### `SQLiteArithmeticExpr`

Minimum fields:

- left SQLite value expression
- arithmetic operator
- right SQLite value expression

Supported operators:

- `+`
- `-`
- `*`
- `/`
- `%`

The SQLite planner receives only resolver-accepted arithmetic expressions.
It should not perform Gelite type checking again. Its responsibility is to:

- lower resolved paths to column references and joins
- preserve arithmetic expression tree shape
- preserve literal bind order from left to right
- expose enough structure for SQL generation to render parentheses where needed

SQL generation should render arithmetic operands with parentheses whenever
omitting them could change meaning. The generated SQL may be more parenthesized
than a handwritten query.

### `SQLiteUnaryArithmeticExpr`

Minimum fields:

- unary arithmetic operator
- operand SQLite value expression

Supported operators:

- unary `+`
- unary `-`

The SQLite planner receives only resolver-accepted unary arithmetic
expressions. It should preserve the unary expression tree shape and leave SQL
operator spelling and parentheses to SQL generation. SQL generation should
parenthesize unary operands whenever the operand is not already a single column
reference or literal.

### `SQLiteCastExpr`

Minimum fields:

- operand SQLite value expression
- target scalar type

Supported target types in the numeric cast milestone:

- `int64`
- `float64`

The SQLite planner receives only resolver-accepted casts. It should not repeat
Gelite cast type validation. Its responsibility is to lower the cast operand to
a SQLite value expression, preserve the operand tree shape and bind order, and
record the target type for SQL generation.

SQL generation renders numeric casts as SQLite `CAST` expressions:

- `int64` target: `CAST(<expr> AS INTEGER)`
- `float64` target: `CAST(<expr> AS REAL)`

The generated SQL should keep the cast expression structured until rendering.
Do not store raw SQL fragments in the plan.

### `SQLiteStringFunctionExpr`

Minimum fields:

- function kind
- ordered SQLite value expression arguments
- ordered argument scalar types

Supported function kinds in the first string function milestone:

- `concat`
- `str`

The SQLite planner receives only resolver-accepted string functions. It should
not repeat Gelite arity, type, or cardinality validation. Its responsibility is
to lower each argument to a SQLite value expression, preserve argument order and
bind order, and carry enough scalar type information for SQL generation to
render Gelite string conversion semantics.

SQL generation renders `concat` as SQLite concatenation with `||`, using
parentheses to preserve grouping when needed. The query language does not expose
`||` directly.

SQL generation renders `str` according to Gelite conversion semantics:

- `str`, `uuid`, and `datetime` operands use their stored text expression
- `int64` and `float64` operands use `CAST(<expr> AS TEXT)`
- `bool` operands use a `CASE` expression that returns `'true'` for true and
  `'false'` for false

The generated SQL should keep string function expressions structured until
rendering. Do not store raw SQL fragments in the plan.

## Ordering Model

### `SQLiteOrder`

Minimum fields:

- value expression
- direction

The value expression uses the same SQLite value-expression model as predicates.
Supported order values in the arithmetic and string function milestones are
scalar column references, numeric arithmetic expressions, unary numeric
arithmetic expressions, numeric cast expressions, and supported string function
expressions. Ordering expressions should only reference values already
reachable through the planned join tree.

## Select Value Model

The plan should explicitly list which physical values must be fetched.

### `SQLiteSelectValue`

Minimum fields:

- slot id
- value expression
- output SQL alias
- value role

Suggested roles:

- `root_id`
- `root_scalar`
- `nested_object_id`
- `nested_scalar`
- `computed`

Schema-backed selected values usually lower to column references. Computed
selected values lower to SQLite value expressions such as arithmetic trees and
unary arithmetic trees and must not require a logical `schema_model::FieldRef`.
The planner assigns stable SQL aliases for computed values so SQL generation
and result decoding can refer to the same output column.

When a computed value appears inside a nested result shape, its value expression
is lowered from that nested shape's source alias. It must not implicitly start
from the root query alias.

This keeps result shaping deterministic.

## Result Shape Plan

Semantic IR already describes the logical shape. SQLite Plan must describe how
physical rows become that shape.

### `SQLiteResultShapePlan`

Minimum fields:

- root object identity slot
- ordered output fields
- nested single-object shape descriptors
- nested multi-object shape descriptors

### `SQLiteNestedShapePlan`

Minimum fields:

- output name
- cardinality
- optional identity slot
- field slots or computed value slots
- optional source for follow-up fetching

Rules:

- single-link nested shapes may be satisfied by the main query
- multi-link nested shapes may be satisfied by follow-up fetch plans
- computed fields appear in the ordered output field list but do not have
  identity slots or schema field references

## `SQLiteFollowUpFetchPlan`

This plan represents a secondary query used mostly for `multi link` fetching.

Minimum fields:

- parent field reference
- parent identity input slot
- root source for the follow-up query
- join-table access description
- selected value slots
- optional nested result shape plan

The MVP planner may choose this approach whenever joining everything in one
query would complicate shaping or duplicate root rows excessively.

## Mutation Plans

Mutation planning may be simpler than select planning in the MVP, but it still
deserves explicit nodes.

### `SQLiteInsertPlan`

Minimum fields:

- root table
- generated id strategy
- scalar column assignments
- single-link foreign key assignments
- optional returning plan for inserted object identity

Constraints:

- inserts may not assign `id` directly
- inserts may not assign `multi link`

### `SQLiteUpdatePlan`

Minimum fields:

- root table
- assignments
- optional predicate

Assignment forms:

- literal scalar or single-link foreign-key value
- nested `SQLiteSelectPlan` used as a scalar single-link foreign-key value

The planner lowers a resolved `LinkSelect` assignment through the existing
select planner. The nested plan must select the linked object's identity and
is rendered in the destination `<field>_id` value position. Semantic
resolution has already verified the link target, projection, literal type, and
at-most-one cardinality before this layer.

### `SQLiteDeletePlan`

Minimum fields:

- root table
- optional predicate
- predicate joins

- root-only predicates render directly against the target
- joined predicates select target identities as filtered updates do
- filter binds follow expression order

Delete-side cleanup for `multi link` rows is handled by SQLite foreign key
rules, not by extra logical mutation nodes in the MVP.

### `SQLiteMultiLinkMutationPlan`

Minimum fields:

- operation: add or remove
- join-table name
- source identity select plan derived from the outer update target and filter
- target identity select plan derived from the resolved nested select
- `source_id` and `target_id` column names

Add rendering uses one `INSERT ... SELECT` over the Cartesian product of the
source and target identity queries, with a conflict target on the join table's
composite primary key. Remove rendering uses one `DELETE` constrained by the
same two identity sets. The plan does not assign the reserved `position`
column.

## Boundary With SQL Generation

SQLite Plan includes:

- table names
- column names
- table aliases
- join conditions
- planned predicate structure
- follow-up fetch structure
- result slot metadata

SQLite Plan does not include:

- final SQL strings
- placeholder numbering syntax
- driver binding calls
- row decoding implementation

Those belong to the SQL generation and runtime layers.

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

The SQLite Plan should capture at least:

- root source `post`
- selected slots for `post.id` and `post.title`
- a join from `post.author_id` to `user.id`
- selected slots for `user.id` and `user.name`
- a predicate equivalent to `post.author_id = ?`
- ordering by `post.title`
- limit `10`
- result shape metadata saying `author` is a nested single object

It does not yet need to contain:

- the final `SELECT ... FROM ...` SQL string
- concrete parameter binding code

## Recommended MVP Planning Strategy

For the first implementation:

- lower root object access to a single base table
- handle scalar fields as direct column reads
- handle selected single links with explicit joins
- lower filter traversal through single links using either joins or direct
  foreign-key predicates where possible
- fetch `multi link` nested shapes with follow-up queries
- use object `id` as the primary deduplication key during result shaping

This strategy favors clarity over aggressive optimization.
