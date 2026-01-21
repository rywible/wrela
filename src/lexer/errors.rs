#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
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
        code(lang::lex::unexpected_tab),
        help("Make sure your indents are exactly four spaces each")
    )]
    IndentNotMultipleOfFour {
        #[label("tab character found here")]
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
}
