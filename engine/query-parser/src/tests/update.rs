use alloc::string::ToString;

use query_ast::{CompareOp, Literal};

use super::fixtures::{assert_compare_expr, assert_literal_expr, assert_path_expr};
use crate::{Keyword, ParseErrorKind, TokenKind, lex, parse_insert, parse_select, parse_update};

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

#[test]
fn parser_rejects_update_and_set_as_identifiers() {
    for error in [
        parse_select("select update { title }").expect_err("update should be reserved"),
        parse_select("select Thing { set }").expect_err("set should be reserved"),
        parse_insert(r#"insert Thing { set := "value" }"#).expect_err("set should be reserved"),
    ] {
        assert_eq!(
            error.kind(),
            &ParseErrorKind::UnexpectedToken { expected: "IDENT" }
        );
    }
}

#[test]
fn parser_can_parse_filtered_update() {
    let query = parse_update(
        r#"update Post filter .id = "post-1" set { title := "Closed Case", author := "user-2" }"#,
    )
    .expect("update query should parse");

    assert_eq!(query.target_type_name(), "Post");
    let (left, right) = assert_compare_expr(
        query.filter().expect("update should keep its filter"),
        CompareOp::Eq,
    );
    assert_path_expr(left, &["id"]);
    assert_literal_expr(right, &Literal::String("post-1".to_string()));
    assert_eq!(query.assignments().len(), 2);
    assert_eq!(query.assignments()[0].field_name(), "title");
    assert_eq!(
        query.assignments()[0].value(),
        &Literal::String("Closed Case".to_string())
    );
    assert_eq!(query.assignments()[1].field_name(), "author");
    assert_eq!(
        query.assignments()[1].value(),
        &Literal::String("user-2".to_string())
    );
}

#[test]
fn parser_can_parse_unfiltered_update() {
    let query =
        parse_update(r#"update Post set { title := "Archived" }"#).expect("query should parse");

    assert_eq!(query.target_type_name(), "Post");
    assert!(query.filter().is_none());
    assert_eq!(query.assignments().len(), 1);
}

#[test]
fn parser_preserves_empty_update_set_for_resolver() {
    let query = parse_update("update Post set {}").expect("empty set is valid syntax");

    assert!(query.assignments().is_empty());
}

#[test]
fn parser_preserves_semantically_invalid_update_assignments_for_resolver() {
    let query =
        parse_update(r#"update Post set { id := "post-2", title := "First", title := "Second" }"#)
            .expect("semantic assignment errors should parse");

    assert_eq!(query.assignments().len(), 3);
    assert_eq!(query.assignments()[0].field_name(), "id");
    assert_eq!(query.assignments()[1].field_name(), "title");
    assert_eq!(query.assignments()[2].field_name(), "title");
}

#[test]
fn parser_rejects_update_without_set_keyword() {
    let error = parse_update(r#"update Post { title := "Archived" }"#)
        .expect_err("update without set should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: "set" }
    );
}

#[test]
fn parser_rejects_update_without_set_block() {
    let error = parse_update("update Post set").expect_err("update without set block should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: "{" }
    );
}

#[test]
fn parser_rejects_update_filter_without_expression() {
    let error = parse_update("update Post filter set {}")
        .expect_err("update filter without expression should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "expression"
        }
    );
}
