# Browser Runtime and SolidStart Site Plan

## Status

This document defines the browser runtime milestone that follows the native
runner and the first append-only schema migration support. It is the source of
truth for issues #76 through #82.

The milestone covers a WASM SQLite runner, a private framework-independent
TypeScript library, and one SolidStart site that contains both the project
documentation and an interactive tutorial.

## Current Starting Point

The repository currently provides:

- a `no_std`-oriented compiler and runtime core that uses `alloc`
- native SQLite schema, query, and transaction execution
- nested single- and multi-link result shaping
- initial and supported append-only schema planning and application
- CLI and REPL workflows over the native runner

The native SQLite backend links the system SQLite library. A successful
`cargo check` for `wasm32-unknown-unknown` therefore does not prove that the
final WASM artifact can link, load in a browser, or execute SQL.

## Goals

- Execute the existing Gelite schema and query pipeline in a browser.
- Preserve the portable `no_std + alloc` core goal across native, WASM, and
  possible future embedded targets.
- Keep the runner core and public SQLite adapter contract portable while using
  `rusqlite` as the built-in native and WASM adapter.
- Expose the WASM runner through a private, framework-independent TypeScript
  library.
- Replace the mdBook site with one SolidStart documentation and playground
  site without preserving the old site structure or URLs.
- Demonstrate the Organization/CFP workflow with an in-memory browser
  database.

## Non-Goals

This milestone does not include:

- npm or crates.io publication
- a release schedule or public package compatibility policy
- OPFS persistence
- an HTTP server
- an administration UI
- a general-purpose SQL console
- schema or migration editors
- an embedded-target SQLite adapter
- documentation search
- Worker-based execution or concurrency optimization

## Architecture

```text
compiler and runner core (`no_std + alloc`)
        |
        +-- public SQLite connection adapter contract
                    |
                    +-- built-in RusqliteAdapter (`std`)
                    |       +-- native: libsqlite3-sys
                    |       +-- WASM: sqlite-wasm-rs
                    |                   |
                    |            WASM bindings
                    |                   |
                    |       private TypeScript library
                    |             packages/gelite
                    |                   |
                    |            SolidStart site
                    |                apps/site
                    |          +-- /docs/...
                    |          +-- /playground/...
                    |
                    +-- external adapters
```

### Portable Core and Runner Contracts

Schema parsing, query compilation, migration planning, result shaping, and
transaction semantics remain shared `no_std + alloc` Rust logic. The runner
core exposes a narrow connection adapter contract for connection lifecycle,
statement preparation, value binding, row stepping, column reads, affected-row
counts, and backend error translation.

The existing command-facing `SQLiteRunner`, `SQLiteSchemaReader`,
`SQLiteQueryRunner`, and `SQLiteTransactionRunner` contracts remain above this
connection boundary. Schema verification, migration ordering, query
orchestration, transaction semantics, and nested result shaping stay in the
shared runner core rather than in an adapter.

External crates may implement the connection adapter and reuse the shared
runner without adding a Gelite feature. This preserves a path for embedded,
custom SQLite, and platform-specific VFS integrations without implementing any
of them in this milestone.

The shared runner uses static generic composition:

```rust
pub struct SQLiteEngine<A> {
    adapter: A,
}

pub type RusqliteRunner = SQLiteEngine<RusqliteAdapter>;
```

Do not add dynamic dispatch, an adapter registry, a factory, or `Send + Sync`
bounds in this milestone. Exact trait methods, associated types, and lifetimes
are decided in #77 from the operations proven by #76.

### Target and Feature Roles

- The default feature set keeps the runner core `no_std + alloc` compatible.
- A `std` feature enables standard-library integration.
- A `rusqlite` feature enables the built-in `RusqliteAdapter` and implies
  `std`.
- The target selects the FFI used inside `rusqlite`: `libsqlite3-sys` on native
  targets and `sqlite-wasm-rs` on `wasm32-unknown-unknown`.
- Unsupported built-in feature and target combinations must fail clearly at
  compile time.
- External adapters do not require the `rusqlite` feature.

If a future WASM build offers both Memory VFS and OPFS, storage selection should
be a runtime option rather than a separate copy of the shared runner logic.

## Built-In Rusqlite Adapter

`RusqliteAdapter` is the only built-in adapter in this milestone. Native and
WASM builds share the same adapter and runner logic; `rusqlite` selects the
target-specific low-level FFI.

Issue #76 must prove the current `rusqlite` WASM path through a final link and a
real browser smoke test. The spike must cover:

- `Connection::open_in_memory()`
- DDL and prepared statements
- integer, real, text, and null binding
- row stepping, column names, types, and owned values
- affected-row counts
- commit and rollback
- typed SQLite failures without panics

`cargo check` alone is not an acceptance gate. Only if this retained browser
spike fails should the project reconsider a direct `sqlite-wasm-rs` adapter.

### Native Migration

Issue #77 replaces `powersync_sqlite_nostd`, direct SQLite C API calls, and the
manual `-lsqlite3` build script with `RusqliteAdapter`. It preserves existing
native behavior and moves native-gated result shaping into the shared runner
core.

The adapter contract must not expose `rusqlite` types or abstract beyond
SQLite. A test-only adapter must demonstrate that an external implementation
can reuse the shared schema, query, migration, transaction, and result-shaping
logic.

## Browser Storage

### Memory VFS MVP

The first browser runner uses an ephemeral in-memory SQLite database. Data
survives only for the lifetime of the page session. Reloading the page or using
the tutorial reset action creates a new database and reapplies the example
schema and data.

This keeps persistence, locking, browser storage permissions, and recovery out
of the first executable browser slice.

### OPFS Follow-Up

OPFS is tracked by #82 and has no schedule in this milestone. It should be
revisited only after the Memory VFS site is stable. The follow-up owns file
lifecycle, locking, Worker placement, browser support, security headers, and
recovery behavior.

## Private TypeScript Library

`packages/gelite` is a private pnpm workspace package that wraps the WASM
bindings with a JavaScript-friendly API.

It owns:

- WASM module initialization
- browser runner creation and reset
- schema application and query execution entry points
- JavaScript-facing inputs, results, and error translation

It must remain independent of Solid and SolidStart. Its `package.json` uses
`"private": true`, and the package is consumed only inside this repository.
Do not add npm publishing metadata or a public compatibility promise.

The site may consume the TypeScript source directly when the toolchain supports
it. Add `tsdown` only if the library requires a separate compile or bundle
artifact.

## SolidStart Documentation and Playground Site

`apps/site` is the only browser application. It consumes `packages/gelite` and
provides a shared layout and navigation for documentation and the interactive
tutorial.

The initial route mapping is:

| Content | SolidStart route |
| --- | --- |
| Short introduction with documentation and playground links | `/` |
| `docs/src/README.md` | `/docs/` |
| `docs/src/examples.md` | `/docs/examples/` |
| `docs/src/organization.md` | `/docs/examples/organization/` |
| `docs/src/store.md` | `/docs/examples/store/` |
| `docs/src/music.md` | `/docs/examples/music/` |
| `docs/src/cli.md` | `/docs/cli/` |
| `docs/src/limitations.md` | `/docs/limitations/` |

Reuse the existing Markdown content where practical. Issue #80 should choose
the smallest SolidStart-compatible Markdown integration and must not introduce
a general content-management layer.

The `/playground/` tutorial uses the Organization/CFP example to demonstrate:

1. creating a Memory VFS runner
2. applying the example schema
3. loading sample data
4. editing and running the prepared Gelite query for each tutorial step
5. displaying shaped results
6. resetting the database

The tutorial allows free editing within each prepared query step. It is not a
general SQL console, schema editor, or database administration tool.

## pnpm Workspace

The JavaScript workspace has two independent products:

```text
pnpm-workspace.yaml
packages/gelite/
apps/site/
```

pnpm manages dependencies and workspace links. The Rust and JavaScript
workspaces remain separate build systems; no combined wrapper command is needed
for this milestone.

## mdBook Migration

mdBook and SolidStart are not maintained as parallel documentation sites.
Issue #80 moves the current documentation into `apps/site`. Issue #81 switches
GitHub Pages to the SolidStart static output and removes:

- `docs/book.toml`
- `docs/src/SUMMARY.md`
- mdBook installation and build steps
- README instructions for `mdbook serve docs`

Documentation content is moved, not discarded. Existing mdBook routes and URLs
do not require compatibility redirects.

## Implementation Sequence

### #76: Validate Rusqlite for Browser WASM

- Link a real WASM artifact.
- Load it in a browser.
- Open an in-memory database through `rusqlite` and execute the required
  connection operations.
- Record any API or target limitations that affect the adapter contract.

Branch: `issue-76-rusqlite-wasm-validation`

### #77: Add the Adapter Contract and Migrate Native Execution

- Extract the minimum public connection adapter contract proven by the native
  runner and #76.
- Compose the shared runner as `SQLiteEngine<A>` and provide the
  `RusqliteRunner` alias.
- Add the built-in `RusqliteAdapter` and migrate native execution to it.
- Remove `powersync_sqlite_nostd`, direct C API calls, and manual SQLite
  linking.
- Prove external adapter reuse with a test-only implementation.

Branch: `issue-77-sqlite-adapter-rusqlite`

### #78: Enable the Rusqlite Adapter in Browser WASM

- Use the same shared runner and `RusqliteAdapter` as native execution.
- Enable schema, migration, query, and transaction execution over Memory VFS.
- Add browser tests for schema and query execution.

Branch: `issue-78-rusqlite-wasm-runner`

### #79: Add WASM Bindings and the TypeScript Library

- Expose the runner through WASM bindings.
- Add private `packages/gelite` APIs.
- Add `tsdown` only if a separate package build is required.

Branch: `issue-79-wasm-js-package`

### #80: Build the SolidStart Documentation Site and Playground

- Add `apps/site`.
- Move the mdBook content into the site.
- Add shared navigation and the Organization/CFP tutorial.
- Verify local documentation routes and Memory VFS execution.

Branch: `issue-80-solidstart-docs-playground`

### #81: Deploy the SolidStart Site

- Add pnpm checks and the static site build to CI.
- Deploy the SolidStart output to GitHub Pages.
- Verify direct routes and asset paths.
- Remove the mdBook configuration and deployment path.

Branch: `issue-81-solidstart-site-deployment`

### #82: Add OPFS Persistence

This is an unscheduled follow-up after the Memory VFS site is stable.

Branch: `issue-82-opfs-persistence`

## Validation Gates

The implementation issues must preserve:

- native Rust tests
- a no-default-features `no_std + alloc` runner-core compile check
- a test-only external connection adapter using the shared runner
- final WASM linking and browser SQL execution
- pnpm install consistency, type checks, and production site builds
- browser smoke tests for the tutorial reset and query flow
- valid migrated documentation links and direct GitHub Pages routes

Issue #75 itself is documentation-only and requires link inspection and
`git diff --check`; it does not require Rust or browser tests.

## Completion Criteria

The browser milestone is complete when:

- the runner core exposes a public SQLite connection adapter contract without
  depending on `std` or `rusqlite`
- external adapters can reuse shared Gelite execution behavior
- native and WASM execution use the built-in `RusqliteAdapter`
- `powersync_sqlite_nostd` and direct C API wrappers are removed
- native and WASM runners preserve the supported Gelite behavior
- `packages/gelite` exposes the private framework-independent browser API
- `apps/site` serves the migrated documentation and executable tutorial
- GitHub Pages deploys the SolidStart site without mdBook
- Memory VFS is the only promised browser storage mode
- npm, crates.io, release planning, and OPFS remain outside the milestone
