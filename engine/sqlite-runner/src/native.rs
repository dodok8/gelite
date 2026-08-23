extern crate alloc;

use alloc::ffi::CString;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use powersync_sqlite_nostd::{
    ColumnType, Connection, Destructor, ManagedConnection, ManagedStmt, ResultCode,
};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality, Uniqueness,
};
use sqlite_schema_plan::SQLiteValuePlan;

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
                let second = statement
                    .column_text(1)
                    .map_err(|error| self.result_error("read text column", error))?
                    .to_string();
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

    pub fn load_schema_catalog(&self) -> Result<SchemaCatalog, SQLiteRunnerError> {
        let objects = self.read_catalog_objects()?;
        let fields = self.read_catalog_fields()?;
        let mut object_types = Vec::new();

        for object in &objects {
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

                        declared_fields.push(Field::Link(LinkField::with_uniqueness(
                            field.name.clone(),
                            target_object.name.clone(),
                            cardinality,
                            uniqueness,
                        )));
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

        if rows.is_empty() {
            return Err(SQLiteRunnerError::execution_failed(
                "database does not contain Gelite catalog objects",
            ));
        }

        Ok(rows)
    }

    fn read_catalog_fields(&self) -> Result<Vec<CatalogFieldRow>, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(
                "SELECT object_id, field_id, name, field_kind, cardinality, scalar_type, target_object_id, is_implicit, is_unique
                 FROM _engine_catalog_fields
                 ORDER BY object_id ASC, field_id ASC",
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
                }),
                Ok(ResultCode::DONE) => break,
                Ok(result) => return Err(self.result_error("step catalog field query", result)),
                Err(result) => return Err(self.result_error("step catalog field query", result)),
            }
        }

        if rows.is_empty() {
            return Err(SQLiteRunnerError::execution_failed(
                "database does not contain Gelite catalog fields",
            ));
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
}

fn read_text_column(
    statement: &ManagedStmt,
    index: i32,
    context: &str,
) -> Result<String, SQLiteRunnerError> {
    statement
        .column_text(index)
        .map(|value| value.to_string())
        .map_err(|error| SQLiteRunnerError::execution_failed(format!("{context}: {error:?}")))
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
    match statement.column_int64(index) {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: expected 0 or 1, got {value}"
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
