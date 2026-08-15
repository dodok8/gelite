use super::fixtures::{
    empty_post_insert_query, post_insert_with_author_link, post_insert_with_author_select,
    post_insert_with_null_assignments, post_insert_with_ordered_assignments,
    post_insert_with_scalar_assignments, quoted_insert_query,
};
use crate::{SQLiteBindValue, render_insert};
use alloc::string::ToString;

const GENERATED_ID: &str = "00000000-0000-0000-0000-000000000010";

#[test]
fn sqlite_sqlgen_can_render_insert_with_generated_id() {
    let plan = sqlite_query_plan::plan_insert(&empty_post_insert_query());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(statement.sql(), "INSERT INTO \"post\" (\"id\") VALUES (?)");
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String(GENERATED_ID.to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_scalar_insert_assignments() {
    let plan = sqlite_query_plan::plan_insert(&post_insert_with_scalar_assignments());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\" (\"id\", \"title\", \"view_count\", \"rating\", \"published\") VALUES (?, ?, ?, ?, ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String(GENERATED_ID.to_string()),
            SQLiteBindValue::String("Case File".to_string()),
            SQLiteBindValue::Int64(7),
            SQLiteBindValue::Float64(4.5),
            SQLiteBindValue::Bool(true),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_single_link_foreign_key_assignment() {
    let plan = sqlite_query_plan::plan_insert(&post_insert_with_author_link());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\" (\"id\", \"author_id\") VALUES (?, ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String(GENERATED_ID.to_string()),
            SQLiteBindValue::String("00000000-0000-0000-0000-000000000001".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_insert_link_select_assignment() {
    let plan = sqlite_query_plan::plan_insert(&post_insert_with_author_select());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\" (\"id\", \"author_id\") VALUES (?, (SELECT \"root\".\"id\" FROM \"user\" AS \"root\" WHERE \"root\".\"id\" = ?))"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String(GENERATED_ID.to_string()),
            SQLiteBindValue::String("user-1".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_bind_scalar_and_link_null_assignments() {
    let plan = sqlite_query_plan::plan_insert(&post_insert_with_null_assignments());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\" (\"id\", \"subtitle\", \"author_id\") VALUES (?, ?, ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String(GENERATED_ID.to_string()),
            SQLiteBindValue::Null,
            SQLiteBindValue::Null,
        ]
    );
}

#[test]
fn sqlite_sqlgen_preserves_insert_column_and_bind_order() {
    let plan = sqlite_query_plan::plan_insert(&post_insert_with_ordered_assignments());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\" (\"id\", \"view_count\", \"title\", \"author_id\") VALUES (?, ?, ?, ?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String(GENERATED_ID.to_string()),
            SQLiteBindValue::Int64(7),
            SQLiteBindValue::String("Case File".to_string()),
            SQLiteBindValue::String("00000000-0000-0000-0000-000000000001".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_quotes_insert_identifiers() {
    let plan = sqlite_query_plan::plan_insert(&quoted_insert_query());

    let statement = render_insert(&plan, GENERATED_ID);

    assert_eq!(
        statement.sql(),
        "INSERT INTO \"post\"\"archive\" (\"id\", \"title\"\"quote\") VALUES (?, ?)"
    );
}
