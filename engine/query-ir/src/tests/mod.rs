mod fixtures;

use crate::{
    ArithmeticExpr, ArithmeticOp, Assignment, AssignmentValue, CastExpr, CompareExpr, CompareOp,
    Expr, InExpr, InOp, InsertQuery, Literal, OrderDirection, OrderExpr, ResolvedComputedField,
    ResolvedPath, ResolvedPathError, ResolvedPathStep, ResolvedPathStepKind, ResolvedShape,
    ResolvedShapeField, ResolvedShapeItem, SelectQuery, StringFunctionArg, StringFunctionExpr,
    StringFunctionKind, UnaryArithmeticExpr, UnaryArithmeticOp, UpdateQuery, ValueExpr,
};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use fixtures::{
    empty_post_shape, post_author_field, post_subtitle_field, post_subtitle_path_value,
    post_title_field, post_title_path_value, post_type, post_view_count_path_value,
    user_name_field, user_name_shape, user_type,
};
use schema_model::{Cardinality, ObjectTypeId, ScalarType};

#[test]
fn resolved_select_query_can_store_root_object_type() {
    let root_object_type = post_type();
    let shape = ResolvedShape::new(root_object_type.clone(), vec![]);

    let query = SelectQuery::new(root_object_type, shape, None, vec![], None, None);

    assert_eq!(query.root_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(query.root_object_type().name(), "Post");
}

#[test]
fn resolved_insert_query_can_store_root_object_type() {
    let query = InsertQuery::new(post_type(), vec![]);

    assert_eq!(query.root_object_type().id(), ObjectTypeId::new(1));
    assert_eq!(query.root_object_type().name(), "Post");
    assert!(query.assignments().is_empty());
}

#[test]
fn resolved_insert_query_preserves_assignment_order() {
    let query = InsertQuery::new(
        post_type(),
        vec![
            Assignment::new(
                post_title_field(),
                AssignmentValue::Scalar(Literal::String("The Witch Trial".to_string())),
            ),
            Assignment::new(
                post_author_field(),
                AssignmentValue::LinkId("00000000-0000-0000-0000-000000000001".to_string()),
            ),
        ],
    );

    let assignments = query.assignments();

    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].field().name(), "title");
    assert_eq!(assignments[1].field().name(), "author");
}

#[test]
fn resolved_update_query_stores_target_filter_and_assignment_order() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let query = UpdateQuery::new(
        post_type(),
        Some(filter.clone()),
        vec![
            Assignment::new(
                post_title_field(),
                AssignmentValue::Scalar(Literal::String("Closed".to_string())),
            ),
            Assignment::new(
                post_author_field(),
                AssignmentValue::LinkId("user-2".to_string()),
            ),
        ],
    );

    assert_eq!(query.target_object_type().name(), "Post");
    assert_eq!(query.filter(), Some(&filter));
    assert_eq!(query.assignments()[0].field().name(), "title");
    assert_eq!(query.assignments()[1].field().name(), "author");
}

#[test]
fn resolved_assignment_can_store_supported_value_kinds() {
    let scalar = Assignment::new(
        post_title_field(),
        AssignmentValue::Scalar(Literal::String(
            "00000000-0000-0000-0000-000000000001".to_string(),
        )),
    );
    let link_id = Assignment::new(
        post_author_field(),
        AssignmentValue::LinkId("00000000-0000-0000-0000-000000000001".to_string()),
    );
    let scalar_null = Assignment::new(post_subtitle_field(), AssignmentValue::ScalarNull);
    let link_null = Assignment::new(post_author_field(), AssignmentValue::LinkNull);

    assert!(matches!(
        scalar.value(),
        AssignmentValue::Scalar(Literal::String(_))
    ));
    assert!(matches!(link_id.value(), AssignmentValue::LinkId(_)));
    assert_eq!(scalar_null.value(), &AssignmentValue::ScalarNull);
    assert_eq!(link_null.value(), &AssignmentValue::LinkNull);
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
fn resolved_shape_can_contain_computed_projection() {
    let computed = ResolvedComputedField::new(
        "score",
        ValueExpr::Arithmetic(ArithmeticExpr::new(
            post_view_count_path_value(),
            ArithmeticOp::Add,
            ValueExpr::Literal(Literal::Int64(1)),
            ScalarType::Int64,
        )),
        ScalarType::Int64,
        Cardinality::Required,
    );

    let shape = ResolvedShape::with_items(post_type(), vec![ResolvedShapeItem::Computed(computed)]);
    let items = shape.items();

    assert_eq!(items.len(), 1);
    let ResolvedShapeItem::Computed(computed) = &items[0] else {
        panic!("shape item should be a computed projection");
    };
    assert_eq!(computed.output_name(), "score");
    assert_eq!(computed.scalar_type(), ScalarType::Int64);
    assert_eq!(computed.cardinality(), Cardinality::Required);

    let ValueExpr::Arithmetic(arithmetic) = computed.value() else {
        panic!("computed projection should store an arithmetic value expression");
    };
    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
}

#[test]
fn unary_arithmetic_expr_can_store_operand_and_operator() {
    let expr = ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
        UnaryArithmeticOp::Minus,
        ValueExpr::Literal(Literal::Int64(1)),
        ScalarType::Int64,
    ));

    let ValueExpr::UnaryArithmetic(unary) = expr else {
        panic!("value expression should store a unary arithmetic expression");
    };

    assert_eq!(unary.op(), UnaryArithmeticOp::Minus);
    assert_eq!(unary.operand(), &ValueExpr::Literal(Literal::Int64(1)));
    assert_eq!(unary.scalar_type(), ScalarType::Int64);
}

#[test]
fn cast_expr_can_store_operand_and_target_type() {
    let expr = ValueExpr::Cast(CastExpr::new(
        ValueExpr::Literal(Literal::Int64(1)),
        ScalarType::Float64,
    ));

    let ValueExpr::Cast(cast) = expr else {
        panic!("value expression should store a cast expression");
    };

    assert_eq!(cast.operand(), &ValueExpr::Literal(Literal::Int64(1)));
    assert_eq!(cast.target_type(), ScalarType::Float64);
}

#[test]
fn string_function_expr_can_store_kind_arguments_types_and_cardinality() {
    let expr = ValueExpr::StringFunction(StringFunctionExpr::new(
        StringFunctionKind::Concat,
        vec![
            StringFunctionArg::new(
                ValueExpr::Literal(Literal::String("Hello".to_string())),
                ScalarType::Str,
            ),
            StringFunctionArg::new(post_title_path_value(), ScalarType::Str),
        ],
        Cardinality::Required,
    ));

    let ValueExpr::StringFunction(function) = expr else {
        panic!("value expression should store a string function");
    };

    assert_eq!(function.kind(), StringFunctionKind::Concat);
    assert_eq!(function.cardinality(), Cardinality::Required);
    assert_eq!(function.args().len(), 2);
    assert_eq!(function.args()[0].scalar_type(), ScalarType::Str);
    assert_eq!(
        function.args()[0].value(),
        &ValueExpr::Literal(Literal::String("Hello".to_string()))
    );
    assert_eq!(function.args()[1].scalar_type(), ScalarType::Str);
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
fn resolved_path_can_represent_root_scalar_field() {
    let path = ResolvedPath::try_new(
        post_type(),
        vec![ResolvedPathStep::scalar(
            post_title_field(),
            Cardinality::Required,
        )],
    )
    .expect("root scalar field path should be valid");

    assert_eq!(path.root_object_type().name(), "Post");
    assert_eq!(path.result_cardinality(), Cardinality::Required);
    assert_eq!(path.steps().len(), 1);

    let step = &path.steps()[0];
    assert_eq!(step.field().owner_object_type().name(), "Post");
    assert_eq!(step.field().name(), "title");
    assert_eq!(step.cardinality(), Cardinality::Required);

    match step.kind() {
        ResolvedPathStepKind::Scalar => {}
        ResolvedPathStepKind::Link { .. } => {
            panic!("root scalar field path should contain a scalar step")
        }
    }
}

#[test]
fn resolved_path_result_cardinality_becomes_optional_when_any_step_is_optional() {
    let path = ResolvedPath::try_new(
        post_type(),
        vec![
            ResolvedPathStep::link(post_author_field(), user_type(), Cardinality::Optional),
            ResolvedPathStep::scalar(user_name_field(), Cardinality::Required),
        ],
    )
    .expect("path through an optional link should be valid");

    assert_eq!(path.result_cardinality(), Cardinality::Optional);
}

#[test]
fn resolved_path_rejects_empty_steps() {
    let error = ResolvedPath::try_new(post_type(), vec![]).expect_err("empty path is invalid");

    assert_eq!(error, ResolvedPathError::EmptyPath);
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
fn resolved_select_query_can_store_order_by_path() {
    let order = OrderExpr::new(post_title_path_value(), OrderDirection::Desc);

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
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        ValueExpr::Literal(_) => panic!("order by should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("order by should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => panic!("order by should reference a resolved path"),
        ValueExpr::Cast(_) => panic!("order by should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("order by should reference a resolved path"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_compare_expr() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
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

    let Expr::Compare(compare) = query.filter().expect("select query has filter") else {
        panic!("filter should be a compare expression");
    };
    assert_eq!(compare.op(), CompareOp::Eq);

    match compare.left() {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        ValueExpr::Literal(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should reference a resolved path")
        }
        ValueExpr::Cast(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("filter left side should reference a resolved path"),
    }

    match compare.right() {
        ValueExpr::Literal(Literal::String(value)) => assert_eq!(value, "Hello"),
        ValueExpr::Literal(other) => {
            panic!("filter right side should store a string literal, got {other:?}")
        }
        ValueExpr::Path(_) => panic!("filter right side should store a literal"),
        ValueExpr::Arithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::UnaryArithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::Cast(_) => panic!("filter right side should store a literal"),
        ValueExpr::StringFunction(_) => panic!("filter right side should store a literal"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_compare_int_literal() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::Int64(42)),
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Compare(compare) = query.filter().expect("select query has filter") else {
        panic!("filter should be a compare expression");
    };
    assert_eq!(compare.op(), CompareOp::Eq);

    match compare.right() {
        ValueExpr::Literal(Literal::Int64(value)) => assert_eq!(*value, 42),
        ValueExpr::Literal(other) => {
            panic!("filter right side should store an int literal, got {other:?}")
        }
        ValueExpr::Path(_) => panic!("filter right side should store a literal"),
        ValueExpr::Arithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::UnaryArithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::Cast(_) => panic!("filter right side should store a literal"),
        ValueExpr::StringFunction(_) => panic!("filter right side should store a literal"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_compare_bool_literal() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::Bool(true)),
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Compare(compare) = query.filter().expect("select query has filter") else {
        panic!("filter should be a compare expression");
    };
    assert_eq!(compare.op(), CompareOp::Eq);

    match compare.right() {
        ValueExpr::Literal(Literal::Bool(value)) => assert!(*value),
        ValueExpr::Literal(other) => {
            panic!("filter right side should store a bool literal, got {other:?}")
        }
        ValueExpr::Path(_) => panic!("filter right side should store a literal"),
        ValueExpr::Arithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::UnaryArithmetic(_) => panic!("filter right side should store a literal"),
        ValueExpr::Cast(_) => panic!("filter right side should store a literal"),
        ValueExpr::StringFunction(_) => panic!("filter right side should store a literal"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_non_equality_compare_expr() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Ne,
        ValueExpr::Literal(Literal::String("Archived".to_string())),
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Compare(compare) = query.filter().expect("select query has filter") else {
        panic!("filter should be a compare expression");
    };

    assert_eq!(compare.op(), CompareOp::Ne);
}

#[test]
fn resolved_select_query_can_store_filter_is_null_expr() {
    let filter = Expr::IsNull(post_subtitle_path_value());

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let filter = query.filter().expect("select query has filter");

    let Expr::IsNull(value) = filter else {
        panic!("filter should be an is null expression");
    };

    match value {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        ValueExpr::Literal(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should reference a resolved path")
        }
        ValueExpr::Cast(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("filter left side should reference a resolved path"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_is_not_null_expr() {
    let filter = Expr::IsNotNull(post_subtitle_path_value());

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let filter = query.filter().expect("select query has filter");

    let Expr::IsNotNull(value) = filter else {
        panic!("filter should be an is not null expression");
    };

    match value {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "subtitle");
        }
        ValueExpr::Literal(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should reference a resolved path")
        }
        ValueExpr::Cast(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("filter left side should reference a resolved path"),
    }
}

#[test]
fn resolved_select_query_can_store_filter_in_expr() {
    let filter = Expr::In(InExpr::new(
        post_title_path_value(),
        InOp::In,
        vec![
            ValueExpr::Literal(Literal::String("Draft".to_string())),
            ValueExpr::Literal(Literal::String("Published".to_string())),
        ],
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::In(in_expr) = query.filter().expect("select query has filter") else {
        panic!("filter should be an in expression");
    };

    match in_expr.left() {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        ValueExpr::Literal(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => {
            panic!("filter left side should reference a resolved path")
        }
        ValueExpr::Cast(_) => panic!("filter left side should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("filter left side should reference a resolved path"),
    }

    assert_eq!(in_expr.op(), InOp::In);
    assert_eq!(in_expr.right().len(), 2);
    assert_eq!(
        in_expr.right()[0],
        ValueExpr::Literal(Literal::String("Draft".to_string()))
    );
    assert_eq!(
        in_expr.right()[1],
        ValueExpr::Literal(Literal::String("Published".to_string()))
    );
}

#[test]
fn resolved_select_query_can_store_filter_not_in_expr() {
    let filter = Expr::In(InExpr::new(
        post_title_path_value(),
        InOp::NotIn,
        vec![ValueExpr::Literal(Literal::String("Archived".to_string()))],
    ));

    let query = SelectQuery::new(
        post_type(),
        empty_post_shape(),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::In(in_expr) = query.filter().expect("select query has filter") else {
        panic!("filter should be an in expression");
    };

    assert_eq!(in_expr.op(), InOp::NotIn);
}

#[test]
fn value_expr_can_reference_resolved_path() {
    let path = ResolvedPath::try_new(
        post_type(),
        vec![ResolvedPathStep::scalar(
            post_title_field(),
            Cardinality::Required,
        )],
    )
    .expect("root scalar field path should be valid");

    let value = ValueExpr::Path(path);

    match value {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        ValueExpr::Literal(_) => panic!("value expression should reference a resolved path"),
        ValueExpr::Arithmetic(_) => panic!("value expression should reference a resolved path"),
        ValueExpr::UnaryArithmetic(_) => {
            panic!("value expression should reference a resolved path")
        }
        ValueExpr::Cast(_) => panic!("value expression should reference a resolved path"),
        ValueExpr::StringFunction(_) => panic!("value expression should reference a resolved path"),
    }
}

#[test]
fn value_expr_can_store_literal() {
    let value = ValueExpr::Literal(Literal::String("Hello".to_string()));

    match value {
        ValueExpr::Literal(Literal::String(value)) => assert_eq!(value, "Hello"),
        ValueExpr::Literal(other) => {
            panic!("value expression should store string literal, got {other:?}")
        }
        ValueExpr::Path(_) => panic!("value expression should store a literal"),
        ValueExpr::Arithmetic(_) => panic!("value expression should store a literal"),
        ValueExpr::UnaryArithmetic(_) => panic!("value expression should store a literal"),
        ValueExpr::Cast(_) => panic!("value expression should store a literal"),
        ValueExpr::StringFunction(_) => panic!("value expression should store a literal"),
    }
}

#[test]
fn value_expr_can_store_arithmetic_expr() {
    let value = ValueExpr::Arithmetic(ArithmeticExpr::new(
        post_view_count_path_value(),
        ArithmeticOp::Add,
        ValueExpr::Literal(Literal::Int64(1)),
        ScalarType::Int64,
    ));

    let ValueExpr::Arithmetic(arithmetic) = value else {
        panic!("value expression should store arithmetic expression");
    };

    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
    assert_eq!(arithmetic.scalar_type(), ScalarType::Int64);

    match arithmetic.left() {
        ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "view_count");
        }
        _ => panic!("arithmetic left side should store a resolved path"),
    }

    assert_eq!(arithmetic.right(), &ValueExpr::Literal(Literal::Int64(1)));
}

#[test]
fn resolved_select_query_can_store_filter_and_expr() {
    let left = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Hello".to_string())),
    ));

    let right = Expr::Compare(CompareExpr::new(
        post_subtitle_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));

    let filter = Expr::And(Box::new(left), Box::new(right));

    let query = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::And(left, right) = query.filter().expect("select query has filter") else {
        panic!("filter should be an and expression");
    };

    assert!(matches!(left.as_ref(), Expr::Compare(_)));
    assert!(matches!(right.as_ref(), Expr::Compare(_)));
}

#[test]
fn resolved_select_query_can_store_filter_or_expr() {
    let left = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Hello".to_string())),
    ));

    let right = Expr::Compare(CompareExpr::new(
        post_subtitle_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));

    let filter = Expr::Or(Box::new(left), Box::new(right));

    let query = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Or(left, right) = query.filter().expect("select query has filter") else {
        panic!("filter should be an or expression");
    };

    assert!(matches!(left.as_ref(), Expr::Compare(_)));
    assert!(matches!(right.as_ref(), Expr::Compare(_)));
}

#[test]
fn resolved_select_query_can_store_filter_not_expr() {
    let inner = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Hello".to_string())),
    ));

    let filter = Expr::Not(Box::new(inner));

    let query = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let Expr::Not(inner) = query.filter().expect("select query has filter") else {
        panic!("filter should be a not expression");
    };

    assert!(matches!(inner.as_ref(), Expr::Compare(_)));
}
