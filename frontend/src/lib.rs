//! Wrela's source frontend. Syntax admission is deliberately separate from semantic validity.
pub mod cst;
pub mod diagnostic;
pub mod inspection;
pub mod lexer;
pub mod parsing;
pub mod source;
pub mod syntax;
pub mod token;
mod unicode16;

use diagnostic::{Diagnostic, DiagnosticCode, push_bounded};
use lexer::Limits;
use source::Source;
use syntax::Document;

/// An inspectable result, including all rejected input bytes and partial syntax.
#[derive(Debug)]
pub struct ParsedDocument {
    pub source: Source,
    pub syntax: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub complete: bool,
    diagnostic_limit: usize,
}

/// A syntax-valid document admitted to subsequent phases. This says nothing about types.
/// Construction is private so a recovered/error document cannot bypass admission.
#[derive(Debug)]
pub struct SyntaxValidDocument {
    source: Source,
    syntax: Document,
}
impl SyntaxValidDocument {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn syntax(&self) -> &Document {
        &self.syntax
    }
}

pub fn parse(bytes: &[u8]) -> ParsedDocument {
    parse_with_limits(bytes, Limits::default())
}

pub fn parse_with_limits(bytes: &[u8], limits: Limits) -> ParsedDocument {
    let lexed = lexer::lex(bytes, limits);
    let parsed = parsing::parse_with_limit(&lexed.source, &lexed.tokens, limits.max_diagnostics);
    let mut diagnostics = lexed.diagnostics;
    for diagnostic in parsed.diagnostics {
        push_bounded(&mut diagnostics, diagnostic, limits.max_diagnostics);
    }
    ParsedDocument {
        source: lexed.source,
        syntax: parsed.document,
        diagnostics,
        complete: parsed.complete,
        diagnostic_limit: limits.max_diagnostics,
    }
}

impl ParsedDocument {
    pub fn cst(&self) -> cst::CstNode {
        cst::export(&self.syntax, &self.source)
    }

    pub fn is_syntax_eligible(&self) -> bool {
        self.complete
            && self.diagnostics.is_empty()
            && !self.syntax.has_errors()
            && cst::validate(&self.cst(), &self.source).is_ok()
    }

    #[allow(clippy::result_large_err)]
    pub fn admit(mut self) -> Result<SyntaxValidDocument, Self> {
        if !self.complete || !self.diagnostics.is_empty() || self.syntax.has_errors() {
            return Err(self);
        }
        if let Err(errors) = cst::validate(&self.cst(), &self.source) {
            for error in errors {
                push_bounded(
                    &mut self.diagnostics,
                    Diagnostic::new(
                        DiagnosticCode::Syntax,
                        format!("syntax structure is invalid: {error:?}"),
                        self.syntax.syntax_range,
                    ),
                    self.diagnostic_limit,
                );
            }
            return Err(self);
        }
        Ok(SyntaxValidDocument {
            source: self.source,
            syntax: self.syntax,
        })
    }
}
