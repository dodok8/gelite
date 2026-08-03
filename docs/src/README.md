# Getting started

Gelite compiles a Gel-like schema and query language to SQLite. The current CLI
can plan or apply an initial schema and can compile or execute queries through
the REPL.

## Apply the example schema

From the repository root:

```sh
cargo run -p gelite-cli -- schema apply examples/organization.geli --database organization.db
```

This creates a SQLite database for the organization example. Schema application
is for a new database; migration diffing and migration history are not
implemented.

## Open the REPL

```sh
cargo run -p gelite-cli -- repl --database organization.db
```

Enter each query without a trailing semicolon. Regular Enter continues while
braces are unbalanced. `Alt+Enter` inserts a newline without submitting input.

Use `--schema` instead when only compilation and rendered SQL are needed:

```sh
cargo run -p gelite-cli -- repl --schema examples/organization.geli --debug \
  'select Employee { name } filter .active = true'
```

The `--schema` mode does not open a database or execute the query. The
`--database` mode loads the catalog stored in the database and executes the
current `select`, `insert`, `update`, and `delete` subsets.

Continue with the [Examples](examples.md) overview and choose one of the three
runnable domains.

## Build this documentation

Install the same mdBook version used by CI, then run:

```sh
cargo install mdbook --version 0.5.4 --locked
mdbook serve docs
```

The Markdown under `docs/src` is also readable directly on GitHub.
