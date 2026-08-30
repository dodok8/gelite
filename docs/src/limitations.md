# Current limitations

The examples document behavior that exists in the current pipeline. The main
limitations are:

- The REPL reconstructs selected single links as nested objects and selected
  multi links as collections while keeping top-level rows tab-separated.
  Multi-link collection order is unspecified.
- Declared inverse links are readonly and always multi. Stored forward links
  own the foreign keys or join tables; inverse links create no duplicate storage.
- Filters support a multi path compared with a literal using independent
  existence conditions. Same-target predicate scopes are deferred to #68.
  Multi paths remain unsupported in ordering, computed values, arithmetic,
  function arguments, membership operands, and path-to-path comparisons.
- `gelite repl --schema` compiles and renders queries but cannot execute them.
  Use `--debug` to inspect SQL and bind values.
- `gelite repl --database` executes `select`, `insert`, `update`, and `delete`.
  It does not provide a JSON result format.
- Schema application supports new objects, nullable scalar fields, optional
  single links, and multi links. Removal, rename inference, required or unique
  additions to existing objects, table rebuilds, backfills, and concurrent or
  online migration coordination are not implemented.
- Inserts and regular updates accept scalar literals, single-link ID strings,
  and single-link selects narrowed by an implicit `id` or declared `unique`
  scalar field. Multi-link updates support one `+=` or `-=` operation per
  statement with a target select that projects implicit `id`; replacement,
  literals, and mixed regular assignments are not supported. Membership
  filters also accept uncorrelated selects that project one compatible required
  scalar field. Nested inserts and subqueries in other expression positions are
  not implemented.
- Composite unique constraints are not available in the current schema syntax.
  Association objects such as `OrderItem` and `PlaylistTrack` rely on the
  application to reject duplicate link pairs when needed.
- Update and delete filters are optional. The CLI does not ask for confirmation
  before an unfiltered mutation.
- Transaction commands work only in an interactive database-backed REPL. Enter
  `start transaction`, `commit`, or `rollback` as separate inputs.

The authoritative syntax and semantic contracts remain in `spec/schema.md` and
`spec/query.md`.
