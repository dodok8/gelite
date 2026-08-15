# Current limitations

The examples document behavior that exists in the current pipeline. The main
limitations are:

- The REPL executes the supported queries but prints flat tab-separated rows.
  Nested select shapes are present in the resolved IR and SQLite plan, but
  runtime nested result reconstruction is not implemented.
- `gelite repl --schema` compiles and renders queries but cannot execute them.
  Use `--debug` to inspect SQL and bind values.
- `gelite repl --database` executes `select`, `insert`, `update`, and `delete`.
  It does not provide a JSON result format.
- Initial schema application expects a new database. Migration diffing and
  migration history are not implemented.
- Inserts and updates accept scalar literals, single-link ID strings, and
  single-link selects narrowed by an implicit `id` or declared `unique` scalar
  field. Nested inserts, general subqueries, and multi-link mutations are not
  implemented.
- Composite unique constraints are not available in the current schema syntax.
  Association objects such as `OrderItem` and `PlaylistTrack` rely on the
  application to reject duplicate link pairs when needed.
- Update and delete filters are optional. The CLI does not ask for confirmation
  before an unfiltered mutation.
- Transaction commands work only in an interactive database-backed REPL. Enter
  `start transaction`, `commit`, or `rollback` as separate inputs.

The authoritative syntax and semantic contracts remain in `spec/schema.md` and
`spec/query.md`.
