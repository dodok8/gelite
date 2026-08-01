use alloc::string::ToString;

use query_ast::{CompareOp, Literal};

use super::fixtures::{assert_compare_expr, assert_literal_expr, assert_path_expr};
use crate::{ParseErrorKind, TokenKind, lex, parse_delete};

#[test]
fn lexer_can_tokenize_delete_statement() {
    let tokens = lex(r#"delete Post filter .id = "post-1""#).expect("delete query should lex");

    let TokenKind::Keyword(keyword) = tokens[0].kind() else {
        panic!("delete should be a keyword");
    };
    assert_eq!(keyword.as_str(), "delete");
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("Post".to_string()));
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Keyword(crate::Keyword::Filter)
    );
    assert_eq!(tokens[3].kind(), &TokenKind::Dot);
    assert_eq!(tokens[4].kind(), &TokenKind::Ident("id".to_string()));
    assert_eq!(tokens[5].kind(), &TokenKind::Eq);
    assert_eq!(tokens[6].kind(), &TokenKind::String("post-1".to_string()));
}

#[test]
fn lexer_keeps_delete_prefix_identifier() {
    let tokens = lex("deleteArchive").expect("identifier should lex");

    assert_eq!(
        tokens[0].kind(),
        &TokenKind::Ident("deleteArchive".to_string())
    );
}

#[test]
fn parser_can_parse_filtered_delete() {
    let query =
        parse_delete(r#"delete Post filter .id = "post-1""#).expect("filtered delete should parse");

    assert_eq!(query.target_type_name(), "Post");
    let (left, right) = assert_compare_expr(
        query.filter().expect("delete should keep its filter"),
        CompareOp::Eq,
    );
    assert_path_expr(left, &["id"]);
    assert_literal_expr(right, &Literal::String("post-1".to_string()));
}

#[test]
fn parser_can_parse_unfiltered_delete() {
    let query = parse_delete("delete Post").expect("unfiltered delete should parse");

    assert_eq!(query.target_type_name(), "Post");
    assert!(query.filter().is_none());
}

#[test]
fn parser_rejects_delete_without_target_type() {
    let error = parse_delete("delete").expect_err("delete without target should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: "IDENT" }
    );
}

#[test]
fn parser_rejects_delete_filter_without_expression() {
    let error = parse_delete("delete Post filter").expect_err("empty filter should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof {
            expected: "expression"
        }
    );
}
