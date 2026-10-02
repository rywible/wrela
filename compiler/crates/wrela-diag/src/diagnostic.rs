//! The diagnostic data model and its builder.

use std::fmt;

use crate::{Code, Span};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A span with an optional message shown next to it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Label {
    pub span: Span,
    pub message: Option<String>,
}

impl Label {
    pub fn new(span: Span) -> Self {
        Label {
            span,
            message: None,
        }
    }

    pub fn with_message(span: Span, message: impl Into<String>) -> Self {
        Label {
            span,
            message: Some(message.into()),
        }
    }
}

/// Replace the text at `span` with `replacement`. An empty span inserts.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Edit {
    pub span: Span,
    pub replacement: String,
}

impl Edit {
    pub fn new(span: Span, replacement: impl Into<String>) -> Self {
        Edit {
            span,
            replacement: replacement.into(),
        }
    }
}

/// Advice for fixing a diagnostic. When `edits` is non-empty, applying them all is the fix.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Help {
    pub message: String,
    pub edits: Vec<Edit>,
}

/// One problem found in the source.
///
/// Messages are plain language: lowercase start, no trailing period, specifics in backticks
/// (`` unexpected character `$` ``). Help gives a concrete fix when one exists.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    /// Where the problem is. Conformance annotations match on this span's line.
    pub primary: Label,
    /// Related places, such as an earlier definition.
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<Help>,
}

impl Diagnostic {
    /// A diagnostic with the code's default severity and an unlabelled primary span.
    pub fn new(code: Code, message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            code,
            severity: code.default_severity(),
            message: message.into(),
            primary: Label::new(span),
            secondary: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Sets the message shown at the primary span.
    #[must_use]
    pub fn with_label(mut self, message: impl Into<String>) -> Self {
        self.primary.message = Some(message.into());
        self
    }

    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label::with_message(span, message));
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds advice without an edit.
    #[must_use]
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.help.push(Help {
            message: message.into(),
            edits: Vec::new(),
        });
        self
    }

    /// Adds advice with the edits that carry it out.
    #[must_use]
    pub fn with_fix(mut self, message: impl Into<String>, edits: Vec<Edit>) -> Self {
        self.help.push(Help {
            message: message.into(),
            edits,
        });
        self
    }

    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// The order diagnostics are reported in: by file, then position, then code.
    pub fn sort_key(&self) -> (Span, Code) {
        (self.primary.span, self.code)
    }
}
