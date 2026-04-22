//! Owns CLI diagnostic rendering for pretty, JSON, and SARIF output formats.
//! Does not own diagnostic production or command selection logic.
//!
//! Key invariants:
//! - each output format is a projection of the same diagnostic record set.
//! - cascaded diagnostics are suppressed before rendering so summaries stay
//!   truthful to the intended public surface.
//! - JSON/SARIF fields must preserve stable keys because tooling consumes them.
//!
//! Primary entrypoints:
//! - `emit_project_diagnostics`
//! - `emit_json_diagnostics`
//! - `emit_sarif_diagnostics`
//!
//! Failure modes / common pitfalls:
//! - ad hoc format-specific filtering here can make different output modes tell
//!   different stories about the same failure.
//! - losing source-span fidelity in projection code makes automated fixes and
//!   editor integrations much less trustworthy.

#![allow(unused_assignments)]

use super::contracts::OutputFormat;
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;
use wrela::diag::suppress::suppress_cascades;
use wrela::diag::{DiagLabel, DiagRecord, DiagSeverity, DiagSpan, dedupe_records};

const DIAGNOSTIC_SCHEMA_VERSION: u32 = 2;

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
    schema_version: u32,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    override_tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expansion_node_id: Option<String>,
}

#[derive(Serialize)]
struct JsonSuggestion {
    replacement: String,
    span: JsonSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_source: Option<String>,
    rationale: String,
    confidence: f64,
    applicability: String,
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

#[derive(Serialize)]
struct SarifLog {
    #[serde(rename = "$schema")]
    schema: String,
    version: String,
    runs: Vec<SarifRun>,
}

#[derive(Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<SarifRunProperties>,
}

#[derive(Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
struct SarifDriver {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Serialize)]
struct SarifRunProperties {
    diagnostic_schema_version: u32,
}

#[derive(Serialize)]
struct SarifResult {
    #[serde(skip_serializing_if = "Option::is_none", rename = "ruleId")]
    rule_id: Option<String>,
    level: String,
    message: SarifMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    locations: Option<Vec<SarifLocation>>,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    region: SarifRegion,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
    #[serde(rename = "charOffset")]
    char_offset: usize,
    #[serde(rename = "charLength")]
    char_length: usize,
}

pub fn print_help() {
    print!("{}", help_text());
}

pub fn help_text() -> &'static str {
    "usage: wrela <command> [options] <path> [-- args]\n\
\n\
commands:\n\
  init [path]           initialize a new project\n\
  update                update the installed toolchain\n\
  check <path>          parse and typecheck (no codegen)\n\
  analyze <path>        alias for check (parse and typecheck; no codegen)\n\
  fix <path>            apply safe non-overlapping fixes\n\
  fmt <path>            canonical formatting pass (idempotent)\n\
  build <path>          run certification, then compile executable on success only\n\
  compile <path>        alias for build (also certification-gated)\n\
  query-contracts       list family/question query contracts and compatibility aliases\n\
  collision-contracts   list collision contracts and witness schemas\n\
  collision-plan        list collision plans and validation summaries\n\
  collision-run         execute representative collision plans and emit runtime diagnostics\n\
  preview <path>        emit a named view attachment (default: color) as PPM\n\
  frame <path>          evaluate a named view and emit typed frame attachments or PPM\n\
  frame-live <path>     open a live frame window with click-to-log selection output\n\
  frame-contracts <path>  inspect compiled frame contracts for named views\n\
  presentation-plan <path>  inspect compiled presentation contracts and pass plan\n\
  presentation-debug <path> run presentation passes and export color/depth/normal debug attachments\n\
  verify-cert <path>    verify an emitted cert.json report and hashes\n\
  run <path>            compile and run\n\
  dev <path>            watch and rebuild (polling)\n\
  test [path]           run tests from a project root directory\n\
  eval <kind> <path>    run agent evaluation harnesses (currently: one-shot v2 executable corpus)\n\
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
  --query-backend=NAME  compile direct world/view queries with backend (cpu|virtual_gpu|wgsl|auto)\n\
  preview/frame flags:  --view NAME --region NAME --domain NAME --attachment NAME --width N --height N\n\
  frame flags:          --attachment-format=json|ppm\n\
  preview flag:         --json-report\n\
  --integration-mode    run/build fixture mode; bypasses strict naming policy for integration executables\n\
  --poll-ms=N           poll interval for dev (default: 500)\n\
  --jobs=N              test runner parallelism (default: 1)\n\
  --test-timeout-ms=N   per-test timeout in milliseconds (default: 10000)\n\
  env: WRELA_BUDGET_*   Budget Policy v1 overrides (autogen/sim/fuzz/mutation + time caps)\n\
  --record              test maintenance mode; updates integration cassettes\n\
  --update-public-surface  test maintenance mode; updates API snapshot baselines\n\
  --list                list discovered tests with stable id/lane metadata\n\
  --id=ID               run/list a single test by stable id\n\
  --filter=PATTERN      run/list tests matching pattern\n\
  --lane=NAME           run/list tests for lane (fast|full|spec|integration|sim|model|default); fast=spec+default, full=all lanes; valid for test/perf\n\
  --seed=N              schedule seed for sim tests; valid for test/perf\n\
  --benchmark-manifest=PATH  benchmark manifest path (bench.toml)\n\
  --profile=NAME        benchmark profile (smoke|standard|deep|1080p120)\n\
  --repro PATH          replay a single typed repro artifact (autogen|fuzz)\n\
  --replay-trace PATH   validate a sim/model replay trace artifact\n\
  --perf-debug          dump perf counters after tests\n\
  --runs=N              perf/eval run count (default: 5)\n\
  --baseline-out=PATH   perf baseline JSON output path\n\
  --perf-gate=PATH      compare perf summary against baseline JSON\n\
  --perf-max-regression-pct=N  allowed regression percentage (default: 5)\n\
  --perf-cv-max-pct=N   max coefficient of variation percentage (default: 5)\n\
  --why-not-120         perf: print the structured 1080p120 closure diagnosis\n\
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
  --json                shorthand for --error-format=json\n\
  --holes-only          check/analyze mode: emit typed-hole diagnostics only\n\
  --allow-review-fixes  fix/fmt mode: also apply review-tier fixes\n\
  --workspace-diagnostics  fix/fmt: include diagnostics/fixes from imported modules (default: target file only)\n\
  --strict-naming       check/analyze/build/compile/run/dev: promote naming strong/style tiers to errors\n\
  --error-format=human|json|sarif  set diagnostic output format (default: human)\n\
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
    let span = SourceSpan::from((primary.span.offset, primary.span.len));
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
        OutputFormat::Sarif => {
            emit_sarif_for_records(vec![(record, source)]);
        }
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FrontendMachineFields {
    preset_id: Option<String>,
    override_tier: Option<u8>,
    expansion_node_id: Option<String>,
}

fn frontend_machine_fields(record: &DiagRecord) -> FrontendMachineFields {
    let mut fields = FrontendMachineFields::default();
    if let Some(data) = record.data.as_ref() {
        fields.preset_id = data_string_field(data, "preset_id");
        fields.override_tier = data_u8_field(data, "override_tier");
        fields.expansion_node_id = data_string_field(data, "expansion_node_id");
    }
    if fields.override_tier.is_none() {
        fields.override_tier = override_tier_from_message(&record.message);
    }
    if fields.preset_id.is_none() {
        fields.preset_id = quoted_suffix_field(&record.message, "preset `");
    }
    if fields.expansion_node_id.is_none() {
        fields.expansion_node_id = quoted_suffix_field(&record.message, "expansion node `");
    }
    fields
}

fn data_string_field(data: &serde_json::Value, key: &str) -> Option<String> {
    data_field_value(data, key)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn data_u8_field(data: &serde_json::Value, key: &str) -> Option<u8> {
    let value = data_field_value(data, key)?;
    if let Some(raw) = value.as_u64() {
        return u8::try_from(raw).ok();
    }
    value.as_str().and_then(|raw| raw.parse::<u8>().ok())
}

fn data_field_value<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    let top = data.as_object()?;
    if let Some(value) = top.get(key) {
        return Some(value);
    }
    for nested_key in ["machine_fields", "frontend", "metadata"] {
        let Some(nested) = top.get(nested_key).and_then(serde_json::Value::as_object) else {
            continue;
        };
        if let Some(value) = nested.get(key) {
            return Some(value);
        }
    }
    None
}

fn override_tier_from_message(message: &str) -> Option<u8> {
    let marker = "required tier `";
    let start = message.find(marker)?.saturating_add(marker.len());
    let next = message.get(start..)?.chars().next()?;
    if matches!(next, '0' | '1' | '2') {
        return next.to_digit(10).and_then(|value| u8::try_from(value).ok());
    }
    None
}

fn quoted_suffix_field(message: &str, prefix: &str) -> Option<String> {
    let start = message.find(prefix)?.saturating_add(prefix.len());
    let remainder = message.get(start..)?;
    let end = remainder.find('`')?;
    let value = remainder.get(..end)?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

fn json_diag_for_record(record: &DiagRecord) -> JsonDiag {
    let primary = record.labels.first().cloned().unwrap_or_else(|| DiagLabel {
        message: "here".to_string(),
        span: DiagSpan {
            path: "<unknown>".to_string(),
            offset: 0,
            len: 0,
        },
        is_primary: true,
    });
    let machine_fields = frontend_machine_fields(record);
    JsonDiag {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
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
                        expected_source: fix.expected_source.clone(),
                        rationale: fix.rationale.clone(),
                        confidence: fix.confidence,
                        applicability: if fix.expected_source.is_none() {
                            "has_placeholders".to_string()
                        } else if fix.safety_tier == "safe" {
                            "machine_applicable".to_string()
                        } else {
                            "maybe_correct".to_string()
                        },
                        safety_tier: Some(fix.safety_tier.clone()),
                        reason_code: Some(fix.reason_code.clone()),
                    })
                    .collect(),
            )
        },
        data: record.data.clone(),
        preset_id: machine_fields.preset_id,
        override_tier: machine_fields.override_tier,
        expansion_node_id: machine_fields.expansion_node_id,
    }
}

pub(super) fn emit_json_diag_for_record(record: &DiagRecord) {
    let json = json_diag_for_record(record);
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
    let records = suppress_cascades(dedupe_records(deduped));
    if matches!(format, OutputFormat::Sarif) {
        let refs = records
            .iter()
            .map(|record| {
                (
                    record,
                    source_by_id
                        .get(&record.diag_id)
                        .map(|source| source.as_str())
                        .unwrap_or(""),
                )
            })
            .collect::<Vec<_>>();
        emit_sarif_for_records(refs);
        return;
    }
    for record in records {
        let source = source_by_id
            .get(&record.diag_id)
            .cloned()
            .unwrap_or_default();
        emit_diag_record(format, &record, &source);
    }
}

fn emit_sarif_for_records(records: Vec<(&DiagRecord, &str)>) {
    let results = records
        .into_iter()
        .map(|(record, source)| {
            let primary = record.labels.first().cloned().unwrap_or_else(|| DiagLabel {
                message: "here".to_string(),
                span: DiagSpan {
                    path: "<unknown>".to_string(),
                    offset: 0,
                    len: 0,
                },
                is_primary: true,
            });
            let (start_line, start_column) = line_col_at_offset(source, primary.span.offset);
            SarifResult {
                rule_id: record.code.clone().or_else(|| record.rule.clone()),
                level: if matches!(record.severity, DiagSeverity::Warning) {
                    "warning".to_string()
                } else {
                    "error".to_string()
                },
                message: SarifMessage {
                    text: record.message.clone(),
                },
                locations: Some(vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation {
                            uri: primary.span.path,
                        },
                        region: SarifRegion {
                            start_line,
                            start_column,
                            char_offset: primary.span.offset,
                            char_length: primary.span.len,
                        },
                    },
                }]),
            }
        })
        .collect::<Vec<_>>();
    let log = SarifLog {
        schema: "https://json.schemastore.org/sarif-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "wrela".to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                },
            },
            results,
            properties: Some(SarifRunProperties {
                diagnostic_schema_version: DIAGNOSTIC_SCHEMA_VERSION,
            }),
        }],
    };
    println!(
        "{}",
        serde_json::to_string(&log).unwrap_or_else(|_| "{}".to_string())
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrela::diag::DiagStage;

    #[test]
    fn json_diag_surfaces_machine_fields_from_data() {
        let record = DiagRecord::new(
            DiagStage::Parse,
            DiagSeverity::Error,
            "render contract invalid".to_string(),
            "src/main.wr".to_string(),
            SourceSpan::from((4usize, 2usize)),
        )
        .with_data(Some(serde_json::json!({
            "preset_id": "strict-2d",
            "override_tier": 1,
            "expansion_node_id": "node-17"
        })));

        let value =
            serde_json::to_value(json_diag_for_record(&record)).expect("serialize json diag");
        assert_eq!(
            value.get("preset_id").and_then(|v| v.as_str()),
            Some("strict-2d")
        );
        assert_eq!(value.get("override_tier").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            value.get("expansion_node_id").and_then(|v| v.as_str()),
            Some("node-17")
        );
    }

    #[test]
    fn json_diag_infers_override_tier_from_message() {
        let record = DiagRecord::new(
            DiagStage::Parse,
            DiagSeverity::Error,
            "overrides block is missing required tier `2`".to_string(),
            "src/main.wr".to_string(),
            SourceSpan::from((0usize, 1usize)),
        );

        let value =
            serde_json::to_value(json_diag_for_record(&record)).expect("serialize json diag");
        assert_eq!(value.get("override_tier").and_then(|v| v.as_u64()), Some(2));
    }

    #[test]
    fn help_text_excludes_removed_game_surface() {
        let help = help_text();
        for removed in [
            "game <subcommand> <path>",
            "realtime <subcommand> <path>",
            "mmo <subcommand> <path>",
            "frontend <subcommand> <path>",
            "studio <subcommand> <path>",
            "agent-run <path>",
            "--render=NAME",
            "--host=NAME",
            "--client-runtime=MODE",
            "--shader-provenance",
            "--no-shortcuts",
            "--intent-v2",
            "--determinism",
            "--rollback",
            "--render-lane",
            "--asset-streaming",
            "--gpu-metrics",
            "--streaming-metrics",
        ] {
            assert!(
                !help.contains(removed),
                "removed help text leaked back into the CLI surface: {removed}"
            );
        }
    }

    #[test]
    fn help_text_mentions_surviving_surface() {
        let help = help_text();
        assert!(help.contains("init [path]"));
        assert!(help.contains("--holes-only"));
        assert!(help.contains("--allow-review-fixes"));
        assert!(help.contains("--query-backend=NAME"));
        assert!(help.contains("--strict-naming"));
        assert!(help.contains("--why-not-120"));
        assert!(help.contains("collision-contracts"));
        assert!(help.contains("collision-plan"));
        assert!(help.contains("collision-run"));
    }
}
