# Schema Parser Implementation Plan

## Purpose

The `schema-parser` crate turns `.geli` schema source text into
`schema_model::SchemaCatalog` values:

```text
.geli text -> schema-parser -> schema_model::SchemaCatalog -> sqlite-schema-plan
```

The first parser milestone should not introduce a public or crate-level schema
AST. The parser should build `schema_model::ObjectType`, `schema_model::Field`, and
`schema_model::SchemaCatalog` directly. This keeps the frontend small while the
schema model is still the single semantic representation used by resolver,
SQLite planning, and schema application.

## Relationship To Existing Documents

- `spec/schema.md` defines the `.geli` schema surface syntax and semantic
  rules.
- `plan/sqlite-schema-plan-implementation-plan.md` describes how a validated
  `schema_model::SchemaCatalog` becomes SQLite table, index, and metadata insert
  plans.
- This document describes only the source-text frontend that creates the
  catalog consumed by those later stages.

When this document conflicts with `spec/schema.md`, the spec wins. If the
current implementation cannot yet represent a spec feature, add a focused test
or update the plan before widening the parser.

## Scope

Parse first:

- top-level `type` declarations
- empty object types
- scalar fields without the optional `property` keyword
- scalar fields with the optional `property` keyword
- `required` scalar fields
- `unique` scalar fields
- `required unique` scalar fields
- optional single links
- required single links
- multi links
- multiple object types in one file
- `#` line comments, ignored when building the logical catalog

Defer:

- block comments
- string literals
- default values
- schema modules or namespaces
- migrations
- computed fields
- access policies
- custom scalar declarations
- enums
- error recovery after the first syntax error
- preserving source formatting for a formatter

## Public API Boundary

The public API should accept source text and return the semantic catalog:

```rust
pub fn parse_schema(input: &str) -> Result<schema_model::SchemaCatalog, ParseError>
```

Internally, parsing should still be token-based:

```rust
pub fn parse_schema(input: &str) -> Result<schema_model::SchemaCatalog, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_schema_tokens(&tokens)
}

fn parse_schema_tokens(tokens: &[Token]) -> Result<schema_model::SchemaCatalog, ParseError> {
    Parser::new(tokens).parse_schema()
}
```

`parse_schema_tokens` should remain private. Exposing token-level parsing would
make lexer tokens part of the public compatibility surface before the syntax is
stable.

## No Intermediate AST

The first implementation should not create a separate public `SchemaAst`,
`ParsedSchema`, or `ParsedType` layer.

Instead:

- `parse_schema` collects `schema_model::ObjectType` values.
- `parse_type_decl` returns `schema_model::ObjectType`.
- `parse_field_decl` returns `schema_model::Field`.
- scalar field parsing returns `schema_model::Field::Scalar`.
- link field parsing returns `schema_model::Field::Link`.
- the final step calls `schema_model::SchemaCatalog::try_new(object_types)`.

This means semantic validation stays in `schema`, not in the parser. The parser
is responsible for syntax and local modifier shape. The catalog remains
responsible for duplicate type names, duplicate field names, unknown link
targets, reserved type names, and other catalog-level invariants already tested
in `schema`.

Local modifier validation still belongs in the parser because it affects syntax
shape before a field can be constructed:

- duplicate modifiers
- `required multi`
- `multi` on scalar/property fields
- `unique` on link fields

The parser should convert these cases into `ParseError` values with source
spans. Do not construct an invalid `schema_model::Field` and rely on a later panic.

## Lexer Strategy

`schema-parser` should own its lexer.

The query parser already has a lexer, but sharing it now would force query and
schema syntax to evolve together. That coupling is not useful while `.geli`
and `.geliql` are still allowed to change during the 0.x series.

The schema lexer can copy the query parser's structure and implementation
patterns:

- `logos` internally
- project-owned public token types
- byte/line/column spans
- no `logos` types in public API
- tests split from implementation files

The schema lexer should start smaller than the query lexer. It only needs:

- identifiers
- `{`
- `}`
- `:`
- keywords used by schema syntax
- end-of-file handling through parser cursor state

Required keywords for the first milestone:

- `type`
- `property`
- `link`
- `required`
- `multi`
- `unique`
- scalar type words: `str`, `int64`, `float64`, `bool`, `uuid`, `datetime`

Scalar type words may be represented either as keywords or as identifiers that
`parse_scalar_type` recognizes. Prefer keywords if doing so produces clearer
syntax errors and keeps reserved-word handling consistent with `schema`.

## Parser Strategy

Use recursive descent. The schema grammar is declaration-oriented and does not
need Pratt parsing.

Initial parser shape:

```text
Parser
  parse_schema
  parse_type_decl
  parse_field_decl
  parse_modifier_set
  parse_scalar_field
  parse_link_field
  parse_scalar_type
```

The top-level loop should parse type declarations until token exhaustion.

`parse_type_decl` should:

1. expect `type`
2. read the object type identifier
3. expect `{`
4. parse field declarations until `}`
5. expect `}`
6. return `schema_model::ObjectType::new(name, fields)`

`parse_field_decl` should:

1. collect modifiers before the declaration head
2. decide whether the field is a link or scalar/property
3. validate modifier compatibility for that field kind
4. construct `schema_model::Field`

The parser should accept both scalar forms:

```text
required name: str
required property name: str
```

The parser should require the `link` keyword for relation fields:

```text
required link author: User
multi link posts: Post
```

Do not infer a relation field from `author: User`. In the MVP, declarations
without `link` are scalar/property declarations only.

## Cardinality and Uniqueness Mapping

Scalar/property declarations:

```text
name: str                  -> Optional, NotUnique
required name: str         -> Required, NotUnique
unique email: str          -> Optional, Unique
required unique email: str -> Required, Unique
```

Link declarations:

```text
link author: User          -> Optional
required link author: User -> Required
multi link posts: Post     -> Many
```

Use the existing schema types:

- `schema_model::SingleCardinality` for scalar fields
- `schema_model::Cardinality` for links
- `schema_model::Uniqueness` for scalar uniqueness

If the current constructor API is too narrow for this mapping, add the smallest
schema constructor needed before expanding parser behavior.

## Error Model

The parser should define its own `ParseError` and `ParseErrorKind`.

Start with variants needed by tests:

```rust
pub struct ParseError {
    kind: ParseErrorKind,
    span: Option<Span>,
}

pub enum ParseErrorKind {
    UnexpectedToken { expected: &'static str },
    UnexpectedEof { expected: &'static str },
    DuplicateModifier { modifier: &'static str },
    IncompatibleModifiers { message: &'static str },
    InvalidScalarType { name: String },
    InvalidCatalog(schema_model::SchemaError),
}
```

`InvalidCatalog` is the bridge from direct catalog construction. If
`SchemaCatalog::try_new` rejects duplicate type names or unknown link targets,
the parser should return a parse error that preserves the schema error. The
span may be absent for catalog-level validation until the parser stores enough
source context to map those errors back to declarations.

## Module Structure

Use the same file layout style as the other parser crate:

```text
engine/schema-parser/
  Cargo.toml
  src/
    lib.rs
    lexer.rs
    parser.rs
    tests/
      mod.rs
```

`lib.rs` should expose the small public API and re-export stable error/token
types only when tests or users need them.

`lexer.rs` should own:

- `lex`
- `Token`
- `TokenKind`
- `Keyword`
- `Span`
- `Position`
- `LexError`
- `LexErrorKind`

`parser.rs` should own:

- `parse_schema`
- private `parse_schema_tokens`
- `Parser`
- `ParseError`
- `ParseErrorKind`

## Initial Test Sequence

### 1. `lexer_tokenizes_empty_type_declaration`

Input:

```text
type User {}
```

Assert the lexer produces `type`, `User`, `{`, `}` with useful spans.

### 2. `parser_can_parse_empty_object_type`

Input:

```text
type User {}
```

Assert:

- catalog has one object type
- object type name is `User`
- declared fields are empty

Do not assert implicit `id` through parser behavior. Implicit fields belong to
the schema/catalog model and SQLite planning layers.

### 3. `parser_can_parse_required_scalar_field`

Input:

```text
type User {
  required name: str
}
```

Assert:

- `User` has one declared scalar field
- field name is `name`
- scalar type is `ScalarType::Str`
- cardinality is required
- uniqueness is not unique

### 4. `parser_can_parse_property_keyword_scalar_field`

Input:

```text
type User {
  property name: str
}
```

Assert the result is the same as `name: str` except for cardinality implied by
the missing `required` modifier.

### 5. `parser_can_parse_required_unique_scalar_field`

Input:

```text
type User {
  required unique email: str
}
```

Assert the scalar field is required and unique.

### 6. `parser_can_parse_required_link_field`

Input:

```text
type User {}

type Post {
  required link author: User
}
```

Assert:

- `Post.author` is a link field
- target type name is `User`
- cardinality is required

### 7. `parser_can_parse_multi_link_field`

Input:

```text
type User {}

type Post {
  multi link likers: User
}
```

Assert the link cardinality is many.

### 8. `parser_rejects_multi_scalar_field`

Input:

```text
type User {
  multi name: str
}
```

Assert `ParseErrorKind::IncompatibleModifiers`.

### 9. `parser_rejects_unique_link_field`

Input:

```text
type User {}

type Post {
  unique link author: User
}
```

Assert `ParseErrorKind::IncompatibleModifiers`.

### 10. `parser_rejects_duplicate_type_names_through_catalog_validation`

Input:

```text
type User {}
type User {}
```

Assert the parse error wraps `schema_model::SchemaError::DuplicateTypeName`.

## Implementation Notes

Keep each parser step small and test-backed. Line comments are supported for
the deterministic schema identity contract in issue #59. Block comments,
formatter support, migration syntax, and recovery remain deferred.

When syntax and semantic validation overlap, prefer this boundary:

- parser: token order, punctuation, modifier placement, modifier compatibility
- schema catalog: uniqueness, reserved names, link target existence, field name
  uniqueness

This boundary is the main reason the first implementation can avoid an
intermediate AST.
