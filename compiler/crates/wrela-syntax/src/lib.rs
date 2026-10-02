//! wrela's syntax: for now the lexer. The parser, AST and formatter arrive in later slices.

mod lexer;

pub use lexer::{Lexed, Token, TokenKind, lex};

#[cfg(test)]
mod tests;
