# SQLite Schema Implementation Plan

## Goal

Add the first schema application layer for Gelite's SQLite backend.

The immediate target is not a full migration engine. The first target is:

1. take a validated `schema_model::SchemaCatalog`
2. derive the SQLite object tables, relation tables, indexes, and metadata rows
3. apply them to an empty SQLite database as the initial schema
4. make the stored metadata precise enough to rebuild the same
   `schema_model::SchemaCatalog` later

This layer exists before the query runner becomes useful. Query execution needs
a catalog, and the first real catalog should come from the schema that was
applied to the database instead of from test fixtures.

## Crate To Add

Add a new crate:

```text
engine/sqlite-schema-plan
```

The crate is SQLite-specific and may depend on SQLite naming, SQLite DDL, and
the internal metadata table layout from `spec/storage-sqlite.md`.

Initial dependency direction:

```text
sqlite-schema-plan -> schema
```

Do not make `schema` depend on `sqlite-schema-plan`.
Do not make `resolver` depend on `sqlite-schema-plan`.
Do not make `sqlite-query-plan` or `sqlite-query-sqlgen` depend on `sqlite-schema-plan` until a
shared naming module exists.

`sqlite-schema-plan` stays `no_std` because it builds structured plans without
executing them. Schema execution belongs to an environment-specific runner and
does not share this requirement.

## Responsibility Boundary

### `schema` owns

- canonical in-memory object and field model
- schema validation
- deterministic object and field references
- scalar type, field kind, cardinality, and implicit `id` semantics

### `sqlite-schema-plan` owns

- mapping `schema_model::SchemaCatalog` to SQLite schema operations
- object table names
- scalar column names
- single-link foreign key column names
- multi-link join table names
- metadata table definitions
- metadata rows for object and field definitions
- initial schema apply plan
- later: catalog loading from metadata tables

### `sqlite-schema-plan` must not own

- parsing `.geli` files
- query parsing
- name resolution for queries
- `ir` construction
- select planning
- final query SQL rendering
- SQLite connection ownership
- row decoding for query results

Those belong to parser, resolver, planner, sqlgen, and the engine runtime
layer.

## Why This Comes Before The Runner

The engine runtime will eventually execute this pipeline:

```text
query text
-> query-parser
-> load current schema_model::SchemaCatalog
-> resolver
-> sqlite-query-plan
-> sqlite-query-sqlgen
-> execute SQL
```

The missing dependency is the real catalog. Loading the catalog is only useful
after the database has a catalog to load. The first milestone is therefore:

```text
schema_model::SchemaCatalog
-> SQLite initial schema apply
-> object tables + metadata tables
```

After that, the runtime can replace fixture catalogs with metadata-backed
catalog loading.

## Existing Crates That Need Changes

### `schema`

`sqlite-schema-plan` needs to inspect more than field names.

Likely additions:

```rust
impl ScalarField {
    pub fn scalar_type(&self) -> ScalarType
    pub fn cardinality(&self) -> SingleCardinality
    pub fn is_implicit(&self) -> bool
    pub fn is_unique(&self) -> bool
}

impl LinkField {
    pub fn cardinality(&self) -> Cardinality
}

impl Field {
    pub fn as_scalar(&self) -> Option<&ScalarField>
    pub fn as_link(&self) -> Option<&LinkField>
}

impl ObjectType {
    pub fn fields(&self) -> impl Iterator<Item = &Field>
}
```

The exact API can be smaller if tests need less. The key rule is that
`sqlite-schema-plan` should not parse field names to infer kind, scalar type, or
cardinality.

`ObjectType::fields()` should expose implicit fields followed by declared
fields, matching the deterministic reference order already used by
`SchemaCatalog::find_field_ref`.

### `sqlite-query-plan`

`sqlite-query-plan` currently owns physical naming rules for select planning:

- object type name to table name
- scalar field name to column name
- single-link field name to `<field>_id`
- selected single-link alias rules

`sqlite-schema-plan` will need the same naming rules for DDL. Duplicating them is
acceptable for one or two tests, but it should be treated as a short-lived
rule. Before adding more physical naming behavior, move the shared rules into
one backend-specific location.

Candidate options:

- keep naming functions in `sqlite-query-plan` and later extract them
- introduce `engine/sqlite-naming`
- put naming in `sqlite-schema-plan` and make `sqlite-query-plan` depend on it later

Do not create `sqlite-naming` before a test proves duplication is causing real
drift. For the first `sqlite-schema-plan` tests, local naming helpers are acceptable.

### `sqlite-query-sqlgen`

No immediate changes are required. It renders select SQL from `sqlite-query-plan`.
DDL rendering should not be added to this crate. `sqlite-query-sqlgen` owns runtime
query SQL generated from `sqlite-query-plan`, not schema-management SQL.

### `sqlite-schema-sqlgen`

This crate renders SQL for schema application. It depends on `sqlite-schema-plan`
and consumes `SQLiteSchemaPlan`, `SQLiteTablePlan`, `SQLiteIndexPlan`, and
metadata insert plans.

Initial dependency direction:

```text
sqlite-schema-sqlgen -> sqlite-schema-plan -> schema
```

It should render:

- `CREATE TABLE` for metadata, object, and relation tables
- `CREATE INDEX` for planned indexes
- metadata `INSERT` statements for engine catalog rows

It should not render runtime query SQL. SELECT and later user-data DML stay in
`sqlite-query-sqlgen`, which consumes `sqlite-query-plan`.

### `query-parser`

No immediate changes are required for initial schema apply from Rust-built
fixtures.

Later, a schema parser will be needed:

```text
.geli source -> schema AST -> schema_model::SchemaCatalog
```

That should be a separate parser path from `.geliql` query parsing.

### `tools/repl`

The REPL should remain query-focused. It should not become the primary user
interface for schema DDL or migration application.

Later flow:

```text
repl startup
-> open SQLite database
-> load schema_model::SchemaCatalog from metadata
-> use that catalog for query resolution
```

Before catalog loading exists, the REPL can keep using its current fixture
catalog.

Schema changes should be exposed first as command-style workflows:

```text
gelite schema plan path/to/schema.geli --database app.db
gelite schema apply path/to/schema.geli --database app.db
```

`schema plan` should parse `.geli`, build a `schema_model::SchemaCatalog`, create a
`sqlite_schema_plan::SQLiteSchemaPlan`, and render SQL through
`sqlite-schema-sqlgen` without applying it.

`schema apply` should use the same planning and rendering path, then execute
the statements in a transaction once the SQLite runtime boundary exists.

The REPL may later expose schema commands as meta commands:

```text
:schema plan path/to/schema.geli
:schema apply path/to/schema.geli
```

Those meta commands should delegate to the same command implementation instead
of maintaining a separate DDL execution path inside the REPL.

## `sqlite-schema-plan` Public API Shape

Start with pure planning, not connection execution:

```rust
pub fn plan_initial_schema(catalog: &schema_model::SchemaCatalog) -> SQLiteSchemaPlan
```

The returned plan should be inspectable by tests before any SQL is executed.

Suggested first structure:

```rust
pub struct SQLiteSchemaPlan {
    metadata_tables: Vec<SQLiteTablePlan>,
    object_tables: Vec<SQLiteTablePlan>,
    relation_tables: Vec<SQLiteTablePlan>,
    indexes: Vec<SQLiteIndexPlan>,
    catalog_object_rows: Vec<SQLiteCatalogObjectRow>,
    catalog_field_rows: Vec<SQLiteCatalogFieldRow>,
}
```

This structure keeps DDL and catalog metadata together without requiring a
SQLite binding.

Catalog rows should stay semantic. They record the schema meaning that must be
stored, not the exact SQLite statement used to store it. Before rendering SQL,
convert those rows into insert plans:

```rust
pub struct SQLiteInsertPlan {
    table_name: String,
    columns: Vec<String>,
    values: Vec<SQLiteValuePlan>,
}

pub enum SQLiteValuePlan {
    Integer(i64),
    Text(String),
    Null,
}
```

The first implementation can use one `SQLiteInsertPlan` per row. A later
renderer may batch rows into multi-row `INSERT` statements if execution tests
show that the simpler shape is too slow. The semantic row types should not be
removed when insert plans are added; tests should still be able to inspect
`SQLiteCatalogObjectRow` and `SQLiteCatalogFieldRow` directly.

Use separate conversion functions before adding a broad rendering API:

```rust
pub fn plan_catalog_object_inserts(plan: &SQLiteSchemaPlan) -> Vec<SQLiteInsertPlan>

pub fn plan_catalog_field_inserts(plan: &SQLiteSchemaPlan) -> Vec<SQLiteInsertPlan>
```

These functions are SQLite-specific DML planning, but they still do not own a
database connection. They translate structured metadata rows into a stable
column order and bindable values.

The first SQL rendering API in `sqlite-schema-sqlgen` can then be:

```rust
pub fn render_initial_schema(plan: &SQLiteSchemaPlan) -> Vec<String>
```

This returns DDL and metadata insert SQL in deterministic order. It may call the
insert planning functions internally, but it should not execute the statements.
The renderer must not inline user-controlled text directly into SQL literals
once execution is introduced. Prefer rendering placeholders and carrying values
through a bind-value structure when the SQLite binding API is known.

`sqlite-schema-sqlgen` should start with small render functions instead of a
full schema renderer:

```rust
pub fn render_create_table(table: &sqlite_schema_plan::SQLiteTablePlan) -> String

pub fn render_create_index(index: &sqlite_schema_plan::SQLiteIndexPlan) -> String

pub fn render_insert(insert: &sqlite_schema_plan::SQLiteInsertPlan) -> RenderedInsert
```

`RenderedInsert` should keep SQL text and values separate once execution is in
scope:

```rust
pub struct RenderedInsert {
    sql: String,
    values: Vec<sqlite_schema_plan::SQLiteValuePlan>,
}
```

The first renderer tests may compare SQL strings directly. When execution is
added, metadata inserts should render placeholders and carry values separately
instead of embedding text values into SQL.

Execution can be added later through a runtime-specific runner:

```rust
pub fn apply_initial_schema(
    connection: &mut impl SQLiteExecutor,
    catalog: &schema_model::SchemaCatalog,
) -> Result<(), SQLiteSchemaApplyError>
```

Do not add this executor abstraction until an existing runner trait is proven
insufficient by schema apply, prepared statements, bind values, stepping, or
result access.

## Initial Schema Plan Details

### Planning Layers

Keep these layers separate even while they live in the same crate:

```text
schema_model::SchemaCatalog
-> SQLiteSchemaPlan
-> SQLiteInsertPlan / SQLite DDL statement plans
-> rendered SQL + bind values
-> SQLite execution
```

`SQLiteSchemaPlan` is the contract tested first. It answers which tables,
columns, constraints, and semantic metadata rows are required.

`SQLiteInsertPlan` is the first DML planning layer. It answers which metadata
table receives which columns and values.

The SQL renderer is a serialization layer. It should not rediscover schema
semantics by looking at object names or fields again. If the renderer needs
information that is not present in `SQLiteSchemaPlan` or `SQLiteInsertPlan`,
that is evidence that the planning layer is missing data.

### DDL Rendering Rules

`sqlite-schema-sqlgen` renders from `sqlite-schema-plan` plan types only. It must not
accept `schema_model::SchemaCatalog` directly.

The first renderer can produce single-line SQL strings. Stable output matters
more than pretty formatting because migration previews and tests need
byte-identical results for the same plan.

Column rendering rules:

- `SQLiteAffinity::Text` -> `TEXT`
- `SQLiteAffinity::Integer` -> `INTEGER`
- `SQLiteAffinity::Real` -> `REAL`
- nullable columns render `NULL`
- non-nullable columns render `NOT NULL`
- column-level primary keys render `PRIMARY KEY`
- column-level unique constraints render `UNIQUE`

Table-level constraint rendering rules:

- `SQLitePrimaryKeyPlan` renders as `PRIMARY KEY (column_a, column_b)`
- each `SQLiteForeignKeyPlan` renders as
  `FOREIGN KEY (column) REFERENCES target_table(target_column)`

Index rendering rules:

- non-unique indexes render `CREATE INDEX`
- unique indexes render `CREATE UNIQUE INDEX`
- index column order must match `SQLiteIndexPlan::column_names()`

Identifier quoting is deferred for the MVP. Current planner-generated names
come from controlled naming rules and the schema validator rejects names that
would collide with reserved scalar type names. Before accepting arbitrary user
identifiers in rendered SQL, add a dedicated identifier quoting function and
tests for quotes, spaces, keywords, and embedded quote characters.

DDL statement order for full initial schema rendering:

1. metadata tables
2. object tables
3. relation tables
4. indexes
5. metadata inserts

This order ensures that foreign keys reference existing tables and metadata
inserts target existing metadata tables.

### Metadata Tables

Always create these tables first:

- `_engine_schema_versions`
- `_engine_catalog_objects`
- `_engine_catalog_fields`

The definitions should match `spec/storage-sqlite.md`.

The first implementation may omit `_engine_schema_versions` inserts if there is
no migration identity yet, but the table itself should exist.

### Object Tables

For every object type:

- create one table
- add `id TEXT PRIMARY KEY`
- add scalar fields as direct columns
- add unique scalar fields as `UNIQUE`
- add single-link fields as `<field>_id TEXT`
- add foreign key constraints for single links
- do not add columns for multi links

Required fields become `NOT NULL`.
Optional fields become nullable.
Optional unique scalar fields stay nullable. SQLite allows multiple `NULL`
values under a `UNIQUE` constraint, and Gelite treats this as the MVP meaning:
only present values must be unique.

The implicit `id` field must be generated from catalog semantics, not from a
declared schema field.

### Multi-Link Tables

For every `Cardinality::Many` link:

- create one join table named `<source_table>__<field_name>`
- add `source_id TEXT NOT NULL`
- add `target_id TEXT NOT NULL`
- add `position INTEGER NULL`
- add primary key `(source_id, target_id)`
- add foreign keys to the source and target object tables

The first query planner still does not support selecting multi links. Creating
the storage table is still correct because schema application should match the
storage spec, not the current select subset.

### Indexes

Create indexes for:

- every single-link foreign key column
- `source_id` on multi-link tables
- `target_id` on multi-link tables

Index naming should be deterministic.

### Metadata Rows

For every object type, insert one `_engine_catalog_objects` row:

- `object_id` as the deterministic `schema_model::ObjectTypeId` integer value
- `name`

For every field, insert one `_engine_catalog_fields` row:

- `object_id`
- `field_id`
- `name`
- `field_kind`
- `cardinality`
- `scalar_type`
- `target_object_id`
- `is_implicit`
- `is_unique`

Metadata rows must include implicit `id` fields. The resolver and future
catalog loader need the same semantic catalog that the in-memory schema layer
uses.

The MVP stores object and field ids as signed 64-bit integers, not UUIDs.
`object_id` matches the deterministic `schema_model::ObjectTypeId(i64)` value.
`field_id` matches the deterministic `schema_model::FieldId(i64)` value inside its
owning object type, so `_engine_catalog_fields` uses `(object_id, field_id)` as
its primary key. The schema layer uses `i64` instead of `u64` because SQLite
`INTEGER` values are signed 64-bit values. Keeping the Rust id type aligned with
SQLite avoids fallible unsigned-to-signed conversions in metadata insert
planning. Stable UUIDs can be revisited when rename-aware migrations need
persistent identities across schema snapshots.

### Metadata Insert Plans

Catalog object inserts should target `_engine_catalog_objects` with this column
order:

```text
object_id
name
```

Catalog field inserts should target `_engine_catalog_fields` with this column
order:

```text
object_id
field_id
name
field_kind
cardinality
scalar_type
target_object_id
is_implicit
is_unique
```

The insert planner must preserve the row order from `SQLiteSchemaPlan`.
Deterministic order matters because rendered schema application scripts should
be byte-stable for the same catalog.

Use integer values for boolean metadata fields in SQLite-facing plans:

```text
false -> 0
true  -> 1
```

Use `NULL` for fields that do not apply:

- scalar fields have `target_object_id = NULL`
- link fields have `scalar_type = NULL`

Use these text values for enum metadata:

```text
SQLiteCatalogFieldKind::Scalar -> "scalar"
SQLiteCatalogFieldKind::Link   -> "link"

schema_model::Cardinality::Optional -> "optional"
schema_model::Cardinality::Required -> "required"
schema_model::Cardinality::Many     -> "many"

schema_model::ScalarType::Str      -> "str"
schema_model::ScalarType::Int64    -> "int64"
schema_model::ScalarType::Float64  -> "float64"
schema_model::ScalarType::Bool     -> "bool"
schema_model::ScalarType::Uuid     -> "uuid"
schema_model::ScalarType::DateTime -> "datetime"
```

The semantic row types can expose Rust booleans and enums. The insert plan is
where those values become SQLite-compatible values.

## Scalar Type Mapping

Use the storage spec mapping:

```text
schema_model::ScalarType::Str      -> TEXT
schema_model::ScalarType::Int64    -> INTEGER
schema_model::ScalarType::Float64  -> REAL
schema_model::ScalarType::Bool     -> INTEGER
schema_model::ScalarType::Uuid     -> TEXT
schema_model::ScalarType::DateTime -> TEXT
```

The first implementation should model this as a small function in
`sqlite-schema-plan`:

```rust
fn sqlite_affinity(scalar_type: schema_model::ScalarType) -> SQLiteAffinity
```

Use an enum internally instead of raw strings if tests need to inspect the
mapping.

## Initial Test Plan

### 1. `initial_schema_plan_creates_metadata_tables`

Input: an empty or minimal catalog.

Assert:

- `_engine_schema_versions` is present
- `_engine_catalog_objects` is present
- `_engine_catalog_fields` is present

This test fixes the internal metadata table contract before user tables.

### 2. `initial_schema_plan_creates_object_table_for_scalar_fields`

Input:

```text
type User {
  required name: str
  age: int64
}
```

Assert:

- table name is `user`
- `id` is `TEXT PRIMARY KEY`
- `name` is `TEXT NOT NULL`
- `age` is `INTEGER NULL`

This test checks scalar mapping and cardinality mapping.

### 2a. `initial_schema_plan_marks_required_unique_scalar_field`

Input:

```text
type User {
  required unique email: str
}
```

Assert:

- `email` is `TEXT NOT NULL UNIQUE`

This test fixes the required unique scalar mapping.

### 2b. `initial_schema_plan_allows_optional_unique_scalar_field`

Input:

```text
type User {
  unique nickname: str
}
```

Assert:

- `nickname` is `TEXT NULL UNIQUE`

This test documents the MVP null semantics for optional unique fields. Gelite
allows multiple rows with `nickname = null`; duplicate non-null nicknames are
rejected by SQLite.

### 3. `initial_schema_plan_creates_single_link_foreign_key_column`

Input:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

Assert:

- `post` table has `author_id TEXT NOT NULL`
- `author_id` references `user(id)`
- an index exists for `post.author_id`

This test fixes single-link physical storage.

### 4. `initial_schema_plan_creates_optional_single_link_column`

Input:

```text
type Post {
  link author: User
}
```

Assert:

- `author_id` is nullable
- the foreign key still exists

This test prevents conflating optional links with missing storage.

### 4a. `initial_schema_plan_marks_required_unique_single_link_column`

Add this after scalar uniqueness and basic single-link storage are stable.

Input:

```text
type User {
  required name: str
}

type Profile {
  required unique link user: User
}
```

Assert:

- `profile` table has `user_id TEXT NOT NULL UNIQUE`
- `user_id` references `user(id)`

This test fixes the first schema-enforced one-to-one relation form. The
constraint means each `Profile` must reference one `User`, and the same `User`
cannot be referenced by more than one `Profile`.

### 4b. `initial_schema_plan_marks_optional_unique_single_link_column`

Input:

```text
type User {
  required name: str
}

type Profile {
  unique link user: User
}
```

Assert:

- `profile` table has `user_id TEXT NULL UNIQUE`
- `user_id` references `user(id)`

This uses the same null semantics as optional unique scalar fields: duplicate
non-null target ids are rejected, but multiple rows without a target are
allowed.

### 5. `initial_schema_plan_creates_multi_link_join_table`

Input:

```text
type User {
  multi link posts: Post
}
```

Assert:

- join table is `user__posts`
- it has `source_id`, `target_id`, and `position`
- primary key is `(source_id, target_id)`
- source and target foreign keys exist
- source and target indexes exist

This test fixes storage for a feature that query planning has not implemented
yet.

### 6. `initial_schema_plan_records_catalog_object_rows`

Input: a two-type catalog.

Assert:

- object rows preserve deterministic object ids
- names match the canonical catalog
- row order is deterministic

This test is the first step toward metadata-backed catalog loading.

### 7. `initial_schema_plan_records_catalog_field_rows`

Input: a catalog with scalar, single-link, multi-link, and implicit `id`.

Assert:

- implicit `id` is recorded with `is_implicit = true`
- scalar fields record `scalar_type`
- link fields record `target_object_id`
- field kind and cardinality values are stable

This test fixes the metadata format that catalog loading will later consume.

### 8. `initial_schema_plan_can_plan_catalog_object_inserts`

Input: a two-type catalog.

Assert:

- one insert plan is produced per object row
- every insert targets `_engine_catalog_objects`
- columns are exactly `object_id, name`
- values preserve deterministic object ids and names

This test checks DML planning without requiring SQL string rendering.

### 9. `initial_schema_plan_can_plan_catalog_field_inserts`

Input: the same catalog used by `initial_schema_plan_records_catalog_field_rows`.

Assert:

- one insert plan is produced per field row
- every insert targets `_engine_catalog_fields`
- columns match the metadata table definition
- scalar fields use `scalar_type` and `target_object_id = NULL`
- link fields use `scalar_type = NULL` and `target_object_id`
- `is_implicit` and `is_unique` are represented as integer values

This test fixes the boundary between semantic metadata rows and SQLite-facing
DML values.

### 10. `render_create_table_for_catalog_fields_uses_composite_primary_key`

Input: an empty catalog.

Assert:

- rendered DDL for `_engine_catalog_fields` includes
  `PRIMARY KEY (object_id, field_id)`
- both foreign keys are rendered
- `is_unique` is rendered as `INTEGER NOT NULL`

This test should be added before rendering every table type. The catalog fields
table has the most important metadata constraints and will catch mistakes in
table-level constraint rendering.

### 11. `render_create_index_for_single_link_foreign_key_index`

Input: a `SQLiteIndexPlan` produced by
`initial_schema_plan_creates_single_link_foreign_key_index`.

Assert:

- rendered SQL is `CREATE INDEX post__author_id_idx ON post (author_id)`
- unique is not rendered for non-unique indexes

This test fixes the index renderer before full schema rendering.

### 12. `render_catalog_object_insert_uses_placeholders`

Input: the first insert from `plan_catalog_object_inserts`.

Assert:

- rendered SQL targets `_engine_catalog_objects`
- SQL uses placeholders for `object_id` and `name`
- values remain available separately

This test prevents metadata insert rendering from embedding text values into
SQL strings.

### 13. `render_initial_schema_outputs_deterministic_sql`

Input: a fixed catalog.

Assert:

- SQL statements are emitted in dependency-safe order
- metadata tables are created before metadata inserts
- object tables are created before link tables or indexes that reference them
- repeated planning renders byte-identical SQL

This test keeps generated migrations stable.

### 14. `catalog_can_round_trip_through_metadata_rows`

This is not the first test. Add it after metadata rows are stable.

Input:

```text
schema_model::SchemaCatalog
-> SQLiteSchemaPlan
-> metadata rows
-> schema_model::SchemaCatalog
```

Assert:

- rebuilt catalog equals the original catalog
- object and field references remain deterministic

This test is the bridge to query execution without fixture catalogs.

## Initial Implementation Sequence

1. Create `engine/sqlite-schema-plan` with `schema` as its only dependency.
2. Add it to the workspace.
3. Define inspectable plan structs without SQL strings.
4. Implement metadata table planning.
5. Implement object table planning for scalar fields.
6. Add the minimal `schema` getters needed by the tests.
7. Implement single-link column and foreign key planning.
8. Implement scalar unique column planning.
9. Implement unique single-link column planning for one-to-one constraints.
10. Implement multi-link join table planning.
11. Implement catalog metadata row planning.
12. Add metadata insert planning for catalog object rows.
13. Add metadata insert planning for catalog field rows.
14. Add `render_create_table` in `sqlite-schema-sqlgen` for metadata tables.
15. Add `render_create_table` coverage for object tables and relation tables.
16. Add `render_create_index` in `sqlite-schema-sqlgen`.
17. Add metadata insert rendering with placeholders and separate values.
18. Add deterministic full initial schema rendering.
19. Add command-style `schema plan` output once `.geli` parsing exists.
20. Add `sqlite-runner` with the smallest native driver wrapper needed for
    schema apply.
21. Add command-style `schema apply` once runtime execution exists.
22. Add REPL schema meta commands that delegate to the command implementation.
23. Implement metadata-to-`schema_model::SchemaCatalog` loading only after metadata
    rows are tested.

## Open Decisions

### SQLite driver boundary

Only compiler engine layers require `no_std`. SQLite runners are
environment-specific runtime implementations behind the existing runner
traits. Applications may implement those traits with another driver without a
registry or public adapter framework.

Gelite plans to provide native and WASM runners. Each official runner should
use a stable API appropriate to its environment; the two runners do not need
to share one binding crate. `NativeSQLiteRunner` uses `rusqlite`, while the WASM
backend remains a separate implementation boundary.

Driver choices should satisfy the behavior owned by the runner contracts:

- native in-memory database support for tests and CLI
- prepared statements
- integer, text, and null bind values
- deterministic stepping and finalization behavior
- usable error reporting

`sqlite-schema-plan`, `sqlite-schema-sqlgen`, `sqlite-query-plan`, and `sqlite-query-sqlgen`
must not depend directly on any SQLite binding. The execution wrapper belongs
in `sqlite-runner`.

### `sqlite-runner` crate

Add a runtime crate after schema parsing and schema SQL rendering are stable:

```text
engine/sqlite-runner
```

Initial dependency direction:

```text
sqlite-runner
  -> sqlite-schema-sqlgen
  -> sqlite-schema-plan
  -> schema
```

The first version of the crate should define binding-neutral runner contracts
before choosing a concrete backend:

```rust
pub trait SQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError>;

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[sqlite_schema_plan::SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError>;
}
```

Concrete backends can then implement the trait:

```text
sqlite-runner/src/backend/native.rs
sqlite-runner/src/backend/wasm.rs
```

The native backend uses `rusqlite`. Its types stay private to
`NativeSQLiteRunner` and must not leak into planning or command crates.

The first runner target is schema application, not query execution:

```text
schema source
-> schema-parser
-> schema_model::SchemaCatalog
-> sqlite-schema-plan::plan_initial_schema
-> sqlite-schema-sqlgen::render_initial_schema
-> sqlite-runner
-> SQLite database
```

Schema apply should be implemented on top of the trait:

```rust
pub fn apply_schema_statements(
    runner: &mut impl SQLiteRunner,
    statements: &[sqlite_schema_sqlgen::RenderedSchemaStatement],
) -> Result<(), SQLiteRunnerError>
```

This keeps command orchestration independent from the selected SQLite binding.
The native backend may still expose an owned connection wrapper for tests and
CLI setup, but `apply_schema_statements` should only need the trait.

The first implementation should support only:

- executing raw DDL statements from `RenderedSchemaStatement::Sql`
- preparing metadata insert statements from `RenderedSchemaStatement::Insert`
- binding `sqlite_schema_plan::SQLiteValuePlan::Integer`
- binding `sqlite_schema_plan::SQLiteValuePlan::Text`
- binding `sqlite_schema_plan::SQLiteValuePlan::Null`
- finalizing statements without leaking SQLite resources

Do not add SELECT query execution, row decoding, result shaping, migrations, or
catalog loading in the first runner step. Those require additional contracts
around statement lifetimes and returned values.

Gelite code should not return borrowed SQLite values across a step or statement
finalization boundary.

Backend evaluation order:

1. Check whether the candidate can be added through crates.io or a pinned git
   dependency.
2. Build a native in-memory smoke test.
3. Execute DDL.
4. Execute prepared metadata inserts with integer, text, and null values.
5. Query SQLite metadata or rows back using owned Rust values.
6. Keep the native dependency private so it does not constrain a WASM runner.

Do not implement CLI `schema apply` until at least the native backend satisfies
steps 1-5. Do not start the browser demo until a separate WASM runner satisfies
the same execution contracts.

### Runner backend verification plan

The next runner step is a backend spike, not a query execution feature. The
goal is to prove that one concrete SQLite binding can satisfy the existing
`SQLiteRunner` trait without changing planner or SQL generator crates.

The crate keeps the binding-neutral contract available without an official
backend. Official backends are selected independently:

```toml
[features]
default = []
native = ["dep:rusqlite"]
wasm = []
```

The native feature uses `rusqlite` with its bundled SQLite build. This removes
direct SQLite C calls and repository-owned linker configuration. The empty
WASM feature continues to expose only the existing placeholder until a stable
WASM driver is selected in separate work.

The first backend module should be feature-gated:

```text
sqlite-runner/src/native.rs
```

`lib.rs` should expose it only behind the feature:

```rust
#[cfg(feature = "native")]
pub mod native;
```

The first native type should own its SQLite connection:

```rust
pub struct NativeSQLiteRunner {
    // concrete connection type stays private
}

impl NativeSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError>;
}
```

The concrete connection type must not appear in `sqlite-schema-plan`,
`sqlite-schema-sqlgen`, `sqlite-query-plan`, `sqlite-query-sqlgen`,
`schema-parser`, `query-parser`, or command-layer public APIs.

Candidate acceptance gates:

1. `cargo test -p sqlite-runner --no-default-features --offline` still builds
   and runs the binding-neutral tests.
2. `cargo test -p sqlite-runner --features native --offline` can open an
   in-memory database.
3. The native backend can execute a raw DDL statement through
   `SQLiteRunner::execute`.
4. The native backend can execute a prepared insert through
   `SQLiteRunner::execute_with_values`.
5. The native backend binds `SQLiteValuePlan::Integer`,
   `SQLiteValuePlan::Text`, and `SQLiteValuePlan::Null` without string
   interpolation.
6. The native backend can read back owned values for test inspection. Borrowed
   SQLite values must not escape a step/finalize boundary.
7. A rendered initial schema can be applied to an in-memory database, and the
   metadata tables, object tables, and at least one catalog row can be verified.
8. The native dependency does not constrain the separate WASM backend choice.

If a candidate fails gates 2-7, do not adapt the planner or renderer to fit
the candidate. Replace the backend candidate or add a narrower adapter inside
`sqlite-runner`. A backend limitation must not leak into schema/query planning
unless the limitation is part of SQLite itself.

Native backend tests should be added in this order:

1. `native_runner_can_open_in_memory_database`
   - construct `NativeSQLiteRunner::open_in_memory()`
   - assert the result is `Ok`

2. `native_runner_can_execute_create_table_statement`
   - open an in-memory database
   - call `execute("CREATE TABLE post (id TEXT PRIMARY KEY)")`
   - query `sqlite_master` or equivalent metadata using owned Rust values
   - assert the `post` table exists

3. `native_runner_can_execute_insert_statement_with_bind_values`
   - create a small table
   - call `execute_with_values` with integer, text, and null values
   - read the row back
   - assert each stored value matches the bound value

4. `native_runner_can_apply_rendered_initial_schema`
   - build `post_catalog()`
   - call `plan_initial_schema`
   - call `render_initial_schema`
   - call `apply_schema_statements`
   - assert `_engine_schema_versions`, `_engine_catalog_objects`,
     `_engine_catalog_fields`, and `post` exist
   - assert `_engine_catalog_objects` contains the `Post` row

5. `native_runner_reports_execution_errors`
   - execute invalid SQL
   - assert a `SQLiteRunnerError` is returned
   - keep the initial error shape binding-neutral

Transaction support should not be part of the first native spike. Add it after
the schema apply smoke test passes, because rollback behavior needs its own
contract and tests.

WASM verification should not block native runner work. Before browser demo
work starts, select a stable WASM SQLite API and implement the same runner
contracts without changing native consumers.

Current WASM compile gate:

```text
rustup target add wasm32-unknown-unknown
cargo check -p sqlite-runner \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features wasm
```

This gate currently passes with the placeholder `WasmSQLiteRunner`. It proves
only that `sqlite-runner` can compile for `wasm32-unknown-unknown` without the
native SQLite link step. It does not prove that a browser SQLite database can
open or execute SQL yet.

Initial runner tests:

1. `runner_can_execute_create_table_statement`
   - open an in-memory database
   - execute one `RenderedSchemaStatement::Sql`
   - query SQLite metadata to prove the table exists

2. `runner_can_execute_insert_statement_with_bind_values`
   - create `_engine_catalog_objects`
   - execute a `RenderedSchemaStatement::Insert`
   - query the row back and verify integer and text values

3. `runner_can_apply_rendered_initial_schema`
   - parse a small `.geli` source
   - plan and render the initial schema
   - apply all statements in order
   - verify object tables and metadata tables exist

These tests should be written before adding command-line `schema apply`.

### Schema SQL ownership

`sqlite-schema-sqlgen` renders schema-management SQL from `sqlite-schema-plan`
plans. This keeps `sqlite-schema-plan` as a pure planner and keeps `sqlite-query-sqlgen`
focused on runtime query SQL.

The split is by pipeline, not by SQL keyword:

```text
query path:
ir -> sqlite-query-plan -> sqlite-query-sqlgen

schema path:
schema -> sqlite-schema-plan -> sqlite-schema-sqlgen
```

Both paths may eventually render `INSERT` statements. User-data inserts belong
to `sqlite-query-sqlgen`; engine metadata inserts belong to `sqlite-schema-sqlgen`.

### Shared SQLite naming

Initial and migration schema planning share local helpers for physical object
tables, stored columns, relation tables, indexes, and metadata inserts. This
keeps both schema-planning paths aligned without adding a separate naming
crate. Cross-crate extraction with `sqlite-query-plan` remains deferred until a
test or bug shows drift.

### Catalog diff planning

`plan_schema_migration` compares current and desired catalogs by logical object
and field names. It performs a current-to-desired pass for removals and semantic
changes, then a desired-to-current pass for additions. Both passes sort names
before using `try_for_each`, making the first typed unsupported error and the
generated operation order independent of declaration order.

The append-only planner supports new objects, nullable scalar fields, optional
single links, and stored multi links. It preserves current metadata ids and
assigns ids for additions in logical-name order. Required or unique additions
to existing objects and changes requiring removal or rebuild are rejected.
Migration rendering and execution stay outside this pure planner.

`sqlite-schema-sqlgen` renders migration operations in planner order. Added
single-link columns use an inline `REFERENCES` clause because SQLite cannot add
a table-level foreign key constraint after table creation. Metadata values and
the version row remain bound INSERT values.

### Migration identity

`_engine_schema_versions` requires:

- `version_id`
- `checksum`
- `applied_at`
- `schema_snapshot`
- `version_number`

Initial schema plans insert the canonical snapshot and checksum with version
number `1`. The native runner selects the highest version number when reading
the latest record. UUIDs and timestamps are not ordering keys. A non-empty
migration records the complete desired catalog at the verified latest version
number plus one. Number overflow is an error.

The runner exposes one verified stored-schema read containing the logical
catalog and latest version number. No engine metadata tables means a new
database; a partial set is invalid. Existing metadata is checksum- and
catalog-verified before the command plans a migration.

### Transaction boundary

The command renders every migration operation and appends its version INSERT,
then passes the complete statement list to the existing transactional schema
runner. DDL, catalog metadata, and the version row therefore commit or roll
back together. An empty plan returns before opening a write transaction.

## Deferred Work

- table rebuild migrations
- schema parser for `.geli`
- concurrent or online migration coordination
- user-declared indexes and constraints
