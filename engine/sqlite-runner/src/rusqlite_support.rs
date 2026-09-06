use rusqlite::{
    Connection, Row, params_from_iter,
    types::{Value, ValueRef},
};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use schema_model::{
    Cardinality, Field, LinkField, ScalarField, ScalarType, SingleCardinality, Uniqueness,
};
use sqlite_schema_plan::SQLiteValuePlan;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::SQLiteCellValue;
use crate::SQLiteRunnerError;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SchemaVersionRow {
    pub(crate) version_id: String,
    pub(crate) checksum: String,
    pub(crate) applied_at: String,
    pub(crate) schema_snapshot: String,
    pub(crate) version_number: i64,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) struct CatalogObjectRow {
    pub(crate) object_id: i64,
    pub(crate) name: String,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) struct CatalogFieldRow {
    pub(crate) object_id: i64,
    #[allow(dead_code)]
    pub(crate) field_id: i64,
    pub(crate) name: String,
    pub(crate) field_kind: String,
    pub(crate) cardinality: String,
    pub(crate) scalar_type: Option<String>,
    pub(crate) target_object_id: Option<i64>,
    pub(crate) is_implicit: bool,
    pub(crate) is_unique: bool,
    pub(crate) inverse_field_name: Option<String>,
}

pub(crate) fn execute(connection: &Connection, sql: &str) -> Result<(), SQLiteRunnerError> {
    connection
        .execute_batch(sql)
        .map_err(|error| sqlite_error("execute SQL", error))
}

pub(crate) fn execute_with_values(
    connection: &Connection,
    sql: &str,
    values: &[SQLiteValuePlan],
) -> Result<(), SQLiteRunnerError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_error("prepare SQL", error))?;
    let values = complete_bind_values(schema_bind_values(values), statement.parameter_count());
    statement
        .execute(params_from_iter(values))
        .map(|_| ())
        .map_err(|error| sqlite_error("step prepared SQL", error))
}

pub(crate) fn table_exists(
    connection: &Connection,
    table_name: &str,
) -> Result<bool, SQLiteRunnerError> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
        .map_err(|error| sqlite_error("prepare table existence query", error))?;

    statement
        .exists([table_name])
        .map_err(|error| sqlite_error("step table existence query", error))
}

pub(crate) fn first_three_column_row(
    connection: &Connection,
    sql: &str,
) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_error("prepare read-back query", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| sqlite_error("step read-back query", error))?;

    match rows
        .next()
        .map_err(|error| sqlite_error("step read-back query", error))?
    {
        Some(row) => Ok(Some((
            read_integer_column(row, 0, "read integer column")?,
            read_text_column(row, 1, "read text column")?,
            read_nullable_integer_column(row, 2, "read nullable integer column")?,
        ))),
        None => Ok(None),
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn field_from_catalog_row(
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

pub(crate) fn read_text_column(
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

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn read_nullable_text_column(
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

pub(crate) fn read_nullable_integer_column(
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

pub(crate) fn read_integer_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<i64, SQLiteRunnerError> {
    read_nullable_integer_column(row, index, context)?.ok_or_else(|| {
        SQLiteRunnerError::execution_failed(format!("{context}: unexpected column type Null"))
    })
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn read_bool_column(
    row: &Row<'_>,
    index: usize,
    context: &str,
) -> Result<bool, SQLiteRunnerError> {
    match read_nullable_integer_column(row, index, context)? {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        value => Err(SQLiteRunnerError::execution_failed(format!(
            "{context}: expected 0 or 1, got {value:?}"
        ))),
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn read_cell_value(
    row: &Row<'_>,
    index: usize,
) -> Result<SQLiteCellValue, SQLiteRunnerError> {
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

pub(crate) fn sqlite_error(context: &str, error: rusqlite::Error) -> SQLiteRunnerError {
    SQLiteRunnerError::execution_failed(format!("{context}: {error}"))
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn query_bind_values(values: &[sqlite_query_sqlgen::SQLiteBindValue]) -> Vec<Value> {
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

pub(crate) fn complete_bind_values(mut values: Vec<Value>, parameter_count: usize) -> Vec<Value> {
    if values.len() < parameter_count {
        values.resize(parameter_count, Value::Null);
    }
    values
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

fn unexpected_column_type(context: &str, value: ValueRef<'_>) -> SQLiteRunnerError {
    SQLiteRunnerError::execution_failed(format!(
        "{context}: unexpected column type {:?}",
        value.data_type()
    ))
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
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

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
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

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn parse_single_cardinality(value: &str) -> Result<SingleCardinality, SQLiteRunnerError> {
    match parse_cardinality(value)? {
        Cardinality::Optional => Ok(SingleCardinality::Optional),
        Cardinality::Required => Ok(SingleCardinality::Required),
        Cardinality::Many => Err(SQLiteRunnerError::execution_failed(
            "scalar fields cannot have many cardinality",
        )),
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn parse_uniqueness(value: bool) -> Result<Uniqueness, SQLiteRunnerError> {
    if value {
        Ok(Uniqueness::Unique)
    } else {
        Ok(Uniqueness::NotUnique)
    }
}
