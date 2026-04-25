use schema::{ObjectTypeId, ObjectTypeRef};

use crate::{ResolvedShape, SelectQuery};

#[test]
fn resolved_select_query_can_store_root_object_type() {
    let root_object_type = ObjectTypeRef::new(ObjectTypeId::new(1), "Post");
    let shape = ResolvedShape::new(root_object_type.clone(), vec![]);

    let query = SelectQuery::new(root_object_type, shape, None, vec![], None, None);

    assert_eq!(query.root_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(query.root_object_type().name(), "Post");
}
