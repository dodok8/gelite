mod fixtures;

use crate::{ResolveError, resolve_select};
use fixtures::{post_only_catalog, post_with_author_catalog, post_with_title_catalog};
use query_ast::{
    CompareExpr, CompareOp, Expr, Literal, Path, PathStep, SelectQuery, Shape, ShapeItem,
};

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

#[test]
fn resolves_filter_compare_path_to_field_and_literal() {
    let catalog = post_with_title_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Path::new(vec![PathStep::new("title")]),
        CompareOp::Eq,
        Literal::String("Hello".to_string()),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve");

    match compare.left() {
        ir::ValueExpr::Field(field) => {
            assert_eq!(field.owner_object_type().name(), "Post");
            assert_eq!(field.name(), "title");
        }
        ir::ValueExpr::Literal(_) => panic!("filter left side should resolve to a field"),
    }

    assert_eq!(compare.op(), ir::CompareOp::Eq);

    match compare.right() {
        ir::ValueExpr::Literal(ir::Literal::String(value)) => {
            assert_eq!(value, "Hello");
        }
        ir::ValueExpr::Field(_) => panic!("filter right side should resolve to a literal"),
    }
}

#[test]
fn rejects_filter_path_with_unknown_field() {
    let catalog = post_with_title_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Path::new(vec![PathStep::new("missing")]),
        CompareOp::Eq,
        Literal::String("Hello".to_string()),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
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
fn rejects_filter_path_with_link_field() {
    let catalog = post_with_author_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Path::new(vec![PathStep::new("author")]),
        CompareOp::Eq,
        Literal::String("Ada".to_string()),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
}

#[test]
fn resolves_order_path_to_field() {
    let catalog = post_with_title_catalog();

    let order = query_ast::OrderExpr::new(
        Path::new(vec![PathStep::new("title")]),
        query_ast::OrderDirection::Desc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(resolved.order_by()[0].direction(), ir::OrderDirection::Desc);

    match resolved.order_by()[0].value() {
        ir::ValueExpr::Field(field) => {
            assert_eq!(field.owner_object_type().name(), "Post");
            assert_eq!(field.name(), "title");
        }
        ir::ValueExpr::Literal(_) => panic!("order by should resolve to a field"),
    }
}

#[test]
fn rejects_order_path_with_link_field() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        Path::new(vec![PathStep::new("author")]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
}

#[test]
fn passes_limit_and_offset_through() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![],
        Some(10),
        Some(20),
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.limit(), Some(10));
    assert_eq!(resolved.offset(), Some(20));
}
