use alloc::string::ToString;
use alloc::vec;

use query_ir::{Assignment, AssignmentValue, CompareExpr, CompareOp, Expr, Literal, UpdateQuery};

use super::fixtures::{
    post_author_name_path_value, post_author_select_assignment, post_title_field,
    post_title_path_value, post_type,
};
use crate::{SQLiteBindValue, render_update};

#[test]
fn sqlite_sqlgen_can_render_unfiltered_update() {
    let plan = sqlite_query_plan::plan_update(&update_query(None));

    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "UPDATE \"post\" AS \"root\" SET \"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Closed Case".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_renders_assignment_binds_before_filter_binds() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let plan = sqlite_query_plan::plan_update(&update_query(Some(filter)));

    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "UPDATE \"post\" AS \"root\" SET \"title\" = ? WHERE \"root\".\"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Closed Case".to_string()),
            SQLiteBindValue::String("Draft".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_update_link_select_before_filter_binds() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let plan = sqlite_query_plan::plan_update(&UpdateQuery::new(
        post_type(),
        Some(filter),
        vec![post_author_select_assignment()],
    ));

    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "UPDATE \"post\" AS \"root\" SET \"author_id\" = (SELECT \"root\".\"id\" FROM \"user\" AS \"root\" WHERE \"root\".\"id\" = ?) WHERE \"root\".\"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("user-1".to_string()),
            SQLiteBindValue::String("Draft".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_selects_joined_filter_targets_in_subquery() {
    let filter = Expr::Compare(CompareExpr::new(
        post_author_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let plan = sqlite_query_plan::plan_update(&update_query(Some(filter)));

    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "UPDATE \"post\" SET \"title\" = ? WHERE \"id\" IN (SELECT \"root\".\"id\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"name\" = ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Closed Case".to_string()),
            SQLiteBindValue::String("Sheri".to_string()),
        ]
    );
}

fn update_query(filter: Option<Expr>) -> UpdateQuery {
    UpdateQuery::new(
        post_type(),
        filter,
        vec![Assignment::new(
            post_title_field(),
            AssignmentValue::Scalar(Literal::String("Closed Case".to_string())),
        )],
    )
}
