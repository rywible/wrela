#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum LexError {
    #[error("unexpected indentation at top level")]
    #[diagnostic(
        code(lang::lex::unexpected_indent),
        help("Remove leading whitespace on the first line.")
    )]
    UnexpectedTopLevelIndent {
        #[label("indent starts here")]
        span: SourceSpan,
    },

    #[error(r"unexpected tab (\t) character")]
    #[diagnostic(
        code(lang::lex::unexpected_tab),
        help("Make sure your indents use spaces rather than tab characters")
    )]
    UnexpectedTabCharacter {
        #[label("tab character found here")]
        span: SourceSpan,
    },

    #[error(r"indent not multiple of four")]
    #[diagnostic(
        code(lang::lex::indent_error),
        help("Make sure your indents are exactly four spaces each")
    )]
    IndentNotMultipleOfFour {
        #[label("indentation here")]
        span: SourceSpan,
    },

    #[error("inconsistent indentation level")]
    #[diagnostic(
        code(lang::lex::inconsistent_indent),
        help("Dedent must match a previous indentation level.")
    )]
    InconsistentIndent {
        #[label("indentation here")]
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

    #[error("invalid multiline indent")]
    #[diagnostic(
        code(lang::lex::invalid_multiline_indent),
        help(
            "Content inside parentheses/brackets must be indented deeper than the enclosing block."
        )
    )]
    InvalidMultilineIndent {
        #[label("incorrect indentation here")]
        span: SourceSpan,
    },
}
