extern crate alloc;

use alloc::ffi::CString;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use powersync_sqlite_nostd::{
    ColumnType, Connection, Destructor, ManagedConnection, ManagedStmt, ResultCode, Stmt,
};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality, Uniqueness,
};
use sqlite_schema_plan::{SQLiteValuePlan, schema_snapshot_checksum, serialize_schema_snapshot};

use crate::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteQueryRunner, SQLiteRunner, SQLiteRunnerError,
    SQLiteTransactionRunner,
};

/// Native SQLite runner backed by an owned SQLite connection.
///
/// The concrete SQLite binding stays private to this module. Public planner,
/// SQL generator, and command APIs should continue to depend on the
/// `SQLiteRunner` trait instead of this backend type.
pub struct NativeSQLiteRunner {
    connection: ManagedConnection,
}

impl NativeSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError> {
        Self::open(":memory:")
    }

    pub fn open(path: &str) -> Result<Self, SQLiteRunnerError> {
        let filename = CString::new(path)
            .map_err(|_| SQLiteRunnerError::execution_failed("SQLite path contains a null byte"))?;
        let connection = powersync_sqlite_nostd::open(filename.as_ptr()).map_err(|error| {
            SQLiteRunnerError::execution_failed(format!(
                "failed to open SQLite database `{path}`: {error:?}"
            ))
        })?;
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
        let statement = self
            .connection
            .prepare_v2("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .map_err(|_| self.connection_error("prepare table existence query"))?;

        statement
            .bind_text(1, table_name, Destructor::TRANSIENT)
            .map_err(|_| self.connection_error("bind table name"))?;

        match statement.step() {
            Ok(ResultCode::ROW) => Ok(true),
            Ok(ResultCode::DONE) => Ok(false),
            Ok(result) => Err(self.result_error("step table existence query", result)),
            Err(result) => Err(self.result_error("step table existence query", result)),
        }
    }

    /// Reads the first row as owned values for native backend smoke tests.
    ///
    /// This is not the query execution API. It exists only to verify that the
    /// selected SQLite binding stores values through `SQLiteRunner` correctly.
    pub fn first_three_column_row(
        &self,
        sql: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(sql)
            .map_err(|_| self.connection_error("prepare read-back query"))?;

        match statement.step() {
            Ok(ResultCode::ROW) => {
                let first = statement.column_int64(0);
                let second = read_text_column(&statement, 1, "read text column")?;
                let third = match statement
                    .column_type(2)
                    .map_err(|error| self.result_error("read nullable integer column", error))?
                {
                    ColumnType::Null => None,
                    ColumnType::Integer => Some(statement.column_int64(2)),
                    column_type => {
                        return Err(SQLiteRunnerError::execution_failed(format!(
                            "read nullable integer column: unexpected column type {column_type:?}"
                        )));
                    }
                };

                Ok(Some((first, second, third)))
            }
            Ok(ResultCode::DONE) => Ok(None),
            Ok(result) => Err(self.result_error("step read-back query", result)),
            Err(result) => Err(self.result_error("step read-back query", result)),
        }
    }

    /// Verifies the latest snapshot checksum and logical catalog in one read transaction.
    ///
    /// No source file is needed. An existing caller transaction is rejected and left untouched.
    pub fn verify_schema_version(&mut self) -> Result<(), SQLiteRunnerError> {
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
            self.commit_transaction()
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
            let mut declared_fields = Vec::new();

            for field in fields
                .iter()
                .filter(|field| field.object_id == object.object_id && !field.is_implicit)
            {
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

                        declared_fields.push(Field::Scalar(ScalarField::with_uniqueness(
                            field.name.clone(),
                            scalar_type,
                            cardinality,
                            uniqueness,
                        )));
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
                        declared_fields.push(Field::Link(link));
                    }
                    kind => {
                        return Err(SQLiteRunnerError::execution_failed(format!(
                            "unknown catalog field kind `{kind}`"
                        )));
                    }
                }
            }

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
        let prepared = self
            .connection
            .prepare_v2(statement.sql())
            .map_err(|_| self.connection_error("prepare SELECT"))?;

        self.bind_query_values(&prepared, statement.bind_values())?;

        let column_count = prepared.column_count();
        let output_names = statement.output_names();

        if !output_names.is_empty() && output_names.len() != column_count as usize {
            return Err(SQLiteRunnerError::execution_failed(
                "result output metadata does not match SQLite column count",
            ));
        }

        let result_shape = statement.result_shape();
        let (column_indexes, columns): (Vec<_>, Vec<_>) = if result_shape.is_some() {
            ((0..column_count).collect(), Vec::new())
        } else {
            let selected_columns: Vec<(i32, String)> = if output_names.is_empty() {
                (0..column_count)
                    .map(|index| {
                        prepared
                            .column_name(index)
                            .map(|name| (index, name.to_string()))
                            .map_err(|error| self.result_error("read result column name", error))
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
        loop {
            match prepared.step() {
                Ok(ResultCode::ROW) => {
                    let row = column_indexes
                        .iter()
                        .map(|index| read_cell_value(&prepared, *index))
                        .collect::<Result<Vec<_>, _>>()?;

                    rows.push(row);
                }
                Ok(ResultCode::DONE) => break,
                Ok(result) => return Err(self.result_error("step SELECT", result)),
                Err(result) => return Err(self.result_error("step SELECT", result)),
            }
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
        let prepared = self
            .connection
            .prepare_v2(statement.sql())
            .map_err(|_| self.connection_error("prepare INSERT"))?;

        self.bind_query_values(&prepared, statement.bind_values())?;

        match prepared.step() {
            Ok(ResultCode::DONE) => Ok(()),
            Ok(result) => Err(self.result_error("step INSERT", result)),
            Err(result) => Err(self.result_error("step INSERT", result)),
        }
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
        let prepared = self
            .connection
            .prepare_v2(statement.sql())
            .map_err(|_| self.connection_error(&format!("prepare {operation}")))?;

        self.bind_query_values(&prepared, statement.bind_values())?;

        match prepared.step() {
            Ok(ResultCode::DONE) => Ok(self.connection.changes64()),
            Ok(result) => Err(self.result_error(&format!("step {operation}"), result)),
            Err(result) => Err(self.result_error(&format!("step {operation}"), result)),
        }
    }

    fn bind_query_values(
        &self,
        prepared: &ManagedStmt,
        bind_values: &[sqlite_query_sqlgen::SQLiteBindValue],
    ) -> Result<(), SQLiteRunnerError> {
        for (index, value) in bind_values.iter().enumerate() {
            let parameter_index = i32::try_from(index + 1).map_err(|_| {
                SQLiteRunnerError::execution_failed("bind parameter index exceeds i32 range")
            })?;

            match value {
                sqlite_query_sqlgen::SQLiteBindValue::String(value) => {
                    prepared
                        .bind_text(parameter_index, value, Destructor::TRANSIENT)
                        .map_err(|error| self.result_error("bind string value", error))?;
                }
                sqlite_query_sqlgen::SQLiteBindValue::Int64(value) => {
                    prepared
                        .bind_int64(parameter_index, *value)
                        .map_err(|error| self.result_error("bind int64 value", error))?;
                }
                sqlite_query_sqlgen::SQLiteBindValue::Float64(value) => {
                    prepared
                        .bind_double(parameter_index, *value)
                        .map_err(|error| self.result_error("bind float64 value", error))?;
                }
                sqlite_query_sqlgen::SQLiteBindValue::Bool(value) => {
                    prepared
                        .bind_int64(parameter_index, i64::from(*value))
                        .map_err(|error| self.result_error("bind bool value", error))?;
                }
                sqlite_query_sqlgen::SQLiteBindValue::Null => {
                    prepared
                        .bind_null(parameter_index)
                        .map_err(|error| self.result_error("bind null value", error))?;
                }
            }
        }

        Ok(())
    }

    /// Reads the highest numbered stored version without verifying its contents.
    fn read_latest_schema_version(&self) -> Result<Option<SchemaVersionRow>, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(
                "SELECT version_id, checksum, applied_at, schema_snapshot, version_number
                 FROM _engine_schema_versions ORDER BY version_number DESC LIMIT 1",
            )
            .map_err(|_| self.connection_error("prepare schema version query"))?;
        match statement.step() {
            Ok(ResultCode::ROW) => {
                let version_number =
                    read_nullable_integer_column(&statement, 4, "read version number")?
                        .filter(|number| *number > 0)
                        .ok_or_else(|| {
                            SQLiteRunnerError::schema_verification_failed(
                                "stored schema version number must be positive",
                            )
                        })?;
                Ok(Some(SchemaVersionRow {
                    version_id: read_text_column(&statement, 0, "read schema version id")?,
                    checksum: read_text_column(&statement, 1, "read schema version checksum")?,
                    applied_at: read_text_column(&statement, 2, "read schema version applied_at")?,
                    schema_snapshot: read_text_column(
                        &statement,
                        3,
                        "read schema version snapshot",
                    )?,
                    version_number,
                }))
            }
            Ok(ResultCode::DONE) => Ok(None),
            Ok(result) | Err(result) => Err(self.result_error("step schema version query", result)),
        }
    }

    fn read_catalog_objects(&self) -> Result<Vec<CatalogObjectRow>, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(
                "SELECT object_id, name FROM _engine_catalog_objects ORDER BY object_id ASC",
            )
            .map_err(|_| self.connection_error("prepare catalog object query"))?;
        let mut rows = Vec::new();

        loop {
            match statement.step() {
                Ok(ResultCode::ROW) => rows.push(CatalogObjectRow {
                    object_id: statement.column_int64(0),
                    name: read_text_column(&statement, 1, "read catalog object name")?,
                }),
                Ok(ResultCode::DONE) => break,
                Ok(result) => return Err(self.result_error("step catalog object query", result)),
                Err(result) => return Err(self.result_error("step catalog object query", result)),
            }
        }

        Ok(rows)
    }

    fn read_catalog_fields(&self) -> Result<Vec<CatalogFieldRow>, SQLiteRunnerError> {
        let columns = self.connection.prepare_v2(
            "SELECT name FROM pragma_table_info('_engine_catalog_fields') WHERE name = 'inverse_field_name'",
        ).map_err(|_| self.connection_error("prepare catalog compatibility query"))?;
        let inverse_column = match columns.step() {
            Ok(ResultCode::ROW) => "inverse_field_name",
            Ok(ResultCode::DONE) => "NULL",
            Ok(result) | Err(result) => {
                return Err(self.result_error("read catalog columns", result));
            }
        };
        let statement = self
            .connection
            .prepare_v2(
                &format!("SELECT object_id, field_id, name, field_kind, cardinality, scalar_type, target_object_id, is_implicit, is_unique, {inverse_column}
                 FROM _engine_catalog_fields
                 ORDER BY object_id ASC, field_id ASC"),
            )
            .map_err(|_| self.connection_error("prepare catalog field query"))?;
        let mut rows = Vec::new();

        loop {
            match statement.step() {
                Ok(ResultCode::ROW) => rows.push(CatalogFieldRow {
                    object_id: statement.column_int64(0),
                    field_id: statement.column_int64(1),
                    name: read_text_column(&statement, 2, "read catalog field name")?,
                    field_kind: read_text_column(&statement, 3, "read catalog field kind")?,
                    cardinality: read_text_column(&statement, 4, "read catalog field cardinality")?,
                    scalar_type: read_nullable_text_column(&statement, 5, "read scalar_type")?,
                    target_object_id: read_nullable_integer_column(
                        &statement,
                        6,
                        "read target_object_id",
                    )?,
                    is_implicit: read_bool_column(&statement, 7, "read is_implicit")?,
                    is_unique: read_bool_column(&statement, 8, "read is_unique")?,
                    inverse_field_name: read_nullable_text_column(
                        &statement,
                        9,
                        "read inverse_field_name",
                    )?,
                }),
                Ok(ResultCode::DONE) => break,
                Ok(result) => return Err(self.result_error("step catalog field query", result)),
                Err(result) => return Err(self.result_error("step catalog field query", result)),
            }
        }

        Ok(rows)
    }

    fn connection_error(&self, context: &str) -> SQLiteRunnerError {
        let message = self
            .connection
            .errmsg()
            .unwrap_or_else(|_| "unknown SQLite error".to_string());

        SQLiteRunnerError::execution_failed(format!("{context}: {message}"))
    }

    fn result_error(&self, context: &str, result: ResultCode) -> SQLiteRunnerError {
        let message = self
            .connection
            .errmsg()
            .unwrap_or_else(|_| "unknown SQLite error".to_string());

        SQLiteRunnerError::execution_failed(format!("{context}: {result:?}: {message}"))
    }
}

#[allow(dead_code)] // Retain the full stored row even when verification only uses its content.
#[derive(Debug, PartialEq, Eq)]
struct SchemaVersionRow {
    version_id: String,
    checksum: String,
    applied_at: String,
    schema_snapshot: String,
    version_number: i64,
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

fn read_text_column(
    statement: &ManagedStmt,
    index: i32,
    context: &str,
) -> Result<String, SQLiteRunnerError> {
    // SQLite may return a null blob pointer for empty TEXT, which is not SQL NULL.
    if statement.column_type(index) == Ok(ColumnType::Text)
        && statement.stmt.column_bytes(index) == 0
    {
        return Ok(String::new());
    }
    // The binding's column_text uses unchecked UTF-8 conversion, even for corrupt SQLite TEXT.
    let bytes = statement
        .column_blob(index)
        .map_err(|error| SQLiteRunnerError::execution_failed(format!("{context}: {error:?}")))?;
    core::str::from_utf8(bytes)
        .map(ToString::to_string)
        .map_err(|error| {
            SQLiteRunnerError::execution_failed(format!("{context}: invalid UTF-8: {error}"))
        })
}

fn read_nullable_text_column(
    statement: &ManagedStmt,
    index: i32,
    context: &str,
) -> Result<Option<String>, SQLiteRunnerError> {
    match statement
        .column_type(index)
        .map_err(|error| SQLiteRunnerError::execution_failed(format!("{context}: {error:?}")))?
    {
        ColumnType::Null => Ok(None),
        ColumnType::Text => read_text_column(statement, index, context).map(Some),
        column_type => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: unexpected column type {column_type:?}"
        ))),
    }
}

fn read_nullable_integer_column(
    statement: &ManagedStmt,
    index: i32,
    context: &str,
) -> Result<Option<i64>, SQLiteRunnerError> {
    match statement
        .column_type(index)
        .map_err(|error| SQLiteRunnerError::execution_failed(format!("{context}: {error:?}")))?
    {
        ColumnType::Null => Ok(None),
        ColumnType::Integer => Ok(Some(statement.column_int64(index))),
        column_type => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: unexpected column type {column_type:?}"
        ))),
    }
}

fn read_bool_column(
    statement: &ManagedStmt,
    index: i32,
    context: &str,
) -> Result<bool, SQLiteRunnerError> {
    match read_nullable_integer_column(statement, index, context)? {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        value => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: expected 0 or 1, got {value:?}"
        ))),
    }
}

fn read_cell_value(
    statement: &ManagedStmt,
    index: i32,
) -> Result<SQLiteCellValue, SQLiteRunnerError> {
    match statement.column_type(index).map_err(|error| {
        SQLiteRunnerError::execution_failed(format!("read result column type: {error:?}"))
    })? {
        ColumnType::Integer => Ok(SQLiteCellValue::Integer(statement.column_int64(index))),
        ColumnType::Float => Ok(SQLiteCellValue::Real(statement.column_double(index))),
        ColumnType::Text => {
            read_text_column(statement, index, "read text result").map(SQLiteCellValue::Text)
        }
        ColumnType::Null => Ok(SQLiteCellValue::Null),
        ColumnType::Blob => Err(SQLiteRunnerError::execution_failed(
            "blob result values are not supported yet",
        )),
    }
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

impl SQLiteRunner for NativeSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.connection
            .exec_safe(sql)
            .map(|_| ())
            .map_err(|_| self.connection_error("execute SQL"))
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(sql)
            .map_err(|_| self.connection_error("prepare SQL"))?;

        for (index, value) in values.iter().enumerate() {
            let parameter_index = i32::try_from(index + 1).map_err(|_| {
                SQLiteRunnerError::execution_failed("bind parameter index exceeds i32 range")
            })?;
            match value {
                SQLiteValuePlan::Integer(value) => statement
                    .bind_int64(parameter_index, *value)
                    .map_err(|error| self.result_error("bind integer value", error))?,
                SQLiteValuePlan::Text(value) => statement
                    .bind_text(parameter_index, value, Destructor::TRANSIENT)
                    .map_err(|error| self.result_error("bind text value", error))?,
                SQLiteValuePlan::Null => statement
                    .bind_null(parameter_index)
                    .map_err(|error| self.result_error("bind null value", error))?,
            };
        }

        match statement.step() {
            Ok(ResultCode::DONE) => Ok(()),
            Ok(result) => Err(self.result_error("step prepared SQL", result)),
            Err(result) => Err(self.result_error("step prepared SQL", result)),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixtures::{native_runner_with_post_schema, rendered_post_schema_statements};

    #[test]
    fn read_latest_schema_version_uses_number_instead_of_time_uuid_or_insert_order() {
        let mut runner = native_runner_with_post_schema();
        let statements = rendered_post_schema_statements();
        let Some(sqlite_schema_sqlgen::RenderedSchemaStatement::Insert(insert)) = statements.last()
        else {
            panic!("initial schema should end with the version insert");
        };

        // Raw history fixtures stand in for non-initial migration application.
        // Version 10 shares the baseline timestamp; version 2 has a later time and UUID.
        for (number, id, applied_at) in [
            (
                10,
                "11111111-1111-4111-8111-111111111111",
                crate::tests::fixtures::APPLIED_AT,
            ),
            (
                2,
                "ffffffff-ffff-4fff-bfff-ffffffffffff",
                "2099-01-01T00:00:00.000Z",
            ),
        ] {
            runner
                .execute_with_values(
                    insert.sql(),
                    &[
                        SQLiteValuePlan::Text(id.to_string()),
                        SQLiteValuePlan::Text("unchecked checksum".to_string()),
                        SQLiteValuePlan::Text(applied_at.to_string()),
                        SQLiteValuePlan::Text(" {\"name\":\"雪\"}\n".to_string()),
                        SQLiteValuePlan::Integer(number),
                    ],
                )
                .expect("history fixture should be stored");
        }

        let row = runner
            .read_latest_schema_version()
            .expect("latest version should load");
        // Values stay owned after the prepared statement and connection are dropped.
        drop(runner);
        assert_eq!(
            row,
            Some(SchemaVersionRow {
                version_id: "11111111-1111-4111-8111-111111111111".to_string(),
                checksum: "unchecked checksum".to_string(),
                applied_at: crate::tests::fixtures::APPLIED_AT.to_string(),
                schema_snapshot: " {\"name\":\"雪\"}\n".to_string(),
                version_number: 10,
            })
        );
    }

    #[test]
    fn read_latest_schema_version_returns_none_for_empty_table() {
        let mut runner = native_runner_with_post_schema();
        // Normal initial application always inserts a baseline; remove it for the read test.
        runner
            .execute("DELETE FROM _engine_schema_versions")
            .expect("version should be removed");

        assert_eq!(runner.read_latest_schema_version(), Ok(None));
    }

    #[test]
    fn read_latest_schema_version_reports_missing_table() {
        let runner = NativeSQLiteRunner::open_in_memory().expect("database should open");

        let error = runner
            .read_latest_schema_version()
            .expect_err("missing version table should be a query error");

        assert!(error.message().contains("prepare schema version query"));
        assert!(error.message().contains("no such table"));
        assert_eq!(runner.table_exists("_engine_schema_versions"), Ok(false));
    }

    #[test]
    fn read_latest_schema_version_rejects_invalid_version_numbers() {
        // Bypass normal version planning to model corrupt metadata types and ranges.
        for value in ["0", "-1", "1.5", "'invalid'"] {
            let mut runner = native_runner_with_post_schema();
            runner
                .execute(&format!(
                    "UPDATE _engine_schema_versions SET version_number = {value}"
                ))
                .expect("version number should be corrupted");

            runner
                .read_latest_schema_version()
                .expect_err("version number must be a positive integer");
        }
    }

    #[test]
    fn schema_version_number_is_unique() {
        let mut runner = native_runner_with_post_schema();
        // Normal initial application cannot produce two version IDs with the same number.
        let error = runner.execute(
            "INSERT INTO _engine_schema_versions (version_id, checksum, applied_at, schema_snapshot, version_number)
             SELECT '11111111-1111-4111-8111-111111111111', checksum, applied_at, schema_snapshot, version_number
             FROM _engine_schema_versions",
        ).expect_err("duplicate version number should be rejected");

        assert!(error.message().contains("UNIQUE constraint failed"));
    }

    #[test]
    fn read_latest_schema_version_excludes_rolled_back_version() {
        let mut runner = native_runner_with_post_schema();
        let baseline = runner
            .read_latest_schema_version()
            .expect("baseline should load");
        assert_eq!(baseline.as_ref().map(|row| row.version_number), Some(1));
        runner
            .begin_transaction()
            .expect("transaction should begin");
        // Raw history writes stand in for a future migration that fails before commit.
        runner.execute(
            "INSERT INTO _engine_schema_versions (version_id, checksum, applied_at, schema_snapshot, version_number)
             SELECT '11111111-1111-4111-8111-111111111111', checksum, applied_at, schema_snapshot, 2
             FROM _engine_schema_versions",
        ).expect("pending version should be stored");
        assert_eq!(
            runner
                .read_latest_schema_version()
                .expect("pending version should load")
                .map(|row| row.version_number),
            Some(2)
        );
        runner
            .rollback_transaction()
            .expect("failed migration should roll back");

        assert_eq!(runner.read_latest_schema_version(), Ok(baseline));
    }
}
