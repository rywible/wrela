pub mod catalog;
pub mod fixit;
pub mod suppress;

use miette::{Diagnostic, LabeledSpan, SourceSpan};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagStage {
    Lex,
    Parse,
    Validate,
    Project,
    Semantic,
    Type,
    Naming,
    Mir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DiagSpan {
    pub path: String,
    pub offset: usize,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DiagLabel {
    pub message: String,
    pub span: DiagSpan,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagFix {
    pub replacement: String,
    pub span: DiagSpan,
    pub rationale: String,
    pub confidence: f64,
    pub safety_tier: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagRecord {
    pub stage: DiagStage,
    pub severity: DiagSeverity,
    pub message: String,
    pub code: Option<String>,
    pub rule: Option<String>,
    pub help: Option<String>,
    pub labels: Vec<DiagLabel>,
    pub notes: Vec<String>,
    pub fixes: Vec<DiagFix>,
    pub diag_id: String,
    pub is_primary: bool,
    pub blocked_by: Option<String>,
    pub suppression_group: Option<String>,
}

impl DiagRecord {
    pub fn new(
        stage: DiagStage,
        severity: DiagSeverity,
        message: String,
        primary_path: String,
        primary_span: SourceSpan,
    ) -> Self {
        let primary_label = DiagLabel {
            message: "here".to_string(),
            span: DiagSpan {
                path: primary_path,
                offset: primary_span.offset(),
                len: primary_span.len(),
            },
            is_primary: true,
        };
        let mut out = Self {
            stage,
            severity,
            message,
            code: None,
            rule: None,
            help: None,
            labels: vec![primary_label],
            notes: Vec::new(),
            fixes: Vec::new(),
            diag_id: String::new(),
            is_primary: true,
            blocked_by: None,
            suppression_group: None,
        };
        out.diag_id = out.stable_id();
        out
    }

    pub fn with_code(mut self, code: Option<String>) -> Self {
        self.code = code;
        self.rule = self.code.as_deref().and_then(rule_from_code);
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_help(mut self, help: Option<String>) -> Self {
        self.help = help;
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = notes;
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_fixes(mut self, fixes: Vec<DiagFix>) -> Self {
        self.fixes = crate::diag::fixit::normalize_and_filter_fixes(fixes);
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_primary(mut self, is_primary: bool) -> Self {
        self.is_primary = is_primary;
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_suppression_group(mut self, suppression_group: Option<String>) -> Self {
        self.suppression_group = suppression_group;
        self.diag_id = self.stable_id();
        self
    }

    pub fn with_labels(mut self, labels: Vec<DiagLabel>) -> Self {
        if !labels.is_empty() {
            self.labels = labels;
        }
        self.diag_id = self.stable_id();
        self
    }

    pub fn from_diagnostic(
        stage: DiagStage,
        severity: DiagSeverity,
        diag: &dyn Diagnostic,
        primary_path: String,
        primary_span: SourceSpan,
    ) -> Self {
        let mut out = Self::new(
            stage,
            severity,
            diag.to_string(),
            primary_path,
            primary_span,
        )
        .with_code(diag.code().map(|v| v.to_string()))
        .with_help(diag.help().map(|v| v.to_string()));
        let mut labels = Vec::new();
        let primary_span = out.labels[0].span.clone();
        if let Some(iter) = diag.labels() {
            for label in iter {
                let converted =
                    diag_label_from_labeled_span(&out.labels[0].span.path, &label, false);
                if spans_overlap(&converted.span, &primary_span) {
                    if out.labels[0].message == "here" && !converted.message.is_empty() {
                        out.labels[0].message = converted.message;
                    }
                    continue;
                }
                labels.push(converted);
            }
        }
        if !labels.is_empty() {
            labels.insert(0, out.labels[0].clone());
            out = out.with_labels(labels);
        }
        out
    }

    fn stable_id(&self) -> String {
        let code = self.code.as_deref().unwrap_or("no_code");
        let primary = self.labels.first();
        let primary_key = primary
            .map(|label| {
                format!(
                    "{}:{}:{}",
                    label.span.path, label.span.offset, label.span.len
                )
            })
            .unwrap_or_else(|| "unknown:0:0".to_string());
        format!(
            "{:?}:{code}:{primary_key}:{}:{}",
            self.stage,
            self.suppression_group.as_deref().unwrap_or("none"),
            normalize_message_for_id(&self.message)
        )
    }

    pub fn dedupe_key(&self) -> String {
        let code = self.code.as_deref().unwrap_or("no_code");
        let primary = self.labels.first();
        let primary_key = primary
            .map(|label| {
                format!(
                    "{}:{}:{}",
                    label.span.path, label.span.offset, label.span.len
                )
            })
            .unwrap_or_else(|| "unknown:0:0".to_string());
        format!(
            "{:?}:{:?}:{code}:{primary_key}:{}",
            self.stage, self.severity, self.message
        )
    }
}

fn diag_label_from_labeled_span(path: &str, label: &LabeledSpan, is_primary: bool) -> DiagLabel {
    let span = label.inner();
    DiagLabel {
        message: label.label().unwrap_or("").to_string(),
        span: DiagSpan {
            path: path.to_string(),
            offset: span.offset(),
            len: span.len(),
        },
        is_primary,
    }
}

pub fn dedupe_records(records: Vec<DiagRecord>) -> Vec<DiagRecord> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for record in records {
        let key = record.dedupe_key();
        if seen.insert(key) {
            out.push(record);
        }
    }
    out
}

pub fn rule_from_code(code: &str) -> Option<String> {
    let (_, suffix) = code.rsplit_once("::")?;
    Some(suffix.to_string())
}

fn normalize_message_for_id(message: &str) -> String {
    message
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn spans_overlap(a: &DiagSpan, b: &DiagSpan) -> bool {
    if a.path != b.path {
        return false;
    }
    let a0 = a.offset;
    let a1 = a.offset.saturating_add(a.len);
    let b0 = b.offset;
    let b1 = b.offset.saturating_add(b.len);
    a0 < b1 && b0 < a1
}
