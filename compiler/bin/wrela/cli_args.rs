#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommandSpec {
    Help,
    Version,
    Ready(ParsedArgs),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub trace_enabled: bool,
    pub parsed: ParsedCommandSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgs {
    pub emit_mir: bool,
    pub emit_mir_opt: bool,
    pub emit_obj: Option<String>,
    pub emit_bin: Option<String>,
    pub out_path: Option<String>,
    pub prefix_path: Option<String>,
    pub command: String,
    pub game_mode: bool,
    pub game_build_target: Option<String>,
    pub game_render_backend: Option<String>,
    pub game_host_mode: Option<String>,
    pub game_client_runtime: Option<String>,
    pub game_shader_provenance: bool,
    pub game_no_shortcuts: bool,
    pub game_check_determinism: bool,
    pub game_check_rollback: bool,
    pub game_check_render_lane: bool,
    pub game_check_asset_streaming: bool,
    pub game_anim_objective: Option<String>,
    pub game_profile_gpu_metrics: bool,
    pub game_profile_streaming_metrics: bool,
    pub integration_mode: bool,
    pub path_arg: Option<String>,
    pub program_args: Vec<String>,
    pub poll_ms: Option<u64>,
    pub test_jobs: Option<usize>,
    pub test_timeout_ms: Option<u64>,
    pub test_record: bool,
    pub test_update_public_surface: bool,
    pub test_list: bool,
    pub test_id: Option<String>,
    pub test_filter: Option<String>,
    pub test_lane: Option<String>,
    pub test_seed: Option<u64>,
    pub repro_artifact_path: Option<String>,
    pub replay_trace_path: Option<String>,
    pub perf_debug: bool,
    pub perf_runs: Option<usize>,
    pub perf_baseline_out: Option<String>,
    pub perf_gate_path: Option<String>,
    pub perf_max_regression_pct: Option<f64>,
    pub perf_cv_max_pct: Option<f64>,
    pub kpi_check_fallback_max: Option<f64>,
    pub kpi_check_batch_min: Option<f64>,
    pub kpi_scheduler_p99_improve_min_pct: Option<f64>,
    pub kpi_rewrite_overhead_max_pct: Option<f64>,
    pub kpi_actor_throughput_improve_min_pct: Option<f64>,
    pub kpi_queue_age_p99_max_regress_pct: Option<f64>,
    pub kpi_starvation_violations_max: Option<f64>,
    pub kpi_scheduler_throughput_improve_min_pct: Option<f64>,
    pub kpi_scheduler_loop_p99_max_regress_pct: Option<f64>,
    pub kpi_scheduler_local_hit_min: Option<f64>,
    pub benchmark_manifest_path: Option<String>,
    pub perf_profile_name: Option<String>,
    pub perfcmp_baseline_ref: Option<String>,
    pub perfcmp_candidate_ref: Option<String>,
    pub perfcmp_warmup_pairs: Option<usize>,
    pub perfcmp_measure_pairs: Option<usize>,
    pub perfcmp_min_effect_pct: Option<f64>,
    pub perfcmp_confidence_pct: Option<f64>,
    pub deploy_target: Option<String>,
    pub deploy_app: Option<String>,
    pub deploy_region: Option<String>,
    pub deploy_machines: Option<usize>,
    pub deploy_policy: Option<String>,
    pub deploy_replication_factor: Option<u32>,
    pub deploy_write_quorum: Option<u32>,
    pub deploy_logical_shards: Option<u32>,
    pub deploy_active_groups: Option<u32>,
    pub deploy_force: bool,
    pub deploy_generate_only: bool,
    pub analysis_holes_only: bool,
    pub strict_naming: bool,
    pub fix_allow_review_fixes: bool,
    pub workspace_diagnostics: bool,
    pub orchestration_identity: Option<String>,
    pub agent_run_intent_v2: bool,
    pub output_format_human: bool,
    pub output_format_json: bool,
    pub output_format_sarif: bool,
    pub preview_port: u16,
    pub preview_open: bool,
    pub resolve_dry_run: bool,
    pub resolve_force: bool,
    pub resolve_parallel: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrchestrationMode {
    MmoGateway,
    MmoZone,
    MmoWorld,
    MmoLoadtest,
    MmoOps,
    StudioSynth,
    StudioSynthAssets,
    StudioBake,
    StudioBakeAssets,
    StudioPack,
    StudioPackageAssets,
    StudioGate,
    StudioValidateAssets,
    StudioShip,
    StudioPromoteAssets,
    StudioFullFactory,
}

fn apply_output_format_flag(
    flag: &str,
    fmt: &str,
    output_format_human: &mut bool,
    output_format_json: &mut bool,
    output_format_sarif: &mut bool,
) -> Result<(), String> {
    match fmt {
        "human" => {
            *output_format_human = true;
            *output_format_json = false;
            *output_format_sarif = false;
            Ok(())
        }
        "json" => {
            *output_format_human = false;
            *output_format_json = true;
            *output_format_sarif = false;
            Ok(())
        }
        "sarif" => {
            *output_format_human = false;
            *output_format_json = false;
            *output_format_sarif = true;
            Ok(())
        }
        _ => Err(format!(
            "error: invalid {flag} value `{fmt}` (expected one of: human, json, sarif)"
        )),
    }
}

pub fn parse(raw_args: Vec<String>) -> CommandSpec {
    let trace_enabled = std::env::var("WRELA_BUILD_TRACE").is_ok();
    if raw_args.first().is_some_and(|arg| arg == "help") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Help,
        };
    }
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Help,
        };
    }
    if raw_args.iter().any(|arg| arg == "--version" || arg == "-V") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Version,
        };
    }

    let mut emit_mir = false;
    let mut emit_mir_opt = false;
    let mut emit_obj: Option<String> = None;
    let mut emit_bin: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut prefix_path: Option<String> = None;
    let mut command: Option<String> = None;
    let mut game_mode = false;
    let mut game_build_target: Option<String> = None;
    let mut game_render_backend: Option<String> = None;
    let mut game_host_mode: Option<String> = None;
    let mut game_client_runtime: Option<String> = None;
    let mut game_shader_provenance = false;
    let mut game_no_shortcuts = false;
    let mut game_check_determinism = false;
    let mut game_check_rollback = false;
    let mut game_check_render_lane = false;
    let mut game_check_asset_streaming = false;
    let mut game_anim_objective: Option<String> = None;
    let mut game_profile_gpu_metrics = false;
    let mut game_profile_streaming_metrics = false;
    let mut integration_mode = false;
    let mut path_arg: Option<String> = None;
    let mut program_args: Vec<String> = Vec::new();
    let mut poll_ms: Option<u64> = None;
    let mut test_jobs: Option<usize> = None;
    let mut test_timeout_ms: Option<u64> = None;
    let mut test_record = false;
    let mut test_update_public_surface = false;
    let mut test_list = false;
    let mut test_id: Option<String> = None;
    let mut test_filter: Option<String> = None;
    let mut test_lane: Option<String> = None;
    let mut test_seed: Option<u64> = None;
    let mut repro_artifact_path: Option<String> = None;
    let mut replay_trace_path: Option<String> = None;
    let mut perf_debug = false;
    let mut perf_runs: Option<usize> = None;
    let mut perf_baseline_out: Option<String> = None;
    let mut perf_gate_path: Option<String> = None;
    let mut perf_max_regression_pct: Option<f64> = None;
    let mut perf_cv_max_pct: Option<f64> = None;
    let mut kpi_check_fallback_max: Option<f64> = None;
    let mut kpi_check_batch_min: Option<f64> = None;
    let mut kpi_scheduler_p99_improve_min_pct: Option<f64> = None;
    let mut kpi_rewrite_overhead_max_pct: Option<f64> = None;
    let mut kpi_actor_throughput_improve_min_pct: Option<f64> = None;
    let mut kpi_queue_age_p99_max_regress_pct: Option<f64> = None;
    let mut kpi_starvation_violations_max: Option<f64> = None;
    let mut kpi_scheduler_throughput_improve_min_pct: Option<f64> = None;
    let mut kpi_scheduler_loop_p99_max_regress_pct: Option<f64> = None;
    let mut kpi_scheduler_local_hit_min: Option<f64> = None;
    let mut benchmark_manifest_path: Option<String> = None;
    let mut perf_profile_name: Option<String> = None;
    let mut perfcmp_baseline_ref: Option<String> = None;
    let mut perfcmp_candidate_ref: Option<String> = None;
    let mut perfcmp_warmup_pairs: Option<usize> = None;
    let mut perfcmp_measure_pairs: Option<usize> = None;
    let mut perfcmp_min_effect_pct: Option<f64> = None;
    let mut perfcmp_confidence_pct: Option<f64> = None;
    let mut deploy_target: Option<String> = None;
    let mut deploy_app: Option<String> = None;
    let mut deploy_region: Option<String> = None;
    let mut deploy_machines: Option<usize> = None;
    let mut deploy_policy: Option<String> = None;
    let mut deploy_replication_factor: Option<u32> = None;
    let mut deploy_write_quorum: Option<u32> = None;
    let mut deploy_logical_shards: Option<u32> = None;
    let mut deploy_active_groups: Option<u32> = None;
    let mut deploy_force = false;
    let mut deploy_generate_only = false;
    let mut analysis_holes_only = false;
    let mut strict_naming = false;
    let mut fix_allow_review_fixes = false;
    let mut workspace_diagnostics = false;
    let mut orchestration_identity: Option<String> = None;
    let mut agent_run_intent_v2 = false;
    let mut output_format_human = false;
    let mut output_format_json = false;
    let mut output_format_sarif = false;
    let mut orchestration_mode: Option<OrchestrationMode> = None;
    let mut pending_game_anim_subcommand = false;
    let mut seen_double_dash = false;
    let mut preview_port: u16 = 3030;
    let mut preview_open = false;
    let mut resolve_dry_run = false;
    let mut resolve_force = false;
    let mut resolve_parallel: usize = 4;

    let mut iter = raw_args.into_iter();
    while let Some(arg) = iter.next() {
        if seen_double_dash {
            program_args.push(arg);
            continue;
        }
        if arg == "--" {
            seen_double_dash = true;
            continue;
        }
        if arg == "--json" {
            output_format_human = false;
            output_format_json = true;
            output_format_sarif = false;
            continue;
        }
        if arg == "--determinism" {
            game_check_determinism = true;
            continue;
        }
        if arg == "--rollback" {
            game_check_rollback = true;
            continue;
        }
        if arg == "--render-lane" {
            game_check_render_lane = true;
            continue;
        }
        if arg == "--asset-streaming" {
            game_check_asset_streaming = true;
            continue;
        }
        if arg == "--gpu-metrics" {
            game_profile_gpu_metrics = true;
            continue;
        }
        if arg == "--streaming-metrics" {
            game_profile_streaming_metrics = true;
            continue;
        }
        if arg == "--objective" || arg.starts_with("--objective=") {
            let value = if let Some((_, value)) = arg.split_once('=') {
                value.to_string()
            } else if let Some(value) = iter.next() {
                value
            } else {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(
                        "error: missing value for --objective (expected one of: combat, readability, stability)"
                            .to_string(),
                    ),
                };
            };
            if !matches!(value.as_str(), "combat" | "readability" | "stability") {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(format!(
                        "error: invalid --objective value `{value}` (expected one of: combat, readability, stability)"
                    )),
                };
            }
            game_anim_objective = Some(value);
            continue;
        }
        if arg == "--holes-only" {
            analysis_holes_only = true;
            continue;
        }
        if arg == "--strict-naming" {
            strict_naming = true;
            continue;
        }
        if arg == "--allow-review-fixes" {
            fix_allow_review_fixes = true;
            continue;
        }
        if arg == "--format" || arg.starts_with("--format=") {
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(
                    "error: `--format` was removed; use `--error-format`".to_string(),
                ),
            };
        }
        if arg == "--workspace-diagnostics" {
            workspace_diagnostics = true;
            continue;
        }
        if arg == "--intent-v2" {
            agent_run_intent_v2 = true;
            continue;
        }
        if arg == "--error-format" {
            if let Some(fmt) = iter.next() {
                if let Err(err) = apply_output_format_flag(
                    &arg,
                    &fmt,
                    &mut output_format_human,
                    &mut output_format_json,
                    &mut output_format_sarif,
                ) {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(err),
                    };
                }
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(format!("error: missing value for {arg}")),
            };
        }
        if let Some(fmt) = arg.strip_prefix("--error-format=") {
            if let Err(err) = apply_output_format_flag(
                "--error-format",
                fmt,
                &mut output_format_human,
                &mut output_format_json,
                &mut output_format_sarif,
            ) {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(err),
                };
            }
            continue;
        }
        if arg == "--emit-mir" {
            emit_mir = true;
            continue;
        }
        if arg == "--emit-mir-opt" {
            emit_mir_opt = true;
            continue;
        }
        if let Some(path) = arg.strip_prefix("--emit-obj=") {
            emit_obj = Some(path.to_string());
            continue;
        }
        if let Some(path) = arg.strip_prefix("--emit-bin=") {
            emit_bin = Some(path.to_string());
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--poll-ms=") {
            match ms.parse::<u64>() {
                Ok(parsed) => poll_ms = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --poll-ms value `{ms}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(jobs) = arg.strip_prefix("--jobs=") {
            match jobs.parse::<usize>() {
                Ok(parsed) => test_jobs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --jobs value `{jobs}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--test-timeout-ms=") {
            match ms.parse::<u64>() {
                Ok(parsed) => test_timeout_ms = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --test-timeout-ms value `{ms}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--record" {
            test_record = true;
            continue;
        }
        if arg == "--integration-mode" {
            integration_mode = true;
            continue;
        }
        if arg == "--update-public-surface" {
            test_update_public_surface = true;
            continue;
        }
        if arg == "--list" {
            test_list = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--id=") {
            test_id = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--filter=") {
            test_filter = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--lane=") {
            test_lane = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--seed=") {
            match value.parse::<u64>() {
                Ok(seed) => test_seed = Some(seed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --seed value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--repro=") {
            repro_artifact_path = Some(path.to_string());
            continue;
        }
        if arg == "--repro" {
            if let Some(path) = iter.next() {
                repro_artifact_path = Some(path);
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("error: missing path for --repro".to_string()),
            };
        }
        if let Some(path) = arg.strip_prefix("--replay-trace=") {
            replay_trace_path = Some(path.to_string());
            continue;
        }
        if arg == "--replay-trace" {
            if let Some(path) = iter.next() {
                replay_trace_path = Some(path);
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(
                    "error: missing path for --replay-trace".to_string(),
                ),
            };
        }
        if arg == "--perf-debug" {
            perf_debug = true;
            continue;
        }
        if let Some(runs) = arg.strip_prefix("--runs=") {
            match runs.parse::<usize>() {
                Ok(parsed) => perf_runs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --runs value `{runs}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--baseline-out=") {
            perf_baseline_out = Some(path.to_string());
            continue;
        }
        if let Some(path) = arg.strip_prefix("--perf-gate=") {
            perf_gate_path = Some(path.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-max-regression-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perf_max_regression_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --perf-max-regression-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-cv-max-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perf_cv_max_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --perf-cv-max-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-fallback-max=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_check_fallback_max = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-check-fallback-max value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-batch-min=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_check_batch_min = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-check-batch-min value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-p99-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_p99_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-p99-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-rewrite-overhead-max-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_rewrite_overhead_max_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-rewrite-overhead-max-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-actor-throughput-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_actor_throughput_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-actor-throughput-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-queue-age-p99-max-regress-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_queue_age_p99_max_regress_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-queue-age-p99-max-regress-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-starvation-violations-max=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_starvation_violations_max = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-starvation-violations-max value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-throughput-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_throughput_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-throughput-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-loop-p99-max-regress-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_loop_p99_max_regress_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-loop-p99-max-regress-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-local-hit-min=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_local_hit_min = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-local-hit-min value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--benchmark-manifest=") {
            benchmark_manifest_path = Some(path.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--profile=") {
            perf_profile_name = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--baseline-ref=") {
            perfcmp_baseline_ref = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--candidate-ref=") {
            perfcmp_candidate_ref = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--warmup-pairs=") {
            match value.parse::<usize>() {
                Ok(parsed) => perfcmp_warmup_pairs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --warmup-pairs value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--measure-pairs=") {
            match value.parse::<usize>() {
                Ok(parsed) => perfcmp_measure_pairs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --measure-pairs value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--min-effect-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_min_effect_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --min-effect-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--confidence=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_confidence_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --confidence value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--target=") {
            deploy_target = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--render=") {
            game_render_backend = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--host=") {
            game_host_mode = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--client-runtime=") {
            game_client_runtime = Some(value.to_string());
            continue;
        }
        if arg == "--shader-provenance" {
            game_shader_provenance = true;
            continue;
        }
        if arg == "--no-shortcuts" {
            game_no_shortcuts = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--app=") {
            deploy_app = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--region=") {
            deploy_region = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--machines=") {
            match value.parse::<usize>() {
                Ok(parsed) => deploy_machines = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --machines value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--deploy-policy=") {
            deploy_policy = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--rf=") {
            match value.parse::<u32>() {
                Ok(parsed) => deploy_replication_factor = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --rf value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--wq=") {
            match value.parse::<u32>() {
                Ok(parsed) => deploy_write_quorum = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --wq value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--logical-shards=") {
            match value.parse::<u32>() {
                Ok(parsed) => deploy_logical_shards = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --logical-shards value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--active-groups=") {
            match value.parse::<u32>() {
                Ok(parsed) => deploy_active_groups = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --active-groups value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--force" {
            deploy_force = true;
            resolve_force = true;
            continue;
        }
        if arg == "--generate-only" {
            deploy_generate_only = true;
            continue;
        }
        if arg == "--open" {
            preview_open = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--port=") {
            match value.parse::<u16>() {
                Ok(parsed) => preview_port = parsed,
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --port value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--port" {
            if let Some(value) = iter.next() {
                match value.parse::<u16>() {
                    Ok(parsed) => preview_port = parsed,
                    Err(_) => {
                        return CommandSpec {
                            trace_enabled,
                            parsed: ParsedCommandSpec::Error(format!(
                                "error: invalid --port value `{value}`"
                            )),
                        };
                    }
                }
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("error: missing value for --port".to_string()),
            };
        }
        if arg == "--dry-run" {
            resolve_dry_run = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--parallel=") {
            match value.parse::<usize>() {
                Ok(parsed) => resolve_parallel = parsed,
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --parallel value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--parallel" {
            if let Some(value) = iter.next() {
                match value.parse::<usize>() {
                    Ok(parsed) => resolve_parallel = parsed,
                    Err(_) => {
                        return CommandSpec {
                            trace_enabled,
                            parsed: ParsedCommandSpec::Error(format!(
                                "error: invalid --parallel value `{value}`"
                            )),
                        };
                    }
                }
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("error: missing value for --parallel".to_string()),
            };
        }
        if arg == "--prefix" {
            if let Some(path) = iter.next() {
                prefix_path = Some(path);
            } else {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(
                        "error: missing path for --prefix".to_string(),
                    ),
                };
            }
            continue;
        }
        if arg == "-o" || arg == "--out" {
            if let Some(path) = iter.next() {
                out_path = Some(path);
            } else {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(format!(
                        "error: missing output path for {arg}"
                    )),
                };
            }
            continue;
        }
        if matches!(command.as_deref(), Some("game" | "realtime" | "mmo")) && !game_mode {
            if command.as_deref() == Some("game") && arg == "anim" {
                command = Some("anim".to_string());
                game_mode = true;
                pending_game_anim_subcommand = true;
                continue;
            }
            if is_game_subcommand(&arg) {
                if command.as_deref() == Some("mmo") {
                    orchestration_identity = Some(format!("mmo.{arg}"));
                }
                command = Some(arg);
                game_mode = true;
                continue;
            }
            if command.as_deref() == Some("mmo") {
                if let Some((mode, mapped_command, mapped_game_mode)) =
                    map_mmo_orchestration_subcommand(&arg)
                {
                    orchestration_mode = Some(mode);
                    orchestration_identity = Some(format!("mmo.{arg}"));
                    command = Some(mapped_command.to_string());
                    game_mode = mapped_game_mode;
                    apply_frontend_game_gate_defaults(
                        mapped_command,
                        &mut game_client_runtime,
                        &mut game_shader_provenance,
                        &mut game_no_shortcuts,
                    );
                    apply_orchestration_mode_boolean_defaults(
                        mode,
                        &mut game_check_determinism,
                        &mut game_check_rollback,
                        &mut game_check_render_lane,
                        &mut game_check_asset_streaming,
                        &mut game_profile_gpu_metrics,
                        &mut game_profile_streaming_metrics,
                        &mut agent_run_intent_v2,
                    );
                    continue;
                }
            }
        }
        if pending_game_anim_subcommand {
            match map_game_anim_subcommand(&arg) {
                Some(mapped_command) => {
                    command = Some(mapped_command.to_string());
                    pending_game_anim_subcommand = false;
                    continue;
                }
                None => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid `wrela game anim` subcommand `{arg}` (expected one of: synth, mutate, gate)"
                        )),
                    };
                }
            }
        }
        if matches!(command.as_deref(), Some("frontend")) {
            if let Some((mapped_command, mapped_game_mode)) = map_frontend_subcommand(&arg) {
                command = Some(mapped_command.to_string());
                game_mode = mapped_game_mode;
                apply_frontend_game_gate_defaults(
                    mapped_command,
                    &mut game_client_runtime,
                    &mut game_shader_provenance,
                    &mut game_no_shortcuts,
                );
                continue;
            }
        }
        if matches!(command.as_deref(), Some("studio")) {
            if let Some((mode, mapped_command, mapped_game_mode)) =
                map_studio_orchestration_subcommand(&arg)
            {
                orchestration_mode = Some(mode);
                orchestration_identity = Some(format!("studio.{arg}"));
                command = Some(mapped_command.to_string());
                game_mode = mapped_game_mode;
                apply_frontend_game_gate_defaults(
                    mapped_command,
                    &mut game_client_runtime,
                    &mut game_shader_provenance,
                    &mut game_no_shortcuts,
                );
                apply_orchestration_mode_boolean_defaults(
                    mode,
                    &mut game_check_determinism,
                    &mut game_check_rollback,
                    &mut game_check_render_lane,
                    &mut game_check_asset_streaming,
                    &mut game_profile_gpu_metrics,
                    &mut game_profile_streaming_metrics,
                    &mut agent_run_intent_v2,
                );
                continue;
            }
            if let Some((mapped_command, mapped_game_mode)) = map_frontend_subcommand(&arg) {
                orchestration_identity = Some(format!("studio.{arg}"));
                command = Some(mapped_command.to_string());
                game_mode = mapped_game_mode;
                apply_frontend_game_gate_defaults(
                    mapped_command,
                    &mut game_client_runtime,
                    &mut game_shader_provenance,
                    &mut game_no_shortcuts,
                );
                continue;
            }
        }
        if command.is_none() && is_command(&arg) {
            command = Some(arg);
            continue;
        }
        if path_arg.is_none() {
            path_arg = Some(arg);
        } else {
            program_args.push(arg);
        }
    }

    if pending_game_anim_subcommand {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(
                "error: missing `wrela game anim` subcommand (expected one of: synth, mutate, gate)"
                    .to_string(),
            ),
        };
    }

    if command.is_none() && path_arg.is_some() {
        command = Some("run".to_string());
    }

    let command = match command {
        Some(command) => command,
        None => {
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("__print_help__".to_string()),
            };
        }
    };
    if command == "game" {
        let err = if let Some(subcommand) = path_arg {
            format!(
                "error: invalid `wrela game` subcommand `{subcommand}` (expected one of: init, build, run, dev, check, profile, anim)"
            )
        } else {
            "error: missing `wrela game` subcommand (expected one of: init, build, run, dev, check, profile, anim)"
                .to_string()
        };
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        };
    }
    if command == "realtime" {
        let err = if let Some(subcommand) = path_arg {
            format!(
                "error: invalid `wrela realtime` subcommand `{subcommand}` (expected one of: init, build, run, dev, check, profile)"
            )
        } else {
            "error: missing `wrela realtime` subcommand (expected one of: init, build, run, dev, check, profile)"
                .to_string()
        };
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        };
    }
    if command == "mmo" {
        let err = if let Some(subcommand) = path_arg {
            format!(
                "error: invalid `wrela mmo` subcommand `{subcommand}` (expected one of: init, build, run, dev, check, profile, gateway, zone, world, loadtest, ops)"
            )
        } else {
            "error: missing `wrela mmo` subcommand (expected one of: init, build, run, dev, check, profile, gateway, zone, world, loadtest, ops)"
                .to_string()
        };
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        };
    }
    if command == "frontend" {
        let err = if let Some(subcommand) = path_arg {
            format!(
                "error: invalid `wrela frontend` subcommand `{subcommand}` (expected one of: init, build, check, run, preview, explain, fix)"
            )
        } else {
            "error: missing `wrela frontend` subcommand (expected one of: init, build, check, run, preview, explain, fix)"
                .to_string()
        };
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        };
    }
    if command == "studio" {
        let err = if let Some(subcommand) = path_arg {
            format!(
                "error: invalid `wrela studio` subcommand `{subcommand}` (expected one of: init, build, check, run, preview, explain, fix, synth, bake, pack, gate, ship, synth-assets, bake-assets, validate-assets, package-assets, promote-assets, full-factory)"
            )
        } else {
            "error: missing `wrela studio` subcommand (expected one of: init, build, check, run, preview, explain, fix, synth, bake, pack, gate, ship, synth-assets, bake-assets, validate-assets, package-assets, promote-assets, full-factory)"
                .to_string()
        };
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        };
    }
    if game_mode && command == "build" {
        game_build_target = deploy_target.take();
    }
    if let Some(mode) = orchestration_mode {
        if let Err(err) = enforce_orchestration_mode_hard_defaults(
            mode,
            &mut game_build_target,
            &mut game_render_backend,
            &mut game_host_mode,
            &mut game_client_runtime,
        ) {
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(err),
            };
        }
    }
    if game_anim_objective.is_some() && command != "anim-mutate" {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(
                "error: --objective is only valid with `wrela game anim mutate`".to_string(),
            ),
        };
    }
    if command == "anim-mutate" {
        let objective = game_anim_objective
            .clone()
            .unwrap_or_else(|| "combat".to_string());
        game_anim_objective = Some(objective.clone());
        game_build_target = Some(objective);
    }

    CommandSpec {
        trace_enabled,
        parsed: ParsedCommandSpec::Ready(ParsedArgs {
            emit_mir,
            emit_mir_opt,
            emit_obj,
            emit_bin,
            out_path,
            prefix_path,
            command,
            game_mode,
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
            game_anim_objective,
            game_profile_gpu_metrics,
            game_profile_streaming_metrics,
            integration_mode,
            path_arg,
            program_args,
            poll_ms,
            test_jobs,
            test_timeout_ms,
            test_record,
            test_update_public_surface,
            test_list,
            test_id,
            test_filter,
            test_lane,
            test_seed,
            repro_artifact_path,
            replay_trace_path,
            perf_debug,
            perf_runs,
            perf_baseline_out,
            perf_gate_path,
            perf_max_regression_pct,
            perf_cv_max_pct,
            kpi_check_fallback_max,
            kpi_check_batch_min,
            kpi_scheduler_p99_improve_min_pct,
            kpi_rewrite_overhead_max_pct,
            kpi_actor_throughput_improve_min_pct,
            kpi_queue_age_p99_max_regress_pct,
            kpi_starvation_violations_max,
            kpi_scheduler_throughput_improve_min_pct,
            kpi_scheduler_loop_p99_max_regress_pct,
            kpi_scheduler_local_hit_min,
            benchmark_manifest_path,
            perf_profile_name,
            perfcmp_baseline_ref,
            perfcmp_candidate_ref,
            perfcmp_warmup_pairs,
            perfcmp_measure_pairs,
            perfcmp_min_effect_pct,
            perfcmp_confidence_pct,
            deploy_target,
            deploy_app,
            deploy_region,
            deploy_machines,
            deploy_policy,
            deploy_replication_factor,
            deploy_write_quorum,
            deploy_logical_shards,
            deploy_active_groups,
            deploy_force,
            deploy_generate_only,
            analysis_holes_only,
            strict_naming,
            fix_allow_review_fixes,
            workspace_diagnostics,
            orchestration_identity,
            agent_run_intent_v2,
            output_format_human,
            output_format_json,
            output_format_sarif,
            preview_port,
            preview_open,
            resolve_dry_run,
            resolve_force,
            resolve_parallel,
        }),
    }
}

fn is_command(arg: &str) -> bool {
    matches!(
        arg,
        "init"
            | "game"
            | "realtime"
            | "mmo"
            | "frontend"
            | "studio"
            | "agent-run"
            | "update"
            | "check"
            | "analyze"
            | "fix"
            | "fmt"
            | "build"
            | "compile"
            | "verify-cert"
            | "run"
            | "dev"
            | "test"
            | "eval"
            | "perf"
            | "perfcmp"
            | "matrix"
            | "deploy"
            | "preview"
            | "resolve"
    )
}

fn is_game_subcommand(arg: &str) -> bool {
    matches!(arg, "init" | "build" | "run" | "dev" | "check" | "profile")
}

fn map_game_anim_subcommand(arg: &str) -> Option<&'static str> {
    match arg {
        "synth" => Some("anim-synth"),
        "mutate" => Some("anim-mutate"),
        "gate" => Some("anim-gate"),
        _ => None,
    }
}

fn map_mmo_orchestration_subcommand(arg: &str) -> Option<(OrchestrationMode, &'static str, bool)> {
    match arg {
        "gateway" => Some((OrchestrationMode::MmoGateway, "run", true)),
        "zone" => Some((OrchestrationMode::MmoZone, "run", true)),
        "world" => Some((OrchestrationMode::MmoWorld, "run", true)),
        "loadtest" => Some((OrchestrationMode::MmoLoadtest, "profile", true)),
        "ops" => Some((OrchestrationMode::MmoOps, "check", true)),
        _ => None,
    }
}

fn map_studio_orchestration_subcommand(
    arg: &str,
) -> Option<(OrchestrationMode, &'static str, bool)> {
    match arg {
        "synth" => Some((OrchestrationMode::StudioSynth, "agent-run", false)),
        "synth-assets" => Some((OrchestrationMode::StudioSynthAssets, "agent-run", false)),
        "bake" => Some((OrchestrationMode::StudioBake, "build", true)),
        "bake-assets" => Some((OrchestrationMode::StudioBakeAssets, "build", true)),
        "pack" => Some((OrchestrationMode::StudioPack, "build", true)),
        "package-assets" => Some((OrchestrationMode::StudioPackageAssets, "build", true)),
        "gate" => Some((OrchestrationMode::StudioGate, "check", true)),
        "validate-assets" => Some((OrchestrationMode::StudioValidateAssets, "check", true)),
        "ship" => Some((OrchestrationMode::StudioShip, "run", true)),
        "promote-assets" => Some((OrchestrationMode::StudioPromoteAssets, "run", true)),
        "full-factory" => Some((OrchestrationMode::StudioFullFactory, "agent-run", false)),
        _ => None,
    }
}

fn map_frontend_subcommand(arg: &str) -> Option<(&'static str, bool)> {
    match arg {
        "init" => Some(("init", true)),
        "build" => Some(("build", true)),
        "check" => Some(("check", true)),
        "run" => Some(("run", true)),
        "fix" => Some(("frontend-fix", false)),
        "preview" => Some(("frontend-preview", false)),
        "explain" => Some(("frontend-explain", false)),
        _ => None,
    }
}

fn apply_frontend_game_gate_defaults(
    mapped_command: &str,
    game_client_runtime: &mut Option<String>,
    game_shader_provenance: &mut bool,
    game_no_shortcuts: &mut bool,
) {
    if matches!(mapped_command, "build" | "check" | "run") {
        if game_client_runtime.is_none() {
            *game_client_runtime = Some("compiled".to_string());
        }
        *game_shader_provenance = true;
        *game_no_shortcuts = true;
    }
}

fn apply_studio_gate_check_suite_defaults(
    game_check_determinism: &mut bool,
    game_check_rollback: &mut bool,
    game_check_render_lane: &mut bool,
    game_check_asset_streaming: &mut bool,
) {
    *game_check_determinism = true;
    *game_check_rollback = true;
    *game_check_render_lane = true;
    *game_check_asset_streaming = true;
}

fn apply_orchestration_mode_boolean_defaults(
    mode: OrchestrationMode,
    game_check_determinism: &mut bool,
    game_check_rollback: &mut bool,
    game_check_render_lane: &mut bool,
    game_check_asset_streaming: &mut bool,
    game_profile_gpu_metrics: &mut bool,
    game_profile_streaming_metrics: &mut bool,
    agent_run_intent_v2: &mut bool,
) {
    match mode {
        OrchestrationMode::MmoLoadtest => {
            *game_profile_gpu_metrics = true;
            *game_profile_streaming_metrics = true;
        }
        OrchestrationMode::MmoOps
        | OrchestrationMode::StudioGate
        | OrchestrationMode::StudioValidateAssets => {
            apply_studio_gate_check_suite_defaults(
                game_check_determinism,
                game_check_rollback,
                game_check_render_lane,
                game_check_asset_streaming,
            );
        }
        OrchestrationMode::StudioSynth
        | OrchestrationMode::StudioSynthAssets
        | OrchestrationMode::StudioFullFactory => {
            *agent_run_intent_v2 = true;
        }
        _ => {}
    }
}

fn set_or_require_exact_mode_value(
    value: &mut Option<String>,
    expected: &str,
    flag: &str,
    invocation: &str,
) -> Result<(), String> {
    match value.as_deref() {
        Some(actual) if actual != expected => Err(format!(
            "error: `{invocation}` enforces {flag}={expected} (received {flag}={actual})"
        )),
        _ => {
            *value = Some(expected.to_string());
            Ok(())
        }
    }
}

fn enforce_orchestration_mode_hard_defaults(
    mode: OrchestrationMode,
    game_build_target: &mut Option<String>,
    game_render_backend: &mut Option<String>,
    game_host_mode: &mut Option<String>,
    game_client_runtime: &mut Option<String>,
) -> Result<(), String> {
    let invocation = match mode {
        OrchestrationMode::MmoGateway => "wrela mmo gateway",
        OrchestrationMode::MmoZone => "wrela mmo zone",
        OrchestrationMode::MmoWorld => "wrela mmo world",
        OrchestrationMode::MmoLoadtest => "wrela mmo loadtest",
        OrchestrationMode::MmoOps => "wrela mmo ops",
        OrchestrationMode::StudioSynth => "wrela studio synth",
        OrchestrationMode::StudioSynthAssets => "wrela studio synth-assets",
        OrchestrationMode::StudioBake => "wrela studio bake",
        OrchestrationMode::StudioBakeAssets => "wrela studio bake-assets",
        OrchestrationMode::StudioPack => "wrela studio pack",
        OrchestrationMode::StudioPackageAssets => "wrela studio package-assets",
        OrchestrationMode::StudioGate => "wrela studio gate",
        OrchestrationMode::StudioValidateAssets => "wrela studio validate-assets",
        OrchestrationMode::StudioShip => "wrela studio ship",
        OrchestrationMode::StudioPromoteAssets => "wrela studio promote-assets",
        OrchestrationMode::StudioFullFactory => "wrela studio full-factory",
    };

    if !matches!(
        mode,
        OrchestrationMode::StudioSynth
            | OrchestrationMode::StudioSynthAssets
            | OrchestrationMode::StudioFullFactory
    ) {
        set_or_require_exact_mode_value(game_render_backend, "webgpu", "--render", invocation)?;
        set_or_require_exact_mode_value(game_host_mode, "pure-wasm", "--host", invocation)?;
    }

    if matches!(
        mode,
        OrchestrationMode::MmoGateway
            | OrchestrationMode::MmoZone
            | OrchestrationMode::MmoWorld
            | OrchestrationMode::MmoOps
            | OrchestrationMode::StudioBake
            | OrchestrationMode::StudioBakeAssets
            | OrchestrationMode::StudioPack
            | OrchestrationMode::StudioPackageAssets
            | OrchestrationMode::StudioGate
            | OrchestrationMode::StudioValidateAssets
            | OrchestrationMode::StudioShip
            | OrchestrationMode::StudioPromoteAssets
    ) {
        set_or_require_exact_mode_value(
            game_client_runtime,
            "compiled",
            "--client-runtime",
            invocation,
        )?;
    }

    match mode {
        OrchestrationMode::StudioBake | OrchestrationMode::StudioBakeAssets => {
            set_or_require_exact_mode_value(game_build_target, "native", "--target", invocation)?;
        }
        OrchestrationMode::StudioPack | OrchestrationMode::StudioPackageAssets => {
            set_or_require_exact_mode_value(game_build_target, "wasm", "--target", invocation)?;
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_preserves_argument_order() {
        let args = vec![
            "test".to_string(),
            "apps/ledger-lite".to_string(),
            "--list".to_string(),
        ];
        let spec = parse(args.clone());
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "test");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/ledger-lite"));
                assert!(parsed.test_list);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_detects_invalid_seed() {
        let spec = parse(vec!["test".to_string(), "--seed=abc".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --seed value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_help_control() {
        let spec = parse(vec!["--help".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Help));
    }

    #[test]
    fn parse_help_command_alias() {
        let spec = parse(vec!["help".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Help));
    }

    #[test]
    fn parse_version_control() {
        let spec = parse(vec!["--version".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Version));
    }

    #[test]
    fn parse_repro_requires_value() {
        let spec = parse(vec!["test".to_string(), "--repro".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert!(err.contains("missing path for --repro")),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_malformed_numeric_values() {
        let spec = parse(vec!["perfcmp".to_string(), "--confidence=x".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert!(err.contains("invalid --confidence value")),
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["test".to_string(), "--jobs=abc".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert!(err.contains("invalid --jobs value")),
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["dev".to_string(), "--poll-ms=fast".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert!(err.contains("invalid --poll-ms value")),
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "perf".to_string(),
            "--kpi-check-batch-min=nope".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --kpi-check-batch-min value"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_json_format_and_program_args() {
        let spec = parse(vec![
            "--error-format=json".to_string(),
            "run".to_string(),
            "apps/ledger-lite".to_string(),
            "--".to_string(),
            "--dry-run".to_string(),
            "value".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.output_format_human);
                assert!(parsed.output_format_json);
                assert!(!parsed.output_format_sarif);
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/ledger-lite"));
                assert!(!parsed.integration_mode);
                assert_eq!(
                    parsed.program_args,
                    vec!["--dry-run".to_string(), "value".to_string()]
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_sarif_format_and_program_args() {
        let spec = parse(vec![
            "--error-format=sarif".to_string(),
            "check".to_string(),
            "apps/ledger-lite/src/main.wr".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.output_format_human);
                assert!(!parsed.output_format_json);
                assert!(parsed.output_format_sarif);
                assert_eq!(parsed.command, "check");
                assert_eq!(
                    parsed.path_arg.as_deref(),
                    Some("apps/ledger-lite/src/main.wr")
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_json_shorthand_sets_json_output_format() {
        let spec = parse(vec![
            "--json".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.output_format_human);
                assert!(parsed.output_format_json);
                assert!(!parsed.output_format_sarif);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_holes_only_flag() {
        let spec = parse(vec![
            "--holes-only".to_string(),
            "analyze".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.analysis_holes_only);
                assert_eq!(parsed.command, "analyze");
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_allow_review_fixes_flag() {
        let spec = parse(vec![
            "--allow-review-fixes".to_string(),
            "fix".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.fix_allow_review_fixes);
                assert_eq!(parsed.command, "fix");
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_error_format_human_sets_pretty_output() {
        let spec = parse(vec![
            "--error-format=human".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.output_format_human);
                assert!(!parsed.output_format_json);
                assert!(!parsed.output_format_sarif);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_output_format_last_flag_wins() {
        let spec = parse(vec![
            "--json".to_string(),
            "--error-format=sarif".to_string(),
            "--error-format=json".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.output_format_human);
                assert!(parsed.output_format_json);
                assert!(!parsed.output_format_sarif);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "--error-format=json".to_string(),
            "--json".to_string(),
            "--error-format=sarif".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.output_format_human);
                assert!(!parsed.output_format_json);
                assert!(parsed.output_format_sarif);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_invalid_error_format_value() {
        let spec = parse(vec![
            "--error-format=wat".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --error-format value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_workspace_diagnostics_flag() {
        let spec = parse(vec![
            "--workspace-diagnostics".to_string(),
            "fmt".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.workspace_diagnostics);
                assert_eq!(parsed.command, "fmt");
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_removed_format_alias() {
        let spec = parse(vec![
            "--format=json".to_string(),
            "check".to_string(),
            ".".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("`--format` was removed"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_requires_command() {
        let spec = parse(Vec::new());
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert_eq!(err, "__print_help__"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_game_build_target_and_mode() {
        let spec = parse(vec![
            "game".to_string(),
            "build".to_string(),
            "apps/demo".to_string(),
            "--target=wasm".to_string(),
            "--client-runtime=compiled".to_string(),
            "--shader-provenance".to_string(),
            "--no-shortcuts".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.deploy_target.is_none());
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_realtime_build_target_and_mode() {
        let spec = parse(vec![
            "realtime".to_string(),
            "build".to_string(),
            "apps/demo".to_string(),
            "--target=dual".to_string(),
            "--client-runtime=compiled".to_string(),
            "--shader-provenance".to_string(),
            "--no-shortcuts".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("dual"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.deploy_target.is_none());
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_game_check_flags() {
        let spec = parse(vec![
            "game".to_string(),
            "check".to_string(),
            "apps/demo".to_string(),
            "--client-runtime=compiled".to_string(),
            "--shader-provenance".to_string(),
            "--no-shortcuts".to_string(),
            "--determinism".to_string(),
            "--rollback".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "check");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.game_check_determinism);
                assert!(parsed.game_check_rollback);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_game_anim_subcommands() {
        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "synth".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "anim-synth");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "anim-mutate");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_anim_objective.as_deref(), Some("combat"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("combat"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
            "--objective=combat".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "anim-mutate");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_anim_objective.as_deref(), Some("combat"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("combat"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
            "--objective=readability".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "anim-mutate");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_anim_objective.as_deref(), Some("readability"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("readability"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
            "--objective".to_string(),
            "stability".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "anim-mutate");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_anim_objective.as_deref(), Some("stability"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("stability"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_game_anim_requires_known_subcommand() {
        let spec = parse(vec!["game".to_string(), "anim".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela game anim` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "ship".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela game anim` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
            "--objective".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing value for --objective"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "check".to_string(),
            "apps/demo".to_string(),
            "--objective=combat".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("only valid with `wrela game anim mutate`"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec![
            "game".to_string(),
            "anim".to_string(),
            "mutate".to_string(),
            "apps/demo".to_string(),
            "--objective=cinematic".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --objective value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_game_requires_known_subcommand() {
        let spec = parse(vec!["game".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela game` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["game".to_string(), "ship".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela game` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_realtime_requires_known_subcommand() {
        let spec = parse(vec!["realtime".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela realtime` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["realtime".to_string(), "ship".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela realtime` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_requires_known_subcommand() {
        let spec = parse(vec!["mmo".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela mmo` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["mmo".to_string(), "ship".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela mmo` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_build_maps_to_game_mode() {
        let spec = parse(vec![
            "frontend".to_string(),
            "build".to_string(),
            "apps/demo".to_string(),
            "--target=wasm".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_fix_maps_to_frontend_fix_command() {
        let spec = parse(vec![
            "frontend".to_string(),
            "fix".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.game_mode);
                assert_eq!(parsed.command, "frontend-fix");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_preview_maps_to_internal_command() {
        let spec = parse(vec![
            "frontend".to_string(),
            "preview".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.game_mode);
                assert_eq!(parsed.command, "frontend-preview");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_check_defaults_strict_gate_flags() {
        let spec = parse(vec![
            "frontend".to_string(),
            "check".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "check");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_run_defaults_strict_gate_flags() {
        let spec = parse(vec![
            "frontend".to_string(),
            "run".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_requires_known_subcommand() {
        let spec = parse(vec!["frontend".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela frontend` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["frontend".to_string(), "ship".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela frontend` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_requires_known_subcommand() {
        let spec = parse(vec!["studio".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing `wrela studio` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }

        let spec = parse(vec!["studio".to_string(), "launch".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela studio` subcommand"))
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_build_maps_to_game_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "build".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.build")
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_synth_maps_to_agent_run_with_implicit_intent_v2() {
        let spec = parse(vec![
            "studio".to_string(),
            "synth".to_string(),
            "apps/wrela-game-slice".to_string(),
            "ship".to_string(),
            "it".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.game_mode);
                assert_eq!(parsed.command, "agent-run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.synth")
                );
                assert!(parsed.agent_run_intent_v2);
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert_eq!(
                    parsed.program_args,
                    vec!["ship".to_string(), "it".to_string()]
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_synth_assets_maps_to_agent_run_with_implicit_intent_v2() {
        let spec = parse(vec![
            "studio".to_string(),
            "synth-assets".to_string(),
            "apps/wrela-game-slice".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.game_mode);
                assert_eq!(parsed.command, "agent-run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.synth-assets")
                );
                assert!(parsed.agent_run_intent_v2);
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert!(parsed.program_args.is_empty());
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_full_factory_maps_to_agent_run_with_implicit_intent_v2() {
        let spec = parse(vec![
            "studio".to_string(),
            "full-factory".to_string(),
            "apps/wrela-game-slice".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(!parsed.game_mode);
                assert_eq!(parsed.command, "agent-run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.full-factory")
                );
                assert!(parsed.agent_run_intent_v2);
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert!(parsed.program_args.is_empty());
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_gate_maps_to_strict_check_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "gate".to_string(),
            "apps/wrela-game-slice".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "check");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.gate")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.game_check_determinism);
                assert!(parsed.game_check_rollback);
                assert!(parsed.game_check_render_lane);
                assert!(parsed.game_check_asset_streaming);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_validate_assets_maps_to_strict_check_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "validate-assets".to_string(),
            "apps/wrela-game-slice".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "check");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.validate-assets")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.game_check_determinism);
                assert!(parsed.game_check_rollback);
                assert!(parsed.game_check_render_lane);
                assert!(parsed.game_check_asset_streaming);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_build_maps_to_game_mode() {
        let spec = parse(vec![
            "mmo".to_string(),
            "build".to_string(),
            "apps/demo".to_string(),
            "--target=dual".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("dual"));
                assert_eq!(parsed.orchestration_identity.as_deref(), Some("mmo.build"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_gateway_maps_to_run_mode() {
        let spec = parse(vec![
            "mmo".to_string(),
            "gateway".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("mmo.gateway")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(!parsed.game_profile_gpu_metrics);
                assert!(!parsed.game_profile_streaming_metrics);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_zone_maps_to_run_mode() {
        let spec = parse(vec![
            "mmo".to_string(),
            "zone".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.orchestration_identity.as_deref(), Some("mmo.zone"));
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(!parsed.game_profile_gpu_metrics);
                assert!(!parsed.game_profile_streaming_metrics);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_world_maps_to_run_mode() {
        let spec = parse(vec![
            "mmo".to_string(),
            "world".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.orchestration_identity.as_deref(), Some("mmo.world"));
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(!parsed.game_profile_gpu_metrics);
                assert!(!parsed.game_profile_streaming_metrics);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_loadtest_maps_to_profile_with_metrics() {
        let spec = parse(vec![
            "mmo".to_string(),
            "loadtest".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "profile");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("mmo.loadtest")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert!(parsed.game_client_runtime.is_none());
                assert!(parsed.game_profile_gpu_metrics);
                assert!(parsed.game_profile_streaming_metrics);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_mmo_ops_maps_to_strict_check_orchestration_mode() {
        let spec = parse(vec![
            "mmo".to_string(),
            "ops".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "check");
                assert_eq!(parsed.orchestration_identity.as_deref(), Some("mmo.ops"));
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
                assert!(parsed.game_check_determinism);
                assert!(parsed.game_check_rollback);
                assert!(parsed.game_check_render_lane);
                assert!(parsed.game_check_asset_streaming);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_bake_maps_to_strict_native_build_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "bake".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.bake")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("native"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_bake_assets_maps_to_strict_native_build_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "bake-assets".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.bake-assets")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("native"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_pack_maps_to_strict_wasm_build_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "pack".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.pack")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("wasm"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_package_assets_maps_to_strict_wasm_build_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "package-assets".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "build");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.package-assets")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_build_target.as_deref(), Some("wasm"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_ship_maps_to_strict_run_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "ship".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.ship")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
                assert!(parsed.game_shader_provenance);
                assert!(parsed.game_no_shortcuts);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_promote_assets_maps_to_strict_run_mode() {
        let spec = parse(vec![
            "studio".to_string(),
            "promote-assets".to_string(),
            "apps/demo".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.game_mode);
                assert_eq!(parsed.command, "run");
                assert_eq!(
                    parsed.orchestration_identity.as_deref(),
                    Some("studio.promote-assets")
                );
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo"));
                assert_eq!(parsed.game_render_backend.as_deref(), Some("webgpu"));
                assert_eq!(parsed.game_host_mode.as_deref(), Some("pure-wasm"));
                assert_eq!(parsed.game_client_runtime.as_deref(), Some("compiled"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_studio_pack_rejects_conflicting_target() {
        let spec = parse(vec![
            "studio".to_string(),
            "pack".to_string(),
            "apps/demo".to_string(),
            "--target=dual".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("wrela studio pack"));
                assert!(err.contains("--target=wasm"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frontend_dev_rejected_as_invalid_subcommand() {
        let spec = parse(vec!["frontend".to_string(), "dev".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid `wrela frontend` subcommand `dev`"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_agent_run_maps_path_and_prompt_arguments() {
        let spec = parse(vec![
            "agent-run".to_string(),
            "apps/wrela-game-slice".to_string(),
            "make".to_string(),
            "me".to_string(),
            "a".to_string(),
            "game".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "agent-run");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert!(parsed.orchestration_identity.is_none());
                assert!(!parsed.agent_run_intent_v2);
                assert_eq!(
                    parsed.program_args,
                    vec![
                        "make".to_string(),
                        "me".to_string(),
                        "a".to_string(),
                        "game".to_string(),
                    ]
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_agent_run_accepts_intent_v2_flag() {
        let spec = parse(vec![
            "agent-run".to_string(),
            "--intent-v2".to_string(),
            "apps/wrela-game-slice".to_string(),
            "ship".to_string(),
            "it".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "agent-run");
                assert!(parsed.agent_run_intent_v2);
                assert!(parsed.orchestration_identity.is_none());
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/wrela-game-slice"));
                assert_eq!(
                    parsed.program_args,
                    vec!["ship".to_string(), "it".to_string()]
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_accepts_known_commands() {
        for command in [
            "init",
            "agent-run",
            "update",
            "check",
            "analyze",
            "fix",
            "fmt",
            "build",
            "compile",
            "verify-cert",
            "run",
            "dev",
            "test",
            "eval",
            "perf",
            "perfcmp",
            "matrix",
            "deploy",
            "preview",
            "resolve",
        ] {
            let spec = parse(vec![command.to_string()]);
            match spec.parsed {
                ParsedCommandSpec::Ready(parsed) => assert_eq!(parsed.command, command),
                other => panic!("command {command} failed parse: {other:?}"),
            }
        }
    }

    #[test]
    fn parse_deploy_flags() {
        let spec = parse(vec![
            "deploy".to_string(),
            ".".to_string(),
            "--target=fly".to_string(),
            "--app=my-app".to_string(),
            "--region=ord".to_string(),
            "--machines=3".to_string(),
            "--deploy-policy=infra/wrela.deploy.toml".to_string(),
            "--rf=3".to_string(),
            "--wq=2".to_string(),
            "--logical-shards=64".to_string(),
            "--active-groups=8".to_string(),
            "--force".to_string(),
            "--generate-only".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "deploy");
                assert_eq!(parsed.path_arg.as_deref(), Some("."));
                assert_eq!(parsed.deploy_target.as_deref(), Some("fly"));
                assert_eq!(parsed.deploy_app.as_deref(), Some("my-app"));
                assert_eq!(parsed.deploy_region.as_deref(), Some("ord"));
                assert_eq!(parsed.deploy_machines, Some(3));
                assert_eq!(
                    parsed.deploy_policy.as_deref(),
                    Some("infra/wrela.deploy.toml")
                );
                assert_eq!(parsed.deploy_replication_factor, Some(3));
                assert_eq!(parsed.deploy_write_quorum, Some(2));
                assert_eq!(parsed.deploy_logical_shards, Some(64));
                assert_eq!(parsed.deploy_active_groups, Some(8));
                assert!(parsed.deploy_force);
                assert!(parsed.deploy_generate_only);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_run_integration_mode_flag() {
        let spec = parse(vec![
            "run".to_string(),
            "--integration-mode".to_string(),
            "src/application/composition/main.wr".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "run");
                assert!(parsed.integration_mode);
                assert_eq!(
                    parsed.path_arg.as_deref(),
                    Some("src/application/composition/main.wr")
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_preview_command() {
        let spec = parse(vec![
            "preview".to_string(),
            "apps/demo/".to_string(),
            "--port".to_string(),
            "4000".to_string(),
            "--open".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "preview");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo/"));
                assert_eq!(parsed.preview_port, 4000);
                assert!(parsed.preview_open);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_preview_defaults() {
        let spec = parse(vec!["preview".to_string(), "apps/demo/".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "preview");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo/"));
                assert_eq!(parsed.preview_port, 3030);
                assert!(!parsed.preview_open);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_preview_port_equals_syntax() {
        let spec = parse(vec![
            "preview".to_string(),
            "apps/demo/".to_string(),
            "--port=8080".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "preview");
                assert_eq!(parsed.preview_port, 8080);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_preview_invalid_port() {
        let spec = parse(vec!["preview".to_string(), "--port=abc".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --port value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_preview_port_missing_value() {
        let spec = parse(vec!["preview".to_string(), "--port".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing value for --port"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_resolve_command() {
        let spec = parse(vec![
            "resolve".to_string(),
            "apps/demo/".to_string(),
            "--dry-run".to_string(),
            "--force".to_string(),
            "--parallel=8".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "resolve");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo/"));
                assert!(parsed.resolve_dry_run);
                assert!(parsed.resolve_force);
                assert_eq!(parsed.resolve_parallel, 8);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_resolve_defaults() {
        let spec = parse(vec!["resolve".to_string(), "apps/demo/".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "resolve");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/demo/"));
                assert!(!parsed.resolve_dry_run);
                assert!(!parsed.resolve_force);
                assert_eq!(parsed.resolve_parallel, 4);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_resolve_parallel_separate_value() {
        let spec = parse(vec![
            "resolve".to_string(),
            "apps/demo/".to_string(),
            "--parallel".to_string(),
            "16".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.resolve_parallel, 16);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_resolve_invalid_parallel() {
        let spec = parse(vec!["resolve".to_string(), "--parallel=xyz".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --parallel value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_resolve_parallel_missing_value() {
        let spec = parse(vec!["resolve".to_string(), "--parallel".to_string()]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("missing value for --parallel"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }
}
