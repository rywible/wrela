#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum LexError {
    #[error(r"unexpected tab (\t) character")]
    #[diagnostic(
        code(lang::lex::unexpected_tab),
        help("Make sure your indents use spaces rather than tab characters")
    )]
    UnexpectedTabCharacter {
        #[label("tab character found here")]
        span: SourceSpan,
    },

    #[error("unexpected character '{char}'")]
    #[diagnostic(
        code(lang::lex::unexpected_char),
        help("This character is not valid in this position.")
    )]
    UnexpectedCharacter {
        #[label("unexpected character")]
        span: SourceSpan,
        char: char,
    },

    #[error("unterminated string literal")]
    #[diagnostic(
        code(lang::lex::unterminated_string),
        help("Add a closing quote (\") to the end of the string.")
    )]
    UnterminatedString {
        #[label("string starts here")]
        span: SourceSpan,
    },

    #[error("invalid escape sequence '\\{char}'")]
    #[diagnostic(
        code(lang::lex::invalid_escape),
        help("Supported escapes are \\n, \\r, \\t, \\\", and \\\\")
    )]
    InvalidEscapeSequence {
        #[label("invalid escape")]
        span: SourceSpan,
        char: char,
    },

    #[error("legacy `so:` comments are not supported")]
    #[diagnostic(
        code(lang::lex::legacy_so_comment),
        help("Use `// ...` or `/* ... */` comments instead.")
    )]
    LegacySoComment {
        #[label("legacy comment syntax here")]
        span: SourceSpan,
    },
    #[error("invalid numeric literal")]
    #[diagnostic(
        code(lang::lex::invalid_literal),
        help("Check the format of this number.")
    )]
    InvalidLiteral {
        #[label("invalid literal")]
        span: SourceSpan,
    },
}
