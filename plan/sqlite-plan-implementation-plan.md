# SQLite Plan Implementation Plan

## Goal

Implement the next compiler layer after resolver:

```text
ir::SelectQuery -> sqlite-plan::SQLiteSelectPlan
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
- `dev-cli`
  - hardcoded smoke runner for `schema + query-ast -> resolver -> ir`

The next layer should consume `ir`, not `query-ast`.

## Crate Plan

Add a new crate:

```text
engine/crates/sqlite-plan
```

Workspace dependency direction:

```text
sqlite-plan -> ir
sqlite-plan -> schema
```

Do not make `ir` depend on `sqlite-plan`.
Do not make `resolver` depend on `sqlite-plan`.
Do not make `sqlite-plan` depend on `query-ast`.

`sqlite-plan` is backend-specific. It may know SQLite physical naming rules,
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

### `sqlite-plan` owns

- SQLite table names
- SQLite column names
- table aliases
- join descriptions
- selected value slots
- SQLite predicate structure
- SQLite order expressions
- result shape plan
- follow-up fetch plans for future multi-link support

### `sqlite-plan` must not own

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

These can start as simple functions inside `sqlite-plan`.

Initial functions:

```rust
fn table_name(object_type: &schema::ObjectTypeRef) -> String
fn scalar_column_name(field: &schema::FieldRef) -> String
fn single_link_column_name(field: &schema::FieldRef) -> String
```

Do not over-generalize naming yet. Centralize it so future `sqlgen` and
migration code can reuse or replace it.

## Initial Public API

Start with select only:

```rust
pub fn plan_select(query: &ir::SelectQuery) -> SQLiteSelectPlan
```

The first version can assume resolver produced valid Semantic IR.

Use `Result` only once the planner starts rejecting cases that resolver allows
but SQLite planning does not yet support.

Expected future shape:

```rust
pub fn plan_select(query: &ir::SelectQuery) -> Result<SQLiteSelectPlan, SQLitePlanError>
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
    object_type: schema::ObjectTypeRef,
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
    field: schema::FieldRef,
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

For the current implementation, `schema::FieldRef` only carries field identity,
owner object type, and field name. It does not yet carry field kind or
`is_implicit` metadata. Because the schema layer rejects explicit `id` field
declarations, the SQLite planner may temporarily classify a root selected field
with `field.name() == "id"` as `SQLiteValueRole::ObjectId`.

This is an intentional short-term rule, not the final metadata model. If the
planner later needs to distinguish implicit fields, declared scalar fields, and
link fields without relying on reserved names, extend the resolved schema
reference model first. Likely options are:

- add field kind and `is_implicit` metadata to `schema::FieldRef`
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
- output name is preserved from `ir::ResolvedShapeField`
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
    SelectedSingleLink { field: schema::FieldRef },
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

So the first sqlite-plan tests should focus on `Field = Literal` IR values that
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
    cardinality: schema::Cardinality,
    value_slot: Option<SQLiteValueSlot>,
    nested_shape: Option<SQLiteResultShapePlan>,
}
```

This should mirror enough of `ir::ResolvedShape` to help later runtime shaping,
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

The first sqlite-plan tests should construct IR directly.

Do not go through `query-ast` or `resolver` in sqlite-plan unit tests.
The planner consumes IR, so tests should be able to build IR values directly.

Use helpers similar to the `ir` tests:

```rust
fn post_type() -> schema::ObjectTypeRef
fn user_type() -> schema::ObjectTypeRef
fn post_title_field() -> schema::FieldRef
fn post_author_field() -> schema::FieldRef
fn user_name_field() -> schema::FieldRef
fn post_shape_with_title() -> ir::ResolvedShape
```

This keeps planner tests focused on physical lowering rather than resolver
behavior.

## First Test To Write

Start with:

```rust
#[test]
fn sqlite_select_plan_can_store_root_source() {
    let query = ir::SelectQuery::new(
        post_type(),
        ir::ResolvedShape::new(post_type(), vec![]),
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

## Commit Strategy

Use small commits:

1. add workspace crate and empty module/test wiring
2. add root source planning
3. add scalar projection planning
4. add order/limit/offset planning
5. add scalar predicate planning
6. add single-link join planning

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

Do not resolve these prematurely. The first sqlite-plan pass should implement
only root scalar fields and selected required single links.
