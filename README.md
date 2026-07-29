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

- query compilation: `select`, `insert`, and `update` parsing, semantic
  resolution, SQLite query planning, and SQL rendering
- native query execution for the current `select`, `insert`, and `update`
  subsets
- initial schema planning: `.geli` parsing, SQLite schema planning, and DDL SQL
  rendering

It can apply the initial schema to a SQLite database and execute the current
query subset through the CLI REPL. It does not yet provide migration diffing,
`delete`, a server, or a web UI.

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
filter .title = "Hello"
order by .title desc
limit 10
```

Gelite can parse the query, resolve the names against the schema catalog,
produce Semantic IR, build a SQLite plan, and render SQL similar to:

```sql
SELECT "root"."title", "author"."id", "author"."name"
FROM "post" AS "root"
INNER JOIN "user" AS "author" ON "root"."author_id" = "author"."id"
WHERE "root"."title" = ?
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

## What is implemented

- `schema-model`: semantic schema catalog with object types, scalar fields,
  links, cardinality, deterministic references, and implicit `id` lookup.
- `schema-parser`: lexer and parser for the current `.geli` schema syntax.
- `query-ast`: unresolved syntax tree for select, insert, and update queries.
- `query-parser`: lexer and parser for the current query syntax, with source
  spans.
- `query-resolver`: AST-to-IR semantic analysis for select, insert, and update.
- `query-ir`: backend-independent Semantic IR for supported queries.
- `sqlite-query-plan`: SQLite-specific structured query plans.
- `sqlite-query-sqlgen`: SQL renderer that emits bind placeholders.
- `sqlite-schema-plan`: SQLite-specific initial schema plan.
- `sqlite-schema-sqlgen`: SQL renderer for initial schema DDL and metadata
  inserts.
- `sqlite-runner`: native schema and query statement execution.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: command orchestration shared by CLI-facing tools.
- `tools/repl`: inspection tool for running the current pipeline on a query.

## What is not implemented yet

- `gelite query plan` and `gelite query run`.
- Delete.
- Migration diffing and migration history.
- Runtime nested result shaping.
- HTTP API.
- Web playground.

## Running the project

Run all tests:

```sh
cargo test --workspace
```

Run the current CLI:

```sh
cargo run -p gelite-cli -- --help
```

If the `gelite` binary is installed or built directly, the examples below can
be written without the `cargo run -p gelite-cli --` prefix.

### Current CLI commands

The current CLI exposes three working command paths:

```text
gelite schema plan <schema.geli>
gelite schema apply <schema.geli> --database <app.db>
gelite repl --schema <schema.geli> [--debug] [QUERY]...
gelite repl --database <app.db> [--debug] [QUERY]...
```

`gelite schema plan <schema.geli>` parses a schema source file, builds the
initial SQLite schema plan, renders SQL, and prints metadata bind values
separately from SQL text. It does not open or mutate a database.

Example schema file:

```text
type User {
  required email: str
}

type Post {
  required title: str
  required link author: User
}
```

Run schema planning:

```sh
cargo run -p gelite-cli -- schema plan examples/blog.geli
```

Apply the schema to a SQLite database:

```sh
cargo run -p gelite-cli -- schema apply examples/blog.geli --database app.db
```

`gelite repl --schema <schema.geli>` runs the current query inspection pipeline
against a catalog parsed from a schema source file. With no query argument, it
starts the interactive REPL. With a query argument, it parses and renders that
one query.

`gelite repl --database <app.db>` loads the catalog from Gelite metadata tables
inside the SQLite database and executes select, insert, and update queries
against that database. Without `--debug`, it prints result rows, a generated
insert ID, or the number of updated rows. With `--debug`, it prints the rendered
SQL and bind values before the result.

Open the CLI REPL:

```sh
cargo run -p gelite-cli -- repl --database app.db
```

The following queries can be entered into the interactive REPL after applying
`examples/blog.geli`.

Insert a user:

```text
insert User {
  email := "alice@example.com"
}
```

The REPL prints the generated UUID. Use that value when inserting a linked
post:

```text
insert Post {
  title := "Draft",
  view_count := 5,
  author := "<generated-user-id>"
}
```

Read the inserted post and its author:

```text
select Post {
  id,
  title,
  view_count,
  author: {
    email
  }
}
```

Update posts selected through a linked object:

```text
update Post filter .author.email = "alice@example.com" set {
  title := "Reviewed"
}
```

Confirm the update:

```text
select Post {
  title,
  author: {
    email
  }
}
filter .title = "Reviewed"
```

Run one query through the CLI:

```sh
cargo run -p gelite-cli -- repl --database app.db 'select Post { title, author: { email } } filter .title = "Hello" order by .title desc limit 10'
```

Run a membership filter through the CLI:

```sh
cargo run -p gelite-cli -- repl --database app.db 'select Post { title, author: { email } } filter .title in ["Draft", "Published"] and .author.email not in ["blocked@example.com"] order by .title asc limit 20'
```

Print SQL and bind values before the result rows:

```sh
cargo run -p gelite-cli -- repl --database app.db --debug 'select Post { title, author: { email } } filter .title = "Hello"'
```

The CLI REPL does not use a hidden default catalog. If neither `--schema` nor
`--database` is provided, the command exits with a usage-oriented error.

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

The next technical goals are to add the delete pipeline and shape nested SQLite
results back into logical objects.
