use super::fixtures::{
    post_author_name_path_value, post_author_score_path_value, post_author_shape_field,
    post_id_shape_field, post_or_path_value, post_or_shape_field, post_query_with_filter,
    post_query_with_limit_and_offset, post_query_with_order_by, post_query_with_shape,
    post_quote_path_value, post_title_path_value, post_title_shape_field, post_type,
    post_view_count_path_value, user_name_field, user_name_shape_field, user_type,
};
use crate::{SQLiteBindValue, render_follow_up, render_select};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;

#[test]
fn sqlite_sqlgen_can_render_simple_root_scalar_select() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\""
    );
}

#[test]
fn sqlite_sqlgen_quotes_select_identifiers() {
    let ir = post_query_with_shape(vec![post_or_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"or\" FROM \"post\" AS \"root\""
    );
}

#[test]
fn sqlite_sqlgen_can_render_multiple_root_selected_values() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_id_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\", \"root\".\"id\" FROM \"post\" AS \"root\""
    );
}

#[test]
fn sqlite_sqlgen_can_render_computed_projection() {
    let computed = query_ir::ResolvedComputedField::new(
        "score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            post_view_count_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );

    let ir = query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::with_items(
            post_type(),
            vec![query_ir::ResolvedShapeItem::Computed(computed)],
        ),
        None,
        vec![],
        None,
        None,
    );
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT (\"root\".\"view_count\" + ?) AS \"__gelite_value_0\" FROM \"post\" AS \"root\""
    );
    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(1)]);
    assert_eq!(statement.output_names(), vec![Some("score".to_string())]);
}

#[test]
fn sqlite_sqlgen_can_render_computed_unary_arithmetic_projection() {
    let computed = query_ir::ResolvedComputedField::new(
        "neg_views",
        query_ir::ValueExpr::UnaryArithmetic(query_ir::UnaryArithmeticExpr::new(
            query_ir::UnaryArithmeticOp::Minus,
            post_view_count_path_value(),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );

    let ir = query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::with_items(
            post_type(),
            vec![query_ir::ResolvedShapeItem::Computed(computed)],
        ),
        None,
        vec![],
        None,
        None,
    );
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT (-\"root\".\"view_count\") AS \"__gelite_value_0\" FROM \"post\" AS \"root\""
    );
    assert_eq!(statement.bind_values(), &[]);
}

#[test]
fn sqlite_sqlgen_can_render_selected_single_link_join() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\", \"author\".\"id\", \"author\".\"name\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\""
    );
    assert_eq!(
        statement.output_names(),
        vec![Some("title".to_string()), None, Some("name".to_string())]
    )
}

#[test]
fn sqlite_sqlgen_maps_rendered_columns_to_nested_result_shape() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);
    let statement = render_select(&sqlite_query_plan::plan_select(&ir));
    let shape = statement
        .result_shape()
        .expect("rendered select should retain its result shape");

    assert_eq!(shape.identity_column_index(), None);
    assert_eq!(shape.fields()[0].output_name(), "title");
    assert_eq!(shape.fields()[0].column_index(), Some(0));

    let author = &shape.fields()[1];
    assert_eq!(author.output_name(), "author");
    assert_eq!(author.column_index(), None);

    let author_shape = author
        .nested_shape()
        .expect("author should retain a nested result shape");
    assert_eq!(author_shape.identity_column_index(), Some(1));
    assert_eq!(author_shape.fields()[0].output_name(), "name");
    assert_eq!(author_shape.fields()[0].column_index(), Some(2));
}

#[test]
fn sqlite_sqlgen_defers_multi_link_result_fields_to_follow_up_rendering() {
    let posts_shape = query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]);
    let posts = query_ir::ResolvedShapeField::new(
        "posts",
        schema_model::FieldRef::new(schema_model::FieldId::new(3), user_type(), "posts"),
        schema_model::Cardinality::Many,
        Some(posts_shape),
    );
    let ir = query_ir::SelectQuery::new(
        user_type(),
        query_ir::ResolvedShape::new(user_type(), vec![posts]),
        None,
        vec![],
        None,
        None,
    );
    let plan = sqlite_query_plan::plan_select(&ir);

    assert_eq!(plan.result_shape().fields()[0].output_name(), "posts");
    assert_eq!(
        plan.result_shape().fields()[0].follow_up_fetch_index(),
        Some(0)
    );

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"id\" FROM \"user\" AS \"root\""
    );
    assert!(
        statement
            .result_shape()
            .expect("root result shape should be rendered")
            .fields()
            .is_empty()
    );
}

#[test]
fn sqlite_sqlgen_batches_multi_link_parent_ids_in_one_follow_up_statement() {
    let posts_shape = query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]);
    let posts = query_ir::ResolvedShapeField::new(
        "posts",
        schema_model::FieldRef::new(schema_model::FieldId::new(3), user_type(), "posts"),
        schema_model::Cardinality::Many,
        Some(posts_shape),
    );
    let ir = query_ir::SelectQuery::new(
        user_type(),
        query_ir::ResolvedShape::new(user_type(), vec![posts]),
        None,
        vec![],
        None,
        None,
    );
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_follow_up(
        &plan.follow_up_fetches()[0],
        &["user-1".to_string(), "user-2".to_string()],
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"user__posts\".\"source_id\", \"root\".\"id\", \"root\".\"title\" FROM \"user__posts\" INNER JOIN \"post\" AS \"root\" ON \"user__posts\".\"target_id\" = \"root\".\"id\" WHERE \"user__posts\".\"source_id\" IN (?, ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("user-1".to_string()),
            SQLiteBindValue::String("user-2".to_string()),
        ]
    );
    assert_eq!(statement.parent_identity_column_index(), Some(0));

    let shape = statement
        .result_shape()
        .expect("follow-up statement should retain its target result shape");
    assert_eq!(shape.identity_column_index(), Some(1));
    assert_eq!(shape.fields()[0].column_index(), Some(2));
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_string_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_quotes_filter_identifiers() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_quote_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"quote\"\"field\" = ?"
    );
}

#[test]
fn sqlite_sqlgen_can_render_comparison_operators() {
    let cases = [
        (query_ir::CompareOp::Ne, "!="),
        (query_ir::CompareOp::Lt, "<"),
        (query_ir::CompareOp::Le, "<="),
        (query_ir::CompareOp::Gt, ">"),
        (query_ir::CompareOp::Ge, ">="),
    ];

    for (op, expected_sql_op) in cases {
        let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
            post_title_path_value(),
            op,
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Archived".to_string())),
        ));

        let ir = post_query_with_filter(filter);
        let plan = sqlite_query_plan::plan_select(&ir);

        let statement = render_select(&plan);

        assert_eq!(
            statement.sql(),
            alloc::format!(
                "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" {expected_sql_op} ?"
            )
        );

        assert_eq!(
            statement.bind_values(),
            &[SQLiteBindValue::String("Archived".to_string())]
        );
    }
}

#[test]
fn sqlite_sqlgen_can_render_single_link_scalar_equals_string_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_author_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Sheri".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"name\" = ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_int_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(42)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(42)]);
}

#[test]
fn sqlite_sqlgen_can_render_arithmetic_filter_compared_to_int_literal() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(10)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"view_count\" + ?) > ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::Int64(1), SQLiteBindValue::Int64(10)]
    );
}

#[test]
fn sqlite_sqlgen_can_render_arithmetic_filter_compared_to_float_literal() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Div,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(2.5)),
        schema_model::ScalarType::Float64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Ge,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(10.5)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"view_count\" / ?) >= ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::Float64(2.5),
            SQLiteBindValue::Float64(10.5)
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_cast_filter_compared_to_float_literal() {
    let cast = query_ir::ValueExpr::Cast(query_ir::CastExpr::new(
        post_view_count_path_value(),
        schema_model::ScalarType::Float64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        cast,
        query_ir::CompareOp::Ge,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(10.5)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE CAST(\"root\".\"view_count\" AS REAL) >= ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Float64(10.5)]);
}

#[test]
fn sqlite_sqlgen_can_render_concat_filter() {
    let concat = query_ir::ValueExpr::StringFunction(query_ir::StringFunctionExpr::new(
        query_ir::StringFunctionKind::Concat,
        vec![
            query_ir::StringFunctionArg::new(
                post_title_path_value(),
                schema_model::ScalarType::Str,
            ),
            query_ir::StringFunctionArg::new(
                query_ir::ValueExpr::Literal(query_ir::Literal::String("!".to_string())),
                schema_model::ScalarType::Str,
            ),
        ],
        schema_model::Cardinality::Required,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        concat,
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello!".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"title\" || ?) = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("!".to_string()),
            SQLiteBindValue::String("Hello!".to_string())
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_str_bool_filter() {
    let str_bool = query_ir::ValueExpr::StringFunction(query_ir::StringFunctionExpr::new(
        query_ir::StringFunctionKind::Str,
        vec![query_ir::StringFunctionArg::new(
            query_ir::ValueExpr::Literal(query_ir::Literal::Bool(true)),
            schema_model::ScalarType::Bool,
        )],
        schema_model::Cardinality::Required,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        str_bool,
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("true".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE CASE WHEN ? IS NULL THEN NULL WHEN ? THEN 'true' ELSE 'false' END = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::Bool(true),
            SQLiteBindValue::Bool(true),
            SQLiteBindValue::String("true".to_string())
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_str_uuid_filter_as_stored_text() {
    let str_uuid = query_ir::ValueExpr::StringFunction(query_ir::StringFunctionExpr::new(
        query_ir::StringFunctionKind::Str,
        vec![query_ir::StringFunctionArg::new(
            post_title_path_value(),
            schema_model::ScalarType::Uuid,
        )],
        schema_model::Cardinality::Required,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        str_uuid,
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String(
            "00000000-0000-0000-0000-000000000001".to_string(),
        )),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String(
            "00000000-0000-0000-0000-000000000001".to_string()
        )]
    );
}

#[test]
fn sqlite_sqlgen_can_render_cast_arithmetic_filter_with_bind_order() {
    let cast = query_ir::ValueExpr::Cast(query_ir::CastExpr::new(
        post_view_count_path_value(),
        schema_model::ScalarType::Float64,
    ));
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        cast,
        query_ir::ArithmeticOp::Div,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(2.0)),
        schema_model::ScalarType::Float64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Ge,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(10.5)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (CAST(\"root\".\"view_count\" AS REAL) / ?) >= ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::Float64(2.0),
            SQLiteBindValue::Float64(10.5)
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_arithmetic_filter_with_joined_operand() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_author_score_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(10)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE (\"author\".\"score\" + ?) > ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::Int64(1), SQLiteBindValue::Int64(10)]
    );
}

#[test]
fn sqlite_sqlgen_can_render_unary_arithmetic_filter_with_joined_operand() {
    let unary = query_ir::ValueExpr::UnaryArithmetic(query_ir::UnaryArithmeticExpr::new(
        query_ir::UnaryArithmeticOp::Minus,
        post_author_score_path_value(),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        unary,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(0)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE (-\"author\".\"score\") > ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(0)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_bool_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(true)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Bool(true)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_is_null_filter() {
    let filter = query_ir::Expr::IsNull(post_title_path_value());

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" IS NULL"
    );

    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_is_not_null_filter() {
    let filter = query_ir::Expr::IsNotNull(post_title_path_value());

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" IS NOT NULL"
    );

    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_in_filter() {
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_title_path_value(),
        query_ir::InOp::In,
        query_ir::InRhs::List(vec![
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Draft".to_string())),
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Published".to_string())),
        ]),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" IN (?, ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Draft".to_string()),
            SQLiteBindValue::String("Published".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_renders_membership_select_with_deterministic_bind_order() {
    let user_name = user_name_field();
    let membership_select = query_ir::SelectQuery::new(
        user_type(),
        query_ir::ResolvedShape::new(user_type(), vec![user_name_shape_field()]),
        Some(query_ir::Expr::Compare(query_ir::CompareExpr::new(
            query_ir::ValueExpr::Path(
                query_ir::ResolvedPath::try_new(
                    user_type(),
                    vec![query_ir::ResolvedPathStep::scalar(
                        user_name,
                        schema_model::Cardinality::Required,
                    )],
                )
                .expect("user name path should be valid"),
            ),
            query_ir::CompareOp::Eq,
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Sheri".to_string())),
        ))),
        vec![],
        None,
        None,
    );
    let membership = query_ir::Expr::In(query_ir::InExpr::new(
        post_title_path_value(),
        query_ir::InOp::In,
        query_ir::InRhs::Select(Box::new(membership_select)),
    ));
    let later_filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Ne,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Archived".to_string())),
    ));
    let ir = post_query_with_filter(query_ir::Expr::And(
        Box::new(membership),
        Box::new(later_filter),
    ));

    let statement = render_select(&sqlite_query_plan::plan_select(&ir));

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"title\" IN (SELECT \"root\".\"name\" FROM \"user\" AS \"root\" WHERE \"root\".\"name\" = ?) AND \"root\".\"title\" != ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Sheri".to_string()),
            SQLiteBindValue::String("Archived".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_in_arithmetic_value_filter() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        query_ir::ArithmeticOp::Div,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(0)),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_view_count_path_value(),
        query_ir::InOp::In,
        query_ir::InRhs::List(vec![arithmetic]),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"view_count\" IN ((? / ?))"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::Int64(1), SQLiteBindValue::Int64(0)]
    );
}

#[test]
fn sqlite_sqlgen_can_render_single_link_scalar_not_in_filter() {
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_author_name_path_value(),
        query_ir::InOp::NotIn,
        query_ir::InRhs::List(vec![query_ir::ValueExpr::Literal(
            query_ir::Literal::String("Sheri".to_string()),
        )]),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"name\" NOT IN (?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_and_filter() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let right = query_ir::Expr::IsNull(post_title_path_value());
    let filter = query_ir::Expr::And(Box::new(left), Box::new(right));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"title\" = ? AND \"root\".\"title\" IS NULL)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_or_filter_with_bind_order() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let right = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Draft".to_string())),
    ));
    let filter = query_ir::Expr::Or(Box::new(left), Box::new(right));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE (\"root\".\"title\" = ? OR \"root\".\"title\" = ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Hello".to_string()),
            SQLiteBindValue::String("Draft".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_not_filter() {
    let inner = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let filter = query_ir::Expr::Not(Box::new(inner));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE NOT (\"root\".\"title\" = ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_root_scalar_field_desc() {
    let order_by =
        query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Desc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" ORDER BY \"root\".\"title\" DESC"
    );
}

#[test]
fn sqlite_sqlgen_quotes_order_by_identifiers() {
    let order_by = query_ir::OrderExpr::new(post_or_path_value(), query_ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" ORDER BY \"root\".\"or\" ASC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_single_link_scalar_field() {
    let order_by =
        query_ir::OrderExpr::new(post_author_name_path_value(), query_ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" ORDER BY \"author\".\"name\" ASC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_arithmetic_expr() {
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Desc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" ORDER BY (\"root\".\"view_count\" + ?) DESC"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(1)]);
}

#[test]
fn sqlite_sqlgen_can_render_order_by_unary_arithmetic_expr() {
    let order_value = query_ir::ValueExpr::UnaryArithmetic(query_ir::UnaryArithmeticExpr::new(
        query_ir::UnaryArithmeticOp::Plus,
        post_view_count_path_value(),
        schema_model::ScalarType::Int64,
    ));
    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" ORDER BY (+\"root\".\"view_count\") ASC"
    );

    assert_eq!(statement.bind_values(), &[]);
}

#[test]
fn sqlite_sqlgen_can_render_order_by_arithmetic_expr_with_joined_operand() {
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_author_score_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" ORDER BY (\"author\".\"score\" + ?) ASC"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(1)]);
}

#[test]
fn sqlite_sqlgen_preserves_filter_binds_before_order_binds() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Desc);

    let ir = query_ir::SelectQuery::new(
        post_type(),
        query_ir::ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        Some(filter),
        vec![order_by],
        None,
        None,
    );
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ? ORDER BY (\"root\".\"view_count\" + ?) DESC"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Hello".to_string()),
            SQLiteBindValue::Int64(1)
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_limit_and_offset() {
    let ir = post_query_with_limit_and_offset(10, 20);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" LIMIT 10 OFFSET 20"
    );
}
