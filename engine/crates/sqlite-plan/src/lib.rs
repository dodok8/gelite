use ir::{CompareExpr, CompareOp, Expr, SelectQuery};
use schema::{Cardinality, FieldRef, ObjectTypeRef};

pub fn plan_select(ir: &SelectQuery) -> SQLiteSelectPlan {
    let root_object_type = ir.root_object_type().clone();

    let selected_values = plan_shape_values(ir.shape(), "root");

    let order_by = ir
        .order_by()
        .iter()
        .map(|order| SQLiteOrder::root_field(order))
        .collect();

    let filter = ir.filter().map(SQLiteWhereExpr::from_ir);

    let joins = ir
        .shape()
        .fields()
        .iter()
        .filter(|field| field.child_shape().is_some())
        .map(SQLiteJoin::selected_single_link)
        .collect();

    SQLiteSelectPlan {
        root_source: SQLiteObjectSource {
            table_name: root_object_type.name().to_ascii_lowercase().to_string(),
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteValueRole {
    ObjectId,
    Scalar,
}

pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    order_by: Vec<SQLiteOrder>,
    limit: Option<u64>,
    offset: Option<u64>,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
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

    pub fn limit(&self) -> Option<u64> {
        self.limit
    }

    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    pub fn filter(&self) -> &Option<SQLiteWhereExpr> {
        &self.filter
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }
}

pub struct SQLiteSelectValue {
    source_alias: String,
    column_name: String,
    output_name: String,
    field: schema::FieldRef,
    role: SQLiteValueRole,
}

impl SQLiteSelectValue {
    pub fn from_field(
        source_alias: impl Into<String>,
        field: schema::FieldRef,
        output_name: impl Into<String>,
    ) -> Self {
        Self {
            source_alias: source_alias.into(),
            column_name: field.name().to_string(),
            output_name: output_name.into(),
            role: SQLiteValueRole::for_field(&field),
            field,
        }
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn field(&self) -> &schema::FieldRef {
        &self.field
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

impl SQLiteValueRole {
    fn for_field(field: &schema::FieldRef) -> Self {
        if field.name() == "id" {
            Self::ObjectId
        } else {
            Self::Scalar
        }
    }
}

fn plan_shape_values(shape: &ir::ResolvedShape, source_alias: &str) -> Vec<SQLiteSelectValue> {
    shape
        .fields()
        .iter()
        .flat_map(|field| match field.child_shape() {
            Some(child_shape) => {
                let nested_alias = field.output_name();
                let child_id_field = FieldRef::new(
                    schema::FieldId::new(1),
                    child_shape.source_object_type().clone(),
                    "id",
                );

                let mut values = vec![SQLiteSelectValue::from_field(
                    nested_alias,
                    child_id_field,
                    "id",
                )];

                values.extend(plan_shape_values(child_shape, nested_alias));
                values
            }
            None => vec![SQLiteSelectValue::from_field(
                source_alias,
                field.field().clone(),
                field.output_name(),
            )],
        })
        .collect()
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteOrderDirection {
    Asc,
    Desc,
}

impl SQLiteOrderDirection {
    pub fn from_ir(direction: ir::OrderDirection) -> Self {
        match direction {
            ir::OrderDirection::Asc => Self::Asc,
            ir::OrderDirection::Desc => Self::Desc,
        }
    }
}

pub struct SQLiteOrder {
    source_alias: String,
    column_name: String,
    direction: SQLiteOrderDirection,
}

impl SQLiteOrder {
    pub fn root_field(order: &ir::OrderExpr) -> Self {
        let field = match order.value() {
            ir::ValueExpr::Field(field) => field,
            ir::ValueExpr::Literal(_) => todo!("ORDER BY literal is not supported yet"),
        };

        Self {
            source_alias: "root".to_string(),
            column_name: field.name().to_string(),
            direction: SQLiteOrderDirection::from_ir(order.direction()),
        }
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn direction(&self) -> SQLiteOrderDirection {
        self.direction
    }
}

pub enum SQLiteWhereExpr {
    Compare(SQLiteCompareExpr),
}

impl SQLiteWhereExpr {
    pub fn from_ir(expr: &Expr) -> Self {
        match expr {
            Expr::Compare(compare) => SQLiteWhereExpr::Compare(SQLiteCompareExpr::from_ir(compare)),
        }
    }
}

pub struct SQLiteCompareExpr {
    left: SQLiteValueExpr,
    op: SQLiteCompareOp,
    right: SQLiteValueExpr,
}

impl SQLiteCompareExpr {
    pub fn from_ir(compare: &CompareExpr) -> Self {
        SQLiteCompareExpr {
            left: SQLiteValueExpr::from_ir(compare.left()),
            op: SQLiteCompareOp::from_ir(compare.op()),
            right: SQLiteValueExpr::from_ir(compare.right()),
        }
    }

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

pub enum SQLiteValueExpr {
    Column(SQLiteColumnRef),
    Literal(SQLiteLiteral),
}

impl SQLiteValueExpr {
    pub fn from_ir(expr: &ir::ValueExpr) -> Self {
        match expr {
            ir::ValueExpr::Field(field) => SQLiteValueExpr::Column(SQLiteColumnRef {
                source_alias: "root".to_string(),
                column_name: field.name().to_string(),
            }),
            ir::ValueExpr::Literal(ir::Literal::String(value)) => {
                SQLiteValueExpr::Literal(SQLiteLiteral::String(value.clone()))
            }
        }
    }
}

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

pub enum SQLiteLiteral {
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteCompareOp {
    Eq,
}

impl SQLiteCompareOp {
    pub fn from_ir(compare_op: CompareOp) -> Self {
        match compare_op {
            CompareOp::Eq => Self::Eq,
        }
    }
}

pub enum SQLiteJoinReason {
    SelectedSingleLink { field: FieldRef },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteJoinKind {
    Inner,
    Left,
}

impl SQLiteJoinKind {
    pub fn for_selected_single_link(cardinality: schema::Cardinality) -> Self {
        match cardinality {
            Cardinality::Required => Self::Inner,
            Cardinality::Optional => Self::Left,
            Cardinality::Many => {
                todo!("multi link joins are not supported yet")
            }
        }
    }
}

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
    pub fn selected_single_link(shape_field: &ir::ResolvedShapeField) -> Self {
        let child_shape = shape_field
            .child_shape()
            .expect("selected link field must have child shape");

        let field = shape_field.field().clone();

        Self {
            kind: SQLiteJoinKind::for_selected_single_link(shape_field.cardinality()),
            source_alias: "root".to_string(),
            target_table: child_shape
                .source_object_type()
                .name()
                .to_ascii_lowercase()
                .to_string(),
            target_alias: shape_field.output_name().to_string(),
            on: SQLiteJoinCondition {
                left_alias: "root".to_string(),
                left_column: format!("{}_id", field.name()),
                right_alias: shape_field.output_name().to_string(),
                right_column: "id".to_string(),
            },
            reason: SQLiteJoinReason::SelectedSingleLink { field },
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
}

#[cfg(test)]
mod tests;
