use schema::{FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

pub fn post_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(1), "Post")
}

pub fn post_title_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), post_type(), "title")
}

pub fn post_title_shape_field() -> ir::ResolvedShapeField {
    ir::ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema::Cardinality::Required,
        None,
    )
}

pub fn post_query_with_shape(fields: Vec<ir::ResolvedShapeField>) -> ir::SelectQuery {
    ir::SelectQuery::new(
        post_type(),
        ir::ResolvedShape::new(post_type(), fields),
        None,
        vec![],
        None,
        None,
    )
}
