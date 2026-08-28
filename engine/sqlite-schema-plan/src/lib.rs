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
use schema_model::{Cardinality, Field, ScalarType, SchemaCatalog};
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
    let schema_versions_rows = plan_schema_version_rows(catalog, version_id, applied_at)?;

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
        .map(|row| SQLiteInsertPlan {
            table_name: CATALOG_OBJECTS_TABLE.to_string(),
            columns: vec!["object_id".to_string(), "name".to_string()],
            values: vec![
                SQLiteValuePlan::Integer(row.object_id()),
                SQLiteValuePlan::Text(row.name().to_string()),
            ],
        })
        .collect()
}

fn plan_objects(catalog: &SchemaCatalog) -> Vec<SQLiteTablePlan> {
    catalog
        .object_types()
        .iter()
        .map(|object_type| {
            let declared_fields = object_type.declared_fields();
            let mut columns = vec![SQLiteColumnPlan::new(
                "id",
                SQLiteAffinity::Text,
                false,
                true,
                true,
            )];

            columns.extend(declared_fields.iter().filter_map(|field| match field {
                Field::Scalar(scalar) => Some(SQLiteColumnPlan::new(
                    field.name(),
                    sqlite_affinity(scalar.scalar_type()),
                    field.cardinality() != Cardinality::Required,
                    false,
                    scalar.is_unique(),
                )),
                Field::Link(link) => match link.cardinality() {
                    Cardinality::Many => None,
                    Cardinality::Optional | Cardinality::Required => Some(SQLiteColumnPlan::new(
                        format!("{}_id", field.name()),
                        SQLiteAffinity::Text,
                        field.cardinality() != Cardinality::Required,
                        false,
                        link.is_unique(),
                    )),
                },
            }));

            let foreign_keys = declared_fields
                .iter()
                .filter_map(|field| match field {
                    Field::Scalar(_) => None,
                    Field::Link(link) => match link.cardinality() {
                        Cardinality::Many => None,
                        Cardinality::Optional | Cardinality::Required => {
                            Some(SQLiteForeignKeyPlan::new(
                                format!("{}_id", field.name()),
                                link.target_type_name().to_ascii_lowercase(),
                                "id",
                            ))
                        }
                    },
                })
                .collect();

            SQLiteTablePlan::new_with_foreign_keys(
                object_type.name().to_ascii_lowercase(),
                columns,
                foreign_keys,
            )
        })
        .collect()
}

fn plan_relation_tables(catalog: &SchemaCatalog) -> Vec<SQLiteTablePlan> {
    catalog
        .object_types()
        .iter()
        .flat_map(|object_type| {
            object_type
                .declared_fields()
                .iter()
                .filter_map(|field| match field {
                    Field::Scalar(_) => None,
                    Field::Link(link)
                        if link.cardinality() == Cardinality::Many
                            && link.inverse_field_name().is_none() =>
                    {
                        let source_table = object_type.name().to_ascii_lowercase();
                        let target_table = link.target_type_name().to_ascii_lowercase();
                        Some(SQLiteTablePlan::new_with_constraints(
                            format!("{}__{}", source_table, field.name()),
                            vec![
                                SQLiteColumnPlan::new(
                                    "source_id",
                                    SQLiteAffinity::Text,
                                    false,
                                    false,
                                    false,
                                ),
                                SQLiteColumnPlan::new(
                                    "target_id",
                                    SQLiteAffinity::Text,
                                    false,
                                    false,
                                    false,
                                ),
                                SQLiteColumnPlan::new(
                                    "position",
                                    SQLiteAffinity::Integer,
                                    true,
                                    false,
                                    false,
                                ),
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
                    Field::Link(_) => None,
                })
        })
        .collect()
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
        .map(|row| SQLiteInsertPlan {
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
        })
        .collect()
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
    let mut indexes = Vec::new();

    for object_type in catalog.object_types() {
        let table_name = object_type.name().to_ascii_lowercase();

        for field in object_type.declared_fields() {
            let Field::Link(link) = field else {
                continue;
            };

            if link.inverse_field_name().is_some() {
                continue;
            }

            match link.cardinality() {
                Cardinality::Optional | Cardinality::Required => {
                    let column_name = format!("{}_id", field.name());
                    let index_name = format!("{}__{}_idx", table_name, column_name);

                    indexes.push(SQLiteIndexPlan::new(
                        index_name,
                        table_name.clone(),
                        vec![column_name],
                        false,
                    ));
                }
                Cardinality::Many => {
                    let join_table_name = format!("{}__{}", table_name, field.name());

                    indexes.push(SQLiteIndexPlan::new(
                        format!("{}__source_id_idx", join_table_name),
                        join_table_name.clone(),
                        vec!["source_id".to_string()],
                        false,
                    ));

                    indexes.push(SQLiteIndexPlan::new(
                        format!("{}__target_id_idx", join_table_name),
                        join_table_name,
                        vec!["target_id".to_string()],
                        false,
                    ));
                }
            }
        }
    }

    indexes
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

fn plan_schema_version_rows(
    catalog: &SchemaCatalog,
    version_id: &str,
    applied_at: &str,
) -> Result<Vec<SQLiteSchemaVersionRow>, serde_json::Error> {
    let schema_snapshot = serde_json::to_string(&snapshot_schema(catalog))?;
    let checksum = Sha256::digest(schema_snapshot.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    Ok(vec![SQLiteSchemaVersionRow {
        version_id: version_id.to_string(),
        applied_at: applied_at.to_string(),
        schema_snapshot,
        checksum,
    }])
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

/// Converts the initial plan's single version row into a bound INSERT plan.
pub fn plan_schema_version_insert(plan: &SQLiteSchemaPlan) -> SQLiteInsertPlan {
    let [row] = plan.schema_versions_rows() else {
        unreachable!("initial schema planning produces exactly one version row");
    };

    SQLiteInsertPlan {
        table_name: SCHEMA_VERSIONS_TABLE.to_string(),
        columns: vec![
            "version_id".to_string(),
            "checksum".to_string(),
            "applied_at".to_string(),
            "schema_snapshot".to_string(),
        ],
        values: vec![
            SQLiteValuePlan::Text(row.version_id.clone()),
            SQLiteValuePlan::Text(row.checksum.clone()),
            SQLiteValuePlan::Text(row.applied_at.clone()),
            SQLiteValuePlan::Text(row.schema_snapshot.clone()),
        ],
    }
}

#[cfg(test)]
mod tests;
