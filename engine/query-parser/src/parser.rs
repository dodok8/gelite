use crate::{Keyword, LexError, Span, Token, TokenKind, lex};
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{string::String, vec};
use query_ast::{
    ArithmeticExpr, ArithmeticOp, Assignment, CompareExpr, CompareOp, DeleteQuery, Expr, InExpr,
    InOp, InsertQuery, Literal, OrderExpr, Path, PathStep, SelectQuery, Shape, ShapeItem,
    TransactionCommand, UpdateQuery,
};

/// Parses one MVP `select` statement from source text.
///
/// The parser checks syntax only. Schema names, field names, link traversal,
/// and type compatibility are validated by the resolver.
pub fn parse_select(input: &str) -> Result<query_ast::SelectQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_select_tokens(&tokens)
}

fn parse_select_tokens(tokens: &[Token]) -> Result<query_ast::SelectQuery, ParseError> {
    Parser::new(tokens).parse_select_stmt()
}

/// Parses one transaction-control command from source text.
pub fn parse_transaction_command(input: &str) -> Result<TransactionCommand, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    Parser::new(&tokens).parse_transaction_command()
}

/// Parser error with an optional source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
    span: Option<Span>,
}

impl ParseError {
    fn new(kind: ParseErrorKind, span: Option<Span>) -> Self {
        Self { kind, span }
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
    UnexpectedValue { expected: &'static str },
    InvalidIntegerLiteral,
    InvalidFloatLiteral,
    Unsupported,
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}

const OR_BP: (u8, u8) = (1, 2);
const AND_BP: (u8, u8) = (3, 4);
const COMPARISON_BP: (u8, u8) = (5, 6);
const ADDITIVE_BP: (u8, u8) = (7, 8);
const MULTIPLICATIVE_BP: (u8, u8) = (9, 10);
const NOT_RIGHT_BP: u8 = COMPARISON_BP.0;
const UNARY_RIGHT_BP: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InfixOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl InfixOp {
    fn binding_power(self) -> (u8, u8) {
        match self {
            Self::Or => OR_BP,
            Self::And => AND_BP,
            Self::Eq
            | Self::Ne
            | Self::Lt
            | Self::Le
            | Self::Gt
            | Self::Ge
            | Self::In
            | Self::NotIn => COMPARISON_BP,
            Self::Add | Self::Sub => ADDITIVE_BP,
            Self::Mul | Self::Div | Self::Mod => MULTIPLICATIVE_BP,
        }
    }
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, cursor: 0 }
    }

    /// Parses:
    ///
    /// ```text
    /// select_stmt := "select" type_ref shape filter_clause? order_clause?
    ///                limit_clause? offset_clause?
    /// ```
    fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError> {
        self.expect_keyword(Keyword::Select)?;
        let root_type_name = self.expect_ident()?;
        let shape = self.parse_shape()?;
        let filter = self.parse_filter_clause()?;
        let order_by = self.parse_order_clause()?;
        let limit = self.parse_limit_clause()?;
        let offset = self.parse_offset_clause()?;
        self.ensure_eof()?;

        Ok(SelectQuery::new(
            root_type_name,
            shape,
            filter,
            order_by,
            limit,
            offset,
        ))
    }

    fn parse_transaction_command(&mut self) -> Result<TransactionCommand, ParseError> {
        let command = match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Start) => {
                self.advance();
                self.expect_keyword(Keyword::Transaction)?;
                TransactionCommand::Start
            }
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Commit) => {
                self.advance();
                TransactionCommand::Commit
            }
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Rollback) => {
                self.advance();
                TransactionCommand::Rollback
            }
            Some(token) => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        expected: "transaction command",
                    },
                    Some(token.span()),
                ));
            }
            None => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedEof {
                        expected: "transaction command",
                    },
                    None,
                ));
            }
        };

        self.ensure_eof()?;
        Ok(command)
    }

    fn parse_shape(&mut self) -> Result<query_ast::Shape, ParseError> {
        let mut shape_items = vec![];
        self.expect_lbrace()?;

        loop {
            match self.peek() {
                Some(token) if token.kind() == &TokenKind::RBrace => break,
                Some(_) => {
                    shape_items.push(self.parse_shape_item()?);
                    if !self.consume_comma_if_present() {
                        match self.peek() {
                            Some(token) if token.kind() == &TokenKind::RBrace => continue,
                            Some(token) => {
                                return Err(ParseError::new(
                                    ParseErrorKind::UnexpectedToken { expected: ", or }" },
                                    Some(token.span()),
                                ));
                            }
                            None => {
                                return Err(ParseError::new(
                                    ParseErrorKind::UnexpectedEof { expected: "}" },
                                    None,
                                ));
                            }
                        }
                    }
                }
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEof { expected: "}" },
                        None,
                    ));
                }
            }
        }

        self.expect_rbrace()?;
        Ok(Shape::new(shape_items))
    }

    fn parse_shape_item(&mut self) -> Result<query_ast::ShapeItem, ParseError> {
        let field_name = self.expect_ident()?;

        match self.peek() {
            Some(token) if token.kind() == &TokenKind::ColonEq => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(ShapeItem::computed(field_name, expr))
            }
            Some(token) if token.kind() == &TokenKind::Colon => {
                self.advance();
                let child_shape = self.parse_shape()?;
                let path = Path::new(vec![PathStep::new(field_name)]);
                Ok(ShapeItem::new(path, Some(child_shape)))
            }
            _ => {
                let path = Path::new(vec![PathStep::new(field_name)]);
                Ok(ShapeItem::new(path, None))
            }
        }
    }

    fn parse_filter_clause(&mut self) -> Result<Option<query_ast::Expr>, ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Filter) => {
                self.expect_keyword(Keyword::Filter)?;
                Ok(Some(self.parse_expr()?))
            }
            _ => Ok(None),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_expr_bp(0)?;
        self.reject_adjacent_primary_expr()?;

        Ok(expr)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix_or_primary()?;

        while let Some(op) = self.peek_infix_op()? {
            let (left_bp, right_bp) = op.binding_power();

            if left_bp < min_bp {
                break;
            }

            self.consume_op(op)?;
            left = match op {
                InfixOp::In | InfixOp::NotIn => {
                    let right = self.parse_in_rhs()?;
                    let in_op = match op {
                        InfixOp::In => InOp::In,
                        InfixOp::NotIn => InOp::NotIn,
                        _ => unreachable!("in operator checked"),
                    };

                    Expr::In(InExpr::new(left, in_op, right))
                }
                _ => {
                    let right = self.parse_expr_bp(right_bp)?;
                    make_binary_expr(left, op, right)
                }
            };
        }

        Ok(left)
    }

    fn reject_adjacent_primary_expr(&self) -> Result<(), ParseError> {
        if self.peek().is_some_and(is_primary_expr_start) {
            let token = self.peek().expect("peek checked token exists");
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: "comparison operator",
                },
                Some(token.span()),
            ));
        }

        Ok(())
    }

    fn parse_prefix_or_primary(&mut self) -> Result<Expr, ParseError> {
        if self.consume_contextual_keyword_if_present("not") {
            return Ok(Expr::Not(Box::new(self.parse_expr_bp(NOT_RIGHT_BP)?)));
        }

        if let Some(op) = self.consume_unary_arithmetic_op_if_present() {
            if op == query_ast::UnaryArithmeticOp::Minus
                && self.consume_i64_min_literal_if_present()
            {
                return Ok(Expr::Literal(query_ast::Literal::Int64(i64::MIN)));
            }

            return Ok(Expr::UnaryArithmetic(query_ast::UnaryArithmeticExpr::new(
                op,
                self.parse_expr_bp(UNARY_RIGHT_BP)?,
            )));
        }

        self.parse_primary_expr()
    }

    fn peek_infix_op(&self) -> Result<Option<InfixOp>, ParseError> {
        let Some(token) = self.peek() else {
            return Ok(None);
        };

        let op = match token.kind() {
            TokenKind::Plus => Some(InfixOp::Add),
            TokenKind::Minus => Some(InfixOp::Sub),
            TokenKind::Star => Some(InfixOp::Mul),
            TokenKind::Slash => Some(InfixOp::Div),
            TokenKind::Percent => Some(InfixOp::Mod),

            TokenKind::Eq => Some(InfixOp::Eq),
            TokenKind::Ne => Some(InfixOp::Ne),
            TokenKind::Lt => Some(InfixOp::Lt),
            TokenKind::Le => Some(InfixOp::Le),
            TokenKind::Gt => Some(InfixOp::Gt),
            TokenKind::Ge => Some(InfixOp::Ge),

            TokenKind::Ident(value) if value == "or" => Some(InfixOp::Or),
            TokenKind::Ident(value) if value == "and" => Some(InfixOp::And),
            TokenKind::Ident(value) if value == "in" => Some(InfixOp::In),

            TokenKind::Ident(value) if value == "not" => match self.tokens.get(self.cursor + 1) {
                Some(next) if token_is_ident(next, "in") => Some(InfixOp::NotIn),
                _ => None,
            },

            _ => None,
        };

        Ok(op)
    }

    fn consume_op(&mut self, op: InfixOp) -> Result<(), ParseError> {
        match op {
            InfixOp::NotIn => {
                self.expect_contextual_keyword("not")?;
                self.expect_contextual_keyword("in")?;
            }
            InfixOp::In => {
                self.expect_contextual_keyword("in")?;
            }
            InfixOp::And => {
                self.expect_contextual_keyword("and")?;
            }
            InfixOp::Or => {
                self.expect_contextual_keyword("or")?;
            }
            InfixOp::Add => self.expect_token(TokenKind::Plus)?,
            InfixOp::Sub => self.expect_token(TokenKind::Minus)?,
            InfixOp::Mul => self.expect_token(TokenKind::Star)?,
            InfixOp::Div => self.expect_token(TokenKind::Slash)?,
            InfixOp::Mod => self.expect_token(TokenKind::Percent)?,
            InfixOp::Eq => self.expect_token(TokenKind::Eq)?,
            InfixOp::Ne => self.expect_token(TokenKind::Ne)?,
            InfixOp::Lt => self.expect_token(TokenKind::Lt)?,
            InfixOp::Le => self.expect_token(TokenKind::Le)?,
            InfixOp::Gt => self.expect_token(TokenKind::Gt)?,
            InfixOp::Ge => self.expect_token(TokenKind::Ge)?,
        }

        Ok(())
    }

    fn parse_in_rhs(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect_lbracket()?;

        let mut values = vec![];
        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::RBracket)
        {
            self.expect_rbracket()?;
            return Ok(values);
        }

        values.push(self.parse_expr()?);
        while self.consume_comma_if_present() {
            values.push(self.parse_expr()?);
        }

        self.expect_rbracket()?;
        Ok(values)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(expr)
            }
            Some(token) if token.kind() == &TokenKind::Dot => {
                Ok(Expr::Path(self.parse_path(true)?))
            }
            Some(token) => match token.kind() {
                kind if token_kind_is_ident(kind) && self.next_token_is_lparen() => {
                    self.parse_function_call_expr()
                }
                kind if token_kind_is_ident(kind) => Ok(Expr::Path(self.parse_path(false)?)),
                TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Keyword(Keyword::True)
                | TokenKind::Keyword(Keyword::False)
                | TokenKind::Keyword(Keyword::Null) => Ok(Expr::Literal(self.expect_literal()?)),
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        expected: "expression",
                    },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "expression",
                },
                None,
            )),
        }
    }

    fn parse_function_call_expr(&mut self) -> Result<Expr, ParseError> {
        let name = self.expect_ident()?;
        self.expect_token(TokenKind::LParen)?;

        let mut args = vec![];
        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::RParen)
        {
            self.expect_rparen()?;
            return Ok(Expr::FunctionCall(query_ast::FunctionCallExpr::new(
                name, args,
            )));
        }

        args.push(self.parse_expr()?);
        while self.consume_comma_if_present() {
            args.push(self.parse_expr()?);
        }

        self.expect_rparen()?;
        Ok(Expr::FunctionCall(query_ast::FunctionCallExpr::new(
            name, args,
        )))
    }

    fn parse_order_clause(&mut self) -> Result<Vec<OrderExpr>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Order))
        {
            return Ok(vec![]);
        }

        let mut results = vec![];

        self.expect_keyword(Keyword::Order)?;
        self.expect_keyword(Keyword::By)?;

        results.push(self.parse_order_item()?);
        while self.consume_comma_if_present() {
            results.push(self.parse_order_item()?);
        }

        Ok(results)
    }

    fn parse_order_item(&mut self) -> Result<OrderExpr, ParseError> {
        let expr = self.parse_expr()?;
        let direction = match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Desc) => {
                self.advance();
                query_ast::OrderDirection::Desc
            }
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Asc) => {
                self.advance();
                query_ast::OrderDirection::Asc
            }
            _ => query_ast::OrderDirection::Asc,
        };

        Ok(OrderExpr::new(expr, direction))
    }
    fn parse_path(&mut self, allow_leading_dot: bool) -> Result<Path, ParseError> {
        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Dot)
        {
            if !allow_leading_dot {
                let token = self.peek().expect("peek checked token exists");
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken { expected: "IDENT" },
                    Some(token.span()),
                ));
            }

            self.advance();
        }

        let mut steps = vec![];
        steps.push(PathStep::new(self.expect_ident()?));

        while self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Dot)
        {
            self.advance();
            steps.push(PathStep::new(self.expect_ident()?));
        }

        Ok(Path::new(steps))
    }

    fn parse_limit_clause(&mut self) -> Result<Option<i64>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Limit))
        {
            return Ok(None);
        }

        self.expect_keyword(Keyword::Limit)?;

        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Minus)
        {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
                },
                None,
            ));
        }

        match self.expect_literal()? {
            Literal::Int64(value) if value >= 0 => Ok(Some(value)),
            _ => Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
                },
                None,
            )),
        }
    }

    fn parse_offset_clause(&mut self) -> Result<Option<i64>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Offset))
        {
            return Ok(None);
        }

        self.expect_keyword(Keyword::Offset)?;
        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Minus)
        {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
                },
                None,
            ));
        }

        match self.expect_literal()? {
            Literal::Int64(value) if value >= 0 => Ok(Some(value)),
            _ => Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
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

    fn expect_contextual_keyword(&mut self, expected: &'static str) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token_is_ident(token, expected) => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected },
                None,
            )),
        }
    }

    fn expect_token(&mut self, expected: TokenKind) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &expected => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: token_kind_description(&expected),
                },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: token_kind_description(&expected),
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
                TokenKind::Keyword(
                    keyword @ (Keyword::Start
                    | Keyword::Transaction
                    | Keyword::Commit
                    | Keyword::Rollback),
                ) => {
                    let value = String::from(keyword.as_str());
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

    fn expect_lbracket(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::LBracket => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "[" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "[" },
                None,
            )),
        }
    }

    fn expect_rbracket(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::RBracket => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "]" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "]" },
                None,
            )),
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::RParen => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: ")" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: ")" },
                None,
            )),
        }
    }

    fn expect_literal(&mut self) -> Result<query_ast::Literal, ParseError> {
        match self.peek() {
            Some(token) => match token.kind() {
                TokenKind::Int(value) => {
                    let parsed = value.parse::<i64>().map_err(|_| {
                        ParseError::new(ParseErrorKind::InvalidIntegerLiteral, Some(token.span()))
                    })?;
                    self.advance();
                    Ok(query_ast::Literal::Int64(parsed))
                }
                TokenKind::Float(value) => {
                    let parsed = value.parse::<f64>().map_err(|_| {
                        ParseError::new(ParseErrorKind::InvalidFloatLiteral, Some(token.span()))
                    })?;
                    self.advance();
                    Ok(query_ast::Literal::Float64(parsed))
                }
                TokenKind::String(value) => {
                    let value = value.clone();
                    self.advance();
                    Ok(query_ast::Literal::String(value))
                }
                TokenKind::Keyword(Keyword::True) => {
                    self.advance();
                    Ok(query_ast::Literal::Bool(true))
                }
                TokenKind::Keyword(Keyword::False) => {
                    self.advance();
                    Ok(query_ast::Literal::Bool(false))
                }
                TokenKind::Keyword(Keyword::Null) => {
                    self.advance();
                    Ok(query_ast::Literal::Null)
                }
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        expected: "literal",
                    },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "literal",
                },
                None,
            )),
        }
    }

    fn ensure_eof(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) => Err(ParseError {
                kind: ParseErrorKind::UnexpectedToken { expected: "EOF" },
                span: Some(token.span()),
            }),
            None => Ok(()),
        }
    }

    fn consume_comma_if_present(&mut self) -> bool {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Comma => {
                self.advance();
                true
            }
            _ => false,
        }
    }

    fn consume_contextual_keyword_if_present(&mut self, keyword: &str) -> bool {
        match self.peek() {
            Some(token) if token_is_ident(token, keyword) => {
                self.advance();
                true
            }
            _ => false,
        }
    }

    fn consume_unary_arithmetic_op_if_present(&mut self) -> Option<query_ast::UnaryArithmeticOp> {
        match self.peek().map(Token::kind) {
            Some(TokenKind::Plus) => {
                self.advance();
                Some(query_ast::UnaryArithmeticOp::Plus)
            }
            Some(TokenKind::Minus) => {
                self.advance();
                Some(query_ast::UnaryArithmeticOp::Minus)
            }
            _ => None,
        }
    }

    fn next_token_is_lparen(&self) -> bool {
        self.tokens
            .get(self.cursor + 1)
            .is_some_and(|token| token.kind() == &TokenKind::LParen)
    }

    fn consume_i64_min_literal_if_present(&mut self) -> bool {
        match self.peek().map(Token::kind) {
            Some(TokenKind::Int(value)) if value == "9223372036854775808" => {
                self.advance();
                true
            }
            _ => false,
        }
    }
}

fn is_primary_expr_start(token: &Token) -> bool {
    if token_is_ident(token, "and") || token_is_ident(token, "or") || token_is_ident(token, "in") {
        return false;
    }

    matches!(
        token.kind(),
        TokenKind::Dot
            | TokenKind::Ident(_)
            | TokenKind::Int(_)
            | TokenKind::String(_)
            | TokenKind::LParen
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False)
            | TokenKind::Keyword(Keyword::Null)
    )
}

fn make_binary_expr(left: Expr, op: InfixOp, right: Expr) -> Expr {
    match op {
        InfixOp::Or => Expr::Or(Box::new(left), Box::new(right)),
        InfixOp::And => Expr::And(Box::new(left), Box::new(right)),
        InfixOp::Eq => Expr::Compare(CompareExpr::new(left, CompareOp::Eq, right)),
        InfixOp::Ne => Expr::Compare(CompareExpr::new(left, CompareOp::Ne, right)),
        InfixOp::Lt => Expr::Compare(CompareExpr::new(left, CompareOp::Lt, right)),
        InfixOp::Le => Expr::Compare(CompareExpr::new(left, CompareOp::Le, right)),
        InfixOp::Gt => Expr::Compare(CompareExpr::new(left, CompareOp::Gt, right)),
        InfixOp::Ge => Expr::Compare(CompareExpr::new(left, CompareOp::Ge, right)),
        InfixOp::Add => Expr::Arithmetic(ArithmeticExpr::new(left, ArithmeticOp::Add, right)),
        InfixOp::Sub => Expr::Arithmetic(ArithmeticExpr::new(left, ArithmeticOp::Sub, right)),
        InfixOp::Mul => Expr::Arithmetic(ArithmeticExpr::new(left, ArithmeticOp::Mul, right)),
        InfixOp::Div => Expr::Arithmetic(ArithmeticExpr::new(left, ArithmeticOp::Div, right)),
        InfixOp::Mod => Expr::Arithmetic(ArithmeticExpr::new(left, ArithmeticOp::Mod, right)),
        InfixOp::In | InfixOp::NotIn => unreachable!("membership expressions need a list RHS"),
    }
}

fn token_kind_description(token_kind: &TokenKind) -> &'static str {
    match token_kind {
        TokenKind::Keyword(keyword) => keyword.as_str(),
        TokenKind::Ident(_) => "IDENT",
        TokenKind::String(_) => "string",
        TokenKind::Float(_) => "float",
        TokenKind::Int(_) => "integer",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::Comma => ",",
        TokenKind::ColonEq => ":=",
        TokenKind::Colon => ":",
        TokenKind::Dot => ".",
        TokenKind::Eq => "=",
        TokenKind::Ne => "!=",
        TokenKind::Lt => "<",
        TokenKind::Le => "<=",
        TokenKind::Gt => ">",
        TokenKind::Ge => ">=",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::Percent => "%",
    }
}

fn token_is_ident(token: &Token, expected: &str) -> bool {
    matches!(token.kind(), TokenKind::Ident(value) if value == expected)
}

fn token_kind_is_ident(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident(_)
            | TokenKind::Keyword(
                Keyword::Start | Keyword::Transaction | Keyword::Commit | Keyword::Rollback
            )
    )
}

/// Parses one MVP `insert` statement from source text.
///
/// The parser checks syntax only. Schema names, field names, link traversal,
/// and type compatibility are validated by the resolver.
pub fn parse_insert(input: &str) -> Result<query_ast::InsertQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_insert_tokens(&tokens)
}

fn parse_insert_tokens(tokens: &[Token]) -> Result<query_ast::InsertQuery, ParseError> {
    Parser::new(tokens).parse_insert_stmt()
}

impl<'a> Parser<'a> {
    fn parse_insert_stmt(&mut self) -> Result<InsertQuery, ParseError> {
        self.expect_keyword(Keyword::Insert)?;
        let root_type_name = self.expect_ident()?;
        let assignments = self.parse_assignments()?;
        self.ensure_eof()?;

        Ok(InsertQuery::new(root_type_name, assignments))
    }

    fn parse_assignments(&mut self) -> Result<Vec<Assignment>, ParseError> {
        let mut results = Vec::new();
        self.expect_lbrace()?;

        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::RBrace)
        {
            self.expect_rbrace()?;
            return Ok(results);
        }

        results.push(self.parse_assignment()?);

        loop {
            if self.consume_comma_if_present() {
                if self
                    .peek()
                    .is_some_and(|token| token.kind() == &TokenKind::RBrace)
                {
                    break;
                }

                results.push(self.parse_assignment()?);
                continue;
            }

            match self.peek() {
                Some(token) if token.kind() == &TokenKind::RBrace => break,
                Some(token) => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedToken { expected: "comma" },
                        Some(token.span()),
                    ));
                }
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEof { expected: "}" },
                        None,
                    ));
                }
            }
        }

        self.expect_rbrace()?;

        Ok(results)
    }

    fn parse_assignment(&mut self) -> Result<Assignment, ParseError> {
        let field_name = self.expect_ident()?;
        self.expect_token(TokenKind::ColonEq)?;
        let value = self.expect_literal()?;

        Ok(Assignment::new(
            field_name,
            query_ast::AssignmentValue::Literal(value),
        ))
    }
}

/// Parses one MVP `update` statement from source text.
///
/// The parser preserves unresolved filters and assignments. Schema and
/// writable-field validation belong to the resolver.
pub fn parse_update(input: &str) -> Result<query_ast::UpdateQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_update_tokens(&tokens)
}

fn parse_update_tokens(tokens: &[Token]) -> Result<query_ast::UpdateQuery, ParseError> {
    Parser::new(tokens).parse_update_stmt()
}

impl Parser<'_> {
    fn parse_update_stmt(&mut self) -> Result<UpdateQuery, ParseError> {
        self.expect_keyword(Keyword::Update)?;
        let target_type_name = self.expect_ident()?;
        let filter = self.parse_filter_clause()?;
        self.expect_keyword(Keyword::Set)?;
        let assignments = self.parse_assignments()?;
        self.ensure_eof()?;

        Ok(UpdateQuery::new(target_type_name, filter, assignments))
    }
}

/// Parses one MVP `delete` statement from source text.
pub fn parse_delete(input: &str) -> Result<query_ast::DeleteQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    Parser::new(&tokens).parse_delete_stmt()
}

impl Parser<'_> {
    fn parse_delete_stmt(&mut self) -> Result<DeleteQuery, ParseError> {
        self.expect_keyword(Keyword::Delete)?;
        let target_type_name = self.expect_ident()?;
        let filter = self.parse_filter_clause()?;
        self.ensure_eof()?;

        Ok(DeleteQuery::new(target_type_name, filter))
    }
}
