# CLI and Tooling Plan

## Purpose

Gelite needs a command-line interface that exercises the engine pipeline
without putting parsing, planning, SQL rendering, or SQLite execution logic in
the CLI crate itself.

The CLI should stay thin:

```text
read files / parse arguments
-> call engine crates
-> print diagnostics, plans, SQL, or query results
```

The same command implementation should be usable from a future REPL meta
command and from tests. The binary should not become the only place where
schema apply or query execution behavior exists.

## Command Shape

Use one top-level binary:

```text
gelite
```

Initial commands:

```text
gelite schema plan <schema.geli>
gelite schema apply <schema.geli> --database <app.db>

gelite query plan <query.geliql> --schema <schema.geli>
gelite query run <query.geliql> --database <app.db>

gelite repl --schema <schema.geli>
gelite repl --database <app.db>
```

`schema plan` and `schema apply` should be implemented before query execution
commands. A database cannot provide a real catalog until a schema has been
applied.

## CLI Parser

Use `clap` for the first CLI implementation.

`bpaf` is closer to Haskell `optparse-applicative` because it supports
combinator-style parser construction, but the Gelite CLI is not the core
learning target. The CLI should be conventional, well documented, and easy to
extend with nested subcommands. `clap` gives a stable derive-based model for
that:

```rust
#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    Query {
        #[command(subcommand)]
        command: QueryCommand,
    },
    Repl(ReplArgs),
}
```

The CLI parser should produce typed command structs only. It should not parse
`.geli` or `.geliql` language content.

## Crate Boundary

The project should split command orchestration from the binary entrypoint.

Recommended layout:

```text
tools/gelite-commands
tools/gelite-cli
tools/repl
```

`tools/gelite-cli` owns:

- `main`
- `clap` argument parsing
- process exit codes
- writing command output to stdout/stderr

`tools/gelite-commands` owns:

- schema command orchestration
- query command orchestration
- formatting command output into stable structs or text
- shared command functions used by CLI and REPL meta commands
- command-level tests that exercise engine pipelines without process I/O

`tools/gelite-commands` must not own:

- argument parsing
- process exit codes
- direct stdout/stderr writes
- interactive line editing
- language parsing implementations
- SQLite planning or SQL generation internals

`tools/gelite-commands` should depend on engine crates and runner-facing
traits, not on a concrete SQLite binding. Native CLI setup can construct a
native runner backend and pass it to the command layer. This keeps command
orchestration out of `engine` while still allowing `tools/repl` and
`tools/gelite-cli` to share the same command behavior.

`tools/gelite-cli` must stay a binary entrypoint. It should translate process
arguments into command structs, call `tools/gelite-commands`, print the result,
and choose an exit code. It should not contain schema planning, SQL rendering,
or runner-specific apply logic.

The existing `tools/repl` can remain while the new CLI is introduced. Once
`gelite repl` exists, `tools/repl` can either be replaced by the new binary or
turned into a thin wrapper around the same command/repl implementation.

## Schema Commands

Issue #59 adds the version-record and preview contracts below. They are design
requirements, not current behavior: the runtime does not yet insert a version
row, and schema plan output does not yet include version values or a preview
notice. Storage requirements are defined in
[SQLite Storage MVP Spec](../spec/storage-sqlite.md#initial-version-record-contract).

### Version record implementation choices

Use snapshot format v1, SHA-256, UUID v4, and UTC timestamps with millisecond
precision as defined in the storage spec. Keep snapshot encoding and hashing
pure in `sqlite-schema-plan`; the caller supplies application IDs and times.
Reuse the existing `uuid` v4 dependency in `gelite-commands`.

For SHA-256, use RustCrypto's `sha2` in `sqlite-schema-plan` with this dependency
configuration:

```toml
sha2 = { version = "0.11", default-features = false }
```

The [0.11.0 source](https://github.com/RustCrypto/hashes/blob/sha2-v0.11.0/sha2/src/lib.rs)
declares `#![no_std]`. Its default features are `alloc` and `oid`; neither is
needed for `Sha256::digest`. Keep the engine hashing path free of `std`
requirements and do not implement SHA-256 locally. Snapshot generation and
version-row insertion remain pending implementation.

Implement only the fixed snapshot structure; do not introduce a general
serialization framework or a general JSON canonicalizer for this milestone.
Test fixed snapshot bytes and checksums, invariance under declaration order
and source formatting changes, and sensitivity to logical schema changes.

### `gelite schema plan <schema.geli>`

Purpose: show what would be applied to an empty SQLite database.

Pipeline:

```text
read schema.geli
-> schema_parser::parse_schema
-> prepare version ID and timestamp preview placeholders
-> sqlite_schema_plan::plan_initial_schema
-> sqlite_schema_sqlgen::render_initial_schema
-> print SQL statements and bind values
```

This command must not open a database. It should work in environments where
SQLite execution is unavailable.

It must not generate or reserve a version ID or read the current time. The
version row uses these fixed display values:

- `version_id`: `<version-id-on-apply>`
- `applied_at`: `<applied-at-on-apply>`

The snapshot and checksum are computed values, not placeholders. Repeated
previews of the same source under the same snapshot format version must produce
identical output. Preview and apply share planning and SQL generation; the
caller supplies preview placeholders or actual application values.

Output must begin with:

```text
Schema plan preview.
Version ID and applied timestamp will be generated during schema apply.
No database was opened or changed.
```

Output should keep SQL and bind values separate:

```text
CREATE TABLE user (...)

INSERT INTO _engine_catalog_objects (object_id, name) VALUES (?, ?)
  binds: [Integer(1), Text("User")]
```

Do not inline metadata text values into generated SQL output.

First command-level test:

```text
schema_plan_command_renders_initial_schema_from_source
```

Assert:

- `.geli` source is parsed
- metadata tables appear before object tables
- object metadata insert statements keep bind values
- planned indexes appear after table creation
- the version insert follows all other schema statements
- repeated previews produce identical output without database, clock, or
  version ID generator access
- the output identifies preview values and uses the exact placeholders above

### `gelite schema apply <schema.geli> --database <app.db>`

Purpose: apply the initial schema to an empty SQLite database.

Pipeline:

```text
read schema.geli
-> schema_parser::parse_schema
-> prepare the actual version ID and UTC applied timestamp once
-> sqlite_schema_plan::plan_initial_schema
-> sqlite_schema_sqlgen::render_initial_schema
-> sqlite_runner::apply_schema_statements over a native runner backend
```

`schema apply` must reread and validate the source and build its own plan. It
does not consume preview output or replace placeholders in a saved preview.
A preview neither freezes nor approves an execution plan; editing the source
after previewing can change what is applied.

For an equivalent logical catalog and the same snapshot format version, the
stored snapshot and checksum must match the preview. The stored version ID and
timestamp must be actual application values, never preview placeholders.

Initial scope:

- initial schema only
- empty or newly created database
- one transaction for schema DDL, catalog metadata, indexes, and the version row
- no schema diffing
- exactly one initial version row once issue #59 is implemented

First command-level test:

```text
schema_apply_command_applies_initial_schema_to_database
```

Assert:

- database file or in-memory database is created
- metadata tables exist after apply
- object tables exist after apply
- catalog object rows exist after apply
- exactly one initial version row exists and contains no preview placeholders
- the stored snapshot and checksum match a preview of the same logical catalog
- statement or commit failure rolls back the schema and version row together
- reapplying an initial schema does not append or overwrite a baseline

Do not implement this command before `sqlite-runner` has tests for DDL,
prepared metadata inserts, and full rendered initial schema application.
The concrete native backend must also pass the runner backend verification
gates in `plan/sqlite-schema-plan-implementation-plan.md`. The CLI should not
be the first place where a SQLite binding is proven.

## Query Commands

### `gelite query plan <query.geliql> --schema <schema.geli>`

Purpose: compile a query without executing it.

Pipeline:

```text
read schema.geli
-> schema_parser::parse_schema
read query.geliql
-> query_parser splits semicolon-terminated statements
-> parse and resolve every data statement against the schema catalog
-> validate the complete transaction sequence
-> sqlite_query_plan and sqlite_query_sqlgen render each data statement
-> print each data statement's SQL and bind values plus transaction SQL
```

This command should support `--debug` later. Without `--debug`, it should print
only final SQL and bind values. With `--debug`, it can print parsed query AST,
IR, SQLite plan, and rendered SQL.

This command does not need SQLite execution and can be added before
`query run`.

### `gelite query run <query.geliql> --database <app.db>`

Purpose: execute a query against a database that already has Gelite metadata.

Pipeline:

```text
read query.geliql
-> load schema_model::SchemaCatalog from SQLite metadata
-> parse, compile, resolve, and transaction-validate the complete script
-> execute in order through one binding-neutral SQLite runner connection
-> print results with statement boundaries
```

The command opens one connection and accepts one legacy unterminated data
statement or a semicolon-terminated script. Selects return columns and rows,
inserts return the generated UUID, updates and deletes return `affected_rows`,
and transaction commands render as `BEGIN TRANSACTION`, `COMMIT`, or `ROLLBACK`
in plan output. Runtime failure rolls back an active explicit transaction while
preserving earlier autocommit statements.

The CLI requires the database file to exist before opening it so a misspelled
path cannot create an empty SQLite database.

## REPL

The REPL is an interactive query workflow, not the primary migration or schema
application interface.

Initial modes:

```text
gelite repl --schema <schema.geli>
gelite repl --database <app.db>
```

`--schema` mode can compile/debug queries without executing them. `--database`
mode should load the catalog from metadata once catalog loading exists.

Allowed early meta commands:

```text
:help
:exit
:debug on
:debug off
:schema
```

Later schema meta commands may be added:

```text
:schema plan path/to/schema.geli
:schema apply path/to/schema.geli
```

Those commands must delegate to `gelite-commands`; they should not implement a
second schema planning or execution path inside the REPL.

## WASM and Browser Demo

The engine crates are being kept mostly `no_std`, and `sqlite-rs-embedded`
describes itself as `no_std` and WASM-compatible. That makes a browser demo
possible, but it should not change the first CLI/runtime sequence.

The intended browser demo stack is:

- Solid.js for the UI
- a WASM build of the Gelite engine crates
- a WASM-compatible SQLite runner backend implementing the same runner-facing
  contracts as the native backend
- Optique only where a TypeScript command parser is useful, such as a browser
  command palette, scripted demo commands, or a web-based CLI-like input

Optique is a type-safe combinatorial CLI parser for TypeScript. It should not
replace the Rust CLI parser used by `tools/gelite-cli`, and it should not own
Gelite language parsing. `.geli` and `.geliql` parsing must still come from the
Rust parser crates compiled to WASM.

The likely browser execution pipeline is:

```text
schema source in editor
-> schema_parser
-> sqlite_schema
-> sqlite_schema_sqlgen
-> sqlite_runner using a WASM backend
-> query_parser / resolver / sqlite_plan / sqlite_sqlgen
-> execute query in browser
-> render result rows
```

The first browser demo should be a developer tool, not a production server
replacement:

- in-browser schema editor
- query editor
- optional CLI-like command input for demos
- generated SQL view
- SQLite result preview
- optional debug panels for AST, IR, and SQLite plan

Do not start the browser demo until:

- `sqlite-runner` can apply an initial schema
- the native runner backend has proven the runner contract
- a WASM runner backend can open a database and execute the same smoke tests in
  the target browser/WASM environment
- SELECT execution and result shaping have tests outside the browser

The browser demo should reuse engine and command-layer code where possible.
Avoid putting language parsing or SQL generation logic in TypeScript.

## Implementation Sequence

Steps 1 through 11 are implemented. WASM runner validation and the browser
demo remain deferred.

1. Add `sqlite-runner` and define binding-neutral runner traits for DDL and
   metadata inserts.
2. Add `tools/gelite-commands` with `schema plan` orchestration over source
   text.
3. Validate one native SQLite backend against the runner trait.
4. Add `tools/gelite-cli` using `clap`.
5. Implement `gelite schema plan`.
6. Implement `gelite schema apply`.
7. Add `query plan --schema`.
8. Add catalog loading from SQLite metadata.
9. Add SELECT execution to `sqlite-runner`.
10. Add `query run --database`.
11. Route `gelite repl` through the shared command/query implementation.
12. Validate a WASM runner backend against the same smoke tests.
13. Revisit WASM/browser demo once runner behavior is tested outside the
    browser.

## Current Non-Goals

- full migration diffing
- applying schema changes to non-empty databases
- query execution before catalog loading exists
- TypeScript reimplementation of Gelite parsers or planners
- browser demo before the SQLite runner contract is stable
