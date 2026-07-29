use alloc::string::ToString;

use crate::{Keyword, TokenKind, lex};

#[test]
fn lexer_can_tokenize_update_statement() {
    let tokens = lex(r#"update Post filter .id = "post-1" set { title := "Closed Case" }"#)
        .expect("update query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Update));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("Post".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[3].kind(), &TokenKind::Dot);
    assert_eq!(tokens[4].kind(), &TokenKind::Ident("id".to_string()));
    assert_eq!(tokens[5].kind(), &TokenKind::Eq);
    assert_eq!(tokens[6].kind(), &TokenKind::String("post-1".to_string()));
    assert_eq!(tokens[7].kind(), &TokenKind::Keyword(Keyword::Set));
    assert_eq!(tokens[8].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[9].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[10].kind(), &TokenKind::ColonEq);
    assert_eq!(
        tokens[11].kind(),
        &TokenKind::String("Closed Case".to_string())
    );
    assert_eq!(tokens[12].kind(), &TokenKind::RBrace);
}
