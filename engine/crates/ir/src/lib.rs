use schema::ObjectTypeRef;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectQuery {
    root_object_type: ObjectTypeRef,
    shape: ResolvedShape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<u64>,
    offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedShape {
    source_object_type: ObjectTypeRef,
    fields: Vec<ResolvedShapeField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedShapeField;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderExpr;

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
}

impl ResolvedShape {
    pub fn new(source_object_type: ObjectTypeRef, fields: Vec<ResolvedShapeField>) -> Self {
        Self {
            source_object_type,
            fields,
        }
    }
}

#[cfg(test)]
mod tests;
