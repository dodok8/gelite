//! Shared command orchestration for Gelite tools.
//!
//! This crate belongs to the tools layer. It composes parser, planner,
//! renderer, and runner crates into user-facing commands, but it does not own
//! process argument parsing, stdout/stderr, or process exit codes.

use query_parser::{parse_delete, parse_select, parse_update};
use schema_model::SchemaCatalog;
use sqlite_query_sqlgen::SQLiteStatement;
use sqlite_runner::{SQLiteRunner, SQLiteRunnerError, apply_schema_statements};
use sqlite_schema_plan::SQLiteValuePlan;
use sqlite_schema_sqlgen::RenderedSchemaStatement;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    message: String,
}

impl CommandError {
    fn new(message: String) -> Self {
        Self { message }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaPlanOutput {
    statements: Vec<SchemaPlanStatement>,
}

impl SchemaPlanOutput {
    pub fn statements(&self) -> &[SchemaPlanStatement] {
        &self.statements
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaPlanStatement {
    Sql(String),
    Insert {
        sql: String,
        values: Vec<SQLiteValuePlan>,
    },
}

impl SchemaPlanStatement {
    pub fn sql(&self) -> &str {
        match self {
            Self::Sql(sql) => sql,
            Self::Insert { sql, .. } => sql,
        }
    }

    pub fn values(&self) -> Option<&[SQLiteValuePlan]> {
        match self {
            Self::Sql(_) => None,
            Self::Insert { values, .. } => Some(values),
        }
    }
}

pub fn plan_schema(source: &str) -> Result<SchemaPlanOutput, CommandError> {
    let catalog = schema_parser::parse_schema(source).map_err(|error| CommandError {
        message: format!("failed to parse schema: {error:?}"),
    })?;
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog);
    let statements = sqlite_schema_sqlgen::render_initial_schema(&plan)
        .into_iter()
        .map(schema_plan_statement_from_rendered)
        .collect();

    Ok(SchemaPlanOutput { statements })
}

pub fn apply_schema(source: &str, runner: &mut impl SQLiteRunner) -> Result<(), CommandError> {
    let catalog = schema_parser::parse_schema(source).map_err(|error| CommandError {
        message: format!("failed to parse schema: {error:?}"),
    })?;
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog);
    let statements = sqlite_schema_sqlgen::render_initial_schema(&plan);

    apply_schema_statements(runner, &statements).map_err(command_error_from_runner)
}

fn command_error_from_runner(error: SQLiteRunnerError) -> CommandError {
    CommandError {
        message: format!("failed to apply schema: {}", error.message()),
    }
}

fn schema_plan_statement_from_rendered(statement: RenderedSchemaStatement) -> SchemaPlanStatement {
    match statement {
        RenderedSchemaStatement::Sql(sql) => SchemaPlanStatement::Sql(sql),
        RenderedSchemaStatement::Insert(insert) => SchemaPlanStatement::Insert {
            sql: insert.sql().to_string(),
            values: insert.values().to_vec(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryKind {
    Select,
    Insert { generated_id: String },
    Update,
    Delete,
}

pub struct CompiledQuery {
    pub kind: QueryKind,
    pub statement: SQLiteStatement,
}

pub fn compile_query(catalog: &SchemaCatalog, source: &str) -> Result<CompiledQuery, CommandError> {
    let (kind, statement) = match source.split_whitespace().next() {
        Some("select") => {
            let query = parse_select(source)
                .map_err(|error| CommandError::new(format!("failed to parse query: {error:#?}")))?;
            let resolved = query_resolver::resolve_select(catalog, &query).map_err(|error| {
                CommandError::new(format!("failed to resolve query: {error:#?}"))
            })?;
            let plan = sqlite_query_plan::plan_select(&resolved);

            (QueryKind::Select, sqlite_query_sqlgen::render_select(&plan))
        }
        Some("insert") => {
            let query = query_parser::parse_insert(source)
                .map_err(|error| CommandError::new(format!("failed to parse query: {error:#?}")))?;
            let resolved = query_resolver::resolve_insert(catalog, &query).map_err(|error| {
                CommandError::new(format!("failed to resolve query: {error:#?}"))
            })?;
            let plan = sqlite_query_plan::plan_insert(&resolved);
            let generated_id = uuid::Uuid::new_v4().to_string();
            let statement = sqlite_query_sqlgen::render_insert(&plan, &generated_id);

            (QueryKind::Insert { generated_id }, statement)
        }
        Some("update") => {
            let query = parse_update(source)
                .map_err(|error| CommandError::new(format!("failed to parse query: {error:#?}")))?;
            let resolved = query_resolver::resolve_update(catalog, &query).map_err(|error| {
                CommandError::new(format!("failed to resolve query: {error:#?}"))
            })?;
            let plan = sqlite_query_plan::plan_update(&resolved);

            (QueryKind::Update, sqlite_query_sqlgen::render_update(&plan))
        }
        Some("delete") => {
            let query = parse_delete(source)
                .map_err(|error| CommandError::new(format!("failed to parse query: {error:#?}")))?;
            let resolved = query_resolver::resolve_delete(catalog, &query).map_err(|error| {
                CommandError::new(format!("failed to resolve query: {error:#?}"))
            })?;
            let plan = sqlite_query_plan::plan_delete(&resolved);

            (QueryKind::Delete, sqlite_query_sqlgen::render_delete(&plan))
        }
        _ => {
            return Err(CommandError::new(
                "query must start with `select`, `insert`, `update`, or `delete`".to_string(),
            ));
        }
    };

    Ok(CompiledQuery { kind, statement })
}

#[cfg(test)]
mod tests;
