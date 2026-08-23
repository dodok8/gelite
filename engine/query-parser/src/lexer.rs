use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use logos::Logos;

/// Tokenizes query source and attaches byte, line, and column spans.
pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let line_map = LineMap::new(input);
    let mut lexer = RawTokenKind::lexer(input);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let raw = result.map_err(|_| {
            let range = lexer.span();
            LexError::new(
                LexErrorKind::UnexpectedChar(input[range.clone()].chars().next().unwrap()),
                line_map.position(input, range.start),
            )
        })?;

        let range = lexer.span();
        let span = Span::new(
            line_map.position(input, range.start),
            line_map.position(input, range.end),
        );

        let kind = match raw {
            RawTokenKind::Select => TokenKind::Keyword(Keyword::Select),
            RawTokenKind::Insert => TokenKind::Keyword(Keyword::Insert),
            RawTokenKind::Update => TokenKind::Keyword(Keyword::Update),
            RawTokenKind::Delete => TokenKind::Keyword(Keyword::Delete),
            RawTokenKind::Start => TokenKind::Keyword(Keyword::Start),
            RawTokenKind::Transaction => TokenKind::Keyword(Keyword::Transaction),
            RawTokenKind::Commit => TokenKind::Keyword(Keyword::Commit),
            RawTokenKind::Rollback => TokenKind::Keyword(Keyword::Rollback),
            RawTokenKind::Filter => TokenKind::Keyword(Keyword::Filter),
            RawTokenKind::Set => TokenKind::Keyword(Keyword::Set),
            RawTokenKind::Order => TokenKind::Keyword(Keyword::Order),
            RawTokenKind::By => TokenKind::Keyword(Keyword::By),
            RawTokenKind::Limit => TokenKind::Keyword(Keyword::Limit),
            RawTokenKind::Offset => TokenKind::Keyword(Keyword::Offset),
            RawTokenKind::Asc => TokenKind::Keyword(Keyword::Asc),
            RawTokenKind::Desc => TokenKind::Keyword(Keyword::Desc),
            RawTokenKind::True => TokenKind::Keyword(Keyword::True),
            RawTokenKind::False => TokenKind::Keyword(Keyword::False),
            RawTokenKind::Null => TokenKind::Keyword(Keyword::Null),

            RawTokenKind::LBrace => TokenKind::LBrace,
            RawTokenKind::RBrace => TokenKind::RBrace,
            RawTokenKind::LBracket => TokenKind::LBracket,
            RawTokenKind::RBracket => TokenKind::RBracket,
            RawTokenKind::LParen => TokenKind::LParen,
            RawTokenKind::RParen => TokenKind::RParen,
            RawTokenKind::Comma => TokenKind::Comma,
            RawTokenKind::Semicolon => TokenKind::Semicolon,
            RawTokenKind::ColonEq => TokenKind::ColonEq,
            RawTokenKind::PlusEq => TokenKind::PlusEq,
            RawTokenKind::MinusEq => TokenKind::MinusEq,
            RawTokenKind::Colon => TokenKind::Colon,
            RawTokenKind::Dot => TokenKind::Dot,
            RawTokenKind::Eq => TokenKind::Eq,
            RawTokenKind::Ne => TokenKind::Ne,
            RawTokenKind::Le => TokenKind::Le,
            RawTokenKind::Lt => TokenKind::Lt,
            RawTokenKind::Ge => TokenKind::Ge,
            RawTokenKind::Gt => TokenKind::Gt,
            RawTokenKind::Plus => TokenKind::Plus,
            RawTokenKind::Minus => TokenKind::Minus,
            RawTokenKind::Star => TokenKind::Star,
            RawTokenKind::Slash => TokenKind::Slash,
            RawTokenKind::Percent => TokenKind::Percent,

            RawTokenKind::Ident => TokenKind::Ident(lexer.slice().to_string()),
            RawTokenKind::Float => TokenKind::Float(lexer.slice().to_string()),
            RawTokenKind::Int => TokenKind::Int(lexer.slice().to_string()),
            RawTokenKind::String => {
                let raw = lexer.slice();
                let value = &raw[1..raw.len() - 1];
                TokenKind::String(value.to_string())
            }
            RawTokenKind::UnterminatedString => {
                return Err(LexError::new(
                    LexErrorKind::UnterminatedString,
                    span.start(),
                ));
            }
            RawTokenKind::InvalidFloatWithoutFractionalPart => {
                let invalid_byte = range.end - 1;
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar('.'),
                    line_map.position(input, invalid_byte),
                ));
            }
            RawTokenKind::InvalidFloatWithoutIntegerPart => {
                let invalid_byte = range.start + 1;
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar(input[invalid_byte..].chars().next().unwrap()),
                    line_map.position(input, invalid_byte),
                ));
            }
        };

        tokens.push(Token::new(kind, span));
    }

    Ok(tokens)
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
enum RawTokenKind {
    #[regex(r"\.[0-9]+")]
    InvalidFloatWithoutIntegerPart,

    #[regex(r"[0-9]+\.")]
    InvalidFloatWithoutFractionalPart,

    #[token("select")]
    Select,

    #[token("insert")]
    Insert,

    #[token("update")]
    Update,

    #[token("delete")]
    Delete,

    #[token("start")]
    Start,

    #[token("transaction")]
    Transaction,

    #[token("commit")]
    Commit,

    #[token("rollback")]
    Rollback,

    #[token("filter")]
    Filter,

    #[token("set")]
    Set,

    #[token("order")]
    Order,

    #[token("by")]
    By,

    #[token("limit")]
    Limit,

    #[token("offset")]
    Offset,

    #[token("asc")]
    Asc,

    #[token("desc")]
    Desc,

    #[token("true")]
    True,

    #[token("false")]
    False,

    #[token("null")]
    Null,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token(",")]
    Comma,

    #[token(";")]
    Semicolon,

    #[token(":=")]
    ColonEq,

    #[token("+=")]
    PlusEq,

    #[token("-=")]
    MinusEq,

    #[token(":")]
    Colon,

    #[token(".")]
    Dot,

    #[token("=")]
    Eq,

    #[token("!=")]
    Ne,

    #[token("<=")]
    Le,

    #[token("<")]
    Lt,

    #[token(">=")]
    Ge,

    #[token(">")]
    Gt,

    #[regex(r#""[^"]*""#)]
    String,

    #[regex(r#""[^"]*"#)]
    UnterminatedString,

    #[regex(r"[0-9]+\.[0-9]+")]
    Float,

    #[regex(r"[0-9]+")]
    Int,

    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,
}

/// Lexed token with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn kind(&self) -> &TokenKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

/// Token categories recognized by the MVP query lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(Keyword),
    Ident(String),
    String(String),
    Float(String),
    Int(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Semicolon,
    ColonEq,
    PlusEq,
    MinusEq,
    Colon,
    Dot,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
}

/// Reserved keywords recognized by the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Select,
    Insert,
    Update,
    Delete,
    Start,
    Transaction,
    Commit,
    Rollback,
    Filter,
    Set,
    Order,
    By,
    Limit,
    Offset,
    Asc,
    Desc,
    True,
    False,
    Null,
}

impl Keyword {
    pub fn as_str(&self) -> &'static str {
        match self {
            Keyword::Select => "select",
            Keyword::Insert => "insert",
            Keyword::Update => "update",
            Keyword::Delete => "delete",
            Keyword::Start => "start",
            Keyword::Transaction => "transaction",
            Keyword::Commit => "commit",
            Keyword::Rollback => "rollback",
            Keyword::Filter => "filter",
            Keyword::Set => "set",
            Keyword::Order => "order",
            Keyword::By => "by",
            Keyword::Limit => "limit",
            Keyword::Offset => "offset",
            Keyword::Asc => "asc",
            Keyword::Desc => "desc",
            Keyword::True => "true",
            Keyword::False => "false",
            Keyword::Null => "null",
        }
    }
}

/// Half-open source span from start position to end position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    start: Position,
    end: Position,
}

impl Span {
    pub(crate) fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn start(&self) -> Position {
        self.start
    }

    pub fn end(&self) -> Position {
        self.end
    }
}

/// Source position tracked as byte offset plus one-based line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    byte: usize,
    line: usize,
    column: usize,
}

impl Position {
    fn new(byte: usize, line: usize, column: usize) -> Self {
        Self { byte, line, column }
    }

    pub fn byte(&self) -> usize {
        self.byte
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

struct LineMap {
    line_starts: Vec<usize>,
}

impl LineMap {
    fn new(input: &str) -> Self {
        let mut line_starts = vec![0];

        for (byte, ch) in input.char_indices() {
            if ch == '\n' {
                line_starts.push(byte + ch.len_utf8());
            }
        }

        Self { line_starts }
    }

    fn position(&self, input: &str, byte: usize) -> Position {
        let line_index = self
            .line_starts
            .partition_point(|line_start| *line_start <= byte)
            .saturating_sub(1);

        let line_start = self.line_starts[line_index];
        let column = input[line_start..byte].chars().count() + 1;
        Position::new(byte, line_index + 1, column)
    }
}

/// Lexer error with source position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    kind: LexErrorKind,
    position: Position,
}

impl LexError {
    fn new(kind: LexErrorKind, position: Position) -> Self {
        Self { kind, position }
    }

    pub fn kind(&self) -> &LexErrorKind {
        &self.kind
    }

    pub fn position(&self) -> Position {
        self.position
    }
}

/// Lexer error category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
}
