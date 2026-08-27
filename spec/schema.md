# Schema MVP Spec

## Goal

Define the minimum schema language needed for the first end-to-end milestone.
This spec intentionally excludes advanced features so the parser, validator,
catalog, and SQLite mapping can stabilize early.

## Non-Goals

The MVP schema does not support:

- inheritance or abstract types
- computed properties
- access policies
- custom functions
- union or intersection types
- polymorphic relations
- custom defaults beyond a small built-in set

## Source File Shape

Schema source files use the `.geli` extension during the 0.x series.

The extension is intentionally project-specific. Gel uses `.gel` for schema
source files and `.edgeql` for EdgeQL scripts and migrations, but Gelite is not
claiming Gel compatibility. The `.geli` extension identifies this project's
schema language while the syntax is still allowed to change before 1.0.

A schema file is a sequence of top-level `type` declarations.

Example:

```text
type User {
  required name: str
  email: str
}

type Post {
  required title: str
  body: str
  required link author: User
}
```

## Grammar Sketch

This is a syntax sketch, not a parser generator grammar.

```text
schema          := type_decl*
type_decl       := "type" IDENT "{" field_decl* "}"
field_decl      := property_decl | link_decl
property_decl   := property_mod* "property"? IDENT ":" scalar_type
link_decl       := link_mod* "link" IDENT ":" IDENT ("inverse" IDENT)?
property_mod    := "required" | "unique"
link_mod        := "required" | "multi"
scalar_type     := "str" | "int64" | "float64" | "bool" | "uuid" | "datetime"
```

Notes:

- Scalar fields may omit the `property` keyword.
- Relation fields must use the `link` keyword.
- `multi` is valid only on `link` declarations.
- `unique` is valid only on scalar/property declarations.

## Declarations

### Object Types

- Each `type` introduces one object type.
- Type names must be unique within the schema.
- Type names use PascalCase in examples, but the parser should accept any valid
  identifier form allowed by the lexer.

### Fields

- A field belongs to exactly one object type.
- Field names must be unique within a type.
- A field is either a scalar field or a relation field.
- Scalar fields may be written as `name: str` or `property name: str`.
- Relation fields must be written as `link author: User`.
- A declaration without `link` is never interpreted as a relation field.

## Cardinality Rules

The MVP uses three modifiers:

- `required`
- `multi`
- `unique`

Allowed combinations:

- scalar/property with no modifier: optional single
- scalar/property with `required`: required single
- scalar/property with `unique`: optional single with unique non-null values
- scalar/property with `required unique`: required single with unique values
- `link` with no modifier: optional single
- `required link`: required single
- `multi link`: optional many

Disallowed combinations:

- `multi` on scalar/property declarations
- `unique` on `link` declarations
- `required multi`
- duplicate modifiers

This gives the first version only three cardinalities:

- optional single
- required single
- many

`many` is treated as an unordered collection in the schema model. Ordering is a
query concern, not a schema concern.

`unique` does not change cardinality. It adds a value constraint to scalar
fields. For optional unique scalar fields, Gelite rejects duplicate non-null
values and allows multiple rows where the field is absent. This follows
SQLite's `UNIQUE` behavior for `NULL` values in the MVP.

## Scalar Types

Built-in scalar types for the MVP:

- `str`
- `int64`
- `float64`
- `bool`
- `uuid`
- `datetime`

These map directly to a fixed internal type enum. User-defined scalar types and
enums are explicitly out of scope for the first milestone.

Built-in scalar type names are reserved and may not be reused as object type
names.

`update`, `set`, and `delete` are also reserved because SQLite recognizes them
as keywords and the MVP schema language does not support quoted identifiers.

## Relation Rules

- A relation field must be declared with the `link` keyword.
- A `link` target must be an object type.
- Single relations may be optional or required.
- Multi relations are represented with `multi TargetType`.
- `multi` is supported only for relation fields.
- Inverse links must explicitly name a stored link on their target type.
- Two stored fields remain independent unless one explicitly declares an inverse.

Example:

```text
type User {
  required name: str
}

type Post {
  required link author: User
}
```

The engine should not infer inverse links automatically in the MVP.
Without an `inverse` clause, two link declarations remain independent stored
relations.

## Semantic Validation

The validator must check:

- duplicate type names
- use of reserved scalar type names as object type names
- duplicate field names within a type
- unknown type references
- use of an object type target without `link`
- use of `link` with a scalar type target
- illegal modifier combinations
- use of `multi` on scalar/property declarations
- use of `unique` on link declarations
- explicit declaration of `id`
- use of reserved keywords as identifiers, if the lexer reserves them

The validator does not need to check cyclic relations as an error. Cycles are
allowed in the object graph.

## Catalog Model Requirements

The semantic schema catalog should preserve:

- object type id
- object type name
- field id
- owning type id
- field name
- field kind: scalar or relation
- target scalar type or target object type id
- cardinality: optional, required, multi
- unique flag for scalar fields

The catalog should be deterministic so migrations and diffs are stable.
Catalog ids are signed 64-bit integers. This matches SQLite `INTEGER` storage
for backend metadata and avoids converting deterministic schema ids from an
unsigned range into SQLite's signed integer range.

## Defaults and Constraints

The MVP supports one scalar constraint:

- `unique`

`unique` is valid on scalar/property fields only. It may be combined with
`required`.

Default handling for the first milestone:

- object identity fields are implicit, readonly, and generated by the system
- all other fields default to `null` when optional
- required fields must be supplied on insert
- optional unique fields may be `null` in multiple rows

## Implicit Fields

Every object type has an implicit identity field:

- `id: uuid`

The implicit `id` field is:

- required
- readonly
- globally unique
- not writable in schema source
- not user assignable in MVP inserts

This field is not written in the schema source, but it exists in the semantic
catalog and storage mapping.

## Formatting Expectations

The pretty-printer, once implemented, should normalize:

- one field per line
- a single space after `:`
- modifiers in the order `required` then `multi`

Since `required multi` is illegal, this ordering rule mostly prevents future
ambiguity.

## Canonical Example

```text
type User {
  required name: str
  email: str
}

type Post {
  required title: str
  body: str
  required link author: User
}
```

## Deferred Features

These should be revisited only after the MVP works end to end:

- enums
- custom scalars
- advanced uniqueness constraints, such as composite unique constraints and
  named constraints
- indexes declared in schema
- computed fields
- deletion behavior annotations
- inferred inverse links

## Declared Inverse Links

`multi link employees: Employee inverse department` on `Department` exposes
`Employee.department` in reverse. The named field must be a stored link whose
target is the declaring type. Resolve the name only within the declared target
type; do not infer a field from compatible types. Declaration order does not
matter. Stored single and multi links are both valid sources.

Inverse fields are readonly, unordered, and always `many` (zero or more).
Single or required inverse declarations are rejected. Required forward links
do not guarantee that any object points at a particular target. Inverse chains,
unknown fields, scalar sources, and mismatched target types are rejected by the
catalog. Self-referential stored links are allowed. Multiple explicitly named
inverse views of the same stored field are allowed.

`inverse` is contextual after a link target; it remains usable as an identifier
elsewhere. The catalog preserves the source field name and validates it before
query resolution or metadata reconstruction succeeds.
