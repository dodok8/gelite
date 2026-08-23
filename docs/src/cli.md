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

`query plan` reads query and schema files, compiles the complete script, then
prints each data statement's rendered SQL and bind values plus transaction SQL
without opening a database or executing the query. Multi-link selects also
report how many follow-up plans may render batched queries. The number of query
batches is determined after parent identities are known at execution time.

`query run` loads the schema catalog from an existing Gelite database, validates
the complete script, then executes its statements in order on one connection.
It prints clear statement boundaries, select columns and rows, the generated
UUID for an insert, `affected_rows` for an update or delete, and `OK` for a
transaction command. It does not create a missing database.

Query scripts use semicolons as statement terminators and may contain multiline
data statements plus `start transaction`, `commit`, and `rollback`. Semicolons
inside strings are ignored. A single data statement without a semicolon remains
supported. Nested or unmatched transaction commands and scripts ending inside a
transaction are rejected before execution. A runtime failure rolls back the
active transaction while preserving earlier autocommit statements.

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

Transaction commands must be entered separately in the interactive REPL and are
not accepted in compile-only REPL sessions. Query script files may include them.
