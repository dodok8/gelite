use alloc::string::ToString;

use query_ir::{CompareExpr, CompareOp, DeleteQuery, Expr, Literal};

use super::fixtures::{post_author_name_path_value, post_title_path_value, post_type};
use crate::{SQLiteBindValue, render_delete};

#[test]
fn sqlite_sqlgen_can_render_unfiltered_delete() {
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), None));

    let statement = render_delete(&plan);

    assert_eq!(statement.sql(), "DELETE FROM \"post\"");
    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_filtered_delete() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    let statement = render_delete(&plan);

    assert_eq!(
        statement.sql(),
        "DELETE FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Draft".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_selects_joined_delete_targets_in_subquery() {
    let filter = Expr::Compare(CompareExpr::new(
        post_author_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let plan = sqlite_query_plan::plan_delete(&DeleteQuery::new(post_type(), Some(filter)));

    let statement = render_delete(&plan);

    assert_eq!(
        statement.sql(),
        "DELETE FROM \"post\" WHERE \"id\" IN (SELECT \"root\".\"id\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"name\" = ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}
