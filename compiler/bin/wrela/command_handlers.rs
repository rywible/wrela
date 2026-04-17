#![allow(unused_assignments)]
#![allow(unused_imports)]

use super::cli_args::{CommandSpec, ParsedCommandSpec};
use super::contracts::{
    EXIT_CODEGEN, EXIT_OK, EXIT_PARSE, EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, OutputFormat,
};
use super::{cert_engine, diag_emit, perf_engine, replay_trace};
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

#[path = "commands/build_compile.rs"]
mod build_compile;
#[path = "commands/check_analyze.rs"]
mod check_analyze;
#[path = "commands/fix_fmt.rs"]
mod fix_fmt;
#[path = "commands/run_dev.rs"]
mod run_dev;
#[path = "commands/shared.rs"]
mod shared;
#[path = "commands/test_eval_perf.rs"]
mod test_eval_perf;

pub(crate) use build_compile::{
    BudgetPolicyV1, TestTarget, evaluate_connector_contract_gate, fnv1a64_hex,
    load_benchmark_manifest, resolve_budget_policy_v1, resolve_test_target,
    update_public_surface_baseline, write_test_maintenance_summary,
};
pub(crate) use shared::{
    WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV,
    WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV, execute, run_repro_artifact,
};
pub(crate) use test_eval_perf::{
    BenchmarkCollisionSpec, BenchmarkManifest, BenchmarkPresentationSpec, BenchmarkScenario,
    CertPerfTimings, CollisionBenchmarkExecutionReport, CollisionBenchmarkReport,
    DifferentialPipeline, HttpCassetteMode, KpiThresholds, PerfCmpConfig, PerfGateConfig,
    PerfProfile, PerfReport, PerfSummary, PresentationBenchmarkComparison,
    PresentationBenchmarkReport, PresentationWgslWorkgroupComparison, RunOnceTimings,
    TEST_JSON_SUMMARY_SEED, TestExecution, TestSelection, WholeFrameBenchmarkReport,
    aggregate_perf_samples, budget_jobs_timeout, build_benchmark_selection, compute_cv,
    emit_perf_summary, evaluate_perf_gate, first_signature_mismatch_detail,
    load_perf_baseline_summary, overlay_perf_summary_runtime_cases, run_mutation_gate,
    run_tests_once, set_test_selection_include_ids, test_selection_has_filters,
};
