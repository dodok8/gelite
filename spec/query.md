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

## Supported Statements

The MVP supports exactly four top-level statements:

- `select`
- `insert`
- `update`
- `delete`

Only one statement is executed per request in the first version.

## Shared Conventions

- Identifiers refer to schema types or fields.
- String literals use double quotes.
- Numeric literals support integers and decimal floats.
- Boolean literals are `true` and `false`.
- `null` is supported only where the schema allows an optional value.
- Parameters are deferred. The first milestone may inline literals only.

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
filter_clause    := "filter" expr
order_clause     := "order" "by" order_item ("," order_item)*
order_item       := path ("asc" | "desc")?
limit_clause     := "limit" INT
offset_clause    := "offset" INT
path             := IDENT ("." IDENT)*
```

### Select Semantics

- The target after `select` must be an object type.
- The shape must list fields explicitly.
- Selecting a relation field requires a nested shape.
- Selecting a scalar field does not allow a nested shape.
- Omitted fields are not returned.
- `id` may be selected explicitly even though it is implicit in schema source.

### Filters

Supported filter operators:

- `=`
- `!=`
- `>`
- `>=`
- `<`
- `<=`
- `and`
- `or`
- `not`

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

### Filter Expression Scope

The MVP supports:

- field paths from the root object
- scalar comparisons against literals
- boolean composition

The MVP does not support:

- arbitrary subqueries
- aggregation
- `exists`
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
assign_item     := IDENT ":=" value_expr ","?
```

### Insert Semantics

- The target must be an object type.
- Required scalar fields must be supplied unless a built-in default exists.
- Optional scalar fields may be omitted.
- Single relation fields may be assigned by target object id only in the MVP.
- Multi relation inserts are deferred from the first execution milestone.

Allowed single relation insert example:

```text
insert Post {
  title := "Case File",
  author := "00000000-0000-0000-0000-000000000001"
}
```

This assignment should resolve to the related object's identity.

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
- `filter` is optional, but omitting it deletes every row.
- Relation cleanup behavior is defined by the storage model, not query syntax.

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
- function calls
- computed expressions in assignment

## Error Conditions

The analyzer should report:

- unknown type names
- unknown field names
- scalar field used with nested shape
- relation field selected without nested shape
- type mismatches in assignment
- type mismatches in filter comparisons
- invalid cardinality usage

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
- function calls
- query parameters
- upsert
