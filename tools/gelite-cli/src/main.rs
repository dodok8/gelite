use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use gelite_commands::{
    CompiledQuery, QueryKind, SchemaPlanStatement, apply_schema, compile_query, plan_schema,
};
use sqlite_runner::native::NativeSQLiteRunner;

#[derive(Debug, Parser)]
#[command(name = "gelite")]
#[command(about = "Gelite command-line tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    Query {
        #[command(subcommand)]
        command: QueryCommand,
    },
    Repl(ReplCommand),
}

#[derive(Debug, Subcommand)]
enum SchemaCommand {
    Plan {
        schema_file: PathBuf,
    },
    Apply {
        schema_file: PathBuf,
        #[arg(long)]
        database: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum QueryCommand {
    Plan {
        query_file: PathBuf,
        #[arg(long)]
        schema: PathBuf,
    },
}

#[derive(Debug, Args)]
struct ReplCommand {
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    schema: Option<PathBuf>,
    #[arg(long)]
    database: Option<PathBuf>,
    #[arg(trailing_var_arg = true)]
    query: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Schema { command } => run_schema_command(command),
        Command::Query { command } => run_query_command(command),
        Command::Repl(command) => run_repl_command(command),
    }
}

fn run_schema_command(command: SchemaCommand) -> Result<(), String> {
    match command {
        SchemaCommand::Plan { schema_file } => {
            let source = fs::read_to_string(&schema_file)
                .map_err(|error| format!("failed to read {}: {error}", schema_file.display()))?;
            let output = plan_schema(&source).map_err(|error| error.message().to_string())?;

            for statement in output.statements() {
                println!("{}", statement.sql());
                if let SchemaPlanStatement::Insert { values, .. } = statement {
                    println!("  binds: {values:?}");
                }
            }

            Ok(())
        }
        SchemaCommand::Apply {
            schema_file,
            database,
        } => {
            let source = fs::read_to_string(&schema_file)
                .map_err(|error| format!("failed to read {}: {error}", schema_file.display()))?;
            let database = path_to_str(&database)?;
            let mut runner = NativeSQLiteRunner::open(database)
                .map_err(|error| format!("failed to open database: {}", error.message()))?;

            apply_schema(&source, &mut runner).map_err(|error| error.message().to_string())?;
            println!("Applied schema to {database}");

            Ok(())
        }
    }
}

fn run_repl_command(command: ReplCommand) -> Result<(), String> {
    let (catalog, mut runner) = match (command.schema, command.database) {
        (Some(_), Some(_)) => {
            return Err("gelite repl accepts either --schema or --database, not both".to_string());
        }
        (Some(schema), None) => {
            let source = fs::read_to_string(&schema)
                .map_err(|error| format!("failed to read {}: {error}", schema.display()))?;
            (
                schema_parser::parse_schema(&source).map_err(|error| format!("{error:#?}"))?,
                None,
            )
        }
        (None, Some(database)) => {
            let database = path_to_str(&database)?;
            let runner = NativeSQLiteRunner::open(database)
                .map_err(|error| format!("failed to open database: {}", error.message()))?;
            let catalog = runner
                .load_schema_catalog()
                .map_err(|error| format!("failed to load catalog: {}", error.message()))?;

            (catalog, Some(runner))
        }
        (None, None) => {
            return Err(
                "gelite repl needs a catalog. Pass --schema <schema.geli> for compile-only query inspection or --database <app.db> to load an applied catalog."
                    .to_string(),
            );
        }
    };

    let query = if command.query.is_empty() {
        None
    } else {
        Some(command.query.join(" "))
    };

    let options = repl::ReplOptions {
        debug: command.debug,
        query,
    };

    match runner.as_mut() {
        Some(runner) => {
            let mut executor = |request| execute_request(runner, request);

            repl::run_with_executor(&catalog, options, &mut executor)
        }
        None => repl::run_with_catalog(&catalog, options),
    }
    .map_err(|_| "gelite repl failed".to_string())
}

fn execute_request(
    runner: &mut NativeSQLiteRunner,
    request: repl::ExecutionRequest,
) -> Result<Option<sqlite_runner::SQLiteQueryResult>, String> {
    match request {
        repl::ExecutionRequest::Query(CompiledQuery { kind, statement }) => match kind {
            QueryKind::Select => runner
                .execute_select(&statement)
                .map(Some)
                .map_err(|error| error.message().to_string()),
            QueryKind::Insert { generated_id } => {
                runner
                    .execute_insert(&statement)
                    .map_err(|error| error.message().to_string())?;

                Ok(Some(sqlite_runner::SQLiteQueryResult::new(
                    vec!["id".to_string()],
                    vec![vec![sqlite_runner::SQLiteCellValue::Text(generated_id)]],
                )))
            }
            QueryKind::Update => runner
                .execute_update(&statement)
                .map(affected_rows_result)
                .map(Some)
                .map_err(|error| error.message().to_string()),
            QueryKind::Delete => runner
                .execute_delete(&statement)
                .map(affected_rows_result)
                .map(Some)
                .map_err(|error| error.message().to_string()),
        },
        repl::ExecutionRequest::Transaction(command) => match command {
            repl::TransactionCommand::Start => runner.begin_transaction(),
            repl::TransactionCommand::Commit => runner.commit_transaction(),
            repl::TransactionCommand::Rollback => runner.rollback_transaction(),
        }
        .map(|()| None)
        .map_err(|error| error.message().to_string()),
    }
}

fn affected_rows_result(affected_rows: i64) -> sqlite_runner::SQLiteQueryResult {
    sqlite_runner::SQLiteQueryResult::new(
        vec!["affected_rows".to_string()],
        vec![vec![sqlite_runner::SQLiteCellValue::Integer(affected_rows)]],
    )
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn run_query_command(command: QueryCommand) -> Result<(), String> {
    let QueryCommand::Plan { query_file, schema } = command;

    let schema_source = fs::read_to_string(&schema)
        .map_err(|error| format!("failed to read {}: {error}", schema.display()))?;
    let query_source = fs::read_to_string(&query_file)
        .map_err(|error| format!("failed to read {}: {error}", query_file.display()))?;
    let catalog = schema_parser::parse_schema(&schema_source)
        .map_err(|error| format!("failed to parse schema {}: {error:#?}", schema.display()))?;
    let compiled =
        compile_query(&catalog, &query_source).map_err(|error| error.message().to_string())?;

    println!("SQL:\n{}", compiled.statement.sql());
    println!("Bind values: {:?}", compiled.statement.bind_values());

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;
    use gelite_commands::{CompiledQuery, QueryKind};
    use repl::{ExecutionRequest, TransactionCommand};
    use sqlite_query_sqlgen::{SQLiteBindValue, SQLiteStatement};
    use sqlite_runner::{SQLiteCellValue, SQLiteRunner, native::NativeSQLiteRunner};

    use super::{Cli, Command, QueryCommand, execute_request, run_query_command};

    #[test]
    fn query_plan_accepts_query_file_and_schema_option() {
        let cli = Cli::try_parse_from([
            "gelite",
            "query",
            "plan",
            "query.geliql",
            "--schema",
            "schema.geli",
        ])
        .expect("query plan arguments should parse");
        let Command::Query {
            command: QueryCommand::Plan { query_file, schema },
        } = cli.command
        else {
            panic!("expected query plan command");
        };

        assert_eq!(query_file, PathBuf::from("query.geliql"));
        assert_eq!(schema, PathBuf::from("schema.geli"));
    }

    #[test]
    fn query_plan_reports_schema_parse_context() {
        let schema = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let error = run_query_command(QueryCommand::Plan {
            query_file: schema.clone(),
            schema: schema.clone(),
        })
        .expect_err("invalid schema should fail");

        assert!(error.starts_with(&format!("failed to parse schema {}:", schema.display())));
    }

    #[test]
    fn native_repl_executor_maps_query_results() {
        let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");
        runner
            .execute("CREATE TABLE entry (id TEXT PRIMARY KEY, value INTEGER NOT NULL)")
            .expect("table should be created");

        let insert = execute_request(
            &mut runner,
            ExecutionRequest::Query(CompiledQuery {
                kind: QueryKind::Insert {
                    generated_id: "entry-1".to_string(),
                },
                statement: SQLiteStatement::new(
                    "INSERT INTO entry (id, value) VALUES (?, ?)",
                    vec![
                        SQLiteBindValue::String("entry-1".to_string()),
                        SQLiteBindValue::Int64(1),
                    ],
                ),
            }),
        )
        .expect("insert should execute")
        .expect("insert should return the generated id");
        assert_eq!(
            insert.rows(),
            &[vec![SQLiteCellValue::Text("entry-1".to_string())]]
        );

        let select = execute_request(
            &mut runner,
            ExecutionRequest::Query(CompiledQuery {
                kind: QueryKind::Select,
                statement: SQLiteStatement::new("SELECT value FROM entry", vec![]),
            }),
        )
        .expect("select should execute")
        .expect("select should return rows");
        assert_eq!(select.rows(), &[vec![SQLiteCellValue::Integer(1)]]);

        for (kind, sql) in [
            (
                QueryKind::Update,
                "UPDATE entry SET value = 2 WHERE id = 'entry-1'",
            ),
            (QueryKind::Delete, "DELETE FROM entry WHERE id = 'entry-1'"),
        ] {
            let result = execute_request(
                &mut runner,
                ExecutionRequest::Query(CompiledQuery {
                    kind,
                    statement: SQLiteStatement::new(sql, vec![]),
                }),
            )
            .expect("mutation should execute")
            .expect("mutation should return affected rows");
            assert_eq!(result.rows(), &[vec![SQLiteCellValue::Integer(1)]]);
        }
    }

    #[test]
    fn native_repl_executor_uses_one_connection_for_transactions() {
        let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");
        runner
            .execute("CREATE TABLE entry (id TEXT PRIMARY KEY)")
            .expect("table should be created");

        assert_eq!(
            execute_request(
                &mut runner,
                ExecutionRequest::Transaction(TransactionCommand::Start)
            ),
            Ok(None)
        );
        runner
            .execute("INSERT INTO entry VALUES ('rolled-back')")
            .expect("insert should execute");
        assert_eq!(
            execute_request(
                &mut runner,
                ExecutionRequest::Transaction(TransactionCommand::Rollback)
            ),
            Ok(None)
        );

        assert_eq!(
            execute_request(
                &mut runner,
                ExecutionRequest::Transaction(TransactionCommand::Start)
            ),
            Ok(None)
        );
        runner
            .execute("INSERT INTO entry VALUES ('committed')")
            .expect("insert should execute");
        assert_eq!(
            execute_request(
                &mut runner,
                ExecutionRequest::Transaction(TransactionCommand::Commit)
            ),
            Ok(None)
        );

        let result = execute_request(
            &mut runner,
            ExecutionRequest::Query(CompiledQuery {
                kind: QueryKind::Select,
                statement: SQLiteStatement::new("SELECT id FROM entry", vec![]),
            }),
        )
        .expect("select should execute")
        .expect("select should return rows");
        assert_eq!(
            result.rows(),
            &[vec![SQLiteCellValue::Text("committed".to_string())]]
        );

        let error = execute_request(
            &mut runner,
            ExecutionRequest::Transaction(TransactionCommand::Commit),
        )
        .expect_err("commit without an active transaction should fail");
        assert!(error.contains("no transaction is active"));
    }
}
