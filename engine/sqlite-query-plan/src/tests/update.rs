use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;

use query_ir::{
    Assignment, AssignmentOperator, AssignmentValue, CompareExpr, CompareOp, Expr, Literal,
    ResolvedShape, ResolvedShapeField, SelectQuery, UpdateQuery,
};
use schema_model::{Cardinality, FieldId, FieldRef, ObjectTypeId, ObjectTypeRef};

use super::fixtures::{
    post_author_field, post_author_name_path_value, post_generated_join_name_path_value,
    post_id_field, post_title_field, post_title_path_value, post_type, user_best_friend_field,
    user_name_field, user_type,
};
use crate::{
    SQLiteAssignmentValue, SQLiteJoinKind, SQLiteLiteral, SQLiteMultiLinkMutationOp,
    SQLiteValueExpr, SQLiteWhereExpr, plan_update,
};

#[test]
fn sqlite_update_plan_targets_one_root_table() {
    let ir = UpdateQuery::new(post_type(), None, vec![title_assignment()]);

    let plan = plan_update(&ir);

    assert_eq!(plan.target().object_type().name(), "Post");
    assert_eq!(plan.target().table_name(), "post");
    assert_eq!(plan.target().alias(), "root");
    assert!(plan.filter().is_none());
    assert!(plan.joins().is_empty());
}

#[test]
fn sqlite_update_plan_maps_assignments_in_definition_order() {
    let ir = UpdateQuery::new(
        post_type(),
        None,
        vec![
            title_assignment(),
            Assignment::new(
                post_author_field(),
                AssignmentValue::LinkId("user-2".to_string()),
            ),
        ],
    );

    let plan = plan_update(&ir);

    assert_eq!(plan.assignments().len(), 2);
    assert_eq!(plan.assignments()[0].column_name(), "title");
    let SQLiteAssignmentValue::Literal(title) = plan.assignments()[0].value() else {
        panic!("expected literal title assignment");
    };
    assert_eq!(title, &SQLiteLiteral::String("Closed Case".to_string()));
    assert_eq!(plan.assignments()[1].column_name(), "author_id");
    let SQLiteAssignmentValue::Literal(author) = plan.assignments()[1].value() else {
        panic!("expected literal author assignment");
    };
    assert_eq!(author, &SQLiteLiteral::String("user-2".to_string()));
}

#[test]
fn sqlite_update_plan_reuses_root_filter_planning() {
    let filter = Expr::Compare(CompareExpr::new(
        post_title_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Draft".to_string())),
    ));
    let ir = UpdateQuery::new(post_type(), Some(filter), vec![title_assignment()]);

    let plan = plan_update(&ir);

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };
    let SQLiteValueExpr::Column(column) = compare.left() else {
        panic!("expected root column");
    };
    assert_eq!(column.source_alias(), "root");
    assert_eq!(column.column_name(), "title");
}

#[test]
fn sqlite_update_plan_uses_join_only_for_related_filter_path() {
    let filter = Expr::Compare(CompareExpr::new(
        post_author_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let ir = UpdateQuery::new(post_type(), Some(filter), vec![title_assignment()]);

    let plan = plan_update(&ir);

    assert_eq!(plan.target().table_name(), "post");
    assert_eq!(plan.joins().len(), 1);
    assert_eq!(plan.joins()[0].kind(), SQLiteJoinKind::Inner);
    assert_eq!(plan.joins()[0].source_alias(), "root");
    assert_eq!(plan.joins()[0].target_table(), "user");
    assert_eq!(plan.joins()[0].target_alias(), "author");

    let Some(SQLiteWhereExpr::Compare(compare)) = plan.filter() else {
        panic!("expected compare filter");
    };
    let SQLiteValueExpr::Column(column) = compare.left() else {
        panic!("expected related column");
    };
    assert_eq!(column.source_alias(), "author");
    assert_eq!(column.column_name(), "name");
}

#[test]
fn sqlite_update_plan_reserves_root_path_aliases_for_generated_join_aliases() {
    let root_link_filter = Expr::Compare(CompareExpr::new(
        post_generated_join_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let nested_link_filter = Expr::Compare(CompareExpr::new(
        post_author_best_friend_name_path_value(),
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Carol".to_string())),
    ));
    let ir = UpdateQuery::new(
        post_type(),
        Some(Expr::And(
            Box::new(root_link_filter),
            Box::new(nested_link_filter),
        )),
        vec![title_assignment()],
    );

    let plan = plan_update(&ir);

    assert_eq!(
        plan.joins()
            .iter()
            .filter(|join| join.target_alias() == "__gelite_join_0")
            .count(),
        1
    );
}

#[test]
fn sqlite_update_plan_does_not_reuse_root_alias_for_a_link() {
    let root_link = FieldRef::new(FieldId::new(8), post_type(), "root");
    let path = query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(root_link, user_type(), Cardinality::Required),
                query_ir::ResolvedPathStep::scalar(user_name_field(), Cardinality::Required),
            ],
        )
        .expect("post root name path should be valid"),
    );
    let filter = Expr::Compare(CompareExpr::new(
        path,
        CompareOp::Eq,
        query_ir::ValueExpr::Literal(Literal::String("Sheri".to_string())),
    ));
    let ir = UpdateQuery::new(post_type(), Some(filter), vec![title_assignment()]);

    let plan = plan_update(&ir);

    assert_ne!(plan.joins()[0].target_alias(), plan.target().alias());
}

#[test]
fn sqlite_update_plan_maps_multi_link_add_and_remove_to_join_table_plans() {
    for (operator, expected) in [
        (AssignmentOperator::Add, SQLiteMultiLinkMutationOp::Add),
        (
            AssignmentOperator::Remove,
            SQLiteMultiLinkMutationOp::Remove,
        ),
    ] {
        let plan = plan_update(&multi_link_update(operator, None, None));
        let mutation = plan
            .multi_link_mutation()
            .expect("multi-link update should have a join-table plan");

        assert_eq!(mutation.operation(), expected);
        assert_eq!(mutation.join_table_name(), "user__posts");
        assert_eq!(mutation.source_column(), "source_id");
        assert_eq!(mutation.target_column(), "target_id");
        assert_eq!(mutation.sources().root_source().table_name(), "user");
        assert_eq!(mutation.targets().root_source().table_name(), "post");
        assert!(plan.assignments().is_empty());
    }
}

fn multi_link_update(
    operator: AssignmentOperator,
    source_filter: Option<Expr>,
    target_filter: Option<Expr>,
) -> UpdateQuery {
    let user = ObjectTypeRef::new(ObjectTypeId::new(10), "User");
    let posts = FieldRef::new(FieldId::new(11), user.clone(), "posts");
    let post_id = post_id_field();
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
        target_filter,
        vec![],
        None,
        None,
    );

    UpdateQuery::new(
        user,
        source_filter,
        vec![Assignment::with_operator(
            posts,
            operator,
            AssignmentValue::MultiLinkSelect(Box::new(targets)),
        )],
    )
}

fn post_author_best_friend_name_path_value() -> query_ir::ValueExpr {
    query_ir::ValueExpr::Path(
        query_ir::ResolvedPath::try_new(
            post_type(),
            vec![
                query_ir::ResolvedPathStep::link(
                    post_author_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::link(
                    user_best_friend_field(),
                    user_type(),
                    schema_model::Cardinality::Required,
                ),
                query_ir::ResolvedPathStep::scalar(
                    user_name_field(),
                    schema_model::Cardinality::Required,
                ),
            ],
        )
        .expect("post author best_friend name path should be valid"),
    )
}

fn title_assignment() -> Assignment {
    Assignment::new(
        post_title_field(),
        AssignmentValue::Scalar(Literal::String("Closed Case".to_string())),
    )
}
