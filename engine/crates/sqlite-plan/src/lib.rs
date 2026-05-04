use ir::{CompareExpr, CompareOp, Expr, SelectQuery};
use schema::ObjectTypeRef;

pub fn plan_select(ir: &SelectQuery) -> SQLiteSelectPlan {
    let root_object_type = ir.root_object_type().clone();

    let selected_values = ir
        .shape()
        .fields()
        .iter()
        .map(|field| SQLiteSelectValue::root_scalar(field.field().clone(), field.output_name()))
        .collect();

    let order_by = ir
        .order_by()
        .iter()
        .map(|order| SQLiteOrderBy::root_field(order))
        .collect();

    let filter = ir.filter().map(SQLiteWhereExpr::from_ir);

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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteValueRole {
    RootScalar,
}

pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    order_by: Vec<SQLiteOrderBy>,
    limit: Option<u64>,
    offset: Option<u64>,
    filter: Option<SQLiteWhereExpr>,
}

impl SQLiteSelectPlan {
    pub fn root_source(&self) -> &SQLiteObjectSource {
        &self.root_source
    }

    pub fn selected_values(&self) -> &[SQLiteSelectValue] {
        &self.selected_values
    }

    pub fn order_by(&self) -> &[SQLiteOrderBy] {
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
}

pub struct SQLiteSelectValue {
    source_alias: String,
    column_name: String,
    output_name: String,
    field: schema::FieldRef,
    role: SQLiteValueRole,
}

impl SQLiteSelectValue {
    pub fn root_scalar(field: schema::FieldRef, output_name: impl Into<String>) -> Self {
        Self {
            source_alias: "root".to_string(),
            column_name: field.name().to_string(),
            output_name: output_name.into(),
            field,
            role: SQLiteValueRole::RootScalar,
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

pub struct SQLiteOrderBy {
    source_alias: String,
    column_name: String,
    direction: SQLiteOrderDirection,
}

impl SQLiteOrderBy {
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

#[cfg(test)]
mod tests;
