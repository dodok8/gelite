use super::fixtures::{
    empty_post_query, optional_post_author_shape_field,
    optional_post_author_with_best_friend_shape_field, optional_post_author_with_posts_shape_field,
    post_author_field, post_author_name_path_value, post_author_score_path_value,
    post_author_shape_field, post_author_shape_field_with_id_then_name,
    post_author_with_best_friend_shape_field, post_best_friend_field,
    post_best_friend_name_path_value, post_best_friend_shape_field,
    post_generated_join_name_path_value, post_id_path_value, post_id_shape_field,
    post_query_with_shape, post_title_field, post_title_path_value, post_title_shape_field,
    post_type, post_view_count_path_value, user_best_friend_score_path_value,
    user_best_friend_with_best_friend_shape_field, user_name_shape_field, user_posts_shape_field,
    user_posts_with_author_shape_field, user_posts_with_nested_posts_shape_field,
    user_query_with_shape, user_score_field, user_type,
};
use crate::{
    SQLiteArithmeticOp, SQLiteCastTarget, SQLiteCompareOp, SQLiteInExpr, SQLiteInOp, SQLiteInRhs,
    SQLiteJoinKind, SQLiteJoinReason, SQLiteLiteral, SQLiteOrder, SQLiteOrderDirection,
    SQLiteStringFunctionKind, SQLiteUnaryArithmeticOp, SQLiteValueExpr, SQLiteValueRole,
    SQLiteWhereExpr, plan_select,
};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use query_ir::{
    InRhs, Literal, ResolvedComputedField, ResolvedShape, ResolvedShapeField, ResolvedShapeItem,
    SelectQuery, UnaryArithmeticExpr, UnaryArithmeticOp,
};

fn assert_order_column(order: &SQLiteOrder, source_alias: &str, column_name: &str) {
    match order.value() {
        SQLiteValueExpr::Column(column) => {
            assert_eq!(column.source_alias(), source_alias);
            assert_eq!(column.column_name(), column_name);
        }
        SQLiteValueExpr::Literal(_) => panic!("order value should be a column"),
        SQLiteValueExpr::Arithmetic(_) => panic!("order value should be a column"),
        SQLiteValueExpr::UnaryArithmetic(_) => panic!("order value should be a column"),
        SQLiteValueExpr::Cast(_) => panic!("order value should be a column"),
        SQLiteValueExpr::StringFunction(_) => panic!("order value should be a column"),
    }
}

fn assert_column_value(value: &SQLiteValueExpr, source_alias: &str, column_name: &str) {
    match value {
        SQLiteValueExpr::Column(column) => {
            assert_eq!(column.source_alias(), source_alias);
            assert_eq!(column.column_name(), column_name);
        }
        SQLiteValueExpr::Literal(_) => panic!("value should be a column"),
        SQLiteValueExpr::Arithmetic(_) => panic!("value should be a column"),
        SQLiteValueExpr::UnaryArithmetic(_) => panic!("value should be a column"),
        SQLiteValueExpr::Cast(_) => panic!("value should be a column"),
        SQLiteValueExpr::StringFunction(_) => panic!("value should be a column"),
    }
}

fn assert_in_list(in_expr: &SQLiteInExpr) -> &[SQLiteValueExpr] {
    let SQLiteInRhs::List(values) = in_expr.right() else {
        panic!("membership right side should be a list");
    };

    values
}

fn assert_int_literal_value(value: &SQLiteValueExpr, expected: i64) {
    match value {
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => assert_eq!(*value, expected),
        SQLiteValueExpr::Literal(_) => panic!("value should be an int literal"),
        SQLiteValueExpr::Column(_) => panic!("value should be a literal"),
        SQLiteValueExpr::Arithmetic(_) => panic!("value should be a literal"),
        SQLiteValueExpr::UnaryArithmetic(_) => panic!("value should be a literal"),
        SQLiteValueExpr::Cast(_) => panic!("value should be a literal"),
        SQLiteValueExpr::StringFunction(_) => panic!("value should be a literal"),
    }
}

fn assert_unary_value(
    value: &SQLiteValueExpr,
    expected_op: SQLiteUnaryArithmeticOp,
) -> &SQLiteValueExpr {
    let SQLiteValueExpr::UnaryArithmetic(unary) = value else {
        panic!("value should be a unary arithmetic expression");
    };

    assert_eq!(unary.op(), expected_op);
    unary.operand()
}

fn assert_selected_field(
    value: &crate::SQLiteSelectValue,
    source_alias: &str,
    column_name: &str,
    output_name: Option<&str>,
    field_name: &str,
    role: SQLiteValueRole,
) {
    let field = value
        .as_field()
        .expect("selected value should be field-backed");
    assert_eq!(field.source_alias(), source_alias);
    assert_eq!(field.column_name(), column_name);
    assert_eq!(field.output_name(), output_name);
    assert_eq!(field.field().name(), field_name);
    assert_eq!(field.role(), role);
}

#[test]
fn sqlite_select_plan_can_store_root_source() {
    let ir = empty_post_query();

    let plan = plan_select(&ir);

    assert_eq!(plan.root_source().object_type().name(), "Post");
    assert_eq!(plan.root_source().table_name(), "post");
    assert_eq!(plan.root_source().alias(), "root");
    assert_eq!(plan.root_source().id_column(), "id");
}

#[test]
fn sqlite_select_plan_can_project_root_scalar_field() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_selected_field(
        &selected_values[0],
        "root",
        "title",
        Some("title"),
        "title",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_preserves_root_scalar_output_name() {
    let title = ResolvedShapeField::new(
        "headline",
        post_title_field(),
        schema_model::Cardinality::Required,
        None,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![title]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_selected_field(
        &selected_values[0],
        "root",
        "title",
        Some("headline"),
        "title",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_can_project_computed_value() {
    let computed = ResolvedComputedField::new(
        "score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            post_view_count_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::with_items(post_type(), vec![ResolvedShapeItem::Computed(computed)]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_eq!(selected_values[0].output_name(), Some("score"));
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Computed);
    let computed_value = selected_values[0]
        .as_computed()
        .expect("selected value should be computed");
    assert_eq!(computed_value.sql_alias(), "__gelite_value_0");
    assert_eq!(selected_values[0].source_alias(), None);
    assert_eq!(selected_values[0].column_name(), None);
    assert!(selected_values[0].field().is_none());

    let SQLiteValueExpr::Arithmetic(arithmetic) = selected_values[0].value() else {
        panic!("computed projection should lower to an arithmetic SQLite value expression");
    };
    assert_eq!(arithmetic.op(), SQLiteArithmeticOp::Add);
    assert_column_value(arithmetic.left(), "root", "view_count");
    assert_int_literal_value(arithmetic.right(), 1);
}

#[test]
fn sqlite_select_plan_can_project_computed_unary_arithmetic_value() {
    let computed = ResolvedComputedField::new(
        "neg_views",
        query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
            UnaryArithmeticOp::Minus,
            post_view_count_path_value(),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::with_items(post_type(), vec![ResolvedShapeItem::Computed(computed)]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_eq!(selected_values[0].output_name(), Some("neg_views"));
    assert_eq!(selected_values[0].role(), SQLiteValueRole::Computed);
    let computed_value = selected_values[0]
        .as_computed()
        .expect("selected value should be computed");
    assert_eq!(computed_value.sql_alias(), "__gelite_value_0");

    let operand = assert_unary_value(selected_values[0].value(), SQLiteUnaryArithmeticOp::Minus);
    assert_column_value(operand, "root", "view_count");
}

#[test]
fn sqlite_select_plan_uses_nested_source_alias_for_computed_value() {
    let computed = ResolvedComputedField::new(
        "boosted_score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            query_ir::ValueExpr::Path(
                query_ir::ResolvedPath::try_new(
                    user_type(),
                    vec![query_ir::ResolvedPathStep::scalar(
                        user_score_field(),
                        schema_model::Cardinality::Required,
                    )],
                )
                .expect("user score path should be valid"),
            ),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape =
        ResolvedShape::with_items(user_type(), vec![ResolvedShapeItem::Computed(computed)]);
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);
    assert_eq!(selected_values[1].output_name(), Some("boosted_score"));
    assert_eq!(
        selected_values[1]
            .as_computed()
            .expect("boosted_score should be computed")
            .sql_alias(),
        "__gelite_value_0"
    );

    let SQLiteValueExpr::Arithmetic(arithmetic) = selected_values[1].value() else {
        panic!("computed projection should lower to an arithmetic value expression");
    };
    assert_column_value(arithmetic.left(), "author", "score");
    assert_int_literal_value(arithmetic.right(), 1);
}

#[test]
fn sqlite_select_plan_avoids_alias_collision_for_nested_computed_path_join() {
    let computed = ResolvedComputedField::new(
        "friend_score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            user_best_friend_score_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape =
        ResolvedShape::with_items(user_type(), vec![ResolvedShapeItem::Computed(computed)]);
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_best_friend_shape_field(), author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 3);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_alias(), "best_friend");
    assert_eq!(joins[1].source_alias(), "root");
    assert_eq!(joins[1].target_alias(), "author");
    assert_eq!(joins[2].source_alias(), "author");
    assert_eq!(joins[2].target_alias(), "__gelite_join_0");

    let SQLiteValueExpr::Arithmetic(arithmetic) = plan.selected_values()[3].value() else {
        panic!("computed projection should lower to an arithmetic value expression");
    };
    assert_column_value(arithmetic.left(), "__gelite_join_0", "score");
}

#[test]
fn sqlite_select_plan_uses_planner_owned_alias_for_nested_computed_path_join() {
    let computed = ResolvedComputedField::new(
        "friend_score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            user_best_friend_score_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape =
        ResolvedShape::with_items(user_type(), vec![ResolvedShapeItem::Computed(computed)]);
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let root_best_friend = ResolvedShapeField::new(
        "author_best_friend",
        post_best_friend_field(),
        schema_model::Cardinality::Required,
        Some(ResolvedShape::new(
            user_type(),
            vec![user_name_shape_field()],
        )),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![root_best_friend, author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 3);
    assert_eq!(joins[0].target_alias(), "author_best_friend");
    assert_eq!(joins[1].target_alias(), "author");
    assert_eq!(joins[2].source_alias(), "author");
    assert_eq!(joins[2].target_alias(), "__gelite_join_0");

    let SQLiteValueExpr::Arithmetic(arithmetic) = plan.selected_values()[3].value() else {
        panic!("computed projection should lower to an arithmetic value expression");
    };
    assert_column_value(arithmetic.left(), "__gelite_join_0", "score");
}

#[test]
fn sqlite_select_plan_uses_left_descendant_join_under_optional_nested_source() {
    let computed = ResolvedComputedField::new(
        "friend_score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            user_best_friend_score_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Optional,
    );
    let author_shape =
        ResolvedShape::with_items(user_type(), vec![ResolvedShapeItem::Computed(computed)]);
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Optional,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 2);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Left);
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[1].source_alias(), "author");
    assert_eq!(joins[1].kind(), SQLiteJoinKind::Left);
    assert_eq!(joins[1].target_alias(), "__gelite_join_0");
}

#[test]
fn sqlite_select_plan_assigns_unique_sql_aliases_for_repeated_computed_output_names() {
    let root_computed = ResolvedComputedField::new(
        "score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            post_view_count_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let nested_computed = ResolvedComputedField::new(
        "score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            query_ir::ValueExpr::Path(
                query_ir::ResolvedPath::try_new(
                    user_type(),
                    vec![query_ir::ResolvedPathStep::scalar(
                        user_score_field(),
                        schema_model::Cardinality::Required,
                    )],
                )
                .expect("user score path should be valid"),
            ),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape = ResolvedShape::with_items(
        user_type(),
        vec![ResolvedShapeItem::Computed(nested_computed)],
    );
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::with_items(
            post_type(),
            vec![
                ResolvedShapeItem::Computed(root_computed),
                ResolvedShapeItem::Field(author),
            ],
        ),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 3);
    assert_eq!(selected_values[0].output_name(), Some("score"));
    assert_eq!(
        selected_values[0]
            .as_computed()
            .expect("root score should be computed")
            .sql_alias(),
        "__gelite_value_0"
    );
    assert_selected_field(
        &selected_values[1],
        "author",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_eq!(selected_values[2].output_name(), Some("score"));
    assert_eq!(
        selected_values[2]
            .as_computed()
            .expect("nested score should be computed")
            .sql_alias(),
        "__gelite_value_1"
    );

    let root_field = &plan.result_shape().fields()[0];
    assert_eq!(root_field.output_name(), "score");
    let root_value = root_field
        .value()
        .expect("root score should point to a selected value");
    assert_eq!(root_value.column_name(), "__gelite_value_0");

    let author_field = &plan.result_shape().fields()[1];
    let nested_shape = author_field
        .nested_shape()
        .expect("author should have nested result shape");
    let nested_field = &nested_shape.fields()[0];
    assert_eq!(nested_field.output_name(), "score");
    let nested_value = nested_field
        .value()
        .expect("nested score should point to a selected value");
    assert_eq!(nested_value.column_name(), "__gelite_value_1");
}

#[test]
fn sqlite_select_plan_reserves_computed_sql_aliases_against_selected_field_columns() {
    let reserved_field = ResolvedShapeField::new(
        "__gelite_value_0",
        schema_model::FieldRef::new(
            schema_model::FieldId::new(99),
            post_type(),
            "__gelite_value_0",
        ),
        schema_model::Cardinality::Required,
        None,
    );
    let computed = ResolvedComputedField::new(
        "score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            post_view_count_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::with_items(
            post_type(),
            vec![
                ResolvedShapeItem::Field(reserved_field),
                ResolvedShapeItem::Computed(computed),
            ],
        ),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);
    assert_selected_field(
        &selected_values[0],
        "root",
        "__gelite_value_0",
        Some("__gelite_value_0"),
        "__gelite_value_0",
        SQLiteValueRole::Scalar,
    );
    assert_eq!(
        selected_values[1]
            .as_computed()
            .expect("score should be computed")
            .sql_alias(),
        "__gelite_value_1"
    );

    let computed_field = &plan.result_shape().fields()[1];
    let computed_value = computed_field
        .value()
        .expect("computed field should point to a selected value");
    assert_eq!(computed_value.column_name(), "__gelite_value_1");
}

#[test]
fn sqlite_select_plan_reuses_alias_for_repeated_nested_deep_path_join() {
    let first_computed = ResolvedComputedField::new(
        "friend_score",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            user_best_friend_score_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let second_computed = ResolvedComputedField::new(
        "friend_score_plus",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            user_best_friend_score_path_value(),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(2)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape = ResolvedShape::with_items(
        user_type(),
        vec![
            ResolvedShapeItem::Computed(first_computed),
            ResolvedShapeItem::Computed(second_computed),
        ],
    );
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 2);
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[1].source_alias(), "author");
    assert_eq!(joins[1].target_alias(), "__gelite_join_0");

    let SQLiteValueExpr::Arithmetic(first_arithmetic) = plan.selected_values()[1].value() else {
        panic!("first computed projection should lower to arithmetic");
    };
    let SQLiteValueExpr::Arithmetic(second_arithmetic) = plan.selected_values()[2].value() else {
        panic!("second computed projection should lower to arithmetic");
    };
    assert_column_value(first_arithmetic.left(), "__gelite_join_0", "score");
    assert_column_value(second_arithmetic.left(), "__gelite_join_0", "score");
}

#[test]
fn sqlite_select_plan_keeps_nested_identity_and_computed_id_column_names_distinct() {
    let computed_id = ResolvedComputedField::new(
        "id",
        query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            query_ir::ValueExpr::Path(
                query_ir::ResolvedPath::try_new(
                    user_type(),
                    vec![query_ir::ResolvedPathStep::scalar(
                        user_score_field(),
                        schema_model::Cardinality::Required,
                    )],
                )
                .expect("user score path should be valid"),
            ),
            query_ir::ArithmeticOp::Add,
            query_ir::ValueExpr::Literal(Literal::Int64(1)),
            schema_model::ScalarType::Int64,
        )),
        schema_model::ScalarType::Int64,
        schema_model::Cardinality::Required,
    );
    let author_shape =
        ResolvedShape::with_items(user_type(), vec![ResolvedShapeItem::Computed(computed_id)]);
    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        Some(author_shape),
    );
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);
    assert_selected_field(
        &selected_values[0],
        "author",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_eq!(selected_values[1].output_name(), Some("id"));
    assert_eq!(
        selected_values[1]
            .as_computed()
            .expect("nested id should be computed")
            .sql_alias(),
        "__gelite_value_0"
    );

    let author_field = &plan.result_shape().fields()[0];
    let nested_shape = author_field
        .nested_shape()
        .expect("author should have nested result shape");
    let identity = nested_shape
        .identity_value()
        .expect("nested identity should point to selected id");
    assert_eq!(identity.column_name(), "id");

    let computed_field = &nested_shape.fields()[0];
    assert_eq!(computed_field.output_name(), "id");
    let computed_value = computed_field
        .value()
        .expect("computed id should point to a selected value");
    assert_eq!(computed_value.column_name(), "__gelite_value_0");
}

#[test]
fn sqlite_select_plan_preserves_root_scalar_projection_order() {
    let title = ResolvedShapeField::new(
        "title",
        post_title_field(),
        schema_model::Cardinality::Required,
        None,
    );

    let author = ResolvedShapeField::new(
        "author",
        post_author_field(),
        schema_model::Cardinality::Required,
        None,
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![title, author]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);

    assert_selected_field(
        &selected_values[0],
        "root",
        "title",
        Some("title"),
        "title",
        SQLiteValueRole::Scalar,
    );
    assert_selected_field(
        &selected_values[1],
        "root",
        "author",
        Some("author"),
        "author",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_can_apply_limit() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        Some(10),
        None,
    );

    let plan = plan_select(&ir);

    assert_eq!(plan.limit(), Some(10));
}

#[test]
fn sqlite_select_plan_can_apply_offset() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        Some(20),
    );

    let plan = plan_select(&ir);

    assert_eq!(plan.offset(), Some(20));
}

#[test]
fn sqlite_select_plan_can_order_by_root_scalar_field() {
    let order_by = query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_order_column(&order_by[0], "root", "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
}

#[test]
fn sqlite_select_plan_can_order_by_root_scalar_field_desc() {
    let order_by =
        query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Desc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_order_column(&order_by[0], "root", "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Desc);
}

#[test]
fn sqlite_select_plan_can_order_by_single_link_scalar_path() {
    let order_by =
        query_ir::OrderExpr::new(post_author_name_path_value(), query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_order_column(&order_by[0], "author", "name");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
}

#[test]
fn sqlite_select_plan_can_join_order_single_link_scalar_path() {
    let order_by =
        query_ir::OrderExpr::new(post_author_name_path_value(), query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");

    let on = joins[0].on();
    assert_eq!(on.left_alias(), "root");
    assert_eq!(on.left_column(), "author_id");
    assert_eq!(on.right_alias(), "author");
    assert_eq!(on.right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("order path join should be marked as path traversal")
        }
    }
}

#[test]
fn sqlite_select_plan_preserves_order_by_order() {
    let title_order =
        query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Asc);

    let id_order = query_ir::OrderExpr::new(post_id_path_value(), query_ir::OrderDirection::Desc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![title_order, id_order],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 2);
    assert_order_column(&order_by[0], "root", "title");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
    assert_order_column(&order_by[1], "root", "id");
    assert_eq!(order_by[1].direction(), SQLiteOrderDirection::Desc);
}

#[test]
fn sqlite_select_plan_can_order_by_numeric_arithmetic_expr() {
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));

    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Desc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Desc);

    let SQLiteValueExpr::Arithmetic(arithmetic) = order_by[0].value() else {
        panic!("order value should be an arithmetic expression");
    };

    assert_eq!(arithmetic.op(), SQLiteArithmeticOp::Add);
    assert_column_value(arithmetic.left(), "root", "view_count");
    assert_int_literal_value(arithmetic.right(), 1);
}

#[test]
fn sqlite_select_plan_can_order_by_unary_arithmetic_expr() {
    let order_value = query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
        UnaryArithmeticOp::Minus,
        post_view_count_path_value(),
        schema_model::ScalarType::Int64,
    ));

    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Desc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Desc);

    let operand = assert_unary_value(order_by[0].value(), SQLiteUnaryArithmeticOp::Minus);
    assert_column_value(operand, "root", "view_count");
}

#[test]
fn sqlite_select_plan_preserves_parenthesized_order_arithmetic_shape() {
    let inner = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        inner,
        query_ir::ArithmeticOp::Mul,
        query_ir::ValueExpr::Literal(Literal::Int64(10)),
        schema_model::ScalarType::Int64,
    ));

    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);

    let SQLiteValueExpr::Arithmetic(arithmetic) = order_by[0].value() else {
        panic!("order value should be an arithmetic expression");
    };

    assert_eq!(arithmetic.op(), SQLiteArithmeticOp::Mul);
    assert_int_literal_value(arithmetic.right(), 10);

    let SQLiteValueExpr::Arithmetic(inner) = arithmetic.left() else {
        panic!("left side should preserve the nested arithmetic expression");
    };

    assert_eq!(inner.op(), SQLiteArithmeticOp::Add);
    assert_column_value(inner.left(), "root", "view_count");
    assert_int_literal_value(inner.right(), 1);
}

#[test]
fn sqlite_select_plan_can_order_by_arithmetic_expr_through_single_link_path() {
    let order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_author_score_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));

    let order_by = query_ir::OrderExpr::new(order_value, query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);

    let SQLiteValueExpr::Arithmetic(arithmetic) = order_by[0].value() else {
        panic!("order value should be an arithmetic expression");
    };

    assert_eq!(arithmetic.op(), SQLiteArithmeticOp::Add);
    assert_column_value(arithmetic.left(), "author", "score");
    assert_int_literal_value(arithmetic.right(), 1);

    let joins = plan.joins();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[0].on().left_column(), "author_id");
    assert_eq!(joins[0].on().right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("order arithmetic join should be marked as path traversal")
        }
    }
}

#[test]
fn sqlite_select_plan_preserves_order_value_order_after_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    ));

    let first_order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let second_order_value = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Sub,
        query_ir::ValueExpr::Literal(Literal::Int64(2)),
        schema_model::ScalarType::Int64,
    ));

    let first_order = query_ir::OrderExpr::new(first_order_value, query_ir::OrderDirection::Desc);
    let second_order = query_ir::OrderExpr::new(second_order_value, query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        Some(filter),
        vec![first_order, second_order],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 2);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Desc);
    assert_eq!(order_by[1].direction(), SQLiteOrderDirection::Asc);

    let SQLiteValueExpr::Arithmetic(first) = order_by[0].value() else {
        panic!("first order value should be an arithmetic expression");
    };
    let SQLiteValueExpr::Arithmetic(second) = order_by[1].value() else {
        panic!("second order value should be an arithmetic expression");
    };

    assert_eq!(first.op(), SQLiteArithmeticOp::Add);
    assert_int_literal_value(first.right(), 1);
    assert_eq!(second.op(), SQLiteArithmeticOp::Sub);
    assert_int_literal_value(second.right(), 2);
}

#[test]
fn sqlite_select_plan_stores_path_order_as_value_expr() {
    let order_by = query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![post_title_shape_field()]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
    assert_order_column(&order_by[0], "root", "title");
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_equals_string_literal() {
    let filter = query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            match compare.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "root");
                    assert_eq!(column.column_name(), "title");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter left side should be a column")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter left side should be a column")
                }
            }

            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => {
                    assert_eq!(value, "hello");
                }
                SQLiteValueExpr::Literal(_) => {
                    panic!("filter right side should be a string literal")
                }
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter right side should be a literal")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter right side should be a literal")
                }
            }
        }
        _ => panic!("expected compare filter"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_arithmetic_expr_compared_to_int_literal() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_view_count_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(Literal::Int64(10)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };

    assert_eq!(compare.op(), SQLiteCompareOp::Gt);

    let SQLiteValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("filter left side should be an arithmetic expression");
    };

    assert_eq!(arithmetic.op(), SQLiteArithmeticOp::Add);

    match arithmetic.left() {
        SQLiteValueExpr::Column(column) => {
            assert_eq!(column.source_alias(), "root");
            assert_eq!(column.column_name(), "view_count");
        }
        _ => panic!("arithmetic left side should be a column"),
    }

    match arithmetic.right() {
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => assert_eq!(*value, 1),
        _ => panic!("arithmetic right side should be an int literal"),
    }

    match compare.right() {
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => assert_eq!(*value, 10),
        _ => panic!("comparison right side should be an int literal"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_cast_expr_compared_to_float_literal() {
    let cast = query_ir::ValueExpr::Cast(query_ir::CastExpr::new(
        post_view_count_path_value(),
        schema_model::ScalarType::Float64,
    ));
    let filter = query_ir::CompareExpr::new(
        cast,
        query_ir::CompareOp::Ge,
        query_ir::ValueExpr::Literal(Literal::Float64(10.5)),
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(query_ir::Expr::Compare(filter)),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };

    let SQLiteValueExpr::Cast(cast) = compare.left() else {
        panic!("filter left side should be a cast expression");
    };
    assert_eq!(cast.target(), SQLiteCastTarget::Float64);
    assert_column_value(cast.operand(), "root", "view_count");

    match compare.right() {
        SQLiteValueExpr::Literal(SQLiteLiteral::Float64(value)) => assert_eq!(*value, 10.5),
        _ => panic!("comparison right side should be a float literal"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_string_function_expr() {
    let concat = query_ir::ValueExpr::StringFunction(query_ir::StringFunctionExpr::new(
        query_ir::StringFunctionKind::Concat,
        vec![
            query_ir::StringFunctionArg::new(
                post_title_path_value(),
                schema_model::ScalarType::Str,
            ),
            query_ir::StringFunctionArg::new(
                query_ir::ValueExpr::Literal(Literal::String("!".to_string())),
                schema_model::ScalarType::Str,
            ),
        ],
        schema_model::Cardinality::Required,
    ));
    let filter = query_ir::CompareExpr::new(
        concat,
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Hello!".to_string())),
    );

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(query_ir::Expr::Compare(filter)),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };
    let SQLiteValueExpr::StringFunction(function) = compare.left() else {
        panic!("filter left side should be a string function");
    };

    assert_eq!(function.kind(), SQLiteStringFunctionKind::Concat);
    assert_eq!(function.args().len(), 2);
    assert_eq!(
        function.args()[0].scalar_type(),
        schema_model::ScalarType::Str
    );
    assert_column_value(function.args()[0].value(), "root", "title");
    assert_eq!(
        function.args()[1].scalar_type(),
        schema_model::ScalarType::Str
    );
    match function.args()[1].value() {
        SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => assert_eq!(value, "!"),
        _ => panic!("second concat argument should be a string literal"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_unary_arithmetic_expr_compared_to_int_literal() {
    let unary = query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
        UnaryArithmeticOp::Minus,
        post_view_count_path_value(),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::CompareExpr::new(
        unary,
        query_ir::CompareOp::Lt,
        query_ir::ValueExpr::Literal(Literal::Int64(0)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };

    assert_eq!(compare.op(), SQLiteCompareOp::Lt);
    let operand = assert_unary_value(compare.left(), SQLiteUnaryArithmeticOp::Minus);
    assert_column_value(operand, "root", "view_count");
    assert_int_literal_value(compare.right(), 0);
}

#[test]
fn sqlite_select_plan_collects_joins_from_arithmetic_expr_operands() {
    let arithmetic = query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
        post_author_score_path_value(),
        query_ir::ArithmeticOp::Add,
        query_ir::ValueExpr::Literal(Literal::Int64(1)),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::CompareExpr::new(
        arithmetic,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(Literal::Int64(10)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let joins = plan.joins();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[0].on().left_column(), "author_id");
    assert_eq!(joins[0].on().right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("arithmetic operand join should be marked as path traversal")
        }
    }

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };

    let SQLiteValueExpr::Arithmetic(arithmetic) = compare.left() else {
        panic!("filter left side should be an arithmetic expression");
    };

    match arithmetic.left() {
        SQLiteValueExpr::Column(column) => {
            assert_eq!(column.source_alias(), "author");
            assert_eq!(column.column_name(), "score");
        }
        _ => panic!("arithmetic left side should be a joined column"),
    }
}

#[test]
fn sqlite_select_plan_collects_joins_from_unary_arithmetic_expr_operand() {
    let unary = query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
        UnaryArithmeticOp::Minus,
        post_author_score_path_value(),
        schema_model::ScalarType::Int64,
    ));
    let filter = query_ir::CompareExpr::new(
        unary,
        query_ir::CompareOp::Gt,
        query_ir::ValueExpr::Literal(Literal::Int64(0)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let joins = plan.joins();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[0].on().left_column(), "author_id");
    assert_eq!(joins[0].on().right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("unary arithmetic operand join should be marked as path traversal")
        }
    }

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };

    let operand = assert_unary_value(compare.left(), SQLiteUnaryArithmeticOp::Minus);
    assert_column_value(operand, "author", "score");
}

#[test]
fn sqlite_select_plan_can_filter_single_link_scalar_path_equals_string_literal() {
    let filter = query_ir::CompareExpr::new(
        post_author_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => match compare.left() {
            SQLiteValueExpr::Column(column) => {
                assert_eq!(column.source_alias(), "author");
                assert_eq!(column.column_name(), "name");
            }
            SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
            SQLiteValueExpr::Arithmetic(_) => panic!("filter left side should be a column"),
            SQLiteValueExpr::UnaryArithmetic(_) => panic!("filter left side should be a column"),
            SQLiteValueExpr::Cast(_) => panic!("filter left side should be a column"),
            SQLiteValueExpr::StringFunction(_) => panic!("filter left side should be a column"),
        },
        _ => panic!("expected compare filter"),
    }
}

#[test]
fn sqlite_select_plan_can_join_filter_single_link_scalar_path() {
    let filter = query_ir::CompareExpr::new(
        post_author_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);

    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[0].on().left_column(), "author_id");
    assert_eq!(joins[0].on().right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("filter path join should be marked as path traversal")
        }
    }
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_equals_int_literal() {
    let filter = query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::Int64(42)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => {
                    assert_eq!(*value, 42);
                }
                SQLiteValueExpr::Literal(_) => panic!("filter right side should be an int literal"),
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter right side should be a literal")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter right side should be a literal")
                }
            }
        }
        _ => panic!("expected compare filter"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_equals_bool_literal() {
    let filter = query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::Bool(true)),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(SQLiteLiteral::Bool(value)) => {
                    assert!(*value);
                }
                SQLiteValueExpr::Literal(_) => panic!("filter right side should be a bool literal"),
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter right side should be a literal")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter right side should be a literal")
                }
            }
        }
        _ => panic!("expected compare filter"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_in_literal_list() {
    let expr = query_ir::Expr::In(query_ir::InExpr::new(
        post_title_path_value(),
        query_ir::InOp::In,
        InRhs::List(vec![
            query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
            query_ir::ValueExpr::Literal(Literal::String("Published".to_string())),
        ]),
    ));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::In(in_expr)) => {
            match in_expr.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "root");
                    assert_eq!(column.column_name(), "title");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter left side should be a column")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter left side should be a column")
                }
            }

            assert_eq!(in_expr.op(), SQLiteInOp::In);
            assert_eq!(
                assert_in_list(in_expr),
                &[
                    SQLiteValueExpr::Literal(SQLiteLiteral::String("Draft".to_string())),
                    SQLiteValueExpr::Literal(SQLiteLiteral::String("Published".to_string()))
                ]
            );
        }
        _ => panic!("expected in filter"),
    }
}

#[test]
fn sqlite_select_plan_preserves_membership_select_as_nested_plan() {
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_title_path_value(),
        query_ir::InOp::In,
        InRhs::Select(Box::new(user_query_with_shape(vec![
            user_name_shape_field(),
        ]))),
    ));
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let Some(SQLiteWhereExpr::In(in_expr)) = plan.filter() else {
        panic!("expected in filter");
    };
    let SQLiteInRhs::Select(select) = in_expr.right() else {
        panic!("membership right side should be a select");
    };

    assert_column_value(in_expr.left(), "root", "title");
    assert_eq!(select.root_source().table_name(), "user");
    assert_eq!(select.root_source().alias(), "root");
    assert_eq!(select.selected_values().len(), 1);
    assert_eq!(select.selected_values()[0].source_alias(), Some("root"));
    assert_eq!(select.selected_values()[0].column_name(), Some("name"));
}

#[test]
fn sqlite_select_plan_can_filter_path_in_unary_arithmetic_literal_list() {
    let expr = query_ir::Expr::In(query_ir::InExpr::new(
        post_view_count_path_value(),
        query_ir::InOp::In,
        InRhs::List(vec![
            query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
                UnaryArithmeticOp::Minus,
                query_ir::ValueExpr::Literal(Literal::Int64(1)),
                schema_model::ScalarType::Int64,
            )),
            query_ir::ValueExpr::UnaryArithmetic(UnaryArithmeticExpr::new(
                UnaryArithmeticOp::Plus,
                query_ir::ValueExpr::Literal(Literal::Int64(2)),
                schema_model::ScalarType::Int64,
            )),
        ]),
    ));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::In(in_expr)) = plan.filter() else {
        panic!("expected in filter");
    };

    assert_column_value(in_expr.left(), "root", "view_count");
    assert_eq!(in_expr.op(), SQLiteInOp::In);
    let right = assert_in_list(in_expr);
    assert_eq!(right.len(), 2);

    let operand = assert_unary_value(&right[0], SQLiteUnaryArithmeticOp::Minus);
    assert_int_literal_value(operand, 1);

    let operand = assert_unary_value(&right[1], SQLiteUnaryArithmeticOp::Plus);
    assert_int_literal_value(operand, 2);
}

#[test]
fn sqlite_select_plan_can_filter_single_link_scalar_path_not_in_literal_list() {
    let expr = query_ir::Expr::In(query_ir::InExpr::new(
        post_author_name_path_value(),
        query_ir::InOp::NotIn,
        InRhs::List(vec![query_ir::ValueExpr::Literal(Literal::String(
            "Sheri".to_string(),
        ))]),
    ));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::In(in_expr)) => {
            match in_expr.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "author");
                    assert_eq!(column.column_name(), "name");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter left side should be a column")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter left side should be a column")
                }
            }

            assert_eq!(in_expr.op(), SQLiteInOp::NotIn);
            assert_eq!(
                assert_in_list(in_expr),
                &[SQLiteValueExpr::Literal(SQLiteLiteral::String(
                    "Sheri".to_string()
                ))]
            );
        }
        _ => panic!("expected not in filter"),
    }

    assert_eq!(plan.joins().len(), 1);
    assert_eq!(plan.joins()[0].target_alias(), "author");
}

#[test]
fn sqlite_select_plan_can_filter_root_scalar_field_is_null() {
    let expr = query_ir::Expr::IsNull(post_title_path_value());

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::IsNull(value)) => match value {
            SQLiteValueExpr::Column(column) => {
                assert_eq!(column.source_alias(), "root");
                assert_eq!(column.column_name(), "title");
            }
            SQLiteValueExpr::Literal(_) => panic!("is null value should be a column"),
            SQLiteValueExpr::Arithmetic(_) => panic!("is null value should be a column"),
            SQLiteValueExpr::UnaryArithmetic(_) => panic!("is null value should be a column"),
            SQLiteValueExpr::Cast(_) => panic!("is null value should be a column"),
            SQLiteValueExpr::StringFunction(_) => panic!("is null value should be a column"),
        },
        _ => panic!("expected is null filter"),
    }
}

#[test]
fn sqlite_select_plan_can_filter_and_expression() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    ));
    let right = query_ir::Expr::IsNull(post_id_path_value());
    let expr = query_ir::Expr::And(Box::new(left), Box::new(right));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::And(left, right)) = plan.filter() else {
        panic!("expected and filter");
    };

    assert!(matches!(left.as_ref(), SQLiteWhereExpr::Compare(_)));
    assert!(matches!(right.as_ref(), SQLiteWhereExpr::IsNull(_)));
}

#[test]
fn sqlite_select_plan_can_filter_or_expression() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    ));
    let right = query_ir::Expr::IsNull(post_id_path_value());
    let expr = query_ir::Expr::Or(Box::new(left), Box::new(right));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Or(left, right)) = plan.filter() else {
        panic!("expected or filter");
    };

    assert!(matches!(left.as_ref(), SQLiteWhereExpr::Compare(_)));
    assert!(matches!(right.as_ref(), SQLiteWhereExpr::IsNull(_)));
}

#[test]
fn sqlite_select_plan_can_filter_not_expression() {
    let inner = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    ));
    let expr = query_ir::Expr::Not(Box::new(inner));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    let Some(SQLiteWhereExpr::Not(inner)) = plan.filter() else {
        panic!("expected not filter");
    };

    assert!(matches!(inner.as_ref(), SQLiteWhereExpr::Compare(_)));
}

#[test]
fn sqlite_select_plan_can_join_filter_boolean_expression_paths() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_author_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let right = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    ));
    let expr = query_ir::Expr::And(Box::new(left), Box::new(right));

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_alias(), "author");

    match joins[0].reason() {
        SQLiteJoinReason::PathTraversal { path } => {
            assert_eq!(path, &vec!["author".to_string()]);
        }
        SQLiteJoinReason::SelectedSingleLink { .. } => {
            panic!("boolean filter path join should be marked as path traversal")
        }
    }
}

#[test]
fn sqlite_select_plan_preserves_absent_filter() {
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);

    assert!(plan.filter().is_none());
}

#[test]
fn sqlite_select_plan_can_filter_implicit_id_equals_string_literal() {
    let filter = query_ir::CompareExpr::new(
        post_id_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("hello".to_string())),
    );

    let expr = query_ir::Expr::Compare(filter);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        Some(expr),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let filter = plan.filter();

    match filter {
        Some(SQLiteWhereExpr::Compare(compare)) => {
            match compare.left() {
                SQLiteValueExpr::Column(column) => {
                    assert_eq!(column.source_alias(), "root");
                    assert_eq!(column.column_name(), "id");
                }
                SQLiteValueExpr::Literal(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter left side should be a column")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter left side should be a column"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter left side should be a column")
                }
            }

            assert_eq!(compare.op(), SQLiteCompareOp::Eq);

            match compare.right() {
                SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => {
                    assert_eq!(value, "hello");
                }
                SQLiteValueExpr::Literal(_) => {
                    panic!("filter right side should be a string literal")
                }
                SQLiteValueExpr::Column(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::Arithmetic(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::UnaryArithmetic(_) => {
                    panic!("filter right side should be a literal")
                }
                SQLiteValueExpr::Cast(_) => panic!("filter right side should be a literal"),
                SQLiteValueExpr::StringFunction(_) => {
                    panic!("filter right side should be a literal")
                }
            }
        }
        _ => panic!("expected compare filter"),
    }
}

#[test]
fn sqlite_select_plan_can_order_by_implicit_id() {
    let order_by = query_ir::OrderExpr::new(post_id_path_value(), query_ir::OrderDirection::Asc);

    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(post_type(), vec![]),
        None,
        vec![order_by],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let order_by = plan.order_by();

    assert_eq!(order_by.len(), 1);
    assert_order_column(&order_by[0], "root", "id");
    assert_eq!(order_by[0].direction(), SQLiteOrderDirection::Asc);
}

#[test]
fn sqlite_select_plan_can_project_implicit_id() {
    let ir = post_query_with_shape(vec![post_id_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 1);
    assert_selected_field(
        &selected_values[0],
        "root",
        "id",
        Some("id"),
        "id",
        SQLiteValueRole::ObjectId,
    );
}

#[test]
fn sqlite_select_plan_can_join_selected_single_link() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");

    let on = joins[0].on();

    assert_eq!(on.left_alias(), "root");
    assert_eq!(on.left_column(), "author_id");
    assert_eq!(on.right_alias(), "author");
    assert_eq!(on.right_column(), "id");

    match joins[0].reason() {
        SQLiteJoinReason::SelectedSingleLink { field } => {
            assert_eq!(field.name(), "author");
        }
        SQLiteJoinReason::PathTraversal { .. } => {
            panic!("selected link join should be marked as selected single link")
        }
    }
}

#[test]
fn sqlite_select_plan_can_join_nested_selected_single_link() {
    let ir = post_query_with_shape(vec![post_author_with_best_friend_shape_field()]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 2);

    assert_eq!(joins[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_table(), "user");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[0].on().left_alias(), "root");
    assert_eq!(joins[0].on().left_column(), "author_id");
    assert_eq!(joins[0].on().right_alias(), "author");

    assert_eq!(joins[1].kind(), SQLiteJoinKind::Inner);
    assert_eq!(joins[1].source_alias(), "author");
    assert_eq!(joins[1].target_table(), "user");
    assert_eq!(joins[1].target_alias(), "best_friend");
    assert_eq!(joins[1].on().left_alias(), "author");
    assert_eq!(joins[1].on().left_column(), "best_friend_id");
    assert_eq!(joins[1].on().right_alias(), "best_friend");

    match joins[1].reason() {
        SQLiteJoinReason::SelectedSingleLink { field } => {
            assert_eq!(field.name(), "best_friend");
        }
        SQLiteJoinReason::PathTraversal { .. } => {
            panic!("nested selected link join should be marked as selected single link")
        }
    }
}

#[test]
fn sqlite_select_plan_uses_unique_aliases_for_repeated_nested_selected_link_names() {
    let ir = user_query_with_shape(vec![user_best_friend_with_best_friend_shape_field()]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 2);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_alias(), "best_friend");
    assert_eq!(joins[1].source_alias(), "best_friend");
    assert_ne!(joins[1].target_alias(), "best_friend");
    assert_ne!(joins[1].target_alias(), joins[0].target_alias());
    assert_eq!(joins[1].on().left_alias(), "best_friend");
    assert_eq!(joins[1].on().right_alias(), joins[1].target_alias());

    assert_selected_field(
        &plan.selected_values()[0],
        "best_friend",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &plan.selected_values()[1],
        joins[1].target_alias(),
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &plan.selected_values()[2],
        joins[1].target_alias(),
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );

    let first_friend = &plan.result_shape().fields()[0];
    let first_friend_shape = first_friend
        .nested_shape()
        .expect("first best_friend should have nested result shape");
    let second_friend = &first_friend_shape.fields()[0];
    let second_friend_shape = second_friend
        .nested_shape()
        .expect("second best_friend should have nested result shape");
    let second_friend_identity = second_friend_shape
        .identity_value()
        .expect("second best_friend shape should have identity value");
    let second_friend_name = second_friend_shape.fields()[0]
        .value()
        .expect("second best_friend name should point to a selected value");

    assert_eq!(
        second_friend_identity.source_alias(),
        joins[1].target_alias()
    );
    assert_eq!(second_friend_name.source_alias(), joins[1].target_alias());
}

#[test]
fn sqlite_select_plan_avoids_root_path_alias_collision_with_nested_selected_link() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_best_friend_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Carol".to_string())),
    ));
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(
            post_type(),
            vec![post_author_with_best_friend_shape_field()],
        ),
        Some(filter),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 3);
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[1].source_alias(), "author");
    assert_ne!(joins[1].target_alias(), "best_friend");
    assert_eq!(joins[2].source_alias(), "root");
    assert_eq!(joins[2].target_alias(), "best_friend");

    assert_selected_field(
        &plan.selected_values()[3],
        joins[1].target_alias(),
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("filter should be a compare expression");
    };
    assert_column_value(compare.left(), "best_friend", "name");
}

#[test]
fn sqlite_select_plan_reserves_root_selected_aliases_before_nested_selected_links() {
    let ir = post_query_with_shape(vec![
        post_author_with_best_friend_shape_field(),
        post_best_friend_shape_field(),
    ]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 3);
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[1].source_alias(), "author");
    assert_ne!(joins[1].target_alias(), "best_friend");
    assert_eq!(joins[2].source_alias(), "root");
    assert_eq!(joins[2].target_alias(), "best_friend");

    assert_selected_field(
        &plan.selected_values()[3],
        joins[1].target_alias(),
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );
    assert_selected_field(
        &plan.selected_values()[5],
        "best_friend",
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_reserves_root_path_aliases_for_generated_join_aliases() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_generated_join_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Carol".to_string())),
    ));
    let ir = SelectQuery::new(
        post_type(),
        ResolvedShape::new(
            post_type(),
            vec![
                post_best_friend_shape_field(),
                post_author_with_best_friend_shape_field(),
            ],
        ),
        Some(filter),
        vec![],
        None,
        None,
    );

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 4);
    assert_eq!(joins[0].target_alias(), "best_friend");
    assert_eq!(joins[1].target_alias(), "author");
    assert_eq!(joins[2].source_alias(), "author");
    assert_ne!(joins[2].target_alias(), "__gelite_join_0");
    assert_eq!(joins[3].source_alias(), "root");
    assert_eq!(joins[3].target_alias(), "__gelite_join_0");

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("filter should be a compare expression");
    };
    assert_column_value(compare.left(), "__gelite_join_0", "name");
}

#[test]
fn sqlite_select_plan_uses_left_join_for_nested_selected_link_under_optional_source() {
    let ir = post_query_with_shape(vec![optional_post_author_with_best_friend_shape_field()]);

    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 2);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Left);
    assert_eq!(joins[0].source_alias(), "root");
    assert_eq!(joins[0].target_alias(), "author");
    assert_eq!(joins[1].kind(), SQLiteJoinKind::Left);
    assert_eq!(joins[1].source_alias(), "author");
    assert_eq!(joins[1].target_alias(), "best_friend");
}

#[test]
fn sqlite_select_plan_plans_multi_link_under_optional_source_as_follow_up() {
    let ir = post_query_with_shape(vec![optional_post_author_with_posts_shape_field()]);

    let plan = plan_select(&ir);

    assert_eq!(plan.joins().len(), 1);
    assert_eq!(plan.joins()[0].kind(), SQLiteJoinKind::Left);

    let fetch = &plan.follow_up_fetches()[0];
    assert_eq!(fetch.parent_identity().source_alias(), "author");
    assert_eq!(fetch.parent_identity().column_name(), "id");
    assert_eq!(fetch.parent_field().name(), "posts");
    assert_eq!(fetch.join_table_name(), Some("user__posts"));
}

#[test]
fn sqlite_select_plan_plans_root_multi_link_follow_up() {
    let ir = user_query_with_shape(vec![user_posts_shape_field()]);

    let plan = plan_select(&ir);

    assert!(plan.joins().is_empty());
    assert_eq!(plan.selected_values().len(), 1);
    assert_selected_field(
        &plan.selected_values()[0],
        "root",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );

    let identity = plan
        .result_shape()
        .identity_value()
        .expect("root shape should retain parent identity");
    assert_eq!(identity.source_alias(), "root");
    assert_eq!(identity.column_name(), "id");

    let fetch = &plan.follow_up_fetches()[0];
    assert_eq!(fetch.parent_field().name(), "posts");
    assert_eq!(fetch.parent_identity().source_alias(), "root");
    assert_eq!(fetch.parent_identity().column_name(), "id");
    assert_eq!(fetch.join_table_name(), Some("user__posts"));
    assert_eq!(fetch.source_column(), "source_id");
    assert_eq!(fetch.target_column(), "target_id");
    assert_eq!(fetch.target_source().object_type().name(), "Post");
    assert_eq!(fetch.target_source().table_name(), "post");
    assert_eq!(fetch.target_source().alias(), "root");

    assert_eq!(fetch.selected_values().len(), 2);
    assert_selected_field(
        &fetch.selected_values()[0],
        "root",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &fetch.selected_values()[1],
        "root",
        "title",
        Some("title"),
        "title",
        SQLiteValueRole::Scalar,
    );
    assert!(fetch.joins().is_empty());
    assert!(fetch.follow_up_fetches().is_empty());
}

#[test]
fn sqlite_select_plan_preserves_multi_link_output_position() {
    let score = ResolvedShapeField::new(
        "score",
        user_score_field(),
        schema_model::Cardinality::Required,
        None,
    );
    let ir = user_query_with_shape(vec![
        user_name_shape_field(),
        user_posts_shape_field(),
        score,
    ]);

    let plan = plan_select(&ir);
    let fields = plan.result_shape().fields();

    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].output_name(), "name");
    assert_eq!(fields[1].output_name(), "posts");
    assert_eq!(fields[1].cardinality(), schema_model::Cardinality::Many);
    assert_eq!(fields[1].follow_up_fetch_index(), Some(0));
    assert_eq!(fields[2].output_name(), "score");
}

#[test]
fn sqlite_select_plan_aligns_sibling_multi_link_fetch_indexes() {
    let featured_posts = ResolvedShapeField::new(
        "featured_posts",
        schema_model::FieldRef::new(schema_model::FieldId::new(6), user_type(), "featured_posts"),
        schema_model::Cardinality::Many,
        Some(ResolvedShape::new(
            post_type(),
            vec![post_title_shape_field()],
        )),
    );
    let ir = user_query_with_shape(vec![user_posts_shape_field(), featured_posts]);

    let plan = plan_select(&ir);
    let fields = plan.result_shape().fields();

    assert_eq!(plan.follow_up_fetches().len(), 2);
    assert_eq!(fields[0].follow_up_fetch_index(), Some(0));
    assert_eq!(fields[1].follow_up_fetch_index(), Some(1));
    assert_eq!(
        plan.follow_up_fetches()[0].join_table_name(),
        Some("user__posts")
    );
    assert_eq!(
        plan.follow_up_fetches()[1].join_table_name(),
        Some("user__featured_posts")
    );
}

#[test]
fn sqlite_multi_link_follow_up_reuses_single_link_shape_planning() {
    let ir = user_query_with_shape(vec![user_posts_with_author_shape_field()]);

    let plan = plan_select(&ir);
    let fetch = &plan.follow_up_fetches()[0];

    assert_eq!(fetch.joins().len(), 1);
    assert_eq!(fetch.joins()[0].source_alias(), "root");
    assert_eq!(fetch.joins()[0].target_table(), "user");
    assert_eq!(fetch.joins()[0].target_alias(), "author");
    assert_eq!(fetch.selected_values().len(), 4);

    let author = &fetch.result_shape().fields()[1];
    assert_eq!(author.output_name(), "author");
    assert_eq!(author.cardinality(), schema_model::Cardinality::Required);
    assert!(author.nested_shape().is_some());
}

#[test]
fn sqlite_multi_link_follow_up_plans_nested_multi_link_recursively() {
    let ir = user_query_with_shape(vec![user_posts_with_nested_posts_shape_field()]);

    let plan = plan_select(&ir);
    let posts = &plan.follow_up_fetches()[0];

    assert_eq!(posts.follow_up_fetches().len(), 1);
    let nested_posts = &posts.follow_up_fetches()[0];
    assert_eq!(nested_posts.parent_identity().source_alias(), "author");
    assert_eq!(nested_posts.parent_identity().column_name(), "id");
    assert_eq!(nested_posts.join_table_name(), Some("user__posts"));
    assert_eq!(nested_posts.target_source().object_type().name(), "Post");

    let author = &posts.result_shape().fields()[0];
    let author_shape = author
        .nested_shape()
        .expect("author should retain its result shape");
    assert_eq!(author_shape.fields()[0].output_name(), "posts");
    assert_eq!(author_shape.fields()[0].follow_up_fetch_index(), Some(0));
}

#[test]
fn sqlite_select_plan_can_project_selected_single_link_scalar_field() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_selected_field(
        &selected_values[1],
        "author",
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_projects_selected_single_link_identity() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 2);

    assert_selected_field(
        &selected_values[0],
        "author",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &selected_values[1],
        "author",
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_preserves_selected_value_order_with_nested_link() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);

    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 3);

    assert_selected_field(
        &selected_values[0],
        "root",
        "title",
        Some("title"),
        "title",
        SQLiteValueRole::Scalar,
    );
    assert_selected_field(
        &selected_values[1],
        "author",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &selected_values[2],
        "author",
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );
}

#[test]
fn sqlite_select_plan_can_build_result_shape_for_root_scalar_fields() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);
    let plan = plan_select(&ir);
    let result_shape = plan.result_shape();

    let fields = result_shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);

    let value = fields[0]
        .value()
        .expect("title should point to a selected value");

    assert_eq!(value.source_alias(), "root");
    assert_eq!(value.column_name(), "title");
    assert_eq!(value.role(), SQLiteValueRole::Scalar);

    assert!(fields[0].nested_shape().is_none());
}

#[test]
fn sqlite_select_plan_can_build_result_shape_for_selected_single_link() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);
    let result_shape = plan.result_shape();

    let fields = result_shape.fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);
    assert!(fields[0].value().is_none());

    let nested_shape = fields[0]
        .nested_shape()
        .expect("author should have nested result shape");

    let nested_fields = nested_shape.fields();

    assert_eq!(nested_fields.len(), 1);
    assert_eq!(nested_fields[0].output_name(), "name");
    assert_eq!(
        nested_fields[0].cardinality(),
        schema_model::Cardinality::Required
    );

    let value = nested_fields[0]
        .value()
        .expect("name should point to a selected value");

    assert_eq!(value.source_alias(), "author");
    assert_eq!(value.column_name(), "name");
    assert_eq!(value.role(), SQLiteValueRole::Scalar);
    assert!(nested_fields[0].nested_shape().is_none());
}

#[test]
fn sqlite_result_shape_for_selected_single_link_has_identity_value() {
    let ir = post_query_with_shape(vec![post_author_shape_field()]);
    let plan = plan_select(&ir);

    let author = &plan.result_shape().fields()[0];
    let nested_shape = author
        .nested_shape()
        .expect("author should have nested result shape");

    let identity = nested_shape
        .identity_value()
        .expect("nested shape should have identity value");

    assert_eq!(identity.source_alias(), "author");
    assert_eq!(identity.column_name(), "id");
    assert_eq!(identity.role(), SQLiteValueRole::ObjectId);
}

#[test]
fn sqlite_select_plan_preserves_result_shape_field_order() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);
    let plan = plan_select(&ir);
    let fields = plan.result_shape().fields();

    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[1].output_name(), "author");
}

#[test]
fn sqlite_select_plan_can_join_optional_selected_single_link() {
    let ir = post_query_with_shape(vec![optional_post_author_shape_field()]);
    let plan = plan_select(&ir);
    let joins = plan.joins();

    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].kind(), SQLiteJoinKind::Left);
    assert_eq!(joins[0].target_alias(), "author");
}

#[test]
fn sqlite_select_plan_preserves_nested_result_shape_field_order() {
    let ir = post_query_with_shape(vec![post_author_shape_field_with_id_then_name()]);
    let plan = plan_select(&ir);
    let selected_values = plan.selected_values();

    assert_eq!(selected_values.len(), 3);
    assert_selected_field(
        &selected_values[0],
        "author",
        "id",
        None,
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &selected_values[1],
        "author",
        "id",
        Some("id"),
        "id",
        SQLiteValueRole::ObjectId,
    );
    assert_selected_field(
        &selected_values[2],
        "author",
        "name",
        Some("name"),
        "name",
        SQLiteValueRole::Scalar,
    );

    let author = &plan.result_shape().fields()[0];
    let nested_shape = author
        .nested_shape()
        .expect("author should have nested result shape");
    let nested_fields = nested_shape.fields();

    assert_eq!(nested_fields.len(), 2);
    assert_eq!(nested_fields[0].output_name(), "id");
    assert_eq!(nested_fields[1].output_name(), "name");
}

#[test]
fn inverse_follow_up_uses_stored_relation_in_reverse() {
    for cardinality in [
        schema_model::Cardinality::Optional,
        schema_model::Cardinality::Many,
    ] {
        let field = user_posts_shape_field().with_link_traversal(query_ir::LinkTraversal::new(
            post_author_field(),
            cardinality,
            query_ir::LinkDirection::Inverse,
        ));
        let plan = plan_select(&user_query_with_shape(vec![field]));
        let fetch = &plan.follow_up_fetches()[0];
        assert_eq!(
            fetch.source_column(),
            if cardinality == schema_model::Cardinality::Many {
                "target_id"
            } else {
                "author_id"
            }
        );
        assert_eq!(
            fetch.target_column(),
            if cardinality == schema_model::Cardinality::Many {
                "source_id"
            } else {
                "id"
            }
        );
    }
}
