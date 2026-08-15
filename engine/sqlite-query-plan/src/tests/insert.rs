use super::fixtures::{
    empty_post_insert_query, post_insert_with_author_select_assignment,
    post_insert_with_ordered_assignments, post_insert_with_title_assignment,
};
use crate::{SQLiteAssignmentValue, SQLiteGeneratedIdStrategy, SQLiteLiteral, plan_insert};
use alloc::string::ToString;

#[test]
fn sqlite_insert_plan_can_store_root_target() {
    let ir = empty_post_insert_query();

    let plan = plan_insert(&ir);

    assert_eq!(plan.root_target().table_name(), "post");
    assert_eq!(plan.root_target().id_column(), "id");
    assert!(plan.assignments().is_empty());
}

#[test]
fn sqlite_insert_plan_uses_runtime_generated_uuid() {
    let ir = empty_post_insert_query();

    let plan = plan_insert(&ir);

    assert_eq!(
        plan.generated_id_strategy(),
        SQLiteGeneratedIdStrategy::RuntimeUuid
    );
}

#[test]
fn sqlite_insert_plan_maps_scalar_assignment_to_column() {
    let ir = post_insert_with_title_assignment();

    let plan = plan_insert(&ir);
    let assignments = plan.assignments();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].column_name(), "title");
    let SQLiteAssignmentValue::Literal(value) = assignments[0].value() else {
        panic!("expected literal assignment");
    };
    assert_eq!(value, &SQLiteLiteral::String("Case File".to_string()));
}

#[test]
fn sqlite_insert_plan_maps_link_select_to_foreign_key_scalar_select() {
    let ir = post_insert_with_author_select_assignment();

    let plan = plan_insert(&ir);
    let assignment = &plan.assignments()[0];

    assert_eq!(assignment.column_name(), "author_id");
    let SQLiteAssignmentValue::Select(select) = assignment.value() else {
        panic!("expected scalar select assignment");
    };
    assert_eq!(select.root_source().table_name(), "user");
    assert_eq!(select.selected_values()[0].column_name(), Some("id"));
    assert!(select.filter().is_some());
}

#[test]
fn sqlite_insert_plan_preserves_assignment_order() {
    let ir = post_insert_with_ordered_assignments();

    let plan = plan_insert(&ir);
    let assignments = plan.assignments();

    assert_eq!(assignments.len(), 3);
    assert_eq!(assignments[0].column_name(), "view_count");
    assert_eq!(assignments[1].column_name(), "title");
    assert_eq!(assignments[2].column_name(), "author_id");
}
