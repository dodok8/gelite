use alloc::string::ToString;

use crate::{Keyword, TokenKind, lex};

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
