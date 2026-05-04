use schema::{FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

pub fn post_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(1), "Post")
}

pub fn post_title_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), post_type(), "title")
}

pub fn post_author_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), post_type(), "author")
}

pub fn post_id_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), post_type(), "id")
}

pub fn user_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(2), "User")
}

pub fn user_name_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), user_type(), "name")
}

pub fn empty_post_query() -> ir::SelectQuery {
    ir::SelectQuery::new(
        post_type(),
        ir::ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        None,
    )
}

pub fn post_title_shape_field() -> ir::ResolvedShapeField {
    ir::ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema::Cardinality::Required,
        None,
    )
}

pub fn post_id_shape_field() -> ir::ResolvedShapeField {
    ir::ResolvedShapeField::new("id", post_id_field(), schema::Cardinality::Required, None)
}

pub fn user_name_shape_field() -> ir::ResolvedShapeField {
    ir::ResolvedShapeField::new(
        "name",
        user_name_field(),
        schema::Cardinality::Required,
        None,
    )
}

pub fn post_author_shape_field() -> ir::ResolvedShapeField {
    let author_shape = ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]);

    ir::ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema::Cardinality::Required,
        Some(author_shape),
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
