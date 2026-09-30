use crate::source::ByteRange;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticCode {
    Lexical,
    Syntax,
    Limit,
    Truncated,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub range: ByteRange,
}
impl Diagnostic {
    pub fn new(code: DiagnosticCode, message: impl Into<String>, range: ByteRange) -> Self {
        Self {
            code,
            message: message.into(),
            range,
        }
    }
}
/// Reserves the last slot for the fact that further errors were suppressed.
pub fn push_bounded(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic, maximum: usize) {
    let maximum = maximum.max(1);
    if diagnostics.len() < maximum {
        diagnostics.push(diagnostic);
    } else if diagnostics
        .last()
        .is_some_and(|d| d.code != DiagnosticCode::Truncated)
    {
        diagnostics[maximum - 1] = Diagnostic::new(
            DiagnosticCode::Truncated,
            "further diagnostics suppressed",
            diagnostic.range,
        );
    }
}
