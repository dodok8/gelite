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
- inferred inverse traversal
- schema-level computed fields
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
- `ResolvedShapeItem`
- `ResolvedSchemaField`
- `ResolvedComputedField`
- `ResolvedPath`
- `PathStep`
- `FieldRef`
- `ObjectTypeRef`
- `ScalarTypeRef`
- `Expr`
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
- ordered assignments

### `UpdateQuery`

Minimum fields:

- root object type
- optional filter expression
- assignments

### `DeleteQuery`

Minimum fields:

- resolved root object type
- optional resolved filter; absence selects every object of the target type

## Shape Model

Nested result shaping is a primary responsibility of Semantic IR.

### `ResolvedShape`

Minimum fields:

- source object type
- ordered list of shape items

Shape items preserve the user-visible output order. A shape item is either a
schema-backed field selection or a computed result field.

### `ResolvedSchemaField`

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

### `ResolvedComputedField`

Minimum fields:

- output name
- value expression
- result scalar type
- result cardinality

Computed fields are query-local output fields. They must not carry a
`schema_model::FieldRef` because no schema field owns the value. The resolver
is responsible for assigning the output name, validating the expression, and
storing enough type and cardinality metadata for SQLite planning and result
decoding.

The value expression is resolved against the `ResolvedShape` source object that
contains the computed field. A computed field inside a nested link shape uses
the nested shape source object, not the root select object.

Computed projections accept direct scalar paths, scalar numeric arithmetic
expressions, and supported built-in value functions over scalar paths and
scalar literals. A direct path may traverse declared single links and must end
at a scalar field. The computed field uses the terminal scalar type and the
cardinality derived from the complete path. Boolean expressions, membership
expressions, link values, many-cardinality paths, `null`, unsupported function
calls, and subqueries are rejected before SQLite planning. Literal-only
computed projections are also rejected in this milestone because they do not
depend on the current row.

Output names must be unique within one `ResolvedShape`. This rule applies
across schema-backed fields and computed fields. Nested shapes have independent
output namespaces.

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

The Semantic IR path model supports:

- root field access
- traversal through declared single links
- stored and declared inverse multi links within existential filter comparisons
- terminal scalar access

The Semantic IR MVP does not support:

- inferred inverse relations
- alias scope traversal
- arbitrary subquery paths

## Expression Model

The Semantic IR expression model is a resolved expression tree. Query parsing
may produce syntax nodes for future expression forms, but the resolver should
only emit IR expression variants that are semantically accepted for the current
milestone.

Each resolved expression should carry enough information for later stages to
validate and lower it without re-reading schema names from source text.

### `Expr`

Minimum variants:

- literal
- path
- arithmetic
- unary arithmetic
- cast
- string function
- comparison
- membership
- boolean `and`
- boolean `or`
- boolean `not`

Minimum metadata:

- result type
- result cardinality

The result type may be a scalar type, an object type for future subquery work,
or a dedicated boolean type for predicates. The current implementation needs
literal scalar values, resolved scalar paths, arithmetic scalar values, unary
arithmetic scalar values, numeric cast values, string function values, and
boolean predicate results.

The expression tree does not store SQLite SQL fragments. SQLite-specific
operator spelling, parentheses, bind placeholders, and joins belong to SQLite
planning and SQL generation.

### `LiteralExpr`

Supported literal kinds:

- string
- integer
- float
- boolean
- null

Minimum fields:

- literal kind
- literal value
- result type
- result cardinality

### `PathExpr`

Wraps a resolved path used in a filter or ordering context.

Minimum fields:

- resolved path
- result type
- result cardinality

### `ArithmeticExpr`

Represents a resolved binary numeric value expression.

Minimum fields:

- left expression
- arithmetic operator
- right expression
- result type
- result cardinality

Supported operators:

- `+`
- `-`
- `*`
- `/`
- `%`

Arithmetic operands must resolve to scalar numeric value expressions. The
resolver must reject string, boolean, uuid, `null`, object, and link operands
before SQLite planning.

Accepted operand and result types:

- `int64 + int64 -> int64`
- `int64 - int64 -> int64`
- `int64 * int64 -> int64`
- `int64 / int64 -> int64`
- `int64 % int64 -> int64`
- `float64 + float64 -> float64`
- `float64 - float64 -> float64`
- `float64 * float64 -> float64`
- `float64 / float64 -> float64`

Mixed numeric operands such as `int64 + float64` are rejected until explicit
cast expressions exist. `%` is not defined for `float64`.

`int64 / int64` preserves SQLite integer division semantics. Division by zero
is not rewritten by Semantic IR. If the divisor can only be known at runtime,
SQLite determines the result. Because SQLite can return `NULL` for division or
modulo by zero, computed projection metadata treats `/` and `%` as optional
unless the divisor is a non-zero numeric literal.

Arithmetic expressions may appear as value operands inside filter comparisons,
membership expressions, order expressions, and computed select projections.

### `UnaryArithmeticExpr`

Represents a resolved unary numeric value expression.

Minimum fields:

- unary arithmetic operator
- operand expression
- result type
- result cardinality

Supported operators:

- unary `+`
- unary `-`

The operand must resolve to a scalar numeric value expression. Unary arithmetic
preserves the operand scalar type: `int64` stays `int64`, and `float64` stays
`float64`. String, boolean, uuid, `null`, object, link, and many-cardinality
operands are rejected before SQLite planning.

Unary arithmetic has the same cardinality as its operand. It does not introduce
additional `NULL` results by itself.

### `CastExpr`

Represents a resolved explicit scalar cast.

Minimum fields:

- operand expression
- target scalar type
- result cardinality

Supported target types in the numeric cast milestone:

- `int64`
- `float64`

Supported source and target combinations:

- `int64 -> int64`
- `int64 -> float64`
- `float64 -> int64`
- `float64 -> float64`

The source syntax for these casts is the built-in function-call form
`i64(expr)` or `f64(expr)`, but Semantic IR stores the resolved meaning as a
cast expression rather than as an opaque function call. The resolver must
reject unsupported function names, unsupported arities, non-numeric source
types, `null`, object values, link values, and many-cardinality operands before
IR construction.

Cast cardinality follows the operand cardinality. A cast of an optional scalar
is optional; a cast of a required scalar or literal is required. Casts do not
introduce additional `NULL` results by themselves. SQLite runtime conversion
details are backend-specific lowering behavior and must not be encoded as SQL
fragments in Semantic IR.

### `StringFunctionExpr`

Represents a resolved built-in string value function.

Minimum fields:

- function kind
- ordered argument expressions
- ordered argument scalar types
- result cardinality

Supported function kinds in the first string function milestone:

- `concat`
- `str`

`concat` accepts two or more scalar `str` value expressions and returns `str`.
The resolver must reject non-string arguments, `null`, object values, link
values, and many-cardinality operands before IR construction. `concat`
cardinality is null-propagating: if any argument is optional, the result is
optional; otherwise the result is required.

`str` accepts exactly one scalar value expression and returns `str`. The
accepted source scalar types are `str`, `int64`, `float64`, `bool`, `uuid`, and
`datetime`. The resolver must reject `null`, object values, link values, and
many-cardinality operands before IR construction. `str` cardinality follows the
operand cardinality.

The source syntax for these operations is function-call syntax, but Semantic IR
stores the resolved operation as a string function expression rather than an
opaque function call or SQL fragment. Backend-specific text conversion rules,
including SQLite `CAST` or boolean rendering choices, belong to backend
planning and SQL generation.

### `CompareExpr`

Minimum fields:

- left expression
- comparison operator
- right expression

Supported operators:

- `=`
- `!=`
- `<`
- `<=`
- `>`
- `>=`

Comparison expressions must resolve to a boolean result. The resolver is
responsible for rejecting incompatible operands before the expression reaches
SQLite planning.

`= null` and `null = <path>` lower to `IsNull`. `!= null` and `null != <path>`
lower to `IsNotNull`. Other comparison operators with `null` are rejected by
the resolver because SQL three-valued null comparison semantics are not the
Gelite filter contract.

### `InExpr`

Minimum fields:

- left expression
- membership operator: `in` or `not in`
- right-hand side, represented explicitly as either a value list or resolved
  select query

Supported right-hand side:

- a non-empty list of non-null scalar value expressions
- a resolved `SelectQuery` that projects exactly one schema-backed required
  scalar field compatible with the left expression

Membership expressions must resolve to a boolean result. The resolver is
responsible for rejecting empty lists, incompatible operand types, `null` list
items, non-scalar list items, and invalid select projections before the
expression reaches SQLite planning.

Right-hand list items must be row-independent in the arithmetic filter
milestone. Literals and arithmetic expressions over literals are accepted.
Path expressions, link traversals, subqueries, boolean predicates, and any
expression that depends on the current row are rejected as list items.

The resolved select right-hand side retains its root object type, shape,
filter, ordering, limit, and offset as Semantic IR. Its shape must contain one
`ResolvedShapeField` for a scalar field with required cardinality. Computed
fields, link fields, nested shapes, optional fields, and multiple shape items
are invalid. The nested select may return zero, one, or many rows.

Nested paths resolve only against the nested select root. The Semantic IR does
not carry an outer-scope reference into the nested select. The nested query
remains structured Semantic IR; it must not contain backend SQL text.

The Semantic IR should model `not in` explicitly instead of rewriting it to a
boolean `not` around `in`. Keeping the operator in the membership node lets
SQLite planning preserve bind order and choose a direct `NOT IN` predicate.

### Boolean Expressions

The MVP also needs:

- `AndExpr`
- `OrExpr`
- `NotExpr`

Boolean expression operands must resolve to boolean expressions. Parentheses do
not need a dedicated IR node; they affect the parsed tree shape. SQL generation
must preserve grouping when rendering `and` and `or` combinations.

### Reserved Expression Forms

The parser and AST may reserve syntax for these forms before they become
accepted Semantic IR:

- `FunctionCallExpr`
- `SubqueryExpr`

The resolver may lower accepted built-in function calls to specific Semantic IR
nodes, such as `CastExpr` for `i64(expr)` and `f64(expr)` or
`StringFunctionExpr` for `concat(...)` and `str(expr)`. It must reject
unsupported forms with diagnostics. Do not pass an unsupported expression
through IR as an opaque node.

## Ordering Model

### `OrderExpr`

Minimum fields:

- value expression
- direction: `asc` or `desc`

The order value must resolve to a scalar `ValueExpr`. Supported order values
are resolved scalar paths, numeric arithmetic expressions over scalar paths and
numeric literals, unary arithmetic, numeric casts, and string functions that
refer to the current row.
Boolean expressions, membership expressions, and literal-only order values are
rejected by the resolver before SQLite planning.

## Mutation Model

### `Assignment`

Minimum fields:

- field reference
- assignment operation
- mutation value

`Assignment` is shared by `InsertQuery` and future `UpdateQuery` values. Its
field reference is resolved against the target object type; it must not retain
the parsed field name as the semantic source of truth.

Assignment operations are:

- `assign` for scalar and single-link `:=`
- `add` for multi-link `+=`
- `remove` for multi-link `-=`

Insert accepts only `assign`. An update containing `add` or `remove` contains
exactly one assignment in the first multi-link mutation milestone.

### `MutationValue`

Mutation values preserve the semantic distinction between scalar values,
temporary object-ID shorthand, and resolved single-link select assignments:

- scalar literal, including `null` where the resolved field is optional
- `LinkId` string for the temporary single-link object-id shorthand
- `LinkSelect` containing a resolved `SelectQuery` that returns the linked
  object's identity
- `MultiLinkSelect` containing a resolved `SelectQuery` that returns zero or
  more target identities for an `add` or `remove` operation

`LinkId` deliberately records the relation shorthand separately from a scalar
string literal. The resolver chooses this variant only for a declared single
`link` field. `LinkSelect` is also specific to a declared single `link`; its
payload contains resolved schema references and expressions rather than raw
AST names.

Before constructing `LinkSelect`, the resolver must verify that the nested
select root matches the link target, its shape contains exactly the target's
implicit `id`, and its result cardinality is at most one. The first accepted
cardinality proof is equality between implicit `id` or a declared `unique`
scalar field and a compatible non-null scalar literal. `limit 1` alone is not
a semantic uniqueness proof.

Before constructing `MultiLinkSelect`, the resolver must verify that the
assignment field is a declared multi link, the nested select root matches the
link target, and its shape contains exactly the target's implicit `id`.
Unlike `LinkSelect`, the nested select does not require an at-most-one proof.

MVP constraints:

- assignments may target declared scalar fields
- assignments may target declared single `link` fields
- assignments may not target implicit `id`
- `assign` may not target `multi` links
- `add` and `remove` may target only `multi` links and require
  `MultiLinkSelect`
- scalar values must match the resolved scalar type
- `null` requires an optional resolved field
- a single-link value is `LinkId`, `LinkSelect`, or, for an optional link,
  `null`

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

## Declared Inverse Link Contract

Resolved shape links and path steps carry the stored field reference, its
forward cardinality, and the traversal direction. The selected field reference
still identifies the declared output field. SQLite names and access strategy
remain outside Semantic IR. Inverse steps have `Many` cardinality.

Multi-path filter comparisons explicitly represent existential evaluation of
one atomic comparison or null predicate. Boolean composition and negation stay
outside that scope. Paths retain their full cardinality; they must not be
misrepresented as single values to enable filtering.

`LinkTraversal` records ownership and direction for both `ResolvedShapeField`
and `ResolvedPathStep`. `Expr::Exists(ExistsComparison)` contains one resolved
many-valued path, comparison operator, literal, and operand order. Null literals
use only equality or inequality; the single-valued suffix after the last many
step must be optional. This distinguishes a missing value within a related
object from a relation with no matching objects.
