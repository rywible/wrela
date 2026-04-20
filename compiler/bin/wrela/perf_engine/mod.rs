//! Owns perf-oriented CLI orchestration: benchmark collection, closure verdicts,
//! comparison reports, and matrix/dev-loop evidence assembly.
//! Does not own benchmark scenario parsing at the CLI boundary, domain runtime
//! execution, or the semantics of the underlying compiler backends.
//!
//! Key invariants:
//! - reports and closure verdicts must reflect the scenario and lane that
//!   actually ran.
//! - baseline overlays may enrich reports, but they must not overwrite observed
//!   runtime truth with placeholders.
//! - typed scenario/lane identities are the internal protocol; string parsing is
//!   confined to the command boundary.
//!
//! Primary entrypoints:
//! - `execute_perf_command`
//! - `execute_perfcmp_command`
//! - `execute_matrix_command`
//!
//! Failure modes / common pitfalls:
//! - mixing human-readable labels with internal identity values makes closure
//!   and comparison reports drift.
//! - treating missing report data as "not applicable" instead of a violated or
//!   unknown lane can make the maintenance closure story dishonest.

use super::command_handlers::{build_compile, presentation_command, test_eval_perf};
use super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE, OutputFormat};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela::acceleration::report::explain_why_not_120_findings as explain_acceleration_why_not_120_findings;
use wrela::gpu_runtime::{GpuLimitRequest, shared_wgpu_context};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::perf_target::{
    PerfClosureExecutionStory, PerfClosureFinding, PerfClosureLaneStatus,
    PerfClosureLaneStatusReport, PerfClosureProfile, PerfClosureReport, PerfClosureVerdict,
    PerfClosureVerdictStatus, quality_degradation_step_name,
};
use wrela::presentation_exec::cost::explain_why_not_120_findings as explain_frame_why_not_120_findings;
use wrela::query_exec::{
    QueryExecContext, QueryTraceSolverMode, WGSL_WORKGROUP_SIZE_OVERRIDE_ENV,
    stable_region_scene_capture_id,
};

use build_compile::{
    TestTarget, load_benchmark_manifest, resolve_budget_policy_v1, resolve_test_target,
};
use presentation_command::{
    WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV,
    WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV,
};
#[cfg(test)]
use test_eval_perf::build_benchmark_selection;
use test_eval_perf::{
    BenchmarkManifest, CollisionBenchmarkReport, DifferentialPipeline, EngineFrameBenchmarkReport,
    KpiThresholds, PerfCmpConfig, PerfGateConfig, PerfProfile, PerfReport,
    PresentationBenchmarkComparison, PresentationBenchmarkReport,
    PresentationWgslWorkgroupComparison, TestSelection, WholeFrameBenchmarkReport,
    budget_jobs_timeout,
};

mod closure;
mod collection;
mod matrix;
mod perfcmp;

#[cfg(test)]
mod tests;

use self::{closure::*, collection::*, perfcmp::*};

pub(crate) use self::collection::{PerfCommandInput, execute_perf_command};
pub(crate) use self::matrix::{MatrixCommandInput, execute_matrix_command};
pub(crate) use self::perfcmp::{PerfcmpCommandInput, execute_perfcmp_command};
