use crate::{
    ArithmeticExpr, ArithmeticOp, Assignment, CompareExpr, CompareOp, Expr, FunctionCallExpr,
    InExpr, InOp, InsertQuery, Literal, OrderDirection, OrderExpr, Path, PathStep, SelectQuery,
    Shape, ShapeItem, UpdateQuery,
};
use alloc::string::ToString;
use alloc::vec;

#[test]
fn assignment_can_store_field_name_and_literal_value() {
    let assignment = Assignment::new("name", Literal::String("Sheri".to_string()));

    assert_eq!(assignment.field_name(), "name");
    assert_eq!(assignment.value(), &Literal::String("Sheri".to_string()));
}

#[test]
fn insert_query_can_store_root_type_and_assignments_in_definition_order() {
    let query = InsertQuery::new(
        "User",
        vec![
            Assignment::new("name", Literal::String("Sheri".to_string())),
            Assignment::new("email", Literal::String("sheri@example.com".to_string())),
        ],
    );

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 2);
    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &Literal::String("Sheri".to_string())
    );
    assert_eq!(query.assignments()[1].field_name(), "email");
    assert_eq!(
        query.assignments()[1].value(),
        &Literal::String("sheri@example.com".to_string())
    );
}

#[test]
fn update_query_can_store_unresolved_filter_and_assignments_in_definition_order() {
    let filter = Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("id")])),
        CompareOp::Eq,
        Expr::Literal(Literal::String("post-1".to_string())),
    ));
    let query = UpdateQuery::new(
        "Post",
        Some(filter.clone()),
        vec![
            Assignment::new("title", Literal::String("Closed Case".to_string())),
            Assignment::new("author", Literal::String("user-2".to_string())),
        ],
    );

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.filter(), Some(&filter));
    assert_eq!(query.assignments().len(), 2);
    assert_eq!(query.assignments()[0].field_name(), "title");
    assert_eq!(query.assignments()[1].field_name(), "author");
}

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
fn shape_can_contain_computed_projection() {
    let expr = Expr::Arithmetic(ArithmeticExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("likes")])),
        ArithmeticOp::Add,
        Expr::Literal(Literal::Int64(1)),
    ));
    let shape = Shape::new(vec![ShapeItem::computed("score", expr)]);

    let items = shape.items();

    assert_eq!(items.len(), 1);
    let computed = items[0]
        .as_computed()
        .expect("shape item should be a computed projection");
    assert_eq!(computed.output_name(), "score");

    let Expr::Arithmetic(arithmetic) = computed.expr() else {
        panic!("computed projection should store an arithmetic expression");
    };
    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
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

#[test]
fn literal_expr_can_store_string_values() {
    let hello = Literal::String("hello".to_string());

    match hello {
        Literal::String(val) => {
            assert_eq!(val, "hello".to_string());
        }
        _ => panic!("Expected String"),
    }
}

#[test]
fn compare_expr_can_reference_path_and_literal() {
    let left_path = Path::new(vec![PathStep::new("author"), PathStep::new("id")]);

    let right_literal = Literal::String("00000000-0000-0000-0000-000000000001".to_string());

    let expr = Expr::Compare(CompareExpr::new(
        Expr::Path(left_path),
        CompareOp::Eq,
        Expr::Literal(right_literal),
    ));

    match expr {
        Expr::Compare(compare) => {
            let Expr::Path(left) = compare.left() else {
                panic!("expected compare expression left side to be a path");
            };
            assert_eq!(left.steps().len(), 2);
            assert_eq!(left.steps()[0].field_name(), "author");
            assert_eq!(left.steps()[1].field_name(), "id");
            assert_eq!(compare.op(), CompareOp::Eq);

            match compare.right() {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "00000000-0000-0000-0000-000000000001");
                }
                _ => panic!("expected compare expression right side to be a string literal"),
            }
        }
        _ => panic!("expected expression to be a compare expression"),
    }
}

#[test]
fn compare_expr_can_store_non_equality_operator() {
    let expr = CompareExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("view_count")])),
        CompareOp::Ge,
        Expr::Literal(Literal::Int64(10)),
    );

    assert_eq!(expr.op(), CompareOp::Ge);
}

#[test]
fn function_call_expr_can_store_name_and_arguments() {
    let expr = Expr::FunctionCall(FunctionCallExpr::new(
        "f64",
        vec![Expr::Path(Path::new(vec![PathStep::new("view_count")]))],
    ));

    let Expr::FunctionCall(function) = expr else {
        panic!("expected expression to be a function call");
    };

    assert_eq!(function.name(), "f64");
    assert_eq!(function.args().len(), 1);
    let Expr::Path(path) = &function.args()[0] else {
        panic!("expected function argument to be a path");
    };
    assert_eq!(path.steps()[0].field_name(), "view_count");
}

#[test]
fn order_expr_can_reference_a_path() {
    let path = Expr::Path(Path::new(vec![PathStep::new("title")]));
    let order = OrderExpr::new(path, crate::OrderDirection::Asc);

    match order.expr() {
        Expr::Path(path) => {
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field_name(), "title");
        }
        other => panic!("order expression should reference a path, got {other:?}"),
    }
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn select_query_can_store_filter_order_and_limit() {
    let shape = Shape::new(vec![
        ShapeItem::new(Path::new(vec![PathStep::new("id")]), None),
        ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
    ]);

    let filter = Expr::Compare(CompareExpr::new(
        Expr::Path(Path::new(vec![
            PathStep::new("author"),
            PathStep::new("id"),
        ])),
        CompareOp::Eq,
        Expr::Literal(Literal::String(
            "00000000-0000-0000-0000-000000000001".to_string(),
        )),
    ));

    let order = OrderExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("title")])),
        OrderDirection::Asc,
    );

    let query = SelectQuery::new("Post", shape, Some(filter), vec![order], Some(10), Some(0));

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.shape().items().len(), 2);

    let filter = query
        .filter()
        .expect("select query should store its filter expression");

    match filter {
        Expr::Compare(compare) => {
            let Expr::Path(left) = compare.left() else {
                panic!("expected select query filter left side to be a path");
            };
            assert_eq!(left.steps().len(), 2);
            assert_eq!(left.steps()[0].field_name(), "author");
            assert_eq!(left.steps()[1].field_name(), "id");
            assert_eq!(compare.op(), CompareOp::Eq);

            match compare.right() {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "00000000-0000-0000-0000-000000000001");
                }
                _ => panic!("expected select query filter to store a string literal"),
            }
        }
        _ => panic!("expected select query filter to be a compare expression"),
    }

    assert_eq!(query.order_by().len(), 1);
    match query.order_by()[0].expr() {
        Expr::Path(path) => assert_eq!(path.steps()[0].field_name(), "title"),
        other => panic!("order expression should reference a path, got {other:?}"),
    }
    assert_eq!(query.order_by()[0].direction(), OrderDirection::Asc);
    assert_eq!(query.limit(), Some(10));
    assert_eq!(query.offset(), Some(0));
}

#[test]
fn in_expr_can_reference_path_and_literal_list() {
    let left_path = Path::new(vec![PathStep::new("status")]);

    let expr = Expr::In(InExpr::new(
        Expr::Path(left_path),
        InOp::In,
        vec![
            Expr::Literal(Literal::String("draft".to_string())),
            Expr::Literal(Literal::String("published".to_string())),
        ],
    ));

    match expr {
        Expr::In(in_expr) => {
            let Expr::Path(left) = in_expr.left() else {
                panic!("expected in expression left side to be a path");
            };
            assert_eq!(left.steps().len(), 1);
            assert_eq!(left.steps()[0].field_name(), "status");
            assert_eq!(in_expr.op(), InOp::In);

            assert_eq!(in_expr.right().len(), 2);
            match &in_expr.right()[0] {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "draft");
                }
                _ => panic!("expected first in expression item to be a string literal"),
            }
            match &in_expr.right()[1] {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "published");
                }
                _ => panic!("expected second in expression item to be a string literal"),
            }
        }
        _ => panic!("expected expression to be an in expression"),
    }
}

#[test]
fn not_in_expr_can_store_membership_operator() {
    let left_path = Path::new(vec![PathStep::new("status")]);

    let expr = Expr::In(InExpr::new(
        Expr::Path(left_path),
        InOp::NotIn,
        vec![
            Expr::Literal(Literal::String("draft".to_string())),
            Expr::Literal(Literal::String("published".to_string())),
        ],
    ));

    match expr {
        Expr::In(in_expr) => {
            let Expr::Path(left) = in_expr.left() else {
                panic!("expected in expression left side to be a path");
            };
            assert_eq!(left.steps().len(), 1);
            assert_eq!(left.steps()[0].field_name(), "status");
            assert_eq!(in_expr.op(), InOp::NotIn);

            assert_eq!(in_expr.right().len(), 2);
            match &in_expr.right()[0] {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "draft");
                }
                _ => panic!("expected first in expression item to be a string literal"),
            }
            match &in_expr.right()[1] {
                Expr::Literal(Literal::String(value)) => {
                    assert_eq!(value, "published");
                }
                _ => panic!("expected second in expression item to be a string literal"),
            }
        }
        _ => panic!("expected expression to be an in expression"),
    };
}
