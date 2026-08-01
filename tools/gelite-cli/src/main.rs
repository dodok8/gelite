use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use gelite_commands::{SchemaPlanStatement, apply_schema, plan_schema};
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
            let mut executor = |request: repl::ExecutionRequest| match request {
                repl::ExecutionRequest::Query { kind, statement } => match kind {
                    repl::QueryKind::Select => runner
                        .execute_select(&statement)
                        .map(Some)
                        .map_err(|error| error.message().to_string()),
                    repl::QueryKind::Insert { generated_id } => {
                        runner
                            .execute_insert(&statement)
                            .map_err(|error| error.message().to_string())?;

                        Ok(Some(sqlite_runner::SQLiteQueryResult::new(
                            vec!["id".to_string()],
                            vec![vec![sqlite_runner::SQLiteCellValue::Text(generated_id)]],
                        )))
                    }
                    repl::QueryKind::Update => runner
                        .execute_update(&statement)
                        .map(affected_rows_result)
                        .map(Some)
                        .map_err(|error| error.message().to_string()),
                    repl::QueryKind::Delete => runner
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
            };

            repl::run_with_executor(&catalog, options, &mut executor)
        }
        None => repl::run_with_catalog(&catalog, options),
    }
    .map_err(|_| "gelite repl failed".to_string())
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
