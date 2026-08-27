use crate::{Keyword, LexError, Span, Token, TokenKind, lex};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog, SchemaError,
    SingleCardinality, Uniqueness,
};

/// Parses one `.geli` schema source into the semantic schema catalog.
///
/// The parser checks syntax and local modifier compatibility. Catalog-level
/// validation, such as duplicate type names and unknown link targets, remains
/// owned by `schema_model::SchemaCatalog`.
pub fn parse_schema(input: &str) -> Result<schema_model::SchemaCatalog, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_schema_tokens(&tokens)
}

fn parse_schema_tokens(tokens: &[Token]) -> Result<schema_model::SchemaCatalog, ParseError> {
    Parser::new(tokens).parse_schema()
}

/// Parser error with an optional source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: Box<ParseErrorKind>,
    span: Option<Span>,
}

impl ParseError {
    fn new(kind: ParseErrorKind, span: Option<Span>) -> Self {
        Self {
            kind: Box::new(kind),
            span,
        }
    }

    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    pub fn span(&self) -> Option<Span> {
        self.span
    }
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self::new(ParseErrorKind::Lex(error), None)
    }
}

/// Parser error category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedEof { expected: &'static str },
    UnexpectedToken { expected: &'static str },
    DuplicateModifier { modifier: &'static str },
    IncompatibleModifiers { message: &'static str },
    InvalidCatalog(SchemaError),
}

#[derive(Debug, Default)]
struct ModifierSet {
    required: bool,
    multi: bool,
    unique: bool,
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn parse_schema(&mut self) -> Result<schema_model::SchemaCatalog, ParseError> {
        let mut object_types = Vec::new();

        while self.peek().is_some() {
            object_types.push(self.parse_type_decl()?);
        }

        SchemaCatalog::try_new(object_types)
            .map_err(|error| ParseError::new(ParseErrorKind::InvalidCatalog(error), None))
    }

    fn parse_type_decl(&mut self) -> Result<ObjectType, ParseError> {
        self.expect_keyword(Keyword::Type)?;
        let name = self.expect_ident()?;
        self.expect_lbrace()?;

        let mut fields = Vec::new();
        while !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::RBrace)
        {
            if self.peek().is_none() {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof { expected: "}" },
                    None,
                ));
            }
            fields.push(self.parse_field_decl()?);
        }

        self.expect_rbrace()?;
        Ok(ObjectType::new(name, fields))
    }

    fn parse_field_decl(&mut self) -> Result<Field, ParseError> {
        let modifiers = self.parse_modifier_set()?;

        if modifiers.required && modifiers.multi {
            return Err(ParseError::new(
                ParseErrorKind::IncompatibleModifiers {
                    message: "`required multi` is not a valid cardinality",
                },
                self.peek().map(|token| token.span()),
            ));
        }

        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Link) => {
                self.parse_link_field(modifiers)
            }
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Property) => {
                self.advance();
                self.parse_scalar_field(modifiers)
            }
            Some(_) => self.parse_scalar_field(modifiers),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "field declaration",
                },
                None,
            )),
        }
    }

    fn parse_modifier_set(&mut self) -> Result<ModifierSet, ParseError> {
        let mut modifiers = ModifierSet::default();

        loop {
            match self.peek() {
                Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Required) => {
                    reject_duplicate_modifier(modifiers.required, "required", token.span())?;
                    modifiers.required = true;
                    self.advance();
                }
                Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Multi) => {
                    reject_duplicate_modifier(modifiers.multi, "multi", token.span())?;
                    modifiers.multi = true;
                    self.advance();
                }
                Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Unique) => {
                    reject_duplicate_modifier(modifiers.unique, "unique", token.span())?;
                    modifiers.unique = true;
                    self.advance();
                }
                _ => break,
            }
        }

        Ok(modifiers)
    }

    fn parse_scalar_field(&mut self, modifiers: ModifierSet) -> Result<Field, ParseError> {
        if modifiers.multi {
            return Err(ParseError::new(
                ParseErrorKind::IncompatibleModifiers {
                    message: "`multi` is only valid on link fields",
                },
                self.peek().map(|token| token.span()),
            ));
        }

        let name = self.expect_ident()?;
        self.expect_colon()?;
        let scalar_type = self.expect_scalar_type()?;

        let cardinality = if modifiers.required {
            SingleCardinality::Required
        } else {
            SingleCardinality::Optional
        };
        let uniqueness = if modifiers.unique {
            Uniqueness::Unique
        } else {
            Uniqueness::NotUnique
        };

        Ok(Field::Scalar(ScalarField::with_uniqueness(
            name,
            scalar_type,
            cardinality,
            uniqueness,
        )))
    }

    fn parse_link_field(&mut self, modifiers: ModifierSet) -> Result<Field, ParseError> {
        if modifiers.unique {
            return Err(ParseError::new(
                ParseErrorKind::IncompatibleModifiers {
                    message: "`unique` is only valid on scalar fields",
                },
                self.peek().map(|token| token.span()),
            ));
        }

        self.expect_keyword(Keyword::Link)?;
        let name = self.expect_ident()?;
        self.expect_colon()?;
        let target_type_name = self.expect_ident()?;

        let cardinality = if modifiers.multi {
            Cardinality::Many
        } else if modifiers.required {
            Cardinality::Required
        } else {
            Cardinality::Optional
        };

        // A following `inverse:` starts a field declaration, not an inverse clause.
        if matches!(self.peek().map(Token::kind), Some(TokenKind::Ident(word)) if word == "inverse")
            && !matches!(
                self.tokens.get(self.cursor + 1).map(Token::kind),
                Some(TokenKind::Colon)
            )
        {
            self.advance();
            let source_field = self.expect_ident()?;
            return Ok(Field::Link(LinkField::with_inverse(
                name,
                target_type_name,
                cardinality,
                source_field,
            )));
        }

        Ok(Field::Link(LinkField::new(
            name,
            target_type_name,
            cardinality,
        )))
    }

    fn expect_keyword(&mut self, expected: Keyword) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(expected) => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: expected.as_str(),
                },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: expected.as_str(),
                },
                None,
            )),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(token) => match token.kind() {
                TokenKind::Ident(value) => {
                    let value = value.clone();
                    self.advance();
                    Ok(value)
                }
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken { expected: "IDENT" },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "IDENT" },
                None,
            )),
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::LBrace => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "{" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "{" },
                None,
            )),
        }
    }

    fn expect_rbrace(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::RBrace => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "}" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "}" },
                None,
            )),
        }
    }

    fn expect_colon(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Colon => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: ":" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: ":" },
                None,
            )),
        }
    }

    fn expect_scalar_type(&mut self) -> Result<ScalarType, ParseError> {
        match self.peek() {
            Some(token) => {
                let scalar_type = match token.kind() {
                    TokenKind::Keyword(Keyword::Str) => ScalarType::Str,
                    TokenKind::Keyword(Keyword::Int64) => ScalarType::Int64,
                    TokenKind::Keyword(Keyword::Float64) => ScalarType::Float64,
                    TokenKind::Keyword(Keyword::Bool) => ScalarType::Bool,
                    TokenKind::Keyword(Keyword::Uuid) => ScalarType::Uuid,
                    TokenKind::Keyword(Keyword::DateTime) => ScalarType::DateTime,
                    _ => {
                        return Err(ParseError::new(
                            ParseErrorKind::UnexpectedToken {
                                expected: "scalar type",
                            },
                            Some(token.span()),
                        ));
                    }
                };
                self.advance();
                Ok(scalar_type)
            }
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "scalar type",
                },
                None,
            )),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.cursor);
        self.cursor += usize::from(token.is_some());
        token
    }
}

fn reject_duplicate_modifier(
    already_seen: bool,
    modifier: &'static str,
    span: Span,
) -> Result<(), ParseError> {
    if already_seen {
        return Err(ParseError::new(
            ParseErrorKind::DuplicateModifier { modifier },
            Some(span),
        ));
    }

    Ok(())
}
