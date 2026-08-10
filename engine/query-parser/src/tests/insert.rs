use crate::{LexErrorKind, ParseErrorKind, TokenKind, lex, parse_insert};
use alloc::string::ToString;
use query_ast::{AssignmentValue, Literal};

#[test]
fn lexer_can_tokenize_insert_assignment() {
    let tokens = lex("insert User { name := \"Sheri\" }").expect("insert query should lex");

    match tokens[0].kind() {
        TokenKind::Keyword(keyword) => assert_eq!(keyword.as_str(), "insert"),
        token_kind => panic!("expected insert keyword, got {token_kind:?}"),
    }
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("User".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("name".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::ColonEq);
    assert_eq!(tokens[5].kind(), &TokenKind::String("Sheri".to_string()));
    assert_eq!(tokens[6].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_rejects_insert_assignment_with_unterminated_string() {
    let error =
        lex("insert User { name := \"Sheri }").expect_err("insert query should fail to lex");

    assert_eq!(error.kind(), &LexErrorKind::UnterminatedString);
    assert_eq!(error.position().byte(), 22);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 23);
}

#[test]
fn parser_can_parse_empty_insert() {
    let query = parse_insert("insert User {}").expect("query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert!(query.assignments().is_empty());
}

#[test]
fn parser_can_parse_insert_with_string_assignment() {
    let query =
        parse_insert("insert User { name := \"Sheri\" }").expect("insert query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 1);
    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String("Sheri".to_string()))
    );
}

#[test]
fn parser_can_parse_insert_with_string_link_id_assignment() {
    let query = parse_insert("insert Post { author := \"00000000-0000-0000-0000-000000000001\" }")
        .expect("insert query should parse");

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.assignments().len(), 1);
    assert_eq!(query.assignments()[0].field_name(), "author");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String(
            "00000000-0000-0000-0000-000000000001".to_string()
        ))
    );
}

#[test]
fn parser_can_parse_insert_with_multiple_assignment() {
    let query = parse_insert("insert User { name := \"Sheri\", email := \"sheri@tachibana.com\" }")
        .expect("insert query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 2);
    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String("Sheri".to_string()))
    );
    assert_eq!(query.assignments()[1].field_name(), "email");
    assert_eq!(
        query.assignments()[1].value(),
        &AssignmentValue::Literal(Literal::String("sheri@tachibana.com".to_string()))
    );
}

#[test]
fn parser_can_parse_insert_with_trailing_comma() {
    let query =
        parse_insert("insert User { name := \"Sheri\", }").expect("insert query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 1);
    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String("Sheri".to_string()))
    );
}

#[test]
fn parser_can_parse_insert_with_multiline_insert() {
    let query = parse_insert(
        "insert User\n  {\n    name := \"Sheri\",\n    email := \"sheri@tachibana.com\"\n  }",
    )
    .expect("insert query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 2);
    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String("Sheri".to_string()))
    );
    assert_eq!(query.assignments()[1].field_name(), "email");
    assert_eq!(
        query.assignments()[1].value(),
        &AssignmentValue::Literal(Literal::String("sheri@tachibana.com".to_string()))
    );
}

#[test]
fn parser_can_parse_insert_literal_assignment() {
    let query = parse_insert(
        "insert User { name := \"Sheri\", age := 15, weight := 55.0, alive := true, etc := null }",
    )
    .expect("insert query should parse");

    assert_eq!(query.root_type_name(), "User");
    assert_eq!(query.assignments().len(), 5);

    assert_eq!(query.assignments()[0].field_name(), "name");
    assert_eq!(
        query.assignments()[0].value(),
        &AssignmentValue::Literal(Literal::String("Sheri".to_string()))
    );

    assert_eq!(query.assignments()[1].field_name(), "age");
    assert_eq!(
        query.assignments()[1].value(),
        &AssignmentValue::Literal(Literal::Int64(15))
    );

    assert_eq!(query.assignments()[2].field_name(), "weight");
    assert_eq!(
        query.assignments()[2].value(),
        &AssignmentValue::Literal(Literal::Float64(55.0))
    );

    assert_eq!(query.assignments()[3].field_name(), "alive");
    assert_eq!(
        query.assignments()[3].value(),
        &AssignmentValue::Literal(Literal::Bool(true))
    );

    assert_eq!(query.assignments()[4].field_name(), "etc");
    assert_eq!(
        query.assignments()[4].value(),
        &AssignmentValue::Literal(Literal::Null)
    );
}

#[test]
fn parser_rejects_insert_without_target_type_with_object() {
    let error = parse_insert("insert { name := \"Sheri\" }").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: "IDENT" }
    );
}

#[test]
fn parser_rejects_insert_without_target_type() {
    let error = parse_insert("insert").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: "IDENT" }
    )
}

#[test]
fn parser_rejects_insert_without_object_literal() {
    let error = parse_insert("insert User").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: "{" }
    );
}

#[test]
fn parser_rejects_insert_assignment_without_field_name() {
    let error = parse_insert("insert User { := \"Sheri\"}").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: "IDENT" }
    )
}

#[test]
fn parser_rejects_insert_assignment_without_colon_eq() {
    let error = parse_insert("insert User { name \"Sheri\"").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: ":=" }
    )
}

#[test]
fn parser_rejects_insert_assignment_without_value() {
    let error = parse_insert("insert User { name := }").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "literal"
        }
    )
}

#[test]
fn parser_rejects_insert_assignment_without_value_at_eof() {
    let error = parse_insert("insert User { name := ").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof {
            expected: "literal"
        }
    )
}

#[test]
fn parser_rejects_insert_assignments_without_comma() {
    let error =
        parse_insert("insert User {   name := \"Sheri\"   email := \"sheri@tachibana.com\" }")
            .expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: "comma" }
    )
}
