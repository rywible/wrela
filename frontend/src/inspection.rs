//! JSON inspection is an external representation; the syntax domains remain typed.
use crate::{
    ParsedDocument, cst::CstNode, diagnostic::Diagnostic, source::Source, syntax::Document,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct Inspection<'a> {
    pub syntax_policy: &'static str,
    pub syntax_eligible: bool,
    pub complete: bool,
    pub source: &'a Source,
    pub syntax: &'a Document,
    pub cst: CstNode,
    pub diagnostics: &'a [Diagnostic],
}
impl<'a> From<&'a ParsedDocument> for Inspection<'a> {
    fn from(document: &'a ParsedDocument) -> Self {
        Self {
            syntax_policy: "0.1",
            syntax_eligible: document.is_syntax_eligible(),
            complete: document.complete,
            source: &document.source,
            syntax: &document.syntax,
            cst: document.cst(),
            diagnostics: &document.diagnostics,
        }
    }
}
