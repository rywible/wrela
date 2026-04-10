use wrela::query_plan::DispatchBackend;

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
    pub query_backend: Option<DispatchBackend>,
    pub command: String,
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
    pub orchestration_identity: Option<String>,
    pub analysis_holes_only: bool,
    pub strict_naming: bool,
    pub fix_allow_review_fixes: bool,
    pub workspace_diagnostics: bool,
    pub output_format_human: bool,
    pub output_format_json: bool,
    pub output_format_sarif: bool,
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

fn parse_query_backend_flag(value: &str) -> Result<DispatchBackend, String> {
    match value {
        "cpu" => Ok(DispatchBackend::Cpu),
        "virtual_gpu" => Ok(DispatchBackend::VirtualGpu),
        "wgsl" => Ok(DispatchBackend::Wgsl),
        "auto" => Ok(DispatchBackend::Auto),
        _ => Err(format!(
            "error: invalid --query-backend value `{value}` (expected one of: cpu, virtual_gpu, wgsl, auto)"
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
    let mut query_backend: Option<DispatchBackend> = None;
    let mut command: Option<String> = None;
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
    let orchestration_identity: Option<String> = None;
    let mut analysis_holes_only = false;
    let mut strict_naming = false;
    let mut fix_allow_review_fixes = false;
    let mut workspace_diagnostics = false;
    let mut output_format_human = false;
    let mut output_format_json = false;
    let mut output_format_sarif = false;
    let mut seen_double_dash = false;

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
        if let Some(value) = arg.strip_prefix("--query-backend=") {
            match parse_query_backend_flag(value) {
                Ok(parsed) => query_backend = Some(parsed),
                Err(err) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(err),
                    };
                }
            }
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
    CommandSpec {
        trace_enabled,
        parsed: ParsedCommandSpec::Ready(ParsedArgs {
            emit_mir,
            emit_mir_opt,
            emit_obj,
            emit_bin,
            out_path,
            prefix_path,
            query_backend,
            command,
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
            orchestration_identity,
            analysis_holes_only,
            strict_naming,
            fix_allow_review_fixes,
            workspace_diagnostics,
            output_format_human,
            output_format_json,
            output_format_sarif,
        }),
    }
}

fn is_command(arg: &str) -> bool {
    matches!(
        arg,
        "init"
            | "update"
            | "check"
            | "analyze"
            | "fix"
            | "fmt"
            | "build"
            | "compile"
            | "query-contracts"
            | "verify-cert"
            | "run"
            | "dev"
            | "test"
            | "eval"
            | "perf"
            | "perfcmp"
            | "matrix"
    )
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
    fn parse_accepts_known_commands() {
        for command in [
            "init",
            "update",
            "check",
            "analyze",
            "fix",
            "fmt",
            "build",
            "compile",
            "query-contracts",
            "verify-cert",
            "run",
            "dev",
            "test",
            "eval",
            "perf",
            "perfcmp",
            "matrix",
        ] {
            let spec = parse(vec![command.to_string()]);
            match spec.parsed {
                ParsedCommandSpec::Ready(parsed) => assert_eq!(parsed.command, command),
                other => panic!("command {command} failed parse: {other:?}"),
            }
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
    fn parse_run_query_backend_flag() {
        let spec = parse(vec![
            "run".to_string(),
            "--query-backend=wgsl".to_string(),
            "language/preview".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.query_backend, Some(DispatchBackend::Wgsl));
                assert_eq!(parsed.path_arg.as_deref(), Some("language/preview"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_invalid_query_backend_flag() {
        let spec = parse(vec![
            "run".to_string(),
            "--query-backend=metal".to_string(),
            "language/preview".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Error(err) => {
                assert!(err.contains("invalid --query-backend value"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }
}
