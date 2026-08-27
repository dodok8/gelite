extern crate alloc;

use crate::{
    SQLiteAffinity, SQLiteCatalogFieldKind, SQLiteForeignKeyAction, SQLiteInsertPlan,
    SQLiteValuePlan, plan_catalog_field_inserts, plan_catalog_object_inserts, plan_initial_schema,
};
use alloc::vec;
use alloc::vec::Vec;
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality, Uniqueness,
};

#[test]
fn initial_schema_plan_creates_metadata_tables() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();

    let plan = plan_initial_schema(&catalog);

    let table_names = plan
        .metadata_tables()
        .iter()
        .map(|table| table.name())
        .collect::<Vec<_>>();

    assert_eq!(
        table_names,
        vec![
            "_engine_schema_versions",
            "_engine_catalog_objects",
            "_engine_catalog_fields",
        ]
    );
}

#[test]
fn initial_schema_plan_defines_catalog_objects_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[1].name(), "_engine_catalog_objects");
    assert_eq!(plan.metadata_tables()[1].columns().len(), 2);

    let columns = plan.metadata_tables()[1].columns();
    assert_eq!(columns[0].name(), "object_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Integer);
    assert!(!columns[0].is_nullable());
    assert!(columns[0].is_primary_key());
    assert!(columns[0].is_unique());

    assert_eq!(columns[1].name(), "name");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());
    assert!(columns[1].is_unique());
}

#[test]
fn initial_schema_plan_defines_schema_versions_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[0].name(), "_engine_schema_versions");
    assert_eq!(plan.metadata_tables()[0].columns().len(), 4);

    let columns = plan.metadata_tables()[0].columns();
    assert_eq!(columns[0].name(), "version_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert!(!columns[0].is_nullable());
    assert!(columns[0].is_primary_key());
    assert!(columns[0].is_unique());

    assert_eq!(columns[1].name(), "checksum");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());
    assert!(!columns[1].is_unique());

    assert_eq!(columns[2].name(), "applied_at");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert!(!columns[2].is_nullable());
    assert!(!columns[2].is_primary_key());
    assert!(!columns[2].is_unique());

    assert_eq!(columns[3].name(), "schema_snapshot");
    assert_eq!(columns[3].affinity(), SQLiteAffinity::Text);
    assert!(!columns[3].is_nullable());
    assert!(!columns[3].is_primary_key());
    assert!(!columns[3].is_unique());
}

#[test]
fn initial_schema_plan_defines_catalog_fields_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[2].name(), "_engine_catalog_fields");
    assert_eq!(plan.metadata_tables()[2].columns().len(), 10);

    let columns = plan.metadata_tables()[2].columns();
    assert_eq!(columns[0].name(), "object_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Integer);
    assert!(!columns[0].is_nullable());
    assert!(!columns[0].is_primary_key());
    assert!(!columns[0].is_unique());

    assert_eq!(columns[1].name(), "field_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Integer);
    assert!(!columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());
    assert!(!columns[1].is_unique());

    assert_eq!(columns[2].name(), "name");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert!(!columns[2].is_nullable());
    assert!(!columns[2].is_primary_key());
    assert!(!columns[2].is_unique());

    assert_eq!(columns[3].name(), "field_kind");
    assert_eq!(columns[3].affinity(), SQLiteAffinity::Text);
    assert!(!columns[3].is_nullable());
    assert!(!columns[3].is_primary_key());
    assert!(!columns[3].is_unique());

    assert_eq!(columns[4].name(), "cardinality");
    assert_eq!(columns[4].affinity(), SQLiteAffinity::Text);
    assert!(!columns[4].is_nullable());
    assert!(!columns[4].is_primary_key());
    assert!(!columns[4].is_unique());

    assert_eq!(columns[5].name(), "scalar_type");
    assert_eq!(columns[5].affinity(), SQLiteAffinity::Text);
    assert!(columns[5].is_nullable());
    assert!(!columns[5].is_primary_key());
    assert!(!columns[5].is_unique());

    assert_eq!(columns[6].name(), "target_object_id");
    assert_eq!(columns[6].affinity(), SQLiteAffinity::Integer);
    assert!(columns[6].is_nullable());
    assert!(!columns[6].is_primary_key());
    assert!(!columns[6].is_unique());

    assert_eq!(columns[7].name(), "is_implicit");
    assert_eq!(columns[7].affinity(), SQLiteAffinity::Integer);
    assert!(!columns[7].is_nullable());
    assert!(!columns[7].is_primary_key());
    assert!(!columns[7].is_unique());

    assert_eq!(columns[8].name(), "is_unique");
    assert_eq!(columns[8].affinity(), SQLiteAffinity::Integer);
    assert!(!columns[8].is_nullable());
    assert!(!columns[8].is_primary_key());
    assert!(!columns[8].is_unique());

    let primary_key = plan.metadata_tables()[2].primary_key().unwrap();
    assert_eq!(primary_key.column_names().len(), 2);
    assert_eq!(primary_key.column_names()[0], "object_id");
    assert_eq!(primary_key.column_names()[1], "field_id");
}

#[test]
fn initial_schema_plan_defines_catalog_fields_foreign_keys() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    let catalog_fields = &plan.metadata_tables()[2];
    assert_eq!(catalog_fields.name(), "_engine_catalog_fields");
    assert_eq!(catalog_fields.foreign_keys().len(), 2);

    let object_foreign_key = &catalog_fields.foreign_keys()[0];
    assert_eq!(object_foreign_key.column_name(), "object_id");
    assert_eq!(object_foreign_key.target_table(), "_engine_catalog_objects");
    assert_eq!(object_foreign_key.target_column(), "object_id");

    let target_object_foreign_key = &catalog_fields.foreign_keys()[1];
    assert_eq!(target_object_foreign_key.column_name(), "target_object_id");
    assert_eq!(
        target_object_foreign_key.target_table(),
        "_engine_catalog_objects"
    );
    assert_eq!(target_object_foreign_key.target_column(), "object_id");
}

#[test]
fn initial_schema_plan_creates_object_table_for_scalar_fields() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![
            Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "age",
                ScalarType::Int64,
                SingleCardinality::Optional,
            )),
        ],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert!(!columns[0].is_nullable());
    assert!(columns[0].is_primary_key());

    assert_eq!(columns[1].name(), "name");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());

    assert_eq!(columns[2].name(), "age");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Integer);
    assert!(columns[2].is_nullable());
}

#[test]
fn initial_schema_plan_maps_all_scalar_types_to_sqlite_affinities() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "ScalarSample",
        vec![
            Field::Scalar(ScalarField::new(
                "str_field",
                ScalarType::Str,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "int64_field",
                ScalarType::Int64,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "float64_field",
                ScalarType::Float64,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "bool_field",
                ScalarType::Bool,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "uuid_field",
                ScalarType::Uuid,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "datetime_field",
                ScalarType::DateTime,
                SingleCardinality::Optional,
            )),
        ],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let columns = plan.object_tables()[0].columns();

    let expected_affinities = [
        ("id", SQLiteAffinity::Text),
        ("str_field", SQLiteAffinity::Text),
        ("int64_field", SQLiteAffinity::Integer),
        ("float64_field", SQLiteAffinity::Real),
        ("bool_field", SQLiteAffinity::Integer),
        ("uuid_field", SQLiteAffinity::Text),
        ("datetime_field", SQLiteAffinity::Text),
    ];

    for (index, (expected_name, expected_affinity)) in expected_affinities.iter().enumerate() {
        assert_eq!(columns[index].name(), *expected_name);
        assert_eq!(columns[index].affinity(), *expected_affinity);
    }
}

#[test]
fn initial_schema_plan_creates_required_single_link_foreign_key_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
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
                Field::Link(LinkField::new(
                    "author",
                    "User",
                    schema_model::Cardinality::Required,
                )),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let post = &plan.object_tables()[1];
    assert_eq!(post.name(), "post");

    let columns = post.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert!(!columns[0].is_nullable());
    assert!(columns[0].is_primary_key());

    assert_eq!(columns[1].name(), "title");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());

    assert_eq!(columns[2].name(), "author_id");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert!(!columns[2].is_nullable());
    assert!(!columns[2].is_primary_key());

    assert_eq!(post.foreign_keys().len(), 1);

    let foreign_key = &post.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "author_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
    assert_eq!(foreign_key.on_delete(), SQLiteForeignKeyAction::Restrict);
}

#[test]
fn initial_schema_plan_creates_optional_single_link_foreign_key_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Link(LinkField::new(
                "author",
                "User",
                schema_model::Cardinality::Optional,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let post = &plan.object_tables()[1];
    assert_eq!(post.name(), "post");

    let columns = post.columns();
    assert_eq!(columns[1].name(), "author_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());

    assert_eq!(post.foreign_keys().len(), 1);

    let foreign_key = &post.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "author_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn schema_scalar_field_can_be_marked_unique() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField::with_uniqueness(
            "email",
            ScalarType::Str,
            SingleCardinality::Required,
            Uniqueness::Unique,
        ))],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert!(!columns[0].is_nullable());
    assert!(columns[0].is_primary_key());
    assert!(columns[0].is_unique());

    assert_eq!(columns[1].name(), "email");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());
    assert!(columns[1].is_unique());
}

#[test]
fn schema_scalar_field_new_is_not_unique_by_default() {
    let field = ScalarField::new("name", ScalarType::Str, SingleCardinality::Required);

    assert_eq!(field.uniqueness(), Uniqueness::NotUnique);
    assert!(!field.is_unique());
}

#[test]
fn initial_schema_plan_allows_optional_unique_scalar_field() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField::with_uniqueness(
            "nickname",
            ScalarType::Str,
            SingleCardinality::Optional,
            Uniqueness::Unique,
        ))],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[1].name(), "nickname");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(columns[1].is_nullable());
    assert!(columns[1].is_unique());
}

#[test]
fn initial_schema_plan_marks_required_unique_single_link_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Profile",
            vec![Field::Link(LinkField::with_uniqueness(
                "user",
                "User",
                schema_model::Cardinality::Required,
                Uniqueness::Unique,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let profile = &plan.object_tables()[1];
    assert_eq!(profile.name(), "profile");

    let columns = profile.columns();
    assert_eq!(columns[1].name(), "user_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());
    assert!(columns[1].is_unique());

    assert_eq!(profile.foreign_keys().len(), 1);

    let foreign_key = &profile.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "user_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn initial_schema_plan_marks_optional_unique_single_link_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Profile",
            vec![Field::Link(LinkField::with_uniqueness(
                "user",
                "User",
                schema_model::Cardinality::Optional,
                Uniqueness::Unique,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let profile = &plan.object_tables()[1];
    assert_eq!(profile.name(), "profile");

    let columns = profile.columns();
    assert_eq!(columns[1].name(), "user_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(columns[1].is_nullable());
    assert!(!columns[1].is_primary_key());
    assert!(columns[1].is_unique());

    assert_eq!(profile.foreign_keys().len(), 1);

    let foreign_key = &profile.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "user_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn initial_schema_plan_creates_multi_link_join_table() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Link(LinkField::new(
                "posts",
                "Post",
                Cardinality::Many,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let relation_tables = plan.relation_tables();
    assert_eq!(relation_tables.len(), 1);

    let user_posts = &relation_tables[0];
    assert_eq!(user_posts.name(), "user__posts");

    let columns = user_posts.columns();
    assert_eq!(columns[0].name(), "source_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert!(!columns[0].is_nullable());

    assert_eq!(columns[1].name(), "target_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert!(!columns[1].is_nullable());

    assert_eq!(columns[2].name(), "position");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Integer);
    assert!(columns[2].is_nullable());

    let primary_key = user_posts
        .primary_key()
        .expect("join table should have primary key");
    assert_eq!(primary_key.column_names(), &["source_id", "target_id"]);

    let foreign_keys = user_posts.foreign_keys();
    assert_eq!(foreign_keys.len(), 2);

    assert_eq!(foreign_keys[0].column_name(), "source_id");
    assert_eq!(foreign_keys[0].target_table(), "user");
    assert_eq!(foreign_keys[0].target_column(), "id");
    assert_eq!(foreign_keys[0].on_delete(), SQLiteForeignKeyAction::Cascade);

    assert_eq!(foreign_keys[1].column_name(), "target_id");
    assert_eq!(foreign_keys[1].target_table(), "post");
    assert_eq!(foreign_keys[1].target_column(), "id");
    assert_eq!(foreign_keys[1].on_delete(), SQLiteForeignKeyAction::Cascade);
}

#[test]
fn initial_schema_plan_records_catalog_object_rows() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Link(LinkField::new(
                "posts",
                "Post",
                Cardinality::Many,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let rows = plan.catalog_object_rows();

    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0].object_id(), 1);
    assert_eq!(rows[0].name(), "User");

    assert_eq!(rows[1].object_id(), 2);
    assert_eq!(rows[1].name(), "Post");
}

#[test]
fn initial_schema_plan_records_catalog_field_rows() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![
                Field::Scalar(ScalarField::with_uniqueness(
                    "email",
                    ScalarType::Str,
                    SingleCardinality::Required,
                    Uniqueness::Unique,
                )),
                Field::Link(LinkField::new("posts", "Post", Cardinality::Many)),
            ],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::with_uniqueness(
                    "author",
                    "User",
                    Cardinality::Required,
                    Uniqueness::Unique,
                )),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let rows = plan.catalog_field_rows();

    assert_eq!(rows.len(), 6);

    assert_eq!(rows[0].object_id(), 1);
    assert_eq!(rows[0].field_id(), 1);
    assert_eq!(rows[0].name(), "id");
    assert_eq!(rows[0].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[0].cardinality(), Cardinality::Required);
    assert_eq!(rows[0].scalar_type(), Some(ScalarType::Uuid));
    assert_eq!(rows[0].target_object_id(), None);
    assert!(rows[0].is_implicit());
    assert!(!rows[0].is_unique());

    assert_eq!(rows[1].object_id(), 1);
    assert_eq!(rows[1].field_id(), 2);
    assert_eq!(rows[1].name(), "email");
    assert_eq!(rows[1].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[1].cardinality(), Cardinality::Required);
    assert_eq!(rows[1].scalar_type(), Some(ScalarType::Str));
    assert_eq!(rows[1].target_object_id(), None);
    assert!(!rows[1].is_implicit());
    assert!(rows[1].is_unique());

    assert_eq!(rows[2].object_id(), 1);
    assert_eq!(rows[2].field_id(), 3);
    assert_eq!(rows[2].name(), "posts");
    assert_eq!(rows[2].field_kind(), SQLiteCatalogFieldKind::Link);
    assert_eq!(rows[2].cardinality(), Cardinality::Many);
    assert_eq!(rows[2].scalar_type(), None);
    assert_eq!(rows[2].target_object_id(), Some(2));
    assert!(!rows[2].is_implicit());
    assert!(!rows[2].is_unique());

    assert_eq!(rows[3].object_id(), 2);
    assert_eq!(rows[3].field_id(), 1);
    assert_eq!(rows[3].name(), "id");
    assert_eq!(rows[3].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[3].cardinality(), Cardinality::Required);
    assert_eq!(rows[3].scalar_type(), Some(ScalarType::Uuid));
    assert_eq!(rows[3].target_object_id(), None);
    assert!(rows[3].is_implicit());
    assert!(!rows[3].is_unique());

    assert_eq!(rows[4].object_id(), 2);
    assert_eq!(rows[4].field_id(), 2);
    assert_eq!(rows[4].name(), "title");
    assert_eq!(rows[4].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[4].cardinality(), Cardinality::Required);
    assert_eq!(rows[4].scalar_type(), Some(ScalarType::Str));
    assert_eq!(rows[4].target_object_id(), None);
    assert!(!rows[4].is_implicit());
    assert!(!rows[4].is_unique());

    assert_eq!(rows[5].object_id(), 2);
    assert_eq!(rows[5].field_id(), 3);
    assert_eq!(rows[5].name(), "author");
    assert_eq!(rows[5].field_kind(), SQLiteCatalogFieldKind::Link);
    assert_eq!(rows[5].cardinality(), Cardinality::Required);
    assert_eq!(rows[5].scalar_type(), None);
    assert_eq!(rows[5].target_object_id(), Some(1));
    assert!(!rows[5].is_implicit());
    assert!(rows[5].is_unique());
}

#[test]
fn initial_schema_plan_can_plan_catalog_object_inserts() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let inserts = plan_catalog_object_inserts(&plan);

    assert_eq!(inserts.len(), 2);

    assert_eq!(inserts[0].table_name(), "_engine_catalog_objects");
    assert_eq!(inserts[0].columns().len(), 2);
    assert_eq!(inserts[0].columns()[0], "object_id");
    assert_eq!(inserts[0].columns()[1], "name");
    assert_eq!(inserts[0].values().len(), 2);
    assert_eq!(inserts[0].values()[0], SQLiteValuePlan::Integer(1));
    match &inserts[0].values()[1] {
        SQLiteValuePlan::Text(value) => assert_eq!(value, "User"),
        value => panic!("expected object name text value, got {value:?}"),
    }

    assert_eq!(inserts[1].table_name(), "_engine_catalog_objects");
    assert_eq!(inserts[1].columns().len(), 2);
    assert_eq!(inserts[1].columns()[0], "object_id");
    assert_eq!(inserts[1].columns()[1], "name");
    assert_eq!(inserts[1].values().len(), 2);
    assert_eq!(inserts[1].values()[0], SQLiteValuePlan::Integer(2));
    match &inserts[1].values()[1] {
        SQLiteValuePlan::Text(value) => assert_eq!(value, "Post"),
        value => panic!("expected object name text value, got {value:?}"),
    }
}

#[test]
fn initial_schema_plan_can_plan_catalog_field_inserts() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![
                Field::Scalar(ScalarField::with_uniqueness(
                    "email",
                    ScalarType::Str,
                    SingleCardinality::Required,
                    Uniqueness::Unique,
                )),
                Field::Link(LinkField::new("posts", "Post", Cardinality::Many)),
            ],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::with_uniqueness(
                    "author",
                    "User",
                    Cardinality::Required,
                    Uniqueness::Unique,
                )),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let inserts = plan_catalog_field_inserts(&plan);

    assert_eq!(inserts.len(), 6);

    assert_eq!(inserts[0].table_name(), "_engine_catalog_fields");
    assert_eq!(inserts[0].columns().len(), 10);
    assert_eq!(inserts[0].columns()[0], "object_id");
    assert_eq!(inserts[0].columns()[1], "field_id");
    assert_eq!(inserts[0].columns()[2], "name");
    assert_eq!(inserts[0].columns()[3], "field_kind");
    assert_eq!(inserts[0].columns()[4], "cardinality");
    assert_eq!(inserts[0].columns()[5], "scalar_type");
    assert_eq!(inserts[0].columns()[6], "target_object_id");
    assert_eq!(inserts[0].columns()[7], "is_implicit");
    assert_eq!(inserts[0].columns()[8], "is_unique");

    assert_eq!(inserts[0].values().len(), 10);
    assert_eq!(inserts[0].values()[0], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[0].values()[1], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[0].values()[2], SQLiteValuePlan::Text("id".into()));
    assert_eq!(
        inserts[0].values()[3],
        SQLiteValuePlan::Text("scalar".into())
    );
    assert_eq!(
        inserts[0].values()[4],
        SQLiteValuePlan::Text("required".into())
    );
    assert_eq!(inserts[0].values()[5], SQLiteValuePlan::Text("uuid".into()));
    assert_eq!(inserts[0].values()[6], SQLiteValuePlan::Null);
    assert_eq!(inserts[0].values()[7], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[0].values()[8], SQLiteValuePlan::Integer(0));

    assert_eq!(inserts[1].table_name(), "_engine_catalog_fields");
    assert_eq!(inserts[1].columns(), inserts[0].columns());
    assert_eq!(inserts[1].values()[0], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[1].values()[1], SQLiteValuePlan::Integer(2));
    assert_eq!(
        inserts[1].values()[2],
        SQLiteValuePlan::Text("email".into())
    );
    assert_eq!(
        inserts[1].values()[3],
        SQLiteValuePlan::Text("scalar".into())
    );
    assert_eq!(
        inserts[1].values()[4],
        SQLiteValuePlan::Text("required".into())
    );
    assert_eq!(inserts[1].values()[5], SQLiteValuePlan::Text("str".into()));
    assert_eq!(inserts[1].values()[6], SQLiteValuePlan::Null);
    assert_eq!(inserts[1].values()[7], SQLiteValuePlan::Integer(0));
    assert_eq!(inserts[1].values()[8], SQLiteValuePlan::Integer(1));

    assert_eq!(inserts[2].table_name(), "_engine_catalog_fields");
    assert_eq!(inserts[2].columns(), inserts[0].columns());
    assert_eq!(inserts[2].values()[0], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[2].values()[1], SQLiteValuePlan::Integer(3));
    assert_eq!(
        inserts[2].values()[2],
        SQLiteValuePlan::Text("posts".into())
    );
    assert_eq!(inserts[2].values()[3], SQLiteValuePlan::Text("link".into()));
    assert_eq!(inserts[2].values()[4], SQLiteValuePlan::Text("many".into()));
    assert_eq!(inserts[2].values()[5], SQLiteValuePlan::Null);
    assert_eq!(inserts[2].values()[6], SQLiteValuePlan::Integer(2));
    assert_eq!(inserts[2].values()[7], SQLiteValuePlan::Integer(0));
    assert_eq!(inserts[2].values()[8], SQLiteValuePlan::Integer(0));

    assert_eq!(inserts[5].table_name(), "_engine_catalog_fields");
    assert_eq!(inserts[5].columns(), inserts[0].columns());
    assert_eq!(inserts[5].values()[0], SQLiteValuePlan::Integer(2));
    assert_eq!(inserts[5].values()[1], SQLiteValuePlan::Integer(3));
    assert_eq!(
        inserts[5].values()[2],
        SQLiteValuePlan::Text("author".into())
    );
    assert_eq!(inserts[5].values()[3], SQLiteValuePlan::Text("link".into()));
    assert_eq!(
        inserts[5].values()[4],
        SQLiteValuePlan::Text("required".into())
    );
    assert_eq!(inserts[5].values()[5], SQLiteValuePlan::Null);
    assert_eq!(inserts[5].values()[6], SQLiteValuePlan::Integer(1));
    assert_eq!(inserts[5].values()[7], SQLiteValuePlan::Integer(0));
    assert_eq!(inserts[5].values()[8], SQLiteValuePlan::Integer(1));
}

#[test]
fn initial_schema_plan_creates_single_link_foreign_key_index() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::with_uniqueness(
                "email",
                ScalarType::Str,
                SingleCardinality::Required,
                Uniqueness::Unique,
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

    let plan = plan_initial_schema(&catalog);
    let indexes = plan.indexes();

    assert_eq!(indexes.len(), 1);

    assert_eq!(indexes[0].name(), "post__author_id_idx");
    assert_eq!(indexes[0].table_name(), "post");
    assert_eq!(indexes[0].column_names().len(), 1);
    assert_eq!(indexes[0].column_names()[0], "author_id");
    assert!(!indexes[0].is_unique());
}

#[test]
fn initial_schema_plan_creates_multi_link_join_table_indexes() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Link(LinkField::new(
                "posts",
                "Post",
                Cardinality::Many,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let indexes = plan.indexes();

    assert_eq!(indexes.len(), 2);

    assert_eq!(indexes[0].name(), "user__posts__source_id_idx");
    assert_eq!(indexes[0].table_name(), "user__posts");
    assert_eq!(indexes[0].column_names().len(), 1);
    assert_eq!(indexes[0].column_names()[0], "source_id");
    assert!(!indexes[0].is_unique());

    assert_eq!(indexes[1].name(), "user__posts__target_id_idx");
    assert_eq!(indexes[1].table_name(), "user__posts");
    assert_eq!(indexes[1].column_names().len(), 1);
    assert_eq!(indexes[1].column_names()[0], "target_id");
    assert!(!indexes[1].is_unique());
}

#[test]
fn inverse_schema_owns_no_storage_and_records_source_metadata() {
    for cardinality in [Cardinality::Optional, Cardinality::Many] {
        let catalog = SchemaCatalog::try_new(vec![
            ObjectType::new(
                "Department",
                vec![Field::Link(LinkField::with_inverse(
                    "employees",
                    "Employee",
                    Cardinality::Many,
                    "department",
                ))],
            ),
            ObjectType::new(
                "Employee",
                vec![Field::Link(LinkField::new(
                    "department",
                    "Department",
                    cardinality,
                ))],
            ),
        ])
        .expect("valid inverse schema");
        let plan = plan_initial_schema(&catalog);
        assert_eq!(plan.object_tables()[0].columns().len(), 1);
        assert!(plan.object_tables()[0].foreign_keys().is_empty());
        assert!(
            plan.relation_tables()
                .iter()
                .all(|table| table.name() != "department__employees")
        );
        assert!(
            plan.indexes()
                .iter()
                .all(|index| !index.table_name().starts_with("department"))
        );
        assert_eq!(
            plan.relation_tables().len(),
            usize::from(cardinality == Cardinality::Many)
        );
        let inserts = plan_catalog_field_inserts(&plan);
        let inverse = inserts
            .iter()
            .find(|row| row.values()[2] == SQLiteValuePlan::Text("employees".into()))
            .expect("inverse metadata");
        let index = inverse
            .columns()
            .iter()
            .position(|name| name == "inverse_field_name")
            .expect("source metadata column");
        assert_eq!(
            inverse.values()[index],
            SQLiteValuePlan::Text("department".into())
        );
    }
}

const VERSION_ID: &str = "9b496060-9a5c-4c7e-9f32-210f698fe497";
const APPLIED_AT: &str = "2026-08-28T12:34:56.789Z";

fn initial_version_insert(
    catalog: &SchemaCatalog,
    version_id: &str,
    applied_at: &str,
) -> SQLiteInsertPlan {
    // Issue #59: the planner accepts caller-supplied application values and
    // exposes one version insert, following the existing catalog insert API.
    let plan = plan_initial_schema(catalog, version_id, applied_at);
    crate::plan_schema_version_insert(&plan)
}

fn version_content(insert: &SQLiteInsertPlan) -> (&str, &str) {
    let [
        SQLiteValuePlan::Text(_),
        SQLiteValuePlan::Text(checksum),
        SQLiteValuePlan::Text(_),
        SQLiteValuePlan::Text(snapshot),
    ] = insert.values()
    else {
        panic!("expected version_id, checksum, applied_at, and schema_snapshot text bindings");
    };
    (snapshot, checksum)
}

fn version_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![
                Field::Link(LinkField::with_inverse(
                    "posts",
                    "Post",
                    Cardinality::Many,
                    "author",
                )),
                Field::Scalar(ScalarField::with_uniqueness(
                    "name",
                    ScalarType::Str,
                    SingleCardinality::Optional,
                    Uniqueness::Unique,
                )),
            ],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::with_uniqueness(
                    "author",
                    "User",
                    Cardinality::Required,
                    Uniqueness::Unique,
                )),
            ],
        ),
    ])
    .expect("valid schema with scalar, stored link, and inverse fields")
}

#[test]
fn initial_schema_version_insert_binds_values_and_omits_implicit_identity() {
    // Digests were independently computed from the literal UTF-8 snapshots.
    for (objects, snapshot, checksum) in [
        (
            vec![],
            r#"{"format_version":1,"objects":[]}"#,
            "f9da3ff0eb7caee22c22eb769ba23ac93e400d922e831da626a064d86091ce53",
        ),
        (
            vec![ObjectType::new("User", vec![])],
            r#"{"format_version":1,"objects":[{"name":"User","fields":[]}]}"#,
            "c24b341cc751d78ec9ca7da1aca92c9bcfe90c7b47ef6cb6d36547842e508397",
        ),
    ] {
        let catalog = SchemaCatalog::try_new(objects).expect("valid empty schema or object");
        let insert = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);

        assert_eq!(insert.table_name(), "_engine_schema_versions");
        assert_eq!(
            insert.columns(),
            ["version_id", "checksum", "applied_at", "schema_snapshot"]
        );
        assert_eq!(
            insert.values(),
            [
                SQLiteValuePlan::Text(VERSION_ID.into()),
                SQLiteValuePlan::Text(checksum.into()),
                SQLiteValuePlan::Text(APPLIED_AT.into()),
                SQLiteValuePlan::Text(snapshot.into()),
            ]
        );
    }
}

#[test]
fn initial_schema_version_snapshot_matches_canonical_bytes_and_sha256() {
    let catalog = version_catalog();
    let insert = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);
    let expected = concat!(
        r#"{"format_version":1,"objects":[{"name":"Post","fields":["#,
        r#"{"name":"author","kind":"link","target_type":"User","cardinality":"required","unique":true,"inverse_field":null},"#,
        r#"{"name":"title","kind":"scalar","scalar_type":"str","cardinality":"required","unique":false}]},"#,
        r#"{"name":"User","fields":["#,
        r#"{"name":"name","kind":"scalar","scalar_type":"str","cardinality":"optional","unique":true},"#,
        r#"{"name":"posts","kind":"link","target_type":"Post","cardinality":"many","unique":false,"inverse_field":"author"}]}]}"#,
    );

    assert_eq!(
        version_content(&insert),
        (
            expected,
            "40bdaa3e66c1b8a14be4221d35348439f145eb71d776c8c9e9a59ba83d1222a6"
        )
    );
}

#[test]
fn initial_schema_version_snapshot_ignores_declaration_order_and_catalog_ids() {
    let catalog = version_catalog();
    let original = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);

    for (reverse_objects, reverse_fields) in [(true, false), (false, true), (true, true)] {
        let mut objects = catalog
            .object_types()
            .iter()
            .map(|object| {
                let mut fields = object.declared_fields().to_vec();
                if reverse_fields {
                    fields.reverse();
                }
                ObjectType::new(object.name(), fields)
            })
            .collect::<Vec<_>>();
        if reverse_objects {
            objects.reverse();
        }
        let reordered = SchemaCatalog::try_new(objects).expect("reordering preserves validity");
        let insert = initial_version_insert(&reordered, VERSION_ID, APPLIED_AT);

        assert_eq!(version_content(&insert), version_content(&original));
    }
}

#[test]
fn initial_schema_version_snapshot_preserves_every_scalar_type() {
    for (scalar_type, name) in [
        (ScalarType::Str, "str"),
        (ScalarType::Int64, "int64"),
        (ScalarType::Float64, "float64"),
        (ScalarType::Bool, "bool"),
        (ScalarType::Uuid, "uuid"),
        (ScalarType::DateTime, "datetime"),
    ] {
        let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
            "Sample",
            vec![Field::Scalar(ScalarField::new(
                "value",
                scalar_type,
                SingleCardinality::Optional,
            ))],
        )])
        .expect("valid scalar schema");
        let insert = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);
        let expected = alloc::format!(
            "{{\"format_version\":1,\"objects\":[{{\"name\":\"Sample\",\"fields\":[{{\"name\":\"value\",\"kind\":\"scalar\",\"scalar_type\":\"{name}\",\"cardinality\":\"optional\",\"unique\":false}}]}}]}}"
        );

        assert_eq!(version_content(&insert).0, expected);
    }
}

#[test]
fn initial_schema_version_snapshot_sorts_names_by_utf8_without_normalizing() {
    // U+E000 sorts before U+10000 in UTF-8, unlike UTF-16 ordering.
    let catalog = SchemaCatalog::try_new(
        ["\u{10000}", "\u{e000}", "é", "e\u{301}", "a", "A"]
            .into_iter()
            .map(|name| ObjectType::new(name, vec![]))
            .collect(),
    )
    .expect("case and canonically equivalent Unicode names remain distinct");
    let insert = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);
    let expected = concat!(
        r#"{"format_version":1,"objects":[{"name":"A","fields":[]},"#,
        "{\"name\":\"a\",\"fields\":[]},{\"name\":\"e\u{301}\",\"fields\":[]},",
        "{\"name\":\"é\",\"fields\":[]},{\"name\":\"\u{e000}\",\"fields\":[]},",
        "{\"name\":\"\u{10000}\",\"fields\":[]}]}",
    );

    assert_eq!(version_content(&insert).0, expected);
}

#[test]
fn initial_schema_version_snapshot_escapes_names_and_link_references() {
    let name = "\0\u{1}\u{1f}\u{8}\t\n\u{c}\r\"\\/한글\u{2028}";
    let escaped = concat!(r#"\u0000\u0001\u001f\b\t\n\f\r\"\\/한글"#, "\u{2028}");
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        name,
        vec![
            Field::Link(LinkField::with_inverse(
                "back",
                name,
                Cardinality::Many,
                name,
            )),
            Field::Link(LinkField::new(name, name, Cardinality::Optional)),
        ],
    )])
    .expect("catalog names are not limited to parser identifiers");
    let insert = initial_version_insert(&catalog, VERSION_ID, APPLIED_AT);
    let expected = alloc::format!(
        "{{\"format_version\":1,\"objects\":[{{\"name\":\"{escaped}\",\"fields\":[{{\"name\":\"{escaped}\",\"kind\":\"link\",\"target_type\":\"{escaped}\",\"cardinality\":\"optional\",\"unique\":false,\"inverse_field\":null}},{{\"name\":\"back\",\"kind\":\"link\",\"target_type\":\"{escaped}\",\"cardinality\":\"many\",\"unique\":false,\"inverse_field\":\"{escaped}\"}}]}}]}}"
    );

    assert_eq!(version_content(&insert).0, expected);
}

#[test]
fn initial_schema_version_content_changes_with_scalar_semantics() {
    let catalog = |name, scalar_type, cardinality, uniqueness| {
        SchemaCatalog::try_new(vec![ObjectType::new(
            "Sample",
            vec![Field::Scalar(ScalarField::with_uniqueness(
                name,
                scalar_type,
                cardinality,
                uniqueness,
            ))],
        )])
        .expect("valid scalar variant")
    };
    let original = initial_version_insert(
        &catalog(
            "value",
            ScalarType::Str,
            SingleCardinality::Optional,
            Uniqueness::NotUnique,
        ),
        VERSION_ID,
        APPLIED_AT,
    );
    for (name, scalar_type, cardinality, uniqueness) in [
        (
            "renamed",
            ScalarType::Str,
            SingleCardinality::Optional,
            Uniqueness::NotUnique,
        ),
        (
            "value",
            ScalarType::Uuid,
            SingleCardinality::Optional,
            Uniqueness::NotUnique,
        ),
        (
            "value",
            ScalarType::Str,
            SingleCardinality::Required,
            Uniqueness::NotUnique,
        ),
        (
            "value",
            ScalarType::Str,
            SingleCardinality::Optional,
            Uniqueness::Unique,
        ),
    ] {
        let changed = initial_version_insert(
            &catalog(name, scalar_type, cardinality, uniqueness),
            VERSION_ID,
            APPLIED_AT,
        );
        assert_ne!(version_content(&original).0, version_content(&changed).0);
        assert_ne!(version_content(&original).1, version_content(&changed).1);
    }
}

#[test]
fn initial_schema_version_content_distinguishes_link_and_catalog_changes() {
    let fields = vec![
        vec![],
        vec![Field::Scalar(ScalarField::new(
            "value",
            ScalarType::Uuid,
            SingleCardinality::Optional,
        ))],
        vec![Field::Link(LinkField::new(
            "value",
            "A",
            Cardinality::Optional,
        ))],
        vec![Field::Link(LinkField::new(
            "value",
            "B",
            Cardinality::Optional,
        ))],
        vec![Field::Link(LinkField::new(
            "value",
            "A",
            Cardinality::Required,
        ))],
        vec![Field::Link(LinkField::new("value", "A", Cardinality::Many))],
        vec![Field::Link(LinkField::with_uniqueness(
            "value",
            "A",
            Cardinality::Optional,
            Uniqueness::Unique,
        ))],
        vec![Field::Link(LinkField::with_inverse(
            "value",
            "A",
            Cardinality::Many,
            "owner",
        ))],
        vec![Field::Link(LinkField::with_inverse(
            "value",
            "A",
            Cardinality::Many,
            "editor",
        ))],
    ];
    let mut variants = fields
        .into_iter()
        .map(|fields| {
            SchemaCatalog::try_new(vec![
                ObjectType::new("Root", fields),
                ObjectType::new(
                    "A",
                    vec![
                        Field::Link(LinkField::new("owner", "Root", Cardinality::Optional)),
                        Field::Link(LinkField::new("editor", "Root", Cardinality::Optional)),
                    ],
                ),
                ObjectType::new("B", vec![]),
            ])
            .expect("valid scalar, stored link, or inverse variant")
        })
        .collect::<Vec<_>>();
    let mut objects = variants[0].object_types().to_vec();
    objects.push(ObjectType::new("Extra", vec![]));
    variants.push(SchemaCatalog::try_new(objects).expect("valid added object"));
    let mut objects = variants[0].object_types().to_vec();
    objects[2] = ObjectType::new("Renamed", vec![]);
    variants.push(SchemaCatalog::try_new(objects).expect("valid renamed unreferenced object"));
    let inserts = variants
        .iter()
        .map(|catalog| initial_version_insert(catalog, VERSION_ID, APPLIED_AT))
        .collect::<Vec<_>>();

    for (index, insert) in inserts.iter().enumerate() {
        for other in &inserts[..index] {
            assert_ne!(version_content(insert).0, version_content(other).0);
            assert_ne!(version_content(insert).1, version_content(other).1);
        }
    }
}

#[test]
fn initial_schema_version_content_is_independent_of_application_values() {
    let catalog = version_catalog();
    let preview =
        initial_version_insert(&catalog, "<version-id-on-apply>", "<applied-at-on-apply>");
    assert_eq!(
        preview.values()[0],
        SQLiteValuePlan::Text("<version-id-on-apply>".into())
    );
    assert_eq!(
        preview.values()[2],
        SQLiteValuePlan::Text("<applied-at-on-apply>".into())
    );

    for (version_id, applied_at) in [
        (VERSION_ID, APPLIED_AT),
        ("d735740d-6058-4531-bd1c-b9b2c2a5ecfb", APPLIED_AT),
        (VERSION_ID, "2026-08-29T00:00:00.000Z"),
    ] {
        let insert = initial_version_insert(&catalog, version_id, applied_at);
        assert_eq!(insert.values()[0], SQLiteValuePlan::Text(version_id.into()));
        assert_eq!(insert.values()[2], SQLiteValuePlan::Text(applied_at.into()));
        assert_eq!(version_content(&insert), version_content(&preview));
        assert_eq!(
            insert.values(),
            initial_version_insert(&catalog, version_id, applied_at).values()
        );
    }
}
