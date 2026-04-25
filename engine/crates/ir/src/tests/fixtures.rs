use crate::{ResolvedShape, ResolvedShapeField};
use schema::{Cardinality, FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

pub fn post_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(1), "Post")
}

pub fn user_type() -> ObjectTypeRef {
    ObjectTypeRef::new(ObjectTypeId::new(2), "User")
}

pub fn post_title_field() -> FieldRef {
    FieldRef::new(FieldId::new(1), post_type(), "title")
}

pub fn post_author_field() -> FieldRef {
    FieldRef::new(FieldId::new(2), post_type(), "author")
}

pub fn post_subtitle_field() -> FieldRef {
    FieldRef::new(FieldId::new(3), post_type(), "subtitle")
}

pub fn user_name_field() -> FieldRef {
    FieldRef::new(FieldId::new(4), user_type(), "name")
}

pub fn empty_post_shape() -> ResolvedShape {
    ResolvedShape::new(post_type(), vec![])
}

pub fn user_name_shape() -> ResolvedShape {
    let shape_field =
        ResolvedShapeField::new("name", user_name_field(), Cardinality::Required, None);

    ResolvedShape::new(user_type(), vec![shape_field])
}
