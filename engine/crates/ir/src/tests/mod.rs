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
