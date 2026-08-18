use super::fixtures::{
    arithmetic_expr, filter_compare_int, filter_eq_bool, filter_eq_int, filter_eq_null,
    filter_eq_string, filter_in_bools, filter_in_empty, filter_in_floats, filter_in_ints,
    filter_in_null, filter_in_path_item, filter_in_select, filter_in_strings, filter_lt_null,
    filter_ne_null, filter_not_in_strings, filter_null_eq, filter_null_ne, function_call_expr,
    literal_float_expr, literal_int_expr, literal_null_expr, literal_string_expr, path_expr,
    post_only_catalog, post_with_author_catalog, post_with_optional_subtitle_catalog,
    post_with_scalar_fields_catalog, post_with_title_catalog, user_with_posts_catalog,
};
use crate::{ResolveError, resolve_select, tests::fixtures::literal_bool_expr};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use query_ast::{
    ArithmeticExpr, CompareExpr,
    Expr::{self, Compare},
    InExpr, InRhs, Path, PathStep, SelectQuery, Shape, ShapeItem, UnaryArithmeticExpr,
    UnaryArithmeticOp,
};
use query_ir::ValueExpr;
use schema_model::{Cardinality, ScalarType};

fn in_list_expr(left: Expr, op: query_ast::InOp, right: Vec<Expr>) -> InExpr {
    InExpr::new(left, op, InRhs::List(right))
}

fn select_post(filter: Expr) -> SelectQuery {
    SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    )
}

fn assert_resolved_in_select(in_expr: &query_ir::InExpr) -> &query_ir::SelectQuery {
    let query_ir::InRhs::Select(select) = in_expr.right() else {
        panic!("membership right side should resolve to a select");
    };

    select
}

fn assert_resolved_in_list(in_expr: &query_ir::InExpr) -> &[ValueExpr] {
    let query_ir::InRhs::List(values) = in_expr.right() else {
        panic!("membership right side should resolve to a list");
    };

    values
}

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
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn resolves_computed_projection_shape_item() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Add,
                literal_int_expr(1),
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let items = resolved.shape().items();

    assert_eq!(items.len(), 1);
    let query_ir::ResolvedShapeItem::Computed(computed) = &items[0] else {
        panic!("shape item should resolve to a computed projection");
    };
    assert_eq!(computed.output_name(), "score");
    assert_eq!(computed.scalar_type(), schema_model::ScalarType::Int64);
    assert_eq!(computed.cardinality(), schema_model::Cardinality::Required);

    let query_ir::ValueExpr::Arithmetic(arithmetic) = computed.value() else {
        panic!("computed projection should store an arithmetic value expression");
    };
    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
}

#[test]
fn resolves_computed_projection_concat_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "label",
            function_call_expr(
                "concat",
                vec![path_expr(&["title"]), literal_string_expr("!")],
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.output_name(), "label");
    assert_eq!(computed.scalar_type(), schema_model::ScalarType::Str);
    assert_eq!(computed.cardinality(), schema_model::Cardinality::Required);

    let query_ir::ValueExpr::StringFunction(function) = computed.value() else {
        panic!("computed projection should store a string function expression");
    };
    assert_eq!(function.kind(), query_ir::StringFunctionKind::Concat);
    assert_eq!(function.args().len(), 2);
}

#[test]
fn resolves_computed_projection_runtime_division_as_optional() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Div,
                path_expr(&["view_count"]),
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.scalar_type(), schema_model::ScalarType::Int64);
    assert_eq!(computed.cardinality(), schema_model::Cardinality::Optional);
}

#[test]
fn resolves_computed_projection_runtime_modulo_as_optional() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Mod,
                path_expr(&["view_count"]),
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.scalar_type(), schema_model::ScalarType::Int64);
    assert_eq!(computed.cardinality(), schema_model::Cardinality::Optional);
}

#[test]
fn resolves_computed_projection_nonzero_literal_division_as_required() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Div,
                literal_int_expr(2),
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.scalar_type(), schema_model::ScalarType::Int64);
    assert_eq!(computed.cardinality(), schema_model::Cardinality::Required);
}

#[test]
fn resolves_computed_projection_signed_nonzero_literal_division_as_required() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![
            ShapeItem::computed(
                "negative_ratio",
                arithmetic_expr(
                    path_expr(&["view_count"]),
                    query_ast::ArithmeticOp::Div,
                    Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
                        UnaryArithmeticOp::Minus,
                        literal_int_expr(2),
                    )),
                ),
            ),
            ShapeItem::computed(
                "positive_ratio",
                arithmetic_expr(
                    path_expr(&["view_count"]),
                    query_ast::ArithmeticOp::Div,
                    Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
                        UnaryArithmeticOp::Plus,
                        literal_int_expr(2),
                    )),
                ),
            ),
        ]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    for item in resolved.shape().items() {
        let query_ir::ResolvedShapeItem::Computed(computed) = item else {
            panic!("shape item should resolve to a computed projection");
        };

        assert_eq!(computed.scalar_type(), schema_model::ScalarType::Int64);
        assert_eq!(computed.cardinality(), schema_model::Cardinality::Required);
    }
}

#[test]
fn resolves_computed_projection_unary_arithmetic_path() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "neg_views",
            Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
                UnaryArithmeticOp::Minus,
                path_expr(&["view_count"]),
            )),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    assert_eq!(resolved.shape().items().len(), 1);
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.output_name(), "neg_views");
    assert_eq!(computed.scalar_type(), ScalarType::Int64);
    assert_eq!(computed.cardinality(), Cardinality::Required);
    match computed.value() {
        ValueExpr::UnaryArithmetic(unary) => {
            assert_eq!(unary.op(), query_ir::UnaryArithmeticOp::Minus);
            assert_eq!(unary.scalar_type(), ScalarType::Int64);
            let query_ir::ValueExpr::Path(path) = unary.operand() else {
                panic!("unary operand should resolve to a path");
            };

            let [step] = path.steps() else {
                panic!("unary operand should resolve to one path step");
            };

            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(step.field().name(), "view_count");
            assert_eq!(step.cardinality(), schema_model::Cardinality::Required);
            assert_eq!(
                path.result_cardinality(),
                schema_model::Cardinality::Required
            );
        }
        other => panic!("computed value should be unary arithmetic, got {other:?}"),
    }
}

#[test]
fn resolves_computed_projection_numeric_cast_path() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "view_count_float",
            function_call_expr("f64", vec![path_expr(&["view_count"])]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.output_name(), "view_count_float");
    assert_eq!(computed.scalar_type(), ScalarType::Float64);
    assert_eq!(computed.cardinality(), Cardinality::Required);

    let ValueExpr::Cast(cast) = computed.value() else {
        panic!("computed projection should resolve to a cast value expression");
    };
    assert_eq!(cast.target_type(), ScalarType::Float64);
}

#[test]
fn resolves_filter_compare_unary_arithmetic_path() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            path_expr(&["view_count"]),
        )),
        query_ast::CompareOp::Lt,
        literal_int_expr(0),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    assert_eq!(compare.op(), query_ir::CompareOp::Lt);

    let ValueExpr::UnaryArithmetic(unary) = compare.left() else {
        panic!("filter left side should resolve to unary arithmetic");
    };

    assert_eq!(unary.op(), query_ir::UnaryArithmeticOp::Minus);
    assert_eq!(unary.scalar_type(), ScalarType::Int64);

    let ValueExpr::Path(path) = unary.operand() else {
        panic!("unary operand should resolve to a path");
    };

    let [step] = path.steps() else {
        panic!("unary operand should resolve to one path step");
    };

    assert_eq!(path.root_object_type().name(), "Post");
    assert_eq!(step.field().name(), "view_count");
    assert_eq!(step.cardinality(), Cardinality::Required);
    assert_eq!(path.result_cardinality(), Cardinality::Required);

    let ValueExpr::Literal(query_ir::Literal::Int64(value)) = compare.right() else {
        panic!("filter right side should resolve to an int64 literal");
    };

    assert_eq!(*value, 0);
}

#[test]
fn rejects_unary_arithmetic_string_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            literal_string_expr("hello"),
        )),
        query_ast::CompareOp::Eq,
        literal_int_expr(1),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "str".to_string(),
        })
    );
}

#[test]
fn resolves_order_unary_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();
    let order = query_ast::OrderExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            path_expr(&["view_count"]),
        )),
        query_ast::OrderDirection::Desc,
    );

    let query = SelectQuery::new("Post", Shape::new(vec![]), None, vec![order], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Desc
    );

    let ValueExpr::UnaryArithmetic(unary) = resolved.order_by()[0].value() else {
        panic!("order by should resolve to unary arithmetic");
    };
    assert_eq!(unary.op(), query_ir::UnaryArithmeticOp::Minus);
    assert_eq!(unary.scalar_type(), ScalarType::Int64);

    let ValueExpr::Path(path) = unary.operand() else {
        panic!("unary order operand should resolve to a path");
    };
    let [step] = path.steps() else {
        panic!("unary order operand should resolve to one path step");
    };

    assert_eq!(path.root_object_type().name(), "Post");
    assert_eq!(step.field().name(), "view_count");
    assert_eq!(step.cardinality(), Cardinality::Required);
    assert_eq!(path.result_cardinality(), Cardinality::Required);
}

#[test]
fn resolves_membership_unary_arithmetic_literal_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["view_count"]),
        query_ast::InOp::In,
        vec![
            Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
                UnaryArithmeticOp::Minus,
                literal_int_expr(1),
            )),
            Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
                UnaryArithmeticOp::Plus,
                literal_int_expr(2),
            )),
        ],
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let ValueExpr::Path(path) = in_expr.left() else {
        panic!("in expression left side should resolve to a path");
    };
    let [step] = path.steps() else {
        panic!("in expression left side should resolve to one path step");
    };
    assert_eq!(step.field().name(), "view_count");
    assert_eq!(in_expr.op(), query_ir::InOp::In);
    let right = assert_resolved_in_list(in_expr);
    assert_eq!(right.len(), 2);

    let ValueExpr::UnaryArithmetic(first) = &right[0] else {
        panic!("first membership item should resolve to unary arithmetic");
    };
    assert_eq!(first.op(), query_ir::UnaryArithmeticOp::Minus);
    assert_eq!(first.scalar_type(), ScalarType::Int64);
    assert_eq!(
        first.operand(),
        &ValueExpr::Literal(query_ir::Literal::Int64(1))
    );

    let ValueExpr::UnaryArithmetic(second) = &right[1] else {
        panic!("second membership item should resolve to unary arithmetic");
    };
    assert_eq!(second.op(), query_ir::UnaryArithmeticOp::Plus);
    assert_eq!(second.scalar_type(), ScalarType::Int64);
    assert_eq!(
        second.operand(),
        &ValueExpr::Literal(query_ir::Literal::Int64(2))
    );
}

#[test]
fn resolves_filter_compare_unary_arithmetic_literal_rhs() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        path_expr(&["view_count"]),
        query_ast::CompareOp::Eq,
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            literal_int_expr(1),
        )),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    let ValueExpr::Path(path) = compare.left() else {
        panic!("filter left side should resolve to a path");
    };
    let [step] = path.steps() else {
        panic!("filter left side should resolve to one path step");
    };
    assert_eq!(step.field().name(), "view_count");
    assert_eq!(compare.op(), query_ir::CompareOp::Eq);

    let ValueExpr::UnaryArithmetic(unary) = compare.right() else {
        panic!("filter right side should resolve to unary arithmetic");
    };
    assert_eq!(unary.op(), query_ir::UnaryArithmeticOp::Minus);
    assert_eq!(unary.scalar_type(), ScalarType::Int64);
    assert_eq!(
        unary.operand(),
        &ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn rejects_unary_arithmetic_bool_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            literal_bool_expr(true),
        )),
        query_ast::CompareOp::Eq,
        literal_int_expr(1),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "bool".to_string(),
        })
    );
}

#[test]
fn rejects_unary_arithmetic_null_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            literal_null_expr(),
        )),
        query_ast::CompareOp::Eq,
        literal_int_expr(1),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "str".to_string(),
        })
    );
}

#[test]
fn rejects_membership_unary_arithmetic_path_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["view_count"]),
        query_ast::InOp::In,
        vec![Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            path_expr(&["view_count"]),
        ))],
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership list item".to_string(),
        })
    );
}

#[test]
fn resolves_unary_arithmetic_parenthesized_binary_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Add,
                literal_int_expr(1),
            ),
        )),
        query_ast::CompareOp::Lt,
        literal_int_expr(0),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    let ValueExpr::UnaryArithmetic(unary) = compare.left() else {
        panic!("filter left side should resolve to unary arithmetic");
    };
    assert_eq!(unary.op(), query_ir::UnaryArithmeticOp::Minus);
    assert_eq!(unary.scalar_type(), ScalarType::Int64);

    let ValueExpr::Arithmetic(arithmetic) = unary.operand() else {
        panic!("unary operand should resolve to arithmetic");
    };
    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);

    let ValueExpr::Path(path) = arithmetic.left() else {
        panic!("arithmetic left side should resolve to a path");
    };
    let [step] = path.steps() else {
        panic!("arithmetic left side should resolve to one path step");
    };
    assert_eq!(step.field().name(), "view_count");
    assert_eq!(
        arithmetic.right(),
        &ValueExpr::Literal(query_ir::Literal::Int64(1))
    );

    let ValueExpr::Literal(query_ir::Literal::Int64(value)) = compare.right() else {
        panic!("filter right side should resolve to an int64 literal");
    };
    assert_eq!(*value, 0);
}

#[test]
fn resolves_computed_projection_plain_path_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            path_expr(&["view_count"]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.output_name(), "score");
    assert_eq!(computed.scalar_type(), ScalarType::Int64);
    assert_eq!(computed.cardinality(), Cardinality::Required);

    let ValueExpr::Path(path) = computed.value() else {
        panic!("computed projection should store a path value expression");
    };
    assert_eq!(path.root_object_type().name(), "Post");
    assert_eq!(path.steps().len(), 1);
    assert_eq!(path.steps()[0].field().name(), "view_count");
}

#[test]
fn resolves_computed_projection_path_through_single_link() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "author_name",
            path_expr(&["author", "name"]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.output_name(), "author_name");
    assert_eq!(computed.scalar_type(), ScalarType::Str);
    assert_eq!(computed.cardinality(), Cardinality::Required);

    let ValueExpr::Path(path) = computed.value() else {
        panic!("computed projection should store a path value expression");
    };
    assert_eq!(path.root_object_type().name(), "Post");
    assert_eq!(path.steps().len(), 2);
    assert_eq!(path.steps()[0].field().name(), "author");
    assert_eq!(path.steps()[1].field().name(), "name");
}

#[test]
fn resolves_nested_computed_projection_path_relative_to_child_shape() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author")]),
            Some(Shape::new(vec![ShapeItem::computed(
                "name_copy",
                path_expr(&["name"]),
            )])),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Field(author) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a field");
    };
    let child_shape = author
        .child_shape()
        .expect("author should have a child shape");
    let query_ir::ResolvedShapeItem::Computed(computed) = &child_shape.items()[0] else {
        panic!("child shape item should resolve to a computed projection");
    };
    let ValueExpr::Path(path) = computed.value() else {
        panic!("computed projection should store a path value expression");
    };

    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(computed.output_name(), "name_copy");
    assert_eq!(computed.scalar_type(), ScalarType::Str);
    assert_eq!(computed.cardinality(), Cardinality::Required);
    assert_eq!(path.root_object_type().name(), "User");
    assert_eq!(path.steps()[0].field().name(), "name");
}

#[test]
fn preserves_optional_computed_projection_path_cardinality() {
    let catalog = post_with_optional_subtitle_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "subtitle_copy",
            path_expr(&["subtitle"]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::ResolvedShapeItem::Computed(computed) = &resolved.shape().items()[0] else {
        panic!("shape item should resolve to a computed projection");
    };

    assert_eq!(computed.scalar_type(), ScalarType::Str);
    assert_eq!(computed.cardinality(), Cardinality::Optional);
}

#[test]
fn rejects_computed_projection_path_ending_at_link() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "author_value",
            path_expr(&["author"]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedPath)
    );
}

#[test]
fn rejects_computed_projection_path_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let query = SelectQuery::new(
        "User",
        Shape::new(vec![ShapeItem::computed(
            "post_views",
            path_expr(&["posts", "view_count"]),
        )]),
        None,
        vec![],
        None,
        None,
    );

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedPath)
    );
}

#[test]
fn rejects_computed_projection_literal_only_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::computed(
            "score",
            arithmetic_expr(
                literal_int_expr(1),
                query_ast::ArithmeticOp::Add,
                literal_int_expr(2),
            ),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "computed projection".to_string()
        })
    );
}

#[test]
fn rejects_computed_projection_duplicate_output_name() {
    let catalog = post_with_scalar_fields_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![
            ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
            ShapeItem::computed(
                "title",
                arithmetic_expr(
                    path_expr(&["view_count"]),
                    query_ast::ArithmeticOp::Add,
                    literal_int_expr(1),
                ),
            ),
        ]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::DuplicateOutputName {
            name: "title".to_string()
        })
    );
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
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);
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
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);

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
        schema_model::Cardinality::Required
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
fn rejects_multi_step_shape_path() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author"), PathStep::new("name")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
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

    let filter = filter_eq_string(&["title"], "Hello");

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
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::Arithmetic(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should resolve to a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("filter left side should resolve to a path")
        }
    }

    assert_eq!(compare.op(), query_ir::CompareOp::Eq);

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "Hello");
        }
        _ => panic!("filter right side should resolve to a literal"),
    }
}

#[test]
fn resolves_filter_compare_int_path_to_int_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_int(&["view_count"], 42);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(value)) => assert_eq!(*value, 42),
        _ => panic!("filter right side should resolve to an int64 literal"),
    }
}

#[test]
fn resolves_filter_compare_numeric_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            path_expr(&["view_count"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
        query_ast::CompareOp::Gt,
        literal_int_expr(10),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    assert_eq!(compare.op(), query_ir::CompareOp::Gt);

    let query_ir::ValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("comparison left side should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);

    match arithmetic.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "view_count");
        }
        _ => panic!("arithmetic left side should resolve to the view_count path"),
    }

    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
    assert_eq!(
        compare.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(10))
    );
}

#[test]
fn resolves_filter_compare_float_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            path_expr(&["rating"]),
            query_ast::ArithmeticOp::Div,
            literal_float_expr(2.5),
        ),
        query_ast::CompareOp::Ge,
        literal_float_expr(10.5),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("rating")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    let query_ir::ValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("comparison left side should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Div);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Float64);
    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Float64(2.5))
    );
    assert_eq!(
        compare.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Float64(10.5))
    );
}

#[test]
fn resolves_filter_compare_explicit_float_cast_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            function_call_expr("f64", vec![path_expr(&["view_count"])]),
            query_ast::ArithmeticOp::Div,
            literal_float_expr(2.0),
        ),
        query_ast::CompareOp::Ge,
        literal_float_expr(10.5),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    let query_ir::ValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("comparison left side should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Div);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Float64);

    let query_ir::ValueExpr::Cast(cast) = arithmetic.left() else {
        panic!("arithmetic left side should resolve to a cast value expression");
    };
    assert_eq!(cast.target_type(), schema_model::ScalarType::Float64);

    let query_ir::ValueExpr::Path(path) = cast.operand() else {
        panic!("cast operand should resolve to a path");
    };
    assert_eq!(path.steps()[0].field().name(), "view_count");

    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Float64(2.0))
    );
}

#[test]
fn resolves_filter_compare_explicit_int_cast_modulo_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            function_call_expr("i64", vec![path_expr(&["rating"])]),
            query_ast::ArithmeticOp::Mod,
            literal_int_expr(10),
        ),
        query_ast::CompareOp::Eq,
        literal_int_expr(0),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    let query_ir::ValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("comparison left side should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Mod);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);

    let query_ir::ValueExpr::Cast(cast) = arithmetic.left() else {
        panic!("arithmetic left side should resolve to a cast value expression");
    };
    assert_eq!(cast.target_type(), schema_model::ScalarType::Int64);
}

#[test]
fn rejects_arithmetic_expr_as_filter_root() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Arithmetic(ArithmeticExpr::new(
        path_expr(&["view_count"]),
        query_ast::ArithmeticOp::Add,
        literal_int_expr(1),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let _resolved =
        resolve_select(&catalog, &query).expect_err("filter root should be boolean expression");
}

#[test]
fn rejects_mixed_numeric_arithmetic_operands() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            path_expr(&["view_count"]),
            query_ast::ArithmeticOp::Add,
            path_expr(&["rating"]),
        ),
        query_ast::CompareOp::Gt,
        literal_int_expr(10),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "int64".to_string(),
            actual: "float64".to_string()
        })
    );
}

#[test]
fn rejects_float_modulo_arithmetic_operands() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            path_expr(&["rating"]),
            query_ast::ArithmeticOp::Mod,
            literal_float_expr(2.5),
        ),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("rating")]),
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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "int64".to_string(),
            actual: "float64".to_string()
        })
    );
}

#[test]
fn rejects_arithmetic_expr_with_string_left_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        arithmetic_expr(
            path_expr(&["title"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
        query_ast::CompareOp::Gt,
        literal_int_expr(10),
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
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "str".to_string()
        })
    );
}

#[test]
fn rejects_unknown_function_call() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("round", vec![path_expr(&["rating"])]),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "function call".to_string(),
        })
    );
}

#[test]
fn resolves_variadic_concat_filter_value_expr() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr(
            "concat",
            vec![
                path_expr(&["title"]),
                literal_string_expr(" - "),
                path_expr(&["subtitle"]),
            ],
        ),
        query_ast::CompareOp::Eq,
        literal_string_expr("Hello - Draft"),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);
    let resolved = resolve_select(&catalog, &query).expect("query should resolve");

    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should exist") else {
        panic!("filter should resolve to a compare expression");
    };
    let query_ir::ValueExpr::StringFunction(function) = compare.left() else {
        panic!("left side should resolve to a string function");
    };

    assert_eq!(function.kind(), query_ir::StringFunctionKind::Concat);
    assert_eq!(function.cardinality(), schema_model::Cardinality::Optional);
    assert_eq!(function.args().len(), 3);
    assert_eq!(
        function.args()[0].scalar_type(),
        schema_model::ScalarType::Str
    );
    assert_eq!(
        function.args()[1].scalar_type(),
        schema_model::ScalarType::Str
    );
    assert_eq!(
        function.args()[2].scalar_type(),
        schema_model::ScalarType::Str
    );
}

#[test]
fn resolves_str_bool_filter_value_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("str", vec![path_expr(&["published"])]),
        query_ast::CompareOp::Eq,
        literal_string_expr("true"),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);
    let resolved = resolve_select(&catalog, &query).expect("query should resolve");

    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should exist") else {
        panic!("filter should resolve to a compare expression");
    };
    let query_ir::ValueExpr::StringFunction(function) = compare.left() else {
        panic!("left side should resolve to a string function");
    };

    assert_eq!(function.kind(), query_ir::StringFunctionKind::Str);
    assert_eq!(function.cardinality(), schema_model::Cardinality::Required);
    assert_eq!(function.args().len(), 1);
    assert_eq!(
        function.args()[0].scalar_type(),
        schema_model::ScalarType::Bool
    );
}

#[test]
fn rejects_str_without_exactly_one_argument() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("str", vec![]),
        query_ast::CompareOp::Eq,
        literal_string_expr(""),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "str arity".to_string(),
        })
    );
}

#[test]
fn rejects_str_null_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("str", vec![literal_null_expr()]),
        query_ast::CompareOp::Eq,
        literal_string_expr(""),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "str null operand".to_string(),
        })
    );
}

#[test]
fn rejects_concat_null_argument() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("concat", vec![path_expr(&["title"]), literal_null_expr()]),
        query_ast::CompareOp::Eq,
        literal_string_expr("Hello"),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "concat null argument".to_string(),
        })
    );
}

#[test]
fn rejects_null_comparison_against_string_function_value() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr(
            "concat",
            vec![path_expr(&["title"]), literal_string_expr("!")],
        ),
        query_ast::CompareOp::Eq,
        literal_null_expr(),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison value".to_string(),
        })
    );
}

#[test]
fn rejects_concat_with_non_string_argument() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr(
            "concat",
            vec![path_expr(&["title"]), path_expr(&["view_count"])],
        ),
        query_ast::CompareOp::Eq,
        literal_string_expr("Hello1"),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string(),
        })
    );
}

#[test]
fn rejects_concat_without_at_least_two_arguments() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("concat", vec![path_expr(&["title"])]),
        query_ast::CompareOp::Eq,
        literal_string_expr("Hello"),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "concat arity".to_string(),
        })
    );
}

#[test]
fn rejects_numeric_cast_without_exactly_one_argument() {
    let catalog = post_with_scalar_fields_catalog();

    let no_args = Expr::Compare(CompareExpr::new(
        function_call_expr("f64", vec![]),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));
    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![]),
        Some(no_args),
        vec![],
        None,
        None,
    );

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "numeric cast arity".to_string(),
        })
    );

    let too_many_args = Expr::Compare(CompareExpr::new(
        function_call_expr("f64", vec![path_expr(&["view_count"]), literal_int_expr(1)]),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));
    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![]),
        Some(too_many_args),
        vec![],
        None,
        None,
    );

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "numeric cast arity".to_string(),
        })
    );
}

#[test]
fn rejects_numeric_cast_non_numeric_operand() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("f64", vec![path_expr(&["title"])]),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "str".to_string(),
        })
    );
}

#[test]
fn rejects_numeric_cast_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        function_call_expr("f64", vec![path_expr(&["posts", "view_count"])]),
        query_ast::CompareOp::Eq,
        literal_float_expr(1.0),
    ));

    let query = SelectQuery::new("User", Shape::new(vec![]), Some(filter), vec![], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedPath)
    );
}

#[test]
fn rejects_numeric_arithmetic_result_compared_to_string_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let left = arithmetic_expr(
        path_expr(&["view_count"]),
        query_ast::ArithmeticOp::Add,
        literal_int_expr(1),
    );
    let right = literal_string_expr("10");

    let filter = Expr::Compare(CompareExpr::new(left, query_ast::CompareOp::Eq, right));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "int64".to_string(),
            actual: "str".to_string()
        })
    );
}

#[test]
fn resolves_filter_compare_non_equality_operator() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_compare_int(&["view_count"], query_ast::CompareOp::Ge, 10);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    assert_eq!(compare.op(), query_ir::CompareOp::Ge);
}

#[test]
fn resolves_filter_compare_bool_path_to_bool_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_bool(&["published"], true);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(value)) => assert!(*value),
        _ => panic!("filter right side should resolve to a bool literal"),
    }
}

#[test]
fn rejects_filter_compare_string_path_to_int_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_int(&["title"], 42);

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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_bool_path_to_string_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_string(&["published"], "true");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "bool".to_string(),
            actual: "str".to_string()
        })
    );
}

#[test]
fn resolves_filter_compare_uuid_path_to_string_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_string(&["id"], "01987211-d8f1-7b31-8b3e-f5043e6b08f0");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "01987211-d8f1-7b31-8b3e-f5043e6b08f0");
        }
        _ => panic!("filter right side should resolve to a string literal"),
    }
}

#[test]
fn resolves_filter_compare_null_literal_to_is_null_expr() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = filter_eq_null(&["subtitle"]);

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
    let query_ir::Expr::IsNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is null expression should reference a path"),
        query_ir::ValueExpr::Arithmetic(_) => {
            panic!("is null expression should reference a path")
        }
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("is null expression should reference a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("is null expression should reference a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("is null expression should reference a path")
        }
    }
}

#[test]
fn resolves_filter_compare_left_null_literal_to_is_null_expr() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = filter_null_eq(&["subtitle"]);

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
    let query_ir::Expr::IsNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is null expression should reference a path"),
        query_ir::ValueExpr::Arithmetic(_) => {
            panic!("is null expression should reference a path")
        }
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("is null expression should reference a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("is null expression should reference a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("is null expression should reference a path")
        }
    }
}

#[test]
fn resolves_filter_compare_not_null_literal_to_is_not_null_expr() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = filter_ne_null(&["subtitle"]);

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
    let query_ir::Expr::IsNotNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is not null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is not null expression should reference a path"),
        query_ir::ValueExpr::Arithmetic(_) => {
            panic!("is not null expression should reference a path")
        }
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("is not null expression should reference a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("is not null expression should reference a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("is not null expression should reference a path")
        }
    }
}

#[test]
fn resolves_filter_compare_left_not_null_literal_to_is_not_null_expr() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = filter_null_ne(&["subtitle"]);

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
    let query_ir::Expr::IsNotNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is not null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is not null expression should reference a path"),
        query_ir::ValueExpr::Arithmetic(_) => {
            panic!("is not null expression should reference a path")
        }
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("is not null expression should reference a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("is not null expression should reference a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("is not null expression should reference a path")
        }
    }
}

#[test]
fn rejects_filter_compare_required_path_to_null_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_null(&["title"]);

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
        Err(ResolveError::NullComparisonOnNonOptionalPath {
            cardinality: "required".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_left_null_literal_to_required_path() {
    let catalog = post_with_title_catalog();

    let filter = filter_null_eq(&["title"]);

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
        Err(ResolveError::NullComparisonOnNonOptionalPath {
            cardinality: "required".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_required_path_to_not_null_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_ne_null(&["title"]);

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
        Err(ResolveError::NullComparisonOnNonOptionalPath {
            cardinality: "required".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_left_not_null_literal_to_required_path() {
    let catalog = post_with_title_catalog();

    let filter = filter_null_ne(&["title"]);

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
        Err(ResolveError::NullComparisonOnNonOptionalPath {
            cardinality: "required".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_ordering_operator_with_null_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_lt_null(&["title"]);

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
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison operator".to_string()
        })
    );
}

#[test]
fn resolves_filter_in_literal_list_to_in_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_strings(&["title"], &["Draft", "Published"]);

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
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    match in_expr.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("in expression left side should be a path"),
        query_ir::ValueExpr::Arithmetic(_) => panic!("in expression left side should be a path"),
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("in expression left side should be a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("in expression left side should be a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("in expression left side should be a path")
        }
    }

    assert_eq!(in_expr.op(), query_ir::InOp::In);
    assert_eq!(
        assert_resolved_in_list(in_expr),
        &[
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Draft".to_string())),
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Published".to_string()))
        ]
    );
}

#[test]
fn resolves_filter_in_concat_literal_item_to_value_expr() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr(
            "concat",
            vec![literal_string_expr("Dr"), literal_string_expr("aft")],
        )],
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
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let [query_ir::ValueExpr::StringFunction(function)] = assert_resolved_in_list(in_expr) else {
        panic!("membership item should resolve to a string function");
    };
    assert_eq!(function.kind(), query_ir::StringFunctionKind::Concat);
    assert_eq!(function.args().len(), 2);
}

#[test]
fn resolves_filter_in_str_literal_item_to_value_expr() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr("str", vec![literal_bool_expr(true)])],
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
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let [query_ir::ValueExpr::StringFunction(function)] = assert_resolved_in_list(in_expr) else {
        panic!("membership item should resolve to a string function");
    };
    assert_eq!(function.kind(), query_ir::StringFunctionKind::Str);
    assert_eq!(
        function.args()[0].scalar_type(),
        schema_model::ScalarType::Bool
    );
}

#[test]
fn rejects_filter_in_unknown_function_item() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr(
            "lower",
            vec![literal_string_expr("draft")],
        )],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "function call".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_concat_item_without_at_least_two_arguments() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr(
            "concat",
            vec![literal_string_expr("Draft")],
        )],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "concat arity".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_concat_item_with_null_argument() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr(
            "concat",
            vec![literal_string_expr("Draft"), literal_null_expr()],
        )],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_concat_item_with_non_string_argument() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr(
            "concat",
            vec![literal_string_expr("Draft"), literal_int_expr(1)],
        )],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_str_item_without_exactly_one_argument() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr("str", vec![])],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "str arity".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_str_null_item() {
    let catalog = post_with_title_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["title"]),
        query_ast::InOp::In,
        vec![function_call_expr("str", vec![literal_null_expr()])],
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

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string(),
        })
    );
}

#[test]
fn resolves_filter_in_int_literal_list_to_in_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_ints(&["view_count"], &[1, 2]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        assert_resolved_in_list(in_expr),
        &[
            query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
            query_ir::ValueExpr::Literal(query_ir::Literal::Int64(2))
        ]
    );
}

#[test]
fn resolves_filter_in_float_literal_list_to_in_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_floats(&["rating"], &[1.5, 2.5]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("rating")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        assert_resolved_in_list(in_expr),
        &[
            query_ir::ValueExpr::Literal(query_ir::Literal::Float64(1.5)),
            query_ir::ValueExpr::Literal(query_ir::Literal::Float64(2.5))
        ]
    );
}

#[test]
fn resolves_filter_in_arithmetic_literal_item_to_value_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["view_count"]),
        query_ast::InOp::In,
        vec![arithmetic_expr(
            literal_int_expr(1),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        )],
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let [query_ir::ValueExpr::Arithmetic(arithmetic)] = assert_resolved_in_list(in_expr) else {
        panic!("membership item should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);
    assert_eq!(
        arithmetic.left(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn resolves_filter_in_numeric_cast_literal_item_to_value_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["rating"]),
        query_ast::InOp::In,
        vec![function_call_expr("f64", vec![literal_int_expr(1)])],
    ));

    let query = SelectQuery::new("Post", Shape::new(vec![]), Some(filter), vec![], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let [query_ir::ValueExpr::Cast(cast)] = assert_resolved_in_list(in_expr) else {
        panic!("membership item should resolve to a cast value expression");
    };

    assert_eq!(cast.target_type(), schema_model::ScalarType::Float64);
    assert_eq!(
        cast.operand(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn resolves_filter_in_overflowing_arithmetic_literal_item_without_folding() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = Expr::In(in_list_expr(
        path_expr(&["view_count"]),
        query_ast::InOp::In,
        vec![arithmetic_expr(
            literal_int_expr(i64::MAX),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        )],
    ));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    let [query_ir::ValueExpr::Arithmetic(arithmetic)] = assert_resolved_in_list(in_expr) else {
        panic!("membership item should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
    assert_eq!(
        arithmetic.left(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(i64::MAX))
    );
    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn resolves_filter_in_bool_literal_list_to_in_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_bools(&["published"], &[true, false]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        assert_resolved_in_list(in_expr),
        &[
            query_ir::ValueExpr::Literal(query_ir::Literal::Bool(true)),
            query_ir::ValueExpr::Literal(query_ir::Literal::Bool(false))
        ]
    );
}

#[test]
fn rejects_filter_in_literal_list_with_incompatible_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_ints(&["title"], &[1]);

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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_bool_path_with_string_literal_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_strings(&["published"], &["true"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
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
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "bool".to_string(),
            actual: "str".to_string()
        })
    );
}

#[test]
fn resolves_filter_in_uuid_path_with_string_literal_list() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_strings(
        &["id"],
        &[
            "01987211-d8f1-7b31-8b3e-f5043e6b08f0",
            "01987211-e162-7a3f-9934-7ab05658ef7f",
        ],
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        assert_resolved_in_list(in_expr),
        &[
            query_ir::ValueExpr::Literal(query_ir::Literal::String(
                "01987211-d8f1-7b31-8b3e-f5043e6b08f0".to_string()
            )),
            query_ir::ValueExpr::Literal(query_ir::Literal::String(
                "01987211-e162-7a3f-9934-7ab05658ef7f".to_string()
            ))
        ]
    );
}

#[test]
fn resolves_filter_not_in_literal_list_to_not_in_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_not_in_strings(&["title"], &["Archived"]);

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
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(in_expr.op(), query_ir::InOp::NotIn);
}

#[test]
fn resolves_membership_select_with_required_non_unique_scalar() {
    let catalog = post_with_author_catalog();
    let query = select_post(filter_in_select(
        &["author", "score"],
        "User",
        &["score"],
        None,
    ));

    let resolved = resolve_select(&catalog, &query).expect("membership select should resolve");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a membership expression");
    };
    let select = assert_resolved_in_select(in_expr);

    assert_eq!(select.root_object_type().name(), "User");
    assert_eq!(select.shape().items()[0].output_name(), "score");
}

#[test]
fn resolves_membership_select_with_implicit_id() {
    let catalog = post_with_author_catalog();
    let query = select_post(filter_in_select(&["author", "id"], "User", &["id"], None));

    let resolved = resolve_select(&catalog, &query).expect("membership select should resolve");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a membership expression");
    };

    assert_eq!(
        assert_resolved_in_select(in_expr).shape().items()[0].output_name(),
        "id"
    );
}

#[test]
fn rejects_membership_select_with_multiple_projected_fields() {
    let catalog = post_with_author_catalog();
    let query = select_post(filter_in_select(
        &["author", "id"],
        "User",
        &["id", "name"],
        None,
    ));

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership select projection".to_string(),
        })
    );
}

#[test]
fn rejects_membership_select_with_computed_projection() {
    let catalog = post_with_author_catalog();
    let select = SelectQuery::new(
        "User",
        Shape::new(vec![ShapeItem::computed(
            "score_copy",
            path_expr(&["score"]),
        )]),
        None,
        vec![],
        None,
        None,
    );
    let filter = Expr::In(InExpr::new(
        path_expr(&["author", "score"]),
        query_ast::InOp::In,
        InRhs::Select(Box::new(select)),
    ));

    assert_eq!(
        resolve_select(&catalog, &select_post(filter)),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership select projection".to_string(),
        })
    );
}

#[test]
fn rejects_membership_select_with_optional_scalar() {
    let catalog = post_with_optional_subtitle_catalog();
    let query = select_post(filter_in_select(&["title"], "Post", &["subtitle"], None));

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership select projection".to_string(),
        })
    );
}

#[test]
fn rejects_membership_select_with_incompatible_scalar_type() {
    let catalog = post_with_author_catalog();
    let query = select_post(filter_in_select(
        &["author", "id"],
        "User",
        &["score"],
        None,
    ));

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "uuid".to_string(),
            actual: "int64".to_string(),
        })
    );
}

#[test]
fn rejects_outer_scope_path_in_membership_select() {
    let catalog = post_with_author_catalog();
    let query = select_post(filter_in_select(
        &["author", "id"],
        "User",
        &["id"],
        Some(filter_eq_string(&["title"], "Draft")),
    ));

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnknownField {
            object_type: "User".to_string(),
            field: "title".to_string(),
        })
    );
}

#[test]
fn rejects_filter_in_empty_literal_list() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_empty(&["title"]);

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
        Err(ResolveError::UnsupportedExpr {
            expr_type: "empty membership list".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_null_literal_item() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_null(&["title"]);

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
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_non_literal_item() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_path_item(&["title"], &["title"]);

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
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership list item".to_string()
        })
    );
}

#[test]
fn resolves_filter_and_expression() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = Expr::And(
        Box::new(filter_eq_string(&["title"], "Hello")),
        Box::new(filter_eq_null(&["subtitle"])),
    );

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

    let query_ir::Expr::And(left, right) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an and expression");
    };

    assert!(matches!(left.as_ref(), query_ir::Expr::Compare(_)));
    assert!(matches!(right.as_ref(), query_ir::Expr::IsNull(_)));
}

#[test]
fn resolves_filter_or_expression() {
    let catalog = post_with_optional_subtitle_catalog();

    let filter = Expr::Or(
        Box::new(filter_eq_string(&["title"], "Hello")),
        Box::new(filter_eq_null(&["subtitle"])),
    );

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

    let query_ir::Expr::Or(left, right) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an or expression");
    };

    assert!(matches!(left.as_ref(), query_ir::Expr::Compare(_)));
    assert!(matches!(right.as_ref(), query_ir::Expr::IsNull(_)));
}

#[test]
fn resolves_filter_not_expression() {
    let catalog = post_with_title_catalog();

    let filter = Expr::Not(Box::new(filter_eq_string(&["title"], "Hello")));

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

    let query_ir::Expr::Not(inner) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a not expression");
    };

    assert!(matches!(inner.as_ref(), query_ir::Expr::Compare(_)));
}

#[test]
fn rejects_filter_path_with_unknown_field() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_string(&["missing"], "Hello");

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

    let filter = filter_eq_string(&["author"], "Sheri");

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
fn resolves_order_path_to_resolved_path() {
    let catalog = post_with_title_catalog();

    let order = query_ast::OrderExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("title")])),
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
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Desc
    );

    match resolved.order_by()[0].value() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::Arithmetic(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::UnaryArithmetic(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::Cast(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::StringFunction(_) => panic!("order by should resolve to a path"),
    }
}

#[test]
fn resolves_order_concat_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        function_call_expr(
            "concat",
            vec![path_expr(&["title"]), literal_string_expr("!")],
        ),
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

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    let query_ir::ValueExpr::StringFunction(function) = resolved.order_by()[0].value() else {
        panic!("order value should resolve to a string function");
    };
    assert_eq!(function.kind(), query_ir::StringFunctionKind::Concat);
    assert_eq!(function.args().len(), 2);
}

#[test]
fn rejects_order_str_expr_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let order = query_ast::OrderExpr::new(
        function_call_expr("str", vec![path_expr(&["posts", "view_count"])]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new("User", Shape::new(vec![]), None, vec![order], None, None);

    assert_eq!(
        resolve_select(&catalog, &query),
        Err(ResolveError::UnsupportedPath)
    );
}

#[test]
fn resolves_order_numeric_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["view_count"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
        query_ast::OrderDirection::Desc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Desc
    );

    let query_ir::ValueExpr::Arithmetic(arithmetic) = resolved.order_by()[0].value() else {
        panic!("order by should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);

    match arithmetic.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "view_count");
        }
        other => panic!("arithmetic left side should resolve to a path, got {other:?}"),
    }

    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn resolves_order_numeric_cast_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        function_call_expr("f64", vec![path_expr(&["view_count"])]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new("Post", Shape::new(vec![]), None, vec![order], None, None);

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    let query_ir::ValueExpr::Cast(cast) = resolved.order_by()[0].value() else {
        panic!("order by should resolve to a cast value expression");
    };

    assert_eq!(cast.target_type(), schema_model::ScalarType::Float64);
    let query_ir::ValueExpr::Path(path) = cast.operand() else {
        panic!("cast operand should resolve to a path");
    };
    assert_eq!(path.steps()[0].field().name(), "view_count");
}

#[test]
fn resolves_order_parenthesized_numeric_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            arithmetic_expr(
                path_expr(&["view_count"]),
                query_ast::ArithmeticOp::Add,
                literal_int_expr(1),
            ),
            query_ast::ArithmeticOp::Mul,
            literal_int_expr(10),
        ),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Asc
    );

    let query_ir::ValueExpr::Arithmetic(arithmetic) = resolved.order_by()[0].value() else {
        panic!("order by should resolve to an arithmetic value expression")
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Mul);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);

    match arithmetic.left() {
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            match arithmetic.left() {
                query_ir::ValueExpr::Path(path) => {
                    assert_eq!(path.root_object_type().name(), "Post");
                    assert_eq!(path.steps().len(), 1);
                    assert_eq!(path.steps()[0].field().name(), "view_count");
                }
                other => panic!("arithmetic left side should resolve to a path, got {other:?}"),
            }
            assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
            assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);
            assert_eq!(
                arithmetic.right(),
                &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
            );
        }
        other => panic!("arithmetic left side should resolve to a arithmetic, got {other:?}"),
    }

    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(10))
    );
}

#[test]
fn resolves_order_arithmetic_expr_through_single_link_path() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["author", "score"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
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

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Asc
    );

    let query_ir::ValueExpr::Arithmetic(arithmetic) = resolved.order_by()[0].value() else {
        panic!("order by should resolve to an arithmetic value expression");
    };

    assert_eq!(arithmetic.op(), query_ir::ArithmeticOp::Add);
    assert_eq!(arithmetic.scalar_type(), schema_model::ScalarType::Int64);

    match arithmetic.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 2);
            assert_eq!(path.steps()[0].field().name(), "author");
            assert_eq!(path.steps()[1].field().name(), "score");
        }
        other => panic!("arithmetic left side should resolve to a path, got {other:?}"),
    }

    assert_eq!(
        arithmetic.right(),
        &query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1))
    );
}

#[test]
fn rejects_order_string_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["title"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
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

    assert_eq!(
        resolved,
        Err(ResolveError::NonNumericArithmeticOperand {
            actual: "str".to_string()
        })
    );
}

#[test]
fn rejects_order_mixed_numeric_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["view_count"]),
            query_ast::ArithmeticOp::Add,
            path_expr(&["rating"]),
        ),
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

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "int64".to_string(),
            actual: "float64".to_string()
        })
    );
}

#[test]
fn rejects_order_float_modulo_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["rating"]),
            query_ast::ArithmeticOp::Mod,
            literal_float_expr(2.5),
        ),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "int64".to_string(),
            actual: "float64".to_string()
        })
    );
}

#[test]
fn rejects_order_boolean_predicate_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        Compare(CompareExpr::new(
            path_expr(&["published"]),
            query_ast::CompareOp::Eq,
            literal_bool_expr(true),
        )),
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

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "comparison value".to_string()
        })
    );
}

#[test]
fn rejects_order_membership_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        Expr::In(in_list_expr(
            path_expr(&["view_count"]),
            query_ast::InOp::In,
            vec![literal_int_expr(1), literal_int_expr(2)],
        )),
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

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "boolean value".to_string()
        })
    );
}

#[test]
fn rejects_order_literal_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(literal_int_expr(1), query_ast::OrderDirection::Asc);

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

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string()
        })
    );
}

#[test]
fn rejects_order_literal_only_arithmetic_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            literal_int_expr(1),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(2),
        ),
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

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string()
        })
    );
}

#[test]
fn rejects_order_path_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let order = query_ast::OrderExpr::new(
        path_expr(&["posts", "view_count"]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "User",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("email")]),
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
fn rejects_order_arithmetic_expr_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let order = query_ast::OrderExpr::new(
        arithmetic_expr(
            path_expr(&["posts", "view_count"]),
            query_ast::ArithmeticOp::Add,
            literal_int_expr(1),
        ),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "User",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("email")]),
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
fn rejects_filter_unary_arithmetic_expr_through_multi_link() {
    let catalog = user_with_posts_catalog();

    let filter = Expr::Compare(CompareExpr::new(
        Expr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            path_expr(&["posts", "view_count"]),
        )),
        query_ast::CompareOp::Lt,
        literal_int_expr(0),
    ));

    let query = SelectQuery::new(
        "User",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("email")]),
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
fn rejects_order_path_with_link_field() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        Expr::Path(Path::new(vec![PathStep::new("author")])),
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

#[test]
fn resolves_filter_path_through_single_link_to_scalar_field() {
    let catalog = post_with_author_catalog();

    let filter = filter_eq_string(&["author", "name"], "Sheri");

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

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    assert_eq!(compare.op(), query_ir::CompareOp::Eq);

    match compare.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 2);
            assert_eq!(
                path.result_cardinality(),
                schema_model::Cardinality::Required
            );

            let link_step = &path.steps()[0];
            assert_eq!(link_step.field().owner_object_type().name(), "Post");
            assert_eq!(link_step.field().name(), "author");
            assert_eq!(link_step.cardinality(), schema_model::Cardinality::Required);

            match link_step.kind() {
                query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                    assert_eq!(target_object_type.name(), "User");
                }
                query_ir::ResolvedPathStepKind::Scalar => {
                    panic!("first path step should resolve to a link")
                }
            }

            let scalar_step = &path.steps()[1];
            assert_eq!(scalar_step.field().owner_object_type().name(), "User");
            assert_eq!(scalar_step.field().name(), "name");
            assert_eq!(
                scalar_step.cardinality(),
                schema_model::Cardinality::Required
            );

            match scalar_step.kind() {
                query_ir::ResolvedPathStepKind::Scalar => {}
                query_ir::ResolvedPathStepKind::Link { .. } => {
                    panic!("terminal path step should resolve to a scalar")
                }
            }
        }
        query_ir::ValueExpr::Literal(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::Arithmetic(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should resolve to a path")
        }
        query_ir::ValueExpr::Cast(_) => panic!("filter left side should resolve to a path"),
        query_ir::ValueExpr::StringFunction(_) => {
            panic!("filter left side should resolve to a path")
        }
    }

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "Sheri");
        }
        _ => panic!("filter right side should resolve to a string literal"),
    }
}

#[test]
fn resolves_order_path_through_single_link_to_scalar_field() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        Expr::Path(Path::new(vec![
            PathStep::new("author"),
            PathStep::new("name"),
        ])),
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

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Asc
    );

    match resolved.order_by()[0].value() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 2);
            assert_eq!(
                path.result_cardinality(),
                schema_model::Cardinality::Required
            );

            let link_step = &path.steps()[0];
            assert_eq!(link_step.field().owner_object_type().name(), "Post");
            assert_eq!(link_step.field().name(), "author");
            assert_eq!(link_step.cardinality(), schema_model::Cardinality::Required);

            match link_step.kind() {
                query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                    assert_eq!(target_object_type.name(), "User");
                }
                query_ir::ResolvedPathStepKind::Scalar => {
                    panic!("first path step should resolve to a link")
                }
            }

            let scalar_step = &path.steps()[1];
            assert_eq!(scalar_step.field().owner_object_type().name(), "User");
            assert_eq!(scalar_step.field().name(), "name");
            assert_eq!(
                scalar_step.cardinality(),
                schema_model::Cardinality::Required
            );

            match scalar_step.kind() {
                query_ir::ResolvedPathStepKind::Scalar => {}
                query_ir::ResolvedPathStepKind::Link { .. } => {
                    panic!("terminal path step should resolve to a scalar")
                }
            }
        }
        query_ir::ValueExpr::Literal(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::Arithmetic(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::UnaryArithmetic(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::Cast(_) => panic!("order by should resolve to a path"),
        query_ir::ValueExpr::StringFunction(_) => panic!("order by should resolve to a path"),
    }
}
