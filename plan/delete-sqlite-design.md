# SQLite Delete Planning and SQL Generation Design

## Goal

Lower resolved delete queries into a SQLite-specific plan and render executable
SQLite statements for unfiltered, root-filtered, and relation-filtered deletes.

## Scope

This step adds SQLite planning and SQL generation only. Runtime execution,
command integration, REPL behavior, and end-to-end query pipeline tests remain
separate follow-up work.

## Architecture

`query_ir::DeleteQuery` is lowered by `sqlite_query_plan::plan_delete` into a
dedicated `SQLiteDeletePlan`. The plan contains only the target object source,
an optional planned predicate, and the joins required by that predicate.

Update and delete planning share one private mutation-filter planning helper.
The helper reserves root path aliases, allocates join aliases, plans the
predicate, and deduplicates joins. Assignment planning remains update-specific.

SQL generation exposes `sqlite_query_sqlgen::render_delete`. It reuses the
existing identifier quoting, predicate rendering, join rendering, and bind
conversion functions.

## Plan Contract

`SQLiteDeletePlan` contains:

- `target: SQLiteObjectSource`
- `filter: Option<SQLiteWhereExpr>`
- `joins: Vec<SQLiteJoin>`

The target uses the same physical table, `root` alias, `id` column, and object
type metadata as an update target. An omitted filter stays absent and means all
rows in the target table.

## SQL Forms

An unfiltered delete renders without an alias or predicate:

```sql
DELETE FROM "post"
```

A predicate that only references the root object renders directly against a
target alias:

```sql
DELETE FROM "post" AS "root" WHERE "root"."title" = ?
```

A predicate that traverses a relation selects target identities in a subquery:

```sql
DELETE FROM "post"
WHERE "id" IN (
  SELECT "root"."id"
  FROM "post" AS "root"
  JOIN ...
  WHERE ...
)
```

Bind values follow predicate traversal order. Delete has no assignment binds.

## Referential Behavior

Delete SQL does not emit explicit relation cleanup statements. Existing SQLite
foreign keys define behavior: multi-link join rows use `ON DELETE CASCADE`, and
required single-link references use `ON DELETE RESTRICT`. Constraint failures
are reported by the runner in a later integration step.

## Error Handling

Planning accepts already-resolved Semantic IR and introduces no new semantic
errors. SQL generation is deterministic and does not execute statements. SQLite
constraint and execution errors remain runner responsibilities.

## Testing

Planner tests cover:

- target table and identity metadata
- absent filters
- direct root predicates without joins
- relation predicates with the required join and collision-safe aliases

SQL generation tests cover:

- unfiltered delete SQL
- direct predicate SQL and bind order
- relation predicate identity-subquery SQL and bind order
- identifier quoting through the shared renderer

Each layer is implemented test-first and committed independently after focused
and workspace-wide verification.

## Alternatives Rejected

Reusing `SQLiteUpdatePlan` would expose assignments on deletes and weaken the
plan contract. Duplicating update predicate planning would create two paths for
alias allocation and join deduplication. A dedicated delete plan with one
private shared helper keeps the public model explicit while minimizing code.
