//! The machine-readable output, for agents and tools.
//!
//! ```json
//! {
//!   "version": 1,
//!   "diagnostics": [{
//!     "code": "E0001", "severity": "error", "message": "unexpected character `$`",
//!     "primary": { "span": SPAN, "message": "not part of any token" },
//!     "secondary": [], "notes": [],
//!     "help": [{ "message": "...", "edits": [{ "span": SPAN, "replacement": "..." }] }]
//!   }]
//! }
//! ```
//!
//! A `SPAN` is `{ "file": NAME, "start": POS, "end": POS }`, and a `POS` is
//! `{ "offset": BYTES, "line": 1-BASED, "column": 1-BASED CHARS }`. Every key is always present;
//! an absent label message is `null`. Changing this shape means bumping [`JSON_VERSION`]; the
//! golden tests pin it.

use serde::Serialize;

use crate::{Code, Diagnostic, Edit, Help, Label, Severity, SourceMap, Span};

/// The version of the JSON shape.
pub const JSON_VERSION: u32 = 1;

#[derive(Serialize)]
struct Output<'a> {
    version: u32,
    diagnostics: Vec<JsonDiagnostic<'a>>,
}

#[derive(Serialize)]
struct JsonDiagnostic<'a> {
    code: Code,
    severity: Severity,
    message: &'a str,
    primary: JsonLabel<'a>,
    secondary: Vec<JsonLabel<'a>>,
    notes: &'a [String],
    help: Vec<JsonHelp<'a>>,
}

#[derive(Serialize)]
struct JsonLabel<'a> {
    span: JsonSpan<'a>,
    message: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonHelp<'a> {
    message: &'a str,
    edits: Vec<JsonEdit<'a>>,
}

#[derive(Serialize)]
struct JsonEdit<'a> {
    span: JsonSpan<'a>,
    replacement: &'a str,
}

#[derive(Serialize)]
struct JsonSpan<'a> {
    /// The file's name, or `"<unknown>"` with line and column 0 if the span's file isn't in the map.
    file: &'a str,
    start: Position,
    end: Position,
}

#[derive(Serialize)]
struct Position {
    offset: u32,
    line: u32,
    column: u32,
}

/// The diagnostics as a pretty-printed JSON document, with a trailing newline.
pub fn to_json(diagnostics: &[Diagnostic], sources: &SourceMap) -> String {
    let output = Output {
        version: JSON_VERSION,
        diagnostics: diagnostics.iter().map(|d| diagnostic(d, sources)).collect(),
    };
    // Serializing these plain structs can't fail: every map key is a string.
    let mut json = serde_json::to_string_pretty(&output).unwrap_or_default();
    json.push('\n');
    json
}

fn diagnostic<'a>(d: &'a Diagnostic, sources: &'a SourceMap) -> JsonDiagnostic<'a> {
    JsonDiagnostic {
        code: d.code,
        severity: d.severity,
        message: &d.message,
        primary: label(&d.primary, sources),
        secondary: d.secondary.iter().map(|l| label(l, sources)).collect(),
        notes: &d.notes,
        help: d.help.iter().map(|h| help(h, sources)).collect(),
    }
}

fn label<'a>(label: &'a Label, sources: &'a SourceMap) -> JsonLabel<'a> {
    JsonLabel {
        span: span(label.span, sources),
        message: label.message.as_deref(),
    }
}

fn help<'a>(help: &'a Help, sources: &'a SourceMap) -> JsonHelp<'a> {
    JsonHelp {
        message: &help.message,
        edits: help.edits.iter().map(|e| edit(e, sources)).collect(),
    }
}

fn edit<'a>(edit: &'a Edit, sources: &'a SourceMap) -> JsonEdit<'a> {
    JsonEdit {
        span: span(edit.span, sources),
        replacement: &edit.replacement,
    }
}

fn span(span: Span, sources: &SourceMap) -> JsonSpan<'_> {
    let Some(file) = sources.get(span.file) else {
        let unknown = |offset| Position {
            offset,
            line: 0,
            column: 0,
        };
        return JsonSpan {
            file: "<unknown>",
            start: unknown(span.start),
            end: unknown(span.end),
        };
    };
    let position = |offset: u32| {
        let at = file.line_col(offset);
        Position {
            offset: offset.min(file.len()),
            line: at.line,
            column: at.column,
        }
    };
    JsonSpan {
        file: file.name(),
        start: position(span.start),
        end: position(span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::UNEXPECTED_CHARACTER;
    use crate::{Diagnostic, Edit};

    #[test]
    fn json_has_offsets_lines_and_columns() {
        let mut map = SourceMap::new();
        let f = map.add("a.wrela", "x\nπ$\n").unwrap();
        let d = Diagnostic::new(
            UNEXPECTED_CHARACTER,
            "unexpected character `$`",
            Span::new(f, 4, 5),
        )
        .with_fix("delete it", vec![Edit::new(Span::new(f, 4, 5), "")]);
        let value: serde_json::Value = serde_json::from_str(&to_json(&[d], &map)).unwrap();
        assert_eq!(value["version"], 1);
        let diag = &value["diagnostics"][0];
        assert_eq!(diag["code"], "E0001");
        assert_eq!(diag["severity"], "error");
        assert_eq!(diag["primary"]["message"], serde_json::Value::Null);
        let span = &diag["primary"]["span"];
        assert_eq!(span["file"], "a.wrela");
        assert_eq!(
            span["start"],
            serde_json::json!({"offset": 4, "line": 2, "column": 2})
        );
        assert_eq!(
            span["end"],
            serde_json::json!({"offset": 5, "line": 2, "column": 3})
        );
        assert_eq!(diag["help"][0]["edits"][0]["replacement"], "");
    }

    #[test]
    fn empty_output_is_still_versioned() {
        assert_eq!(
            to_json(&[], &SourceMap::new()),
            "{\n  \"version\": 1,\n  \"diagnostics\": []\n}\n"
        );
    }
}
