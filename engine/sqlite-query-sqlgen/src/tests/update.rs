use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;

use query_ir::{
    Assignment, AssignmentOperator, AssignmentValue, CompareExpr, CompareOp, Expr, Literal,
    ResolvedPath, ResolvedPathStep, ResolvedShape, ResolvedShapeField, SelectQuery, UpdateQuery,
    ValueExpr,
};
use schema_model::{Cardinality, FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

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

#[test]
fn sqlite_sqlgen_renders_batched_multi_link_add_with_conflict_noop() {
    let plan = sqlite_query_plan::plan_update(&multi_link_update(AssignmentOperator::Add));
    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"user__posts\" (\"source_id\", \"target_id\") SELECT \"source\".\"id\", \"target\".\"id\" FROM (SELECT \"root\".\"id\" FROM \"user\" AS \"root\" WHERE \"root\".\"name\" = ?) AS \"source\" CROSS JOIN (SELECT \"root\".\"id\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?) AS \"target\" WHERE true ON CONFLICT (\"source_id\", \"target_id\") DO NOTHING"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Sheri".to_string()),
            SQLiteBindValue::String("Case File".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_renders_batched_multi_link_remove() {
    let plan = sqlite_query_plan::plan_update(&multi_link_update(AssignmentOperator::Remove));
    let statement = render_update(&plan);

    assert_eq!(
        statement.sql(),
        "DELETE FROM \"user__posts\" WHERE \"source_id\" IN (SELECT \"root\".\"id\" FROM \"user\" AS \"root\" WHERE \"root\".\"name\" = ?) AND \"target_id\" IN (SELECT \"root\".\"id\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" = ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Sheri".to_string()),
            SQLiteBindValue::String("Case File".to_string()),
        ]
    );
}

fn multi_link_update(operator: AssignmentOperator) -> UpdateQuery {
    let user = ObjectTypeRef::new(ObjectTypeId::new(10), "User");
    let user_name = FieldRef::new(FieldId::new(12), user.clone(), "name");
    let posts = FieldRef::new(FieldId::new(11), user.clone(), "posts");
    let source_filter = Expr::Compare(CompareExpr::new(
        ValueExpr::Path(
            ResolvedPath::try_new(
                user.clone(),
                vec![ResolvedPathStep::scalar(user_name, Cardinality::Required)],
            )
            .expect("user name path should be valid"),
        ),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let post_id = super::fixtures::post_id_field();
    let target_filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        ValueExpr::Literal(Literal::String("Case File".to_string())),
    ));
    let targets = SelectQuery::new(
        post_type(),
        ResolvedShape::new(
            post_type(),
            vec![ResolvedShapeField::new(
                "id",
                post_id,
                Cardinality::Required,
                None,
            )],
        ),
        Some(target_filter),
        vec![],
        None,
        None,
    );

    UpdateQuery::new(
        user,
        Some(source_filter),
        vec![Assignment::with_operator(
            posts,
            operator,
            AssignmentValue::MultiLinkSelect(Box::new(targets)),
        )],
    )
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
