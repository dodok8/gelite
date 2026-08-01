use alloc::string::ToString;

use query_ast::TransactionCommand;

use crate::{Keyword, ParseErrorKind, TokenKind, lex, parse_transaction_command};

#[test]
fn lexer_can_tokenize_transaction_commands() {
    let tokens = lex("start transaction commit rollback").expect("commands should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Start));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Transaction));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Commit));
    assert_eq!(tokens[3].kind(), &TokenKind::Keyword(Keyword::Rollback));
}

#[test]
fn lexer_keeps_transaction_keyword_prefix_identifiers() {
    let tokens =
        lex("startTime transactionLog commitHash rollbackReason").expect("identifiers should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("startTime".to_string()));
    assert_eq!(
        tokens[1].kind(),
        &TokenKind::Ident("transactionLog".to_string())
    );
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Ident("commitHash".to_string())
    );
    assert_eq!(
        tokens[3].kind(),
        &TokenKind::Ident("rollbackReason".to_string())
    );
}

#[test]
fn parser_can_parse_transaction_commands() {
    for (source, expected) in [
        ("start transaction", TransactionCommand::Start),
        ("commit", TransactionCommand::Commit),
        ("rollback", TransactionCommand::Rollback),
    ] {
        assert_eq!(
            parse_transaction_command(source),
            Ok(expected),
            "{source} should parse"
        );
    }
}

#[test]
fn parser_rejects_start_without_transaction_keyword() {
    let error = parse_transaction_command("start").expect_err("incomplete start should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof {
            expected: "transaction"
        }
    );
}

#[test]
fn parser_rejects_start_with_wrong_second_keyword() {
    let error = parse_transaction_command("start commit").expect_err("invalid start should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "transaction"
        }
    );
}

#[test]
fn parser_rejects_trailing_transaction_command_tokens() {
    for source in [
        "start transaction commit",
        "commit rollback",
        "rollback commit",
    ] {
        let error = parse_transaction_command(source).expect_err("trailing token should fail");

        assert_eq!(
            error.kind(),
            &ParseErrorKind::UnexpectedToken { expected: "EOF" },
            "{source} should reject trailing tokens"
        );
    }
}
