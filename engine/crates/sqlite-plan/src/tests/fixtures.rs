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
