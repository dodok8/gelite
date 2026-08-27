# Query Expression Model Plan

## Scoped Relationship Predicate Follow-Up (#68)

The original sequencing below predates implemented mutation and multi-path
filters. Explicit same-child predicates now extend the shared filter pipeline:

1. Define `exists .relation { predicate }` and its scope in the query spec.
2. Add explicit AST and Semantic IR nodes; resolve the body from one target.
3. Reuse correlated SQLite existence plans and parameterize predicate source
   aliases so the complete body refers to that target.
4. Test independent versus same-child conditions, null and negation, stored and
   inverse relations, pagination, and select/update/delete behavior.

Nested scopes, additional multi traversal inside the body, body subqueries,
outer references, and returned-child filtering are excluded from this step.

## Goal

Extend the query pipeline from a path-and-literal filter model to a small
expression model that can later support `in`, aggregate functions, and
subqueries without rewriting the parser and IR again.

This plan belongs before insert, update, delete, and transaction syntax work
because those statements will reuse the same expression model for filters,
assignments, and later returned values.

## Current State

The current query implementation supports `select` with explicit shapes,
filters, ordering, limit, and offset. The filter and ordering paths are enough
for the first SQLite select pipeline, but they are not a stable base for:

- `in`
- computed shape fields
- function calls such as `count()` and `avg(.field)`
- subqueries used by filters
- mutation filters shared by `update` and `delete`

The existing specs already mention boolean composition and comparison
operators, but the implementation should first make expression structure
explicit before adding more syntax.

## Non-Goals

This task does not implement full select expansion in one step.

The first pass should not add:

- arbitrary subqueries
- aggregation lowering
- grouping
- computed shape output
- mutation statements
- transaction syntax
- query parameters
- alias scope or `with` bindings

Those features need the expression model, but they should remain separate
issues.

## Proposed Scope

### Specs

Update `spec/query.md` to define the expression grammar used by filters and
future value positions.

The first expression grammar should cover:

- path expressions from the current root
- literal expressions
- comparison expressions
- boolean `and`, `or`, and `not`
- parenthesized expressions
- a reserved syntax position for function calls
- a reserved syntax position for `in`
- a reserved syntax position for subqueries

Update `spec/ir.md` so expression nodes are described as one expression tree
rather than as isolated comparison and path nodes.

### AST

Refactor `query-ast` so filter expressions are represented by a general
expression enum.

The AST should be able to represent syntax that the resolver may still reject.
For example, function calls can be parsed into the AST before the resolver
accepts specific functions.

### Parser

Refactor `query-parser` around expression precedence.

The first parser milestone should support existing valid queries plus
parenthesized boolean expressions:

```text
filter (.title = "Hello" or .title = "Draft") and not .archived = true
```

The grammar should make it possible to add `in`, function calls, and subqueries
without changing the shape of the AST again.

### Resolver

Refactor `query-resolver` to resolve AST expressions into IR expressions.

The resolver should continue enforcing current MVP constraints:

- paths must start from the root object
- link traversal is allowed only through declared single links
- terminal filter paths must resolve to scalar fields
- comparison operands must have compatible types
- unsupported expression forms produce diagnostics instead of reaching SQLite
  planning

### IR

Refactor `query-ir` so filter expressions use a general expression enum.

The first IR expression set should include:

- literal
- resolved path
- comparison
- boolean and/or/not

Do not add backend-specific function or subquery lowering details to IR in this
task.

### SQLite Planning And SQL Generation

Update `sqlite-query-plan` and `sqlite-query-sqlgen` only enough to preserve
current behavior through the new expression representation.

This task should not add new SQL capabilities except boolean parentheses if
needed to preserve expression meaning.

## Suggested Implementation Sequence

1. Update `spec/query.md` and `spec/ir.md` with the expression boundary.
2. Refactor `query-ast` filter nodes into a general expression enum.
3. Add parser tests for boolean precedence and parentheses.
4. Refactor parser expression parsing without changing existing query output.
5. Refactor `query-ir` expression nodes.
6. Refactor resolver expression lowering and diagnostics.
7. Update SQLite planning and SQL generation for the new IR shape.
8. Run `cargo test --workspace`.

## Acceptance Criteria

- Existing select queries still parse, resolve, plan, and render SQL.
- Parser tests cover `not`, `and`, `or`, and parentheses precedence.
- Resolver tests cover valid boolean composition and at least one unsupported
  expression diagnostic.
- SQLite SQL generation preserves expression grouping where it affects meaning.
- `cargo test --workspace` passes.

## Follow-Up Issues

After this task lands, the next focused issues should be:

1. Add literal-list `in`.
2. Add `count()` as the first aggregate function.
3. Add `avg()` with numeric type checking.
4. Add subquery expressions, starting with `in (select ...)`.
5. Add mutation statements that reuse the expression model.
6. Add transaction syntax after statement execution semantics are stable.
