use rusqlite::{
    Connection, Row, params_from_iter,
    types::{Value, ValueRef},
};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality, Uniqueness,
};
use sqlite_schema_plan::{SQLiteValuePlan, schema_snapshot_checksum, serialize_schema_snapshot};
use std::time::Duration;

use crate::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteQueryRunner, SQLiteRunner, SQLiteRunnerError,
    SQLiteSchemaReader, SQLiteStoredSchema, SQLiteTransactionRunner,
};

/// Native SQLite runner backed by an owned SQLite connection.
///
/// The concrete SQLite binding stays private to this module. Public planner,
/// SQL generator, and command APIs should continue to depend on the
/// `SQLiteRunner` trait instead of this backend type.
pub struct NativeSQLiteRunner {
    connection: Connection,
}

impl NativeSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError> {
        Self::open(":memory:")
    }

    pub fn open(path: &str) -> Result<Self, SQLiteRunnerError> {
        let connection = Connection::open(path).map_err(|error| {
            SQLiteRunnerError::execution_failed(format!(
                "failed to open SQLite database `{path}`: {error}"
            ))
        })?;
        connection
            .busy_timeout(Duration::ZERO)
            .map_err(|error| sqlite_error("configure SQLite busy timeout", error))?;
        let mut runner = Self { connection };
        runner.execute("PRAGMA foreign_keys = ON")?;

        Ok(runner)
    }

    pub fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("BEGIN")
    }

    pub fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("COMMIT")
    }

    pub fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("ROLLBACK")
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool, SQLiteRunnerError> {
        let mut statement = self
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .map_err(|error| sqlite_error("prepare table existence query", error))?;

        statement
            .exists([table_name])
            .map_err(|error| sqlite_error("step table existence query", error))
    }

    /// Reads the first row as owned values for native backend smoke tests.
    ///
    /// This is not the query execution API. It exists only to verify that the
    /// selected SQLite binding stores values through `SQLiteRunner` correctly.
    pub fn first_three_column_row(
        &self,
        sql: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| sqlite_error("prepare read-back query", error))?;
        let mut rows = statement
            .query([])
            .map_err(|error| sqlite_error("step read-back query", error))?;

        match rows
            .next()
            .map_err(|error| sqlite_error("step read-back query", error))?
        {
            Some(row) => {
                let first = read_integer_column(row, 0, "read integer column")?;
                let second = read_text_column(row, 1, "read text column")?;
                let third = read_nullable_integer_column(row, 2, "read nullable integer column")?;

                Ok(Some((first, second, third)))
            }
            None => Ok(None),
        }
    }

    pub fn load_verified_schema(
        &mut self,
    ) -> Result<Option<SQLiteStoredSchema>, SQLiteRunnerError> {
        let metadata_tables = [
            "_engine_schema_versions",
            "_engine_catalog_objects",
            "_engine_catalog_fields",
        ];
        let existing = metadata_tables
            .iter()
            .map(|table| self.table_exists(table))
            .collect::<Result<Vec<_>, _>>()?;
        if existing.iter().all(|exists| !exists) {
            return Ok(None);
        }
        if existing.iter().any(|exists| !exists) {
            return Err(SQLiteRunnerError::schema_verification_failed(
                "database contains partial engine schema metadata",
            ));
        }

        self.begin_transaction()?;
        let result = (|| {
            let last_schema = self.read_latest_schema_version()?.ok_or_else(|| {
                SQLiteRunnerError::schema_verification_failed(
                    "database does not contain a stored schema version",
                )
            })?;
            if schema_snapshot_checksum(&last_schema.schema_snapshot) != last_schema.checksum {
                return Err(SQLiteRunnerError::schema_verification_failed(
                    "stored schema snapshot checksum mismatch",
                ));
            }

            let catalog = self.load_schema_catalog()?;
            let snapshot = serialize_schema_snapshot(&catalog).map_err(|error| {
                SQLiteRunnerError::schema_verification_failed(format!(
                    "failed to serialize stored schema catalog: {error}",
                ))
            })?;
            if snapshot != last_schema.schema_snapshot {
                return Err(SQLiteRunnerError::schema_verification_failed(
                    "stored schema snapshot does not match the canonical logical catalog",
                ));
            }
            let stored_schema = SQLiteStoredSchema {
                catalog,
                version_number: last_schema.version_number,
            };
            self.commit_transaction()?;
            Ok(Some(stored_schema))
        })();

        result.map_err(|mut error: SQLiteRunnerError| {
            if let Err(rollback_error) = self.rollback_transaction() {
                let message = match &mut error {
                    SQLiteRunnerError::ExecutionFailed { message }
                    | SQLiteRunnerError::SchemaVerificationFailed { message } => message,
                };
                message.push_str(&format!("; rollback failed: {}", rollback_error.message()));
            }
            error
        })
    }

    /// Verifies the latest snapshot checksum and logical catalog in one read transaction.
    ///
    /// No source file is needed. An existing caller transaction is rejected and left untouched.
    pub fn verify_schema_version(&mut self) -> Result<(), SQLiteRunnerError> {
        self.load_verified_schema()?
            .ok_or_else(|| {
                SQLiteRunnerError::schema_verification_failed(
                    "database does not contain a stored schema version",
                )
            })
            .map(|_| ())
    }

    pub fn load_schema_catalog(&self) -> Result<SchemaCatalog, SQLiteRunnerError> {
        let objects = self.read_catalog_objects()?;
        let fields = self.read_catalog_fields()?;
        if fields.iter().any(|field| {
            !objects
                .iter()
                .any(|object| object.object_id == field.object_id)
        }) {
            return Err(SQLiteRunnerError::execution_failed(
                "catalog field references an unknown owner object",
            ));
        }
        if fields.iter().any(|field| {
            field.inverse_field_name.is_some()
                && (field.field_kind != "link" || field.is_implicit || field.is_unique)
        }) {
            return Err(SQLiteRunnerError::execution_failed(
                "invalid inverse field metadata",
            ));
        }
        if fields.iter().any(|field| {
            (field.field_kind == "scalar" && field.target_object_id.is_some())
                || (field.field_kind == "link" && field.scalar_type.is_some())
        }) {
            return Err(SQLiteRunnerError::execution_failed(
                "catalog field contains metadata for a different field kind",
            ));
        }

        let mut object_types = Vec::new();
        for object in &objects {
            let mut implicit = fields
                .iter()
                .filter(|field| field.object_id == object.object_id && field.is_implicit);
            let valid_id = implicit.next().is_some_and(|field| {
                field.name == "id"
                    && field.field_kind == "scalar"
                    && field.scalar_type.as_deref() == Some("uuid")
                    && field.cardinality == "required"
                    && !field.is_unique
                    && field.target_object_id.is_none()
                    && field.inverse_field_name.is_none()
            });
            if !valid_id || implicit.next().is_some() {
                return Err(SQLiteRunnerError::execution_failed(format!(
                    "catalog object `{}` must contain exactly one implicit UUID id field",
                    object.name,
                )));
            }
            let declared_fields = fields
                .iter()
                .filter(|field| field.object_id == object.object_id && !field.is_implicit)
                .map(|field| field_from_catalog_row(field, &objects))
                .collect::<Result<Vec<_>, _>>()?;

            object_types.push(ObjectType::new(object.name.clone(), declared_fields));
        }

        SchemaCatalog::try_new(object_types).map_err(|error| {
            SQLiteRunnerError::execution_failed(format!("invalid catalog metadata: {error:?}"))
        })
    }

    pub fn execute_select(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        let mut prepared = self
            .connection
            .prepare(statement.sql())
            .map_err(|error| sqlite_error("prepare SELECT", error))?;

        let column_count = prepared.column_count();
        let output_names = statement.output_names();

        if !output_names.is_empty() && output_names.len() != column_count {
            return Err(SQLiteRunnerError::execution_failed(
                "result output metadata does not match SQLite column count",
            ));
        }

        let result_shape = statement.result_shape();
        let (column_indexes, columns): (Vec<_>, Vec<_>) = if result_shape.is_some() {
            ((0..column_count).collect(), Vec::new())
        } else {
            let selected_columns: Vec<(usize, String)> = if output_names.is_empty() {
                (0..column_count)
                    .map(|index| {
                        prepared
                            .column_name(index)
                            .map(|name| (index, name.to_string()))
                            .map_err(|error| sqlite_error("read result column name", error))
                    })
                    .collect::<Result<_, _>>()?
            } else {
                (0..column_count)
                    .zip(output_names)
                    .filter_map(|(index, name)| name.as_ref().map(|name| (index, name.clone())))
                    .collect()
            };

            selected_columns.into_iter().unzip()
        };

        let mut rows = Vec::new();
        let values = complete_bind_values(
            query_bind_values(statement.bind_values()),
            prepared.parameter_count(),
        );
        let mut result_rows = prepared
            .query(params_from_iter(values))
            .map_err(|error| sqlite_error("step SELECT", error))?;
        while let Some(result_row) = result_rows
            .next()
            .map_err(|error| sqlite_error("step SELECT", error))?
        {
            rows.push(
                column_indexes
                    .iter()
                    .map(|index| read_cell_value(result_row, *index))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        match result_shape {
            Some(shape) => {
                let follow_up_fetch_count = crate::follow_up_fetch_count(shape);
                let shaped = rows
                    .into_iter()
                    .map(|mut row| {
                        let parent_identity =
                            crate::identity_at(statement.parent_identity_column_index(), &row)?;
                        let mut follow_up_parent_identities = vec![None; follow_up_fetch_count];
                        let fields = crate::shape_fields_with_identities(
                            shape,
                            &mut row,
                            &mut follow_up_parent_identities,
                        )?;

                        Ok((fields, parent_identity, follow_up_parent_identities))
                    })
                    .collect::<Result<Vec<_>, SQLiteRunnerError>>()?;
                let mut result_rows = Vec::with_capacity(shaped.len());
                let mut parent_identities = Vec::with_capacity(shaped.len());
                let mut follow_up_parent_identities = Vec::with_capacity(shaped.len());
                for (row, parent_identity, row_follow_up_parent_identities) in shaped {
                    result_rows.push(row);
                    parent_identities.push(parent_identity);
                    follow_up_parent_identities.push(row_follow_up_parent_identities);
                }

                Ok(SQLiteQueryResult::with_identities(
                    shape
                        .fields()
                        .iter()
                        .map(|field| field.output_name().into())
                        .collect(),
                    result_rows,
                    parent_identities,
                    follow_up_parent_identities,
                ))
            }
            None => Ok(SQLiteQueryResult::new(columns, rows)),
        }
    }

    pub fn execute_insert(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<(), SQLiteRunnerError> {
        self.execute_query_statement(statement, "INSERT")
            .map(|_| ())
    }

    pub fn execute_update(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        self.execute_mutation(statement, "UPDATE")
    }

    pub fn execute_delete(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        self.execute_mutation(statement, "DELETE")
    }

    fn execute_mutation(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
        operation: &str,
    ) -> Result<i64, SQLiteRunnerError> {
        let count = self.execute_query_statement(statement, operation)?;
        i64::try_from(count).map_err(|_| {
            SQLiteRunnerError::execution_failed(format!(
                "step {operation}: affected row count exceeds i64 range"
            ))
        })
    }

    fn execute_query_statement(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
        operation: &str,
    ) -> Result<usize, SQLiteRunnerError> {
        let mut prepared = self
            .connection
            .prepare(statement.sql())
            .map_err(|error| sqlite_error(&format!("prepare {operation}"), error))?;
        let values = complete_bind_values(
            query_bind_values(statement.bind_values()),
            prepared.parameter_count(),
        );
        let count = prepared
            .execute(params_from_iter(values))
            .map_err(|error| sqlite_error(&format!("step {operation}"), error))?;
        Ok(count)
    }

    /// Reads the highest numbered stored version without verifying its contents.
    pub(crate) fn read_latest_schema_version(
        &self,
    ) -> Result<Option<SchemaVersionRow>, SQLiteRunnerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT version_id, checksum, applied_at, schema_snapshot, version_number
                 FROM _engine_schema_versions ORDER BY version_number DESC LIMIT 1",
            )
            .map_err(|error| sqlite_error("prepare schema version query", error))?;
        let mut rows = statement
            .query([])
            .map_err(|error| sqlite_error("step schema version query", error))?;
        match rows
            .next()
            .map_err(|error| sqlite_error("step schema version query", error))?
        {
            Some(row) => {
                let version_number = read_nullable_integer_column(row, 4, "read version number")?
                    .filter(|number| *number > 0)
                    .ok_or_else(|| {
                        SQLiteRunnerError::schema_verification_failed(
                            "stored schema version number must be positive",
                        )
                    })?;
                Ok(Some(SchemaVersionRow {
                    version_id: read_text_column(row, 0, "read schema version id")?,
                    checksum: read_text_column(row, 1, "read schema version checksum")?,
                    applied_at: read_text_column(row, 2, "read schema version applied_at")?,
                    schema_snapshot: read_text_column(row, 3, "read schema version snapshot")?,
                    version_number,
                }))
            }
            None => Ok(None),
        }
    }

    fn read_catalog_objects(&self) -> Result<Vec<CatalogObjectRow>, SQLiteRunnerError> {
        let mut statement = self
            .connection
            .prepare("SELECT object_id, name FROM _engine_catalog_objects ORDER BY object_id ASC")
            .map_err(|error| sqlite_error("prepare catalog object query", error))?;
        let mut rows = Vec::new();
        let mut result_rows = statement
            .query([])
            .map_err(|error| sqlite_error("step catalog object query", error))?;

        while let Some(row) = result_rows
            .next()
            .map_err(|error| sqlite_error("step catalog object query", error))?
        {
            rows.push(CatalogObjectRow {
                object_id: read_integer_column(row, 0, "read catalog object id")?,
                name: read_text_column(row, 1, "read catalog object name")?,
            });
        }

        Ok(rows)
    }

    fn read_catalog_fields(&self) -> Result<Vec<CatalogFieldRow>, SQLiteRunnerError> {
        let mut columns = self.connection.prepare(
            "SELECT name FROM pragma_table_info('_engine_catalog_fields') WHERE name = 'inverse_field_name'",
        ).map_err(|error| sqlite_error("prepare catalog compatibility query", error))?;
        let inverse_column = if columns
            .exists([])
            .map_err(|error| sqlite_error("read catalog columns", error))?
        {
            "inverse_field_name"
        } else {
            "NULL"
        };
        let mut statement = self
            .connection
            .prepare(
                &format!("SELECT object_id, field_id, name, field_kind, cardinality, scalar_type, target_object_id, is_implicit, is_unique, {inverse_column}
                 FROM _engine_catalog_fields
                 ORDER BY object_id ASC, field_id ASC"),
            )
            .map_err(|error| sqlite_error("prepare catalog field query", error))?;
        let mut rows = Vec::new();
        let mut result_rows = statement
            .query([])
            .map_err(|error| sqlite_error("step catalog field query", error))?;

        while let Some(row) = result_rows
            .next()
            .map_err(|error| sqlite_error("step catalog field query", error))?
        {
            rows.push(CatalogFieldRow {
                object_id: read_integer_column(row, 0, "read catalog object id")?,
                field_id: read_integer_column(row, 1, "read catalog field id")?,
                name: read_text_column(row, 2, "read catalog field name")?,
                field_kind: read_text_column(row, 3, "read catalog field kind")?,
                cardinality: read_text_column(row, 4, "read catalog field cardinality")?,
                scalar_type: read_nullable_text_column(row, 5, "read scalar_type")?,
                target_object_id: read_nullable_integer_column(row, 6, "read target_object_id")?,
                is_implicit: read_bool_column(row, 7, "read is_implicit")?,
                is_unique: read_bool_column(row, 8, "read is_unique")?,
                inverse_field_name: read_nullable_text_column(row, 9, "read inverse_field_name")?,
            });
        }

        Ok(rows)
    }
}

#[allow(dead_code)] // Retain the full stored row even when verification only uses its content.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SchemaVersionRow {
    pub(crate) version_id: String,
    pub(crate) checksum: String,
    pub(crate) applied_at: String,
    pub(crate) schema_snapshot: String,
    pub(crate) version_number: i64,
}

struct CatalogObjectRow {
    object_id: i64,
    name: String,
}

struct CatalogFieldRow {
    object_id: i64,
    #[allow(dead_code)]
    field_id: i64,
    name: String,
    field_kind: String,
    cardinality: String,
    scalar_type: Option<String>,
    target_object_id: Option<i64>,
    is_implicit: bool,
    is_unique: bool,
    inverse_field_name: Option<String>,
}

fn field_from_catalog_row(
    field: &CatalogFieldRow,
    objects: &[CatalogObjectRow],
) -> Result<Field, SQLiteRunnerError> {
    match field.field_kind.as_str() {
        "scalar" => {
            let scalar_type =
                parse_scalar_type(field.scalar_type.as_deref().ok_or_else(|| {
                    SQLiteRunnerError::execution_failed(format!(
                        "catalog field `{}` is missing scalar_type",
                        field.name
                    ))
                })?)?;
            let cardinality = parse_single_cardinality(&field.cardinality)?;
            let uniqueness = parse_uniqueness(field.is_unique)?;

            Ok(Field::Scalar(ScalarField::with_uniqueness(
                field.name.clone(),
                scalar_type,
                cardinality,
                uniqueness,
            )))
        }
        "link" => {
            let target_object_id = field.target_object_id.ok_or_else(|| {
                SQLiteRunnerError::execution_failed(format!(
                    "catalog link field `{}` is missing target_object_id",
                    field.name
                ))
            })?;
            let target_object = objects
                .iter()
                .find(|object| object.object_id == target_object_id)
                .ok_or_else(|| {
                    SQLiteRunnerError::execution_failed(format!(
                        "catalog link field `{}` references unknown target object id {target_object_id}",
                        field.name
                    ))
                })?;
            let cardinality = parse_cardinality(&field.cardinality)?;
            let uniqueness = parse_uniqueness(field.is_unique)?;

            let link = match &field.inverse_field_name {
                Some(source) => LinkField::with_inverse(
                    field.name.clone(),
                    target_object.name.clone(),
                    cardinality,
                    source.clone(),
                ),
                None => LinkField::with_uniqueness(
                    field.name.clone(),
                    target_object.name.clone(),
                    cardinality,
                    uniqueness,
                ),
            };
            Ok(Field::Link(link))
        }
        kind => Err(SQLiteRunnerError::execution_failed(format!(
            "unknown catalog field kind `{kind}`"
        ))),
    }
}

fn read_text_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<String, SQLiteRunnerError> {
    match row
        .get_ref(index)
        .map_err(|error| sqlite_error(context, error))?
    {
        ValueRef::Text(bytes) => core::str::from_utf8(bytes)
            .map(ToString::to_string)
            .map_err(|error| {
                SQLiteRunnerError::execution_failed(format!("{context}: invalid UTF-8: {error}"))
            }),
        value => Err(unexpected_column_type(context, value)),
    }
}

fn read_nullable_text_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<Option<String>, SQLiteRunnerError> {
    match row
        .get_ref(index)
        .map_err(|error| sqlite_error(context, error))?
    {
        ValueRef::Null => Ok(None),
        ValueRef::Text(_) => read_text_column(row, index, context).map(Some),
        value => Err(unexpected_column_type(context, value)),
    }
}

fn read_nullable_integer_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<Option<i64>, SQLiteRunnerError> {
    match row
        .get_ref(index)
        .map_err(|error| sqlite_error(context, error))?
    {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(value) => Ok(Some(value)),
        value => Err(unexpected_column_type(context, value)),
    }
}

fn read_integer_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<i64, SQLiteRunnerError> {
    read_nullable_integer_column(row, index, context)?.ok_or_else(|| {
        SQLiteRunnerError::execution_failed(format!("{context}: unexpected column type Null"))
    })
}

fn read_bool_column(row: &Row<'_>, index: usize, context: &str) -> Result<bool, SQLiteRunnerError> {
    match read_nullable_integer_column(row, index, context)? {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        value => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: expected 0 or 1, got {value:?}"
        ))),
    }
}

fn read_cell_value(row: &Row<'_>, index: usize) -> Result<SQLiteCellValue, SQLiteRunnerError> {
    match row
        .get_ref(index)
        .map_err(|error| sqlite_error("read result column type", error))?
    {
        ValueRef::Integer(value) => Ok(SQLiteCellValue::Integer(value)),
        ValueRef::Real(value) => Ok(SQLiteCellValue::Real(value)),
        ValueRef::Text(_) => {
            read_text_column(row, index, "read text result").map(SQLiteCellValue::Text)
        }
        ValueRef::Null => Ok(SQLiteCellValue::Null),
        ValueRef::Blob(_) => Err(SQLiteRunnerError::execution_failed(
            "blob result values are not supported yet",
        )),
    }
}

fn unexpected_column_type(context: &str, value: ValueRef<'_>) -> SQLiteRunnerError {
    SQLiteRunnerError::execution_failed(format!(
        "{context}: unexpected column type {:?}",
        value.data_type()
    ))
}

fn sqlite_error(context: &str, error: rusqlite::Error) -> SQLiteRunnerError {
    SQLiteRunnerError::execution_failed(format!("{context}: {error}"))
}

fn parse_scalar_type(value: &str) -> Result<ScalarType, SQLiteRunnerError> {
    match value {
        "str" => Ok(ScalarType::Str),
        "int64" => Ok(ScalarType::Int64),
        "float64" => Ok(ScalarType::Float64),
        "bool" => Ok(ScalarType::Bool),
        "uuid" => Ok(ScalarType::Uuid),
        "datetime" => Ok(ScalarType::DateTime),
        _ => Err(SQLiteRunnerError::execution_failed(format!(
            "unknown scalar type `{value}`"
        ))),
    }
}

fn parse_cardinality(value: &str) -> Result<Cardinality, SQLiteRunnerError> {
    match value {
        "optional" => Ok(Cardinality::Optional),
        "required" => Ok(Cardinality::Required),
        "many" => Ok(Cardinality::Many),
        _ => Err(SQLiteRunnerError::execution_failed(format!(
            "unknown cardinality `{value}`"
        ))),
    }
}

fn parse_single_cardinality(value: &str) -> Result<SingleCardinality, SQLiteRunnerError> {
    match parse_cardinality(value)? {
        Cardinality::Optional => Ok(SingleCardinality::Optional),
        Cardinality::Required => Ok(SingleCardinality::Required),
        Cardinality::Many => Err(SQLiteRunnerError::execution_failed(
            "scalar fields cannot have many cardinality",
        )),
    }
}

fn parse_uniqueness(value: bool) -> Result<Uniqueness, SQLiteRunnerError> {
    if value {
        Ok(Uniqueness::Unique)
    } else {
        Ok(Uniqueness::NotUnique)
    }
}

fn query_bind_values(values: &[sqlite_query_sqlgen::SQLiteBindValue]) -> Vec<Value> {
    values
        .iter()
        .map(|value| match value {
            sqlite_query_sqlgen::SQLiteBindValue::String(value) => Value::Text(value.clone()),
            sqlite_query_sqlgen::SQLiteBindValue::Int64(value) => Value::Integer(*value),
            sqlite_query_sqlgen::SQLiteBindValue::Float64(value) => Value::Real(*value),
            sqlite_query_sqlgen::SQLiteBindValue::Bool(value) => Value::Integer(i64::from(*value)),
            sqlite_query_sqlgen::SQLiteBindValue::Null => Value::Null,
        })
        .collect()
}

fn schema_bind_values(values: &[SQLiteValuePlan]) -> Vec<Value> {
    values
        .iter()
        .map(|value| match value {
            SQLiteValuePlan::Integer(value) => Value::Integer(*value),
            SQLiteValuePlan::Text(value) => Value::Text(value.clone()),
            SQLiteValuePlan::Null => Value::Null,
        })
        .collect()
}

fn complete_bind_values(mut values: Vec<Value>, parameter_count: usize) -> Vec<Value> {
    if values.len() < parameter_count {
        values.resize(parameter_count, Value::Null);
    }
    values
}

impl SQLiteRunner for NativeSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.connection
            .execute_batch(sql)
            .map_err(|error| sqlite_error("execute SQL", error))
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| sqlite_error("prepare SQL", error))?;
        let values = complete_bind_values(schema_bind_values(values), statement.parameter_count());
        statement
            .execute(params_from_iter(values))
            .map(|_| ())
            .map_err(|error| sqlite_error("step prepared SQL", error))
    }
}

impl SQLiteQueryRunner for NativeSQLiteRunner {
    fn execute_select(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_select(self, statement)
    }

    fn execute_insert(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::execute_insert(self, statement)
    }

    fn execute_update(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_update(self, statement)
    }

    fn execute_delete(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_delete(self, statement)
    }
}

impl SQLiteTransactionRunner for NativeSQLiteRunner {
    fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::begin_transaction(self)
    }

    fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::commit_transaction(self)
    }

    fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::rollback_transaction(self)
    }
}

impl SQLiteSchemaReader for NativeSQLiteRunner {
    fn load_verified_schema(&mut self) -> Result<Option<SQLiteStoredSchema>, SQLiteRunnerError> {
        NativeSQLiteRunner::load_verified_schema(self)
    }
}
