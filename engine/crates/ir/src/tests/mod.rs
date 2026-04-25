use crate::{ResolvedShape, ResolvedShapeField, SelectQuery};
use schema::{Cardinality, FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

#[test]
fn resolved_select_query_can_store_root_object_type() {
    let root_object_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let shape = ResolvedShape::new(root_object_type.clone(), vec![]);

    let query = SelectQuery::new(root_object_type, shape, None, vec![], None, None);

    assert_eq!(query.root_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(query.root_object_type().name(), "Post");
}

#[test]
fn resolve_shape_can_store_source_object_type() {
    let source_object_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");

    let shape = ResolvedShape::new(source_object_type, vec![]);

    assert_eq!(shape.source_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(shape.source_object_type().name(), "Post");
}

#[test]
fn resolved_shape_can_contain_scalar_field() {
    let post_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let title_field = FieldRef::new(FieldId::new(1), post_type.clone(), "title");

    let shape_field = ResolvedShapeField::new("title", title_field, Cardinality::Required, None);

    let shape = ResolvedShape::new(post_type, vec![shape_field]);

    let fields = shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[0].cardinality(), Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn resolved_shape_can_contain_link_field_with_child_shape() {
    let post_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let user_type = ObjectTypeRef::new(ObjectTypeId::new(2), "User");
    let author_field = FieldRef::new(FieldId::new(1), post_type.clone(), "author");
    let user_name_field = FieldRef::new(FieldId::new(2), user_type.clone(), "name");

    let user_name_shape_field =
        ResolvedShapeField::new("name", user_name_field, Cardinality::Required, None);
    let user_shape = ResolvedShape::new(user_type, vec![user_name_shape_field]);
    let author_shape_field = ResolvedShapeField::new(
        "author",
        author_field,
        Cardinality::Required,
        Some(user_shape),
    );

    let post_shape = ResolvedShape::new(post_type, vec![author_shape_field]);
    let fields = post_shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].field().name(), "author");
    assert_eq!(fields[0].cardinality(), Cardinality::Required);

    let child_shape = fields[0].child_shape().expect("link field has child shape");
    assert_eq!(child_shape.source_object_type().id(), ObjectTypeId::new(2));
    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(child_shape.fields().len(), 1);
    assert_eq!(child_shape.fields()[0].output_name(), "name");
    assert_eq!(child_shape.fields()[0].field().name(), "name");
    assert_eq!(child_shape.fields()[0].cardinality(), Cardinality::Required);
    assert!(child_shape.fields()[0].child_shape().is_none());
}

#[test]
fn resolved_shape_preserves_field_order() {
    let user_type = ObjectTypeRef::new(ObjectTypeId::new(2), "User");
    let user_name_field = FieldRef::new(FieldId::new(2), user_type.clone(), "name");
    let user_name_shape_field =
        ResolvedShapeField::new("name", user_name_field, Cardinality::Required, None);
    let user_shape = ResolvedShape::new(user_type, vec![user_name_shape_field]);

    let post_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let title_field = FieldRef::new(FieldId::new(2), post_type.clone(), "title");
    let author_field = FieldRef::new(FieldId::new(1), post_type.clone(), "author");
    let author_shape_field =
        ResolvedShapeField::new("author", author_field, Cardinality::Many, Some(user_shape));
    let title_shape_field =
        ResolvedShapeField::new("title", title_field, Cardinality::Required, None);

    let shape = ResolvedShape::new(post_type, vec![title_shape_field, author_shape_field]);
    let fields = shape.fields();

    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[1].output_name(), "author");
}

#[test]
fn resolved_shape_field_can_have_output_alias() {
    let user_type = ObjectTypeRef::new(ObjectTypeId::new(2), "User");
    let user_name_field = FieldRef::new(FieldId::new(2), user_type.clone(), "name");
    let user_name_shape_field =
        ResolvedShapeField::new("name", user_name_field, Cardinality::Required, None);
    let user_shape = ResolvedShape::new(user_type, vec![user_name_shape_field]);

    let post_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let author_field = FieldRef::new(FieldId::new(1), post_type.clone(), "author");
    let shape_field =
        ResolvedShapeField::new("writer", author_field, Cardinality::Many, Some(user_shape));

    assert_eq!(shape_field.output_name(), "writer");
    assert_eq!(shape_field.field().name(), "author");
}
