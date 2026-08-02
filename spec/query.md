# Query MVP Spec

## Goal

Define a small query language that is expressive enough to prove the pipeline:

- parse text into AST
- resolve names against the schema catalog
- build typed IR
- lower IR to SQLite
- shape nested results

The language is intentionally smaller than Gel and should stay small until the
typed IR and lowering model are stable.

## Supported Top-Level Inputs

The MVP supports four data statements:

- `select`
- `insert`
- `update`
- `delete`

The database-backed interactive REPL also supports three transaction-control
commands:

- `start transaction`
- `commit`
- `rollback`

Only one statement or transaction command is executed per input in the first
version.

## Shared Conventions

- Identifiers refer to schema types or fields.
- String literals use double quotes.
- Numeric literals support unsigned integers and decimal floats.
- Decimal float literals must include digits on both sides of the decimal point,
  such as `0.5` or `10.5`. Shorthand forms such as `.5` and `5.` are not part
  of the MVP grammar.
- A leading `+` or `-` is not part of a numeric literal. Signed values are
  parsed as unary arithmetic operators applied to unsigned numeric literals or
  other value expressions.
- Boolean literals are `true` and `false`.
- `null` is supported only where the schema allows an optional value.
- Parameters are deferred. The first milestone may inline literals only.
- Queries operate on the semantic schema catalog, not raw SDL text.
- Relation fields are the fields declared with `link` in schema.
- Scalar fields are the non-`link` fields declared in schema.

## Select

### Shape

`select` returns objects with explicit shape selection.

Example:

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
```

### Grammar Sketch

```text
select_stmt     := "select" type_ref shape filter_clause? order_clause?
                   limit_clause? offset_clause?
shape           := "{" shape_item* "}"
shape_item       := IDENT ","?
                | IDENT ":" shape ","?
                | IDENT ":=" computed_expr ","?
computed_expr   := additive_expr
filter_clause    := "filter" expr
order_clause     := "order" "by" order_item ("," order_item)*
order_item       := expr ("asc" | "desc")?
limit_clause     := "limit" INT
offset_clause    := "offset" INT

expr             := boolean_or_expr
boolean_or_expr  := boolean_and_expr ("or" boolean_and_expr)*
boolean_and_expr := boolean_not_expr ("and" boolean_not_expr)*
boolean_not_expr := "not" boolean_not_expr | comparison_expr
comparison_expr  := membership_expr (compare_op membership_expr)?
membership_expr  := additive_expr (("in" | "not" "in") in_rhs)?
additive_expr    := multiplicative_expr (("+" | "-") multiplicative_expr)*
multiplicative_expr := unary_expr (("*" | "/" | "%") unary_expr)*
unary_expr       := ("+" | "-") unary_expr | primary_expr
primary_expr     := literal | path | "(" expr ")"
```

### Select Semantics

- The target after `select` must be an object type.
- The shape must list fields explicitly.
- Selecting a relation field requires a nested shape.
- Selecting a scalar field does not allow a nested shape.
- Multi relation fields may be selected only with a nested shape.
- Computed shape items use `alias := expr` and produce query-local result
  fields. They are not schema fields and are not stored.
- Omitted fields are not returned.
- `id` may be selected explicitly even though it is implicit in schema source.
- Backlink traversal and inferred inverse relations are not supported in the
  MVP.

Computed projection expressions are value expressions. The first computed
projection milestone accepts numeric arithmetic expressions and supported
built-in value functions over scalar paths and scalar literals:

```text
select Post {
  title,
  score := .likes * 10 + .view_count
}
```

Computed projection aliases share the same output namespace as schema-backed
shape fields in the same shape. The resolver rejects duplicate output names
inside one shape, including collisions such as `title` and `title := .views`.
Nested shapes have their own output namespace.

Computed projection paths are resolved relative to the object source of the
shape that contains the computed item. In a nested shape, `.score` refers to the
nested object type, not the root object type:

```text
select Post {
  author: {
    boosted_score := .score + 1
  }
}
```

Computed projections do not introduce names that can be referenced by filters,
other computed projections, order clauses, or nested shape items in this
milestone. They are output fields only.

The resolver rejects boolean expressions, membership expressions, `null`, link
values, many-cardinality paths, and literal-only computed projections before
SQLite planning. Unsupported function calls and subqueries remain reserved
until their own issues define resolver rules.

### Filters

Filters use the shared expression grammar. The first implemented expression
surface is intentionally small, but it must be represented as a general
expression tree in the parser, AST, resolver, and Semantic IR so later select
features do not require another filter rewrite.

Supported filter expressions:

- root-relative field paths
- literal values
- arithmetic value expressions
- comparisons
- bracketed-list `in`
- bracketed-list `not in`
- `and`
- `or`
- `not`
- parentheses

Supported comparison operators:

- `=`
- `!=`
- `<`
- `<=`
- `>`
- `>=`

Supported filter example:

```text
select Post {
  id,
  title
}
filter .author.id = "00000000-0000-0000-0000-000000000001"
order by .title asc
limit 20
```

The leading `.` in filter paths refers to the current row root.

Parentheses control boolean grouping:

```text
select Post {
  id,
  title
}
filter (.title = "Hello" or .title = "Draft") and not .archived = true
```

Membership checks use bracketed value expressions:

```text
select Post {
  id,
  title
}
filter .status not in ["archived", "deleted"]
```

The right-hand list may contain row-independent scalar value expressions:

```text
filter .view_count in [1 + 1, 2 * 3]
```

`null` comparisons are supported only with equality operators. `= null` and
`null = .field` match absent optional values. `!= null` and `null != .field`
match present values. Other comparison operators with `null` are rejected by
the resolver before SQLite planning.

Arithmetic expressions may be used as value operands inside filters:

```text
select Post {
  title
}
filter .view_count + 10 >= 100
```

Supported arithmetic operators:

- `+`
- `-`
- `*`
- `/`
- `%`

The same `+` and `-` tokens are also accepted as unary prefix operators. Unary
operators bind tighter than `*`, `/`, and `%`. For example, `-.score * 2`
parses as `(-.score) * 2`, and `-(.score + 1)` preserves the parenthesized
addition as the unary operand.

Arithmetic is numeric only. The resolver accepts same-type numeric operands:

- `int64 op int64`
- `float64 op float64`

`float64` arithmetic may use declared `float64` fields, decimal float literals,
and explicit `f64(expr)` casts. Integer literals are `int64` literals unless an
explicit cast is used.

Mixed numeric operands such as `int64 + float64` are rejected unless one side is
explicitly cast to the other numeric type. String, boolean, uuid, `null`,
object, and link operands are rejected before SQLite planning. `%` is accepted
only for `int64 % int64`.

`int64 / int64` follows SQLite integer division semantics. If fractional
division is required, the query must use an explicit `f64(expr)` cast. Division
by zero is not normalized by Gelite in this milestone; if a runtime operand is
zero, SQLite's result is used.

### Numeric Casts

Numeric casts use the function-call syntax but are resolved as explicit cast
expressions, not as general user-defined functions:

```text
filter f64(.view_count) / 2.0 >= 10.5
filter i64(.score) % 10 = 0
```

Supported cast functions:

- `i64(expr) -> int64`
- `f64(expr) -> float64`

Each cast accepts exactly one scalar numeric value expression. The resolver
accepts `int64` and `float64` sources and rejects string, boolean, uuid,
datetime, `null`, object, link, and many-cardinality operands before SQLite
planning. String-to-number casts are deferred because they would expose
SQLite-specific text coercion behavior.

Numeric casts may appear anywhere a scalar value expression is allowed,
including filter comparisons, membership list items, order expressions, and
computed select projections. Context-specific restrictions still apply:
membership list items must be row-independent, and order expressions must refer
to the current row rather than being literal-only.

The parser may represent any `IDENT "(" ... ")"` form as a function call, but
the resolver only accepts the built-in numeric casts and string functions listed
in this document. Unsupported function names and unsupported arities are
rejected before Semantic IR construction.

### String Functions

String operations use named built-in functions. Gelite does not overload `+`
for string concatenation and does not expose SQLite `||` directly in query
syntax.

Supported string value functions:

- `concat(first, second, ...rest) -> str`
- `str(value) -> str`

`concat` accepts two or more scalar string value expressions. Every argument
must resolve to `str`; numeric, boolean, uuid, datetime, `null`, object, link,
and many-cardinality arguments are rejected before SQLite planning. The result
is `str`. Cardinality is null-propagating: if any argument is optional, the
result is optional; otherwise the result is required.

`str` accepts exactly one scalar value expression. It accepts `str`, `int64`,
`float64`, `bool`, `uuid`, and `datetime` operands and rejects `null`, object,
link, and many-cardinality operands before SQLite planning. `str` is an
explicit conversion; it does not enable implicit casts for comparisons,
membership checks, arithmetic, or concatenation. `str` of an optional value is
optional. `str` of a required value or scalar literal is required.

`str` uses Gelite conversion semantics, not backend-default text coercion.
`str` of a `str`, `uuid`, or `datetime` value returns its canonical stored text
form. `str` of `int64` or `float64` returns the backend-rendered decimal text
for that numeric value. `str` of `bool` returns `"true"` or `"false"`.

String functions may appear anywhere a scalar value expression is allowed,
including filter comparisons, membership list items, order expressions, and
computed select projections. Context-specific restrictions still apply:
membership list items must be row-independent, order expressions must refer to
the current row rather than being literal-only, and computed projections must
depend on the current row.

Deferred string functions include `length`, `lower`, `upper`, `trim`, `ltrim`,
`rtrim`, `replace`, `substr`, `contains`, `starts_with`, and `ends_with`.
These names remain reserved until their resolver rules and backend lowering are
specified.

### Ordering

Order clauses use the value-expression subset of the shared expression grammar.

Supported order values:

- scalar paths
- numeric arithmetic expressions over scalar paths, numeric literals, and
  numeric casts
- supported string functions that refer to the current row

Examples:

```text
order by .title asc
order by .view_count + 1 desc
order by (.view_count + 1) * 10 asc
order by f64(.view_count) / 2.0 asc
order by concat(.title, " draft") asc
```

Order expressions must resolve to scalar values. Boolean expressions such as
comparisons, `and`, `or`, `not`, and membership expressions are rejected before
SQLite planning. Literal-only order values such as `order by 1` are also
rejected in this milestone because they do not refer to data from the current
row.

Arithmetic and casts in order clauses follow the same numeric rules as
arithmetic and casts in filters: arithmetic operands must be same-type numeric
values, `%` is accepted only for `int64 % int64`, and division semantics are
delegated to SQLite.

### Expression Grammar

The expression grammar is shared by filters and later value positions such as
mutation assignments and computed shape fields.

```text
expr              := or_expr
or_expr           := and_expr ("or" and_expr)*
and_expr          := not_expr ("and" not_expr)*
not_expr          := "not" not_expr
                  | compare_expr
compare_expr      := in_expr
                  | additive_expr compare_op additive_expr
                  | additive_expr
compare_op        := "=" | "!=" | "<" | "<=" | ">" | ">="
in_expr           := additive_expr in_op in_rhs
in_op             := "in"
                  | "not" "in"
additive_expr     := multiplicative_expr (("+" | "-") multiplicative_expr)*
multiplicative_expr
                  := unary_expr (("*" | "/" | "%") unary_expr)*
unary_expr        := ("+" | "-") unary_expr
                  | primary_expr
primary_expr      := literal
                  | path
                  | "(" expr ")"
                  | function_call
                  | subquery_expr
path              := "."? IDENT ("." IDENT)*
function_call     := IDENT "(" argument_list? ")"
argument_list     := expr ("," expr)*
in_rhs            := "[" expr_list? "]"
                  | "(" select_stmt ")"
expr_list         := expr ("," expr)*
subquery_expr     := "(" select_stmt ")"
```

Only path, literal, arithmetic, supported built-in function calls, comparison,
bracketed-list `in`, bracketed-list `not in`, boolean, and parenthesized
expressions are accepted by the resolver in the current expression milestones.
Context determines which subset is valid: filters accept boolean expressions,
ordering accepts scalar order values, and computed projection accepts value
expressions that refer to the current row. `function_call` is currently
accepted only for built-in numeric casts and supported string functions. Other
function names remain reserved. `subquery_expr` is also reserved until subquery
expression scope is defined.

The first accepted `in_rhs` form is a non-empty bracketed list. The parser may
accept `null` as a list item because it is a literal expression, but the
resolver must reject `in []`, `not in []`, `null` list items, and list items
that are not scalar value expressions. List items must be row-independent in
this milestone: literals and arithmetic expressions over literals are accepted,
but paths, link traversals, subqueries, and boolean predicates inside the list
are rejected before SQLite planning. Use an explicit null comparison when a
filter should match null and non-null values together:

```text
filter .deleted_at = null or .deleted_at in ["2323"]
```

Subquery RHS forms such as `.author.id in (select User { id })` are reserved by
the grammar but rejected until subquery expression scope is defined.

Precedence from strongest to weakest:

1. primary expressions
2. unary `+`, `-`
3. `*`, `/`, `%`
4. `+`, `-`
5. membership and comparisons
6. `not`
7. `and`
8. `or`

### Filter Expression Scope

The MVP supports:

- field paths from the root object
- traversal through declared single relation fields
- scalar comparisons against literals
- numeric arithmetic expressions used as comparison or membership operands
- unary numeric arithmetic expressions
- explicit numeric casts with `i64(expr)` and `f64(expr)`
- string functions with `concat(...)` and `str(expr)`
- scalar membership checks against non-empty lists of non-null scalar value
  expressions
- boolean composition
- parenthesized grouping

The MVP does not support:

- traversal through backlinks or inferred inverse relations
- arbitrary subqueries
- aggregation
- `exists`
- subquery `in`
- arbitrary function calls other than supported built-ins
- implicit numeric casts
- implicit string casts
- path scoping with aliases

## Insert

### Shape

`insert` creates one object of a target type.

Example:

```text
insert User {
  name := "Sheri",
  email := "sheri@example.com"
}
```

### Grammar Sketch

```text
insert_stmt     := "insert" type_ref object_literal
object_literal  := "{" assign_item* "}"
assign_item     := IDENT ":=" literal ","?
```

### Insert Semantics

- The target must be an object type.
- Assignments may target declared scalar fields and declared single relation
  fields only.
- A field may appear at most once in an insert assignment list.
- Required scalar fields must be supplied unless a built-in default exists.
- Required single relation fields must be supplied unless a built-in default
  exists.
- Optional scalar fields may be omitted.
- Optional single relation fields may be omitted.
- Assigning `id` is not allowed.
- Scalar assignment literals must match the declared scalar type. `null` is
  accepted only for optional scalar fields.
- A single relation assignment accepts a string literal as a temporary MVP
  object-id shorthand. It does not accept a scalar value of another kind.
- `null` is accepted only for optional single relation fields.
- Multi relation inserts are deferred from the first execution milestone.
- Relation assignments must target declared `link` fields.

The temporary string-literal relation shorthand is an execution-scope
convenience, not a general object-expression feature. Nested inserts,
subqueries, and object-valued assignment expressions remain unsupported until
a later mutation design replaces or extends this syntax.

Allowed single relation insert example:

```text
insert Post {
  title := "Case File",
  author := "00000000-0000-0000-0000-000000000001"
}
```

This assignment should resolve to the related object's identity. The assigned
field must be a declared single `link`.

## Update

### Shape

`update` modifies zero or more objects selected by a filter.

Example:

```text
update Post
filter .id = "00000000-0000-0000-0000-000000000010"
set {
  title := "Updated Title"
}
```

### Grammar Sketch

```text
update_stmt     := "update" type_ref filter_clause? set_clause
set_clause      := "set" object_literal
```

### Update Semantics

- The target must be an object type.
- `filter` is optional, but omitting it updates every row. The CLI and server
  should consider adding safety confirmation later.
- Only scalar fields and single relations may be updated in the MVP.
- Multi relations may not be updated in the MVP.
- Relation assignments must target declared `link` fields.
- Updating `id` is not allowed.

## Delete

### Shape

`delete` removes zero or more objects selected by a filter.

Example:

```text
delete Post
filter .id = "00000000-0000-0000-0000-000000000010"
```

### Grammar Sketch

```text
delete_stmt     := "delete" type_ref filter_clause?
```

### Delete Semantics

- The target must be an object type.
- `filter` uses the `select` and `update` expression surface; omitting it deletes
  every row.
- Relation cleanup behavior is defined by the storage model, not query syntax.
- The MVP omits `order by`, `offset`, `limit`, and deleted-object results;
  CLI/REPL execution reports `affected_rows`.

## Transaction Control

### Grammar

```text
transaction_command := "start" "transaction"
                     | "commit"
                     | "rollback"
```

Transaction commands are session-level syntax. They are represented in the
syntax tree and dispatched directly by the REPL; they do not enter Semantic IR,
SQLite query planning, or SQL generation.

### Session Semantics

- Transaction commands are supported only by a database-backed interactive
  REPL, where every input uses the same native SQLite connection.
- `start transaction` begins an explicit SQLite transaction on that connection.
- `commit` persists successful changes made since `start transaction`.
- `rollback` discards changes made since `start transaction`.
- Data statements outside an explicit transaction retain SQLite autocommit
  behavior.
- Starting a transaction while one is active, or committing or rolling back
  without an active transaction, returns an execution error.
- Closing the REPL or database connection rolls back an active transaction.
- Compile-only REPL input and one-shot CLI input reject transaction commands.

The MVP does not support transaction modes, savepoints, nested transactions,
automatic retries, client callback APIs, multi-command scripts, transactions
across CLI process invocations, WASM transaction execution, or schema-apply
transaction changes.

## Values

Supported literal values:

- strings
- integers
- floats
- booleans
- `null`

The first version does not support:

- array literals
- nested object literals in expressions
- computed expressions in assignment
- function calls
- `in`
- subqueries

## Error Conditions

The analyzer should report:

- unknown type names
- unknown field names
- writes to undeclared fields
- scalar field used with nested shape
- relation field selected without nested shape
- relation assignment to a non-`link` field
- `id` assignment in insert or update
- type mismatches in assignment
- type mismatches in filter comparisons
- unsupported expression forms
- invalid cardinality usage
- use of backlink or inferred inverse traversal

## Canonical MVP Examples

### Select

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
filter .author.name = "Sheri"
limit 10
```

### Insert

```text
insert User {
  name := "Sheri"
}
```

### Update

```text
update User
filter .name = "Sheri"
set {
  email := "assistant@example.com"
}
```

### Delete

```text
delete User
filter .name = "Sheri"
```

## Deferred Features

These are intentionally out of scope until the end-to-end path is stable:

- aliases
- `with` bindings
- nested inserts
- multi relation mutation syntax
- aggregation
- grouping
- pagination cursors
- arbitrary function calls beyond supported built-ins
- subqueries
- query parameters
- upsert
