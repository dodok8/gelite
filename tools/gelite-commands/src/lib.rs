//! Shared command orchestration for Gelite tools.
//!
//! This crate belongs to the tools layer. It composes parser, planner,
//! renderer, and runner crates into user-facing commands, but it does not own
//! process argument parsing, stdout/stderr, or process exit codes.

use query_ast::TransactionCommand;
use query_parser::{QueryScriptStatement, parse_delete, parse_script, parse_select, parse_update};
use schema_model::SchemaCatalog;
use sqlite_query_sqlgen::SQLiteStatement;
use sqlite_runner::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteQueryRunner, SQLiteRunner, SQLiteRunnerError,
    SQLiteTransactionRunner, apply_schema_statements,
};
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

pub struct CompiledScript {
    statements: Vec<CompiledScriptStatement>,
}

impl CompiledScript {
    pub fn statements(&self) -> &[CompiledScriptStatement] {
        &self.statements
    }
}

pub enum CompiledScriptStatement {
    Query(CompiledQuery),
    Transaction(TransactionCommand),
}

impl CompiledScriptStatement {
    pub fn sql(&self) -> &str {
        match self {
            Self::Query(query) => query.statement.sql(),
            Self::Transaction(TransactionCommand::Start) => "BEGIN TRANSACTION",
            Self::Transaction(TransactionCommand::Commit) => "COMMIT",
            Self::Transaction(TransactionCommand::Rollback) => "ROLLBACK",
        }
    }

    pub fn statement(&self) -> Option<&SQLiteStatement> {
        match self {
            Self::Query(query) => Some(&query.statement),
            Self::Transaction(_) => None,
        }
    }
}

pub fn compile_script(
    catalog: &SchemaCatalog,
    source: &str,
) -> Result<CompiledScript, CommandError> {
    let script = parse_script(source)
        .map_err(|error| CommandError::new(format!("failed to parse query script: {error:#?}")))?;
    let mut statements = Vec::with_capacity(script.statements().len());
    let mut transaction_start = None;

    for (index, statement) in script.statements().iter().enumerate() {
        let number = index + 1;
        let span = statement.span().start();
        let compiled = match statement {
            QueryScriptStatement::Query { source, .. } => compile_query(catalog, source)
                .map(CompiledScriptStatement::Query)
                .map_err(|error| {
                    CommandError::new(format!(
                        "statement {number} at line {}, column {}: {}",
                        span.line(),
                        span.column(),
                        error.message()
                    ))
                })?,
            QueryScriptStatement::Transaction { command, .. } => {
                match command {
                    TransactionCommand::Start if transaction_start.is_some() => {
                        return Err(script_transaction_error(
                            number,
                            span.line(),
                            span.column(),
                            "nested transactions are not supported",
                        ));
                    }
                    TransactionCommand::Start => transaction_start = Some((number, span)),
                    TransactionCommand::Commit | TransactionCommand::Rollback
                        if transaction_start.is_none() =>
                    {
                        return Err(script_transaction_error(
                            number,
                            span.line(),
                            span.column(),
                            "no transaction is active",
                        ));
                    }
                    TransactionCommand::Commit | TransactionCommand::Rollback => {
                        transaction_start = None;
                    }
                }
                CompiledScriptStatement::Transaction(*command)
            }
        };
        statements.push(compiled);
    }

    if let Some((number, span)) = transaction_start {
        return Err(script_transaction_error(
            number,
            span.line(),
            span.column(),
            "transaction is still active at end of script",
        ));
    }

    Ok(CompiledScript { statements })
}

fn script_transaction_error(
    number: usize,
    line: usize,
    column: usize,
    message: &str,
) -> CommandError {
    CommandError::new(format!(
        "statement {number} at line {line}, column {column}: {message}"
    ))
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
            if !plan.follow_up_fetches().is_empty() {
                return Err(CommandError::new(
                    "selected multi-link execution is not supported yet".to_string(),
                ));
            }

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
        Some("start" | "commit" | "rollback") => {
            return Err(CommandError::new(
                "transaction commands require a database-backed interactive REPL".to_string(),
            ));
        }
        _ => {
            return Err(CommandError::new(
                "query must start with `select`, `insert`, `update`, or `delete`".to_string(),
            ));
        }
    };

    Ok(CompiledQuery { kind, statement })
}

pub fn execute_query(
    runner: &mut impl SQLiteQueryRunner,
    query: CompiledQuery,
) -> Result<SQLiteQueryResult, CommandError> {
    let CompiledQuery { kind, statement } = query;

    match kind {
        QueryKind::Select => runner.execute_select(&statement),
        QueryKind::Insert { generated_id } => runner.execute_insert(&statement).map(|()| {
            SQLiteQueryResult::new(
                vec!["id".to_string()],
                vec![vec![SQLiteCellValue::Text(generated_id)]],
            )
        }),
        QueryKind::Update => runner.execute_update(&statement).map(affected_rows_result),
        QueryKind::Delete => runner.execute_delete(&statement).map(affected_rows_result),
    }
    .map_err(|error| CommandError::new(error.message().to_string()))
}

pub fn execute_script(
    runner: &mut (impl SQLiteQueryRunner + SQLiteTransactionRunner),
    script: CompiledScript,
) -> Result<Vec<Option<SQLiteQueryResult>>, CommandError> {
    let mut results = Vec::with_capacity(script.statements.len());
    let mut in_transaction = false;

    for (index, statement) in script.statements.into_iter().enumerate() {
        let number = index + 1;
        let result = match statement {
            CompiledScriptStatement::Query(query) => execute_query(runner, query).map(Some),
            CompiledScriptStatement::Transaction(command) => {
                let transaction_result = match command {
                    TransactionCommand::Start => runner.begin_transaction(),
                    TransactionCommand::Commit => runner.commit_transaction(),
                    TransactionCommand::Rollback => runner.rollback_transaction(),
                };
                transaction_result
                    .map(|()| {
                        in_transaction = command == TransactionCommand::Start;
                        None
                    })
                    .map_err(|error| CommandError::new(error.message().to_string()))
            }
        };

        match result {
            Ok(result) => results.push(result),
            Err(error) => {
                if in_transaction {
                    let _ = runner.rollback_transaction();
                }
                return Err(CommandError::new(format!(
                    "statement {number}: {}",
                    error.message()
                )));
            }
        }
    }

    Ok(results)
}

fn affected_rows_result(affected_rows: i64) -> SQLiteQueryResult {
    SQLiteQueryResult::new(
        vec!["affected_rows".to_string()],
        vec![vec![SQLiteCellValue::Integer(affected_rows)]],
    )
}

pub fn format_query_result(result: &SQLiteQueryResult) -> String {
    let mut lines = Vec::new();

    if !result.columns().is_empty() {
        lines.push(result.columns().join("\t"));
    }

    lines.extend(result.rows().iter().map(|row| {
        row.iter()
            .map(format_cell_value)
            .collect::<Vec<_>>()
            .join("\t")
    }));

    if result.rows().is_empty() {
        lines.push("(0 rows)".to_string());
    }

    lines.join("\n")
}

fn format_cell_value(value: &SQLiteCellValue) -> String {
    match value {
        SQLiteCellValue::Integer(value) => value.to_string(),
        SQLiteCellValue::Real(value) => value.to_string(),
        SQLiteCellValue::Text(value) => value.clone(),
        SQLiteCellValue::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", format_cell_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SQLiteCellValue::Null => "NULL".to_string(),
    }
}

#[cfg(test)]
mod tests;
