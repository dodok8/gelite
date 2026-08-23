use gelite_commands::{CompiledQuery, compile_query, format_query_result};
pub use query_ast::TransactionCommand;
use query_parser::parse_transaction_command;
use rustyline::{Cmd, DefaultEditor, KeyCode, KeyEvent, Modifiers, error::ReadlineError};
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};
use sqlite_runner::SQLiteQueryResult;

pub struct ReplOptions {
    pub debug: bool,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplError;

pub enum ExecutionRequest {
    Query(CompiledQuery),
    Transaction(TransactionCommand),
}

type QueryExecutor<'a> =
    dyn FnMut(ExecutionRequest) -> Result<Option<SQLiteQueryResult>, String> + 'a;

pub fn run(options: ReplOptions) -> Result<(), ReplError> {
    let catalog = build_development_schema();

    run_with_catalog(&catalog, options)
}

pub fn run_with_catalog(catalog: &SchemaCatalog, options: ReplOptions) -> Result<(), ReplError> {
    let mut runtime = ReplRuntime { executor: None };

    runtime.run(catalog, options)
}

pub fn run_with_executor(
    catalog: &SchemaCatalog,
    options: ReplOptions,
    executor: &mut QueryExecutor<'_>,
) -> Result<(), ReplError> {
    let mut runtime = ReplRuntime {
        executor: Some(executor),
    };

    runtime.run(catalog, options)
}

struct ReplRuntime<'a> {
    executor: Option<&'a mut QueryExecutor<'a>>,
}

enum ReplLoopAction {
    Continue,
    Break,
}

impl ReplRuntime<'_> {
    fn run(&mut self, catalog: &SchemaCatalog, options: ReplOptions) -> Result<(), ReplError> {
        match options.query {
            Some(query_text) => self.inspect_query(catalog, &query_text, options.debug, false),
            None => self.run_repl(catalog, options.debug),
        }
    }

    fn run_repl(&mut self, catalog: &SchemaCatalog, debug: bool) -> Result<(), ReplError> {
        run_repl(catalog, debug, self)
    }

    fn inspect_query(
        &mut self,
        catalog: &SchemaCatalog,
        query_text: &str,
        debug: bool,
        interactive: bool,
    ) -> Result<(), ReplError> {
        let request = compile_input(catalog, query_text, debug)?;

        if matches!(&request, ExecutionRequest::Transaction(_))
            && (!interactive || self.executor.is_none())
        {
            eprintln!("transaction commands require a database-backed interactive REPL");
            return Err(ReplError);
        }

        match self.executor.as_deref_mut() {
            Some(executor) => {
                let result = executor(request).map_err(|error| {
                    eprintln!("failed to execute query: {error}");
                    ReplError
                })?;
                if let Some(result) = result {
                    print_query_result(&result);
                }
            }
            None => {
                let ExecutionRequest::Query(CompiledQuery { statement, .. }) = request else {
                    unreachable!("transaction commands are rejected without an executor")
                };
                println!("{}", statement.sql());
            }
        }

        Ok(())
    }
}

fn run_repl(
    catalog: &SchemaCatalog,
    debug: bool,
    runtime: &mut ReplRuntime<'_>,
) -> Result<(), ReplError> {
    println!("gelite repl");
    println!("Type a query, start transaction, commit, rollback, or quit / exit to leave.");
    println!(
        "Press Alt+Enter to insert a newline. See https://github.com/gelite-dev/gelite/blob/main/README.md#multiline-repl-input."
    );
    println!("Press Ctrl-C twice in a row to leave.");
    if debug {
        println!("Debug output is enabled.");
    }

    let mut editor = DefaultEditor::new().map_err(|error| {
        eprintln!("failed to initialize line editor: {error}");
        ReplError
    })?;
    editor.bind_sequence(KeyEvent(KeyCode::Enter, Modifiers::ALT), Cmd::Newline);
    let mut pending = String::new();
    let mut interrupt_count = 0;

    loop {
        let prompt = repl_prompt(&pending);

        match editor.readline(prompt) {
            Ok(line) => match handle_repl_line(
                catalog,
                debug,
                runtime,
                &mut editor,
                &mut pending,
                &mut interrupt_count,
                line,
            )? {
                ReplLoopAction::Continue => {}
                ReplLoopAction::Break => break,
            },
            Err(error) => {
                match handle_repl_read_error(error, &mut pending, &mut interrupt_count)? {
                    ReplLoopAction::Continue => {}
                    ReplLoopAction::Break => break,
                }
            }
        }
    }

    Ok(())
}

fn repl_prompt(pending: &str) -> &'static str {
    if pending.is_empty() {
        "gelite> "
    } else {
        "   ...> "
    }
}

fn handle_repl_line(
    catalog: &SchemaCatalog,
    debug: bool,
    runtime: &mut ReplRuntime<'_>,
    editor: &mut DefaultEditor,
    pending: &mut String,
    interrupt_count: &mut i32,
    line: String,
) -> Result<ReplLoopAction, ReplError> {
    *interrupt_count = 0;

    for query_text in complete_repl_inputs(pending, &line) {
        if is_exit_command(&query_text) {
            return Ok(ReplLoopAction::Break);
        }
        let _ = editor.add_history_entry(query_text.as_str());
        if runtime
            .inspect_query(catalog, &query_text, debug, true)
            .is_err()
        {
            break;
        }
    }

    Ok(ReplLoopAction::Continue)
}

fn complete_repl_inputs(pending: &mut String, input: &str) -> Vec<String> {
    let mut queries = Vec::new();

    for line in input.lines() {
        let is_transaction_command = is_transaction_command_line(line);
        let can_start_new_input = !needs_more_input(pending);

        if (is_transaction_command || starts_data_statement(line))
            && can_start_new_input
            && let Some(query) = take_complete_repl_input(pending)
        {
            queries.push(query);
        }

        if is_transaction_command && can_start_new_input {
            queries.push(line.trim().to_string());
        } else {
            append_pending_line(pending, line);
        }
    }

    if let Some(query) = take_complete_repl_input(pending) {
        queries.push(query);
    }

    queries
}

fn is_transaction_command_line(line: &str) -> bool {
    matches!(line.trim(), "start transaction" | "commit" | "rollback")
}

fn starts_data_statement(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("select" | "insert" | "update" | "delete")
    )
}

fn take_complete_repl_input(pending: &mut String) -> Option<String> {
    (!needs_more_input(pending))
        .then(|| core::mem::take(pending).trim().to_string())
        .filter(|query| !query.is_empty())
}

fn append_pending_line(pending: &mut String, line: &str) {
    if !pending.is_empty() {
        pending.push('\n');
    }
    pending.push_str(line);
}

fn handle_repl_read_error(
    error: ReadlineError,
    pending: &mut String,
    interrupt_count: &mut i32,
) -> Result<ReplLoopAction, ReplError> {
    match error {
        ReadlineError::Interrupted => handle_repl_interrupt(pending, interrupt_count),
        ReadlineError::Eof => Ok(ReplLoopAction::Break),
        error => {
            eprintln!("failed to read input: {error}");
            Err(ReplError)
        }
    }
}

fn handle_repl_interrupt(
    pending: &mut String,
    interrupt_count: &mut i32,
) -> Result<ReplLoopAction, ReplError> {
    pending.clear();
    *interrupt_count += 1;

    if *interrupt_count >= 2 {
        return Ok(ReplLoopAction::Break);
    }

    println!("input cancelled. Press Ctrl-C again to leave.");
    Ok(ReplLoopAction::Continue)
}

fn is_exit_command(input: &str) -> bool {
    matches!(input, ":quit" | ":q" | ":exit" | "quit" | "exit")
}

fn needs_more_input(input: &str) -> bool {
    let mut balance = 0;
    let mut in_string = false;

    for ch in input.chars() {
        match ch {
            '"' => in_string = !in_string,
            '{' if !in_string => balance += 1,
            '}' if !in_string => balance -= 1,
            _ => {}
        }
    }

    in_string || balance > 0
}

fn compile_input(
    catalog: &SchemaCatalog,
    input: &str,
    debug: bool,
) -> Result<ExecutionRequest, ReplError> {
    match input.split_whitespace().next() {
        Some("start" | "commit" | "rollback") => parse_transaction_command(input)
            .map(ExecutionRequest::Transaction)
            .map_err(|error| {
                eprintln!("failed to parse transaction command: {error:#?}");
                ReplError
            }),
        _ => compile_query(catalog, input)
            .map(|compiled| {
                if debug {
                    println!("SQL:\n{}", compiled.statement.sql());
                    println!("Bind values: {:?}", compiled.statement.bind_values());
                }
                ExecutionRequest::Query(compiled)
            })
            .map_err(|error| {
                eprintln!("{}", error.message());
                ReplError
            }),
    }
}

fn print_query_result(result: &SQLiteQueryResult) {
    println!("{}", format_query_result(result));
}

fn build_development_schema() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .expect("hardcoded development schema should be valid")
}

#[cfg(test)]
mod tests {
    use gelite_commands::{CompiledQuery, QueryKind};
    use query_ast::TransactionCommand;
    use sqlite_query_sqlgen::SQLiteStatement;
    use sqlite_runner::{SQLiteCellValue, SQLiteRunner, native::NativeSQLiteRunner};

    use super::{
        DefaultEditor, ExecutionRequest, ReplError, ReplLoopAction, ReplOptions, ReplRuntime,
        build_development_schema, complete_repl_inputs, handle_repl_line, needs_more_input,
        run_with_catalog, run_with_executor,
    };

    #[test]
    fn pasted_lines_are_split_at_complete_queries() {
        let mut pending = String::new();

        let queries = complete_repl_inputs(
            &mut pending,
            "start transaction\ninsert User {\n  name := \"Sheri\"\n}\ninsert User {\n  name := \"Alice\"\n}\ncommit",
        );

        assert_eq!(
            queries,
            [
                "start transaction",
                "insert User {\n  name := \"Sheri\"\n}",
                "insert User {\n  name := \"Alice\"\n}",
                "commit",
            ]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn pasted_input_stops_after_an_execution_error() {
        let catalog = build_development_schema();
        let mut query_count = 0;
        let mut commands = Vec::new();

        {
            let mut executor = |request| match request {
                ExecutionRequest::Transaction(command) => {
                    commands.push(command);
                    Ok(None)
                }
                ExecutionRequest::Query(_) => {
                    query_count += 1;
                    if query_count == 2 {
                        Err("forced execution error".to_string())
                    } else {
                        Ok(None)
                    }
                }
            };
            let mut runtime = ReplRuntime {
                executor: Some(&mut executor),
            };
            let mut editor = DefaultEditor::new().expect("editor should initialize");
            let mut pending = String::new();
            let mut interrupt_count = 0;

            let action = handle_repl_line(
                &catalog,
                false,
                &mut runtime,
                &mut editor,
                &mut pending,
                &mut interrupt_count,
                "start transaction\ninsert User { name := \"Sheri\" }\ninsert User { name := \"Alice\" }\ncommit"
                    .to_string(),
            )
            .expect("interactive errors should return control to the REPL");

            assert!(matches!(action, ReplLoopAction::Continue));
        }

        assert_eq!(query_count, 2);
        assert_eq!(commands, [TransactionCommand::Start]);
    }

    #[test]
    fn multiline_update_is_kept_as_one_query() {
        let mut pending = String::new();

        let queries = complete_repl_inputs(
            &mut pending,
            "update User\nfilter .name = \"Sheri\"\nset {\n  name := \"Alice\"\n}",
        );

        assert_eq!(
            queries,
            ["update User\nfilter .name = \"Sheri\"\nset {\n  name := \"Alice\"\n}"]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn multiline_select_clauses_are_kept_with_the_shape() {
        let mut pending = String::new();

        let queries = complete_repl_inputs(
            &mut pending,
            "select Post {\n  title\n}\nfilter .title = \"Draft\"\norder by .title",
        );

        assert_eq!(
            queries,
            ["select Post {\n  title\n}\nfilter .title = \"Draft\"\norder by .title"]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn multiline_strings_do_not_create_input_boundaries() {
        let mut pending = String::new();

        let queries = complete_repl_inputs(
            &mut pending,
            "select Post { title } filter .title = \"first\ncommit\nselect second\"",
        );

        assert_eq!(
            queries,
            ["select Post { title } filter .title = \"first\ncommit\nselect second\""]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn multiline_input_continues_until_braces_are_balanced() {
        assert!(needs_more_input("select Post {"));
        assert!(needs_more_input("select Post {\n  author: { name }"));
        assert!(!needs_more_input("select Post {\n  author: { name }\n}"));
    }

    #[test]
    fn braces_inside_strings_do_not_start_multiline_input() {
        assert!(!needs_more_input(
            r#"select Post { title } filter .title = "{""#
        ));
    }

    #[test]
    fn interactive_database_repl_dispatches_transaction_commands() {
        let catalog = build_development_schema();
        let mut commands = Vec::new();
        {
            let mut executor = |request| {
                let ExecutionRequest::Transaction(command) = request else {
                    panic!("expected transaction command");
                };
                commands.push(command);
                Ok(None)
            };
            let mut runtime = ReplRuntime {
                executor: Some(&mut executor),
            };

            for source in ["start transaction", "commit", "rollback"] {
                runtime
                    .inspect_query(&catalog, source, false, true)
                    .expect("interactive transaction command should execute");
            }
        }

        assert_eq!(
            commands,
            vec![
                TransactionCommand::Start,
                TransactionCommand::Commit,
                TransactionCommand::Rollback,
            ]
        );
    }

    #[test]
    fn compile_only_repl_rejects_transaction_commands() {
        let result = run_with_catalog(
            &build_development_schema(),
            ReplOptions {
                debug: false,
                query: Some("commit".to_string()),
            },
        );

        assert_eq!(result, Err(ReplError));
    }

    #[test]
    fn one_shot_database_repl_rejects_transaction_commands_without_executing() {
        let mut executed = false;
        let mut executor = |_request| {
            executed = true;
            Ok(None)
        };
        let result = run_with_executor(
            &build_development_schema(),
            ReplOptions {
                debug: false,
                query: Some("rollback".to_string()),
            },
            &mut executor,
        );

        assert_eq!(result, Err(ReplError));
        assert!(!executed);
    }

    #[test]
    fn interactive_database_repl_commits_and_rolls_back_on_one_connection() {
        let catalog = build_development_schema();
        let mut runner = NativeSQLiteRunner::open_in_memory().expect("database should open");
        runner
            .execute("CREATE TABLE user (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
            .expect("user table should be created");
        {
            let mut executor = |request| match request {
                ExecutionRequest::Query(CompiledQuery {
                    kind: QueryKind::Insert { .. },
                    statement,
                    ..
                }) => runner
                    .execute_insert(&statement)
                    .map(|()| None)
                    .map_err(|error| error.message().to_string()),
                ExecutionRequest::Transaction(command) => match command {
                    TransactionCommand::Start => runner.begin_transaction(),
                    TransactionCommand::Commit => runner.commit_transaction(),
                    TransactionCommand::Rollback => runner.rollback_transaction(),
                }
                .map(|()| None)
                .map_err(|error| error.message().to_string()),
                ExecutionRequest::Query(_) => panic!("expected insert query"),
            };
            let mut runtime = ReplRuntime {
                executor: Some(&mut executor),
            };

            for source in [
                "start transaction",
                r#"insert User { name := "Sheri" }"#,
                "commit",
                "start transaction",
                r#"insert User { name := "Alice" }"#,
                "rollback",
            ] {
                runtime
                    .inspect_query(&catalog, source, false, true)
                    .expect("interactive input should execute");
            }
        }

        let result = runner
            .execute_select(&SQLiteStatement::new(
                "SELECT name FROM user ORDER BY name",
                vec![],
            ))
            .expect("committed users should be readable");
        assert_eq!(
            result.rows(),
            &[vec![SQLiteCellValue::Text("Sheri".to_string())]]
        );
    }
}
