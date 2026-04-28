mod fixtures;

use crate::{ResolveError, resolve_select};
use fixtures::{post_only_catalog, post_with_author_catalog, post_with_title_catalog};
use query_ast::{Path, PathStep, SelectQuery, Shape, ShapeItem};

#[test]
fn resolves_select_root_object_type() {
    let query = SelectQuery::new("Post", Shape::new(vec![]), None, vec![], None, None);
    let catalog = post_only_catalog();
    assert_eq!(query.root_type_name(), "Post");

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    assert_eq!(resolved.root_object_type().name(), "Post");
}

#[test]
fn rejects_unknown_root_object_type() {
    let catalog = post_only_catalog();
    let query = SelectQuery::new("Book", Shape::new(vec![]), None, vec![], None, None);

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnknownObjectType {
            name: "Book".to_string()
        })
    );
}

#[test]
fn resolves_scalar_shape_field() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[0].cardinality(), schema::Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn rejects_unknown_shape_field() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("missing")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        })
    );
}

#[test]
fn resolves_implicit_id_shape_field() {
    let catalog = post_with_title_catalog();

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

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "id");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "id");
    assert_eq!(fields[0].cardinality(), schema::Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn rejects_nested_shape_on_scalar_field() {
    let catalog = post_with_title_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            Some(child_shape),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::NestedShapeOnScalarField {
            object_type: "Post".to_string(),
            field: "title".to_string(),
        })
    );
}

#[test]
fn resolves_link_shape_with_child_shape() {
    let catalog = post_with_author_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author")]),
            Some(child_shape),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "author");
    assert_eq!(fields[0].cardinality(), schema::Cardinality::Required);

    let child_shape = fields[0]
        .child_shape()
        .expect("link field should resolve child shape");

    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(child_shape.fields().len(), 1);
    assert_eq!(
        child_shape.fields()[0].field().owner_object_type().name(),
        "User"
    );
    assert_eq!(child_shape.fields()[0].field().name(), "name");
    assert_eq!(
        child_shape.fields()[0].cardinality(),
        schema::Cardinality::Required
    );
    assert!(child_shape.fields()[0].child_shape().is_none());
}

#[test]
fn rejects_link_shape_without_child_shape() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::MissingShapeOnLinkField {
            object_type: "Post".to_string(),
            field: "author".to_string(),
        })
    );
}

#[test]
fn preserves_shape_field_order() {
    let catalog = post_with_author_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![
            ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
            ShapeItem::new(Path::new(vec![PathStep::new("author")]), Some(child_shape)),
        ]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[1].field().name(), "author");
}
