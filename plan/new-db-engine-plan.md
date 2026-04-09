# New DB Engine Plan

## Goal

Build a new database engine inspired by Gel/EdgeDB's language and modeling
ideas, but implemented as a separate system using:

- Rust for the backend and engine core
- SQLite as the storage backend
- Solid for the frontend and developer-facing UI

This is not a migration of the existing Gel codebase. The current repository is
being used as a reference for language, schema, and compiler pipeline ideas.

## Guiding Principles

- Reuse concepts, not implementation details
- Keep the first version small and coherent
- Treat SQLite as the persistence layer, not just a temporary stand-in
- Optimize for a usable end-to-end developer experience early
- Prefer a typed query engine over a general-purpose database feature surface

## What To Borrow From Gel

- Schema language ideas
- Query language shape and ergonomics
- Compiler staging: AST -> IR -> backend lowering
- Type system and cardinality concepts
- Migration and schema catalog ideas

## What Not To Copy Directly

- Postgres-specific assumptions
- Python/Cython-heavy implementation structure
- Large protocol and compatibility surface
- Advanced engine features needed only at much larger scale

## High-Level Product Scope

The realistic first target is:

- a typed schema system
- a custom query language
- a compiler that lowers queries to SQLite SQL
- a runtime that shapes relational rows into nested results
- a Rust HTTP server and CLI
- a Solid-based playground/admin UI

The initial target is not:

- a full general-purpose database engine
- a distributed system
- a Postgres-compatible server
- a full GraphQL platform

## Major Components Needed

### Language

- Query lexer
- Query parser
- Query AST
- Formatter / pretty-printer

### Schema

- Schema definition language
- Schema AST
- Schema validator
- Schema catalog persistence

### Type System

- Scalar types
- Object types
- Links / relations
- Optional and multi cardinality

### Compiler

- Name resolution
- Semantic analysis
- Typed IR
- SQLite SQL lowering
- SQL generation

### Runtime

- Query execution
- Transaction handling
- Prepared statement management
- Nested result shaping

### Storage

- SQLite table layout
- Metadata tables
- Migrations
- Index and constraint support

### Server and Tooling

- Rust HTTP API
- CLI for schema/query/migration workflows
- Auth/session strategy
- Logging and diagnostics

### Frontend

- Solid playground
- Schema browser
- Query console
- Migration/status UI

## Recommended Build Sequence

1. Define the minimum schema language
2. Define the minimum query language
3. Implement parser and ASTs
4. Implement schema catalog and validation
5. Design typed IR
6. Implement IR -> SQLite SQL lowering
7. Implement runtime and nested result shaping
8. Add Rust server endpoints
9. Add CLI workflows
10. Add Solid playground and admin UI

## MVP Scope

The first milestone should support:

- declaring a small schema
- defining a few object types
- basic insert/select/update/delete
- simple filters, ordering, and limits
- basic 1:1 and 1:N relations
- migration apply
- HTTP query execution
- web playground query execution

## Features To Exclude From MVP

- distributed operation
- custom binary protocol
- advanced query optimizer work
- subscriptions
- complex polymorphism
- wide auth/provider integrations

## Early Risk Areas

### Scope explosion

Trying to build a full database engine from the start will stall delivery.

### Overfitting to Gel internals

The existing repository is deeply tied to Postgres and Python. The new engine
should adopt the design lessons, not the original boundaries.

### SQLite mismatch

SQLite works well for local, embedded, and moderate-concurrency workloads, but
write concurrency and some advanced backend features will need explicit design
limits.

### Weak intermediate representation

Compiling directly from AST to SQL will become brittle quickly. A typed IR
should be treated as a core milestone, not an optional refactor.

## Suggested Tech Stack

- Backend framework: `axum`
- Async runtime: `tokio`
- SQLite access: `sqlx` or `rusqlite`
- Serialization: `serde`
- Diagnostics: `miette` or equivalent
- Logging: `tracing`
- Frontend: `SolidStart`

## Immediate Next Steps

1. Write a short language and schema spec
2. Define the Rust workspace and crate boundaries
3. Choose parser strategy
4. Design the metadata/catalog tables for SQLite
5. Lock the MVP query surface before implementation starts
