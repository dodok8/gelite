# CLI reference

The current executable command paths are:

```text
gelite schema plan <schema.geli>
gelite schema apply <schema.geli> --database <app.db>
gelite query plan <query.geliql> --schema <schema.geli>
gelite query run <query.geliql> --database <app.db>
gelite repl --schema <schema.geli> [--debug] [QUERY]...
gelite repl --database <app.db> [--debug] [QUERY]...
```

When running from this repository, prefix each command with
`cargo run -p gelite-cli --`.

## Schema modes

`schema plan` prints initial SQLite DDL and metadata bind values without opening
a database. `schema apply` creates the initial schema in a new database.

## Query modes

`query plan` reads query and schema files, then prints the rendered SQL and bind
values without opening a database or executing the query.

`query run` reads one `select`, `insert`, `update`, or `delete` statement, loads
the schema catalog from an existing Gelite database, and executes the statement.
It prints select columns and rows, the generated UUID for an insert, or
`affected_rows` for an update or delete. It does not create a missing database.

Transaction commands and files containing multiple statements are rejected.
Transaction control remains available only in an interactive database-backed
REPL session.

## REPL modes

`repl --schema` compiles queries without executing them. `repl --database`
loads the schema catalog from the database and executes supported data queries.

Insert compilation generates a fresh UUID v4 for the implicit `id` bind value.
Its rendered bind output changes between runs and is not suitable for stable
snapshot comparison or use as a reproducible plan artifact.

With no query argument, either mode starts an interactive REPL. Enter each
statement without a semicolon. Regular Enter continues while braces are
unbalanced; `Alt+Enter` inserts a newline without submitting.

Database-backed interactive sessions also accept these inputs:

```text
start transaction
commit
rollback
```

Transaction commands must be entered separately and are not accepted in
compile-only or one-shot sessions.
