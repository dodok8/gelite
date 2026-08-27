use super::fixtures::{
    assert_and_expr, assert_arithmetic_expr, assert_compare_expr, assert_literal_expr,
    assert_not_expr, assert_or_expr, assert_path_expr, assert_unary_arithmetic_expr,
};
use crate::{
    Keyword::{self},
    LexErrorKind, ParseErrorKind, TokenKind, lex, parse_select,
};
use alloc::string::ToString;
use query_ast::{
    ArithmeticOp, CompareOp, Expr, InOp, InRhs, Literal, OrderDirection, UnaryArithmeticOp,
};

fn assert_in_list(in_expr: &query_ast::InExpr) -> &[Expr] {
    let InRhs::List(values) = in_expr.right() else {
        panic!("membership right side should be a list");
    };

    values
}

#[test]
fn scoped_predicates_preserve_inner_and_outer_boolean_precedence() {
    let query = parse_select(
        "select Department { id } filter not exists .parent.employees { .name = \"타치바나 셰리\" or .active = true and not .nickname = null } and .id != \"x\"",
    ).expect("scoped predicate");
    let Expr::And(left, _) = query.filter().expect("filter") else {
        panic!("outer and")
    };
    let Expr::Not(inner) = left.as_ref() else {
        panic!("outer not")
    };
    let Expr::Scoped(scoped) = inner.as_ref() else {
        panic!("explicit scope")
    };
    assert_eq!(scoped.path().steps()[0].field_name(), "parent");
    assert_eq!(scoped.path().steps()[1].field_name(), "employees");
    let Expr::Or(_, right) = scoped.predicate() else {
        panic!("inner or")
    };
    let Expr::And(_, right) = right.as_ref() else {
        panic!("inner and")
    };
    assert!(matches!(right.as_ref(), Expr::Not(_)));

    let nested = parse_select(
        "select Department { id } filter exists .employees { exists .reports { .active = true } }",
    )
    .expect("nesting is rejected by resolution, not parsing");
    let Expr::Scoped(scoped) = nested.filter().expect("filter") else {
        panic!("scope")
    };
    assert!(matches!(scoped.predicate(), Expr::Scoped(_)));
}

#[test]
fn scoped_predicate_syntax_rejects_empty_or_unclosed_bodies_and_keeps_exists_identifiers() {
    for source in [
        "select Department { id } filter exists .employees {}",
        "select Department { id } filter exists .employees { .active = true",
        "select Department { id } filter exists employees { .active = true }",
        "select Department { id } filter exists .employees { .active = true, .name = \"A\" }",
    ] {
        assert!(parse_select(source).is_err(), "{source}");
    }
    for source in [
        "select Department { exists } filter exists = true",
        "select Department { exists } filter .exists = true",
        "select Department { id } filter exists.name = \"A\"",
        "select Department { id } filter exists .exists { .name = \"A\" }",
    ] {
        parse_select(source).expect("exists remains a contextual identifier");
    }
}

#[test]
fn lexer_can_tokenize_select_shape() {
    let tokens = lex("select Post { title }").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("Post".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_rejects_insert_assignment_with_unexpected_character() {
    let error = lex("insert User { name := @ }").expect_err("insert query should fail to lex");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().byte(), 22);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 23);
}

#[test]
fn lexer_rejects_insert_assignment_with_invalid_float_literal() {
    let error = lex("insert User { score := .5 }").expect_err("insert query should fail to lex");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('5'));
    assert_eq!(error.position().byte(), 24);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 25);
}

#[test]
fn lexer_tracks_line_and_column_for_tokens() {
    let tokens = lex("select Post {\n  title\n}").expect("query should lex");
    let title_span = tokens[3].span();

    assert_eq!(title_span.start().line(), 2);
    assert_eq!(title_span.start().column(), 3);
    assert_eq!(title_span.end().line(), 2);
    assert_eq!(title_span.end().column(), 8);
}

#[test]
fn lexer_can_tokenize_filter_path_and_string_literal() {
    let tokens = lex("filter .title = \"Hello\"").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Eq);
    assert_eq!(tokens[4].kind(), &TokenKind::String("Hello".to_string()));
}

#[test]
fn lexer_can_tokenize_comparison_operators() {
    let tokens = lex("filter .count != 1 .count < 2 .count <= 3 .count > 4 .count >= 5")
        .expect("query should lex");

    assert_eq!(tokens[3].kind(), &TokenKind::Ne);
    assert_eq!(tokens[7].kind(), &TokenKind::Lt);
    assert_eq!(tokens[11].kind(), &TokenKind::Le);
    assert_eq!(tokens[15].kind(), &TokenKind::Gt);
    assert_eq!(tokens[19].kind(), &TokenKind::Ge);
}

#[test]
fn lexer_can_tokenize_arithmetic_operators() {
    let tokens = lex("filter .view_count + 10 - 2 * 3 / 4 % 5 >= 100").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Ident("view_count".to_string())
    );
    assert_eq!(tokens[3].kind(), &TokenKind::Plus);
    assert_eq!(tokens[4].kind(), &TokenKind::Int("10".to_string()));
    assert_eq!(tokens[5].kind(), &TokenKind::Minus);
    assert_eq!(tokens[6].kind(), &TokenKind::Int("2".to_string()));
    assert_eq!(tokens[7].kind(), &TokenKind::Star);
    assert_eq!(tokens[8].kind(), &TokenKind::Int("3".to_string()));
    assert_eq!(tokens[9].kind(), &TokenKind::Slash);
    assert_eq!(tokens[10].kind(), &TokenKind::Int("4".to_string()));
    assert_eq!(tokens[11].kind(), &TokenKind::Percent);
    assert_eq!(tokens[12].kind(), &TokenKind::Int("5".to_string()));
    assert_eq!(tokens[13].kind(), &TokenKind::Ge);
    assert_eq!(tokens[14].kind(), &TokenKind::Int("100".to_string()));
}

#[test]
fn lexer_can_tokenize_function_call_expression() {
    let tokens = lex("filter f64(.view_count) / 2.0 >= 10.5").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("f64".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LParen);
    assert_eq!(tokens[3].kind(), &TokenKind::Dot);
    assert_eq!(
        tokens[4].kind(),
        &TokenKind::Ident("view_count".to_string())
    );
    assert_eq!(tokens[5].kind(), &TokenKind::RParen);
    assert_eq!(tokens[6].kind(), &TokenKind::Slash);
    assert_eq!(tokens[7].kind(), &TokenKind::Float("2.0".to_string()));
    assert_eq!(tokens[8].kind(), &TokenKind::Ge);
    assert_eq!(tokens[9].kind(), &TokenKind::Float("10.5".to_string()));
}

#[test]
fn lexer_can_tokenize_computed_shape_assignment() {
    let tokens = lex("select Post { score := .likes + 1 }").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("Post".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("score".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::ColonEq);
    assert_eq!(tokens[5].kind(), &TokenKind::Dot);
    assert_eq!(tokens[6].kind(), &TokenKind::Ident("likes".to_string()));
    assert_eq!(tokens[7].kind(), &TokenKind::Plus);
    assert_eq!(tokens[8].kind(), &TokenKind::Int("1".to_string()));
    assert_eq!(tokens[9].kind(), &TokenKind::RBrace);

    let assignment_span = tokens[4].span();
    assert_eq!(assignment_span.start().byte(), 20);
    assert_eq!(assignment_span.start().line(), 1);
    assert_eq!(assignment_span.start().column(), 21);
    assert_eq!(assignment_span.end().byte(), 22);
    assert_eq!(assignment_span.end().line(), 1);
    assert_eq!(assignment_span.end().column(), 23);
}

#[test]
fn lexer_tracks_arithmetic_operator_spans() {
    let tokens = lex("filter .x + 10 - 2 * 3 / 4 % 5").expect("query should lex");

    let plus_span = tokens[3].span();
    assert_eq!(plus_span.start().byte(), 10);
    assert_eq!(plus_span.start().line(), 1);
    assert_eq!(plus_span.start().column(), 11);
    assert_eq!(plus_span.end().byte(), 11);
    assert_eq!(plus_span.end().line(), 1);
    assert_eq!(plus_span.end().column(), 12);

    let minus_span = tokens[5].span();
    assert_eq!(minus_span.start().byte(), 15);
    assert_eq!(minus_span.start().line(), 1);
    assert_eq!(minus_span.start().column(), 16);
    assert_eq!(minus_span.end().byte(), 16);
    assert_eq!(minus_span.end().line(), 1);
    assert_eq!(minus_span.end().column(), 17);

    let star_span = tokens[7].span();
    assert_eq!(star_span.start().byte(), 19);
    assert_eq!(star_span.start().line(), 1);
    assert_eq!(star_span.start().column(), 20);
    assert_eq!(star_span.end().byte(), 20);
    assert_eq!(star_span.end().line(), 1);
    assert_eq!(star_span.end().column(), 21);

    let slash_span = tokens[9].span();
    assert_eq!(slash_span.start().byte(), 23);
    assert_eq!(slash_span.start().line(), 1);
    assert_eq!(slash_span.start().column(), 24);
    assert_eq!(slash_span.end().byte(), 24);
    assert_eq!(slash_span.end().line(), 1);
    assert_eq!(slash_span.end().column(), 25);

    let percent_span = tokens[11].span();
    assert_eq!(percent_span.start().byte(), 27);
    assert_eq!(percent_span.start().line(), 1);
    assert_eq!(percent_span.start().column(), 28);
    assert_eq!(percent_span.end().byte(), 28);
    assert_eq!(percent_span.end().line(), 1);
    assert_eq!(percent_span.end().column(), 29);
}

#[test]
fn lexer_can_tokenize_decimal_float_literals() {
    let tokens = lex("filter .score / 2.5 >= 10.5").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("score".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Slash);
    assert_eq!(tokens[4].kind(), &TokenKind::Float("2.5".to_string()));
    assert_eq!(tokens[5].kind(), &TokenKind::Ge);
    assert_eq!(tokens[6].kind(), &TokenKind::Float("10.5".to_string()));
}

#[test]
fn lexer_keeps_path_dot_separate_from_decimal_float_dot() {
    let tokens = lex("filter .score >= 0.5").expect("query should lex");

    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("score".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Ge);
    assert_eq!(tokens[4].kind(), &TokenKind::Float("0.5".to_string()));
}

#[test]
fn lexer_rejects_float_literal_without_integer_part() {
    let error = lex("filter .score >= .5").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('5'));
    assert_eq!(error.position().byte(), 18);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 19);
}

#[test]
fn lexer_rejects_float_literal_without_fractional_part() {
    let error = lex("filter .score >= 5.").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('.'));
    assert_eq!(error.position().byte(), 18);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 19);
}

#[test]
fn lexer_can_tokenize_membership_list_brackets() {
    let tokens = lex("filter .status in [\"draft\", \"published\"]").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Filter));
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("status".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("in".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::LBracket);
    assert_eq!(tokens[5].kind(), &TokenKind::String("draft".to_string()));
    assert_eq!(tokens[6].kind(), &TokenKind::Comma);
    assert_eq!(
        tokens[7].kind(),
        &TokenKind::String("published".to_string())
    );
    assert_eq!(tokens[8].kind(), &TokenKind::RBracket);
}

#[test]
fn lexer_reports_unexpected_character_position() {
    let error = lex("select Post {\n  @\n}").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 2);
    assert_eq!(error.position().column(), 3);
}

#[test]
fn lexer_reports_unterminated_string_start_position() {
    let error = lex("filter .title = \"Hello").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnterminatedString);
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 17);
}

#[test]
fn lexer_can_tokenize_nested_shape() {
    let tokens =
        lex("select Post {\n  title,\n  author: {\n    name\n  }\n}").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[5].kind(), &TokenKind::Ident("author".to_string()));
    assert_eq!(tokens[6].kind(), &TokenKind::Colon);
    assert_eq!(tokens[7].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[8].kind(), &TokenKind::Ident("name".to_string()));
    assert_eq!(tokens[9].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_tracks_columns_after_newline() {
    let tokens = lex("select Post {\n    title\n}").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Select));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));

    let title_span = tokens[3].span();
    assert_eq!(title_span.start().byte(), 18);
    assert_eq!(title_span.start().line(), 2);
    assert_eq!(title_span.start().column(), 5);
    assert_eq!(title_span.end().byte(), 23);
    assert_eq!(title_span.end().line(), 2);
    assert_eq!(title_span.end().column(), 10);
}

#[test]
fn lexer_tracks_byte_and_column_inside_unicode_string_literal() {
    let tokens = lex("filter .title = \"안녕\" title").expect("query should lex");

    let string_span = tokens[4].span();
    assert_eq!(tokens[4].kind(), &TokenKind::String("안녕".to_string()));
    assert_eq!(string_span.start().line(), 1);
    assert_eq!(string_span.start().column(), 17);
    assert_eq!(string_span.start().byte(), 16);
    assert_eq!(string_span.end().column(), 21);
    assert_eq!(string_span.end().byte(), 24);

    let title_span = tokens[5].span();
    assert_eq!(tokens[5].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(title_span.start().line(), 1);
    assert_eq!(title_span.start().column(), 22);
    assert_eq!(title_span.start().byte(), 25);
}

#[test]
fn lexer_can_tokenize_multiline_string_literal() {
    let tokens = lex("filter .body = \"hello\nworld\" title").expect("query should lex");

    assert_eq!(
        tokens[4].kind(),
        &TokenKind::String("hello\nworld".to_string())
    );

    let string_span = tokens[4].span();
    assert_eq!(string_span.start().line(), 1);
    assert_eq!(string_span.start().column(), 16);
    assert_eq!(string_span.end().line(), 2);
    assert_eq!(string_span.end().column(), 7);

    assert_eq!(tokens[5].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[5].span().start().line(), 2);
    assert_eq!(tokens[5].span().start().column(), 8);
}

#[test]
fn lexer_can_tokenize_order_limit_offset() {
    let tokens = lex("order by .title desc limit 10 offset 20").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Order));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::By));
    assert_eq!(tokens[2].kind(), &TokenKind::Dot);
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("title".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::Keyword(Keyword::Desc));
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::Limit));
    assert_eq!(tokens[6].kind(), &TokenKind::Int("10".to_string()));
    assert_eq!(tokens[7].kind(), &TokenKind::Keyword(Keyword::Offset));
    assert_eq!(tokens[8].kind(), &TokenKind::Int("20".to_string()));
}

#[test]
fn lexer_distinguishes_keyword_prefix_identifiers() {
    let tokens =
        lex("select Post { orderValue, offsetCount, filterText }").expect("query should lex");

    assert_eq!(
        tokens[3].kind(),
        &TokenKind::Ident("orderValue".to_string())
    );
    assert_eq!(
        tokens[5].kind(),
        &TokenKind::Ident("offsetCount".to_string())
    );
    assert_eq!(
        tokens[7].kind(),
        &TokenKind::Ident("filterText".to_string())
    );
}

#[test]
fn lexer_can_tokenize_literal_keywords() {
    let tokens = lex("true false null").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::True));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::False));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Null));
}

#[test]
fn lexer_treats_boolean_operator_words_as_identifiers() {
    let tokens = lex("and or not").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("and".to_string()));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("or".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("not".to_string()));
}

#[test]
fn lexer_treats_membership_operator_word_as_identifier() {
    let tokens = lex("in").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("in".to_string()));
}

#[test]
fn lexer_reports_unexpected_character_byte_offset() {
    let error = lex("select Post { @ }").expect_err("query should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 1);
    assert_eq!(error.position().column(), 15);
    assert_eq!(error.position().byte(), 14);
}

#[test]
fn lexer_tokenizes_negative_integer_as_minus_and_integer() {
    let tokens = lex("- 1").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Minus);
    assert_eq!(tokens[1].kind(), &TokenKind::Int("1".to_string()));
}

#[test]
fn lexer_tokenizes_positive_integer_as_plus_and_integer() {
    let tokens = lex("+ 1").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Plus);
    assert_eq!(tokens[1].kind(), &TokenKind::Int("1".to_string()));
}

#[test]
fn lexer_tokenizes_unary_path_without_merging_tokens() {
    let tokens = lex("- .view_count").expect("query should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Minus);
    assert_eq!(tokens[1].kind(), &TokenKind::Dot);
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Ident("view_count".to_string())
    );
}

#[test]
fn parser_can_parse_select_shape() {
    let query = parse_select("select Post { title }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "title");
    assert!(item.child_shape().is_none());

    assert!(query.filter().is_none());
    assert!(query.order_by().is_empty());
    assert_eq!(query.limit(), None);
    assert_eq!(query.offset(), None);
}

#[test]
fn parser_preserves_shape_item_order() {
    let query = parse_select("select Post { id, title }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");
    assert_eq!(query.shape().items().len(), 2);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "id");
    assert!(item.child_shape().is_none());

    let item = &query.shape().items()[1];
    assert_eq!(item.path().steps()[0].field_name(), "title");
    assert!(item.child_shape().is_none());

    assert!(query.filter().is_none());
    assert!(query.order_by().is_empty());
    assert_eq!(query.limit(), None);
    assert_eq!(query.offset(), None);
}

#[test]
fn parser_can_parse_shape_item_named_boolean_operator_word() {
    let query = parse_select("select Post { or }").expect("query should parse");

    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "or");
    assert!(item.child_shape().is_none());
}

#[test]
fn parser_can_parse_shape_item_named_membership_operator_word() {
    let query = parse_select("select Post { in }").expect("query should parse");

    assert_eq!(query.shape().items().len(), 1);

    let item = &query.shape().items()[0];
    assert_eq!(item.path().steps()[0].field_name(), "in");
    assert!(item.child_shape().is_none());
}

#[test]
fn parser_can_parse_computed_shape_item_arithmetic_expr() {
    let query = parse_select("select Post { score := .likes * 10 + .view_count }")
        .expect("query should parse");

    let items = query.shape().items();

    assert_eq!(items.len(), 1);
    let computed = items[0]
        .as_computed()
        .expect("shape item should be a computed projection");
    assert_eq!(computed.output_name(), "score");

    let Expr::Arithmetic(add) = computed.expr() else {
        panic!("computed projection should be an arithmetic expression");
    };
    assert_eq!(add.op(), ArithmeticOp::Add);

    let Expr::Arithmetic(mul) = add.left() else {
        panic!("multiplication should bind before addition");
    };
    assert_eq!(mul.op(), ArithmeticOp::Mul);
    assert_path_expr(mul.left(), &["likes"]);
    assert_literal_expr(mul.right(), &Literal::Int64(10));
    assert_path_expr(add.right(), &["view_count"]);
}

#[test]
fn parser_can_parse_computed_shape_item_unary_arithmetic_expr() {
    let query = parse_select("select Post { score := -.view_count }").expect("query should parse");

    let items = query.shape().items();

    assert_eq!(items.len(), 1);
    let computed = items[0]
        .as_computed()
        .expect("shape item should be a computed projection");
    assert_eq!(computed.output_name(), "score");

    let operand = assert_unary_arithmetic_expr(computed.expr(), UnaryArithmeticOp::Minus);
    assert_path_expr(operand, &["view_count"]);
}

#[test]
fn parser_rejects_adjacent_shape_items_without_comma() {
    let error =
        parse_select("select Post { id title }").expect_err("query should fail without comma");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: ", or }" }
    );

    let span = error
        .span()
        .expect("parse error should point to unexpected token");

    assert_eq!(span.start().line(), 1);
    assert_eq!(span.start().column(), 18);
    assert_eq!(span.start().byte(), 17);
    assert_eq!(span.end().line(), 1);
    assert_eq!(span.end().column(), 23);
    assert_eq!(span.end().byte(), 22);
}

#[test]
fn parser_can_parse_nested_shape_item() {
    let query = parse_select("select Post { author: { name } }").expect("query should parse");

    assert_eq!(query.root_type_name(), "Post");

    let root_items = query.shape().items();
    assert_eq!(root_items.len(), 1);

    let author_item = &root_items[0];
    assert_eq!(author_item.path().steps().len(), 1);
    assert_eq!(author_item.path().steps()[0].field_name(), "author");

    let author_shape = author_item
        .child_shape()
        .expect("author should have child shape");

    let author_items = author_shape.items();
    assert_eq!(author_items.len(), 1);

    let name_item = &author_items[0];
    assert_eq!(name_item.path().steps().len(), 1);
    assert_eq!(name_item.path().steps()[0].field_name(), "name");
    assert!(name_item.child_shape().is_none());
}

#[test]
fn parser_can_parse_deeply_nested_shape_item() {
    let query = parse_select("select Post { author: { human: { birthday } } }")
        .expect("query should parse");

    let author_item = &query.shape().items()[0];
    assert_eq!(author_item.path().steps()[0].field_name(), "author");

    let author_shape = author_item
        .child_shape()
        .expect("author should have child shape");
    let human_item = &author_shape.items()[0];
    assert_eq!(human_item.path().steps()[0].field_name(), "human");

    let human_shape = human_item
        .child_shape()
        .expect("human should have child shape");
    let birthday_item = &human_shape.items()[0];
    assert_eq!(birthday_item.path().steps()[0].field_name(), "birthday");
    assert!(birthday_item.child_shape().is_none());
}

#[test]
fn parser_can_parse_filter_compare_path_equals_string_literal() {
    let query =
        parse_select("select Post {title} filter .title = \"Hello\"").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["title"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::String("Hello".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_nested_path_equals_string_literal() {
    let query = parse_select("select Post { title } filter .author.name = \"Sheri\"")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["author", "name"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::String("Sheri".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_integer_literal() {
    let query =
        parse_select("select Post { title } filter .view_count = 42").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["view_count"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Int64(42));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_arithmetic_addition() {
    let query = parse_select("select Post { title } filter .view_count + 10 >= 100")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["view_count"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Int64(100));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_function_call_as_arithmetic_operand() {
    let query = parse_select("select Post { title } filter f64(.view_count) / 2.0 >= 10.5")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let Expr::Arithmetic(arithmetic) = compare.left() else {
                panic!("left side should be arithmetic expression");
            };
            assert_eq!(arithmetic.op(), ArithmeticOp::Div);

            let Expr::FunctionCall(function) = arithmetic.left() else {
                panic!("arithmetic left operand should be a function call");
            };
            assert_eq!(function.name(), "f64");
            assert_eq!(function.args().len(), 1);
            assert_path_expr(&function.args()[0], &["view_count"]);

            assert_literal_expr(arithmetic.right(), &Literal::Float64(2.0));
            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Float64(10.5));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_function_call_argument_count() {
    let query = parse_select("select Post { title } filter concat() = concat(.title, \"!\")")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let Expr::FunctionCall(left) = compare.left() else {
                panic!("left side should be a function call");
            };
            assert_eq!(left.name(), "concat");
            assert_eq!(left.args().len(), 0);

            let Expr::FunctionCall(right) = compare.right() else {
                panic!("right side should be a function call");
            };
            assert_eq!(right.name(), "concat");
            assert_eq!(right.args().len(), 2);
            assert_path_expr(&right.args()[0], &["title"]);
            assert_literal_expr(&right.args()[1], &Literal::String("!".to_string()));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_variadic_concat_arguments() {
    let query = parse_select(
        "select Post { title } filter concat(.title, \" - \", .subtitle) = \"Hello - Draft\"",
    )
    .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let Expr::FunctionCall(function) = compare.left() else {
                panic!("left side should be a function call");
            };

            assert_eq!(function.name(), "concat");
            assert_eq!(function.args().len(), 3);
            assert_path_expr(&function.args()[0], &["title"]);
            assert_literal_expr(&function.args()[1], &Literal::String(" - ".to_string()));
            assert_path_expr(&function.args()[2], &["subtitle"]);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_str_function_call_argument() {
    let query = parse_select("select Post { title } filter str(.published) = \"true\"")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let Expr::FunctionCall(function) = compare.left() else {
                panic!("left side should be a function call");
            };

            assert_eq!(function.name(), "str");
            assert_eq!(function.args().len(), 1);
            assert_path_expr(&function.args()[0], &["published"]);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_unary_minus_integer_literal() {
    let query =
        parse_select("select Post { title } filter -1 < .view_count").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let operand = assert_unary_arithmetic_expr(compare.left(), UnaryArithmeticOp::Minus);
            assert_literal_expr(operand, &Literal::Int64(1));

            assert_eq!(compare.op(), CompareOp::Lt);
            assert_path_expr(compare.right(), &["view_count"]);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_unary_plus_integer_literal() {
    let query =
        parse_select("select Post { title } filter +1 < .view_count").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let operand = assert_unary_arithmetic_expr(compare.left(), UnaryArithmeticOp::Plus);
            assert_literal_expr(operand, &Literal::Int64(1));

            assert_eq!(compare.op(), CompareOp::Lt);
            assert_path_expr(compare.right(), &["view_count"]);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_i64_min_literal_after_unary_minus_in_comparison() {
    let query = parse_select("select Post { title } filter .view_count = -9223372036854775808")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["view_count"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Int64(i64::MIN));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_i64_min_literal_after_unary_minus_in_value_expr_positions() {
    let query = parse_select(
        "select Post { min := -9223372036854775808 } filter .view_count in [-9223372036854775808] order by -9223372036854775808 asc",
    )
    .expect("query should parse");

    let items = query.shape().items();
    let computed = items[0]
        .as_computed()
        .expect("shape item should be a computed projection");
    assert_literal_expr(computed.expr(), &Literal::Int64(i64::MIN));

    let filter = query.filter().expect("query should have filter");
    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["view_count"]);
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);
            assert_eq!(right.len(), 1);
            assert_literal_expr(&right[0], &Literal::Int64(i64::MIN));
        }
        _ => panic!("filter should be in expression"),
    }

    assert_eq!(query.order_by().len(), 1);
    assert_literal_expr(query.order_by()[0].expr(), &Literal::Int64(i64::MIN));
    assert_eq!(query.order_by()[0].direction(), OrderDirection::Asc);
}

#[test]
fn parser_parses_unary_minus_path() {
    let query =
        parse_select("select Post { title } filter -.view_count < 0").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let operand = assert_unary_arithmetic_expr(compare.left(), UnaryArithmeticOp::Minus);
            assert_path_expr(operand, &["view_count"]);

            assert_eq!(compare.op(), CompareOp::Lt);
            assert_literal_expr(compare.right(), &Literal::Int64(0));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_treats_after_left_expr_as_binary_subtraction() {
    let query = parse_select("select Post { title } filter .view_count -1 >= 0")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["view_count"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Sub);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(1));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Int64(0));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_unary_arithmetic_before_multiplication() {
    let query =
        parse_select("select Post { title } filter -.score * 2 < 0").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    let operand =
                        assert_unary_arithmetic_expr(arithmetic.left(), UnaryArithmeticOp::Minus);
                    assert_path_expr(operand, &["score"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Mul);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(2));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Lt);
            assert_literal_expr(compare.right(), &Literal::Int64(0));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_parenthesized_arithmetic_as_unary_operand() {
    let query =
        parse_select("select Post { title } filter -(.score + 1) < 0").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            let operand = assert_unary_arithmetic_expr(compare.left(), UnaryArithmeticOp::Minus);
            match operand {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["score"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(1));
                }
                other => {
                    panic!("unary operand should preserve parenthesized addition, got {other:?}")
                }
            }

            assert_eq!(compare.op(), CompareOp::Lt);
            assert_literal_expr(compare.right(), &Literal::Int64(0));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_unary_arithmetic_on_comparison_right_side() {
    let query =
        parse_select("select Post { title } filter .view_count = -1").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["view_count"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            let operand = assert_unary_arithmetic_expr(compare.right(), UnaryArithmeticOp::Minus);
            assert_literal_expr(operand, &Literal::Int64(1));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_float_arithmetic_literals() {
    let query = parse_select("select Post { title } filter .score / 2.5 >= 10.5")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["score"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Div);
                    assert_literal_expr(arithmetic.right(), &Literal::Float64(2.5));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Float64(10.5));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_multiplicative_precedence() {
    let query = parse_select("select Post { title } filter .likes + .view_count * 10 >= 100")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["likes"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Add);

                    match arithmetic.right() {
                        Expr::Arithmetic(arithmetic) => {
                            assert_path_expr(arithmetic.left(), &["view_count"]);
                            assert_eq!(arithmetic.op(), ArithmeticOp::Mul);
                            assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                        }
                        other => {
                            panic!("right side should be arithmetic expression, got {other:?}")
                        }
                    }
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Int64(100));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_parenthesized_arithmetic_grouping() {
    let query = parse_select("select Post { title } filter (.likes + .view_count) * 10 >= 100")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    match arithmetic.left() {
                        Expr::Arithmetic(arithmetic) => {
                            assert_path_expr(arithmetic.left(), &["likes"]);
                            assert_eq!(arithmetic.op(), ArithmeticOp::Add);
                            assert_path_expr(arithmetic.right(), &["view_count"]);
                        }
                        other => {
                            panic!("left side should be arithmetic expression, got {other:?}")
                        }
                    }
                    assert_eq!(arithmetic.op(), ArithmeticOp::Mul);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }

            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Int64(100));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_arithmetic_as_left_associative() {
    let query = parse_select("select Post { title } filter .view_count - 10 - 5 >= 0")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    match arithmetic.left() {
                        Expr::Arithmetic(arithmetic) => {
                            assert_path_expr(arithmetic.left(), &["view_count"]);
                            assert_eq!(arithmetic.op(), ArithmeticOp::Sub);
                            assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                        }
                        other => {
                            panic!("right side should be arithmetic expression, got {other:?}")
                        }
                    }
                    assert_eq!(arithmetic.op(), ArithmeticOp::Sub);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(5));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }
            assert_eq!(compare.op(), CompareOp::Ge);
            assert_literal_expr(compare.right(), &Literal::Int64(0));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_parses_division_and_modulo_as_left_associative() {
    let query = parse_select("select Post { title } filter .view_count / 2 % 3 = 1")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            match compare.left() {
                Expr::Arithmetic(arithmetic) => {
                    match arithmetic.left() {
                        Expr::Arithmetic(arithmetic) => {
                            assert_path_expr(arithmetic.left(), &["view_count"]);
                            assert_eq!(arithmetic.op(), ArithmeticOp::Div);
                            assert_literal_expr(arithmetic.right(), &Literal::Int64(2));
                        }
                        other => {
                            panic!("right side should be arithmetic expression, got {other:?}")
                        }
                    }
                    assert_eq!(arithmetic.op(), ArithmeticOp::Mod);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(3));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Int64(1));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_arithmetic_on_comparison_right_side() {
    let query = parse_select("select Post { title } filter 100 <= .view_count + 10")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_literal_expr(compare.left(), &Literal::Int64(100));
            assert_eq!(compare.op(), CompareOp::Le);
            match compare.right() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["view_count"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                }
                other => panic!("right side should be arithmetic expression, got {other:?}"),
            }
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_arithmetic_in_membership_left_side() {
    let query = parse_select("select Post { title } filter .view_count % 10 in [0, 5]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            match in_expr.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["view_count"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Mod);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);
            assert_literal_expr(&right[0], &Literal::Int64(0));
            assert_literal_expr(&right[1], &Literal::Int64(5));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_arithmetic_in_not_in_membership_left_side() {
    let query = parse_select("select Post { title } filter .view_count % 10 not in [0, 5]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            match in_expr.left() {
                Expr::Arithmetic(arithmetic) => {
                    assert_path_expr(arithmetic.left(), &["view_count"]);
                    assert_eq!(arithmetic.op(), ArithmeticOp::Mod);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
                }
                other => panic!("left side should be arithmetic expression, got {other:?}"),
            }
            assert_eq!(in_expr.op(), InOp::NotIn);
            let right = assert_in_list(in_expr);
            assert_literal_expr(&right[0], &Literal::Int64(0));
            assert_literal_expr(&right[1], &Literal::Int64(5));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_arithmetic_in_membership_rhs_for_resolver() {
    let query = parse_select("select Post { title } filter .view_count in [1 + 1]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["view_count"]);
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);
            match &right[0] {
                Expr::Arithmetic(arithmetic) => {
                    assert_literal_expr(arithmetic.left(), &Literal::Int64(1));
                    assert_eq!(arithmetic.op(), ArithmeticOp::Add);
                    assert_literal_expr(arithmetic.right(), &Literal::Int64(1));
                }
                other => panic!("right side should be arithmetic expression, got {other:?}"),
            }
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_preserves_unary_arithmetic_in_membership_rhs_for_resolver() {
    let query = parse_select("select Post { title } filter .view_count in [-1, +2]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["view_count"]);
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);

            let operand = assert_unary_arithmetic_expr(&right[0], UnaryArithmeticOp::Minus);
            assert_literal_expr(operand, &Literal::Int64(1));

            let operand = assert_unary_arithmetic_expr(&right[1], UnaryArithmeticOp::Plus);
            assert_literal_expr(operand, &Literal::Int64(2));
        }
        _ => panic!("filter should be membership expression"),
    }
}

#[test]
fn parser_preserves_boolean_precedence_with_arithmetic() {
    let query = parse_select(
        "select Post { title } filter .views + 1 >= 10 and .likes * 2 >= 20 or not .archived = true",
    )
    .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    let (or_left, or_right) = assert_or_expr(filter);
    let (and_left, and_right) = assert_and_expr(or_left);

    let (views_compare_left, views_compare_right) = assert_compare_expr(and_left, CompareOp::Ge);
    let (views_arithmetic_left, views_arithmetic_right) =
        assert_arithmetic_expr(views_compare_left, ArithmeticOp::Add);
    assert_path_expr(views_arithmetic_left, &["views"]);
    assert_literal_expr(views_arithmetic_right, &Literal::Int64(1));
    assert_literal_expr(views_compare_right, &Literal::Int64(10));

    let (likes_compare_left, likes_compare_right) = assert_compare_expr(and_right, CompareOp::Ge);
    let (likes_arithmetic_left, likes_arithmetic_right) =
        assert_arithmetic_expr(likes_compare_left, ArithmeticOp::Mul);
    assert_path_expr(likes_arithmetic_left, &["likes"]);
    assert_literal_expr(likes_arithmetic_right, &Literal::Int64(2));
    assert_literal_expr(likes_compare_right, &Literal::Int64(20));

    let not_operand = assert_not_expr(or_right);
    let (archived_compare_left, archived_compare_right) =
        assert_compare_expr(not_operand, CompareOp::Eq);
    assert_path_expr(archived_compare_left, &["archived"]);
    assert_literal_expr(archived_compare_right, &Literal::Bool(true));
}

#[test]
fn parser_can_parse_filter_compare_path_equals_true_literal() {
    let query =
        parse_select("select Post { title } filter .published = true").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["published"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Bool(true));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_false_literal() {
    let query = parse_select("select Post { title } filter .published = false")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["published"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Bool(false));
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_compare_path_equals_null_literal() {
    let query = parse_select("select Post { title } filter .deleted_at = null")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Compare(compare) => {
            assert_path_expr(compare.left(), &["deleted_at"]);
            assert_eq!(compare.op(), CompareOp::Eq);
            assert_literal_expr(compare.right(), &Literal::Null);
        }
        _ => panic!("filter should be compare expression"),
    }
}

#[test]
fn parser_can_parse_filter_comparison_operators() {
    let cases = [
        ("!=", CompareOp::Ne),
        ("<", CompareOp::Lt),
        ("<=", CompareOp::Le),
        (">", CompareOp::Gt),
        (">=", CompareOp::Ge),
    ];

    for (source_op, expected_op) in cases {
        let source = alloc::format!("select Post {{ title }} filter .view_count {source_op} 42");
        let query = parse_select(&source).expect("query should parse");
        let filter = query.filter().expect("query should have filter");

        match filter {
            Expr::Compare(compare) => {
                assert_path_expr(compare.left(), &["view_count"]);
                assert_eq!(compare.op(), expected_op);
                assert_literal_expr(compare.right(), &Literal::Int64(42));
            }
            _ => panic!("filter should be compare expression"),
        }
    }
}

#[test]
fn parser_can_parse_filter_in_literal_list() {
    let query = parse_select("select Post { title } filter .status in [\"draft\", \"published\"]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);
            assert_eq!(right.len(), 2);
            assert_literal_expr(&right[0], &Literal::String("draft".to_string()));
            assert_literal_expr(&right[1], &Literal::String("published".to_string()));
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_can_parse_filter_not_in_literal_list() {
    let query = parse_select("select Post { title } filter .status not in [\"archived\"]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::NotIn);
            let right = assert_in_list(in_expr);
            assert_eq!(right.len(), 1);
            assert_literal_expr(&right[0], &Literal::String("archived".to_string()));
        }
        _ => panic!("filter should be a not in expression"),
    }
}

#[test]
fn parser_can_parse_filter_in_select() {
    let query = parse_select(
        r#"select Post { title }
        filter .author.id in (
            select User { id }
            filter .email = "sheri@example.com"
        )"#,
    )
    .expect("query should parse");

    let Expr::In(in_expr) = query.filter().expect("query should have filter") else {
        panic!("filter should be a membership expression");
    };

    assert_path_expr(in_expr.left(), &["author", "id"]);
    assert_eq!(in_expr.op(), InOp::In);

    let InRhs::Select(select) = in_expr.right() else {
        panic!("membership right side should be a select");
    };

    assert_eq!(select.root_type_name(), "User");
    assert_eq!(select.shape().items().len(), 1);
    assert!(select.filter().is_some());
}

#[test]
fn parser_can_parse_filter_not_in_select() {
    let query = parse_select("select Post { title } filter .author.id not in (select User { id })")
        .expect("query should parse");

    let Expr::In(in_expr) = query.filter().expect("query should have filter") else {
        panic!("filter should be a membership expression");
    };

    assert_path_expr(in_expr.left(), &["author", "id"]);
    assert_eq!(in_expr.op(), InOp::NotIn);
    assert!(matches!(in_expr.right(), InRhs::Select(_)));
}

#[test]
fn parser_preserves_boolean_precedence_after_membership_select() {
    let query = parse_select(
        "select Post { title } filter .author.id in (select User { id }) or .archived = true",
    )
    .expect("query should parse");

    let filter = query.filter().expect("query should have filter");
    let (left, right) = assert_or_expr(filter);

    assert!(matches!(left, Expr::In(_)));
    assert!(matches!(right, Expr::Compare(_)));
}

#[test]
fn parser_rejects_membership_select_without_closing_paren() {
    let error = parse_select("select Post { title } filter .author.id in (select User { id }")
        .expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof { expected: ")" }
    );
}

#[test]
fn parser_can_parse_filter_in_empty_list() {
    let query =
        parse_select("select Post { title } filter .status in []").expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            assert!(assert_in_list(in_expr).is_empty());
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_preserves_null_and_path_membership_items_for_resolver() {
    let query = parse_select("select Post { title } filter .status in [null, .other_status]")
        .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::In(in_expr) => {
            assert_path_expr(in_expr.left(), &["status"]);
            assert_eq!(in_expr.op(), InOp::In);
            let right = assert_in_list(in_expr);
            assert_eq!(right.len(), 2);
            assert_literal_expr(&right[0], &Literal::Null);
            assert_path_expr(&right[1], &["other_status"]);
        }
        _ => panic!("filter should be an in expression"),
    }
}

#[test]
fn parser_preserves_membership_precedence_with_boolean_or() {
    let query =
        parse_select("select Post { title } filter .status in [\"draft\"] or .title = \"Hello\"")
            .expect("query should parse");

    let filter = query.filter().expect("query should have filter");

    match filter {
        Expr::Or(left, right) => {
            assert!(matches!(left.as_ref(), Expr::In(_)));
            assert!(matches!(right.as_ref(), Expr::Compare(_)));
        }
        _ => panic!("filter should be an or expression"),
    }
}

#[test]
fn parser_can_parse_select_without_filter() {
    let query = parse_select("select Post { title }").expect("query should parse");

    assert!(query.filter().is_none());
}

#[test]
fn parser_rejects_filter_without_path() {
    let error =
        parse_select("select Post { title } filter = \"Hello\"").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "expression"
        }
    );
}

#[test]
fn parser_rejects_filter_without_comparison_operator() {
    let error = parse_select("select Post { title } filter title \"Hello\"")
        .expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "comparison operator"
        }
    );
}

#[test]
fn parser_rejects_filter_without_literal() {
    let error =
        parse_select("select Post { title } filter title =").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedEof {
            expected: "expression"
        }
    );
}

#[test]
fn parser_can_parse_order_by_path_desc() {
    let query =
        parse_select("select Post {title} order by .title desc").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_path_expr(order.expr(), &["title"]);
    assert_eq!(order.direction(), OrderDirection::Desc);
}

#[test]
fn parser_defaults_order_direction_to_asc() {
    let query = parse_select("select Post {title} order by .title").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_path_expr(order.expr(), &["title"]);
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_can_parse_order_by_nested_path() {
    let query =
        parse_select("select Post {title} order by .author.birthday").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    assert_path_expr(order.expr(), &["author", "birthday"]);
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_can_parse_multiple_order_by_items() {
    let query = parse_select("select Post {title} order by .title desc, .created_at asc")
        .expect("query should parse");

    assert_eq!(query.order_by().len(), 2);

    let order = &query.order_by()[0];
    assert_path_expr(order.expr(), &["title"]);
    assert_eq!(order.direction(), OrderDirection::Desc);

    let order = &query.order_by()[1];
    assert_path_expr(order.expr(), &["created_at"]);
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_can_parse_order_by_arithmetic_expr_desc() {
    let query = parse_select("select Post {title} order by .likes + .view_count desc")
        .expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    match order.expr() {
        Expr::Arithmetic(arithmetic) => {
            assert_path_expr(arithmetic.left(), &["likes"]);
            assert_eq!(arithmetic.op(), ArithmeticOp::Add);
            assert_path_expr(arithmetic.right(), &["view_count"]);
        }
        other => panic!("order by should parse arithmetic expression, got {other:?}"),
    }
    assert_eq!(order.direction(), OrderDirection::Desc);
}

#[test]
fn parser_can_parse_order_by_unary_arithmetic_expr_desc() {
    let query =
        parse_select("select Post {title} order by -.view_count desc").expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    let operand = assert_unary_arithmetic_expr(order.expr(), UnaryArithmeticOp::Minus);
    assert_path_expr(operand, &["view_count"]);
    assert_eq!(order.direction(), OrderDirection::Desc);
}

#[test]
fn parser_preserves_parenthesized_order_by_arithmetic_grouping() {
    let query = parse_select("select Post {title} order by (.likes + .view_count) * 10 desc")
        .expect("query should parse");

    assert_eq!(query.order_by().len(), 1);

    let order = &query.order_by()[0];
    match order.expr() {
        Expr::Arithmetic(arithmetic) => {
            match arithmetic.left() {
                Expr::Arithmetic(inner) => {
                    assert_path_expr(inner.left(), &["likes"]);
                    assert_eq!(inner.op(), ArithmeticOp::Add);
                    assert_path_expr(inner.right(), &["view_count"]);
                }
                other => panic!("left side should preserve parenthesized addition, got {other:?}"),
            }

            assert_eq!(arithmetic.op(), ArithmeticOp::Mul);
            assert_literal_expr(arithmetic.right(), &Literal::Int64(10));
        }
        other => panic!("order by should parse arithmetic expression, got {other:?}"),
    }
    assert_eq!(order.direction(), OrderDirection::Desc);
}

#[test]
fn parser_can_parse_multiple_order_by_value_expr_items() {
    let query = parse_select("select Post {title} order by .likes + 1 desc, .created_at asc")
        .expect("query should parse");

    assert_eq!(query.order_by().len(), 2);

    let order = &query.order_by()[0];
    match order.expr() {
        Expr::Arithmetic(arithmetic) => {
            assert_path_expr(arithmetic.left(), &["likes"]);
            assert_eq!(arithmetic.op(), ArithmeticOp::Add);
            assert_literal_expr(arithmetic.right(), &Literal::Int64(1));
        }
        other => panic!("first order item should parse arithmetic expression, got {other:?}"),
    }
    assert_eq!(order.direction(), OrderDirection::Desc);

    let order = &query.order_by()[1];
    assert_path_expr(order.expr(), &["created_at"]);
    assert_eq!(order.direction(), OrderDirection::Asc);
}

#[test]
fn parser_rejects_order_by_without_path() {
    let error = parse_select("select Post { title } order by desc").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken {
            expected: "expression"
        }
    );
}

#[test]
fn parser_rejects_order_without_by() {
    let error = parse_select("select Post { title } order .title").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedToken { expected: "by" }
    );
}

#[test]
fn parser_can_parse_limit() {
    let query = parse_select("select Post { title } limit 10").expect("query should parse");

    assert_eq!(query.limit(), Some(10))
}

#[test]
fn parser_can_parse_offset() {
    let query = parse_select("select Post { title } offset 10").expect("query should parse");

    assert_eq!(query.offset(), Some(10))
}

#[test]
fn parser_can_parse_limit_and_offset() {
    let query =
        parse_select("select Post { title } limit 20 offset 10").expect("query should parse");

    assert_eq!(query.limit(), Some(20));
    assert_eq!(query.offset(), Some(10));
}

#[test]
fn parser_rejects_negative_limit() {
    let error = parse_select("select Post { title } limit -10").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedValue {
            expected: "non-negative integer"
        }
    );
}

#[test]
fn parser_rejects_negative_offset() {
    let error = parse_select("select Post { title } offset -10").expect_err("query should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::UnexpectedValue {
            expected: "non-negative integer"
        }
    );
}

#[test]
fn parser_rejects_integer_literal_outside_i64_range() {
    let error = parse_select("select Post { title } filter .view_count = 9223372036854775808")
        .expect_err("query should fail");

    assert_eq!(error.kind(), &ParseErrorKind::InvalidIntegerLiteral);
}
