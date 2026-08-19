# Gelite

Gelite is a practical reimplementation experiment for a Gel-like query
language.

The goal is not to clone Gel's codebase or rebuild every database feature at
once. The goal is to reproduce the useful language ideas in a smaller Rust
codebase:

- object types instead of table-first modeling
- explicit links between objects
- shaped `select` queries
- schema-aware name resolution
- typed intermediate representation
- lowering into ordinary SQLite SQL

The project is also a learning project. The implementation is intentionally
split into visible compiler stages so the language pipeline can be studied,
tested, and extended without hiding the important steps behind a large engine.

## What this project is trying to prove

Gel's query language is useful because a query can describe the object shape it
wants back:

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

That style is easier to read than manually assembling a set of joins and then
reconstructing nested objects in application code.

Gelite asks a smaller question:

Can that style of query language be implemented in a compact Rust engine that
targets SQLite?

The current answer is being built one layer at a time:

```text
query text
  -> syntax tree
  -> schema-resolved Semantic IR
  -> SQLite-specific plan
  -> SQL text + bind values
```

## Current scope

Gelite's current scope includes:

- query compilation: `select`, `insert`, `update`, and `delete` parsing, semantic
  resolution, SQLite query planning, and SQL rendering
- native query execution for the current `select`, `insert`, `update`, and `delete`
  subsets
- explicit `start transaction`, `commit`, and `rollback` commands in the
  database-backed interactive REPL
- initial schema planning: `.geli` parsing, SQLite schema planning, and DDL SQL
  rendering

It can apply the initial schema to a SQLite database and execute the current
query subset through the CLI REPL. It does not yet provide migration diffing, a
server, or a web UI.

That is intentional for this stage. The first useful milestone is to make the
language and schema pipelines correct and understandable before building
runtime features on top of them.

## Example

The schema model currently exists as Rust catalog values. The language being
modeled is:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

Given this query:

```text
select Post {
  title,
  author: {
    name
  }
}
filter .author.id in (
  select User { id }
  filter .name = "Sheri Tachibana"
)
order by .title desc
limit 10
```

Gelite can parse the query, resolve the names against the schema catalog,
produce Semantic IR, build a SQLite plan, and render SQL similar to:

```sql
SELECT "root"."title", "author"."id", "author"."name"
FROM "post" AS "root"
INNER JOIN "user" AS "author" ON "root"."author_id" = "author"."id"
WHERE "author"."id" IN (
  SELECT "root"."id"
  FROM "user" AS "root"
  WHERE "root"."name" = ?
)
ORDER BY "root"."title" DESC
LIMIT 10
```

The exact SQL is an implementation detail. The important part is that query
meaning passes through typed, inspectable stages before SQL is emitted.

## Why the stages matter

The project deliberately avoids compiling straight from text to SQL.

Each stage has one responsibility:

- Parser: turns source text into syntax.
- Schema catalog: stores object types, fields, links, cardinality, and implicit
  `id`.
- Resolver: checks names and shape rules against the catalog.
- Semantic IR: records the resolved meaning of a query without backend details.
- SQLite planner: chooses tables, columns, aliases, joins, predicates, and
  result-shaping metadata.
- SQL generator: renders the SQLite plan into SQL text and bind values.

This structure keeps Gel-like language semantics separate from SQLite-specific
storage decisions. It also makes the code useful as a study project: each
compiler step can be inspected independently.

Insert compilation generates a fresh UUID v4 for the implicit `id` bind value.
Rendered insert bind output is therefore non-deterministic and must not be used
as a stable snapshot or reproducible plan artifact.

## What is implemented

- `schema-model`: semantic schema catalog with object types, scalar fields,
  links, cardinality, deterministic references, and implicit `id` lookup.
- `schema-parser`: lexer and parser for the current `.geli` schema syntax.
- `query-ast`: unresolved syntax tree for data queries and transaction commands.
- `query-parser`: lexer and parser for the current query syntax, with source
  spans.
- `query-resolver`: AST-to-IR semantic analysis for select, insert, update, and delete.
- `query-ir`: backend-independent Semantic IR for supported queries.
- `sqlite-query-plan`: SQLite-specific structured query plans.
- `sqlite-query-sqlgen`: SQL renderer that emits bind placeholders.
- `sqlite-schema-plan`: SQLite-specific initial schema plan.
- `sqlite-schema-sqlgen`: SQL renderer for initial schema DDL and metadata
  inserts.
- `sqlite-runner`: native schema, query, and transaction execution.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: shared query compilation and execution orchestration.
- `tools/repl`: inspection tool for running the current pipeline on a query.

## What is not implemented yet

- Migration diffing and migration history.
- Runtime nested result shaping.
- HTTP API.
- Web playground.

## Running the project

Apply the organization example to a local SQLite database:

```sh
cargo run -p gelite-cli -- schema apply examples/organization.geli --database organization.db
```

Open the database-backed REPL:

```sh
cargo run -p gelite-cli -- repl --database organization.db
```

Execute one query file against an existing Gelite database:

```sh
cargo run -p gelite-cli -- query run query.geliql --database organization.db
```

The documentation contains three runnable examples, REPL input notes, and
current output limitations:

- [Getting started](docs/src/README.md)
- [Examples](docs/src/examples.md)
- [Organization](docs/src/organization.md)
- [Store](docs/src/store.md)
- [Music catalog](docs/src/music.md)
- [CLI reference](docs/src/cli.md)
- [Current limitations](docs/src/limitations.md)

Install the CI-pinned mdBook version, then serve the documentation locally:

```sh
cargo install mdbook --version 0.5.4 --locked
mdbook serve docs
```

The source Markdown remains readable without mdBook.

Run the project checks with `cargo test --workspace`.

## Repository guide

`spec/` defines what the language and engine stages mean:

- `spec/schema.md`: schema language and catalog semantics.
- `spec/query.md`: MVP query language surface.
- `spec/ir.md`: Semantic IR contract.
- `spec/storage-sqlite.md`: SQLite storage mapping.
- `spec/sqlite-query-plan.md`: SQLite query planning contract.

`plan/` explains the implementation order and design reasoning:

- `plan/new-db-engine-plan.md`
- `plan/new-db-engine-design.md`
- `plan/implementation-start-plan.md`
- `plan/query-parser-implementation-plan.md`
- `plan/select-path-traversal-plan.md`
- `plan/sqlite-query-plan-implementation-plan.md`
- `plan/sqlite-schema-plan-implementation-plan.md`
- `plan/cli-and-tooling-plan.md`

When these documents conflict, `spec/` wins for meaning and `plan/` wins for
work sequencing.

## Development principle

Gelite is written to learn how a Gel-like query compiler works by rebuilding
the important pieces in a smaller system.

That learning goal does not mean loose code. The project should keep the same
standard expected from production foundations:

- small features with clear contracts
- tests for semantic behavior
- explicit crate boundaries
- no direct AST-to-SQL shortcuts
- documentation that says what exists now and what is still missing

The next technical goal is to shape nested SQLite results back into logical
objects.
