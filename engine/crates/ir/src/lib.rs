use schema::{Cardinality, FieldRef, ObjectTypeRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectQuery {
    root_object_type: ObjectTypeRef,
    shape: ResolvedShape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<u64>,
    offset: Option<u64>,
}

impl SelectQuery {
    pub fn new(
        root_object_type: ObjectTypeRef,
        shape: ResolvedShape,
        filter: Option<Expr>,
        order_by: Vec<OrderExpr>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Self {
        Self {
            root_object_type,
            shape,
            filter,
            order_by,
            limit,
            offset,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn limit(&self) -> Option<u64> {
        self.limit
    }

    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    pub fn order_by(&self) -> &[OrderExpr] {
        &self.order_by
    }

    pub fn shape(&self) -> &ResolvedShape {
        &self.shape
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedShape {
    source_object_type: ObjectTypeRef,
    fields: Vec<ResolvedShapeField>,
}

impl ResolvedShape {
    pub fn new(source_object_type: ObjectTypeRef, fields: Vec<ResolvedShapeField>) -> Self {
        Self {
            source_object_type,
            fields,
        }
    }

    pub fn source_object_type(&self) -> &ObjectTypeRef {
        &self.source_object_type
    }

    pub fn fields(&self) -> &[ResolvedShapeField] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedShapeField {
    output_name: String,
    field: FieldRef,
    cardinality: Cardinality,
    child_shape: Option<ResolvedShape>,
}

impl ResolvedShapeField {
    pub fn new(
        output_name: impl Into<String>,
        field: FieldRef,
        cardinality: Cardinality,
        child_shape: Option<ResolvedShape>,
    ) -> Self {
        Self {
            output_name: output_name.into(),
            field,
            cardinality,
            child_shape,
        }
    }
    pub fn output_name(&self) -> &str {
        &self.output_name
    }
    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
    pub fn child_shape(&self) -> Option<&ResolvedShape> {
        self.child_shape.as_ref()
    }
    pub fn field(&self) -> &FieldRef {
        &self.field
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Compare(CompareExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareExpr {
    left: ValueExpr,
    op: CompareOp,
    right: ValueExpr,
}

impl CompareExpr {
    pub fn new(left: ValueExpr, op: CompareOp, right: ValueExpr) -> Self {
        Self { left, op, right }
    }

    pub fn left(&self) -> &ValueExpr {
        &self.left
    }

    pub fn op(&self) -> CompareOp {
        self.op
    }

    pub fn right(&self) -> &ValueExpr {
        &self.right
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderExpr {
    value: ValueExpr,
    direction: OrderDirection,
}

impl OrderExpr {
    pub fn new(value: ValueExpr, direction: OrderDirection) -> Self {
        Self { value, direction }
    }

    pub fn value(&self) -> &ValueExpr {
        &self.value
    }

    pub fn direction(&self) -> OrderDirection {
        self.direction
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    Field(FieldRef),
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
}

#[cfg(test)]
mod tests;
