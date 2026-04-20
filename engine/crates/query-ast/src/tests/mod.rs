use crate::{Path, PathStep, SelectQuery, Shape, ShapeItem};

#[test]
fn select_query_can_store_root_type_name() {
    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    assert_eq!(query.root_type_name(), "Post");
}

#[test]
fn shape_can_contain_scalar_field_selection() {
    let shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("title")]),
        None,
    )]);

    let items = shape.items();

    assert_eq!(items.len(), 1);
    assert!(items[0].child_shape().is_none());
    assert_eq!(items[0].path().steps().len(), 1);
    assert_eq!(items[0].path().steps()[0].field_name(), "title");
}

#[test]
fn shape_can_contain_nested_link_selection() {
    let nested_shape = Shape::new(vec![
        ShapeItem::new(Path::new(vec![PathStep::new("id")]), None),
        ShapeItem::new(Path::new(vec![PathStep::new("name")]), None),
    ]);

    let shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("author")]),
        Some(nested_shape),
    )]);

    let items = shape.items();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path().steps()[0].field_name(), "author");
    assert!(items[0].child_shape().is_some());
}

#[test]
fn shape_preserves_item_definition_order() {
    let shape = Shape::new(vec![
        ShapeItem::new(Path::new(vec![PathStep::new("id")]), None),
        ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
        ShapeItem::new(Path::new(vec![PathStep::new("author")]), None),
    ]);

    let items = shape.items();

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].path().steps()[0].field_name(), "id");
    assert_eq!(items[1].path().steps()[0].field_name(), "title");
    assert_eq!(items[2].path().steps()[0].field_name(), "author");
}

#[test]
fn path_can_represent_single_step_field_access() {
    let path = Path::new(vec![PathStep::new("title")]);
    let steps = path.steps();

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].field_name(), "title");
}

#[test]
fn path_can_represent_multi_step_link_traversal() {
    let path = Path::new(vec![PathStep::new("author"), PathStep::new("id")]);
    let steps = path.steps();

    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].field_name(), "author");
    assert_eq!(steps[1].field_name(), "id");
}
