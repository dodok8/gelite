use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use logos::Logos;

/// Tokenizes `.geli` schema source and attaches byte, line, and column spans.
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
            RawTokenKind::Type => TokenKind::Keyword(Keyword::Type),
            RawTokenKind::Property => TokenKind::Keyword(Keyword::Property),
            RawTokenKind::Link => TokenKind::Keyword(Keyword::Link),
            RawTokenKind::Required => TokenKind::Keyword(Keyword::Required),
            RawTokenKind::Multi => TokenKind::Keyword(Keyword::Multi),
            RawTokenKind::Unique => TokenKind::Keyword(Keyword::Unique),
            RawTokenKind::Str => TokenKind::Keyword(Keyword::Str),
            RawTokenKind::Int64 => TokenKind::Keyword(Keyword::Int64),
            RawTokenKind::Float64 => TokenKind::Keyword(Keyword::Float64),
            RawTokenKind::Bool => TokenKind::Keyword(Keyword::Bool),
            RawTokenKind::Uuid => TokenKind::Keyword(Keyword::Uuid),
            RawTokenKind::DateTime => TokenKind::Keyword(Keyword::DateTime),
            RawTokenKind::Update => TokenKind::Keyword(Keyword::Update),
            RawTokenKind::Set => TokenKind::Keyword(Keyword::Set),
            RawTokenKind::Delete => TokenKind::Keyword(Keyword::Delete),

            RawTokenKind::LBrace => TokenKind::LBrace,
            RawTokenKind::RBrace => TokenKind::RBrace,
            RawTokenKind::Colon => TokenKind::Colon,

            RawTokenKind::Ident => TokenKind::Ident(lexer.slice().to_string()),
        };

        tokens.push(Token::new(kind, span));
    }

    Ok(tokens)
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip(r"#[^\r\n]*", allow_greedy = true))]
enum RawTokenKind {
    #[token("type")]
    Type,

    #[token("property")]
    Property,

    #[token("link")]
    Link,

    #[token("required")]
    Required,

    #[token("multi")]
    Multi,

    #[token("unique")]
    Unique,

    #[token("str")]
    Str,

    #[token("int64")]
    Int64,

    #[token("float64")]
    Float64,

    #[token("bool")]
    Bool,

    #[token("uuid")]
    Uuid,

    #[token("datetime")]
    DateTime,

    #[token("update")]
    Update,

    #[token("set")]
    Set,

    #[token("delete")]
    Delete,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token(":")]
    Colon,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,
}

/// Lexed schema token with its source span.
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

/// Token categories recognized by the MVP schema lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(Keyword),
    Ident(String),
    LBrace,
    RBrace,
    Colon,
}

/// Reserved words recognized by the schema parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Type,
    Property,
    Link,
    Required,
    Multi,
    Unique,
    Str,
    Int64,
    Float64,
    Bool,
    Uuid,
    DateTime,
    Update,
    Set,
    Delete,
}

impl Keyword {
    pub fn as_str(&self) -> &'static str {
        match self {
            Keyword::Type => "type",
            Keyword::Property => "property",
            Keyword::Link => "link",
            Keyword::Required => "required",
            Keyword::Multi => "multi",
            Keyword::Unique => "unique",
            Keyword::Str => "str",
            Keyword::Int64 => "int64",
            Keyword::Float64 => "float64",
            Keyword::Bool => "bool",
            Keyword::Uuid => "uuid",
            Keyword::DateTime => "datetime",
            Keyword::Update => "update",
            Keyword::Set => "set",
            Keyword::Delete => "delete",
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
    fn new(start: Position, end: Position) -> Self {
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
}
