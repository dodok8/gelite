mod fixtures;

use crate::{
    CompareExpr, CompareOp, Expr, Literal, OrderDirection, OrderExpr, ResolvedShape,
    ResolvedShapeField, SelectQuery, ValueExpr,
};
use fixtures::{
    empty_post_shape, post_author_field, post_subtitle_field, post_title_field, post_type,
    user_name_shape,
};
use schema::{Cardinality, ObjectTypeId};

#[test]
fn resolved_select_query_can_store_root_object_type() {
    let root_object_type = post_type();
    let shape = ResolvedShape::new(root_object_type.clone(), vec![]);

    let query = SelectQuery::new(root_object_type, shape, None, vec![], None, None);

    assert_eq!(query.root_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(query.root_object_type().name(), "Post");
}

#[test]
fn resolve_shape_can_store_source_object_type() {
    let shape = empty_post_shape();

    assert_eq!(shape.source_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(shape.source_object_type().name(), "Post");
}

#[test]
fn resolved_shape_can_contain_scalar_field() {
    let shape_field =
        ResolvedShapeField::new("title", post_title_field(), Cardinality::Required, None);

    let shape = ResolvedShape::new(post_type(), vec![shape_field]);
    let fields = shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[0].cardinality(), Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn resolved_shape_can_contain_link_field_with_child_shape() {
    let author_shape_field = ResolvedShapeField::new(
        "author",
        post_author_field(),
        Cardinality::Required,
        Some(user_name_shape()),
    );

    let post_shape = ResolvedShape::new(post_type(), vec![author_shape_field]);
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
    let author_shape_field = ResolvedShapeField::new(
        "author",
        post_author_field(),
        Cardinality::Many,
        Some(user_name_shape()),
    );
    let title_shape_field =
        ResolvedShapeField::new("title", post_title_field(), Cardinality::Required, None);

    let shape = ResolvedShape::new(post_type(), vec![title_shape_field, author_shape_field]);
    let fields = shape.fields();

    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[1].output_name(), "author");
}

#[test]
fn resolved_shape_field_can_have_output_alias() {
    let shape_field = ResolvedShapeField::new(
        "writer",
        post_author_field(),
        Cardinality::Many,
        Some(user_name_shape()),
    );

    assert_eq!(shape_field.output_name(), "writer");
    assert_eq!(shape_field.field().name(), "author");
}

#[test]
fn resolved_shape_can_contain_optional_scalar_field() {
    let shape_field = ResolvedShapeField::new(
        "subtitle",
        post_subtitle_field(),
        Cardinality::Optional,
        None,
    );

    let shape = ResolvedShape::new(post_type(), vec![shape_field]);
    let fields = shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "subtitle");
    assert_eq!(fields[0].field().name(), "subtitle");
    assert_eq!(fields[0].cardinality(), Cardinality::Optional);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn resolved_shape_can_contain_multi_link_field() {
    let shape_field = ResolvedShapeField::new(
        "author",
        post_author_field(),
        Cardinality::Many,
        Some(user_name_shape()),
    );

    let shape = ResolvedShape::new(post_type(), vec![shape_field]);
    let fields = shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].field().name(), "author");
    assert_eq!(fields[0].cardinality(), Cardinality::Many);

    let child_shape = fields[0]
        .child_shape()
        .expect("multi link field has child shape");

    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(child_shape.fields()[0].output_name(), "name");
}

#[test]
fn resolved_shape_can_contain_optional_link_field() {
    let shape_field = ResolvedShapeField::new(
        "author",
        post_author_field(),
        Cardinality::Optional,
        Some(user_name_shape()),
    );

    let shape = ResolvedShape::new(post_type(), vec![shape_field]);
    let fields = shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].field().name(), "author");
    assert_eq!(fields[0].cardinality(), Cardinality::Optional);

    let child_shape = fields[0]
        .child_shape()
        .expect("optional link field has child shape");

    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(child_shape.fields()[0].output_name(), "name");
}

#[test]
fn resolved_select_query_can_store_limit_and_offset() {
    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        None,
        vec![],
        Some(10),
        Some(20),
    );

    assert_eq!(query.limit(), Some(10));
    assert_eq!(query.offset(), Some(20));
}

#[test]
fn resolved_select_query_can_store_order_by_field() {
    let order = OrderExpr::new(ValueExpr::Field(post_title_field()), OrderDirection::Desc);

    let query = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order],
        None,
        None,
    );

    assert_eq!(query.order_by().len(), 1);
    assert_eq!(query.order_by()[0].direction(), OrderDirection::Desc);

    match query.order_by()[0].value() {
        ValueExpr::Field(field) => {
            assert_eq!(field.owner_object_type().name(), "Post");
            assert_eq!(field.name(), "title");
        }
        ValueExpr::Literal(_) => panic!("order by should reference a resolved field"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_compare_expr() {
    let filter = Expr::Compare(CompareExpr::new(
        ValueExpr::Field(post_title_field()),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Hello".to_string())),
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Compare(compare) = query.filter().expect("select query has filter");
    assert_eq!(compare.op(), CompareOp::Eq);

    match compare.left() {
        ValueExpr::Field(field) => {
            assert_eq!(field.owner_object_type().name(), "Post");
            assert_eq!(field.name(), "title");
        }
        ValueExpr::Literal(_) => panic!("filter left side should reference a resolved field"),
    }

    match compare.right() {
        ValueExpr::Literal(Literal::String(value)) => assert_eq!(value, "Hello"),
        ValueExpr::Field(_) => panic!("filter right side should store a literal"),
    }
}
