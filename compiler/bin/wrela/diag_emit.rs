#![allow(unused_assignments)]

use super::contracts::OutputFormat;
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;
use wrela::diag::suppress::suppress_cascades;
use wrela::diag::{DiagLabel, DiagRecord, DiagSeverity, DiagSpan, dedupe_records};

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[allow(unused_assignments)]
struct ProjectDiag {
    message: String,
    #[label("here")]
    span: SourceSpan,
}

#[derive(Serialize)]
struct JsonSpan {
    offset: usize,
    len: usize,
}

#[derive(Serialize)]
struct JsonDiag {
    kind: String,
    message: String,
    path: String,
    span: JsonSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<Vec<JsonLabel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diag_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestions: Option<Vec<JsonSuggestion>>,
}

#[derive(Serialize)]
struct JsonSuggestion {
    replacement: String,
    span: JsonSpan,
    rationale: String,
    confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<String>,
}

#[derive(Serialize)]
struct JsonLabel {
    message: String,
    span: JsonSpan,
    is_primary: bool,
}

pub fn print_help() {
    println!("{}", help_text());
}

pub fn help_text() -> &'static str {
    "usage: wrela <command> [options] <path> [-- args]\n\
\n\
commands:\n\
  init [path]           initialize a new project\n\
  update                update the installed toolchain\n\
  check <path>          parse and typecheck (no codegen)\n\
  build <path>          run certification, then compile executable on success only\n\
  compile <path>        alias for build (also certification-gated)\n\
  verify-cert <path>    verify an emitted cert.json report and hashes\n\
  run <path>            compile and run\n\
  dev <path>            watch and rebuild (polling)\n\
  test [path]           run tests from project root or a single .wr file\n\
  perf [path]           run perf harness and write baseline JSON\n\
  perfcmp [path]        run paired baseline/candidate perf comparison\n\
  matrix [path]         run workspace test/spec/perf matrix and write evidence bundle\n\
\n\
options:\n\
  --prefix PATH         install/update prefix (default: $PREFIX or ~/.local/wrela)\n\
  -o, --out PATH        output path for build/run\n\
  --emit-mir            emit MIR before optimization\n\
  --emit-mir-opt        emit MIR after optimization\n\
  --emit-obj=PATH       emit object file\n\
  --emit-bin=PATH       emit executable\n\
  --poll-ms=N           poll interval for dev (default: 500)\n\
  --jobs=N              test runner parallelism (default: 1)\n\
  --test-timeout-ms=N   per-test timeout in milliseconds (default: 5000)\n\
  env: WRELA_BUDGET_*   Budget Policy v1 overrides (autogen/sim/fuzz/mutation + time caps)\n\
  --record              test maintenance mode; updates integration cassettes\n\
  --update-public-surface  test maintenance mode; updates API snapshot baselines\n\
  --list                list discovered tests with stable id/lane metadata\n\
  --id=ID               run/list a single test by stable id\n\
  --filter=PATTERN      run/list tests matching pattern\n\
  --lane=NAME           run/list tests for lane (spec|integration|sim|model|default); valid for test/perf\n\
  --seed=N              schedule seed for sim tests; valid for test/perf\n\
  --benchmark-manifest=PATH  benchmark manifest path (bench.toml)\n\
  --profile=NAME        benchmark profile (smoke|standard|deep)\n\
  --repro PATH          replay a single typed repro artifact (autogen|fuzz)\n\
  --perf-debug          dump perf counters after tests\n\
  --runs=N              perf harness run count (default: 5)\n\
  --baseline-out=PATH   perf baseline JSON output path\n\
  --perf-gate=PATH      compare perf summary against baseline JSON\n\
  --perf-max-regression-pct=N  allowed regression percentage (default: 5)\n\
  --perf-cv-max-pct=N   max coefficient of variation percentage (default: 5)\n\
  --kpi-check-fallback-max=N  max allowed check fallback rate\n\
  --kpi-check-batch-min=N  minimum required average check batch size\n\
  --kpi-scheduler-p99-improve-min-pct=N  min scheduler p99 improvement vs baseline\n\
  --kpi-rewrite-overhead-max-pct=N  max rewrite compile overhead percentage\n\
  --kpi-actor-throughput-improve-min-pct=N  min actor throughput improvement vs baseline\n\
  --kpi-queue-age-p99-max-regress-pct=N  max queue age p99 regression percentage\n\
  --kpi-starvation-violations-max=N  max scheduler starvation violations\n\
  --kpi-scheduler-throughput-improve-min-pct=N  min scheduler throughput improvement vs baseline\n\
  --kpi-scheduler-loop-p99-max-regress-pct=N  max scheduler loop p99 regression percentage\n\
  --kpi-scheduler-local-hit-min=N  minimum local dispatch hit ratio\n\
  --baseline-ref=REF    perfcmp baseline git ref (default: origin/main)\n\
  --candidate-ref=REF   perfcmp candidate git ref (default: HEAD)\n\
  --warmup-pairs=N      perfcmp warmup pair count override\n\
  --measure-pairs=N     perfcmp measured pair count override\n\
  --min-effect-pct=N    perfcmp practical effect threshold (default: 2.0)\n\
  --confidence=N        perfcmp bootstrap CI confidence percent (default: 95)\n\
  --format=json         emit diagnostics as JSON\n\
  -h, --help            show this help\n\
  -V, --version         show version\n"
}

pub(super) fn emit_diag_record(format: OutputFormat, record: &DiagRecord, source: &str) {
    let primary = record.labels.first().cloned().unwrap_or_else(|| DiagLabel {
        message: "here".to_string(),
        span: DiagSpan {
            path: "<unknown>".to_string(),
            offset: 0,
            len: 0,
        },
        is_primary: true,
    });
    let span = clamp_source_span(source, primary.span.offset, primary.span.len);
    match format {
        OutputFormat::Pretty => {
            let report = Report::new(ProjectDiag {
                message: record.message.clone(),
                span,
            })
            .with_source_code(NamedSource::new(
                primary.span.path.clone(),
                source.to_string(),
            ));
            if matches!(record.severity, DiagSeverity::Warning) {
                eprintln!("warning: {report:?}");
            } else {
                eprintln!("{report:?}");
            }
            if let Some(code) = &record.code {
                eprintln!("code: {code}");
            }
            if let Some(help) = &record.help {
                eprintln!("help: {help}");
            }
            let (primary_line, primary_col) = line_col_at_offset(source, primary.span.offset);
            let related = record
                .labels
                .iter()
                .filter(|label| {
                    if label.is_primary {
                        return false;
                    }
                    if label.span.path != primary.span.path {
                        return true;
                    }
                    if primary.span.offset.abs_diff(label.span.offset) <= 1 {
                        return false;
                    }
                    let (line, col) = line_col_at_offset(source, label.span.offset);
                    line != primary_line || col != primary_col
                })
                .collect::<Vec<_>>();
            if !related.is_empty() {
                eprintln!("related:");
                for label in related {
                    let (line, col) = line_col_at_offset(source, label.span.offset);
                    eprintln!(
                        "  - {} at {}:{}:{}",
                        if label.message.is_empty() {
                            "related location"
                        } else {
                            label.message.as_str()
                        },
                        label.span.path,
                        line,
                        col
                    );
                }
            }
            for note in &record.notes {
                eprintln!("note: {note}");
            }
            for fix in &record.fixes {
                eprintln!(
                    "suggested fix [{}] (confidence {:.2}): {}",
                    fix.safety_tier, fix.confidence, fix.rationale
                );
            }
        }
        OutputFormat::Json => {
            emit_json_diag_for_record(record);
        }
    }
}

fn clamp_source_span(source: &str, offset: usize, len: usize) -> SourceSpan {
    let clamped_offset = offset.min(source.len());
    let max_len = source.len().saturating_sub(clamped_offset);
    let clamped_len = len.min(max_len);
    SourceSpan::from((clamped_offset, clamped_len))
}

fn line_col_at_offset(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for b in source.as_bytes().iter().take(clamped) {
        if *b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub(super) fn emit_json_diag_for_record(record: &DiagRecord) {
    let primary = record.labels.first().cloned().unwrap_or_else(|| DiagLabel {
        message: "here".to_string(),
        span: DiagSpan {
            path: "<unknown>".to_string(),
            offset: 0,
            len: 0,
        },
        is_primary: true,
    });
    let json = JsonDiag {
        kind: if matches!(record.severity, DiagSeverity::Warning) {
            "warning".to_string()
        } else {
            "error".to_string()
        },
        message: record.message.clone(),
        path: primary.span.path,
        span: JsonSpan {
            offset: primary.span.offset,
            len: primary.span.len,
        },
        stage: Some(format!("{:?}", record.stage).to_ascii_lowercase()),
        severity: Some(if matches!(record.severity, DiagSeverity::Warning) {
            "warning".to_string()
        } else {
            "error".to_string()
        }),
        code: record.code.clone(),
        rule: record.rule.clone(),
        help: record.help.clone(),
        labels: Some(
            record
                .labels
                .iter()
                .map(|label| JsonLabel {
                    message: label.message.clone(),
                    span: JsonSpan {
                        offset: label.span.offset,
                        len: label.span.len,
                    },
                    is_primary: label.is_primary,
                })
                .collect(),
        ),
        notes: if record.notes.is_empty() {
            None
        } else {
            Some(record.notes.clone())
        },
        diag_id: Some(record.diag_id.clone()),
        suggestions: if record.fixes.is_empty() {
            None
        } else {
            Some(
                record
                    .fixes
                    .iter()
                    .map(|fix| JsonSuggestion {
                        replacement: fix.replacement.clone(),
                        span: JsonSpan {
                            offset: fix.span.offset,
                            len: fix.span.len,
                        },
                        rationale: fix.rationale.clone(),
                        confidence: fix.confidence,
                        safety_tier: Some(fix.safety_tier.clone()),
                        reason_code: Some(fix.reason_code.clone()),
                    })
                    .collect(),
            )
        },
    };
    println!(
        "{}",
        serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
    );
}

pub(super) fn emit_deduped_records_with_sources(
    format: OutputFormat,
    records: Vec<(DiagRecord, String)>,
) {
    let mut source_by_id = HashMap::new();
    let mut deduped = Vec::new();
    for (record, source) in records {
        source_by_id.entry(record.diag_id.clone()).or_insert(source);
        deduped.push(record);
    }
    for record in suppress_cascades(dedupe_records(deduped)) {
        let source = source_by_id
            .get(&record.diag_id)
            .cloned()
            .unwrap_or_default();
        emit_diag_record(format, &record, &source);
    }
}
