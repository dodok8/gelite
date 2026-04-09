# New DB Engine Design Outline

## Core Positioning

This system is best thought of as:

- a typed query engine
- a schema and migration system
- a compiler that lowers custom queries to SQLite
- a runtime that reconstructs nested object results
- an application-facing database service with CLI and web tooling

SQLite is the storage engine. The new system is responsible for language,
typing, planning, lowering, execution orchestration, and developer experience.

## Architectural Shape

Recommended top-level structure:

- `core/ast`
- `core/parser`
- `core/schema`
- `core/ir`
- `core/compiler`
- `core/runtime`
- `core/storage-sqlite`
- `server`
- `cli`
- `web`

If implemented as a Rust workspace, these would map naturally to separate
crates, with the frontend living as a separate Solid app.

## Component Responsibilities

### `core/ast`

- Define query AST
- Define schema AST
- Keep syntax-layer structures separate from semantic-layer structures

### `core/parser`

- Tokenization
- Parsing for schema and query syntax
- Source span tracking
- Syntax diagnostics

### `core/schema`

- Catalog model
- Type definitions
- Link/relation definitions
- Name resolution inputs
- Schema validation
- Migration application model

### `core/ir`

- Typed intermediate representation
- Cardinality-aware expressions
- Resolved paths and relation traversal
- Backend-independent query representation

### `core/compiler`

- Semantic analysis
- Name resolution
- Type checking
- Cardinality checking
- IR construction
- IR to SQLite-lowering rules
- SQL emission

### `core/runtime`

- Transaction boundaries
- Query execution against SQLite
- Parameter binding
- Result decoding
- Nested result shaping
- Error translation

### `core/storage-sqlite`

- Metadata schema
- Physical table mapping
- Join table strategy
- Index creation
- Constraint representation
- Migration persistence

### `server`

- HTTP API
- Session/auth handling
- Query endpoint
- Schema apply endpoint
- Migration/status endpoints
- Admin and diagnostics endpoints

### `cli`

- Project init
- Schema apply
- Migration create/apply
- Query execution
- Explain/debug output

### `web`

- Query playground
- Schema browser
- Migration viewer
- Status and diagnostics screens

## Data Model Design Questions

These questions should be resolved early:

- How object types map to SQLite tables
- How links map to columns vs join tables
- How multi links are represented
- How optionality is enforced
- How computed fields, if any, are modeled
- How enums and custom scalar types are stored
- How schema metadata is versioned
- How migration history is stored

## Query Pipeline

Recommended pipeline:

1. Parse query text into AST
2. Resolve names against the schema catalog
3. Perform type and cardinality analysis
4. Produce typed IR
5. Lower IR to backend-specific plan
6. Emit SQLite SQL
7. Execute SQL
8. Shape flat rows into nested results

This staged pipeline is worth preserving even in the MVP because it creates
clear separation between language concerns and backend concerns.

## Why Typed IR Matters

A typed IR prevents several predictable problems:

- SQL lowering logic becoming tightly coupled to surface syntax
- difficulty supporting diagnostics and explain output
- duplication between validation and execution
- brittle behavior when extending the language later

The IR should carry:

- resolved schema references
- inferred types
- inferred cardinalities
- filter/order/limit semantics
- shape information for nested output

## SQLite-Specific Design Decisions

### Table Mapping

Start with one table per object type. For links:

- use foreign keys for singular relations where possible
- use join tables for multi relations

This keeps the first lowering model straightforward.

### Result Shaping

SQLite naturally returns flat rows. The engine should own reconstruction into
nested objects. This can be implemented by:

- generating joined SQL for simple cases
- batching secondary queries when a single query becomes too complex
- merging rows into object graphs in the runtime layer

### Migrations

Maintain explicit migration files and a metadata table that records:

- applied migration id
- checksum
- applied timestamp
- schema version

### Concurrency Model

Assume SQLite WAL mode and moderate write concurrency. Document limits rather
than hiding them. The server should serialize or retry writes where needed.

## Suggested MVP Language Surface

### Schema

Support a small schema language such as:

```text
type User {
  required name: str
  posts: multi Post
}

type Post {
  required title: str
  author: User
}
```

### Query

Support:

- `select`
- `insert`
- `update`
- `delete`
- basic filtering
- ordering
- limit/offset
- nested shape selection

Anything beyond that should be added only after IR and lowering are stable.

## Suggested Backend Stack

- HTTP: `axum`
- Runtime: `tokio`
- DB access: `sqlx` or `rusqlite`
- Error reporting: `miette`
- Logging/tracing: `tracing`
- Config: environment-based plus local project config file

## Suggested Frontend Stack

- `solid-js`
- `vite` or `SolidStart`
- query editor/playground UI
- schema exploration UI
- migration/status UI

The frontend should start as tooling for developers, not as a consumer app.

## Development Phases

### Phase 1

- workspace layout
- AST and parser
- schema catalog

### Phase 2

- semantic analysis
- typed IR
- SQLite lowering

### Phase 3

- runtime execution
- migrations
- CLI

### Phase 4

- HTTP server
- Solid playground
- explain/debug tooling

## Design Constraints

- Keep backend abstraction narrow and explicit
- Do not promise storage-engine portability before it is needed
- Separate syntax, semantics, and execution layers
- Favor debuggability over early optimization
- Prefer a small coherent language to a broad inconsistent one

## Recommended First Deliverable

The first end-to-end deliverable should be:

- a schema file
- a migration apply command
- a query command
- a Rust HTTP endpoint for query execution
- a simple Solid playground that can run a query and show shaped JSON results

That is small enough to finish, but complete enough to validate the engine's
core architecture.
