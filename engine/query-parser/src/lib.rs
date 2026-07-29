#![no_std]
//! Parser frontend for the query language.
//!
//! This crate is responsible only for turning query text into `query-ast`
//! values. It must not validate schema names, resolve fields, lower to IR, or
//! generate SQL.
//!
//! The lexer attaches a [`Span`] to every token with byte, line, and column
//! positions so parser errors can report the exact source location from the
//! beginning of the implementation.

extern crate alloc;

mod lexer;
mod parser;

pub use lexer::{Keyword, LexError, LexErrorKind, Position, Span, Token, TokenKind, lex};
pub use parser::{ParseError, ParseErrorKind, parse_insert, parse_select, parse_update};

#[cfg(test)]
mod tests;
