#![no_std]

//! SQLite schema planning for Gelite.
//!
//! This crate will map a validated `schema_model::SchemaCatalog` to SQLite object
//! tables, relation tables, metadata tables, indexes, and catalog metadata
//! rows. It should stay independent from SQLite connection execution until the
//! schema planning API is tested.

extern crate alloc;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use schema_model::{Cardinality, Field, ObjectType, ScalarType, SchemaCatalog};
use serde::Serialize;
use sha2::{Digest, Sha256};

const SCHEMA_VERSIONS_TABLE: &str = "_engine_schema_versions";
const CATALOG_OBJECTS_TABLE: &str = "_engine_catalog_objects";
const CATALOG_FIELDS_TABLE: &str = "_engine_catalog_fields";

/// SQLite-specific plan for the first schema application step.
///
/// This type is intentionally structured instead of storing raw DDL strings.
/// Tests can inspect table, column, and constraint decisions before a later
/// renderer turns the plan into `CREATE TABLE` statements.
pub struct SQLiteSchemaPlan {
    metadata_tables: Vec<SQLiteTablePlan>,
    object_tables: Vec<SQLiteTablePlan>,
    relation_tables: Vec<SQLiteTablePlan>,
    indexes: Vec<SQLiteIndexPlan>,
    catalog_object_rows: Vec<SQLiteCatalogObjectRow>,
    catalog_field_rows: Vec<SQLiteCatalogFieldRow>,
    schema_versions_rows: Vec<SQLiteSchemaVersionRow>,
}

impl SQLiteSchemaPlan {
    pub fn metadata_tables(&self) -> &[SQLiteTablePlan] {
        &self.metadata_tables
    }

    pub fn object_tables(&self) -> &[SQLiteTablePlan] {
        &self.object_tables
    }

    pub fn relation_tables(&self) -> &[SQLiteTablePlan] {
        &self.relation_tables
    }

    pub fn catalog_object_rows(&self) -> &[SQLiteCatalogObjectRow] {
        &self.catalog_object_rows
    }

    pub fn catalog_field_rows(&self) -> &[SQLiteCatalogFieldRow] {
        &self.catalog_field_rows
    }

    pub fn indexes(&self) -> &[SQLiteIndexPlan] {
        &self.indexes
    }

    pub fn schema_versions_rows(&self) -> &[SQLiteSchemaVersionRow] {
        &self.schema_versions_rows
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SQLitePrimaryKeyPlan {
    column_names: Vec<String>,
}

impl SQLitePrimaryKeyPlan {
    pub fn new(column_names: Vec<String>) -> Self {
        Self { column_names }
    }

    pub fn column_names(&self) -> &[String] {
        &self.column_names
    }
}

/// Planned SQLite table definition before DDL rendering.
///
/// A table plan describes the physical table shape that should exist in
/// SQLite. It does not record whether the table came from engine metadata,
/// an object type, or a relation table; callers keep those groups separate in
/// the surrounding `SQLiteSchemaPlan`.
#[derive(Debug, PartialEq, Eq)]
pub struct SQLiteTablePlan {
    name: String,
    columns: Vec<SQLiteColumnPlan>,
    foreign_keys: Vec<SQLiteForeignKeyPlan>,
    primary_key: Option<SQLitePrimaryKeyPlan>,
}

impl SQLiteTablePlan {
    /// Creates a planned table with a deterministic table name and column list.
    pub fn new(name: impl Into<String>, columns: Vec<SQLiteColumnPlan>) -> Self {
        Self::new_with_foreign_keys(name, columns, Vec::new())
    }

    /// Creates a planned table with table-level foreign key constraints.
    pub fn new_with_foreign_keys(
        name: impl Into<String>,
        columns: Vec<SQLiteColumnPlan>,
        foreign_keys: Vec<SQLiteForeignKeyPlan>,
    ) -> Self {
        Self::new_with_constraints(name, columns, None, foreign_keys)
    }

    pub fn new_with_constraints(
        name: impl Into<String>,
        columns: Vec<SQLiteColumnPlan>,
        primary_key: Option<SQLitePrimaryKeyPlan>,
        foreign_keys: Vec<SQLiteForeignKeyPlan>,
    ) -> Self {
        Self {
            name: name.into(),
            columns,
            foreign_keys,
            primary_key,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn columns(&self) -> &[SQLiteColumnPlan] {
        &self.columns
    }

    pub fn foreign_keys(&self) -> &[SQLiteForeignKeyPlan] {
        &self.foreign_keys
    }

    pub fn primary_key(&self) -> Option<&SQLitePrimaryKeyPlan> {
        self.primary_key.as_ref()
    }
}

/// Builds the SQLite schema plan for applying a validated schema catalog to an
/// empty SQLite database.
/// Returns an error if the schema snapshot cannot be serialized as JSON.
pub fn plan_initial_schema(
    catalog: &SchemaCatalog,
    version_id: &str,
    applied_at: &str,
) -> Result<SQLiteSchemaPlan, serde_json::Error> {
    let metadata_tables = vec![
        SQLiteTablePlan::new(
            SCHEMA_VERSIONS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "version_id".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    true,
                    true,
                ),
                SQLiteColumnPlan::new(
                    "checksum".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "applied_at".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "schema_snapshot".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "version_number".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    true,
                ),
            ],
        ),
        SQLiteTablePlan::new(
            CATALOG_OBJECTS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "object_id".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    true,
                    true,
                ),
                SQLiteColumnPlan::new("name".to_string(), SQLiteAffinity::Text, false, false, true),
            ],
        ),
        SQLiteTablePlan::new_with_constraints(
            CATALOG_FIELDS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "object_id".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "field_id".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "name".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "field_kind".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "cardinality".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "scalar_type".to_string(),
                    SQLiteAffinity::Text,
                    true,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "target_object_id".to_string(),
                    SQLiteAffinity::Integer,
                    true,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "is_implicit".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "is_unique".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "inverse_field_name",
                    SQLiteAffinity::Text,
                    true,
                    false,
                    false,
                ),
            ],
            Some(SQLitePrimaryKeyPlan::new(vec![
                "object_id".to_string(),
                "field_id".to_string(),
            ])),
            vec![
                SQLiteForeignKeyPlan::new("object_id", CATALOG_OBJECTS_TABLE, "object_id"),
                SQLiteForeignKeyPlan::new("target_object_id", CATALOG_OBJECTS_TABLE, "object_id"),
            ],
        ),
    ];

    let object_tables = plan_objects(catalog);
    let relation_tables = plan_relation_tables(catalog);
    let catalog_object_rows = plan_catalog_object_rows(catalog);
    let catalog_field_rows = plan_catalog_field_rows(catalog);
    let indexes = plan_indexes(catalog);
    let schema_versions_rows = vec![plan_schema_version_row(catalog, version_id, applied_at, 1)?];

    Ok(SQLiteSchemaPlan {
        metadata_tables,
        object_tables,
        relation_tables,
        indexes,
        catalog_object_rows,
        catalog_field_rows,
        schema_versions_rows,
    })
}

fn plan_catalog_field_rows(catalog: &SchemaCatalog) -> Vec<SQLiteCatalogFieldRow> {
    let mut rows = Vec::new();

    for (object_index, object_type) in catalog.object_types().iter().enumerate() {
        let object_id = (object_index + 1) as i64;

        rows.push(SQLiteCatalogFieldRow::implicit_id(object_id));

        for (field_index, field) in object_type.declared_fields().iter().enumerate() {
            let field_id = (field_index + 2) as i64;

            match field {
                Field::Scalar(scalar) => {
                    rows.push(SQLiteCatalogFieldRow::scalar(
                        object_id,
                        field_id,
                        field.name().to_string(),
                        field.cardinality(),
                        scalar.scalar_type(),
                        scalar.is_unique(),
                    ));
                }
                Field::Link(link) => {
                    let target_object_id = catalog
                        .find_type_ref(link.target_type_name())
                        .expect("validated schema should only contain known link targets")
                        .id()
                        .value();

                    let mut row = SQLiteCatalogFieldRow::link(
                        object_id,
                        field_id,
                        field.name().to_string(),
                        field.cardinality(),
                        target_object_id,
                        link.is_unique(),
                    );
                    row.inverse_field_name = link.inverse_field_name().map(ToString::to_string);
                    rows.push(row);
                }
            }
        }
    }

    rows
}

fn plan_catalog_object_rows(catalog: &SchemaCatalog) -> Vec<SQLiteCatalogObjectRow> {
    catalog
        .object_types()
        .iter()
        .enumerate()
        .map(|(index, object_type)| {
            SQLiteCatalogObjectRow::new((index + 1) as i64, object_type.name())
        })
        .collect()
}

/// Converts planned catalog object rows into SQLite-facing insert plans.
///
/// This function does not inspect the original `SchemaCatalog`. It consumes the
/// object metadata already recorded in `SQLiteSchemaPlan` so the DML layer
/// cannot drift from the semantic rows tested earlier.
pub fn plan_catalog_object_inserts(plan: &SQLiteSchemaPlan) -> Vec<SQLiteInsertPlan> {
    plan.catalog_object_rows()
        .iter()
        .map(catalog_object_insert)
        .collect()
}

fn catalog_object_insert(row: &SQLiteCatalogObjectRow) -> SQLiteInsertPlan {
    SQLiteInsertPlan {
        table_name: CATALOG_OBJECTS_TABLE.to_string(),
        columns: vec!["object_id".to_string(), "name".to_string()],
        values: vec![
            SQLiteValuePlan::Integer(row.object_id()),
            SQLiteValuePlan::Text(row.name().to_string()),
        ],
    }
}

fn plan_objects(catalog: &SchemaCatalog) -> Vec<SQLiteTablePlan> {
    catalog
        .object_types()
        .iter()
        .map(|object_type| plan_object_table(object_type.name(), object_type.declared_fields()))
        .collect()
}

fn plan_object_table<'a>(
    object_name: &str,
    fields: impl IntoIterator<Item = &'a Field>,
) -> SQLiteTablePlan {
    let mut columns = vec![SQLiteColumnPlan::new(
        "id",
        SQLiteAffinity::Text,
        false,
        true,
        true,
    )];
    let mut foreign_keys = Vec::new();

    fields.into_iter().for_each(|field| {
        if let Some((column, foreign_key)) = plan_stored_field(field) {
            columns.push(column);
            foreign_keys.extend(foreign_key);
        }
    });

    SQLiteTablePlan::new_with_foreign_keys(object_name.to_ascii_lowercase(), columns, foreign_keys)
}

fn plan_stored_field(field: &Field) -> Option<(SQLiteColumnPlan, Option<SQLiteForeignKeyPlan>)> {
    let column_name = stored_column_name(field)?;

    match field {
        Field::Scalar(scalar) => Some((
            SQLiteColumnPlan::new(
                column_name,
                sqlite_affinity(scalar.scalar_type()),
                field.cardinality() != Cardinality::Required,
                false,
                scalar.is_unique(),
            ),
            None,
        )),
        Field::Link(link) if link.cardinality() != Cardinality::Many => Some((
            SQLiteColumnPlan::new(
                column_name.clone(),
                SQLiteAffinity::Text,
                field.cardinality() != Cardinality::Required,
                false,
                link.is_unique(),
            ),
            Some(SQLiteForeignKeyPlan::new(
                column_name,
                link.target_type_name().to_ascii_lowercase(),
                "id",
            )),
        )),
        Field::Link(_) => None,
    }
}

fn stored_column_name(field: &Field) -> Option<String> {
    match field {
        Field::Scalar(_) => Some(field.name().to_string()),
        Field::Link(link) if link.cardinality() != Cardinality::Many => {
            Some(format!("{}_id", field.name()))
        }
        Field::Link(_) => None,
    }
}

fn plan_relation_tables(catalog: &SchemaCatalog) -> Vec<SQLiteTablePlan> {
    catalog
        .object_types()
        .iter()
        .flat_map(|object_type| {
            object_type
                .declared_fields()
                .iter()
                .filter_map(|field| plan_relation_table(object_type.name(), field))
        })
        .collect()
}

fn plan_relation_table(object_name: &str, field: &Field) -> Option<SQLiteTablePlan> {
    let Field::Link(link) = field else {
        return None;
    };
    if link.cardinality() != Cardinality::Many || link.inverse_field_name().is_some() {
        return None;
    }

    let source_table = object_name.to_ascii_lowercase();
    let target_table = link.target_type_name().to_ascii_lowercase();
    Some(SQLiteTablePlan::new_with_constraints(
        format!("{}__{}", source_table, field.name()),
        vec![
            SQLiteColumnPlan::new("source_id", SQLiteAffinity::Text, false, false, false),
            SQLiteColumnPlan::new("target_id", SQLiteAffinity::Text, false, false, false),
            SQLiteColumnPlan::new("position", SQLiteAffinity::Integer, true, false, false),
        ],
        Some(SQLitePrimaryKeyPlan::new(vec![
            "source_id".to_string(),
            "target_id".to_string(),
        ])),
        vec![
            SQLiteForeignKeyPlan::new_with_on_delete(
                "source_id",
                source_table,
                "id",
                SQLiteForeignKeyAction::Cascade,
            ),
            SQLiteForeignKeyPlan::new_with_on_delete(
                "target_id",
                target_table,
                "id",
                SQLiteForeignKeyAction::Cascade,
            ),
        ],
    ))
}

fn sqlite_affinity(scalar_type: ScalarType) -> SQLiteAffinity {
    match scalar_type {
        ScalarType::Str => SQLiteAffinity::Text,
        ScalarType::Int64 => SQLiteAffinity::Integer,
        ScalarType::Float64 => SQLiteAffinity::Real,
        ScalarType::Bool => SQLiteAffinity::Integer,
        ScalarType::Uuid => SQLiteAffinity::Text,
        ScalarType::DateTime => SQLiteAffinity::Text,
    }
}

pub fn plan_catalog_field_inserts(plan: &SQLiteSchemaPlan) -> Vec<SQLiteInsertPlan> {
    plan.catalog_field_rows()
        .iter()
        .map(catalog_field_insert)
        .collect()
}

fn catalog_field_insert(row: &SQLiteCatalogFieldRow) -> SQLiteInsertPlan {
    SQLiteInsertPlan {
        table_name: CATALOG_FIELDS_TABLE.to_string(),
        columns: vec![
            "object_id".to_string(),
            "field_id".to_string(),
            "name".to_string(),
            "field_kind".to_string(),
            "cardinality".to_string(),
            "scalar_type".to_string(),
            "target_object_id".to_string(),
            "is_implicit".to_string(),
            "is_unique".to_string(),
            "inverse_field_name".to_string(),
        ],
        values: vec![
            SQLiteValuePlan::Integer(row.object_id()),
            SQLiteValuePlan::Integer(row.field_id()),
            SQLiteValuePlan::Text(row.name().to_string()),
            field_kind_value(row.field_kind()),
            cardinality_value(row.cardinality()),
            optional_scalar_type_value(row.scalar_type()),
            optional_i64_value(row.target_object_id()),
            bool_value(row.is_implicit()),
            bool_value(row.is_unique()),
            row.inverse_field_name
                .as_ref()
                .map_or(SQLiteValuePlan::Null, |name| {
                    SQLiteValuePlan::Text(name.clone())
                }),
        ],
    }
}

fn bool_value(value: bool) -> SQLiteValuePlan {
    if value {
        SQLiteValuePlan::Integer(1)
    } else {
        SQLiteValuePlan::Integer(0)
    }
}

fn field_kind_value(kind: SQLiteCatalogFieldKind) -> SQLiteValuePlan {
    match kind {
        SQLiteCatalogFieldKind::Scalar => SQLiteValuePlan::Text("scalar".to_string()),
        SQLiteCatalogFieldKind::Link => SQLiteValuePlan::Text("link".to_string()),
    }
}

fn optional_scalar_type_value(scalar_type: Option<ScalarType>) -> SQLiteValuePlan {
    match scalar_type {
        Some(ScalarType::Str) => SQLiteValuePlan::Text("str".to_string()),
        Some(ScalarType::Int64) => SQLiteValuePlan::Text("int64".to_string()),
        Some(ScalarType::Float64) => SQLiteValuePlan::Text("float64".to_string()),
        Some(ScalarType::Bool) => SQLiteValuePlan::Text("bool".to_string()),
        Some(ScalarType::Uuid) => SQLiteValuePlan::Text("uuid".to_string()),
        Some(ScalarType::DateTime) => SQLiteValuePlan::Text("datetime".to_string()),
        None => SQLiteValuePlan::Null,
    }
}

fn optional_i64_value(value: Option<i64>) -> SQLiteValuePlan {
    match value {
        Some(value) => SQLiteValuePlan::Integer(value),
        None => SQLiteValuePlan::Null,
    }
}

fn cardinality_value(cardinality: Cardinality) -> SQLiteValuePlan {
    match cardinality {
        Cardinality::Optional => SQLiteValuePlan::Text("optional".to_string()),
        Cardinality::Required => SQLiteValuePlan::Text("required".to_string()),
        Cardinality::Many => SQLiteValuePlan::Text("many".to_string()),
    }
}

fn plan_indexes(catalog: &SchemaCatalog) -> Vec<SQLiteIndexPlan> {
    catalog
        .object_types()
        .iter()
        .flat_map(|object_type| {
            object_type
                .declared_fields()
                .iter()
                .flat_map(|field| plan_field_indexes(object_type.name(), field))
        })
        .collect()
}

fn plan_field_indexes(object_name: &str, field: &Field) -> Vec<SQLiteIndexPlan> {
    let Field::Link(link) = field else {
        return Vec::new();
    };
    if link.inverse_field_name().is_some() {
        return Vec::new();
    }

    let table_name = object_name.to_ascii_lowercase();
    match link.cardinality() {
        Cardinality::Optional | Cardinality::Required => {
            let column_name = format!("{}_id", field.name());
            vec![SQLiteIndexPlan::new(
                format!("{}__{}_idx", table_name, column_name),
                table_name,
                vec![column_name],
                false,
            )]
        }
        Cardinality::Many => {
            let join_table_name = format!("{}__{}", table_name, field.name());
            vec![
                SQLiteIndexPlan::new(
                    format!("{}__source_id_idx", join_table_name),
                    join_table_name.clone(),
                    vec!["source_id".to_string()],
                    false,
                ),
                SQLiteIndexPlan::new(
                    format!("{}__target_id_idx", join_table_name),
                    join_table_name,
                    vec!["target_id".to_string()],
                    false,
                ),
            ]
        }
    }
}

/// SQLite type affinity used by physical column plans.
///
/// This is not the same as `ScalarType`. Several semantic scalar types
/// can share one SQLite affinity, such as `bool` and `int64` both mapping to
/// `INTEGER` in the storage spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteAffinity {
    Text,
    Integer,
    Real,
}

/// Planned SQLite column definition before DDL rendering.
///
/// The booleans model the constraints currently needed by the metadata table
/// contract. Foreign keys are intentionally not part of this type; they should
/// be modeled as table-level plans once the first foreign-key test is added.
#[derive(Debug, PartialEq, Eq)]
pub struct SQLiteColumnPlan {
    name: String,
    affinity: SQLiteAffinity,
    nullable: bool,
    primary_key: bool,
    unique: bool,
}

impl SQLiteColumnPlan {
    /// Creates a planned column with the constraints needed by the schema plan.
    pub fn new(
        name: impl Into<String>,
        affinity: SQLiteAffinity,
        nullable: bool,
        primary_key: bool,
        unique: bool,
    ) -> Self {
        Self {
            name: name.into(),
            affinity,
            nullable,
            primary_key,
            unique,
        }
    }

    pub fn affinity(&self) -> SQLiteAffinity {
        self.affinity
    }
    pub fn is_nullable(&self) -> bool {
        self.nullable
    }
    pub fn is_primary_key(&self) -> bool {
        self.primary_key
    }
    pub fn is_unique(&self) -> bool {
        self.unique
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Action applied when a referenced row is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteForeignKeyAction {
    Restrict,
    Cascade,
}

/// Planned table-level foreign key before DDL rendering.
#[derive(Debug, PartialEq, Eq)]
pub struct SQLiteForeignKeyPlan {
    column_name: String,
    target_table: String,
    target_column: String,
    on_delete: SQLiteForeignKeyAction,
}

impl SQLiteForeignKeyPlan {
    pub fn new(
        column_name: impl Into<String>,
        target_table: impl Into<String>,
        target_column: impl Into<String>,
    ) -> Self {
        Self::new_with_on_delete(
            column_name,
            target_table,
            target_column,
            SQLiteForeignKeyAction::Restrict,
        )
    }

    pub fn new_with_on_delete(
        column_name: impl Into<String>,
        target_table: impl Into<String>,
        target_column: impl Into<String>,
        on_delete: SQLiteForeignKeyAction,
    ) -> Self {
        Self {
            column_name: column_name.into(),
            target_table: target_table.into(),
            target_column: target_column.into(),
            on_delete,
        }
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn target_table(&self) -> &str {
        &self.target_table
    }

    pub fn target_column(&self) -> &str {
        &self.target_column
    }

    pub fn on_delete(&self) -> SQLiteForeignKeyAction {
        self.on_delete
    }
}

pub struct SQLiteCatalogObjectRow {
    object_id: i64,
    name: String,
}

impl SQLiteCatalogObjectRow {
    pub fn new(object_id: i64, name: impl Into<String>) -> Self {
        Self {
            object_id,
            name: name.into(),
        }
    }

    pub fn object_id(&self) -> i64 {
        self.object_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct SQLiteCatalogFieldRow {
    object_id: i64,
    field_id: i64,
    name: String,
    field_kind: SQLiteCatalogFieldKind,
    cardinality: Cardinality,
    scalar_type: Option<ScalarType>,
    target_object_id: Option<i64>,
    is_implicit: bool,
    is_unique: bool,
    inverse_field_name: Option<String>,
}

impl SQLiteCatalogFieldRow {
    pub fn implicit_id(object_id: i64) -> Self {
        Self {
            object_id,
            field_id: 1,
            name: "id".to_string(),
            field_kind: SQLiteCatalogFieldKind::Scalar,
            cardinality: Cardinality::Required,
            scalar_type: Some(ScalarType::Uuid),
            target_object_id: None,
            is_implicit: true,
            is_unique: false,
            inverse_field_name: None,
        }
    }

    pub fn scalar(
        object_id: i64,
        field_id: i64,
        name: impl Into<String>,
        cardinality: Cardinality,
        scalar_type: ScalarType,
        is_unique: bool,
    ) -> Self {
        Self {
            object_id,
            field_id,
            name: name.into(),
            field_kind: SQLiteCatalogFieldKind::Scalar,
            cardinality,
            scalar_type: Some(scalar_type),
            target_object_id: None,
            is_implicit: false,
            is_unique,
            inverse_field_name: None,
        }
    }

    pub fn link(
        object_id: i64,
        field_id: i64,
        name: impl Into<String>,
        cardinality: Cardinality,
        target_object_id: i64,
        is_unique: bool,
    ) -> Self {
        Self {
            object_id,
            field_id,
            name: name.into(),
            field_kind: SQLiteCatalogFieldKind::Link,
            cardinality,
            scalar_type: None,
            target_object_id: Some(target_object_id),
            is_implicit: false,
            is_unique,
            inverse_field_name: None,
        }
    }

    pub fn object_id(&self) -> i64 {
        self.object_id
    }

    pub fn field_id(&self) -> i64 {
        self.field_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn field_kind(&self) -> SQLiteCatalogFieldKind {
        self.field_kind
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }

    pub fn scalar_type(&self) -> Option<ScalarType> {
        self.scalar_type
    }

    pub fn target_object_id(&self) -> Option<i64> {
        self.target_object_id
    }

    pub fn is_implicit(&self) -> bool {
        self.is_implicit
    }

    pub fn is_unique(&self) -> bool {
        self.is_unique
    }
}

pub struct SQLiteSchemaVersionRow {
    version_id: String,
    checksum: String,
    applied_at: String,
    schema_snapshot: String,
    version_number: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum SnapshotScalarType {
    Str,
    Int64,
    Float64,
    Bool,
    Uuid,
    DateTime,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum SnapshotCardinality {
    Optional,
    Required,
    Many,
}

#[derive(Serialize)]
struct ScalarSnapshot<'a> {
    name: &'a str,
    kind: &'static str,
    scalar_type: SnapshotScalarType,
    cardinality: SnapshotCardinality,
    unique: bool,
}

#[derive(Serialize)]
struct LinkSnapshot<'a> {
    name: &'a str,
    kind: &'static str,
    #[serde(rename = "target_type")]
    target_type_name: &'a str,
    cardinality: SnapshotCardinality,
    unique: bool,
    #[serde(rename = "inverse_field")]
    inverse_field_name: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum FieldSnapshot<'a> {
    Link(LinkSnapshot<'a>),
    Scalar(ScalarSnapshot<'a>),
}

fn snapshot_field(field: &Field) -> FieldSnapshot<'_> {
    let name = field.name();
    let cardinality = match field.cardinality() {
        Cardinality::Optional => SnapshotCardinality::Optional,
        Cardinality::Required => SnapshotCardinality::Required,
        Cardinality::Many => SnapshotCardinality::Many,
    };

    match field {
        Field::Scalar(scalar) => FieldSnapshot::Scalar(ScalarSnapshot {
            name,
            kind: "scalar",
            scalar_type: match scalar.scalar_type() {
                ScalarType::Str => SnapshotScalarType::Str,
                ScalarType::Int64 => SnapshotScalarType::Int64,
                ScalarType::Float64 => SnapshotScalarType::Float64,
                ScalarType::Bool => SnapshotScalarType::Bool,
                ScalarType::Uuid => SnapshotScalarType::Uuid,
                ScalarType::DateTime => SnapshotScalarType::DateTime,
            },
            cardinality,
            unique: scalar.is_unique(),
        }),
        Field::Link(link) => FieldSnapshot::Link(LinkSnapshot {
            name,
            kind: "link",
            target_type_name: link.target_type_name(),
            cardinality,
            unique: link.is_unique(),
            inverse_field_name: link.inverse_field_name(),
        }),
    }
}

fn snapshot_fields(fields: &[Field]) -> Vec<FieldSnapshot<'_>> {
    let mut fields: Vec<_> = fields.iter().collect();
    fields.sort_by_key(|field| field.name());
    fields.into_iter().map(snapshot_field).collect()
}

#[derive(Serialize)]
struct ObjectSnapshot<'a> {
    name: &'a str,
    declared_fields: Vec<FieldSnapshot<'a>>,
    implicit_fields: Vec<FieldSnapshot<'a>>,
}

#[derive(Serialize)]
struct SchemaVersionSnapshot<'a> {
    format_version: u32,
    objects: Vec<ObjectSnapshot<'a>>,
}

fn snapshot_schema(catalog: &SchemaCatalog) -> SchemaVersionSnapshot<'_> {
    let mut object_snapshots: Vec<ObjectSnapshot> = catalog
        .object_types()
        .iter()
        .map(|object_type| ObjectSnapshot {
            name: object_type.name(),
            declared_fields: snapshot_fields(object_type.declared_fields()),
            implicit_fields: snapshot_fields(object_type.implicit_fields()),
        })
        .collect();
    object_snapshots.sort_by_key(|object| object.name);

    SchemaVersionSnapshot {
        format_version: 1,
        objects: object_snapshots,
    }
}

fn plan_schema_version_row(
    catalog: &SchemaCatalog,
    version_id: &str,
    applied_at: &str,
    version_number: i64,
) -> Result<SQLiteSchemaVersionRow, serde_json::Error> {
    let schema_snapshot = serialize_schema_snapshot(catalog)?;
    let checksum = schema_snapshot_checksum(&schema_snapshot);

    Ok(SQLiteSchemaVersionRow {
        version_number,
        version_id: version_id.to_string(),
        applied_at: applied_at.to_string(),
        schema_snapshot,
        checksum,
    })
}

/// Serializes a logical catalog using the canonical snapshot format, without application metadata.
pub fn serialize_schema_snapshot(catalog: &SchemaCatalog) -> Result<String, serde_json::Error> {
    serde_json::to_string(&snapshot_schema(catalog))
}

/// Returns lowercase SHA-256 of the exact snapshot bytes, without JSON normalization.
pub fn schema_snapshot_checksum(schema_snapshot: &str) -> String {
    Sha256::digest(schema_snapshot.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Kind of field recorded in the SQLite catalog metadata.
///
/// The enum stays separate from `schema_model::Field` because catalog rows store the
/// field kind as metadata, while the schema model stores the full field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteCatalogFieldKind {
    Scalar,
    Link,
}

/// SQLite-facing insert operation before SQL string rendering.
///
/// Insert plans fix the target table, column order, and bindable values while
/// still avoiding SQL string construction. The renderer can later serialize
/// this shape into `INSERT` statements with placeholders and bound values.
#[derive(Debug, PartialEq, Eq)]
pub struct SQLiteInsertPlan {
    table_name: String,
    columns: Vec<String>,
    values: Vec<SQLiteValuePlan>,
}

impl SQLiteInsertPlan {
    pub fn table_name(&self) -> &str {
        &self.table_name
    }
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
    pub fn values(&self) -> &[SQLiteValuePlan] {
        &self.values
    }
}

/// Value representation used by schema metadata insert plans.
///
/// This is intentionally smaller than SQLite's full runtime value model. It
/// only covers the metadata values emitted by `sqlite-schema-plan` before the
/// project adds an execution binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SQLiteValuePlan {
    Integer(i64),
    Text(String),
    Null,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SQLiteIndexPlan {
    name: String,
    table_name: String,
    column_names: Vec<String>,
    unique: bool,
}

impl SQLiteIndexPlan {
    pub fn new(
        name: impl Into<String>,
        table_name: impl Into<String>,
        column_names: Vec<String>,
        unique: bool,
    ) -> Self {
        Self {
            name: name.into(),
            table_name: table_name.into(),
            column_names,
            unique,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn column_names(&self) -> &[String] {
        &self.column_names
    }

    pub fn is_unique(&self) -> bool {
        self.unique
    }
}

/// Converts the initial plan's single version row into a list of bound INSERT plans.
pub fn plan_schema_version_insert(plan: &SQLiteSchemaPlan) -> Vec<SQLiteInsertPlan> {
    let [row] = plan.schema_versions_rows() else {
        unreachable!("initial schema planning produces exactly one version row");
    };

    vec![schema_version_insert(row)]
}

pub fn plan_schema_migration_version_insert(
    catalog: &SchemaCatalog,
    version_id: &str,
    applied_at: &str,
    version_number: i64,
) -> Result<SQLiteInsertPlan, serde_json::Error> {
    plan_schema_version_row(catalog, version_id, applied_at, version_number)
        .map(|row| schema_version_insert(&row))
}

fn schema_version_insert(row: &SQLiteSchemaVersionRow) -> SQLiteInsertPlan {
    SQLiteInsertPlan {
        table_name: SCHEMA_VERSIONS_TABLE.to_string(),
        columns: vec![
            "version_id".to_string(),
            "checksum".to_string(),
            "applied_at".to_string(),
            "schema_snapshot".to_string(),
            "version_number".to_string(),
        ],
        values: vec![
            SQLiteValuePlan::Text(row.version_id.clone()),
            SQLiteValuePlan::Text(row.checksum.clone()),
            SQLiteValuePlan::Text(row.applied_at.clone()),
            SQLiteValuePlan::Text(row.schema_snapshot.clone()),
            SQLiteValuePlan::Integer(row.version_number),
        ],
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SQLiteSchemaMigrationOperation {
    CreateTable(SQLiteTablePlan),
    AddColumn {
        table_name: String,
        column: SQLiteColumnPlan,
        foreign_key: Option<SQLiteForeignKeyPlan>,
    },
    CreateIndex(SQLiteIndexPlan),
    InsertMetadata(SQLiteInsertPlan),
}

pub struct SQLiteSchemaMigrationPlan {
    operations: Vec<SQLiteSchemaMigrationOperation>,
}

impl SQLiteSchemaMigrationPlan {
    pub fn operations(&self) -> &[SQLiteSchemaMigrationOperation] {
        &self.operations
    }
}

#[derive(Debug, PartialEq)]
pub enum SQLiteSchemaMigrationUnsupportedError {
    ObjectRemoval {
        object_type: String,
    },
    FieldRemoval {
        object_type: String,
        field: String,
    },
    FieldKindChange {
        object_type: String,
        field: String,
    },
    ScalarTypeChange {
        object_type: String,
        field: String,
    },
    LinkTargetChange {
        object_type: String,
        field: String,
    },
    CardinalityChange {
        object_type: String,
        field: String,
    },
    UniquenessChange {
        object_type: String,
        field: String,
    },
    RequiredFieldAddition {
        object_type: String,
        field: String,
    },
    UniqueFieldAddition {
        object_type: String,
        field: String,
    },
    InverseLinkChange {
        object_type: String,
        field: String,
    },
    PhysicalColumnNameCollision {
        object_type: String,
        field: String,
        column: String,
    },
}

pub fn plan_schema_migration(
    current: &SchemaCatalog,
    desired: &SchemaCatalog,
) -> Result<SQLiteSchemaMigrationPlan, SQLiteSchemaMigrationUnsupportedError> {
    validate_schema_migration(current, desired)?;

    let new_objects = sorted_objects(desired)
        .into_iter()
        .filter(|object| current.find_type(object.name()).is_none())
        .collect::<Vec<_>>();
    let mut object_tables = Vec::new();
    let mut relation_tables = Vec::new();
    let mut columns = Vec::new();
    let mut indexes = Vec::new();
    let mut object_metadata = Vec::new();
    let mut field_metadata = Vec::new();

    new_objects
        .iter()
        .enumerate()
        .for_each(|(object_index, object)| {
            let fields = sorted_fields(object);
            let object_id = current.object_types().len() as i64 + object_index as i64 + 1;

            object_tables.push(SQLiteSchemaMigrationOperation::CreateTable(
                plan_object_table(object.name(), fields.iter().copied()),
            ));
            relation_tables.extend(fields.iter().filter_map(|field| {
                plan_relation_table(object.name(), field)
                    .map(SQLiteSchemaMigrationOperation::CreateTable)
            }));
            indexes.extend(
                fields
                    .iter()
                    .flat_map(|field| plan_field_indexes(object.name(), field))
                    .map(SQLiteSchemaMigrationOperation::CreateIndex),
            );
            object_metadata.push(SQLiteSchemaMigrationOperation::InsertMetadata(
                catalog_object_insert(&SQLiteCatalogObjectRow::new(object_id, object.name())),
            ));
            field_metadata.push(SQLiteSchemaMigrationOperation::InsertMetadata(
                catalog_field_insert(&SQLiteCatalogFieldRow::implicit_id(object_id)),
            ));
            field_metadata.extend(fields.iter().enumerate().map(|(field_index, field)| {
                SQLiteSchemaMigrationOperation::InsertMetadata(catalog_field_insert(
                    &migration_field_row(
                        current,
                        &new_objects,
                        object_id,
                        field_index as i64 + 2,
                        field,
                    ),
                ))
            }));
        });

    sorted_objects(desired)
        .into_iter()
        .filter_map(|desired_object| {
            current
                .find_type(desired_object.name())
                .map(|current_object| (current_object, desired_object))
        })
        .for_each(|(current_object, desired_object)| {
            sorted_fields(desired_object)
                .into_iter()
                .filter(|field| current_object.find_declared_field(field.name()).is_none())
                .enumerate()
                .for_each(|(field_index, field)| {
                    if let Some((column, foreign_key)) = plan_stored_field(field) {
                        columns.push(SQLiteSchemaMigrationOperation::AddColumn {
                            table_name: desired_object.name().to_ascii_lowercase(),
                            column,
                            foreign_key,
                        });
                    }
                    if let Some(table) = plan_relation_table(desired_object.name(), field) {
                        relation_tables.push(SQLiteSchemaMigrationOperation::CreateTable(table));
                    }
                    indexes.extend(
                        plan_field_indexes(desired_object.name(), field)
                            .into_iter()
                            .map(SQLiteSchemaMigrationOperation::CreateIndex),
                    );

                    let object_id = current
                        .find_type_ref(current_object.name())
                        .expect("current object came from the current catalog")
                        .id()
                        .value();
                    let field_id =
                        current_object.declared_fields().len() as i64 + field_index as i64 + 2;
                    field_metadata.push(SQLiteSchemaMigrationOperation::InsertMetadata(
                        catalog_field_insert(&migration_field_row(
                            current,
                            &new_objects,
                            object_id,
                            field_id,
                            field,
                        )),
                    ));
                });
        });

    Ok(SQLiteSchemaMigrationPlan {
        operations: object_tables
            .into_iter()
            .chain(relation_tables)
            .chain(columns)
            .chain(indexes)
            .chain(object_metadata)
            .chain(field_metadata)
            .collect(),
    })
}

fn validate_schema_migration(
    current: &SchemaCatalog,
    desired: &SchemaCatalog,
) -> Result<(), SQLiteSchemaMigrationUnsupportedError> {
    sorted_objects(current)
        .into_iter()
        .try_for_each(|current_object| {
            let desired_object = desired.find_type(current_object.name()).ok_or_else(|| {
                SQLiteSchemaMigrationUnsupportedError::ObjectRemoval {
                    object_type: current_object.name().to_string(),
                }
            })?;

            sorted_fields(current_object)
                .into_iter()
                .try_for_each(|current_field| {
                    let desired_field = desired_object
                        .find_declared_field(current_field.name())
                        .ok_or_else(|| SQLiteSchemaMigrationUnsupportedError::FieldRemoval {
                            object_type: current_object.name().to_string(),
                            field: current_field.name().to_string(),
                        })?;
                    validate_existing_field(current_object.name(), current_field, desired_field)
                })
        })?;

    sorted_objects(desired)
        .into_iter()
        .try_for_each(|desired_object| {
            let current_object = current.find_type(desired_object.name());

            sorted_fields(desired_object)
                .into_iter()
                .filter(|field| {
                    current_object
                        .is_none_or(|object| object.find_declared_field(field.name()).is_none())
                })
                .try_for_each(|field| {
                    if current_object.is_some() {
                        validate_field_addition(desired_object.name(), field)?;
                    }
                    validate_added_column_name(desired_object, field)
                })
        })
}

fn validate_added_column_name(
    object: &ObjectType,
    field: &Field,
) -> Result<(), SQLiteSchemaMigrationUnsupportedError> {
    let Some(column) = stored_column_name(field) else {
        return Ok(());
    };
    if !object.declared_fields().iter().any(|other| {
        other.name() != field.name()
            && stored_column_name(other).as_deref() == Some(column.as_str())
    }) {
        return Ok(());
    }

    Err(
        SQLiteSchemaMigrationUnsupportedError::PhysicalColumnNameCollision {
            object_type: object.name().to_string(),
            field: field.name().to_string(),
            column,
        },
    )
}

fn validate_existing_field(
    object_type: &str,
    current: &Field,
    desired: &Field,
) -> Result<(), SQLiteSchemaMigrationUnsupportedError> {
    let field = current.name().to_string();
    let object_type = object_type.to_string();

    match (current, desired) {
        (Field::Scalar(current), Field::Scalar(desired)) => {
            if current.scalar_type() != desired.scalar_type() {
                return Err(SQLiteSchemaMigrationUnsupportedError::ScalarTypeChange {
                    object_type,
                    field,
                });
            }
        }
        (Field::Link(current), Field::Link(desired)) => {
            if current.target_type_name() != desired.target_type_name() {
                return Err(SQLiteSchemaMigrationUnsupportedError::LinkTargetChange {
                    object_type,
                    field,
                });
            }
        }
        _ => {
            return Err(SQLiteSchemaMigrationUnsupportedError::FieldKindChange {
                object_type,
                field,
            });
        }
    }

    if current.cardinality() != desired.cardinality() {
        return Err(SQLiteSchemaMigrationUnsupportedError::CardinalityChange {
            object_type,
            field,
        });
    }
    if field_is_unique(current) != field_is_unique(desired) {
        return Err(SQLiteSchemaMigrationUnsupportedError::UniquenessChange { object_type, field });
    }
    if let (Field::Link(current), Field::Link(desired)) = (current, desired)
        && current.inverse_field_name() != desired.inverse_field_name()
    {
        return Err(SQLiteSchemaMigrationUnsupportedError::InverseLinkChange {
            object_type,
            field,
        });
    }

    Ok(())
}

fn validate_field_addition(
    object_type: &str,
    field: &Field,
) -> Result<(), SQLiteSchemaMigrationUnsupportedError> {
    if field.cardinality() == Cardinality::Required {
        return Err(
            SQLiteSchemaMigrationUnsupportedError::RequiredFieldAddition {
                object_type: object_type.to_string(),
                field: field.name().to_string(),
            },
        );
    }
    if field_is_unique(field) {
        return Err(SQLiteSchemaMigrationUnsupportedError::UniqueFieldAddition {
            object_type: object_type.to_string(),
            field: field.name().to_string(),
        });
    }
    Ok(())
}

fn field_is_unique(field: &Field) -> bool {
    match field {
        Field::Scalar(field) => field.is_unique(),
        Field::Link(field) => field.is_unique(),
    }
}

fn sorted_objects(catalog: &SchemaCatalog) -> Vec<&ObjectType> {
    let mut objects = catalog.object_types().iter().collect::<Vec<_>>();
    objects.sort_unstable_by(|left, right| left.name().cmp(right.name()));
    objects
}

fn sorted_fields(object: &ObjectType) -> Vec<&Field> {
    let mut fields = object.declared_fields().iter().collect::<Vec<_>>();
    fields.sort_unstable_by(|left, right| left.name().cmp(right.name()));
    fields
}

fn migration_object_id(
    current: &SchemaCatalog,
    new_objects: &[&ObjectType],
    object_name: &str,
) -> i64 {
    if let Some(object) = current.find_type_ref(object_name) {
        return object.id().value();
    }

    current.object_types().len() as i64
        + new_objects
            .iter()
            .position(|object| object.name() == object_name)
            .expect("desired link target should exist among new objects") as i64
        + 1
}

fn migration_field_row(
    current: &SchemaCatalog,
    new_objects: &[&ObjectType],
    object_id: i64,
    field_id: i64,
    field: &Field,
) -> SQLiteCatalogFieldRow {
    match field {
        Field::Scalar(scalar) => SQLiteCatalogFieldRow::scalar(
            object_id,
            field_id,
            field.name(),
            field.cardinality(),
            scalar.scalar_type(),
            scalar.is_unique(),
        ),
        Field::Link(link) => {
            let mut row = SQLiteCatalogFieldRow::link(
                object_id,
                field_id,
                field.name(),
                field.cardinality(),
                migration_object_id(current, new_objects, link.target_type_name()),
                link.is_unique(),
            );
            row.inverse_field_name = link.inverse_field_name().map(ToString::to_string);
            row
        }
    }
}

#[cfg(test)]
mod tests;
