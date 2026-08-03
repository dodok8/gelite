# SQLite Plan Implementation Plan

## Goal

Implement the next compiler layer after resolver:

```text
query_ir::SelectQuery -> sqlite-query-plan::SQLiteSelectPlan
```

This stage turns backend-independent Semantic IR into a SQLite-specific,
structured execution plan. It must not generate SQL strings yet.

The first goal is to prove that the resolved query shape can be lowered into
deterministic physical tables, columns, joins, ordering, pagination, and result
shape metadata.

## Current Starting Point

The project currently has:

- `schema`
  - object types
  - scalar fields
  - link fields
  - cardinality
  - implicit `id`
  - `ObjectTypeRef`
  - `FieldRef`
- `query-ast`
  - parser-free query representation
  - select shape
  - filter compare expressions
  - order expressions
  - limit and offset
- `ir`
  - resolved select query
  - resolved shape
  - resolved shape field
  - resolved field value expressions
  - compare expressions
  - order expressions
- `resolver`
  - root object resolution
  - scalar and link shape resolution
  - nested single-link shape resolution
  - filter path resolution for scalar fields
  - order path resolution for scalar fields
  - limit and offset passthrough
- `repl`
  - lives under `tools/repl`
  - detailed smoke runner for `query-parser + resolver + sqlite-query-plan + sqlite-query-sqlgen`

The next layer should consume `ir`, not `query-ast`.

## Crate Plan

Add a new crate:

```text
engine/sqlite-query-plan
```

Workspace dependency direction:

```text
sqlite-query-plan -> ir
sqlite-query-plan -> schema
```

Do not make `ir` depend on `sqlite-query-plan`.
Do not make `resolver` depend on `sqlite-query-plan`.
Do not make `sqlite-query-plan` depend on `query-ast`.

`sqlite-query-plan` is backend-specific. It may know SQLite physical naming rules,
table aliases, column names, join conditions, and result slot layout.

## Responsibility Boundary

### `ir` owns

- resolved root object type
- resolved fields
- logical shape tree
- logical cardinality
- logical filter expressions
- logical order expressions
- limit and offset values

### `sqlite-query-plan` owns

- SQLite table names
- SQLite column names
- table aliases
- join descriptions
- selected value slots
- SQLite predicate structure
- SQLite order expressions
- result shape plan
- follow-up fetch plans for future multi-link support

### `sqlite-query-plan` must not own

- final SQL strings
- SQL placeholder numbering
- SQLite driver binding calls
- row decoding
- JSON/result object construction
- query parsing
- schema validation
- resolver diagnostics

Those belong to later `sqlgen` and runtime layers.

## Naming Rules

The first implementation needs deterministic physical names.

Minimum naming rules:

- object type `Post` -> table `post`
- object type `User` -> table `user`
- scalar field `title` -> column `title`
- implicit field `id` -> column `id`
- single link field `author` -> foreign key column `author_id`
- root table alias -> `root`
- selected single-link alias -> field name, for example `author`

These can start as simple functions inside `sqlite-query-plan`.

Initial functions:

```rust
fn table_name(object_type: &schema_model::ObjectTypeRef) -> String
fn scalar_column_name(field: &schema_model::FieldRef) -> String
fn single_link_column_name(field: &schema_model::FieldRef) -> String
```

Do not over-generalize naming yet. Centralize it so future `sqlgen` and
migration code can reuse or replace it.

## Initial Public API

Start with select only:

```rust
pub fn plan_select(query: &query_ir::SelectQuery) -> SQLiteSelectPlan
```

The first version can assume resolver produced valid Semantic IR.

Use `Result` only once the planner starts rejecting cases that resolver allows
but SQLite planning does not yet support.

Expected future shape:

```rust
pub fn plan_select(query: &query_ir::SelectQuery) -> Result<SQLiteSelectPlan, SQLitePlanError>
```

Do not introduce this error enum until a test needs it.

## Initial Data Structures

Start small, but choose names that can grow.

### `SQLiteSelectPlan`

Minimum first shape:

```rust
pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    joins: Vec<SQLiteJoin>,
    predicate: Option<SQLitePredicate>,
    order_by: Vec<SQLiteOrder>,
    limit: Option<u64>,
    offset: Option<u64>,
    result_shape: SQLiteResultShapePlan,
}
```

It is acceptable to introduce fields incrementally as tests require them.

### `SQLiteObjectSource`

Represents a physical object table.

Minimum fields:

```rust
pub struct SQLiteObjectSource {
    object_type: schema_model::ObjectTypeRef,
    table_name: String,
    alias: String,
    id_column: String,
}
```

### `SQLiteSelectValue`

Represents a physical value fetched by the main select.

Minimum fields:

```rust
pub struct SQLiteSelectValue {
    slot: SQLiteValueSlot,
    source_alias: String,
    column_name: String,
    field: schema_model::FieldRef,
    role: SQLiteValueRole,
}
```

### `SQLiteValueSlot`

Use a small stable slot id wrapper.

```rust
pub struct SQLiteValueSlot(u64);
```

Slots are not SQL column aliases yet. They are stable plan identifiers used by
later SQL generation and result shaping.

### `SQLiteValueRole`

Initial roles:

```rust
pub enum SQLiteValueRole {
    ObjectId,
    Scalar,
}
```

The role tells result shaping why the value was selected. It should describe
the value's purpose, not whether the value came from the root source or a nested
source. Root versus nested location is represented by source aliases and result
shape metadata.

For the current implementation, `schema_model::FieldRef` only carries field identity,
owner object type, and field name. It does not yet carry field kind or
`is_implicit` metadata. Because the schema layer rejects explicit `id` field
declarations, the SQLite planner may temporarily classify a root selected field
with `field.name() == "id"` as `SQLiteValueRole::ObjectId`.

This is an intentional short-term rule, not the final metadata model. If the
planner later needs to distinguish implicit fields, declared scalar fields, and
link fields without relying on reserved names, extend the resolved schema
reference model first. Likely options are:

- add field kind and `is_implicit` metadata to `schema_model::FieldRef`
- introduce a richer resolved field descriptor in `ir`
- keep `FieldRef` as identity only but provide catalog-backed metadata lookup
  to planner inputs

Do not expand this yet just to implement root `id` projection. The next test
should document the current behavior and keep the implementation small:

```text
sqlite_select_plan_can_project_implicit_id
```

Expected first-pass behavior:

- selected column is `root.id`
- output name is preserved from `query_ir::ResolvedShapeField`
- selected value role is `ObjectId`

### `SQLiteJoin`

Minimum fields:

```rust
pub struct SQLiteJoin {
    kind: SQLiteJoinKind,
    source_alias: String,
    target_table: String,
    target_alias: String,
    on: SQLiteJoinCondition,
    reason: SQLiteJoinReason,
}
```

Initial join kinds:

```rust
pub enum SQLiteJoinKind {
    Inner,
    Left,
}
```

Initial join reasons:

```rust
pub enum SQLiteJoinReason {
    SelectedSingleLink { field: schema_model::FieldRef },
}
```

### `SQLiteJoinCondition`

Minimum structure:

```rust
pub struct SQLiteJoinCondition {
    left_alias: String,
    left_column: String,
    right_alias: String,
    right_column: String,
}
```

Example:

```text
root.author_id = author.id
```

### `SQLitePredicate`

Start with only compare:

```rust
pub enum SQLitePredicate {
    Compare(SQLiteComparePredicate),
}
```

```rust
pub struct SQLiteComparePredicate {
    left: SQLiteValueExpr,
    op: SQLiteCompareOp,
    right: SQLiteValueExpr,
}
```

For the current resolver path, compare expressions are produced from the
asymmetric `query-ast::CompareExpr` shape:

```text
Path = Literal
```

So the first sqlite-query-plan tests should focus on `Field = Literal` IR values that
the resolver can actually produce today.

However, the SQLite predicate plan should not bake in that AST limitation. The
Semantic IR compare model already uses value expressions on both sides, and the
query AST is expected to become more general later. The planner should therefore
model both sides as SQLite value expressions, even if the first implemented
cases only exercise `Column = Literal`.

Do not add tests for `Literal = Column` or `Column = Column` until the AST and
resolver can produce those forms. Those tests would be valid planner unit tests,
but they are intentionally deferred to avoid making the implementation plan
look broader than the current pipeline contract.

### `SQLiteValueExpr`

Represents a planned value expression used in predicates.

Initial forms:

```rust
pub enum SQLiteValueExpr {
    Column(SQLiteValueRef),
    Literal(SQLiteLiteral),
}
```

### `SQLiteOrder`

Minimum fields:

```rust
pub struct SQLiteOrder {
    value: SQLiteValueRef,
    direction: SQLiteOrderDirection,
}
```

### `SQLiteValueRef`

Represents a planned physical value reference.

```rust
pub struct SQLiteValueRef {
    source_alias: String,
    column_name: String,
}
```

### `SQLiteLiteral`

Start with:

```rust
pub enum SQLiteLiteral {
    String(String),
}
```

### `SQLiteResultShapePlan`

This can start minimal:

```rust
pub struct SQLiteResultShapePlan {
    fields: Vec<SQLiteResultField>,
}
```

```rust
pub struct SQLiteResultField {
    output_name: String,
    cardinality: schema_model::Cardinality,
    value_slot: Option<SQLiteValueSlot>,
    nested_shape: Option<SQLiteResultShapePlan>,
}
```

This should mirror enough of `query_ir::ResolvedShape` to help later runtime shaping,
but it should refer to SQLite value slots instead of semantic fields only.

## First Implementation Scope

Implement select planning in these stages:

1. root source only
2. scalar shape fields
3. implicit `id` field
4. order by scalar field
5. limit and offset passthrough
6. filter compare on scalar field
7. selected single-link child shape
8. result shape metadata

Do not implement yet:

- multi-link follow-up fetches
- filter traversal through links
- order traversal through links
- boolean predicate trees
- SQL string generation
- row shaping runtime
- mutation planning

## Test Plan

### Layer 1: root source and naming

1. `sqlite_select_plan_can_store_root_source`

Input:

```text
ir root object type Post
```

Expected:

- root object type is `Post`
- table name is `post`
- alias is `root`
- id column is `id`

2. `sqlite_table_names_are_deterministic`

Expected:

- `UserProfile` naming behavior is explicit

This test should only be added after deciding the naming convention for
multi-word identifiers. Until then, avoid guessing.

### Layer 2: scalar projection

3. `sqlite_select_plan_can_project_root_scalar_field`

Input IR:

```text
select Post { title }
```

Expected:

- one selected value for `root.title`
- role is `Scalar`
- result shape contains output field `title`

4. `sqlite_select_plan_can_project_implicit_id`

Input IR:

```text
select Post { id }
```

Expected:

- selected value column is `id`
- role is `ObjectId`

### Layer 3: pagination and ordering

5. `sqlite_select_plan_passes_limit_and_offset_through`

Expected:

- limit and offset match IR

6. `sqlite_select_plan_can_order_by_root_scalar_field`

Input IR:

```text
order by .title desc
```

Expected:

- order value is `root.title`
- direction is `Desc`

### Layer 4: predicates

7. `sqlite_select_plan_can_lower_scalar_compare_predicate`

Input IR:

```text
filter .title = "Hello"
```

Expected:

- predicate left is `root.title`
- operator is `Eq`
- right literal is `"Hello"`

### Layer 5: selected single link

8. `sqlite_select_plan_can_join_selected_single_link`

Input IR:

```text
select Post {
  author { name }
}
```

Expected:

- join target table is `user`
- join target alias is `author`
- join condition is `root.author_id = author.id`
- selected value contains `author.name`
- result shape contains nested field `author`

9. `sqlite_select_plan_preserves_output_field_order`

Input IR:

```text
select Post {
  title,
  author { name }
}
```

Expected:

- result shape field order is `title`, `author`
- selected values are stable enough for later shaping

## Suggested First Test Fixture

The first sqlite-query-plan tests should construct IR directly.

Do not go through `query-ast` or `resolver` in sqlite-query-plan unit tests.
The planner consumes IR, so tests should be able to build IR values directly.

Use helpers similar to the `ir` tests:

```rust
fn post_type() -> schema_model::ObjectTypeRef
fn user_type() -> schema_model::ObjectTypeRef
fn post_title_field() -> schema_model::FieldRef
fn post_author_field() -> schema_model::FieldRef
fn user_name_field() -> schema_model::FieldRef
fn post_shape_with_title() -> query_ir::ResolvedShape
```

This keeps planner tests focused on physical lowering rather than resolver
behavior.

## First Test To Write

Start with:

```rust
#[test]
fn sqlite_select_plan_can_store_root_source() {
    let query = query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&query);

    assert_eq!(plan.root_source().object_type().name(), "Post");
    assert_eq!(plan.root_source().table_name(), "post");
    assert_eq!(plan.root_source().alias(), "root");
    assert_eq!(plan.root_source().id_column(), "id");
}
```

This first test fixes the boundary between semantic object type identity and
SQLite physical table access.

## Current Completion Status

The first `sqlite-query-plan` pass now covers the select-planning contracts needed
before SQL generation:

- root source planning
- root scalar projection
- root implicit `id` projection
- selected value output name preservation
- selected value order preservation
- limit and offset passthrough
- root scalar ordering, including `asc`, `desc`, and implicit `id`
- scalar equality predicates over root fields and string literals
- absent filter preservation
- selected required single-link joins
- selected optional single-link joins
- selected single-link scalar projection
- selected single-link identity value projection
- root result shape fields
- selected single-link nested result shape
- nested result identity value
- root and nested result shape field order preservation

This means the project can now move to SQL generation for the currently
supported plan subset.

The next layer should consume `sqlite-query-plan`, not `ir` or `query-ast` directly.

```text
ir -> sqlite-query-plan -> sqlite-query-sqlgen -> SQL string
```

The SQL generation layer should render the already-planned physical structure:

- `SQLiteSelectPlan.root_source` -> `FROM`
- `SQLiteSelectPlan.selected_values` -> `SELECT`
- `SQLiteSelectPlan.joins` -> `JOIN`
- `SQLiteSelectPlan.filter` -> `WHERE`
- `SQLiteSelectPlan.order_by` -> `ORDER BY`
- `limit` and `offset` -> `LIMIT` and `OFFSET`

It should not perform schema resolution, query validation, result shaping, or
new join planning.

The current `WHERE` scope is intentionally limited to one planned compare
predicate. This does not mean the query model should stay limited to a single
condition. `spec/ir.md` already reserves boolean expressions such as `AndExpr`,
`OrExpr`, and `NotExpr`; those should be lowered later as a predicate tree, not
as an unrelated list of filters.

For example, a future query such as:

```text
filter .title = "Hello" and .status = "published"
```

should become a single filter expression tree:

```text
And(
  Compare(Column(root.title), Eq, Literal("Hello")),
  Compare(Column(root.status), Eq, Literal("published"))
)
```

The SQL generation layer should then recursively render that tree into:

```sql
WHERE (root.title = ?) AND (root.status = ?)
```

and collect bind values in the same left-to-right traversal order as the
placeholder generation.

## Next Slice: User-Visible Select Results

Issue #40 should stop exposing physical SQLite result columns as logical query
output. The planner already preserves both sides of this boundary:

- `SQLiteSelectPlan.selected_values` describes the ordered physical columns
  required by SQL execution.
- `SQLiteSelectPlan.result_shape` describes the logical output fields and keeps
  synthesized nested object identities separate from those fields.

The next slice should derive one ordered output descriptor from that existing
metadata and carry it with the rendered select statement to execution. Each
physical column position should either name one user-visible output field or be
marked internal. Runtime row decoding should then:

- rename computed SQL aliases such as `__gelite_value_0` to their logical
  `output_name`
- omit synthesized nested object identity columns from user-facing columns and
  rows
- preserve `id` when it was explicitly selected by the query
- preserve the requested field order

Internal identity values must remain in the generated SQL. They are needed for
later null detection, object deduplication, and nested result reconstruction;
this slice changes only the public projection of the physical row.

Use column positions rather than SQLite column-name lookup. Physical names may
repeat across joins, and an explicitly selected nested `id` may coexist with a
synthesized identity value for the same object.

Implementation order:

1. Represent the visibility and logical name of each ordered selected value in
   the SQLite select plan.
2. Preserve that output descriptor when the REPL compiles a select instead of
   retaining only the rendered statement.
3. Apply the descriptor after native row decoding and before constructing the
   user-facing `SQLiteQueryResult`.
4. Update pipeline and REPL tests for logical computed names, hidden internal
   identities, explicit `id`, and stable field order.

This slice does not reconstruct nested JSON-like objects, merge duplicate rows,
or implement multi-link follow-up fetching. Those behaviors should reuse the
same internal identity values in the later runtime result-shaping slice.

## Commit Strategy

Use small commits:

1. add workspace crate and empty module/test wiring
2. add root source planning
3. add scalar projection planning
4. add order/limit/offset planning
5. add scalar predicate planning
6. add single-link join planning
7. add result shape planning
8. add SQL generation in a separate layer

Avoid mixing SQL generation into these commits.

## Open Design Questions

Answer these only when tests force the decision:

- How should multi-word type and field names map to SQLite names?
- Should aliases be globally unique counters or path-derived names?
- Should required single links use `inner join` or should all selected links use
  `left join` for shape preservation?
- Should filter `.author.id = ...` lower to `root.author_id = ?` or an explicit
  join?
- How much result shaping metadata belongs in SQLite Plan versus runtime?

Do not resolve these prematurely. The first sqlite-query-plan pass should implement
only root scalar fields and selected required single links.

## Temporary Rules and Deferred Design Work

The current sqlite-query-plan implementation intentionally uses several small
short-term rules to keep the learning slice moving. These rules are acceptable
for the first pass, but they must not become hidden permanent architecture.

### Field metadata is inferred from names

Current rule:

- `field.name() == "id"` is treated as an object identity value.
- This maps to `SQLiteValueRole::ObjectId`.
- Other selected fields map to `SQLiteValueRole::Scalar`.

Why this is temporary:

- `schema_model::FieldRef` currently carries only field id, owner object type, and
  field name.
- It does not carry field kind, scalar type, link target, cardinality, or
  `is_implicit`.
- Name-based identity detection works only because the schema layer rejects
  explicit `id` field declarations.

Later replacement options:

- enrich `schema_model::FieldRef` with field metadata
- add a richer resolved field descriptor to `ir`
- pass catalog-backed field metadata into sqlite planning

### Implicit nested object identity is not hidden at runtime yet

Selected nested objects need an identity value for result shaping. For example:

```text
select Post {
  author { name }
}
```

The planner will eventually need both:

```text
author.id
author.name
```

Current limitation:

- the planner synthesizes nested `id` values when entering a selected
  single-link child shape
- `SQLiteResultShapePlan` keeps those identities separate from user-requested
  fields, but `SQLiteSelectValue` does not expose that distinction to execution
- the REPL retains only the rendered statement, so the native runner reports
  SQLite's physical column names and includes the synthesized identities

Planned replacement:

- derive an ordinal output descriptor from `SQLiteSelectPlan`
- mark synthesized identities as internal without removing them from SQL
- carry the descriptor through REPL execution
- project decoded rows to logical names and user-visible values before returning
  `SQLiteQueryResult`

### Selected link detection relies on child shape presence

Current rule:

- a `ResolvedShapeField` with `child_shape().is_some()` is treated as a selected
  single-link field
- a field without child shape is treated as a scalar selected value

Why this is temporary:

- `ResolvedShapeField` currently exposes a `FieldRef`, but `FieldRef` does not
  identify whether the field is scalar or link.
- This rule depends on resolver validation preventing child shapes on scalar
  fields and preventing missing child shapes on selected link fields.

Later replacement:

- carry field kind in resolved field metadata
- allow sqlite planning to distinguish scalar, single-link, and multi-link
  fields directly

### Physical naming is intentionally simple

Current rules:

- object type name is lowercased for table name
- scalar field name is used directly as column name
- single-link foreign key column is `"{field_name}_id"`
- root alias is `"root"`
- selected single-link alias is the output field name

Why this is temporary:

- multi-word names, escaping, reserved SQL words, duplicate aliases, and nested
  paths are not handled yet
- alias generation will need to become collision-resistant before complex
  nested shapes or multiple joins to the same table are supported

Later replacement:

- centralize physical naming helpers
- define identifier escaping rules before SQL generation
- introduce stable alias generation based on path or counters

### Resolved path lowering must plan columns and joins together

Current risk:

- `SQLiteValueExpr::from_ir` can lower `query_ir::ValueExpr::Path` into a
  `SQLiteColumnRef`.
- If it only reads the terminal field and always uses source alias `"root"`,
  traversed paths are lowered incorrectly.

Example:

```text
Post.author.name
```

Incorrect lowering:

```text
root.name
```

Correct lowering:

```text
author.name
```

with the corresponding join:

```sql
INNER JOIN user AS author ON root.author_id = author.id
```

Planned replacement:

- introduce an internal path planning helper, tentatively
  `plan_resolved_path`
- make it return both the terminal `SQLiteColumnRef` and the `SQLiteJoin`
  values required to make that column alias valid
- use it from filter and order planning instead of letting
  `SQLiteValueExpr::from_ir` independently invent aliases
- collect joins from selected shapes, filters, and order clauses in
  `plan_select`

Suggested internal type:

```rust
struct PlannedPath {
    column: SQLiteColumnRef,
    joins: Vec<SQLiteJoin>,
}
```

Initial traversal rules:

- a one-step scalar path lowers to `root.<field>`
- a chain of required or optional single-link steps followed by a scalar lowers
  to path-derived aliases such as `author.name` or `author_organization.name`
- `required` single-link traversal uses an inner join
- `optional` single-link traversal uses a left join
- `multi` link traversal is unsupported until join-table lowering is designed

Test order:

1. prove a single-link filter path lowers to the non-root terminal alias
2. prove the same filter path adds the required join
3. repeat the same contracts for order paths
4. add deduplication tests when the same path is used by shape, filter, and
   order

### Filter predicates only support one compare expression

Current rule:

- `query_ir::Expr` only has `Compare`.
- `SQLiteWhereExpr` only has `Compare`.
- SQL generation only needs to render one comparison predicate in `WHERE`.

Why this is temporary:

- `spec/ir.md` includes boolean expression forms: `AndExpr`, `OrExpr`, and
  `NotExpr`.
- Real filters commonly need multiple predicates joined by boolean operators.
- Modeling multiple predicates as `Vec<Filter>` would lose the logical tree
  structure needed for parentheses, precedence, and nested boolean expressions.

Later replacement:

- extend `query-ast::Expr` with boolean expression variants
- extend `query_ir::Expr` with resolved boolean expression variants
- extend `SQLiteWhereExpr` or replace it with a recursive `SQLitePredicate`
  tree
- render predicates recursively while preserving placeholder and bind-value
  order

### Planner still assumes valid resolver output

Current rule:

- `plan_select` returns `SQLiteSelectPlan`, not `Result<_, SQLitePlanError>`
- unsupported cases may use `todo!()` or `expect()` while tests remain inside
  the supported MVP slice

Why this is temporary:

- once sqlite-query-plan starts receiving valid IR that it cannot yet lower, planner
  errors must become explicit

Later replacement:

- introduce `SQLitePlanError`
- convert unsupported multi-link, unsupported value expression, and invalid
  physical naming cases into structured errors
