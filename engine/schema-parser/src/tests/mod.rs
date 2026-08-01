use crate::{Keyword, LexErrorKind, ParseErrorKind, TokenKind, lex, parse_schema};
use alloc::string::ToString;
use schema_model::{Cardinality, Field, ScalarType, SchemaError, Uniqueness};

#[test]
fn lexer_tokenizes_empty_type_declaration() {
    let tokens = lex("type User {}").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Type));
    assert_eq!(tokens[1].kind(), &TokenKind::Ident("User".to_string()));
    assert_eq!(tokens[2].kind(), &TokenKind::LBrace);
    assert_eq!(tokens[3].kind(), &TokenKind::RBrace);
}

#[test]
fn lexer_tracks_line_and_column_for_field_tokens() {
    let tokens = lex("type User {\n  required name: str\n}").expect("schema should lex");
    let required_span = tokens[3].span();
    let name_span = tokens[4].span();

    assert_eq!(required_span.start().line(), 2);
    assert_eq!(required_span.start().column(), 3);
    assert_eq!(required_span.end().line(), 2);
    assert_eq!(required_span.end().column(), 11);

    assert_eq!(name_span.start().line(), 2);
    assert_eq!(name_span.start().column(), 12);
    assert_eq!(name_span.end().line(), 2);
    assert_eq!(name_span.end().column(), 16);
}

#[test]
fn lexer_tokenizes_scalar_field_modifiers_and_types() {
    let tokens = lex("required unique property email: str").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Required));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Unique));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Property));
    assert_eq!(tokens[3].kind(), &TokenKind::Ident("email".to_string()));
    assert_eq!(tokens[4].kind(), &TokenKind::Colon);
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::Str));
}

#[test]
fn lexer_tokenizes_link_modifiers() {
    let tokens = lex("multi link posts: Post").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Multi));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Link));
    assert_eq!(tokens[2].kind(), &TokenKind::Ident("posts".to_string()));
    assert_eq!(tokens[3].kind(), &TokenKind::Colon);
    assert_eq!(tokens[4].kind(), &TokenKind::Ident("Post".to_string()));
}

#[test]
fn lexer_tokenizes_all_scalar_type_keywords() {
    let tokens = lex("str int64 float64 bool uuid datetime").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Str));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Int64));
    assert_eq!(tokens[2].kind(), &TokenKind::Keyword(Keyword::Float64));
    assert_eq!(tokens[3].kind(), &TokenKind::Keyword(Keyword::Bool));
    assert_eq!(tokens[4].kind(), &TokenKind::Keyword(Keyword::Uuid));
    assert_eq!(tokens[5].kind(), &TokenKind::Keyword(Keyword::DateTime));
}

#[test]
fn lexer_reserves_update_and_set_keywords() {
    let tokens = lex("update set").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Keyword(Keyword::Update));
    assert_eq!(tokens[1].kind(), &TokenKind::Keyword(Keyword::Set));
}

#[test]
fn lexer_reserves_delete_keyword() {
    let tokens = lex("delete").expect("schema should lex");

    let TokenKind::Keyword(keyword) = tokens[0].kind() else {
        panic!("delete should be a keyword");
    };
    assert_eq!(keyword.as_str(), "delete");
}

#[test]
fn lexer_distinguishes_keyword_prefix_identifiers() {
    let tokens = lex("typeName requiredField linkTarget deleteArchive").expect("schema should lex");

    assert_eq!(tokens[0].kind(), &TokenKind::Ident("typeName".to_string()));
    assert_eq!(
        tokens[1].kind(),
        &TokenKind::Ident("requiredField".to_string())
    );
    assert_eq!(
        tokens[2].kind(),
        &TokenKind::Ident("linkTarget".to_string())
    );
    assert_eq!(
        tokens[3].kind(),
        &TokenKind::Ident("deleteArchive".to_string())
    );
}

#[test]
fn lexer_reports_unexpected_character_position() {
    let error = lex("type User {\n  @\n}").expect_err("schema should fail");

    assert_eq!(error.kind(), &LexErrorKind::UnexpectedChar('@'));
    assert_eq!(error.position().line(), 2);
    assert_eq!(error.position().column(), 3);
    assert_eq!(error.position().byte(), 14);
}

#[test]
fn parser_can_parse_empty_object_type() {
    let catalog = parse_schema("type User {}").expect("schema should parse");

    let user = catalog
        .find_type("User")
        .expect("catalog should contain User");

    assert_eq!(catalog.object_types().len(), 1);
    assert_eq!(user.name(), "User");
    assert!(user.declared_fields().is_empty());
}

#[test]
fn parser_rejects_mutation_keywords_as_identifiers() {
    for schema in [
        "type update {}",
        "type delete {}",
        "type Thing { set: str }",
    ] {
        let error = parse_schema(schema).expect_err("reserved identifier should fail");

        assert_eq!(
            error.kind(),
            &ParseErrorKind::UnexpectedToken { expected: "IDENT" }
        );
    }
}

#[test]
fn parser_can_parse_required_scalar_field() {
    let catalog = parse_schema(
        "type User {
  required name: str
}",
    )
    .expect("schema should parse");

    let user = catalog
        .find_type("User")
        .expect("catalog should contain User");
    let field = user
        .find_declared_field("name")
        .expect("User should contain name");

    match field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.scalar_type(), ScalarType::Str);
            assert_eq!(field.cardinality(), Cardinality::Required);
            assert_eq!(scalar.uniqueness(), Uniqueness::NotUnique);
        }
        Field::Link(_) => panic!("name should be a scalar field"),
    }
}

#[test]
fn parser_can_parse_property_keyword_scalar_field() {
    let catalog = parse_schema(
        "type User {
  property name: str
}",
    )
    .expect("schema should parse");

    let user = catalog
        .find_type("User")
        .expect("catalog should contain User");
    let field = user
        .find_declared_field("name")
        .expect("User should contain name");

    match field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.scalar_type(), ScalarType::Str);
            assert_eq!(field.cardinality(), Cardinality::Optional);
        }
        Field::Link(_) => panic!("name should be a scalar field"),
    }
}

#[test]
fn parser_can_parse_required_unique_scalar_field() {
    let catalog = parse_schema(
        "type User {
  required unique email: str
}",
    )
    .expect("schema should parse");

    let user = catalog
        .find_type("User")
        .expect("catalog should contain User");
    let field = user
        .find_declared_field("email")
        .expect("User should contain email");

    match field {
        Field::Scalar(scalar) => {
            assert_eq!(scalar.scalar_type(), ScalarType::Str);
            assert_eq!(field.cardinality(), Cardinality::Required);
            assert_eq!(scalar.uniqueness(), Uniqueness::Unique);
        }
        Field::Link(_) => panic!("email should be a scalar field"),
    }
}

#[test]
fn parser_can_parse_required_link_field() {
    let catalog = parse_schema(
        "type User {}

type Post {
  required link author: User
}",
    )
    .expect("schema should parse");

    let post = catalog
        .find_type("Post")
        .expect("catalog should contain Post");
    let field = post
        .find_declared_field("author")
        .expect("Post should contain author");

    match field {
        Field::Link(link) => {
            assert_eq!(link.target_type_name(), "User");
            assert_eq!(link.cardinality(), Cardinality::Required);
        }
        Field::Scalar(_) => panic!("author should be a link field"),
    }
}

#[test]
fn parser_can_parse_multi_link_field() {
    let catalog = parse_schema(
        "type User {}

type Post {
  multi link likers: User
}",
    )
    .expect("schema should parse");

    let post = catalog
        .find_type("Post")
        .expect("catalog should contain Post");
    let field = post
        .find_declared_field("likers")
        .expect("Post should contain likers");

    match field {
        Field::Link(link) => {
            assert_eq!(link.target_type_name(), "User");
            assert_eq!(link.cardinality(), Cardinality::Many);
        }
        Field::Scalar(_) => panic!("likers should be a link field"),
    }
}

#[test]
fn parser_rejects_multi_scalar_field() {
    let error = parse_schema(
        "type User {
  multi name: str
}",
    )
    .expect_err("schema should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::IncompatibleModifiers {
            message: "`multi` is only valid on link fields"
        }
    );
}

#[test]
fn parser_rejects_unique_link_field() {
    let error = parse_schema(
        "type User {}

type Post {
  unique link author: User
}",
    )
    .expect_err("schema should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::IncompatibleModifiers {
            message: "`unique` is only valid on scalar fields"
        }
    );
}

#[test]
fn parser_rejects_duplicate_type_names_through_catalog_validation() {
    let error = parse_schema("type User {} type User {}").expect_err("schema should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::InvalidCatalog(SchemaError::DuplicateTypeName {
            name: "User".to_string(),
        })
    );
}

#[test]
fn parser_rejects_duplicate_modifiers() {
    let error = parse_schema(
        "type User {
  required required name: str
}",
    )
    .expect_err("schema should fail");

    assert_eq!(
        error.kind(),
        &ParseErrorKind::DuplicateModifier {
            modifier: "required"
        }
    );
}

#[test]
fn parser_can_parse_all_scalar_types() {
    let catalog = parse_schema(
        "type User {
  text: str
  count: int64
  score: float64
  active: bool
  token: uuid
  created_at: datetime
}",
    )
    .expect("schema should parse");

    let user = catalog
        .find_type("User")
        .expect("catalog should contain User");
    let expected = [
        ("text", ScalarType::Str),
        ("count", ScalarType::Int64),
        ("score", ScalarType::Float64),
        ("active", ScalarType::Bool),
        ("token", ScalarType::Uuid),
        ("created_at", ScalarType::DateTime),
    ];

    for (field_name, scalar_type) in expected {
        let field = user
            .find_declared_field(field_name)
            .expect("field should exist");
        match field {
            Field::Scalar(scalar) => assert_eq!(scalar.scalar_type(), scalar_type),
            Field::Link(_) => panic!("field should be scalar"),
        }
    }
}
