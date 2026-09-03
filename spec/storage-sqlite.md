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
- Only stored schema `link` fields create relation storage structures
- Scalar fields never use join tables in the MVP

## SQLite Pragmas

Recommended defaults for local development:

- `journal_mode = WAL`
- `foreign_keys = ON`

The engine should set or validate these at connection startup.

## Runtime Driver Boundary

The parser, semantic, planning, and SQL generation layers remain independent
of a concrete SQLite driver and retain their `no_std` contracts where
applicable. SQLite execution is a runtime boundary rather than part of those
engine contracts.

Runner traits define the behavior required by schema and query consumers.
Applications may provide their own runner implementation for their execution
environment. Gelite plans to provide native and WASM runners using stable APIs
appropriate to each environment; they do not need to share one binding crate.

`NativeSQLiteRunner` is the official native implementation and uses
`rusqlite`. Its driver remains private so changing the native binding does not
change runner-facing APIs, compiler stages, query semantics, or schema
semantics. WASM execution uses a separate backend and is not implied by the
native driver's capabilities.

## Object Table Mapping

For a schema type:

```text
type Post {
  required unique slug: str
  required title: str
  body: str
  required link author: User
}
```

The object table should look conceptually like:

```sql
CREATE TABLE post (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
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
- single `link author` -> column `author_id`
- multi `link posts` on `User` -> join table `user__posts`

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

## Scalar Uniqueness

Scalar fields declared with `unique` map to SQLite `UNIQUE` constraints.

Example:

```text
type User {
  unique nickname: str
  required unique email: str
}
```

Maps conceptually to:

```sql
CREATE TABLE user (
  id TEXT PRIMARY KEY,
  nickname TEXT NULL UNIQUE,
  email TEXT NOT NULL UNIQUE
);
```

For optional unique scalar fields, Gelite uses SQLite's `UNIQUE` behavior:
duplicate non-null values are rejected, but multiple `NULL` values are allowed.
The MVP treats uniqueness as a constraint on present values.

## Single Relation Mapping

Single relations map to a nullable or non-nullable foreign key column on the
owning object's table.

Example:

```text
type Post {
  link author: User
}
```

Maps to:

```sql
author_id TEXT NULL REFERENCES user(id)
```

`required link author: User` becomes `NOT NULL`.

For an MVP insert, a declared single-link assignment writes the related object
id to this `<field>_id` column. The temporary query-language string-literal
link shorthand is bound as a SQLite `TEXT` value; it is not stored as a nested
object or as a value in a join table. An optional single-link `null` assignment
writes SQLite `NULL`.

A supported single-link select assignment renders as a scalar SQLite subquery
in the same `<field>_id` value position. The resolver guarantees that the
subquery projects the target object's `id` and returns at most one row. One row
writes its identity; zero rows produce SQLite `NULL`. The latter is accepted by
an optional link and rejected by the `NOT NULL` constraint of a required link.
Nested filter bind values remain in source assignment order.

## Multi Relation Mapping

Multi relations use a dedicated join table named:

`<source_table>__<field_name>`

Example:

```text
type User {
  multi link posts: Post
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
- Only `multi link` fields produce join tables. Multi-valued scalar storage is
  out of scope for the MVP.

### Multi-Link Mutation Mapping

An add operation inserts the Cartesian product of selected source and target
identities into `source_id` and `target_id`. Conflict handling applies only to
the composite primary key: an existing relationship is left unchanged. A
remove operation deletes rows whose source and target identities belong to the
selected sets. Both operations are rendered as one set-based SQLite statement,
not one statement per target.

The reserved `position` column remains `NULL`; mutation syntax does not assign
or preserve ordering. Foreign keys continue to reject identities that are not
present in their object tables, although the accepted select-based mutation
surface can only produce existing identities.

## Implicit Identity

Every object row has:

- `id TEXT PRIMARY KEY`

For the current insert milestone, the query runtime generates a UUID v4 and
supplies it to SQL rendering. The renderer binds it as the `id` column value in
the same prepared `INSERT` statement as user-provided scalar and single-link
values. After successful execution, the CLI reports the generated id. The
schema language and query language do not expose user control over identity
definition in the MVP.

SQLite constraint failures, including missing required values and invalid
foreign-key targets when foreign keys are enabled, are execution errors;
semantic validation remains responsible for the query language's field,
cardinality, and literal-type rules.

## Internal Metadata Tables

The first version should create at least these internal tables.

### `_engine_schema_versions`

Tracks applied migration revisions.

```sql
CREATE TABLE _engine_schema_versions (
  version_id TEXT PRIMARY KEY,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL,
  schema_snapshot TEXT NOT NULL,
  version_number INTEGER NOT NULL UNIQUE
);
```

#### Initial version record contract

The following contract is defined for issue #59. The engine and schema commands
plan and apply the initial version row.

- A successful initial schema application must record exactly one version row.
- The initial row has `version_number = 1`, including in previews. The number
  records application order and is excluded from the snapshot and checksum.
- The snapshot and checksum must be computed from the validated logical
  `SchemaCatalog`, not from the original source text. Equivalent catalogs must
  produce identical snapshots and checksums under the same snapshot format
  version, regardless of source comments, whitespace, or declaration order.
- The version ID and applied timestamp describe an application attempt and
  must not affect the snapshot or checksum. The caller prepares them once per
  attempt; pure planning and SQL generation must not generate IDs or read the
  clock. The applied timestamp must represent UTC.
- The version insert must follow the schema DDL, catalog metadata, and indexes
  in the same transaction. Statement or commit failure must roll back the
  version row together with the schema changes.
- Reapplying an initial schema to an existing Gelite database must not append
  a duplicate baseline or overwrite the existing version record.

Schema plan previews must show the computed snapshot and checksum, but use
`<version-id-on-apply>` and `<applied-at-on-apply>` as the respective version ID
and timestamp values. These are reserved display placeholders, not valid
persisted version values. The application path must not store them.

See [CLI and Tooling Plan](../plan/cli-and-tooling-plan.md#schema-commands) for
preview output and application behavior.

#### Snapshot format v1

The snapshot is UTF-8 JSON with this fixed structure and property order:

| Value | Properties in output order |
| --- | --- |
| Root | `format_version`, `objects` |
| Object type | `name`, `declared_fields`, `implicit_fields` |
| Scalar field | `name`, `kind`, `scalar_type`, `cardinality`, `unique` |
| Link field | `name`, `kind`, `target_type`, `cardinality`, `unique`, `inverse_field` |

- `format_version` is the integer `1`; `objects`, `declared_fields`, and
  `implicit_fields` are arrays.
- Each object preserves declared and implicit fields in separate arrays.
  Sort objects and each field array independently by their unescaped names
  in ascending UTF-8 byte order, independently of locale and declaration order.
- `kind` is `scalar` or `link`. Scalar types use the schema names `str`,
  `int64`, `float64`, `bool`, `uuid`, and `datetime`. Cardinality is `optional`,
  `required`, or `many`, as permitted by the validated catalog.
- `unique` is a JSON boolean and is always included, even when false.
  `target_type` names the referenced object type. `inverse_field` names the
  forward field for an inverse link and is JSON `null` for a stored link.
- Every object implicitly has the required UUID `id` field supplied by
  `ObjectType::new`. Include it exactly once in `implicit_fields`, even when
  `declared_fields` is empty. Its scalar snapshot has `unique: false`, matching
  the catalog's uniqueness flag; SQLite primary-key uniqueness is unchanged.
  The field remains implicit and cannot be declared by the schema author.
- Internal object and field IDs, source formatting and comments, physical
  SQLite names, version IDs, version numbers, and applied timestamps are excluded.

Emit no byte-order mark, insignificant whitespace, or trailing newline. Empty
arrays remain present. Preserve names exactly, without Unicode normalization
or case folding. Escape quotes and backslashes as `\"` and `\\`. Use `\b`,
`\t`, `\n`, `\f`, and `\r` for their control characters and lowercase `\u00xx`
for the remaining U+0000 through U+001F characters. Emit all other characters
directly as UTF-8, including `/` and non-ASCII characters.

For example, a catalog containing only an empty `User` type is encoded as:

```json
{"format_version":1,"objects":[{"name":"User","declared_fields":[],"implicit_fields":[{"name":"id","kind":"scalar","scalar_type":"uuid","cardinality":"required","unique":false}]}]}
```

The code block's line ending is not part of the snapshot. These rules define
a Gelite-specific canonical representation, not full RFC 8785 compliance.
Changes to the encoding or implicit semantics require a new format version;
readers must reject unsupported format versions rather than reinterpret them.

#### Checksum and application values

- `checksum` is SHA-256 of the exact stored snapshot UTF-8 bytes, including
  `format_version`, encoded as 64 lowercase hexadecimal characters without a
  prefix. Hash the stored representation, not a parsed and reserialized copy.
- `version_id` is a newly generated UUID v4 in lowercase, hyphenated
  `8-4-4-4-12` notation. It identifies an application attempt, not schema content.
- `applied_at` uses RFC 3339 UTC notation with uppercase `T` and `Z` and exactly
  three fractional second digits: `YYYY-MM-DDTHH:MM:SS.sssZ`. Truncate finer
  precision to milliseconds. It records the caller's application-attempt time,
  not the exact commit completion time.
- `version_number` is a positive signed 64-bit integer that defines application
  order within a database. Initial application stores `1`. Migration
  application uses the next number after the verified latest version and
  inserts it in the same write transaction as its schema changes. Reject
  overflow rather than wrapping.

Read the latest stored version with `ORDER BY version_number DESC LIMIT 1`.
An empty version table has no latest version. Do not require the table to
contain exactly one row during lookup. Neither UUIDs nor timestamps define
migration order.

Databases created before `version_number` was added require an explicit metadata
migration or recreation. Automatic upgrades are not implemented; do not infer
historical order from UUIDs, timestamps, or SQLite rowids.

Checking the snapshot checksum detects a mismatch but does not authenticate a
record against an actor who can rewrite both values. Verifying the logical
schema additionally requires comparing the canonical snapshot of the loaded catalog with the
stored snapshot; this does not audit physical SQLite DDL.

#### Native version verification

`NativeSQLiteRunner::verify_schema_version` verifies the latest stored version
in a single read transaction, without reading the original schema source:

1. Read the highest numbered row, rejecting a missing row or invalid version number.
2. Hash the exact stored snapshot bytes and compare with the stored checksum.
3. Load the logical catalog from object and field metadata, including an empty
   catalog when both tables are empty. Reject orphan fields and missing or
   malformed implicit identity metadata instead of silently reconstructing it.
4. Serialize the catalog using the current canonical format and compare the
   entire snapshot byte for byte. Unsupported formats and malformed snapshots
   cannot match this representation and are rejected.

Success and failure both end the verification transaction without changing
stored data. If the caller already has a transaction, verification fails before
reading and leaves the caller's transaction untouched. Database errors remain
`SQLiteRunnerError::ExecutionFailed`; checksum and snapshot mismatches are
`SQLiteRunnerError::SchemaVerificationFailed`.

Verification is explicit, not performed on every query. It verifies the latest
version against the current catalog, not the integrity of every historical row.

### `_engine_catalog_objects`

Stores semantic object definitions for diagnostics and diff support.
Catalog ids use SQLite `INTEGER`, which is a signed 64-bit value. The semantic
schema catalog uses the same signed integer range for object and field ids so
metadata planning does not need unsigned-to-signed conversion.

```sql
CREATE TABLE _engine_catalog_objects (
  object_id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);
```

### `_engine_catalog_fields`

Stores semantic field definitions.

```sql
CREATE TABLE _engine_catalog_fields (
  object_id INTEGER NOT NULL,
  field_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  field_kind TEXT NOT NULL,
  cardinality TEXT NOT NULL,
  scalar_type TEXT NULL,
  target_object_id INTEGER NULL,
  is_implicit INTEGER NOT NULL,
  is_unique INTEGER NOT NULL,
  inverse_field_name TEXT NULL,
  PRIMARY KEY (object_id, field_id),
  FOREIGN KEY (object_id) REFERENCES _engine_catalog_objects(object_id),
  FOREIGN KEY (target_object_id) REFERENCES _engine_catalog_objects(object_id)
);
```

Catalog field metadata uses these stored text values:

- `field_kind`: `scalar` or `link`
- `cardinality`: `optional`, `required`, or `many`
- `scalar_type`: `str`, `int64`, `float64`, `bool`, `uuid`, `datetime`, or
  `NULL` for link fields

`target_object_id` is `NULL` for scalar fields and the target
`_engine_catalog_objects.object_id` for link fields.

Boolean metadata is stored as integer values:

- `0` for false
- `1` for true

These catalog tables are engine-owned metadata, not user-facing schema tables.

## Migration Model

The migration MVP is append-only:

1. Compare desired schema catalog to current catalog
2. Generate one migration plan
3. Apply DDL inside a transaction where SQLite allows it
4. Record the migration in `_engine_schema_versions`
5. Update catalog metadata tables

The pure SQLite schema planner implements steps 1 and 2. It compares object and
field names rather than catalog ids, so declaration-only reordering produces an
empty plan. The planner emits operations in this deterministic order:

1. object tables
2. relation tables
3. added columns
4. indexes
5. object metadata
6. field metadata

Supported changes are new object types, nullable scalar fields, optional single
links, and stored multi links. A new object may contain required or unique
fields because it has no existing rows. Existing catalog ids are preserved;
new object and field ids are assigned by logical-name order after the current
maximum id.

Removing or renaming objects or fields is unsupported. Changing field kind,
scalar type, link target, cardinality, uniqueness, or inverse-link meaning is
also unsupported. Adding a required or unique field to an existing object is
rejected until an explicit backfill or validation strategy exists. These cases
return typed unsupported errors rather than a partial migration plan.
Scalar and single-link fields that lower to the same physical column name are
also rejected before the planner emits an operation.

The initial and migration planners share the same local physical table, column,
relation-table, index, and metadata insert helpers.

Schema application distinguishes a new database from an existing Gelite
database by the three engine metadata tables. If none exist, it uses the
initial schema path. If only some exist, it rejects the partial metadata rather
than treating the database as new. If all exist, it verifies and loads the
latest stored schema before planning a migration.

A migration version row contains the canonical snapshot and checksum of the
complete desired catalog, not only the added fields. Its version number is the
verified latest number plus one, checked for signed 64-bit overflow. The row is
rendered after all migration operations and is committed with the DDL and
catalog metadata through the shared schema transaction. A runtime failure
rolls all of them back.

An empty migration plan succeeds without a write transaction or a new version
row. Unsupported changes and stored-schema verification failures are reported
before the first migration DDL statement executes. Concurrent and online
migration coordination remains outside the MVP; the unique version number
prevents two attempts from recording the same successor version.

## Query Lowering Assumptions

This storage model is designed around these compiler assumptions:

- root `select` begins from one object table
- scalar fields come from direct columns
- single relations use joins on `<field>_id`
- multi relations may use secondary queries or grouped joins
- relation traversal is limited to declared `link` fields
- declared inverse links reuse forward storage; inferred inverse traversals
  are unsupported

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
- filter paths may traverse declared single-link chains such as `.author.id`

This keeps the initial lowering model tractable.

## Indexes

The MVP should create indexes for:

- every foreign key column on object tables
- `target_id` and `source_id` access on join tables

Optional future indexes can be introduced later by schema directives.

## Deletes and Referential Behavior

The MVP uses one explicit policy:

- single relations use SQLite foreign keys with `ON DELETE RESTRICT`
- join tables delete rows with `ON DELETE CASCADE` from either side

Constraint failures are returned to the caller.

## Canonical Example

For:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
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
- enum storage optimizations
- online migration strategies
- schema branching

## Declared Inverse Storage and Metadata

Inverse links create no columns, foreign keys, relation tables, or indexes.
Reverse single-link access reads the stored source table's `<field>_id` column.
Reverse multi-link access uses the existing join table with source and target
roles swapped. Existing forward indexes cover these reads.

Catalog field rows preserve an optional `inverse_field_name` text value, scoped
to `target_object_id`. It is null for stored links and scalar fields. Reloading
metadata reconstructs and validates the same logical catalog, including inverse
ownership. Legacy catalogs without this column load as stored-only catalogs;
no implicit schema upgrade or mutation is performed by reads.
