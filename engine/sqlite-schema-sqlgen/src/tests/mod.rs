extern crate alloc;

use crate::{
    RenderedSchemaStatement, render_create_index, render_create_table, render_initial_schema,
    render_insert,
};
use alloc::string::ToString;
use alloc::vec;
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};
use sqlite_schema_plan::{
    SQLiteAffinity, SQLiteColumnPlan, SQLiteForeignKeyAction, SQLiteForeignKeyPlan,
    SQLiteIndexPlan, SQLitePrimaryKeyPlan, SQLiteTablePlan, SQLiteValuePlan,
    plan_catalog_field_inserts, plan_catalog_object_inserts, plan_initial_schema,
    plan_schema_version_insert,
};

const VERSION_ID: &str = "9b496060-9a5c-4c7e-9f32-210f698fe497";
const APPLIED_AT: &str = "2026-08-28T12:34:56.789Z";

#[test]
fn render_create_table_for_catalog_fields_uses_composite_primary_key() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let catalog_fields = &plan.metadata_tables()[2];

    let sql = render_create_table(catalog_fields);

    assert_eq!(
        sql,
        "CREATE TABLE \"_engine_catalog_fields\" (\"object_id\" INTEGER NOT NULL, \"field_id\" INTEGER NOT NULL, \"name\" TEXT NOT NULL, \"field_kind\" TEXT NOT NULL, \"cardinality\" TEXT NOT NULL, \"scalar_type\" TEXT NULL, \"target_object_id\" INTEGER NULL, \"is_implicit\" INTEGER NOT NULL, \"is_unique\" INTEGER NOT NULL, \"inverse_field_name\" TEXT NULL, PRIMARY KEY (\"object_id\", \"field_id\"), FOREIGN KEY (\"object_id\") REFERENCES \"_engine_catalog_objects\"(\"object_id\") ON DELETE RESTRICT, FOREIGN KEY (\"target_object_id\") REFERENCES \"_engine_catalog_objects\"(\"object_id\") ON DELETE RESTRICT)"
    );
}

#[test]
fn render_create_table_quotes_identifiers() {
    let table = SQLiteTablePlan::new_with_constraints(
        "group",
        vec![
            SQLiteColumnPlan::new("select", SQLiteAffinity::Text, false, false, false),
            SQLiteColumnPlan::new("quote\"field", SQLiteAffinity::Integer, true, false, false),
        ],
        Some(SQLitePrimaryKeyPlan::new(vec![
            "select".to_string(),
            "quote\"field".to_string(),
        ])),
        vec![SQLiteForeignKeyPlan::new(
            "quote\"field",
            "target\"table",
            "id",
        )],
    );

    let sql = render_create_table(&table);

    assert_eq!(
        sql,
        "CREATE TABLE \"group\" (\"select\" TEXT NOT NULL, \"quote\"\"field\" INTEGER NULL, PRIMARY KEY (\"select\", \"quote\"\"field\"), FOREIGN KEY (\"quote\"\"field\") REFERENCES \"target\"\"table\"(\"id\") ON DELETE RESTRICT)"
    );
}

#[test]
fn render_create_table_renders_foreign_key_delete_action() {
    let table = SQLiteTablePlan::new_with_foreign_keys(
        "user__posts",
        vec![SQLiteColumnPlan::new(
            "target_id",
            SQLiteAffinity::Text,
            false,
            false,
            false,
        )],
        vec![SQLiteForeignKeyPlan::new_with_on_delete(
            "target_id",
            "post",
            "id",
            SQLiteForeignKeyAction::Cascade,
        )],
    );

    assert_eq!(
        render_create_table(&table),
        "CREATE TABLE \"user__posts\" (\"target_id\" TEXT NOT NULL, FOREIGN KEY (\"target_id\") REFERENCES \"post\"(\"id\") ON DELETE CASCADE)"
    );
}

#[test]
fn render_create_index_for_single_link_foreign_key_index() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "email",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let index = &plan.indexes()[0];

    let sql = render_create_index(index);

    assert_eq!(
        sql,
        "CREATE INDEX \"post__author_id_idx\" ON \"post\" (\"author_id\")"
    );
}

#[test]
fn render_create_index_quotes_identifiers() {
    let index = SQLiteIndexPlan::new("post index", "group", vec!["select".into()], false);

    let sql = render_create_index(&index);

    assert_eq!(sql, "CREATE INDEX \"post index\" ON \"group\" (\"select\")");
}

#[test]
fn render_create_unique_index_uses_create_unique_index() {
    let index = SQLiteIndexPlan::new("user__email_idx", "user", vec!["email".into()], true);

    let sql = render_create_index(&index);

    assert_eq!(
        sql,
        "CREATE UNIQUE INDEX \"user__email_idx\" ON \"user\" (\"email\")"
    );
}

#[test]
fn render_catalog_object_insert_uses_placeholders() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new("User", vec![])]).unwrap();

    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let inserts = plan_catalog_object_inserts(&plan);
    let rendered = render_insert(&inserts[0]);

    assert_eq!(
        rendered.sql(),
        "INSERT INTO \"_engine_catalog_objects\" (\"object_id\", \"name\") VALUES (?, ?)"
    );
    assert_eq!(
        rendered.values(),
        [
            SQLiteValuePlan::Integer(1),
            SQLiteValuePlan::Text("User".into()),
        ]
    )
}

#[test]
fn render_catalog_field_insert_uses_placeholders_and_preserves_null_values() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new("User", vec![])]).unwrap();

    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let inserts = plan_catalog_field_inserts(&plan);
    let rendered = render_insert(&inserts[0]);

    assert_eq!(
        rendered.sql(),
        "INSERT INTO \"_engine_catalog_fields\" (\"object_id\", \"field_id\", \"name\", \"field_kind\", \"cardinality\", \"scalar_type\", \"target_object_id\", \"is_implicit\", \"is_unique\", \"inverse_field_name\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    );
    assert_eq!(
        rendered.values(),
        [
            SQLiteValuePlan::Integer(1),
            SQLiteValuePlan::Integer(1),
            SQLiteValuePlan::Text("id".into()),
            SQLiteValuePlan::Text("scalar".into()),
            SQLiteValuePlan::Text("required".into()),
            SQLiteValuePlan::Text("uuid".into()),
            SQLiteValuePlan::Null,
            SQLiteValuePlan::Integer(1),
            SQLiteValuePlan::Integer(0),
            SQLiteValuePlan::Null,
        ]
    )
}

#[test]
fn render_initial_schema_includes_version_insert_for_empty_catalog() {
    let catalog = SchemaCatalog::try_new(vec![]).expect("valid empty catalog");
    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let statements = render_initial_schema(&plan);

    assert_eq!(
        statements.len(),
        4,
        "three metadata tables and one version INSERT"
    );
    let RenderedSchemaStatement::Insert(insert) = &statements[3] else {
        panic!("the version INSERT must be last");
    };
    assert_eq!(
        insert.sql(),
        "INSERT INTO \"_engine_schema_versions\" (\"version_id\", \"checksum\", \"applied_at\", \"schema_snapshot\") VALUES (?, ?, ?, ?)"
    );
    assert_eq!(
        insert.values(),
        [
            SQLiteValuePlan::Text(VERSION_ID.into()),
            SQLiteValuePlan::Text(
                "f9da3ff0eb7caee22c22eb769ba23ac93e400d922e831da626a064d86091ce53".into()
            ),
            SQLiteValuePlan::Text(APPLIED_AT.into()),
            SQLiteValuePlan::Text(r#"{"format_version":1,"objects":[]}"#.into()),
        ]
    );
}

#[test]
fn render_initial_schema_outputs_deterministic_sql() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "email",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog, VERSION_ID, APPLIED_AT)
        .expect("schema snapshot should serialize");
    let first = render_initial_schema(&plan);
    let second = render_initial_schema(&plan);

    assert_eq!(first.len(), second.len());
    assert_eq!(first.len(), 14);
    for (first_statement, second_statement) in first.iter().zip(second.iter()) {
        assert_eq!(first_statement.sql(), second_statement.sql());
        match (first_statement, second_statement) {
            (RenderedSchemaStatement::Insert(first), RenderedSchemaStatement::Insert(second)) => {
                assert_eq!(first.values(), second.values());
            }
            (RenderedSchemaStatement::Sql(_), RenderedSchemaStatement::Sql(_)) => {}
            _ => panic!("repeated rendering must preserve statement kinds"),
        }
    }

    assert!(
        first[0]
            .sql()
            .starts_with("CREATE TABLE \"_engine_schema_versions\"")
    );
    assert!(
        first[1]
            .sql()
            .starts_with("CREATE TABLE \"_engine_catalog_objects\"")
    );
    assert!(
        first[2]
            .sql()
            .starts_with("CREATE TABLE \"_engine_catalog_fields\"")
    );
    assert!(first[3].sql().starts_with("CREATE TABLE \"user\""));
    assert!(first[4].sql().starts_with("CREATE TABLE \"post\""));
    assert_eq!(
        first[5].sql(),
        "INSERT INTO \"_engine_catalog_objects\" (\"object_id\", \"name\") VALUES (?, ?)"
    );
    assert_eq!(
        first[7].sql(),
        "INSERT INTO \"_engine_catalog_fields\" (\"object_id\", \"field_id\", \"name\", \"field_kind\", \"cardinality\", \"scalar_type\", \"target_object_id\", \"is_implicit\", \"is_unique\", \"inverse_field_name\") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    );
    assert_eq!(
        first[12].sql(),
        "CREATE INDEX \"post__author_id_idx\" ON \"post\" (\"author_id\")"
    );
    let RenderedSchemaStatement::Insert(version) = &first[13] else {
        panic!("the version INSERT must follow all tables, catalog rows, and indexes");
    };
    assert_eq!(
        version.sql(),
        "INSERT INTO \"_engine_schema_versions\" (\"version_id\", \"checksum\", \"applied_at\", \"schema_snapshot\") VALUES (?, ?, ?, ?)"
    );
    assert_eq!(
        version.values(),
        plan_schema_version_insert(&plan)[0].values()
    );

    match &first[5] {
        RenderedSchemaStatement::Insert(insert) => {
            assert_eq!(
                insert.values(),
                [
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Text("User".into()),
                ]
            );
        }
        RenderedSchemaStatement::Sql(_) => panic!("catalog object row should render as insert"),
    }
    match &first[7] {
        RenderedSchemaStatement::Insert(insert) => {
            assert_eq!(insert.values()[6], SQLiteValuePlan::Null);
        }
        RenderedSchemaStatement::Sql(_) => panic!("catalog field row should render as insert"),
    }
}
