#![allow(unused_imports)]

//! Owns CLI command dispatch, command-local orchestration helpers, and the
//! typed tooling/report surfaces used by sibling command modules.
//! Does not own CLI tokenization or parse-time command legality checks; those
//! stay in [`super::super::cli_args`].
//!
//! Key invariants:
//! - dispatch matches on typed command variants, never free-form command names.
//! - command-local helper types stay private to this tree unless another module
//!   needs them as a stable tooling/report seam.
//! - human and machine-readable report rendering must describe the command path
//!   that actually executed, not the path the parser initially considered.
//!
//! Primary entrypoints:
//! - `command_dispatch::execute`
//! - `run_repro_artifact`
//!
//! Failure modes / common pitfalls:
//! - let new command-only helper structs leak into unrelated modules and the CLI
//!   tree turns back into a grab bag.
//! - skip parse-time validation and dispatch has to recover command legality
//!   with ad hoc checks again.

use super::super::cli_args::*;
use super::super::contracts::{
    EXIT_CODEGEN, EXIT_OK, EXIT_PARSE, EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, OutputFormat,
};
use super::super::{cert_engine, diag_emit, perf_engine, replay_trace};
use miette::SourceSpan;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela::diag::catalog::{mir_descriptor, project_descriptor};
use wrela::diag::suppress::suppress_cascades;
use wrela::diag::{DiagFix, DiagRecord, DiagSeverity, DiagSpan, DiagStage, dedupe_records};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::mir;
use wrela::parser;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;

pub(crate) mod build_compile;
pub(crate) mod check_analyze;
mod collision_command;
mod command_dispatch;
mod contracts_command;
pub(crate) mod fix_fmt;
mod frame_live;
mod live_command;
mod naming_policy;
mod observer_projection;
pub(crate) mod presentation_command;
mod presentation_reports;
mod preview_eval;
pub(crate) mod run_dev;
pub(crate) mod test_eval_perf;

#[path = "../../../query_program_debug/mod.rs"]
mod query_program_debug;
#[path = "../repro.rs"]
pub(crate) mod repro;

use build_compile::{
    BudgetPolicyV1, BuildPerfTimings, TestTarget, certification_cache_hash, emit_build_perf_event,
    emit_certification_cache_hit, enforce_importable_coverage_gate, enforce_public_surface_gate,
    evaluate_connector_contract_gate, fnv1a64_hex, hash_source_fingerprint, init_project,
    init_project_with_template, load_benchmark_manifest, project_record,
    query_contract_catalog_snapshot, resolve_benchmark_manifest_path, resolve_budget_policy_v1,
    resolve_certification_test_selection, resolve_path_from_owner_spans, resolve_test_target,
    resolve_toolchain_version, update_public_surface_baseline, verify_certification_report,
    write_certification_report, write_test_maintenance_summary,
};
use check_analyze::{
    compile_to_mir, integration_mode_entry_path_is_allowed, project_root_for_entry,
    resolve_entry_path, temp_exe_path,
};
pub(crate) use command_dispatch::execute;
use fix_fmt::{
    FixSummary, FmtSummary, apply_source_fixes, collect_safe_fixes, emit_fix_summary,
    emit_fmt_summary, resolve_format_targets, run_format_loop,
};
use naming_policy::{naming_policy_severity, naming_policy_tier, project_naming_diagnostics};
use presentation_command::{
    WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV,
    WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV,
};
use run_dev::{find_src_root, run_dev_loop, update_toolchain};
use test_eval_perf::{
    BenchmarkCollisionSpec, BenchmarkManifest, BenchmarkPresentationSpec, BenchmarkScenario,
    CertPerfTimings, CheckLaneKpis, CollisionBenchmarkExecutionReport, CollisionBenchmarkReport,
    DiagnosticScope, DifferentialPipeline, EvalCommandInput, HttpCassetteMode, KpiThresholds,
    PerfCmpConfig, PerfCv, PerfGateConfig, PerfProfile, PerfReport, PerfSummary,
    PresentationBenchmarkComparison, PresentationBenchmarkReport,
    PresentationWgslWorkgroupComparison, RunOnceTimings, TEST_JSON_SUMMARY_SEED, TestExecution,
    TestHarness, TestHarnessMeta, TestJsonCase, TestJsonSummary, TestJsonTimings, TestLane,
    TestLanePreset, TestLaneSelection, TestSelection, WholeFrameBenchmarkReport,
    aggregate_perf_samples, autogen_boundary_literal, autogen_check_decl_from_function,
    budget_jobs_timeout, build_benchmark_selection, build_function_test_coverage_index,
    canonicalize_function_coverage, certification_coverage_index_path, coefficient_of_variation,
    collect_tests, compute_cv, discover_tests_for_target, emit_perf_summary,
    emit_test_json_summary, enforce_serial_test_cap, evaluate_perf_gate, execute_eval_command,
    first_signature_mismatch_detail, infer_test_lane, list_tests, load_function_coverage_snapshot,
    load_perf_baseline_summary, module_path_for_single_file, overlay_perf_summary_runtime_cases,
    qualified_function_identity, run_mutation_gate, run_tests_once, select_tests,
    set_test_selection_include_ids, stable_function_id, stable_test_id, summarize_run_lane,
    summarize_run_lane_from_json_cases, test_selection_has_filters,
    write_function_coverage_snapshot, write_function_test_coverage_index,
};

pub(crate) fn run_repro_artifact(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
) -> i32 {
    repro::run_repro_artifact(
        workspace_root,
        repro_artifact_path,
        timeout,
        output_format,
        http_mode,
        budget_policy,
    )
}
