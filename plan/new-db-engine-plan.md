# New DB Engine Plan

## Status

This is the broad product roadmap. The native compiler, runner, CLI, nested
result shaping, and first append-only schema migrations now exist. The current
implementation milestone is the browser runtime and unified SolidStart site in
`plan/browser-runtime-and-playground-plan.md`.

HTTP service and administration UI ideas below are long-term possibilities,
not part of the current browser milestone.

## Goal

Build a new database engine inspired by Gel/EdgeDB's language and modeling
ideas, but implemented as a separate system using:

- Rust for the backend and engine core
- SQLite as the storage backend
- a framework-independent TypeScript library for browser bindings
- SolidStart for the documentation and interactive tutorial site

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
- a CLI over the native runner
- a WASM runner and private TypeScript library
- a SolidStart documentation and interactive tutorial site

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
- A public SQLite connection adapter contract
- A built-in `RusqliteAdapter` for native and WASM execution

### Storage

- SQLite table layout
- Metadata tables
- Migrations
- Index and constraint support

### Server and Tooling

- CLI for schema/query/migration workflows
- Command-style schema workflows such as `schema plan` and `schema apply`
- REPL query workflow with optional schema meta commands that delegate to the
  same schema command implementation
- CLI and browser tooling details are tracked in
  `plan/cli-and-tooling-plan.md`
- A possible HTTP service remains outside the current browser milestone

### Frontend

- A private, framework-independent TypeScript library over the WASM bindings
- A SolidStart site containing documentation and a guided playground
- Memory VFS for the first browser runtime
- Browser runtime and site details are tracked in
  `plan/browser-runtime-and-playground-plan.md`

## Recommended Build Sequence

1. Define the minimum schema language
2. Define the minimum query language
3. Implement parser and ASTs
4. Implement schema catalog and validation
5. Design typed IR
6. Implement IR -> SQLite SQL lowering
7. Implement runtime and nested result shaping
8. Add CLI workflows
9. Validate `rusqlite` in browser WASM
10. Extract the public SQLite adapter contract and migrate native execution to
    `RusqliteAdapter`
11. Enable the same adapter and runner behavior in browser WASM
12. Add the private TypeScript library
13. Replace mdBook with the SolidStart documentation and playground site

## 0.x File Extension Decision

Use `.geli` for schema source files and `.geliql` for query, script, and future
migration files.

The split mirrors Gel's distinction between schema source and EdgeQL script
files without reusing Gel's extensions. The project is still in the 0.x phase,
so these extensions are practical conventions rather than permanent
compatibility promises.

## MVP Scope

The first milestone should support:

- declaring a small schema
- defining a few object types
- basic insert/select/update/delete
- simple filters, ordering, and limits
- basic 1:1 and 1:N relations
- migration apply
- native CLI query execution
- browser query execution through a private TypeScript library
- a guided SolidStart playground alongside the project documentation

## Features To Exclude From MVP

- distributed operation
- custom binary protocol
- advanced query optimizer work
- subscriptions
- complex polymorphism
- wide auth/provider integrations
- HTTP service and administration UI
- persistent browser storage

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

## Current Browser Stack

- Portable Rust core using `no_std + alloc`
- Public `no_std + alloc` SQLite connection adapter contract
- Built-in `std`-based `RusqliteAdapter` shared by native and WASM execution
- Target-selected `rusqlite` FFI: `libsqlite3-sys` on native targets and
  `sqlite-wasm-rs` on `wasm32-unknown-unknown`
- Private TypeScript package managed in a pnpm workspace
- SolidStart documentation and interactive tutorial site
- `tsdown` only if the TypeScript library needs a separate build artifact

## Immediate Next Steps

1. Validate `rusqlite` with an in-memory database in browser WASM (#76)
2. Extract the public adapter contract and migrate native execution to
   `RusqliteAdapter` (#77)
3. Enable the same runner and adapter in browser WASM (#78)
4. Add WASM bindings and the private TypeScript library (#79)
5. Build the SolidStart documentation and playground site (#80)
6. Switch CI and GitHub Pages from mdBook to SolidStart (#81)

OPFS persistence remains an unscheduled follow-up in #82. npm, crates.io, and
release planning are outside this sequence.
