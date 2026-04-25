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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderExpr;

#[cfg(test)]
mod tests;
