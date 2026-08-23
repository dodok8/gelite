#![no_std]
//! SQLite-specific execution plan for resolved queries.
//!
//! This crate is the first backend-specific compiler layer. It lowers Semantic
//! IR into structured SQLite access information: root table, aliases, selected
//! columns, joins, predicates, ordering, and result-shaping metadata.
//!
//! The plan is not SQL text. Keeping this layer structured lets tests inspect
//! physical decisions before `sqlite-query-sqlgen` serializes them. It also keeps
//! SQLite naming and join rules out of the backend-independent IR.
//!
//! The current planner handles select queries plus insert and update assignments
//! from literals or supported single-link selects. Select and update filters
//! support direct scalar columns and path traversal through single links.
//! Selected multi links are lowered to follow-up fetch plans. Rendering and
//! executing those follow-ups remain responsibilities of later layers.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use query_ir::{CompareOp, Expr, InOp, SelectQuery};
use schema_model::{Cardinality, FieldRef, ObjectTypeRef};

/// Lowers a resolved select query to a structured SQLite select plan.
pub fn plan_select(ir: &SelectQuery) -> SQLiteSelectPlan {
    let root_object_type = ir.root_object_type().clone();
    let root_identity_required = ir.shape().items().iter().any(|item| {
        matches!(
            item,
            query_ir::ResolvedShapeItem::Field(field)
                if field.cardinality() == Cardinality::Many
        )
    });

    // SELECT values and result-shape refs must assign computed aliases in the
    // same shape traversal order so the shaper reads the rendered SQL columns.
    let mut selected_column_names = selected_field_column_names(ir.shape());
    if root_identity_required {
        selected_column_names.insert(0, "id".to_string());
    }
    let mut select_aliases = SQLiteComputedAliasAllocator::new(selected_column_names.clone());
    let mut join_aliases = SQLiteJoinAliasAllocator::new(selected_link_aliases(ir.shape()));
    let root_path_aliases = root_path_aliases(ir);
    join_aliases.reserve_aliases(&root_path_aliases);
    let selected_shape_aliases =
        plan_selected_shape_aliases(ir.shape(), root_path_aliases, &mut join_aliases);
    let planned_shape_values = plan_shape_values(
        ir.shape(),
        "root",
        false,
        &[],
        &selected_shape_aliases,
        &mut select_aliases,
        &mut join_aliases,
    );
    let mut selected_values = planned_shape_values.values;
    if root_identity_required {
        selected_values.insert(
            0,
            SQLiteSelectValue::from_field("root", object_identity_field(&root_object_type), None),
        );
    }
    let mut result_aliases = SQLiteComputedAliasAllocator::new(selected_column_names);
    let mut next_follow_up_fetch_index = 0;
    let result_shape = plan_result_shape(
        ir.shape(),
        "root",
        root_identity_required,
        &[],
        &selected_shape_aliases,
        &mut result_aliases,
        &mut next_follow_up_fetch_index,
    );

    let planned_orders: Vec<PlannedOrder> = ir
        .order_by()
        .iter()
        .map(|order| plan_order_expr(order, &mut join_aliases))
        .collect();

    let mut order_by = Vec::new();
    let mut order_joins = Vec::new();

    for planned in planned_orders {
        order_by.push(planned.order);
        order_joins.extend(planned.joins);
    }

    let (filter, filter_joins) = match ir.filter() {
        Some(expr) => {
            let planned = plan_where_expr(expr, &mut join_aliases);
            (Some(planned.expr), planned.joins)
        }
        None => (None, vec![]),
    };

    let mut joins =
        plan_selected_shape_joins(ir.shape(), "root", false, &[], &selected_shape_aliases);

    joins.extend(planned_shape_values.joins);
    joins.extend(filter_joins);
    joins.extend(order_joins);
    joins = dedup_joins(joins);
    let follow_up_fetches =
        plan_follow_up_fetches(ir.shape(), "root", &[], &selected_shape_aliases);

    SQLiteSelectPlan {
        root_source: SQLiteObjectSource {
            table_name: sqlite_table_name(&root_object_type),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: root_object_type,
        },
        selected_values,
        order_by,
        filter,
        limit: ir.limit(),
        offset: ir.offset(),
        joins,
        result_shape,
        follow_up_fetches,
    }
}

/// Lowers a resolved insert query to a structured SQLite insert plan.
pub fn plan_insert(ir: &query_ir::InsertQuery) -> SQLiteInsertPlan {
    let root_object_type = ir.root_object_type().clone();
    let assignments = ir.assignments().iter().map(plan_assignment).collect();

    SQLiteInsertPlan {
        root_target: SQLiteInsertTarget {
            table_name: sqlite_table_name(&root_object_type),
            id_column: "id".to_string(),
        },
        generated_id_strategy: SQLiteGeneratedIdStrategy::RuntimeUuid,
        assignments,
    }
}

/// Lowers a resolved update query to a structured SQLite update plan.
pub fn plan_update(ir: &query_ir::UpdateQuery) -> SQLiteUpdatePlan {
    let target_object_type = ir.target_object_type().clone();
    let (filter, joins) = plan_mutation_filter(ir.filter());
    let multi_link_mutation = ir.assignments().first().and_then(|assignment| {
        let operation = match assignment.operator() {
            query_ir::AssignmentOperator::Assign => return None,
            query_ir::AssignmentOperator::Add => SQLiteMultiLinkMutationOp::Add,
            query_ir::AssignmentOperator::Remove => SQLiteMultiLinkMutationOp::Remove,
        };
        let query_ir::AssignmentValue::MultiLinkSelect(targets) = assignment.value() else {
            unreachable!("multi-link operation must contain a resolved target select")
        };
        let source_id = object_identity_field(&target_object_type);
        let sources = query_ir::SelectQuery::new(
            target_object_type.clone(),
            query_ir::ResolvedShape::new(
                target_object_type.clone(),
                vec![query_ir::ResolvedShapeField::new(
                    "id",
                    source_id,
                    Cardinality::Required,
                    None,
                )],
            ),
            ir.filter().cloned(),
            vec![],
            None,
            None,
        );

        Some(SQLiteMultiLinkMutationPlan {
            operation,
            join_table_name: format!(
                "{}__{}",
                sqlite_table_name(&target_object_type),
                assignment.field().name()
            ),
            source_column: "source_id".to_string(),
            target_column: "target_id".to_string(),
            sources: Box::new(plan_select(&sources)),
            targets: Box::new(plan_select(targets)),
        })
    });
    let assignments = if multi_link_mutation.is_some() {
        vec![]
    } else {
        ir.assignments().iter().map(plan_assignment).collect()
    };

    SQLiteUpdatePlan {
        target: SQLiteObjectSource {
            table_name: sqlite_table_name(&target_object_type),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: target_object_type,
        },
        assignments,
        filter,
        joins,
        multi_link_mutation,
    }
}

/// Lowers a resolved delete query to a structured SQLite delete plan.
pub fn plan_delete(ir: &query_ir::DeleteQuery) -> SQLiteDeletePlan {
    let target_object_type = ir.target_object_type().clone();
    let (filter, joins) = plan_mutation_filter(ir.filter());

    SQLiteDeletePlan {
        target: SQLiteObjectSource {
            table_name: sqlite_table_name(&target_object_type),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: target_object_type,
        },
        filter,
        joins,
    }
}

fn plan_mutation_filter(
    filter: Option<&query_ir::Expr>,
) -> (Option<SQLiteWhereExpr>, Vec<SQLiteJoin>) {
    let mut reserved_aliases = vec![];
    if let Some(filter) = filter {
        collect_root_path_aliases_from_expr(filter, &mut reserved_aliases);
    }
    let mut join_aliases = SQLiteJoinAliasAllocator::new(reserved_aliases);

    match filter {
        Some(expr) => {
            let planned = plan_where_expr(expr, &mut join_aliases);
            (Some(planned.expr), dedup_joins(planned.joins))
        }
        None => (None, vec![]),
    }
}

fn sqlite_table_name(object_type: &ObjectTypeRef) -> String {
    object_type.name().to_ascii_lowercase()
}

fn object_identity_field(object_type: &ObjectTypeRef) -> FieldRef {
    FieldRef::new(schema_model::FieldId::new(1), object_type.clone(), "id")
}

fn plan_assignment(assignment: &query_ir::Assignment) -> SQLiteAssignment {
    let field = assignment.field();
    let (column_name, value) = match assignment.value() {
        query_ir::AssignmentValue::Scalar(value) => (
            field.name().to_string(),
            SQLiteAssignmentValue::Literal(sqlite_literal_from_ir(value)),
        ),
        query_ir::AssignmentValue::LinkId(value) => (
            format!("{}_id", field.name()),
            SQLiteAssignmentValue::Literal(SQLiteLiteral::String(value.clone())),
        ),
        query_ir::AssignmentValue::LinkSelect(select) => (
            format!("{}_id", field.name()),
            SQLiteAssignmentValue::Select(Box::new(plan_select(select))),
        ),
        query_ir::AssignmentValue::MultiLinkSelect(_) => {
            unreachable!("multi-link select requires a multi-link mutation plan")
        }
        query_ir::AssignmentValue::ScalarNull => (
            field.name().to_string(),
            SQLiteAssignmentValue::Literal(SQLiteLiteral::Null),
        ),
        query_ir::AssignmentValue::LinkNull => (
            format!("{}_id", field.name()),
            SQLiteAssignmentValue::Literal(SQLiteLiteral::Null),
        ),
    };

    SQLiteAssignment { column_name, value }
}

/// Structured SQLite plan for inserting one resolved object.
pub struct SQLiteInsertPlan {
    root_target: SQLiteInsertTarget,
    generated_id_strategy: SQLiteGeneratedIdStrategy,
    assignments: Vec<SQLiteAssignment>,
}

impl SQLiteInsertPlan {
    pub fn root_target(&self) -> &SQLiteInsertTarget {
        &self.root_target
    }

    pub fn generated_id_strategy(&self) -> SQLiteGeneratedIdStrategy {
        self.generated_id_strategy
    }

    pub fn assignments(&self) -> &[SQLiteAssignment] {
        &self.assignments
    }
}

/// Physical table targeted by a SQLite insert.
pub struct SQLiteInsertTarget {
    table_name: String,
    id_column: String,
}

impl SQLiteInsertTarget {
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn id_column(&self) -> &str {
        &self.id_column
    }
}

/// Runtime strategy used to create the implicit object identity for an insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteGeneratedIdStrategy {
    RuntimeUuid,
}

/// One physical SQLite column assignment in a mutation plan.
pub struct SQLiteAssignment {
    column_name: String,
    value: SQLiteAssignmentValue,
}

impl SQLiteAssignment {
    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn value(&self) -> &SQLiteAssignmentValue {
        &self.value
    }
}

/// Physical value assigned to one SQLite mutation column.
pub enum SQLiteAssignmentValue {
    Literal(SQLiteLiteral),
    Select(Box<SQLiteSelectPlan>),
}

/// Structured SQLite plan for updating resolved objects.
pub struct SQLiteUpdatePlan {
    target: SQLiteObjectSource,
    assignments: Vec<SQLiteAssignment>,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
    multi_link_mutation: Option<SQLiteMultiLinkMutationPlan>,
}

impl SQLiteUpdatePlan {
    pub fn target(&self) -> &SQLiteObjectSource {
        &self.target
    }

    pub fn assignments(&self) -> &[SQLiteAssignment] {
        &self.assignments
    }

    pub fn filter(&self) -> Option<&SQLiteWhereExpr> {
        self.filter.as_ref()
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }

    pub fn multi_link_mutation(&self) -> Option<&SQLiteMultiLinkMutationPlan> {
        self.multi_link_mutation.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteMultiLinkMutationOp {
    Add,
    Remove,
}

pub struct SQLiteMultiLinkMutationPlan {
    operation: SQLiteMultiLinkMutationOp,
    join_table_name: String,
    source_column: String,
    target_column: String,
    sources: Box<SQLiteSelectPlan>,
    targets: Box<SQLiteSelectPlan>,
}

impl SQLiteMultiLinkMutationPlan {
    pub fn operation(&self) -> SQLiteMultiLinkMutationOp {
        self.operation
    }

    pub fn join_table_name(&self) -> &str {
        &self.join_table_name
    }

    pub fn source_column(&self) -> &str {
        &self.source_column
    }

    pub fn target_column(&self) -> &str {
        &self.target_column
    }

    pub fn sources(&self) -> &SQLiteSelectPlan {
        &self.sources
    }

    pub fn targets(&self) -> &SQLiteSelectPlan {
        &self.targets
    }
}

/// Structured SQLite plan for deleting resolved objects.
pub struct SQLiteDeletePlan {
    target: SQLiteObjectSource,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
}

impl SQLiteDeletePlan {
    pub fn target(&self) -> &SQLiteObjectSource {
        &self.target
    }

    pub fn filter(&self) -> Option<&SQLiteWhereExpr> {
        self.filter.as_ref()
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }
}

/// Role of a selected SQLite value in result shaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteValueRole {
    ObjectId,
    Scalar,
    Computed,
}

/// Structured SQLite plan for a select query.
///
/// It records all physical values and joins needed to render SQL and later
/// reconstruct the logical result shape.
pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    order_by: Vec<SQLiteOrder>,
    limit: Option<i64>,
    offset: Option<i64>,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
    result_shape: SQLiteResultShapePlan,
    follow_up_fetches: Vec<SQLiteFollowUpFetchPlan>,
}

impl SQLiteSelectPlan {
    pub fn root_source(&self) -> &SQLiteObjectSource {
        &self.root_source
    }

    pub fn selected_values(&self) -> &[SQLiteSelectValue] {
        &self.selected_values
    }

    pub fn order_by(&self) -> &[SQLiteOrder] {
        &self.order_by
    }

    pub fn limit(&self) -> Option<i64> {
        self.limit
    }

    pub fn offset(&self) -> Option<i64> {
        self.offset
    }

    pub fn filter(&self) -> &Option<SQLiteWhereExpr> {
        &self.filter
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }

    pub fn result_shape(&self) -> &SQLiteResultShapePlan {
        &self.result_shape
    }

    pub fn follow_up_fetches(&self) -> &[SQLiteFollowUpFetchPlan] {
        &self.follow_up_fetches
    }
}

/// Secondary fetch plan for one selected multi link.
pub struct SQLiteFollowUpFetchPlan {
    parent_field: FieldRef,
    parent_identity: SQLiteResultValueRef,
    join_table_name: String,
    target_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    joins: Vec<SQLiteJoin>,
    result_shape: SQLiteResultShapePlan,
    follow_up_fetches: Vec<SQLiteFollowUpFetchPlan>,
}

impl SQLiteFollowUpFetchPlan {
    pub fn parent_field(&self) -> &FieldRef {
        &self.parent_field
    }

    pub fn parent_identity(&self) -> &SQLiteResultValueRef {
        &self.parent_identity
    }

    pub fn join_table_name(&self) -> &str {
        &self.join_table_name
    }

    pub fn source_column(&self) -> &str {
        "source_id"
    }

    pub fn target_column(&self) -> &str {
        "target_id"
    }

    pub fn target_source(&self) -> &SQLiteObjectSource {
        &self.target_source
    }

    pub fn selected_values(&self) -> &[SQLiteSelectValue] {
        &self.selected_values
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }

    pub fn result_shape(&self) -> &SQLiteResultShapePlan {
        &self.result_shape
    }

    pub fn follow_up_fetches(&self) -> &[SQLiteFollowUpFetchPlan] {
        &self.follow_up_fetches
    }
}

/// One value selected by the generated SQL.
pub enum SQLiteSelectValue {
    Field(SQLiteFieldSelectValue),
    Computed(SQLiteComputedSelectValue),
}

/// Schema-backed column selected by the generated SQL.
pub struct SQLiteFieldSelectValue {
    output_name: Option<String>,
    value: SQLiteValueExpr,
    field: schema_model::FieldRef,
    role: SQLiteValueRole,
}

/// Query-local computed value selected by the generated SQL.
pub struct SQLiteComputedSelectValue {
    output_name: String,
    sql_alias: String,
    value: SQLiteValueExpr,
}

impl SQLiteSelectValue {
    pub fn from_field(
        source_alias: impl Into<String>,
        field: schema_model::FieldRef,
        output_name: Option<String>,
    ) -> Self {
        let source_alias = source_alias.into();
        let column_name = field.name().to_string();
        Self::Field(SQLiteFieldSelectValue {
            output_name,
            value: SQLiteValueExpr::Column(SQLiteColumnRef {
                source_alias,
                column_name,
            }),
            role: SQLiteValueRole::for_field(&field),
            field,
        })
    }

    pub fn computed(
        output_name: impl Into<String>,
        sql_alias: impl Into<String>,
        value: SQLiteValueExpr,
    ) -> Self {
        Self::Computed(SQLiteComputedSelectValue {
            output_name: output_name.into(),
            sql_alias: sql_alias.into(),
            value,
        })
    }

    pub fn as_field(&self) -> Option<&SQLiteFieldSelectValue> {
        match self {
            Self::Field(value) => Some(value),
            Self::Computed(_) => None,
        }
    }

    pub fn as_computed(&self) -> Option<&SQLiteComputedSelectValue> {
        match self {
            Self::Field(_) => None,
            Self::Computed(value) => Some(value),
        }
    }

    pub fn output_name(&self) -> Option<&str> {
        match self {
            Self::Field(value) => value.output_name(),
            Self::Computed(value) => Some(value.output_name()),
        }
    }

    pub fn source_alias(&self) -> Option<&str> {
        self.as_field().map(SQLiteFieldSelectValue::source_alias)
    }

    pub fn column_name(&self) -> Option<&str> {
        self.as_field().map(SQLiteFieldSelectValue::column_name)
    }

    pub fn field(&self) -> Option<&schema_model::FieldRef> {
        self.as_field().map(SQLiteFieldSelectValue::field)
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        match self {
            Self::Field(value) => value.value(),
            Self::Computed(value) => value.value(),
        }
    }

    pub fn role(&self) -> SQLiteValueRole {
        match self {
            Self::Field(value) => value.role(),
            Self::Computed(_) => SQLiteValueRole::Computed,
        }
    }
}

impl SQLiteFieldSelectValue {
    pub fn output_name(&self) -> Option<&str> {
        self.output_name.as_deref()
    }

    pub fn source_alias(&self) -> &str {
        match &self.value {
            SQLiteValueExpr::Column(column) => column.source_alias(),
            SQLiteValueExpr::Literal(_)
            | SQLiteValueExpr::Arithmetic(_)
            | SQLiteValueExpr::UnaryArithmetic(_)
            | SQLiteValueExpr::Cast(_)
            | SQLiteValueExpr::StringFunction(_) => {
                unreachable!("field selected values are always columns")
            }
        }
    }

    pub fn column_name(&self) -> &str {
        match &self.value {
            SQLiteValueExpr::Column(column) => column.column_name(),
            SQLiteValueExpr::Literal(_)
            | SQLiteValueExpr::Arithmetic(_)
            | SQLiteValueExpr::UnaryArithmetic(_)
            | SQLiteValueExpr::Cast(_)
            | SQLiteValueExpr::StringFunction(_) => {
                unreachable!("field selected values are always columns")
            }
        }
    }

    pub fn field(&self) -> &schema_model::FieldRef {
        &self.field
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

impl SQLiteComputedSelectValue {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn sql_alias(&self) -> &str {
        &self.sql_alias
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
    }
}

impl SQLiteValueRole {
    fn for_field(field: &schema_model::FieldRef) -> Self {
        if field.name() == "id" {
            Self::ObjectId
        } else {
            Self::Scalar
        }
    }
}

struct PlannedShapeValues {
    values: Vec<SQLiteSelectValue>,
    joins: Vec<SQLiteJoin>,
}

struct SQLiteComputedAliasAllocator {
    next: usize,
    reserved: Vec<String>,
}

impl SQLiteComputedAliasAllocator {
    fn new(reserved: Vec<String>) -> Self {
        Self { next: 0, reserved }
    }

    fn next_alias(&mut self) -> String {
        loop {
            let alias = format!("__gelite_value_{}", self.next);
            self.next += 1;

            if !self.reserved.iter().any(|reserved| reserved == &alias) {
                return alias;
            }
        }
    }
}

struct SQLiteJoinAliasAllocator {
    next: usize,
    reserved: Vec<String>,
    path_aliases: Vec<SQLiteJoinPathAlias>,
}

struct SQLiteJoinPathAlias {
    source_alias: String,
    path: Vec<String>,
    target_alias: String,
}

impl SQLiteJoinAliasAllocator {
    fn new(reserved: Vec<String>) -> Self {
        Self {
            next: 0,
            reserved,
            path_aliases: Vec::new(),
        }
    }

    fn next_alias(&mut self) -> String {
        loop {
            let alias = format!("__gelite_join_{}", self.next);
            self.next += 1;

            if !self.reserved.iter().any(|reserved| reserved == &alias) {
                return alias;
            }
        }
    }

    fn reserve_aliases(&mut self, aliases: &[String]) {
        for alias in aliases {
            if !self.reserved.iter().any(|reserved| reserved == alias) {
                self.reserved.push(alias.clone());
            }
        }
    }

    fn alias_for_path(&mut self, source_alias: &str, path: &[String]) -> String {
        if let Some(cached) = self
            .path_aliases
            .iter()
            .find(|cached| cached.source_alias == source_alias && cached.path == path)
        {
            return cached.target_alias.clone();
        }

        let target_alias = self.next_alias();
        self.path_aliases.push(SQLiteJoinPathAlias {
            source_alias: source_alias.to_string(),
            path: path.to_vec(),
            target_alias: target_alias.clone(),
        });
        target_alias
    }
}

struct SQLiteSelectedShapeAliases {
    aliases: Vec<SQLiteSelectedShapeAlias>,
}

struct SQLiteSelectedShapeAlias {
    shape_path: Vec<usize>,
    sql_alias: String,
}

impl SQLiteSelectedShapeAliases {
    fn alias_for_path(&self, shape_path: &[usize]) -> &str {
        self.aliases
            .iter()
            .find(|alias| alias.shape_path == shape_path)
            .expect("selected shape alias should exist for nested field")
            .sql_alias
            .as_str()
    }
}

fn plan_selected_shape_aliases(
    shape: &query_ir::ResolvedShape,
    reserved_root_path_aliases: Vec<String>,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> SQLiteSelectedShapeAliases {
    let mut aliases = Vec::new();
    let mut used_aliases = vec!["root".to_string()];
    used_aliases.extend(reserved_root_path_aliases);
    used_aliases.extend(root_selected_link_aliases(shape));
    collect_selected_shape_aliases(shape, &[], &mut used_aliases, &mut aliases, join_aliases);

    SQLiteSelectedShapeAliases { aliases }
}

fn root_selected_link_aliases(shape: &query_ir::ResolvedShape) -> Vec<String> {
    shape
        .items()
        .iter()
        .filter_map(|item| {
            let query_ir::ResolvedShapeItem::Field(field) = item else {
                return None;
            };

            (field.cardinality() != Cardinality::Many && field.child_shape().is_some())
                .then(|| field.output_name().to_string())
        })
        .collect()
}

fn collect_selected_shape_aliases(
    shape: &query_ir::ResolvedShape,
    shape_path: &[usize],
    used_aliases: &mut Vec<String>,
    aliases: &mut Vec<SQLiteSelectedShapeAlias>,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) {
    for (index, item) in shape.items().iter().enumerate() {
        let query_ir::ResolvedShapeItem::Field(field) = item else {
            continue;
        };

        let Some(child_shape) = field.child_shape() else {
            continue;
        };
        if field.cardinality() == Cardinality::Many {
            continue;
        }

        let mut child_path = shape_path.to_vec();
        child_path.push(index);
        let preferred_alias = field.output_name();
        let conflicts_with_existing_alias = used_aliases
            .iter()
            .any(|used_alias| used_alias == preferred_alias);
        let sql_alias = if !shape_path.is_empty() && conflicts_with_existing_alias {
            join_aliases.next_alias()
        } else {
            preferred_alias.to_string()
        };

        used_aliases.push(sql_alias.clone());
        aliases.push(SQLiteSelectedShapeAlias {
            shape_path: child_path.clone(),
            sql_alias,
        });
        collect_selected_shape_aliases(
            child_shape,
            &child_path,
            used_aliases,
            aliases,
            join_aliases,
        );
    }
}

fn root_path_aliases(ir: &SelectQuery) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(filter) = ir.filter() {
        collect_root_path_aliases_from_expr(filter, &mut aliases);
    }

    for order in ir.order_by() {
        collect_root_path_aliases_from_value(order.value(), &mut aliases);
    }

    collect_root_computed_path_aliases(ir.shape(), &mut aliases);

    aliases
}

fn collect_root_computed_path_aliases(shape: &query_ir::ResolvedShape, aliases: &mut Vec<String>) {
    for item in shape.items() {
        let query_ir::ResolvedShapeItem::Computed(computed) = item else {
            continue;
        };

        collect_root_path_aliases_from_value(computed.value(), aliases);
    }
}

fn collect_root_path_aliases_from_expr(expr: &Expr, aliases: &mut Vec<String>) {
    match expr {
        Expr::Compare(compare) => {
            collect_root_path_aliases_from_value(compare.left(), aliases);
            collect_root_path_aliases_from_value(compare.right(), aliases);
        }
        Expr::IsNull(value) | Expr::IsNotNull(value) => {
            collect_root_path_aliases_from_value(value, aliases);
        }
        Expr::In(in_expr) => collect_root_path_aliases_from_value(in_expr.left(), aliases),
        Expr::And(left, right) | Expr::Or(left, right) => {
            collect_root_path_aliases_from_expr(left, aliases);
            collect_root_path_aliases_from_expr(right, aliases);
        }
        Expr::Not(inner) => collect_root_path_aliases_from_expr(inner, aliases),
    }
}

fn collect_root_path_aliases_from_value(value: &query_ir::ValueExpr, aliases: &mut Vec<String>) {
    match value {
        query_ir::ValueExpr::Path(path) => collect_root_path_alias_from_path(path, aliases),
        query_ir::ValueExpr::Literal(_) => {}
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            collect_root_path_aliases_from_value(arithmetic.left(), aliases);
            collect_root_path_aliases_from_value(arithmetic.right(), aliases);
        }
        query_ir::ValueExpr::UnaryArithmetic(unary) => {
            collect_root_path_aliases_from_value(unary.operand(), aliases);
        }
        query_ir::ValueExpr::Cast(cast) => {
            collect_root_path_aliases_from_value(cast.operand(), aliases);
        }
        query_ir::ValueExpr::StringFunction(function) => {
            for arg in function.args() {
                collect_root_path_aliases_from_value(arg.value(), aliases);
            }
        }
    }
}

fn collect_root_path_alias_from_path(path: &query_ir::ResolvedPath, aliases: &mut Vec<String>) {
    let Some(first_step) = path.steps().first() else {
        return;
    };

    if matches!(
        first_step.kind(),
        query_ir::ResolvedPathStepKind::Link { .. }
    ) {
        aliases.push(first_step.field().name().to_string());
    }
}

fn selected_field_column_names(shape: &query_ir::ResolvedShape) -> Vec<String> {
    let mut column_names = Vec::new();
    collect_selected_field_column_names(shape, false, &mut column_names);
    column_names
}

fn collect_selected_field_column_names(
    shape: &query_ir::ResolvedShape,
    include_identity: bool,
    column_names: &mut Vec<String>,
) {
    if include_identity {
        column_names.push("id".to_string());
    }

    for item in shape.items() {
        if let query_ir::ResolvedShapeItem::Field(field) = item {
            match field.child_shape() {
                Some(child_shape) if field.cardinality() != Cardinality::Many => {
                    collect_selected_field_column_names(child_shape, true, column_names);
                }
                Some(_) => {}
                None => column_names.push(field.field().name().to_string()),
            }
        }
    }
}

fn selected_link_aliases(shape: &query_ir::ResolvedShape) -> Vec<String> {
    let mut aliases = Vec::new();
    collect_selected_link_aliases(shape, &mut aliases);
    aliases
}

fn collect_selected_link_aliases(shape: &query_ir::ResolvedShape, aliases: &mut Vec<String>) {
    for item in shape.items() {
        if let query_ir::ResolvedShapeItem::Field(field) = item
            && let Some(child_shape) = field.child_shape()
            && field.cardinality() != Cardinality::Many
        {
            aliases.push(field.output_name().to_string());
            collect_selected_link_aliases(child_shape, aliases);
        }
    }
}

fn plan_shape_values(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    source_nullable: bool,
    shape_path: &[usize],
    selected_shape_aliases: &SQLiteSelectedShapeAliases,
    computed_aliases: &mut SQLiteComputedAliasAllocator,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedShapeValues {
    let mut values = Vec::new();
    let mut joins = Vec::new();

    for (index, item) in shape.items().iter().enumerate() {
        match item {
            query_ir::ResolvedShapeItem::Field(field) => match field.child_shape() {
                Some(_) if field.cardinality() == Cardinality::Many => {}
                Some(child_shape) => {
                    let mut child_path = shape_path.to_vec();
                    child_path.push(index);
                    let nested_alias = selected_shape_aliases.alias_for_path(&child_path);
                    let child_id_field = object_identity_field(child_shape.source_object_type());

                    values.push(SQLiteSelectValue::from_field(
                        nested_alias,
                        child_id_field,
                        None,
                    ));

                    let planned_child_values = plan_shape_values(
                        child_shape,
                        nested_alias,
                        source_nullable || field.cardinality() == Cardinality::Optional,
                        &child_path,
                        selected_shape_aliases,
                        computed_aliases,
                        join_aliases,
                    );
                    values.extend(planned_child_values.values);
                    joins.extend(planned_child_values.joins);
                }
                None => values.push(SQLiteSelectValue::from_field(
                    source_alias,
                    field.field().clone(),
                    Some(field.output_name().to_string()),
                )),
            },
            query_ir::ResolvedShapeItem::Computed(computed) => {
                let planned = plan_value_expr(
                    computed.value(),
                    source_alias,
                    source_nullable,
                    join_aliases,
                );
                values.push(SQLiteSelectValue::computed(
                    computed.output_name(),
                    computed_aliases.next_alias(),
                    planned.value,
                ));
                joins.extend(planned.joins);
            }
        }
    }

    PlannedShapeValues { values, joins }
}

fn plan_selected_shape_joins(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    source_nullable: bool,
    shape_path: &[usize],
    selected_shape_aliases: &SQLiteSelectedShapeAliases,
) -> Vec<SQLiteJoin> {
    let mut joins = Vec::new();

    for (index, item) in shape.items().iter().enumerate() {
        let query_ir::ResolvedShapeItem::Field(field) = item else {
            continue;
        };

        let Some(child_shape) = field.child_shape() else {
            continue;
        };
        if field.cardinality() == Cardinality::Many {
            continue;
        }

        let mut child_path = shape_path.to_vec();
        child_path.push(index);
        let target_alias = selected_shape_aliases.alias_for_path(&child_path);
        let join_cardinality = path_step_join_cardinality(source_nullable, field.cardinality());

        joins.push(SQLiteJoin::selected_single_link_with_alias(
            source_alias,
            field,
            target_alias,
            join_cardinality,
        ));
        joins.extend(plan_selected_shape_joins(
            child_shape,
            target_alias,
            source_nullable || field.cardinality() == Cardinality::Optional,
            &child_path,
            selected_shape_aliases,
        ));
    }

    joins
}

fn plan_result_shape(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    include_identity: bool,
    shape_path: &[usize],
    selected_shape_aliases: &SQLiteSelectedShapeAliases,
    computed_aliases: &mut SQLiteComputedAliasAllocator,
    next_follow_up_fetch_index: &mut usize,
) -> SQLiteResultShapePlan {
    let fields = shape
        .items()
        .iter()
        .enumerate()
        .map(|(index, item)| match item {
            query_ir::ResolvedShapeItem::Field(field) => match field.child_shape() {
                Some(_) if field.cardinality() == Cardinality::Many => {
                    let follow_up_fetch_index = *next_follow_up_fetch_index;
                    *next_follow_up_fetch_index += 1;

                    SQLiteResultField {
                        output_name: field.output_name().to_string(),
                        cardinality: field.cardinality(),
                        value: None,
                        nested_shape: None,
                        follow_up_fetch_index: Some(follow_up_fetch_index),
                    }
                }
                Some(child_shape) => {
                    let mut child_path = shape_path.to_vec();
                    child_path.push(index);
                    let nested_alias = selected_shape_aliases.alias_for_path(&child_path);

                    SQLiteResultField {
                        output_name: field.output_name().to_string(),
                        cardinality: field.cardinality(),
                        value: None,
                        nested_shape: Some(plan_result_shape(
                            child_shape,
                            nested_alias,
                            true,
                            &child_path,
                            selected_shape_aliases,
                            computed_aliases,
                            next_follow_up_fetch_index,
                        )),
                        follow_up_fetch_index: None,
                    }
                }
                None => SQLiteResultField {
                    output_name: field.output_name().to_string(),
                    cardinality: field.cardinality(),
                    value: Some(SQLiteResultValueRef {
                        source_alias: source_alias.to_string(),
                        column_name: field.field().name().to_string(),
                        role: SQLiteValueRole::for_field(field.field()),
                    }),
                    nested_shape: None,
                    follow_up_fetch_index: None,
                },
            },
            query_ir::ResolvedShapeItem::Computed(computed) => SQLiteResultField {
                output_name: computed.output_name().to_string(),
                cardinality: computed.cardinality(),
                value: Some(SQLiteResultValueRef {
                    source_alias: source_alias.to_string(),
                    column_name: computed_aliases.next_alias(),
                    role: SQLiteValueRole::Computed,
                }),
                nested_shape: None,
                follow_up_fetch_index: None,
            },
        })
        .collect();

    SQLiteResultShapePlan {
        identity_value: include_identity.then(|| SQLiteResultValueRef {
            source_alias: source_alias.to_string(),
            column_name: "id".to_string(),
            role: SQLiteValueRole::ObjectId,
        }),
        fields,
    }
}

fn plan_follow_up_fetches(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    shape_path: &[usize],
    selected_shape_aliases: &SQLiteSelectedShapeAliases,
) -> Vec<SQLiteFollowUpFetchPlan> {
    let mut fetches = Vec::new();

    for (index, item) in shape.items().iter().enumerate() {
        let query_ir::ResolvedShapeItem::Field(field) = item else {
            continue;
        };
        let Some(child_shape) = field.child_shape() else {
            continue;
        };

        if field.cardinality() == Cardinality::Many {
            fetches.push(plan_follow_up_fetch(field, child_shape, source_alias));
            continue;
        }

        let mut child_path = shape_path.to_vec();
        child_path.push(index);
        fetches.extend(plan_follow_up_fetches(
            child_shape,
            selected_shape_aliases.alias_for_path(&child_path),
            &child_path,
            selected_shape_aliases,
        ));
    }

    fetches
}

fn plan_follow_up_fetch(
    field: &query_ir::ResolvedShapeField,
    target_shape: &query_ir::ResolvedShape,
    parent_alias: &str,
) -> SQLiteFollowUpFetchPlan {
    let target_object_type = target_shape.source_object_type().clone();
    let mut selected_column_names = selected_field_column_names(target_shape);
    selected_column_names.insert(0, "id".to_string());
    let mut select_aliases = SQLiteComputedAliasAllocator::new(selected_column_names.clone());
    let mut join_aliases = SQLiteJoinAliasAllocator::new(selected_link_aliases(target_shape));
    let mut root_path_aliases = Vec::new();
    collect_root_computed_path_aliases(target_shape, &mut root_path_aliases);
    join_aliases.reserve_aliases(&root_path_aliases);
    let selected_shape_aliases =
        plan_selected_shape_aliases(target_shape, root_path_aliases, &mut join_aliases);
    let planned_values = plan_shape_values(
        target_shape,
        "root",
        false,
        &[],
        &selected_shape_aliases,
        &mut select_aliases,
        &mut join_aliases,
    );
    let mut selected_values = vec![SQLiteSelectValue::from_field(
        "root",
        object_identity_field(&target_object_type),
        None,
    )];
    selected_values.extend(planned_values.values);
    let mut joins =
        plan_selected_shape_joins(target_shape, "root", false, &[], &selected_shape_aliases);
    joins.extend(planned_values.joins);
    let mut result_aliases = SQLiteComputedAliasAllocator::new(selected_column_names);
    let mut next_follow_up_fetch_index = 0;
    let result_shape = plan_result_shape(
        target_shape,
        "root",
        true,
        &[],
        &selected_shape_aliases,
        &mut result_aliases,
        &mut next_follow_up_fetch_index,
    );
    let follow_up_fetches =
        plan_follow_up_fetches(target_shape, "root", &[], &selected_shape_aliases);

    SQLiteFollowUpFetchPlan {
        parent_field: field.field().clone(),
        parent_identity: SQLiteResultValueRef {
            source_alias: parent_alias.to_string(),
            column_name: "id".to_string(),
            role: SQLiteValueRole::ObjectId,
        },
        join_table_name: format!(
            "{}__{}",
            sqlite_table_name(field.field().owner_object_type()),
            field.field().name()
        ),
        target_source: SQLiteObjectSource {
            object_type: target_object_type.clone(),
            table_name: sqlite_table_name(&target_object_type),
            alias: "root".to_string(),
            id_column: "id".to_string(),
        },
        selected_values,
        joins: dedup_joins(joins),
        result_shape,
        follow_up_fetches,
    }
}

/// Result-shaping plan for one object level.
pub struct SQLiteResultShapePlan {
    identity_value: Option<SQLiteResultValueRef>,
    fields: Vec<SQLiteResultField>,
}

impl SQLiteResultShapePlan {
    pub fn fields(&self) -> &[SQLiteResultField] {
        &self.fields
    }
    pub fn identity_value(&self) -> Option<&SQLiteResultValueRef> {
        self.identity_value.as_ref()
    }
}

/// One output field in a result-shaping plan.
pub struct SQLiteResultField {
    output_name: String,
    cardinality: schema_model::Cardinality,
    value: Option<SQLiteResultValueRef>,
    nested_shape: Option<SQLiteResultShapePlan>,
    follow_up_fetch_index: Option<usize>,
}

impl SQLiteResultField {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn cardinality(&self) -> schema_model::Cardinality {
        self.cardinality
    }

    pub fn value(&self) -> Option<&SQLiteResultValueRef> {
        self.value.as_ref()
    }

    pub fn nested_shape(&self) -> Option<&SQLiteResultShapePlan> {
        self.nested_shape.as_ref()
    }

    /// Index into the containing plan's [`SQLiteSelectPlan::follow_up_fetches`]
    /// or [`SQLiteFollowUpFetchPlan::follow_up_fetches`] list.
    pub fn follow_up_fetch_index(&self) -> Option<usize> {
        self.follow_up_fetch_index
    }
}

/// Reference to a selected value used while shaping rows into objects.
pub struct SQLiteResultValueRef {
    source_alias: String,
    column_name: String,
    role: SQLiteValueRole,
}

impl SQLiteResultValueRef {
    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

/// Physical root table for a SQLite query.
pub struct SQLiteObjectSource {
    object_type: ObjectTypeRef,
    table_name: String,
    alias: String,
    id_column: String,
}

impl SQLiteObjectSource {
    pub fn object_type(&self) -> &ObjectTypeRef {
        &self.object_type
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn id_column(&self) -> &str {
        &self.id_column
    }
}

/// SQLite sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteOrderDirection {
    Asc,
    Desc,
}

impl SQLiteOrderDirection {
    pub fn from_ir(direction: query_ir::OrderDirection) -> Self {
        match direction {
            query_ir::OrderDirection::Asc => Self::Asc,
            query_ir::OrderDirection::Desc => Self::Desc,
        }
    }
}

/// Planned SQLite ordering item.
pub struct SQLiteOrder {
    value: SQLiteValueExpr,
    direction: SQLiteOrderDirection,
}

impl SQLiteOrder {
    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
    }

    pub fn direction(&self) -> SQLiteOrderDirection {
        self.direction
    }
}

/// Backend-specific predicate expression.
pub enum SQLiteWhereExpr {
    Compare(SQLiteCompareExpr),
    IsNull(SQLiteValueExpr),
    IsNotNull(SQLiteValueExpr),
    In(SQLiteInExpr),
    And(Box<SQLiteWhereExpr>, Box<SQLiteWhereExpr>),
    Or(Box<SQLiteWhereExpr>, Box<SQLiteWhereExpr>),
    Not(Box<SQLiteWhereExpr>),
}

/// Backend-specific comparison expression.
pub struct SQLiteCompareExpr {
    left: SQLiteValueExpr,
    op: SQLiteCompareOp,
    right: SQLiteValueExpr,
}

impl SQLiteCompareExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteCompareOp {
        self.op
    }

    pub fn right(&self) -> &SQLiteValueExpr {
        &self.right
    }
}

/// Backend-specific value expression.
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteValueExpr {
    Column(SQLiteColumnRef),
    Literal(SQLiteLiteral),
    Arithmetic(SQLiteArithmeticExpr),
    UnaryArithmetic(SQLiteUnaryArithmeticExpr),
    Cast(SQLiteCastExpr),
    StringFunction(SQLiteStringFunctionExpr),
}

/// Backend-specific string value function expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteStringFunctionExpr {
    kind: SQLiteStringFunctionKind,
    args: Vec<SQLiteStringFunctionArg>,
}

impl SQLiteStringFunctionExpr {
    pub fn kind(&self) -> SQLiteStringFunctionKind {
        self.kind
    }

    pub fn args(&self) -> &[SQLiteStringFunctionArg] {
        &self.args
    }
}

/// SQLite string function kinds emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteStringFunctionKind {
    Concat,
    Str,
}

impl SQLiteStringFunctionKind {
    fn from_ir(kind: query_ir::StringFunctionKind) -> Self {
        match kind {
            query_ir::StringFunctionKind::Concat => Self::Concat,
            query_ir::StringFunctionKind::Str => Self::Str,
        }
    }
}

/// One SQLite string function argument.
#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteStringFunctionArg {
    value: SQLiteValueExpr,
    scalar_type: schema_model::ScalarType,
}

impl SQLiteStringFunctionArg {
    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
    }

    pub fn scalar_type(&self) -> schema_model::ScalarType {
        self.scalar_type
    }
}

/// Backend-specific scalar cast value expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteCastExpr {
    operand: Box<SQLiteValueExpr>,
    target: SQLiteCastTarget,
}

impl SQLiteCastExpr {
    pub fn operand(&self) -> &SQLiteValueExpr {
        &self.operand
    }

    pub fn target(&self) -> SQLiteCastTarget {
        self.target
    }
}

/// SQLite cast targets emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteCastTarget {
    Int64,
    Float64,
}

impl SQLiteCastTarget {
    fn from_scalar_type(scalar_type: schema_model::ScalarType) -> Self {
        match scalar_type {
            schema_model::ScalarType::Int64 => Self::Int64,
            schema_model::ScalarType::Float64 => Self::Float64,
            schema_model::ScalarType::Str
            | schema_model::ScalarType::Bool
            | schema_model::ScalarType::Uuid
            | schema_model::ScalarType::DateTime => {
                unreachable!("SQLite planner receives only resolver-accepted numeric casts")
            }
        }
    }
}

/// Backend-specific arithmetic value expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteArithmeticExpr {
    left: Box<SQLiteValueExpr>,
    op: SQLiteArithmeticOp,
    right: Box<SQLiteValueExpr>,
}

impl SQLiteArithmeticExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteArithmeticOp {
        self.op
    }

    pub fn right(&self) -> &SQLiteValueExpr {
        &self.right
    }
}

/// SQLite arithmetic operators emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl SQLiteArithmeticOp {
    pub fn from_ir(op: query_ir::ArithmeticOp) -> Self {
        match op {
            query_ir::ArithmeticOp::Add => Self::Add,
            query_ir::ArithmeticOp::Sub => Self::Sub,
            query_ir::ArithmeticOp::Mul => Self::Mul,
            query_ir::ArithmeticOp::Div => Self::Div,
            query_ir::ArithmeticOp::Mod => Self::Mod,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteUnaryArithmeticExpr {
    op: SQLiteUnaryArithmeticOp,
    operand: Box<SQLiteValueExpr>,
}

impl SQLiteUnaryArithmeticExpr {
    pub fn op(&self) -> SQLiteUnaryArithmeticOp {
        self.op
    }

    pub fn operand(&self) -> &SQLiteValueExpr {
        &self.operand
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteUnaryArithmeticOp {
    Plus,
    Minus,
}

impl SQLiteUnaryArithmeticOp {
    pub fn from_ir(op: query_ir::UnaryArithmeticOp) -> Self {
        match op {
            query_ir::UnaryArithmeticOp::Plus => Self::Plus,
            query_ir::UnaryArithmeticOp::Minus => Self::Minus,
        }
    }
}

/// Backend-specific membership expression.
pub struct SQLiteInExpr {
    left: SQLiteValueExpr,
    op: SQLiteInOp,
    right: SQLiteInRhs,
}

impl SQLiteInExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteInOp {
        self.op
    }

    pub fn right(&self) -> &SQLiteInRhs {
        &self.right
    }
}

/// Backend-specific membership right-hand side.
pub enum SQLiteInRhs {
    List(Vec<SQLiteValueExpr>),
    Select(Box<SQLiteSelectPlan>),
}

/// Backend-specific membership operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteInOp {
    In,
    NotIn,
}

impl SQLiteInOp {
    pub fn from_ir(op: InOp) -> Self {
        match op {
            InOp::In => Self::In,
            InOp::NotIn => Self::NotIn,
        }
    }
}

/// Physical column reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SQLiteColumnRef {
    source_alias: String,
    column_name: String,
}

impl SQLiteColumnRef {
    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }
}

struct PlannedPath {
    column: SQLiteColumnRef,
    joins: Vec<SQLiteJoin>,
}

struct PathPlanningState {
    current_alias: String,
    current_nullable: bool,
    alias_parts: Vec<String>,
    path_parts: Vec<String>,
    joins: Vec<SQLiteJoin>,
}

impl PathPlanningState {
    fn new(source_alias: &str, source_nullable: bool) -> Self {
        Self {
            current_alias: source_alias.to_string(),
            current_nullable: source_nullable,
            alias_parts: initial_path_alias_parts(source_alias),
            path_parts: Vec::new(),
            joins: Vec::new(),
        }
    }
}

fn plan_resolved_path(
    path: &query_ir::ResolvedPath,
    source_alias: &str,
    source_nullable: bool,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedPath {
    let mut state = PathPlanningState::new(source_alias, source_nullable);
    let mut column = None;

    for (index, step) in path.steps().iter().enumerate() {
        let is_last = index == path.steps().len() - 1;

        match step.kind() {
            query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                plan_link_path_step(step, target_object_type, is_last, &mut state, join_aliases);
            }
            query_ir::ResolvedPathStepKind::Scalar => {
                column = Some(plan_scalar_path_step(step, is_last, &state));
            }
        }
    }

    PlannedPath {
        column: column.expect("resolved path should end in a scalar field"),
        joins: state.joins,
    }
}

fn initial_path_alias_parts(source_alias: &str) -> Vec<String> {
    if source_alias == "root" {
        Vec::new()
    } else {
        vec![source_alias.to_string()]
    }
}

fn plan_link_path_step(
    step: &query_ir::ResolvedPathStep,
    target_object_type: &ObjectTypeRef,
    is_last: bool,
    state: &mut PathPlanningState,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) {
    if is_last {
        todo!("link-only paths cannot be lowered to SQLite columns");
    }

    let source_alias = state.current_alias.clone();
    state.alias_parts.push(step.field().name().to_string());
    state.path_parts.push(step.field().name().to_string());
    let target_alias = path_step_target_alias(&source_alias, state, join_aliases);
    let join_cardinality = path_step_join_cardinality(state.current_nullable, step.cardinality());

    state.joins.push(SQLiteJoin::path_traversal(
        source_alias,
        step.field(),
        target_object_type,
        target_alias.clone(),
        state.path_parts.clone(),
        join_cardinality,
    ));

    state.current_alias = target_alias;
    state.current_nullable = state.current_nullable || step.cardinality() == Cardinality::Optional;
}

fn path_step_target_alias(
    source_alias: &str,
    state: &PathPlanningState,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> String {
    if source_alias == "root" {
        let alias = state.alias_parts.join("_");
        if alias == source_alias {
            join_aliases.alias_for_path(source_alias, &state.path_parts)
        } else {
            alias
        }
    } else {
        join_aliases.alias_for_path(source_alias, &state.path_parts)
    }
}

fn path_step_join_cardinality(
    current_nullable: bool,
    step_cardinality: Cardinality,
) -> Cardinality {
    match (current_nullable, step_cardinality) {
        (_, Cardinality::Many) => Cardinality::Many,
        (true, _) => Cardinality::Optional,
        (false, cardinality) => cardinality,
    }
}

fn plan_scalar_path_step(
    step: &query_ir::ResolvedPathStep,
    is_last: bool,
    state: &PathPlanningState,
) -> SQLiteColumnRef {
    if !is_last {
        todo!("scalar path step before terminal position is not supported");
    }

    SQLiteColumnRef {
        source_alias: state.current_alias.clone(),
        column_name: step.field().name().to_string(),
    }
}

struct PlannedValueExpr {
    value: SQLiteValueExpr,
    joins: Vec<SQLiteJoin>,
}

fn plan_value_expr(
    expr: &query_ir::ValueExpr,
    source_alias: &str,
    source_nullable: bool,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedValueExpr {
    match expr {
        query_ir::ValueExpr::Path(path) => {
            let planned_path =
                plan_resolved_path(path, source_alias, source_nullable, join_aliases);

            PlannedValueExpr {
                value: SQLiteValueExpr::Column(planned_path.column),
                joins: planned_path.joins,
            }
        }
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::String(
                value.clone(),
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Int64(
                *value,
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Float64(
                *value,
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Bool(
                *value,
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Null) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Null)),
            joins: vec![],
        },
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            let left = plan_value_expr(
                arithmetic.left(),
                source_alias,
                source_nullable,
                join_aliases,
            );
            let right = plan_value_expr(
                arithmetic.right(),
                source_alias,
                source_nullable,
                join_aliases,
            );

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedValueExpr {
                value: SQLiteValueExpr::Arithmetic(SQLiteArithmeticExpr {
                    left: Box::new(left.value),
                    op: SQLiteArithmeticOp::from_ir(arithmetic.op()),
                    right: Box::new(right.value),
                }),
                joins,
            }
        }
        query_ir::ValueExpr::UnaryArithmetic(unary) => {
            let operand =
                plan_value_expr(unary.operand(), source_alias, source_nullable, join_aliases);

            PlannedValueExpr {
                value: SQLiteValueExpr::UnaryArithmetic(SQLiteUnaryArithmeticExpr {
                    op: SQLiteUnaryArithmeticOp::from_ir(unary.op()),
                    operand: Box::new(operand.value),
                }),
                joins: operand.joins,
            }
        }
        query_ir::ValueExpr::Cast(cast) => {
            let operand =
                plan_value_expr(cast.operand(), source_alias, source_nullable, join_aliases);

            PlannedValueExpr {
                value: SQLiteValueExpr::Cast(SQLiteCastExpr {
                    operand: Box::new(operand.value),
                    target: SQLiteCastTarget::from_scalar_type(cast.target_type()),
                }),
                joins: operand.joins,
            }
        }
        query_ir::ValueExpr::StringFunction(function) => {
            let mut args = Vec::new();
            let mut joins = Vec::new();

            for arg in function.args() {
                let planned =
                    plan_value_expr(arg.value(), source_alias, source_nullable, join_aliases);
                joins.extend(planned.joins);
                args.push(SQLiteStringFunctionArg {
                    value: planned.value,
                    scalar_type: arg.scalar_type(),
                });
            }

            PlannedValueExpr {
                value: SQLiteValueExpr::StringFunction(SQLiteStringFunctionExpr {
                    kind: SQLiteStringFunctionKind::from_ir(function.kind()),
                    args,
                }),
                joins,
            }
        }
    }
}

fn sqlite_literal_from_ir(literal: &query_ir::Literal) -> SQLiteLiteral {
    match literal {
        query_ir::Literal::String(value) => SQLiteLiteral::String(value.clone()),
        query_ir::Literal::Int64(value) => SQLiteLiteral::Int64(*value),
        query_ir::Literal::Float64(value) => SQLiteLiteral::Float64(*value),
        query_ir::Literal::Bool(value) => SQLiteLiteral::Bool(*value),
        query_ir::Literal::Null => SQLiteLiteral::Null,
    }
}

struct PlannedWhereExpr {
    expr: SQLiteWhereExpr,
    joins: Vec<SQLiteJoin>,
}

fn plan_where_expr(expr: &Expr, join_aliases: &mut SQLiteJoinAliasAllocator) -> PlannedWhereExpr {
    match expr {
        Expr::Compare(compare) => {
            let left = plan_value_expr(compare.left(), "root", false, join_aliases);
            let right = plan_value_expr(compare.right(), "root", false, join_aliases);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Compare(SQLiteCompareExpr {
                    left: left.value,
                    op: SQLiteCompareOp::from_ir(compare.op()),
                    right: right.value,
                }),
                joins,
            }
        }
        Expr::IsNull(value) => {
            let value = plan_value_expr(value, "root", false, join_aliases);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNull(value.value),
                joins: value.joins,
            }
        }
        Expr::IsNotNull(value) => {
            let value = plan_value_expr(value, "root", false, join_aliases);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNotNull(value.value),
                joins: value.joins,
            }
        }
        Expr::In(in_expr) => {
            let left = plan_value_expr(in_expr.left(), "root", false, join_aliases);
            let joins = left.joins;
            let right = match in_expr.right() {
                query_ir::InRhs::List(values) => SQLiteInRhs::List(
                    values
                        .iter()
                        .map(|value| plan_value_expr(value, "root", false, join_aliases).value)
                        .collect(),
                ),
                query_ir::InRhs::Select(select) => {
                    SQLiteInRhs::Select(Box::new(plan_select(select)))
                }
            };

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::In(SQLiteInExpr {
                    left: left.value,
                    op: SQLiteInOp::from_ir(in_expr.op()),
                    right,
                }),
                joins,
            }
        }
        Expr::And(left, right) => {
            let left = plan_where_expr(left, join_aliases);
            let right = plan_where_expr(right, join_aliases);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::And(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Or(left, right) => {
            let left = plan_where_expr(left, join_aliases);
            let right = plan_where_expr(right, join_aliases);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Or(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Not(inner) => {
            let inner = plan_where_expr(inner, join_aliases);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Not(Box::new(inner.expr)),
                joins: inner.joins,
            }
        }
    }
}

struct PlannedOrder {
    order: SQLiteOrder,
    joins: Vec<SQLiteJoin>,
}

fn plan_order_expr(
    order: &query_ir::OrderExpr,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedOrder {
    let planned_value = plan_value_expr(order.value(), "root", false, join_aliases);

    PlannedOrder {
        order: SQLiteOrder {
            value: planned_value.value,
            direction: SQLiteOrderDirection::from_ir(order.direction()),
        },
        joins: planned_value.joins,
    }
}

/// Literal values supported by SQLite SQL generation.
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteLiteral {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

/// SQLite comparison operators currently emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteCompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl SQLiteCompareOp {
    pub fn from_ir(compare_op: CompareOp) -> Self {
        match compare_op {
            CompareOp::Eq => Self::Eq,
            CompareOp::Ne => Self::Ne,
            CompareOp::Lt => Self::Lt,
            CompareOp::Le => Self::Le,
            CompareOp::Gt => Self::Gt,
            CompareOp::Ge => Self::Ge,
        }
    }
}

/// Logical reason a join exists in the plan.
///
/// This is used for tests and future explain output; SQL generation only needs
/// the physical join fields.
pub enum SQLiteJoinReason {
    SelectedSingleLink { field: FieldRef },
    PathTraversal { path: Vec<String> },
}

/// SQLite join kind chosen from relation cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteJoinKind {
    Inner,
    Left,
}

impl SQLiteJoinKind {
    pub fn for_single_link(cardinality: schema_model::Cardinality) -> Self {
        match cardinality {
            Cardinality::Required => Self::Inner,
            Cardinality::Optional => Self::Left,
            Cardinality::Many => {
                todo!("multi link joins are not supported yet")
            }
        }
    }
}

/// Equality condition connecting two aliases in a join.
pub struct SQLiteJoinCondition {
    left_alias: String,
    left_column: String,
    right_alias: String,
    right_column: String,
}
impl SQLiteJoinCondition {
    pub fn left_alias(&self) -> &str {
        &self.left_alias
    }

    pub fn left_column(&self) -> &str {
        &self.left_column
    }

    pub fn right_alias(&self) -> &str {
        &self.right_alias
    }

    pub fn right_column(&self) -> &str {
        &self.right_column
    }
}

pub struct SQLiteJoin {
    kind: SQLiteJoinKind,
    source_alias: String,
    target_table: String,
    target_alias: String,
    on: SQLiteJoinCondition,
    reason: SQLiteJoinReason,
}

impl SQLiteJoin {
    pub fn selected_single_link(
        source_alias: &str,
        shape_field: &query_ir::ResolvedShapeField,
        cardinality: Cardinality,
    ) -> Self {
        Self::selected_single_link_with_alias(
            source_alias,
            shape_field,
            shape_field.output_name(),
            cardinality,
        )
    }

    fn selected_single_link_with_alias(
        source_alias: &str,
        shape_field: &query_ir::ResolvedShapeField,
        target_alias: &str,
        cardinality: Cardinality,
    ) -> Self {
        let child_shape = shape_field
            .child_shape()
            .expect("selected link field must have child shape");

        let field = shape_field.field().clone();

        Self {
            kind: SQLiteJoinKind::for_single_link(cardinality),
            source_alias: source_alias.to_string(),
            target_table: child_shape
                .source_object_type()
                .name()
                .to_ascii_lowercase()
                .to_string(),
            target_alias: target_alias.to_string(),
            on: SQLiteJoinCondition {
                left_alias: source_alias.to_string(),
                left_column: format!("{}_id", field.name()),
                right_alias: target_alias.to_string(),
                right_column: "id".to_string(),
            },
            reason: SQLiteJoinReason::SelectedSingleLink { field },
        }
    }

    fn path_traversal(
        source_alias: impl Into<String>,
        link_field: &FieldRef,
        target_object_type: &ObjectTypeRef,
        target_alias: impl Into<String>,
        path: Vec<String>,
        cardinality: Cardinality,
    ) -> Self {
        let source_alias = source_alias.into();
        let target_alias = target_alias.into();

        Self {
            kind: SQLiteJoinKind::for_single_link(cardinality),
            source_alias: source_alias.clone(),
            target_table: target_object_type.name().to_ascii_lowercase().to_string(),
            target_alias: target_alias.clone(),
            on: SQLiteJoinCondition {
                left_alias: source_alias,
                left_column: format!("{}_id", link_field.name()),
                right_alias: target_alias,
                right_column: "id".to_string(),
            },
            reason: SQLiteJoinReason::PathTraversal { path },
        }
    }

    pub fn kind(&self) -> SQLiteJoinKind {
        self.kind
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn target_table(&self) -> &str {
        &self.target_table
    }

    pub fn target_alias(&self) -> &str {
        &self.target_alias
    }

    pub fn on(&self) -> &SQLiteJoinCondition {
        &self.on
    }

    pub fn reason(&self) -> &SQLiteJoinReason {
        &self.reason
    }

    fn has_same_identity(&self, other: &SQLiteJoin) -> bool {
        self.kind == other.kind
            && self.source_alias == other.source_alias
            && self.target_table == other.target_table
            && self.target_alias == other.target_alias
            && self.on.left_alias == other.on.left_alias
            && self.on.left_column == other.on.left_column
            && self.on.right_alias == other.on.right_alias
            && self.on.right_column == other.on.right_column
    }
}

fn dedup_joins(joins: Vec<SQLiteJoin>) -> Vec<SQLiteJoin> {
    let mut deduped: Vec<SQLiteJoin> = Vec::new();

    for join in joins {
        if !deduped
            .iter()
            .any(|existing| existing.has_same_identity(&join))
        {
            deduped.push(join);
        }
    }

    deduped
}

#[cfg(test)]
mod tests;
