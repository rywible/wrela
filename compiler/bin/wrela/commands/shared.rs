use super::cli_args::{CommandSpec, ParsedCommandSpec};
use super::contracts::{
    EXIT_CODEGEN, EXIT_OK, EXIT_PARSE, EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, OutputFormat,
};
use super::{cert_engine, deploy, diag_emit, perf_engine, replay_trace};
use miette::SourceSpan;
use serde::{Deserialize, Serialize};
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
use wrela::mir;
use wrela::parser;
use wrela::shader_compiler::{
    ShaderBindingV1, ShaderEntryPointV1, ShaderProgramIRV1, ShaderStageV1,
    shader_program_fingerprint, validate_shader_program,
};
use wrela_agent_control::{
    AgentBudgetSpecV2, AgentExecutionContractSpecV2, AgentFidelityModeV2, AgentFidelitySpecV2,
    AgentPipelineV2, AgentPolicyProfileV2, AgentRunIntentV2, AgentRunSummaryV2,
    AgentToolContractV2, AgentVibeSpecV2, AssetFactoryIntentV1, build_variant_scoring_report,
    compile_plan_deterministic, default_seed_for_prompt,
};
use wrela_asset_pack::{
    AssetChunk, AssetPackManifestV3, AssetPartition, WorldChunk, WorldChunkManifestV2,
    WorldChunkPartition, validate_asset_pack, validate_world_manifest,
};
#[path = "frontend.rs"]
mod frontend;
#[path = "../repro.rs"]
mod repro;

fn naming_policy_tier(error: &hir::naming::NamingError) -> &'static str {
    match error {
        hir::naming::NamingError::ResultPrefixRequired { .. }
        | hir::naming::NamingError::FactoryPrefixRequired { .. }
        | hir::naming::NamingError::ResultErrorTypeShape { .. }
        | hir::naming::NamingError::TopLevelCheckName { .. }
        | hir::naming::NamingError::MemberCheckPrefix { .. } => "strong",
        hir::naming::NamingError::SnakeCaseRequired { .. }
        | hir::naming::NamingError::PascalCaseRequired { .. }
        | hir::naming::NamingError::VerbLedRequired { .. }
        | hir::naming::NamingError::NounOnlyRequired { .. }
        | hir::naming::NamingError::BooleanPrefixRequired { .. }
        | hir::naming::NamingError::InlineCheckCondition { .. }
        | hir::naming::NamingError::ModuleSemanticRequired { .. }
        | hir::naming::NamingError::CollectionPluralityRequired { .. } => "style",
    }
}

fn naming_policy_severity(error: &hir::naming::NamingError, strict_naming: bool) -> DiagSeverity {
    let tier = naming_policy_tier(error);
    if strict_naming && (tier == "strong" || tier == "style") {
        DiagSeverity::Error
    } else {
        DiagSeverity::Warning
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameBuildTarget {
    Native,
    Wasm,
    Dual,
}

fn parse_game_build_target(raw: Option<&str>) -> Result<GameBuildTarget, String> {
    match raw {
        None | Some("native") => Ok(GameBuildTarget::Native),
        Some("wasm") => Ok(GameBuildTarget::Wasm),
        Some("dual") => Ok(GameBuildTarget::Dual),
        Some(other) => Err(format!(
            "error: invalid `wrela game build --target` / `wrela realtime build --target` value `{other}` (expected one of: native, wasm, dual)"
        )),
    }
}

fn orchestration_contract_for_identity(
    identity: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match identity {
        "mmo.init" => Some((
            "wrela mmo init",
            "project-bootstrap",
            &[
                "src/main.wr",
                "src/application/realtime_bootstrap.wr",
                "assets/bootstrap.bin",
            ],
        )),
        "mmo.build" => Some((
            "wrela mmo build",
            "runtime-build",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
            ],
        )),
        "mmo.run" => Some((
            "wrela mmo run",
            "runtime-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.dev" => Some((
            "wrela mmo dev",
            "runtime-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.check" => Some((
            "wrela mmo check",
            "runtime-gate",
            &[
                "test-matrix.json",
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "mmo.profile" => Some((
            "wrela mmo profile",
            "runtime-profile",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.gateway" => Some((
            "wrela mmo gateway",
            "gateway-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.zone" => Some((
            "wrela mmo zone",
            "zone-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.world" => Some((
            "wrela mmo world",
            "world-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.loadtest" => Some((
            "wrela mmo loadtest",
            "loadtest-profile",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "mmo.ops" => Some((
            "wrela mmo ops",
            "ops-gate",
            &[
                "test-matrix.json",
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.init" => Some((
            "wrela studio init",
            "project-bootstrap",
            &[
                "src/main.wr",
                "src/application/realtime_bootstrap.wr",
                "assets/bootstrap.bin",
            ],
        )),
        "studio.build" => Some((
            "wrela studio build",
            "runtime-build",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
            ],
        )),
        "studio.check" => Some((
            "wrela studio check",
            "runtime-gate",
            &[
                "test-matrix.json",
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.run" => Some((
            "wrela studio run",
            "runtime-serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "studio.preview" => Some((
            "wrela studio preview",
            "frontend-preview",
            &["preview-report.json"],
        )),
        "studio.explain" => Some((
            "wrela studio explain",
            "frontend-explain",
            &["explain-report.json"],
        )),
        "studio.fix" => Some(("wrela studio fix", "frontend-fix", &["fix-report.json"])),
        "studio.synth" => Some((
            "wrela studio synth",
            "synth-agent-run",
            &[
                "intent.json",
                "plan.json",
                "execution-report.json",
                "summary.json",
                "orchestration-evidence.json",
            ],
        )),
        "studio.synth-assets" => Some((
            "wrela studio synth-assets",
            "asset-factory-synth",
            &[
                "intent.json",
                "plan.json",
                "variant-scoring-report.json",
                "execution-report.json",
                "summary.json",
                "orchestration-evidence.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.bake" => Some((
            "wrela studio bake",
            "native-bake",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
            ],
        )),
        "studio.bake-assets" => Some((
            "wrela studio bake-assets",
            "asset-factory-bake",
            &[
                "build-manifest.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.pack" => Some((
            "wrela studio pack",
            "wasm-pack",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
            ],
        )),
        "studio.package-assets" => Some((
            "wrela studio package-assets",
            "asset-factory-package",
            &[
                "build-manifest.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.gate" => Some((
            "wrela studio gate",
            "strict-gate",
            &[
                "test-matrix.json",
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.validate-assets" => Some((
            "wrela studio validate-assets",
            "asset-factory-validate",
            &[
                "test-matrix.json",
                "asset-quality-report-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-factory-manifest-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.ship" => Some((
            "wrela studio ship",
            "runtime-ship",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "studio.promote-assets" => Some((
            "wrela studio promote-assets",
            "asset-factory-promote",
            &[
                "build-manifest.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        "studio.full-factory" => Some((
            "wrela studio full-factory",
            "asset-factory-full",
            &[
                "intent.json",
                "plan.json",
                "variant-scoring-report.json",
                "execution-report.json",
                "summary.json",
                "orchestration-evidence.json",
                "full-factory-dag-report.json",
                "test-matrix.json",
                "build-manifest.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
                "asset-factory-promotion.json",
            ],
        )),
        _ => None,
    }
}

fn emit_orchestration_game_event(
    output_format: OutputFormat,
    identity: &str,
    mapped_command: &str,
    app_path: Option<&str>,
    status: &str,
    error: Option<&str>,
) {
    let Some((invocation, evidence_contract, required_outputs)) =
        orchestration_contract_for_identity(identity)
    else {
        return;
    };
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    println!(
        "{}",
        serde_json::json!({
            "event": "orchestration_summary",
            "payload": {
                "summary": {
                    "identity": identity,
                    "invocation": invocation,
                    "mapped_command": mapped_command,
                    "status": status,
                },
                "evidence": {
                    "contract": evidence_contract,
                    "required_outputs": required_outputs,
                    "app_path": app_path,
                    "mmo_role_evidence": mmo_orchestration_role_evidence(identity),
                },
                "error": error,
            }
        })
    );
}

fn mmo_role_contract_for_variant(variant: &str) -> Option<(&'static str, &'static [&'static str])> {
    match variant {
        "gateway" | "zone" | "world" => Some((
            "serve",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "loadtest" => Some((
            "profile",
            &[
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
            ],
        )),
        "ops" => Some((
            "gate",
            &[
                "test-matrix.json",
                "build-manifest.json",
                "render-manifest.json",
                "shader-bundle.json",
                "assets-manifest.json",
                "world-chunks.json",
                "asset-factory-manifest-v2.json",
                "asset-provenance-ledger-v1.json",
                "asset-quality-report-v2.json",
                "ui-atlas-manifest-v1.json",
                "character-bundle-manifest-v2.json",
                "animation-rig-catalog-v1.json",
                "animation-clip-bundle-v1.json",
                "animation-graph-contract-v1.json",
                "flora-sim-contract-v1.json",
                "animation-quality-report-v1.json",
            ],
        )),
        _ => None,
    }
}

fn mmo_orchestration_role_evidence(identity: &str) -> Option<serde_json::Value> {
    let (_, role) = identity.split_once('.')?;
    let (phase, required_outputs) = mmo_role_contract_for_variant(role)?;
    Some(serde_json::json!({
        "role": role,
        "phase": phase,
        "required_outputs": required_outputs,
    }))
}

pub fn execute(spec: CommandSpec) {
    let trace = spec.trace_enabled;
    if trace {
        eprintln!("build: cli start");
    }
    let parsed = match spec.parsed {
        ParsedCommandSpec::Help => {
            diag_emit::print_help();
            return;
        }
        ParsedCommandSpec::Version => {
            println!("wrela {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        ParsedCommandSpec::Error(err) => {
            if err == "__print_help__" {
                diag_emit::print_help();
            } else {
                eprintln!("{err}");
            }
            std::process::exit(EXIT_USAGE);
        }
        ParsedCommandSpec::Ready(parsed) => parsed,
    };
    let output_format = if parsed.output_format_sarif {
        OutputFormat::Sarif
    } else if parsed.output_format_json {
        OutputFormat::Json
    } else if parsed.output_format_human {
        OutputFormat::Pretty
    } else {
        OutputFormat::Pretty
    };
    let emit_mir = parsed.emit_mir;
    let emit_mir_opt = parsed.emit_mir_opt;
    let emit_obj = parsed.emit_obj;
    let emit_bin = parsed.emit_bin;
    let out_path = parsed.out_path;
    let prefix_path = parsed.prefix_path;
    let command = parsed.command;
    let game_mode = parsed.game_mode;
    let game_build_target = parsed.game_build_target;
    let game_render_backend = parsed.game_render_backend;
    let game_host_mode = parsed.game_host_mode;
    let game_client_runtime = parsed.game_client_runtime;
    let game_shader_provenance = parsed.game_shader_provenance;
    let game_no_shortcuts = parsed.game_no_shortcuts;
    let game_check_determinism = parsed.game_check_determinism;
    let game_check_rollback = parsed.game_check_rollback;
    let game_check_render_lane = parsed.game_check_render_lane;
    let game_check_asset_streaming = parsed.game_check_asset_streaming;
    let game_profile_gpu_metrics = parsed.game_profile_gpu_metrics;
    let game_profile_streaming_metrics = parsed.game_profile_streaming_metrics;
    let integration_mode = parsed.integration_mode;
    let path_arg = parsed.path_arg;
    let program_args = parsed.program_args;
    let poll_ms = parsed.poll_ms;
    let test_jobs = parsed.test_jobs;
    let test_timeout_ms = parsed.test_timeout_ms;
    let test_record = parsed.test_record;
    let test_update_public_surface = parsed.test_update_public_surface;
    let test_list = parsed.test_list;
    let test_id = parsed.test_id;
    let test_filter = parsed.test_filter;
    let test_lane = parsed.test_lane;
    let test_seed = parsed.test_seed;
    let repro_artifact_path = parsed.repro_artifact_path;
    let replay_trace_path = parsed.replay_trace_path;
    let perf_debug = parsed.perf_debug;
    let perf_runs = parsed.perf_runs;
    let perf_baseline_out = parsed.perf_baseline_out;
    let perf_gate_path = parsed.perf_gate_path;
    let perf_max_regression_pct = parsed.perf_max_regression_pct;
    let perf_cv_max_pct = parsed.perf_cv_max_pct;
    let kpi_check_fallback_max = parsed.kpi_check_fallback_max;
    let kpi_check_batch_min = parsed.kpi_check_batch_min;
    let kpi_scheduler_p99_improve_min_pct = parsed.kpi_scheduler_p99_improve_min_pct;
    let kpi_rewrite_overhead_max_pct = parsed.kpi_rewrite_overhead_max_pct;
    let kpi_actor_throughput_improve_min_pct = parsed.kpi_actor_throughput_improve_min_pct;
    let kpi_queue_age_p99_max_regress_pct = parsed.kpi_queue_age_p99_max_regress_pct;
    let kpi_starvation_violations_max = parsed.kpi_starvation_violations_max;
    let kpi_scheduler_throughput_improve_min_pct = parsed.kpi_scheduler_throughput_improve_min_pct;
    let kpi_scheduler_loop_p99_max_regress_pct = parsed.kpi_scheduler_loop_p99_max_regress_pct;
    let kpi_scheduler_local_hit_min = parsed.kpi_scheduler_local_hit_min;
    let benchmark_manifest_path = parsed.benchmark_manifest_path;
    let perf_profile_name = parsed.perf_profile_name;
    let perfcmp_baseline_ref = parsed.perfcmp_baseline_ref;
    let perfcmp_candidate_ref = parsed.perfcmp_candidate_ref;
    let perfcmp_warmup_pairs = parsed.perfcmp_warmup_pairs;
    let perfcmp_measure_pairs = parsed.perfcmp_measure_pairs;
    let perfcmp_min_effect_pct = parsed.perfcmp_min_effect_pct;
    let perfcmp_confidence_pct = parsed.perfcmp_confidence_pct;
    let deploy_target = parsed.deploy_target;
    let deploy_app = parsed.deploy_app;
    let deploy_region = parsed.deploy_region;
    let deploy_machines = parsed.deploy_machines;
    let deploy_policy = parsed.deploy_policy;
    let deploy_replication_factor = parsed.deploy_replication_factor;
    let deploy_write_quorum = parsed.deploy_write_quorum;
    let deploy_logical_shards = parsed.deploy_logical_shards;
    let deploy_active_groups = parsed.deploy_active_groups;
    let deploy_force = parsed.deploy_force;
    let deploy_generate_only = parsed.deploy_generate_only;
    let analysis_holes_only = parsed.analysis_holes_only;
    let strict_naming = parsed.strict_naming;
    let fix_allow_review_fixes = parsed.fix_allow_review_fixes;
    let workspace_diagnostics = parsed.workspace_diagnostics;
    let orchestration_identity = parsed.orchestration_identity;
    let agent_run_intent_v2 = parsed.agent_run_intent_v2;
    let preview_port = parsed.preview_port;
    let preview_open = parsed.preview_open;
    let resolve_dry_run = parsed.resolve_dry_run;
    let resolve_force = parsed.resolve_force;
    let resolve_parallel = parsed.resolve_parallel;

    let command = command.as_str();
    let kpi_thresholds = KpiThresholds {
        check_fallback_max: kpi_check_fallback_max,
        check_batch_min: kpi_check_batch_min,
        scheduler_p99_improve_min_pct: kpi_scheduler_p99_improve_min_pct,
        rewrite_overhead_max_pct: kpi_rewrite_overhead_max_pct,
        actor_throughput_improve_min_pct: kpi_actor_throughput_improve_min_pct,
        queue_age_p99_max_regress_pct: kpi_queue_age_p99_max_regress_pct,
        starvation_violations_max: kpi_starvation_violations_max,
        scheduler_throughput_improve_min_pct: kpi_scheduler_throughput_improve_min_pct,
        scheduler_loop_p99_max_regress_pct: kpi_scheduler_loop_p99_max_regress_pct,
        scheduler_local_hit_min: kpi_scheduler_local_hit_min,
    };
    if command != "test" && (test_record || test_update_public_surface) {
        eprintln!("error: --record and --update-public-surface are only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "run" && command != "build" && command != "compile" && integration_mode {
        eprintln!("error: --integration-mode is only valid with `wrela run` or `wrela build`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && test_list {
        eprintln!("error: --list is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && (test_id.is_some() || test_filter.is_some()) {
        eprintln!("error: --id and --filter are only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && test_lane.is_some() {
        eprintln!("error: --lane is only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && test_seed.is_some() {
        eprintln!("error: --seed is only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && repro_artifact_path.is_some() {
        eprintln!("error: --repro is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && replay_trace_path.is_some() {
        eprintln!("error: --replay-trace is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perf" && command != "perfcmp" && benchmark_manifest_path.is_some() {
        eprintln!("error: --benchmark-manifest is only valid with `wrela perf` or `wrela perfcmp`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perf" && command != "perfcmp" && perf_profile_name.is_some() {
        eprintln!("error: --profile is only valid with `wrela perf` or `wrela perfcmp`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perfcmp"
        && (perfcmp_baseline_ref.is_some()
            || perfcmp_candidate_ref.is_some()
            || perfcmp_warmup_pairs.is_some()
            || perfcmp_measure_pairs.is_some()
            || perfcmp_min_effect_pct.is_some()
            || perfcmp_confidence_pct.is_some())
    {
        eprintln!(
            "error: --baseline-ref, --candidate-ref, --warmup-pairs, --measure-pairs, --min-effect-pct, and --confidence are only valid with `wrela perfcmp`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if command != "deploy"
        && (deploy_target.is_some()
            || deploy_app.is_some()
            || deploy_region.is_some()
            || deploy_machines.is_some()
            || deploy_policy.is_some()
            || deploy_replication_factor.is_some()
            || deploy_write_quorum.is_some()
            || deploy_logical_shards.is_some()
            || deploy_active_groups.is_some()
            || deploy_force
            || deploy_generate_only)
    {
        eprintln!(
            "error: --target, --app, --region, --machines, --deploy-policy, --rf, --wq, --logical-shards, --active-groups, --force, and --generate-only are only valid with `wrela deploy`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if command != "check" && command != "analyze" && analysis_holes_only {
        eprintln!("error: --holes-only is only valid with `wrela check` or `wrela analyze`");
        std::process::exit(EXIT_USAGE);
    }
    if strict_naming
        && command != "check"
        && command != "analyze"
        && command != "build"
        && command != "compile"
        && command != "run"
        && command != "dev"
    {
        eprintln!(
            "error: --strict-naming is only valid with `wrela check`, `wrela analyze`, `wrela build`, `wrela compile`, `wrela run`, or `wrela dev`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if command != "fix" && command != "fmt" && fix_allow_review_fixes {
        eprintln!("error: --allow-review-fixes is only valid with `wrela fix` or `wrela fmt`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "fix" && command != "fmt" && workspace_diagnostics {
        eprintln!("error: --workspace-diagnostics is only valid with `wrela fix` or `wrela fmt`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "agent-run" && agent_run_intent_v2 {
        eprintln!("error: --intent-v2 is only valid with `wrela agent-run`");
        std::process::exit(EXIT_USAGE);
    }
    if !(game_mode && command == "check")
        && (game_check_determinism
            || game_check_rollback
            || game_check_render_lane
            || game_check_asset_streaming)
    {
        eprintln!(
            "error: --determinism, --rollback, --render-lane, and --asset-streaming are only valid with `wrela game check`, `wrela realtime check`, `wrela mmo check`, `wrela studio gate`, or `wrela studio validate-assets`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if !(game_mode && command == "profile")
        && (game_profile_gpu_metrics || game_profile_streaming_metrics)
    {
        eprintln!(
            "error: --gpu-metrics and --streaming-metrics are only valid with `wrela game profile`, `wrela realtime profile`, or `wrela mmo profile`/`wrela mmo loadtest`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if (game_render_backend.is_some() || game_host_mode.is_some())
        && !(game_mode && matches!(command, "build" | "run" | "dev" | "check" | "profile"))
    {
        eprintln!(
            "error: --render and --host are only valid with `wrela game build`/`wrela realtime build`/`wrela mmo build`, `wrela game run`/`wrela realtime run`/`wrela mmo run`, `wrela game dev`/`wrela realtime dev`/`wrela mmo dev`, `wrela game check`/`wrela realtime check`/`wrela mmo check`/`wrela studio gate`/`wrela studio validate-assets`, or `wrela game profile`/`wrela realtime profile`/`wrela mmo profile`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if game_client_runtime
        .as_deref()
        .is_some_and(|mode| mode != "compiled")
    {
        let mode = game_client_runtime.as_deref().unwrap_or_default();
        eprintln!("error: invalid --client-runtime value `{mode}` (expected: compiled)");
        std::process::exit(EXIT_USAGE);
    }
    if !(game_mode && matches!(command, "build" | "check" | "run" | "dev"))
        && (game_client_runtime.is_some() || game_shader_provenance || game_no_shortcuts)
    {
        eprintln!(
            "error: --client-runtime=compiled, --shader-provenance, and --no-shortcuts are only valid with `wrela game build`/`wrela realtime build`/`wrela mmo build`, `wrela game check`/`wrela realtime check`/`wrela mmo check`/`wrela studio gate`/`wrela studio validate-assets`, `wrela game run`/`wrela realtime run`/`wrela mmo run`, or `wrela game dev`/`wrela realtime dev`/`wrela mmo dev`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if game_mode
        && matches!(command, "build" | "check")
        && (game_client_runtime.as_deref() != Some("compiled")
            || !game_shader_provenance
            || !game_no_shortcuts)
    {
        eprintln!(
            "error: `wrela game {command}`/`wrela realtime {command}`/`wrela mmo {command}` requires --client-runtime=compiled --shader-provenance --no-shortcuts (also applied by `wrela studio gate`/`wrela studio validate-assets`)"
        );
        std::process::exit(EXIT_USAGE);
    }
    let parsed_test_lane = if let Some(raw_lane) = test_lane.as_deref() {
        match parse_test_lane_filter(raw_lane) {
            Some(lane) => Some(lane),
            None => {
                eprintln!(
                    "error: invalid --lane value `{raw_lane}` (expected one of spec|integration|sim|model|default)"
                );
                std::process::exit(EXIT_USAGE);
            }
        }
    } else {
        None
    };
    let test_selection = TestSelection {
        list: test_list,
        id: test_id,
        filter: test_filter,
        lane: parsed_test_lane,
        include_ids: None,
        cert_selection_report: None,
    };
    let perf_profile = match PerfProfile::parse(perf_profile_name.as_deref().unwrap_or("standard"))
    {
        Some(profile) => profile,
        None => {
            eprintln!("error: invalid --profile value (expected smoke|standard|deep)");
            std::process::exit(EXIT_USAGE);
        }
    };

    if game_mode {
        let orchestration_path_for_metadata = path_arg.clone();
        if let Some(identity) = orchestration_identity.as_deref() {
            emit_orchestration_game_event(
                output_format,
                identity,
                command,
                orchestration_path_for_metadata.as_deref(),
                "dispatch",
                None,
            );
        }
        let command_input = GameCommandInput {
            command: command.to_string(),
            path_arg,
            out_path,
            game_build_target,
            game_render_backend,
            game_host_mode,
            game_client_runtime,
            game_shader_provenance,
            game_no_shortcuts,
            game_check_determinism,
            game_check_rollback,
            game_check_render_lane,
            game_check_asset_streaming,
            game_profile_gpu_metrics,
            game_profile_streaming_metrics,
        };
        if let Err(error) = execute_game_command_with_orchestration(
            command_input,
            output_format,
            orchestration_identity.as_deref(),
        ) {
            if let Some(identity) = orchestration_identity.as_deref() {
                emit_orchestration_game_event(
                    output_format,
                    identity,
                    command,
                    orchestration_path_for_metadata.as_deref(),
                    "failed",
                    Some(error.as_str()),
                );
            }
            eprintln!("{error}");
            std::process::exit(EXIT_CODEGEN);
        }
        if let Some(identity) = orchestration_identity.as_deref() {
            emit_orchestration_game_event(
                output_format,
                identity,
                command,
                orchestration_path_for_metadata.as_deref(),
                "passed",
                None,
            );
        }
        return;
    }

    match command {
        "init" => {
            if trace {
                eprintln!("build: command init");
            }
            let target = path_arg.as_deref().unwrap_or(".");
            if let Err(err) = init_project(target) {
                eprintln!("init error: {err}");
                std::process::exit(EXIT_USAGE);
            }
        }
        "update" => {
            if trace {
                eprintln!("build: command update");
            }
            if path_arg.is_some() {
                eprintln!("error: update does not take a path");
                std::process::exit(EXIT_USAGE);
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            if let Err(err) = update_toolchain(prefix_path.as_deref()) {
                eprintln!("update error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
        }
        "agent-run" => {
            if !agent_run_intent_v2 {
                eprintln!("error: `wrela agent-run` requires --intent-v2 (hard cutover)");
                std::process::exit(EXIT_USAGE);
            }
            let exit = execute_agent_run_command(
                path_arg,
                program_args,
                output_format,
                orchestration_identity,
            );
            std::process::exit(exit);
        }
        "check" | "analyze" => {
            if trace {
                eprintln!("build: command check");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let result = compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                false,
                true,
                strict_naming,
                analysis_holes_only,
            );
            if let Err(code) = result {
                std::process::exit(code);
            }
        }
        "fix" => {
            if trace {
                eprintln!("build: command fix");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            const MAX_PASSES: usize = 12;
            let mut attempted = 0usize;
            let mut applied = 0usize;
            let mut touched_paths: BTreeSet<String> = BTreeSet::new();
            let mut any_fix_candidates = false;
            let mut had_apply_error = false;
            let diagnostic_scope =
                DiagnosticScope::from_entrypoint(&entry_path, workspace_diagnostics);

            for _ in 0..MAX_PASSES {
                let fixes = match collect_safe_fixes(
                    &entry_path,
                    output_format,
                    fix_allow_review_fixes,
                    &diagnostic_scope,
                ) {
                    Ok(fixes) => fixes,
                    Err(code) => {
                        if applied > 0 {
                            break;
                        }
                        std::process::exit(code);
                    }
                };
                if fixes.is_empty() {
                    break;
                }
                any_fix_candidates = true;
                attempted = attempted.saturating_add(fixes.len());
                for fix in &fixes {
                    touched_paths.insert(fix.span.path.clone());
                }
                match apply_source_fixes(&fixes) {
                    Ok(report) => {
                        applied = applied.saturating_add(report.applied);
                        if report.applied == 0 {
                            break;
                        }
                    }
                    Err(err) => {
                        applied = applied.saturating_add(err.applied);
                        had_apply_error = true;
                        eprintln!("fix apply error: {}", err.message);
                        break;
                    }
                }
            }

            let summary = FixSummary {
                attempted,
                applied,
                skipped: attempted.saturating_sub(applied),
                errors: if had_apply_error { 1 } else { 0 },
                touched_files: touched_paths.len(),
            };
            emit_fix_summary(output_format, summary);

            if had_apply_error {
                std::process::exit(EXIT_CODEGEN);
            }
            if !any_fix_candidates || applied == 0 {
                eprintln!("fix: no safe non-overlapping fixes found");
                std::process::exit(EXIT_TYPE);
            }
            eprintln!("fix: applied {} safe fix(es)", applied);
        }
        "fmt" => {
            if trace {
                eprintln!("build: command fmt");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let format_targets = match resolve_format_targets(path_arg.as_deref()) {
                Ok(targets) => targets,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let mut summary = FmtSummary::default();
            summary.targets_scanned = format_targets.len();
            let mut fmt_exit_code: Option<i32> = None;
            for target in &format_targets {
                match run_format_loop(
                    target,
                    output_format,
                    fix_allow_review_fixes,
                    workspace_diagnostics,
                ) {
                    Ok(target_summary) => {
                        summary.iterations =
                            summary.iterations.saturating_add(target_summary.iterations);
                        summary.attempted =
                            summary.attempted.saturating_add(target_summary.attempted);
                        summary.applied = summary.applied.saturating_add(target_summary.applied);
                        summary.touched_files = summary
                            .touched_files
                            .saturating_add(target_summary.touched_files);
                    }
                    Err(code) => {
                        summary.failed_targets = summary.failed_targets.saturating_add(1);
                        if fmt_exit_code.is_none() {
                            fmt_exit_code = Some(code);
                        }
                    }
                }
            }
            emit_fmt_summary(output_format, summary);
            if summary.failed_targets > 0 {
                eprintln!(
                    "fmt: {} target(s) failed during sweep",
                    summary.failed_targets
                );
            } else if summary.applied == 0 {
                eprintln!("fmt: already canonical");
            } else {
                eprintln!(
                    "fmt: applied {} rewrite(s) across {} file(s) in {} pass(es)",
                    summary.applied, summary.touched_files, summary.iterations
                );
            }
            if let Some(code) = fmt_exit_code {
                std::process::exit(code);
            }
        }
        "build" | "compile" => {
            if trace {
                eprintln!("build: command build");
            }
            let build_start = Instant::now();
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            if trace {
                eprintln!("build: resolved entry {}", entry_path.display());
            }
            if find_src_root(&entry_path).is_none() {
                eprintln!(
                    "error: `wrela build` requires project layout (`src/**`) because single-file mode bypasses architecture checks"
                );
                eprintln!(
                    "help: move entrypoint to `src/main.wr` and run `wrela build <project-or-src/main.wr>`"
                );
                std::process::exit(EXIT_USAGE);
            }
            if trace {
                eprintln!("build: source root verified");
            }
            let workspace_root = project_root_for_entry(&entry_path);
            if trace {
                eprintln!("build: workspace root {}", workspace_root.display());
            }
            let budget_policy = resolve_budget_policy_v1(test_jobs, test_timeout_ms);
            let jobs = budget_policy.test_jobs.value as usize;
            let timeout = Duration::from_millis(budget_policy.test_timeout_ms.value);
            if trace {
                eprintln!(
                    "build: budget resolved jobs={} timeout_ms={}",
                    jobs,
                    timeout.as_millis()
                );
                eprintln!("build: collecting coverage id aliases");
            }
            if integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks and certification gates for integration-facing executables"
                );
                let mir_compile_start = Instant::now();
                let mir_module = match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                };
                let mir_compile_ms = mir_compile_start.elapsed().as_millis();
                if let Some(path) = emit_obj {
                    match wrela::backend::cranelift::compile_to_object(&mir_module) {
                        Ok(obj) => {
                            if let Err(err) = fs::write(&path, obj) {
                                eprintln!("failed to write object: {err}");
                                std::process::exit(EXIT_CODEGEN);
                            }
                        }
                        Err(err) => {
                            eprintln!("codegen error: {}", err.0);
                            std::process::exit(EXIT_CODEGEN);
                        }
                    }
                }
                let output_path = out_path
                    .or(emit_bin)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| workspace_root.join("wrela.out"));
                let output = output_path.to_string_lossy().to_string();
                let codegen_start = Instant::now();
                if let Err(err) =
                    wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
                {
                    eprintln!("codegen error: {}", err.0);
                    std::process::exit(EXIT_CODEGEN);
                }
                let codegen_ms = codegen_start.elapsed().as_millis();
                emit_build_perf_event(
                    output_format,
                    true,
                    "integration-mode-skip-cert".to_string(),
                    "integration-mode-skip-cert".to_string(),
                    BuildPerfTimings {
                        certification_ms: 0,
                        cert_collect_tests_ms: 0,
                        cert_compile_harness_ms: 0,
                        cert_determinism_ms: 0,
                        cert_mutation_discovery_ms: 0,
                        cert_mutation_execution_ms: 0,
                        cert_diff_ms: 0,
                        mir_compile_ms,
                        codegen_ms,
                        cert_report_ms: 0,
                        total_ms: build_start.elapsed().as_millis(),
                    },
                );
                return;
            }
            let toolchain_version = resolve_toolchain_version();
            if trace {
                eprintln!("build: toolchain version {}", toolchain_version);
                eprintln!("build: hashing source fingerprint");
            }
            let source_hash = match hash_source_fingerprint(&workspace_root) {
                Ok(hash) => hash,
                Err(err) => {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            if trace {
                eprintln!("build: source fingerprint hash={source_hash}");
            }
            let cert_cache_hash = certification_cache_hash(&source_hash, &toolchain_version);
            let cert_cache_dir = workspace_root
                .join("target")
                .join("wrela_cert")
                .join(&cert_cache_hash);
            let cert_report_path = cert_cache_dir.join("cert.json");
            let function_coverage_path = cert_cache_dir.join("function_coverage.json");
            let mut cert_cache_hit = cert_report_path.is_file() && function_coverage_path.is_file();
            let mut cert_cache_reason = if cert_cache_hit {
                "unchanged-certified-inputs".to_string()
            } else {
                "cache-miss-or-first-run".to_string()
            };
            let certification_start = Instant::now();
            let mut differential_results_hash: Option<String> = None;
            let mut mutation_summary_hash: Option<String> = None;
            let mut cert_timings = CertPerfTimings::default();
            let mut cached_coverage_snapshot = None;
            if cert_cache_hit {
                emit_certification_cache_hit(output_format, &cert_cache_hash, &cert_cache_dir);
                match load_function_coverage_snapshot(&function_coverage_path) {
                    Ok(snapshot) => cached_coverage_snapshot = Some(snapshot),
                    Err(err) => {
                        cert_cache_hit = false;
                        cert_cache_reason = "cache-schema-stale-recomputed".to_string();
                        eprintln!(
                            "certification cache stale; recomputing certification artifacts: {err}"
                        );
                    }
                }
            }
            let function_coverage = if let Some(snapshot) = cached_coverage_snapshot {
                snapshot
            } else {
                let cert_selection =
                    resolve_certification_test_selection(&workspace_root, output_format);
                let cert_result = cert_engine::run_tests(
                    &TestTarget::ProjectRoot(workspace_root.clone()),
                    &budget_policy,
                    jobs,
                    timeout,
                    output_format,
                    perf_debug,
                    None,
                    &cert_selection,
                    true,
                    HttpCassetteMode::Replay,
                    None,
                );
                if cert_result.exit != EXIT_OK {
                    eprintln!("build blocked: certification failed; no artifact emitted");
                    std::process::exit(cert_result.exit);
                }
                differential_results_hash = cert_result.differential_results_hash.clone();
                mutation_summary_hash = cert_result.mutation_summary_hash.clone();
                cert_timings = cert_result.cert_timings;
                let raw_snapshot = cert_result
                    .summary
                    .as_ref()
                    .map(|summary| summary.metrics.function_coverage.clone())
                    .unwrap_or_default();
                let snapshot = canonicalize_function_coverage(&raw_snapshot);
                if let Err(err) =
                    write_function_coverage_snapshot(&function_coverage_path, &snapshot)
                {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
                let coverage_index_path =
                    certification_coverage_index_path(&workspace_root, &cert_cache_hash);
                let coverage_index =
                    build_function_test_coverage_index(cert_result.summary.as_ref());
                if let Err(err) =
                    write_function_test_coverage_index(&coverage_index_path, &coverage_index)
                {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
                snapshot
            };
            let certification_ms = certification_start.elapsed().as_millis();
            if let Err(err) = enforce_importable_coverage_gate(&workspace_root, &function_coverage)
            {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            if let Err(err) = enforce_public_surface_gate(&workspace_root) {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            if integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks for integration-facing executables"
                );
            }
            let mir_compile_start = Instant::now();
            let mir_module = match compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                true,
                !integration_mode,
                strict_naming,
                false,
            ) {
                Ok(mir) => mir,
                Err(code) => std::process::exit(code),
            };
            let mir_compile_ms = mir_compile_start.elapsed().as_millis();
            if let Some(path) = emit_obj {
                match wrela::backend::cranelift::compile_to_object(&mir_module) {
                    Ok(obj) => {
                        if let Err(err) = fs::write(&path, obj) {
                            eprintln!("failed to write object: {err}");
                            std::process::exit(EXIT_CODEGEN);
                        }
                    }
                    Err(err) => {
                        eprintln!("codegen error: {}", err.0);
                        std::process::exit(EXIT_CODEGEN);
                    }
                }
            }
            let output_path = out_path
                .or(emit_bin)
                .map(PathBuf::from)
                .unwrap_or_else(|| workspace_root.join("wrela.out"));
            let output = output_path.to_string_lossy().to_string();
            let codegen_start = Instant::now();
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let codegen_ms = codegen_start.elapsed().as_millis();
            let artifact_path = output_path;
            let cert_report_start = Instant::now();
            if let Err(err) = write_certification_report(
                &entry_path,
                &workspace_root,
                &artifact_path,
                &budget_policy,
                &toolchain_version,
                &source_hash,
                &cert_cache_hash,
                differential_results_hash.as_deref(),
                mutation_summary_hash.as_deref(),
            ) {
                eprintln!("certification report error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
            let cert_report_ms = cert_report_start.elapsed().as_millis();
            let total_ms = build_start.elapsed().as_millis();
            emit_build_perf_event(
                output_format,
                cert_cache_hit,
                cert_cache_hash,
                cert_cache_reason,
                BuildPerfTimings {
                    certification_ms,
                    cert_collect_tests_ms: cert_timings.collect_tests_ms,
                    cert_compile_harness_ms: cert_timings.compile_harness_ms,
                    cert_determinism_ms: cert_timings.determinism_ms,
                    cert_mutation_discovery_ms: cert_timings.mutation_discovery_ms,
                    cert_mutation_execution_ms: cert_timings.mutation_execution_ms,
                    cert_diff_ms: cert_timings.differential_ms,
                    mir_compile_ms,
                    codegen_ms,
                    cert_report_ms,
                    total_ms,
                },
            );
        }
        "verify-cert" => {
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let cert_path = match path_arg {
                Some(path) => PathBuf::from(path),
                None => {
                    eprintln!("error: missing cert path");
                    std::process::exit(EXIT_USAGE);
                }
            };
            if let Err(err) = verify_certification_report(&cert_path) {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            println!("cert verified: {}", cert_path.display());
        }
        "frontend-preview" | "frontend-explain" | "frontend-fix" => {
            if let Some(identity) = orchestration_identity.as_deref() {
                emit_orchestration_game_event(
                    output_format,
                    identity,
                    command,
                    path_arg.as_deref(),
                    "dispatch",
                    None,
                );
            }
            let exit = frontend::execute_frontend_pipeline_command(
                command,
                path_arg,
                program_args,
                output_format,
            );
            if let Some(identity) = orchestration_identity.as_deref() {
                emit_orchestration_game_event(
                    output_format,
                    identity,
                    command,
                    None,
                    if exit == EXIT_OK { "passed" } else { "failed" },
                    None,
                );
            }
            if exit != EXIT_OK {
                std::process::exit(exit);
            }
        }
        "run" => {
            if trace {
                eprintln!("build: command run");
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let mir_module = if integration_mode {
                if !integration_mode_entry_path_is_allowed(&entry_path) {
                    eprintln!(
                        "error: --integration-mode requires entrypoint under src/application/composition/** or src/infrastructure/integrations/**"
                    );
                    eprintln!(
                        "help: move entrypoint to src/application/composition/main.wr or src/infrastructure/integrations/<name>.wr"
                    );
                    std::process::exit(EXIT_USAGE);
                }
                eprintln!(
                    "warning: --integration-mode is fixture-scoped; use only for integration executables under approved paths"
                );
                match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            } else {
                match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    true,
                    strict_naming,
                    false,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            };
            let output = out_path.unwrap_or_else(temp_exe_path);
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let status = match Command::new(&output).args(&program_args).status() {
                Ok(status) => status,
                Err(err) => {
                    eprintln!("error: failed to run compiled binary {output}: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            std::process::exit(status.code().unwrap_or(EXIT_RUNTIME_SIGNAL));
        }
        "dev" => {
            if trace {
                eprintln!("build: command dev");
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let poll = poll_ms.unwrap_or(500);
            run_dev_loop(
                &entry_path,
                poll,
                output_format,
                emit_mir,
                emit_mir_opt,
                strict_naming,
                &program_args,
            );
        }
        "test" => {
            let exit = cert_engine::execute_test_command(cert_engine::TestCommandInput {
                trace,
                program_args,
                out_path,
                emit_obj,
                emit_bin,
                path_arg,
                test_jobs,
                test_timeout_ms,
                test_record,
                test_update_public_surface,
                test_selection,
                repro_artifact_path,
                replay_trace_path,
                output_format,
                perf_debug,
                perf_gate_path,
                perf_max_regression_pct,
                kpi_thresholds,
                test_seed,
            });
            std::process::exit(exit);
        }
        "eval" => {
            let exit = execute_eval_command(EvalCommandInput {
                trace,
                path_arg,
                program_args,
                runs: perf_runs,
                output_format,
            });
            std::process::exit(exit);
        }
        "perf" => {
            let exit = perf_engine::execute_perf_command(perf_engine::PerfCommandInput {
                trace,
                program_args,
                path_arg,
                perf_runs,
                test_jobs,
                test_timeout_ms,
                benchmark_manifest_path,
                perf_profile,
                perf_baseline_out,
                perf_gate_path,
                perf_max_regression_pct,
                perf_cv_max_pct,
                kpi_thresholds,
                output_format,
                perf_debug,
                test_selection,
            });
            std::process::exit(exit);
        }
        "perfcmp" => {
            let exit = perf_engine::execute_perfcmp_command(perf_engine::PerfcmpCommandInput {
                trace,
                program_args,
                path_arg,
                benchmark_manifest_path,
                perfcmp_baseline_ref,
                perfcmp_candidate_ref,
                out_path,
                output_format,
                perf_profile,
                perfcmp_warmup_pairs,
                perfcmp_measure_pairs,
                perfcmp_min_effect_pct,
                perfcmp_confidence_pct,
                test_timeout_ms,
                perf_debug,
            });
            std::process::exit(exit);
        }
        "matrix" => {
            let exit = perf_engine::execute_matrix_command(perf_engine::MatrixCommandInput {
                trace,
                program_args,
                path_arg,
                perf_runs,
                perf_gate_path,
                perf_max_regression_pct,
                kpi_thresholds,
            });
            std::process::exit(exit);
        }
        "deploy" => {
            let exit = deploy::execute_deploy_command(deploy::DeployCommandInput {
                trace,
                path_arg,
                program_args,
                target: deploy_target,
                app: deploy_app,
                region: deploy_region,
                machines: deploy_machines,
                deploy_policy,
                replication_factor: deploy_replication_factor,
                write_quorum: deploy_write_quorum,
                logical_shards: deploy_logical_shards,
                active_groups: deploy_active_groups,
                force: deploy_force,
                generate_only: deploy_generate_only,
            });
            std::process::exit(exit);
        }
        "preview" => {
            if trace {
                eprintln!("build: command preview");
            }
            let exit = handle_preview_command(
                path_arg.as_deref(),
                preview_port,
                preview_open,
            );
            std::process::exit(exit);
        }
        "resolve" => {
            if trace {
                eprintln!("build: command resolve");
            }
            let exit = handle_resolve_command(
                path_arg.as_deref(),
                resolve_dry_run,
                resolve_force,
                resolve_parallel,
            );
            std::process::exit(exit);
        }
        _ => {
            diag_emit::print_help();
            std::process::exit(EXIT_USAGE);
        }
    }
}

fn default_agent_prompt_for_orchestration(identity: &str) -> Option<&'static str> {
    match identity {
        "studio.synth" => Some("synthesize deterministic game implementation"),
        "studio.synth-assets" => Some("synthesize deterministic AAA asset factory output"),
        "studio.full-factory" => Some("make a AAA game"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FullFactoryCheckpointV1 {
    schema_version: u32,
    kind: String,
    phases: BTreeMap<String, FullFactoryCheckpointPhaseV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FullFactoryCheckpointPhaseV1 {
    status: String,
    artifact_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct FullFactoryPhaseReportV1 {
    id: String,
    status: String,
    attempts: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hash_pairs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct FullFactoryDagRunOutcome {
    report_artifact: String,
    phase_artifacts: Vec<String>,
}

fn is_transient_full_factory_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("timeout")
        || lower.contains("temporar")
        || lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("resource busy")
}

fn full_factory_fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn hash_file_for_checkpoint(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read {} for checkpoint hashing: {error}", path.display()))?;
    Ok(full_factory_fnv1a64_hex(bytes.as_slice()))
}

fn load_full_factory_checkpoint(path: &Path) -> Result<FullFactoryCheckpointV1, String> {
    if !path.exists() {
        return Ok(FullFactoryCheckpointV1 {
            schema_version: 1,
            kind: "full_factory_checkpoint_v1".to_string(),
            phases: BTreeMap::new(),
        });
    }
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let checkpoint: FullFactoryCheckpointV1 = serde_json::from_slice(bytes.as_slice())
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(checkpoint)
}

fn write_full_factory_checkpoint(
    path: &Path,
    checkpoint: &FullFactoryCheckpointV1,
) -> Result<(), String> {
    write_json_with_deterministic_rewrite(path, checkpoint, "full factory checkpoint")
}

fn phase_artifacts_are_checkpoint_valid(
    phase: &FullFactoryCheckpointPhaseV1,
) -> bool {
    if phase.status != "passed" {
        return false;
    }
    if phase.artifact_hashes.is_empty() {
        return false;
    }
    phase.artifact_hashes.iter().all(|(artifact, expected_hash)| {
        let artifact_path = PathBuf::from(artifact);
        artifact_path.exists()
            && hash_file_for_checkpoint(artifact_path.as_path())
                .is_ok_and(|actual| actual == *expected_hash)
    })
}

fn should_resume_full_factory_phase(
    downstream_invalidated: bool,
    phase: Option<&FullFactoryCheckpointPhaseV1>,
) -> bool {
    !downstream_invalidated && phase.is_some_and(phase_artifacts_are_checkpoint_valid)
}

fn run_full_factory_checkpoint_dag(
    canonical_app_root: &Path,
    app_path: &str,
    stable_artifact_root: &Path,
    execution_artifact_root: &Path,
    output_format: OutputFormat,
) -> Result<FullFactoryDagRunOutcome, String> {
    let checkpoint_path = stable_artifact_root.join("full-factory-checkpoint.json");
    let report_path = execution_artifact_root.join("full-factory-dag-report.json");
    let mut checkpoint = load_full_factory_checkpoint(checkpoint_path.as_path())?;
    let phase_ids = [
        "synth-assets",
        "bake-assets",
        "validate-assets",
        "package-assets",
        "promote-assets",
    ];
    let mut phase_reports = Vec::new();
    let mut all_phase_artifacts = BTreeSet::new();
    let dist_dir = game_dist_dir(canonical_app_root);
    let mut downstream_invalidated = false;

    for phase_id in phase_ids {
        let existing_phase = checkpoint.phases.get(phase_id);
        if should_resume_full_factory_phase(downstream_invalidated, existing_phase)
        {
            let existing = existing_phase.expect("checked by should_resume_full_factory_phase");
            let mut artifacts = existing.artifact_hashes.keys().cloned().collect::<Vec<_>>();
            artifacts.sort();
            all_phase_artifacts.extend(artifacts.iter().cloned());
            phase_reports.push(FullFactoryPhaseReportV1 {
                id: phase_id.to_string(),
                status: "resumed".to_string(),
                attempts: 0,
                hash_pairs: existing
                    .artifact_hashes
                    .iter()
                    .map(|(artifact, hash)| format!("{artifact}:{hash}"))
                    .collect(),
                artifacts,
                error: None,
            });
            continue;
        }

        // If an upstream phase had to rerun, all downstream phases must rerun too.
        downstream_invalidated = true;
        let mut attempts = 0u32;
        let mut last_error: Option<String> = None;
        let mut artifact_paths = Vec::<PathBuf>::new();
        loop {
            attempts = attempts.saturating_add(1);
            let result = match phase_id {
                "synth-assets" => {
                    let artifacts = [
                        stable_artifact_root.join("intent.json"),
                        stable_artifact_root.join("plan.json"),
                        stable_artifact_root.join("variant-scoring-report.json"),
                    ];
                    let mut missing = artifacts
                        .iter()
                        .filter(|path| !path.exists())
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>();
                    if missing.is_empty() {
                        artifact_paths = artifacts.to_vec();
                        Ok(())
                    } else {
                        missing.sort();
                        Err(format!(
                            "deterministic full-factory synth phase missing artifacts: {}",
                            missing.join(", ")
                        ))
                    }
                }
                "bake-assets" => {
                    execute_game_command_with_orchestration(
                        GameCommandInput {
                            command: "build".to_string(),
                            path_arg: Some(app_path.to_string()),
                            out_path: None,
                            game_build_target: Some("dual".to_string()),
                            game_render_backend: Some("webgpu".to_string()),
                            game_host_mode: Some("pure-wasm".to_string()),
                            game_client_runtime: Some("compiled".to_string()),
                            game_shader_provenance: true,
                            game_no_shortcuts: true,
                            game_check_determinism: false,
                            game_check_rollback: false,
                            game_check_render_lane: false,
                            game_check_asset_streaming: false,
                            game_profile_gpu_metrics: false,
                            game_profile_streaming_metrics: false,
                        },
                        output_format,
                        Some("studio.bake-assets"),
                    )?;
                    artifact_paths = vec![
                        dist_dir.join("build-manifest.json"),
                        dist_dir.join("asset-factory-manifest-v2.json"),
                        dist_dir.join("asset-provenance-ledger-v1.json"),
                        dist_dir.join("asset-quality-report-v2.json"),
                        dist_dir.join("ui-atlas-manifest-v1.json"),
                        dist_dir.join("character-bundle-manifest-v2.json"),
                        dist_dir.join("animation-rig-catalog-v1.json"),
                        dist_dir.join("animation-clip-bundle-v1.json"),
                        dist_dir.join("animation-graph-contract-v1.json"),
                        dist_dir.join("flora-sim-contract-v1.json"),
                        dist_dir.join("animation-quality-report-v1.json"),
                    ];
                    Ok(())
                }
                "validate-assets" => {
                    execute_game_command_with_orchestration(
                        GameCommandInput {
                            command: "check".to_string(),
                            path_arg: Some(app_path.to_string()),
                            out_path: None,
                            game_build_target: Some("dual".to_string()),
                            game_render_backend: Some("webgpu".to_string()),
                            game_host_mode: Some("pure-wasm".to_string()),
                            game_client_runtime: Some("compiled".to_string()),
                            game_shader_provenance: true,
                            game_no_shortcuts: true,
                            game_check_determinism: true,
                            game_check_rollback: true,
                            game_check_render_lane: true,
                            game_check_asset_streaming: true,
                            game_profile_gpu_metrics: false,
                            game_profile_streaming_metrics: false,
                        },
                        output_format,
                        Some("studio.validate-assets"),
                    )?;
                    artifact_paths = vec![
                        dist_dir.join("build-manifest.json"),
                        dist_dir.join("asset-factory-manifest-v2.json"),
                        dist_dir.join("asset-provenance-ledger-v1.json"),
                        dist_dir.join("asset-quality-report-v2.json"),
                        dist_dir.join("ui-atlas-manifest-v1.json"),
                        dist_dir.join("character-bundle-manifest-v2.json"),
                        dist_dir.join("animation-rig-catalog-v1.json"),
                        dist_dir.join("animation-clip-bundle-v1.json"),
                        dist_dir.join("animation-graph-contract-v1.json"),
                        dist_dir.join("flora-sim-contract-v1.json"),
                        dist_dir.join("animation-quality-report-v1.json"),
                    ];
                    Ok(())
                }
                "package-assets" => {
                    let required = [
                        dist_dir.join("asset-factory-manifest-v2.json"),
                        dist_dir.join("asset-provenance-ledger-v1.json"),
                        dist_dir.join("asset-quality-report-v2.json"),
                        dist_dir.join("ui-atlas-manifest-v1.json"),
                        dist_dir.join("character-bundle-manifest-v2.json"),
                        dist_dir.join("animation-rig-catalog-v1.json"),
                        dist_dir.join("animation-clip-bundle-v1.json"),
                        dist_dir.join("animation-graph-contract-v1.json"),
                        dist_dir.join("flora-sim-contract-v1.json"),
                        dist_dir.join("animation-quality-report-v1.json"),
                    ];
                    let mut missing = required
                        .iter()
                        .filter(|path| !path.exists())
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>();
                    if missing.is_empty() {
                        artifact_paths = required.to_vec();
                        Ok(())
                    } else {
                        missing.sort();
                        Err(format!(
                            "deterministic full-factory package phase missing artifacts: {}",
                            missing.join(", ")
                        ))
                    }
                }
                "promote-assets" => {
                    let report = build_asset_factory_check_report(dist_dir.as_path())?;
                    if report
                        .get("passed")
                        .and_then(|value| value.as_bool())
                        != Some(true)
                    {
                        return Err(format!(
                            "deterministic full-factory promote phase failed asset gate: {}",
                            serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string())
                        ));
                    }
                    let promotion_path = dist_dir.join("asset-factory-promotion.json");
                    write_json_with_deterministic_rewrite(
                        promotion_path.as_path(),
                        &serde_json::json!({
                            "schema_version": 1,
                            "kind": "asset-factory-promotion-v1",
                            "status": "promoted",
                            "source_report": report,
                        }),
                        "asset factory promotion",
                    )?;
                    artifact_paths = vec![promotion_path];
                    Ok(())
                }
                _ => Err(format!("unsupported full-factory phase `{phase_id}`")),
            };

            match result {
                Ok(()) => break,
                Err(error) => {
                    let transient = is_transient_full_factory_error(error.as_str());
                    last_error = Some(error.clone());
                    if transient && attempts < 3 {
                        continue;
                    }
                    let classification = if transient {
                        "failed-transient"
                    } else {
                        "failed-deterministic"
                    };
                    phase_reports.push(FullFactoryPhaseReportV1 {
                        id: phase_id.to_string(),
                        status: classification.to_string(),
                        attempts,
                        artifacts: Vec::new(),
                        hash_pairs: Vec::new(),
                        error: Some(error),
                    });
                    write_json_with_deterministic_rewrite(
                        report_path.as_path(),
                        &serde_json::json!({
                            "schema_version": 1,
                            "kind": "full_factory_dag_report_v1",
                            "status": "failed",
                            "phases": phase_reports,
                        }),
                        "full factory dag report",
                    )?;
                    return Err(format!(
                        "full-factory phase `{phase_id}` failed after {attempts} attempt(s): {}",
                        last_error.unwrap_or_else(|| "unknown error".to_string())
                    ));
                }
            }
        }

        let mut artifact_hashes = BTreeMap::new();
        let mut artifact_strings = Vec::new();
        for artifact in &artifact_paths {
            let hash = hash_file_for_checkpoint(artifact.as_path())?;
            let artifact_string = artifact.display().to_string();
            artifact_strings.push(artifact_string.clone());
            artifact_hashes.insert(artifact_string.clone(), hash);
            all_phase_artifacts.insert(artifact_string);
        }
        artifact_strings.sort();
        let hash_pairs = artifact_hashes
            .iter()
            .map(|(artifact, hash)| format!("{artifact}:{hash}"))
            .collect::<Vec<_>>();

        checkpoint.phases.insert(
            phase_id.to_string(),
            FullFactoryCheckpointPhaseV1 {
                status: "passed".to_string(),
                artifact_hashes,
            },
        );
        write_full_factory_checkpoint(checkpoint_path.as_path(), &checkpoint)?;

        phase_reports.push(FullFactoryPhaseReportV1 {
            id: phase_id.to_string(),
            status: "passed".to_string(),
            attempts,
            artifacts: artifact_strings,
            hash_pairs,
            error: None,
        });
    }

    write_json_with_deterministic_rewrite(
        report_path.as_path(),
        &serde_json::json!({
            "schema_version": 1,
            "kind": "full_factory_dag_report_v1",
            "status": "passed",
            "phases": phase_reports,
        }),
        "full factory dag report",
    )?;

    Ok(FullFactoryDagRunOutcome {
        report_artifact: report_path.display().to_string(),
        phase_artifacts: all_phase_artifacts.into_iter().collect(),
    })
}

fn execute_agent_run_command(
    path_arg: Option<String>,
    program_args: Vec<String>,
    output_format: OutputFormat,
    orchestration_identity: Option<String>,
) -> i32 {
    let started_epoch_seconds = unix_epoch_seconds_now();
    let started_epoch_ms = unix_epoch_millis_now();
    let app_root = match resolve_required_game_app_root(path_arg.as_deref()) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_USAGE;
        }
    };

    let canonical_app_root =
        fs::canonicalize(app_root.as_path()).unwrap_or_else(|_| app_root.clone());
    let orchestration_identity =
        orchestration_identity.unwrap_or_else(|| "agent-run.direct".to_string());
    let prompt = {
        let explicit = program_args.join(" ").trim().to_string();
        if !explicit.is_empty() {
            explicit
        } else {
            match default_agent_prompt_for_orchestration(orchestration_identity.as_str()) {
                Some(value) => value.to_string(),
                None => {
                    eprintln!("error: missing agent prompt for `wrela agent-run`");
                    return EXIT_USAGE;
                }
            }
        }
    };
    let app_path = canonical_app_root.display().to_string();
    let asset_factory_enabled = matches!(
        orchestration_identity.as_str(),
        "studio.synth-assets" | "studio.full-factory"
    );
    let intent = AgentRunIntentV2 {
        schema_version: 2,
        kind: "agent_run_intent_v2".to_string(),
        prompt: prompt.clone(),
        app_path: app_path.clone(),
        seed: default_seed_for_prompt(prompt.as_str(), app_path.as_str()),
        target_profile: "aaa-webgpu-60fps-mid-tier".to_string(),
        execution_profile: "deterministic-single-variant".to_string(),
        constraints: vec![
            "webgpu-only".to_string(),
            "host-mode:pure-wasm".to_string(),
            "build-target:dual".to_string(),
            "strict-gates:compiled+shader-provenance+no-shortcuts".to_string(),
            "check-suite:render-lane+asset-streaming".to_string(),
        ],
        vibe_spec: AgentVibeSpecV2 {
            style: "supportive-collaborative".to_string(),
            audience: "engineering".to_string(),
            creativity: 35,
        },
        fidelity_spec: AgentFidelitySpecV2 {
            mode: AgentFidelityModeV2::Strict,
            min_test_coverage: 85,
            require_reproducible_artifacts: true,
        },
        budget_spec: AgentBudgetSpecV2 {
            max_input_tokens: 16_384,
            max_output_tokens: 8_192,
            max_tool_calls: 64,
            max_wall_time_ms: 240_000,
        },
        policy_profile: AgentPolicyProfileV2 {
            sandbox_mode: "danger-full-access".to_string(),
            network_access: true,
            approval_mode: "never".to_string(),
            data_policy: "workspace-only".to_string(),
        },
        tool_contracts: vec![
            AgentToolContractV2 {
                tool_name: "rg".to_string(),
                mode: "read".to_string(),
                required: true,
                deterministic: true,
            },
            AgentToolContractV2 {
                tool_name: "cargo".to_string(),
                mode: "execute".to_string(),
                required: true,
                deterministic: true,
            },
        ],
        execution_contract: AgentExecutionContractSpecV2 {
            pipeline: AgentPipelineV2::CompilerFirstOneShot,
            max_retries: 0,
            fallback_enabled: false,
        },
        asset_factory_intent: AssetFactoryIntentV1 {
            enabled: asset_factory_enabled,
            mode: if orchestration_identity == "studio.full-factory" {
                "full-factory".to_string()
            } else if orchestration_identity == "studio.synth-assets" {
                "synth-assets".to_string()
            } else {
                "disabled".to_string()
            },
            providers: vec![
                "image-default".to_string(),
                "mesh-default".to_string(),
                "audio-default".to_string(),
                "ui-default".to_string(),
            ],
            strict_provenance: true,
            variant_count: if asset_factory_enabled { 3 } else { 1 },
            seed_lock: true,
        },
    };
    let plan = compile_plan_deterministic(&intent);
    let app_slug = game_app_artifact_stem(canonical_app_root.as_path());
    let stable_artifact_root = resolve_game_workspace_root()
        .join(".artifacts")
        .join("agent-studio")
        .join(app_slug.as_str())
        .join(plan.run_id.as_str());
    if let Err(error) = fs::create_dir_all(stable_artifact_root.as_path()) {
        eprintln!(
            "failed to create agent artifact directory {}: {error}",
            stable_artifact_root.display()
        );
        return EXIT_CODEGEN;
    }
    let execution_id = build_agent_execution_id();
    let execution_artifact_root = stable_artifact_root
        .join("executions")
        .join(execution_id.as_str());
    if let Err(error) = fs::create_dir_all(execution_artifact_root.as_path()) {
        eprintln!(
            "failed to create agent execution artifact directory {}: {error}",
            execution_artifact_root.display()
        );
        return EXIT_CODEGEN;
    }

    let execution_report_path = execution_artifact_root.join(AGENT_EXECUTION_REPORT_FILE);
    let execution_report_artifact = execution_report_path.display().to_string();
    let summary_path = execution_artifact_root.join("summary.json");
    let summary_artifact = summary_path.display().to_string();
    let mut execution_steps = initialize_agent_execution_steps(started_epoch_ms);
    let intent_path = stable_artifact_root.join("intent.json");
    let plan_path = stable_artifact_root.join("plan.json");
    let variant_scoring_report_path = stable_artifact_root.join("variant-scoring-report.json");
    let variant_scoring_report_artifact = variant_scoring_report_path.display().to_string();
    let variant_scoring_report = build_variant_scoring_report(&intent, plan.run_id.as_str());
    let mut intent_plan_artifacts = Vec::new();
    let write_intent_plan_step_start = unix_epoch_millis_now();
    if let Err(error) =
        write_json_with_deterministic_rewrite(intent_path.as_path(), &intent, "agent intent")
    {
        eprintln!("{error}");
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "write_intent_plan",
            "failed",
            write_intent_plan_step_start,
            unix_epoch_millis_now(),
            intent_plan_artifacts,
            Some(error),
        );
        if let Err(report_error) = write_agent_execution_report(
            execution_report_path.as_path(),
            plan.run_id.as_str(),
            execution_id.as_str(),
            app_path.as_str(),
            started_epoch_seconds,
            "failed-intent-plan",
            execution_steps.as_slice(),
        ) {
            eprintln!("{report_error}");
        }
        return EXIT_CODEGEN;
    }
    intent_plan_artifacts.push(intent_path.display().to_string());
    if let Err(error) =
        write_json_with_deterministic_rewrite(plan_path.as_path(), &plan, "agent plan")
    {
        eprintln!("{error}");
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "write_intent_plan",
            "failed",
            write_intent_plan_step_start,
            unix_epoch_millis_now(),
            intent_plan_artifacts,
            Some(error),
        );
        if let Err(report_error) = write_agent_execution_report(
            execution_report_path.as_path(),
            plan.run_id.as_str(),
            execution_id.as_str(),
            app_path.as_str(),
            started_epoch_seconds,
            "failed-intent-plan",
            execution_steps.as_slice(),
        ) {
            eprintln!("{report_error}");
        }
        return EXIT_CODEGEN;
    }
    intent_plan_artifacts.push(plan_path.display().to_string());
    if let Err(error) = write_json_with_deterministic_rewrite(
        variant_scoring_report_path.as_path(),
        &variant_scoring_report,
        "agent variant scoring report",
    ) {
        eprintln!("{error}");
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "write_intent_plan",
            "failed",
            write_intent_plan_step_start,
            unix_epoch_millis_now(),
            intent_plan_artifacts,
            Some(error),
        );
        if let Err(report_error) = write_agent_execution_report(
            execution_report_path.as_path(),
            plan.run_id.as_str(),
            execution_id.as_str(),
            app_path.as_str(),
            started_epoch_seconds,
            "failed-intent-plan",
            execution_steps.as_slice(),
        ) {
            eprintln!("{report_error}");
        }
        return EXIT_CODEGEN;
    }
    intent_plan_artifacts.push(variant_scoring_report_artifact.clone());
    record_agent_execution_step(
        execution_steps.as_mut_slice(),
        "write_intent_plan",
        "passed",
        write_intent_plan_step_start,
        unix_epoch_millis_now(),
        intent_plan_artifacts.clone(),
        None,
    );

    let mut summary_artifacts = intent_plan_artifacts.clone();
    let orchestration_evidence_path = execution_artifact_root.join("orchestration-evidence.json");
    let orchestration_evidence_artifact = orchestration_evidence_path.display().to_string();

    let strict_gate_config = GameStrictGateConfig {
        client_runtime_compiled: true,
        shader_provenance: true,
        no_shortcuts: true,
    };
    let game_orchestration_context =
        game_orchestration_context_from_identity(Some(orchestration_identity.as_str()));

    let full_factory_outcome = if orchestration_identity == "studio.full-factory" {
        let full_factory_step_start = unix_epoch_millis_now();
        match run_full_factory_checkpoint_dag(
            canonical_app_root.as_path(),
            app_path.as_str(),
            stable_artifact_root.as_path(),
            execution_artifact_root.as_path(),
            output_format,
        ) {
            Ok(outcome) => {
                let mut artifacts = outcome.phase_artifacts.clone();
                artifacts.push(outcome.report_artifact.clone());
                artifacts.sort();
                artifacts.dedup();
                record_agent_execution_step(
                    execution_steps.as_mut_slice(),
                    "full_factory_dag",
                    "passed",
                    full_factory_step_start,
                    unix_epoch_millis_now(),
                    artifacts.clone(),
                    None,
                );
                summary_artifacts.extend(artifacts);
                Some(outcome)
            }
            Err(error) => {
                eprintln!("{error}");
                record_agent_execution_step(
                    execution_steps.as_mut_slice(),
                    "full_factory_dag",
                    "failed",
                    full_factory_step_start,
                    unix_epoch_millis_now(),
                    Vec::new(),
                    Some(error),
                );
                let mut failed_summary_artifacts = summary_artifacts.clone();
                failed_summary_artifacts.push(execution_report_artifact.clone());
                let summary = AgentRunSummaryV2 {
                    schema_version: 2,
                    kind: "agent_run_summary_v2".to_string(),
                    run_id: plan.run_id.clone(),
                    status: "failed-full-factory".to_string(),
                    selected_variant: "none".to_string(),
                    artifacts: failed_summary_artifacts,
                };
                let write_summary_step_start = unix_epoch_millis_now();
                if let Err(summary_error) = write_agent_run_summary(summary_path.as_path(), &summary) {
                    eprintln!("{summary_error}");
                    record_agent_execution_step(
                        execution_steps.as_mut_slice(),
                        "write_summary",
                        "failed",
                        write_summary_step_start,
                        unix_epoch_millis_now(),
                        Vec::new(),
                        Some(summary_error),
                    );
                    if let Err(report_error) = write_agent_execution_report(
                        execution_report_path.as_path(),
                        plan.run_id.as_str(),
                        execution_id.as_str(),
                        app_path.as_str(),
                        started_epoch_seconds,
                        "failed-summary",
                        execution_steps.as_slice(),
                    ) {
                        eprintln!("{report_error}");
                    }
                    return EXIT_CODEGEN;
                }
                record_agent_execution_step(
                    execution_steps.as_mut_slice(),
                    "write_summary",
                    "passed",
                    write_summary_step_start,
                    unix_epoch_millis_now(),
                    vec![summary_artifact.clone()],
                    None,
                );
                if let Err(report_error) = write_agent_execution_report(
                    execution_report_path.as_path(),
                    plan.run_id.as_str(),
                    execution_id.as_str(),
                    app_path.as_str(),
                    started_epoch_seconds,
                    "failed-full-factory",
                    execution_steps.as_slice(),
                ) {
                    eprintln!("{report_error}");
                    reconcile_agent_summary_after_execution_report_failure(
                        summary_path.as_path(),
                        &summary,
                        execution_report_artifact.as_str(),
                    );
                }
                return EXIT_CODEGEN;
            }
        }
    } else {
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "full_factory_dag",
            "not_applicable",
            unix_epoch_millis_now(),
            unix_epoch_millis_now(),
            Vec::new(),
            None,
        );
        None
    };
    if let Some(outcome) = full_factory_outcome.as_ref() {
        summary_artifacts.push(outcome.report_artifact.clone());
    }

    let game_check_step_start = unix_epoch_millis_now();
    let check = match game_check_project(
        canonical_app_root.as_path(),
        false,
        false,
        true,
        true,
        GameRenderBackend::WebGpu,
        GameHostMode::PureWasm,
        strict_gate_config,
        game_orchestration_context.as_ref(),
    ) {
        Ok(check) => {
            let game_check_artifacts = vec![
                check
                    .dist_dir
                    .join("build-manifest.json")
                    .display()
                    .to_string(),
                check
                    .dist_dir
                    .join("render-manifest.json")
                    .display()
                    .to_string(),
                check
                    .dist_dir
                    .join("shader-bundle.json")
                    .display()
                    .to_string(),
                check
                    .dist_dir
                    .join("assets-manifest.json")
                    .display()
                    .to_string(),
                check
                    .dist_dir
                    .join("world-chunks.json")
                    .display()
                    .to_string(),
                check.test_matrix_path.display().to_string(),
            ];
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "game_check",
                "passed",
                game_check_step_start,
                unix_epoch_millis_now(),
                game_check_artifacts,
                None,
            );
            check
        }
        Err(error) => {
            eprintln!("{error}");
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "game_check",
                "failed",
                game_check_step_start,
                unix_epoch_millis_now(),
                Vec::new(),
                Some(error),
            );
            let mut failed_summary_artifacts = summary_artifacts.clone();
            failed_summary_artifacts.push(execution_report_artifact.clone());
            let summary = AgentRunSummaryV2 {
                schema_version: 2,
                kind: "agent_run_summary_v2".to_string(),
                run_id: plan.run_id.clone(),
                status: "failed-check".to_string(),
                selected_variant: "none".to_string(),
                artifacts: failed_summary_artifacts,
            };
            let write_summary_step_start = unix_epoch_millis_now();
            if let Err(summary_error) = write_agent_run_summary(summary_path.as_path(), &summary) {
                eprintln!("{summary_error}");
                record_agent_execution_step(
                    execution_steps.as_mut_slice(),
                    "write_summary",
                    "failed",
                    write_summary_step_start,
                    unix_epoch_millis_now(),
                    Vec::new(),
                    Some(summary_error),
                );
                if let Err(report_error) = write_agent_execution_report(
                    execution_report_path.as_path(),
                    plan.run_id.as_str(),
                    execution_id.as_str(),
                    app_path.as_str(),
                    started_epoch_seconds,
                    "failed-summary",
                    execution_steps.as_slice(),
                ) {
                    eprintln!("{report_error}");
                }
                return EXIT_CODEGEN;
            }
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "write_summary",
                "passed",
                write_summary_step_start,
                unix_epoch_millis_now(),
                vec![summary_artifact.clone()],
                None,
            );
            if let Err(report_error) = write_agent_execution_report(
                execution_report_path.as_path(),
                plan.run_id.as_str(),
                execution_id.as_str(),
                app_path.as_str(),
                started_epoch_seconds,
                "failed-check",
                execution_steps.as_slice(),
            ) {
                eprintln!("{report_error}");
                reconcile_agent_summary_after_execution_report_failure(
                    summary_path.as_path(),
                    &summary,
                    execution_report_artifact.as_str(),
                );
            }
            return EXIT_CODEGEN;
        }
    };

    let validate_generated_step_start = unix_epoch_millis_now();
    let generated_artifacts = match collect_agent_run_generated_artifacts(&check) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            eprintln!("{error}");
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "validate_generated_artifacts",
                "failed",
                validate_generated_step_start,
                unix_epoch_millis_now(),
                Vec::new(),
                Some(error),
            );
            let mut failed_summary_artifacts = summary_artifacts.clone();
            failed_summary_artifacts.push(execution_report_artifact.clone());
            let summary = AgentRunSummaryV2 {
                schema_version: 2,
                kind: "agent_run_summary_v2".to_string(),
                run_id: plan.run_id.clone(),
                status: "failed-artifacts".to_string(),
                selected_variant: "none".to_string(),
                artifacts: failed_summary_artifacts,
            };
            let write_summary_step_start = unix_epoch_millis_now();
            if let Err(summary_error) = write_agent_run_summary(summary_path.as_path(), &summary) {
                eprintln!("{summary_error}");
                record_agent_execution_step(
                    execution_steps.as_mut_slice(),
                    "write_summary",
                    "failed",
                    write_summary_step_start,
                    unix_epoch_millis_now(),
                    Vec::new(),
                    Some(summary_error),
                );
                if let Err(report_error) = write_agent_execution_report(
                    execution_report_path.as_path(),
                    plan.run_id.as_str(),
                    execution_id.as_str(),
                    app_path.as_str(),
                    started_epoch_seconds,
                    "failed-summary",
                    execution_steps.as_slice(),
                ) {
                    eprintln!("{report_error}");
                }
                return EXIT_CODEGEN;
            }
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "write_summary",
                "passed",
                write_summary_step_start,
                unix_epoch_millis_now(),
                vec![summary_artifact.clone()],
                None,
            );
            if let Err(report_error) = write_agent_execution_report(
                execution_report_path.as_path(),
                plan.run_id.as_str(),
                execution_id.as_str(),
                app_path.as_str(),
                started_epoch_seconds,
                "failed-artifacts",
                execution_steps.as_slice(),
            ) {
                eprintln!("{report_error}");
                reconcile_agent_summary_after_execution_report_failure(
                    summary_path.as_path(),
                    &summary,
                    execution_report_artifact.as_str(),
                );
            }
            return EXIT_CODEGEN;
        }
    };

    if let Err(error) = write_agent_run_orchestration_evidence(
        orchestration_evidence_path.as_path(),
        orchestration_identity.as_str(),
        generated_artifacts.as_slice(),
    ) {
        eprintln!("{error}");
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "validate_generated_artifacts",
            "failed",
            validate_generated_step_start,
            unix_epoch_millis_now(),
            generated_artifacts.clone(),
            Some(error),
        );
        let mut failed_summary_artifacts = summary_artifacts.clone();
        failed_summary_artifacts.push(execution_report_artifact.clone());
        let summary = AgentRunSummaryV2 {
            schema_version: 2,
            kind: "agent_run_summary_v2".to_string(),
            run_id: plan.run_id.clone(),
            status: "failed-artifacts".to_string(),
            selected_variant: "none".to_string(),
            artifacts: failed_summary_artifacts,
        };
        let write_summary_step_start = unix_epoch_millis_now();
        if let Err(summary_error) = write_agent_run_summary(summary_path.as_path(), &summary) {
            eprintln!("{summary_error}");
            record_agent_execution_step(
                execution_steps.as_mut_slice(),
                "write_summary",
                "failed",
                write_summary_step_start,
                unix_epoch_millis_now(),
                Vec::new(),
                Some(summary_error),
            );
            if let Err(report_error) = write_agent_execution_report(
                execution_report_path.as_path(),
                plan.run_id.as_str(),
                execution_id.as_str(),
                app_path.as_str(),
                started_epoch_seconds,
                "failed-summary",
                execution_steps.as_slice(),
            ) {
                eprintln!("{report_error}");
            }
            return EXIT_CODEGEN;
        }
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "write_summary",
            "passed",
            write_summary_step_start,
            unix_epoch_millis_now(),
            vec![summary_artifact.clone()],
            None,
        );
        if let Err(report_error) = write_agent_execution_report(
            execution_report_path.as_path(),
            plan.run_id.as_str(),
            execution_id.as_str(),
            app_path.as_str(),
            started_epoch_seconds,
            "failed-artifacts",
            execution_steps.as_slice(),
        ) {
            eprintln!("{report_error}");
            reconcile_agent_summary_after_execution_report_failure(
                summary_path.as_path(),
                &summary,
                execution_report_artifact.as_str(),
            );
        }
        return EXIT_CODEGEN;
    }
    let mut validate_generated_step_artifacts = generated_artifacts.clone();
    validate_generated_step_artifacts.push(orchestration_evidence_artifact.clone());
    record_agent_execution_step(
        execution_steps.as_mut_slice(),
        "validate_generated_artifacts",
        "passed",
        validate_generated_step_start,
        unix_epoch_millis_now(),
        validate_generated_step_artifacts,
        None,
    );
    summary_artifacts.push(orchestration_evidence_artifact.clone());
    summary_artifacts.extend(generated_artifacts);
    summary_artifacts.sort();
    summary_artifacts.dedup();
    summary_artifacts.push(execution_report_artifact.clone());
    let selected_variant = variant_scoring_report.selected_variant.clone();
    let summary = AgentRunSummaryV2 {
        schema_version: 2,
        kind: "agent_run_summary_v2".to_string(),
        run_id: plan.run_id.clone(),
        status: "passed".to_string(),
        selected_variant,
        artifacts: summary_artifacts,
    };
    let write_summary_step_start = unix_epoch_millis_now();
    if let Err(error) = write_agent_run_summary(summary_path.as_path(), &summary) {
        eprintln!("{error}");
        record_agent_execution_step(
            execution_steps.as_mut_slice(),
            "write_summary",
            "failed",
            write_summary_step_start,
            unix_epoch_millis_now(),
            Vec::new(),
            Some(error),
        );
        if let Err(report_error) = write_agent_execution_report(
            execution_report_path.as_path(),
            plan.run_id.as_str(),
            execution_id.as_str(),
            app_path.as_str(),
            started_epoch_seconds,
            "failed-summary",
            execution_steps.as_slice(),
        ) {
            eprintln!("{report_error}");
        }
        return EXIT_CODEGEN;
    }
    record_agent_execution_step(
        execution_steps.as_mut_slice(),
        "write_summary",
        "passed",
        write_summary_step_start,
        unix_epoch_millis_now(),
        vec![summary_artifact.clone()],
        None,
    );
    if let Err(report_error) = write_agent_execution_report(
        execution_report_path.as_path(),
        plan.run_id.as_str(),
        execution_id.as_str(),
        app_path.as_str(),
        started_epoch_seconds,
        summary.status.as_str(),
        execution_steps.as_slice(),
    ) {
        eprintln!("{report_error}");
        reconcile_agent_summary_after_execution_report_failure(
            summary_path.as_path(),
            &summary,
            execution_report_artifact.as_str(),
        );
        return EXIT_CODEGEN;
    }

    let payload = serde_json::json!({
        "command": "agent-run",
        "status": summary.status.as_str(),
        "orchestration_identity": orchestration_identity.as_str(),
        "orchestration_evidence": orchestration_evidence_path.display().to_string(),
        "full_factory_dag_report": full_factory_outcome
            .as_ref()
            .map(|outcome| outcome.report_artifact.as_str()),
        "run_id": plan.run_id.as_str(),
        "selected_variant": variant_scoring_report.selected_variant.as_str(),
        "variant_scoring_report": variant_scoring_report_path.display().to_string(),
        "artifact_root": stable_artifact_root.display().to_string(),
        "execution_artifact_root": execution_artifact_root.display().to_string(),
        "summary": summary_path.display().to_string(),
        "execution_report": execution_report_path.display().to_string(),
        "test_matrix": check.test_matrix_path.display().to_string(),
    });

    match output_format {
        OutputFormat::Pretty => {
            println!("agent-run passed");
            println!("app: {}", canonical_app_root.display());
            println!("artifact root: {}", stable_artifact_root.display());
            println!(
                "execution artifact root: {}",
                execution_artifact_root.display()
            );
            println!("summary: {}", summary_path.display());
        }
        OutputFormat::Json | OutputFormat::Sarif => {
            println!(
                "{}",
                serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
            );
        }
    }

    EXIT_OK
}

fn build_agent_execution_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    format!(
        "{:010}-{:09}-{}",
        now.as_secs(),
        now.subsec_nanos(),
        std::process::id()
    )
}

fn unix_epoch_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_epoch_millis_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[derive(Clone, Debug, Serialize)]
struct AgentExecutionStepReportV1 {
    id: String,
    status: String,
    start_epoch_ms: u64,
    end_epoch_ms: u64,
    artifacts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct AgentExecutionReportV1 {
    schema_version: u32,
    kind: String,
    run_id: String,
    execution_id: String,
    app_path: String,
    started_epoch_seconds: u64,
    finished_epoch_seconds: u64,
    status: String,
    steps: Vec<AgentExecutionStepReportV1>,
}

fn initialize_agent_execution_steps(start_epoch_ms: u64) -> Vec<AgentExecutionStepReportV1> {
    vec![
        AgentExecutionStepReportV1 {
            id: "write_intent_plan".to_string(),
            status: "not_run".to_string(),
            start_epoch_ms,
            end_epoch_ms: start_epoch_ms,
            artifacts: Vec::new(),
            error: None,
        },
        AgentExecutionStepReportV1 {
            id: "full_factory_dag".to_string(),
            status: "not_run".to_string(),
            start_epoch_ms,
            end_epoch_ms: start_epoch_ms,
            artifacts: Vec::new(),
            error: None,
        },
        AgentExecutionStepReportV1 {
            id: "game_check".to_string(),
            status: "not_run".to_string(),
            start_epoch_ms,
            end_epoch_ms: start_epoch_ms,
            artifacts: Vec::new(),
            error: None,
        },
        AgentExecutionStepReportV1 {
            id: "validate_generated_artifacts".to_string(),
            status: "not_run".to_string(),
            start_epoch_ms,
            end_epoch_ms: start_epoch_ms,
            artifacts: Vec::new(),
            error: None,
        },
        AgentExecutionStepReportV1 {
            id: "write_summary".to_string(),
            status: "not_run".to_string(),
            start_epoch_ms,
            end_epoch_ms: start_epoch_ms,
            artifacts: Vec::new(),
            error: None,
        },
    ]
}

fn record_agent_execution_step(
    steps: &mut [AgentExecutionStepReportV1],
    step_id: &str,
    status: &str,
    start_epoch_ms: u64,
    end_epoch_ms: u64,
    artifacts: Vec<String>,
    error: Option<String>,
) {
    if let Some(step) = steps.iter_mut().find(|step| step.id == step_id) {
        step.status = status.to_string();
        step.start_epoch_ms = start_epoch_ms;
        step.end_epoch_ms = end_epoch_ms;
        step.artifacts = artifacts;
        step.error = error;
    }
}

fn write_agent_execution_report(
    execution_report_path: &Path,
    run_id: &str,
    execution_id: &str,
    app_path: &str,
    started_epoch_seconds: u64,
    status: &str,
    steps: &[AgentExecutionStepReportV1],
) -> Result<(), String> {
    if let Some(forced_mode) = forced_agent_execution_report_write_failure_mode(
        env::var(AGENT_RUN_FORCE_REPORT_WRITE_FAILURE_ENV)
            .ok()
            .as_deref(),
    ) {
        return match forced_mode {
            AgentExecutionReportWriteFailureMode::PreWrite => Err(format!(
                "forced execution report write failure for {} because {AGENT_RUN_FORCE_REPORT_WRITE_FAILURE_ENV}=prewrite",
                execution_report_path.display()
            )),
            AgentExecutionReportWriteFailureMode::PartialCorrupt => {
                force_partial_corrupt_agent_execution_report_write_failure(execution_report_path)
            }
        };
    }
    let report = AgentExecutionReportV1 {
        schema_version: 1,
        kind: "agent_execution_report_v1".to_string(),
        run_id: run_id.to_string(),
        execution_id: execution_id.to_string(),
        app_path: app_path.to_string(),
        started_epoch_seconds,
        finished_epoch_seconds: unix_epoch_seconds_now(),
        status: status.to_string(),
        steps: steps.to_vec(),
    };
    let encoded = serde_json::to_vec_pretty(&report).map_err(|error| {
        format!(
            "failed to serialize {}: {error}",
            execution_report_path.display()
        )
    })?;
    fs::write(execution_report_path, encoded.as_slice()).map_err(|error| {
        format!(
            "failed to write {}: {error}",
            execution_report_path.display()
        )
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentExecutionReportWriteFailureMode {
    PreWrite,
    PartialCorrupt,
}

fn forced_agent_execution_report_write_failure_mode(
    raw: Option<&str>,
) -> Option<AgentExecutionReportWriteFailureMode> {
    match raw {
        Some("1") | Some("prewrite") => Some(AgentExecutionReportWriteFailureMode::PreWrite),
        Some("partial") | Some("partial-corrupt") | Some("partial_truncate") | Some("corrupt") => {
            Some(AgentExecutionReportWriteFailureMode::PartialCorrupt)
        }
        _ => None,
    }
}

fn force_partial_corrupt_agent_execution_report_write_failure(
    execution_report_path: &Path,
) -> Result<(), String> {
    let truncated = br#"{"schema_version":1,"kind":"agent_execution_report_v1","status":"passed","#;
    fs::write(execution_report_path, truncated.as_slice()).map_err(|error| {
        format!(
            "failed to write forced partial/corrupt report {}: {error}",
            execution_report_path.display()
        )
    })?;
    Err(format!(
        "forced execution report partial/corrupt write failure for {} because {AGENT_RUN_FORCE_REPORT_WRITE_FAILURE_ENV}=partial",
        execution_report_path.display()
    ))
}

fn write_agent_run_summary(summary_path: &Path, summary: &AgentRunSummaryV2) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(summary)
        .map_err(|error| format!("failed to serialize {}: {error}", summary_path.display()))?;
    fs::write(summary_path, encoded.as_slice())
        .map_err(|error| format!("failed to write {}: {error}", summary_path.display()))
}

fn sanitize_agent_run_summary_for_execution_report_failure(
    summary: &AgentRunSummaryV2,
    execution_report_artifact: &str,
) -> AgentRunSummaryV2 {
    let mut sanitized = summary.clone();
    sanitized
        .artifacts
        .retain(|artifact| artifact != execution_report_artifact);
    if sanitized.status == "passed" {
        sanitized.status = "failed-report".to_string();
    }
    sanitized
}

fn reconcile_agent_summary_after_execution_report_failure(
    summary_path: &Path,
    summary: &AgentRunSummaryV2,
    execution_report_artifact: &str,
) {
    let sanitized =
        sanitize_agent_run_summary_for_execution_report_failure(summary, execution_report_artifact);
    if let Err(summary_error) = write_agent_run_summary(summary_path, &sanitized) {
        eprintln!("{summary_error}");
        if let Err(remove_error) = fs::remove_file(summary_path) {
            if remove_error.kind() != io::ErrorKind::NotFound {
                eprintln!(
                    "failed to remove {} after execution report failure: {remove_error}",
                    summary_path.display()
                );
            }
        }
    }
}

fn write_json_with_deterministic_rewrite<T: Serialize>(
    file_path: &Path,
    value: &T,
    context: &str,
) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| {
        format!(
            "failed to serialize {context} {}: {error}",
            file_path.display()
        )
    })?;
    let expected_json =
        serde_json::from_slice::<serde_json::Value>(encoded.as_slice()).map_err(|error| {
            format!(
                "failed to decode serialized {context} {}: {error}",
                file_path.display()
            )
        })?;
    match fs::read(file_path) {
        Ok(existing) => {
            if existing == encoded {
                return Ok(());
            }
            let existing_json =
                serde_json::from_slice::<serde_json::Value>(existing.as_slice()).ok();
            if existing_json
                .as_ref()
                .is_some_and(|value| value == &expected_json)
            {
                return Ok(());
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to read {}: {error}", file_path.display())),
    }

    let parent = file_path.parent().ok_or_else(|| {
        format!(
            "failed to rewrite {context} {}; parent directory missing",
            file_path.display()
        )
    })?;
    let file_name = file_path.file_name().ok_or_else(|| {
        format!(
            "failed to rewrite {context} {}; file name missing",
            file_path.display()
        )
    })?;
    let temp_path = parent.join(format!(
        ".{}.rewrite-{}-{}.tmp",
        file_name.to_string_lossy(),
        unix_epoch_millis_now(),
        std::process::id()
    ));
    fs::write(temp_path.as_path(), encoded.as_slice()).map_err(|error| {
        format!(
            "failed to stage deterministic rewrite for {context} {}: {error}",
            file_path.display()
        )
    })?;

    let staged = fs::read(temp_path.as_path()).map_err(|error| {
        format!(
            "failed to verify staged deterministic rewrite for {context} {}: {error}",
            file_path.display()
        )
    })?;
    if staged != encoded {
        let _ = fs::remove_file(temp_path.as_path());
        return Err(format!(
            "staged deterministic rewrite verification failed for {context} at {}; staged bytes differ",
            file_path.display()
        ));
    }

    if let Err(rename_error) = fs::rename(temp_path.as_path(), file_path) {
        let replace_attempt = if rename_error.kind() == io::ErrorKind::AlreadyExists
            || rename_error.kind() == io::ErrorKind::PermissionDenied
        {
            fs::remove_file(file_path)
                .and_then(|_| fs::rename(temp_path.as_path(), file_path))
                .map_err(|fallback_error| {
                    format!(
                        "failed to replace existing {context} at {} after rename error ({rename_error}): {fallback_error}",
                        file_path.display()
                    )
                })
        } else {
            Err(format!(
                "failed to publish deterministic rewrite for {context} at {}: {rename_error}",
                file_path.display()
            ))
        };
        if let Err(error) = replace_attempt {
            let _ = fs::remove_file(temp_path.as_path());
            return Err(error);
        }
    }

    let rewritten = fs::read(file_path).map_err(|error| {
        format!(
            "failed to verify rewritten {}: {error}",
            file_path.display()
        )
    })?;
    if rewritten == encoded {
        return Ok(());
    }
    let rewritten_json = serde_json::from_slice::<serde_json::Value>(rewritten.as_slice())
        .map_err(|error| {
            format!(
                "rewritten {context} at {} is not valid JSON: {error}",
                file_path.display()
            )
        })?;
    if rewritten_json != expected_json {
        return Err(format!(
            "deterministic rewrite verification failed for {context} at {}; rewritten content differs",
            file_path.display()
        ));
    }
    Ok(())
}

const AGENT_EXECUTION_REPORT_FILE: &str = "execution-report.json";
const AGENT_RUN_FORCE_REPORT_WRITE_FAILURE_ENV: &str = "WRELA_AGENT_RUN_FORCE_REPORT_WRITE_FAILURE";

fn resolve_agent_run_artifact_path(raw: &str, dist_dir: &Path) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        dist_dir.join(path)
    }
}

fn collect_agent_run_generated_artifacts(
    check: &GameCheckArtifacts,
) -> Result<Vec<String>, String> {
    let required_paths = [
        check.dist_dir.join("build-manifest.json"),
        check.dist_dir.join("render-manifest.json"),
        check.dist_dir.join("shader-bundle.json"),
        check.dist_dir.join("assets-manifest.json"),
        check.dist_dir.join("world-chunks.json"),
        check.test_matrix_path.clone(),
    ];
    let mut missing_required = required_paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if !missing_required.is_empty() {
        missing_required.sort();
        return Err(format!(
            "agent-run missing required generated artifacts: {}",
            missing_required.join(", ")
        ));
    }

    let build_manifest_path = check.dist_dir.join("build-manifest.json");
    let build_manifest_bytes = fs::read(build_manifest_path.as_path())
        .map_err(|error| format!("failed to read {}: {error}", build_manifest_path.display()))?;
    let build_manifest: serde_json::Value = serde_json::from_slice(build_manifest_bytes.as_slice())
        .map_err(|error| {
            format!(
                "failed to parse {} as JSON: {error}",
                build_manifest_path.display()
            )
        })?;

    let mut artifacts = BTreeSet::new();
    for path in &required_paths {
        artifacts.insert(path.display().to_string());
    }

    let mut missing_manifest_refs = Vec::new();
    for key in [
        "native_artifact",
        "wasm_artifact",
        "client_wasm_artifact",
        "protocol_artifact",
        "render_manifest_artifact",
        "shader_bundle_artifact",
        "asset_stream_manifest",
        "world_chunk_manifest",
        "asset_factory_manifest",
        "asset_provenance_ledger",
        "asset_quality_report",
        "ui_atlas_manifest",
        "character_bundle_manifest",
    ] {
        let Some(raw) = build_manifest.get(key).and_then(|value| value.as_str()) else {
            continue;
        };
        let resolved = resolve_agent_run_artifact_path(raw, check.dist_dir.as_path());
        if resolved.exists() {
            artifacts.insert(resolved.display().to_string());
        } else {
            missing_manifest_refs.push(format!("{key}={}", resolved.display()));
        }
    }
    if !missing_manifest_refs.is_empty() {
        missing_manifest_refs.sort();
        return Err(format!(
            "agent-run build manifest references missing generated artifacts: {}",
            missing_manifest_refs.join(", ")
        ));
    }

    let has_real_build_artifact = ["native_artifact", "wasm_artifact", "client_wasm_artifact"]
        .iter()
        .any(|key| {
            build_manifest
                .get(*key)
                .and_then(|value| value.as_str())
                .map(|raw| resolve_agent_run_artifact_path(raw, check.dist_dir.as_path()))
                .is_some_and(|path| path.exists())
        });
    if !has_real_build_artifact {
        return Err(
            "agent-run synth requires real generated build artifacts (native/wasm/client wasm)"
                .to_string(),
        );
    }

    Ok(artifacts.into_iter().collect())
}

fn write_agent_run_orchestration_evidence(
    path: &Path,
    orchestration_identity: &str,
    generated_artifacts: &[String],
) -> Result<(), String> {
    write_json_with_deterministic_rewrite(
        path,
        &serde_json::json!({
            "schema_version": 1,
            "kind": "agent_run_orchestration_evidence_v1",
            "orchestration_identity": orchestration_identity,
            "contract": "real-generated-build-artifacts",
            "generated_artifact_count": generated_artifacts.len(),
            "generated_artifacts": generated_artifacts,
        }),
        "agent orchestration evidence",
    )
}

#[cfg(test)]
mod shared_execution_report_tests {
    use super::*;

    #[test]
    fn sanitize_summary_for_report_failure_downgrades_passed_and_drops_report_artifact() {
        let summary = AgentRunSummaryV2 {
            schema_version: 2,
            kind: "agent_run_summary_v2".to_string(),
            run_id: "run-1".to_string(),
            status: "passed".to_string(),
            selected_variant: "variant-0".to_string(),
            artifacts: vec![
                "/tmp/summary.json".to_string(),
                "/tmp/execution-report.json".to_string(),
            ],
        };
        let sanitized = sanitize_agent_run_summary_for_execution_report_failure(
            &summary,
            "/tmp/execution-report.json",
        );

        assert_eq!(sanitized.status, "failed-report");
        assert_eq!(sanitized.artifacts, vec!["/tmp/summary.json".to_string()]);
    }

    #[test]
    fn sanitize_summary_for_report_failure_keeps_existing_failure_status() {
        let summary = AgentRunSummaryV2 {
            schema_version: 2,
            kind: "agent_run_summary_v2".to_string(),
            run_id: "run-1".to_string(),
            status: "failed-check".to_string(),
            selected_variant: "none".to_string(),
            artifacts: vec![
                "/tmp/intent.json".to_string(),
                "/tmp/execution-report.json".to_string(),
            ],
        };
        let sanitized = sanitize_agent_run_summary_for_execution_report_failure(
            &summary,
            "/tmp/execution-report.json",
        );

        assert_eq!(sanitized.status, "failed-check");
        assert_eq!(sanitized.artifacts, vec!["/tmp/intent.json".to_string()]);
    }

    #[test]
    fn reconcile_summary_after_report_failure_rewrites_summary_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let summary_path = temp.path().join("summary.json");
        let execution_report_artifact = temp
            .path()
            .join("execution-report.json")
            .display()
            .to_string();
        let summary = AgentRunSummaryV2 {
            schema_version: 2,
            kind: "agent_run_summary_v2".to_string(),
            run_id: "run-1".to_string(),
            status: "passed".to_string(),
            selected_variant: "variant-0".to_string(),
            artifacts: vec![
                temp.path().join("intent.json").display().to_string(),
                execution_report_artifact.clone(),
            ],
        };
        write_agent_run_summary(summary_path.as_path(), &summary).expect("seed summary");

        reconcile_agent_summary_after_execution_report_failure(
            summary_path.as_path(),
            &summary,
            execution_report_artifact.as_str(),
        );

        let encoded = fs::read(summary_path.as_path()).expect("read summary");
        let rewritten: AgentRunSummaryV2 =
            serde_json::from_slice(encoded.as_slice()).expect("decode summary");
        assert_eq!(rewritten.status, "failed-report");
        assert_eq!(
            rewritten.artifacts,
            vec![temp.path().join("intent.json").display().to_string()]
        );
    }

    #[test]
    fn forced_execution_report_failure_mode_maps_expected_values() {
        assert_eq!(
            forced_agent_execution_report_write_failure_mode(Some("1")),
            Some(AgentExecutionReportWriteFailureMode::PreWrite)
        );
        assert_eq!(
            forced_agent_execution_report_write_failure_mode(Some("prewrite")),
            Some(AgentExecutionReportWriteFailureMode::PreWrite)
        );
        assert_eq!(
            forced_agent_execution_report_write_failure_mode(Some("partial")),
            Some(AgentExecutionReportWriteFailureMode::PartialCorrupt)
        );
        assert_eq!(
            forced_agent_execution_report_write_failure_mode(Some("corrupt")),
            Some(AgentExecutionReportWriteFailureMode::PartialCorrupt)
        );
        assert_eq!(
            forced_agent_execution_report_write_failure_mode(Some("unknown")),
            None
        );
        assert_eq!(forced_agent_execution_report_write_failure_mode(None), None);
    }

    #[test]
    fn partial_corrupt_report_failure_writes_invalid_truncated_content() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report_path = temp.path().join("execution-report.json");

        let error =
            force_partial_corrupt_agent_execution_report_write_failure(report_path.as_path())
                .expect_err("forced partial/corrupt mode should always fail");
        assert!(
            error.contains("partial/corrupt"),
            "unexpected error: {error}"
        );
        let written = fs::read(report_path.as_path()).expect("read forced partial report");
        assert!(!written.is_empty(), "partial write should produce bytes");
        let decode = serde_json::from_slice::<serde_json::Value>(written.as_slice());
        assert!(
            decode.is_err(),
            "forced partial/corrupt content should fail JSON decode"
        );
    }

    #[test]
    fn orchestration_contract_catalog_covers_supported_studio_and_mmo_modes() {
        for identity in [
            "mmo.init",
            "mmo.build",
            "mmo.run",
            "mmo.dev",
            "mmo.check",
            "mmo.profile",
            "mmo.gateway",
            "mmo.zone",
            "mmo.world",
            "mmo.loadtest",
            "mmo.ops",
            "studio.init",
            "studio.build",
            "studio.check",
            "studio.run",
            "studio.preview",
            "studio.explain",
            "studio.fix",
            "studio.synth",
            "studio.synth-assets",
            "studio.bake",
            "studio.bake-assets",
            "studio.pack",
            "studio.package-assets",
            "studio.gate",
            "studio.validate-assets",
            "studio.ship",
            "studio.promote-assets",
            "studio.full-factory",
        ] {
            let Some((invocation, contract, required_outputs)) =
                orchestration_contract_for_identity(identity)
            else {
                panic!("missing orchestration contract for identity `{identity}`");
            };
            assert!(
                invocation.starts_with("wrela "),
                "invalid orchestration invocation `{invocation}` for `{identity}`"
            );
            assert!(
                !contract.trim().is_empty(),
                "orchestration contract should be non-empty for `{identity}`"
            );
            assert!(
                !required_outputs.is_empty(),
                "required outputs should be non-empty for `{identity}`"
            );
        }
    }

    #[test]
    fn mmo_orchestration_role_evidence_emits_role_metadata_for_ops() {
        let evidence =
            mmo_orchestration_role_evidence("mmo.ops").expect("expected role evidence for mmo.ops");
        assert_eq!(
            evidence.get("role").and_then(|value| value.as_str()),
            Some("ops")
        );
        assert_eq!(
            evidence.get("phase").and_then(|value| value.as_str()),
            Some("gate")
        );
        assert!(
            evidence
                .get("required_outputs")
                .and_then(|value| value.as_array())
                .is_some_and(|outputs| outputs
                    .iter()
                    .any(|output| output.as_str() == Some("test-matrix.json")))
        );
    }

    #[test]
    fn mmo_orchestration_role_evidence_ignores_non_role_identities() {
        assert!(
            mmo_orchestration_role_evidence("studio.gate").is_none(),
            "studio identities should not emit mmo role evidence"
        );
        assert!(
            mmo_orchestration_role_evidence("mmo.build").is_none(),
            "non-role mmo identities should not emit role evidence"
        );
    }

    #[test]
    fn full_factory_phase_resume_is_blocked_when_downstream_invalidated() {
        let mut hashes = BTreeMap::new();
        hashes.insert("artifact.bin".to_string(), "abcd".to_string());
        let phase = FullFactoryCheckpointPhaseV1 {
            status: "passed".to_string(),
            artifact_hashes: hashes,
        };
        assert!(
            !should_resume_full_factory_phase(true, Some(&phase)),
            "downstream invalidation must block resume regardless of checkpoint shape"
        );
    }

    #[test]
    fn full_factory_phase_resume_requires_valid_checkpoint_state() {
        let empty = FullFactoryCheckpointPhaseV1 {
            status: "passed".to_string(),
            artifact_hashes: BTreeMap::new(),
        };
        assert!(
            !should_resume_full_factory_phase(false, Some(&empty)),
            "phase with empty artifact hashes must not be resumable"
        );
        assert!(
            !should_resume_full_factory_phase(false, None),
            "missing phase checkpoint must not be resumable"
        );
    }
}
