# SQLite Storage MVP Spec

## Goal

Define a concrete SQLite storage model that matches the schema and query MVP.
This spec fixes enough of the physical design for:

- schema application
- migration tracking
- query lowering assumptions
- runtime result shaping

## Core Approach

- One SQLite table per object type
- Implicit `id` primary key on every object table
- Scalar fields stored as direct columns
- Single relations stored as foreign key columns
- Multi relations stored in join tables
- Engine metadata stored in dedicated internal tables

## SQLite Pragmas

Recommended defaults for local development:

- `journal_mode = WAL`
- `foreign_keys = ON`

The engine should set or validate these at connection startup.

## Object Table Mapping

For a schema type:

```text
type Post {
  required title: str
  body: str
  required author: User
}
```

The object table should look conceptually like:

```sql
CREATE TABLE post (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  body TEXT NULL,
  author_id TEXT NOT NULL,
  FOREIGN KEY (author_id) REFERENCES user(id)
);
```

### Naming

The storage layer should use deterministic physical names:

- type `User` -> table `user`
- scalar field `name` -> column `name`
- single relation `author` -> column `author_id`

The exact naming transformation should be centralized in one module so SQL
generation and migrations cannot drift.

## Scalar Type Mapping

Recommended SQLite affinity mapping:

- `str` -> `TEXT`
- `int64` -> `INTEGER`
- `float64` -> `REAL`
- `bool` -> `INTEGER`
- `uuid` -> `TEXT`
- `datetime` -> `TEXT`

Notes:

- `bool` is stored as `0` or `1`
- `uuid` is stored as canonical text in the MVP
- `datetime` is stored as ISO-8601 text in UTC

## Single Relation Mapping

Single relations map to a nullable or non-nullable foreign key column on the
owning object's table.

Example:

```text
type Post {
  author: User
}
```

Maps to:

```sql
author_id TEXT NULL REFERENCES user(id)
```

`required author: User` becomes `NOT NULL`.

## Multi Relation Mapping

Multi relations use a dedicated join table named:

`<source_table>__<field_name>`

Example:

```text
type User {
  posts: multi Post
}
```

Maps to:

```sql
CREATE TABLE user__posts (
  source_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  position INTEGER NULL,
  PRIMARY KEY (source_id, target_id),
  FOREIGN KEY (source_id) REFERENCES user(id),
  FOREIGN KEY (target_id) REFERENCES post(id)
);
```

Notes:

- `position` is reserved for future stable ordering but may remain unused in the
  first runtime implementation.
- The MVP treats multi links as unordered at the language level.

## Implicit Identity

Every object row has:

- `id TEXT PRIMARY KEY`

The runtime is responsible for generating UUID values during insert when the
query does not explicitly provide one. The schema language does not expose user
control over identity definition in the MVP.

## Internal Metadata Tables

The first version should create at least these internal tables.

### `_engine_schema_versions`

Tracks applied migration revisions.

```sql
CREATE TABLE _engine_schema_versions (
  version_id TEXT PRIMARY KEY,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL,
  schema_snapshot TEXT NOT NULL
);
```

### `_engine_catalog_objects`

Stores semantic object definitions for diagnostics and diff support.

```sql
CREATE TABLE _engine_catalog_objects (
  object_id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);
```

### `_engine_catalog_fields`

Stores semantic field definitions.

```sql
CREATE TABLE _engine_catalog_fields (
  field_id TEXT PRIMARY KEY,
  object_id TEXT NOT NULL,
  name TEXT NOT NULL,
  field_kind TEXT NOT NULL,
  cardinality TEXT NOT NULL,
  scalar_type TEXT NULL,
  target_object_id TEXT NULL,
  FOREIGN KEY (object_id) REFERENCES _engine_catalog_objects(object_id)
);
```

These catalog tables are engine-owned metadata, not user-facing schema tables.

## Migration Model

The migration MVP is append-only:

1. Compare desired schema catalog to current catalog
2. Generate one migration plan
3. Apply DDL inside a transaction where SQLite allows it
4. Record the migration in `_engine_schema_versions`
5. Update catalog metadata tables

The first milestone can restrict supported schema changes to:

- create type
- add nullable scalar field
- add required scalar field only if a default/backfill strategy exists
- add single relation
- add multi relation join table

Changes that may require table rebuilds can be rejected initially with a clear
diagnostic.

## Query Lowering Assumptions

This storage model is designed around these compiler assumptions:

- root `select` begins from one object table
- scalar fields come from direct columns
- single relations use joins on `<field>_id`
- multi relations may use secondary queries or grouped joins

The runtime is allowed to fetch nested multi relations with follow-up queries if
that keeps the first implementation simpler and more predictable.

## Result Shaping Contract

The runtime should reconstruct nested JSON-like objects using:

- object identity deduplication by `id`
- per-shape field selection
- merge rules for repeated joined rows

Suggested rule:

- joined scalar and single-relation selections may be handled in one SQL query
- multi-relation nested shapes may use batched follow-up queries keyed by parent
  ids

This keeps the initial lowering model tractable.

## Indexes

The MVP should create indexes for:

- every foreign key column on object tables
- `target_id` and `source_id` access on join tables

Optional future indexes can be introduced later by schema directives.

## Deletes and Referential Behavior

The MVP should choose one explicit policy and document it consistently.

Recommended first policy:

- single relations use SQLite foreign keys with `ON DELETE RESTRICT`
- join tables delete rows with `ON DELETE CASCADE` from either side

This is conservative and avoids hidden object removal.

## Canonical Example

For:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required author: User
}
```

The core physical layout is:

```sql
CREATE TABLE user (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL
);

CREATE TABLE post (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  author_id TEXT NOT NULL REFERENCES user(id) ON DELETE RESTRICT
);
```

## Deferred Features

Out of scope until the basic migration and query loop is proven:

- generated columns
- partial indexes
- full-text search
- user-defined constraints
- enum storage optimizations
- online migration strategies
- schema branching
