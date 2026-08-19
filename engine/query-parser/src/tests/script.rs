use crate::{ParseErrorKind, QueryScriptStatement, parse_script};
use query_ast::TransactionCommand;

#[test]
fn splits_multiline_statements_and_ignores_semicolons_in_strings() {
    let script =
        parse_script("start transaction;;\ninsert Post { title := \"first; post\" };\ncommit;\n")
            .expect("script should parse");

    assert_eq!(script.statements().len(), 3);
    assert!(matches!(
        script.statements()[0],
        QueryScriptStatement::Transaction {
            command: TransactionCommand::Start,
            ..
        }
    ));
    assert!(matches!(
        &script.statements()[1],
        QueryScriptStatement::Query { source, .. } if source.contains("first; post")
    ));
    assert_eq!(script.statements()[1].span().start().line(), 2);
    assert_eq!(script.statements()[1].span().start().column(), 1);
    assert!(matches!(
        script.statements()[2],
        QueryScriptStatement::Transaction {
            command: TransactionCommand::Commit,
            ..
        }
    ));
}

#[test]
fn accepts_one_legacy_statement_without_a_semicolon() {
    let script = parse_script("select Post { title }").expect("single statement should parse");
    assert_eq!(script.statements().len(), 1);
}

#[test]
fn rejects_an_unterminated_statement_after_a_semicolon() {
    let error = parse_script("commit; select Post { title }")
        .expect_err("multi-statement scripts should terminate every statement");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: ";" }
    );
    assert!(error.span().is_some());
}
