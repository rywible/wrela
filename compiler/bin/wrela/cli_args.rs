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
    pub output_format_json: bool,
}

pub fn parse(raw_args: Vec<String>) -> CommandSpec {
    let trace_enabled = std::env::var("WRELA_BUILD_TRACE").is_ok();
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
    let mut output_format_json = false;
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
        if let Some(fmt) = arg.strip_prefix("--format=") {
            output_format_json = fmt == "json";
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
            poll_ms = ms.parse::<u64>().ok();
            continue;
        }
        if let Some(jobs) = arg.strip_prefix("--jobs=") {
            test_jobs = jobs.parse::<usize>().ok();
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--test-timeout-ms=") {
            test_timeout_ms = ms.parse::<u64>().ok();
            continue;
        }
        if arg == "--record" {
            test_record = true;
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
        if arg == "--perf-debug" {
            perf_debug = true;
            continue;
        }
        if let Some(runs) = arg.strip_prefix("--runs=") {
            perf_runs = runs.parse::<usize>().ok();
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
            perf_max_regression_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-cv-max-pct=") {
            perf_cv_max_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-fallback-max=") {
            kpi_check_fallback_max = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-batch-min=") {
            kpi_check_batch_min = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-p99-improve-min-pct=") {
            kpi_scheduler_p99_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-rewrite-overhead-max-pct=") {
            kpi_rewrite_overhead_max_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-actor-throughput-improve-min-pct=") {
            kpi_actor_throughput_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-queue-age-p99-max-regress-pct=") {
            kpi_queue_age_p99_max_regress_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-starvation-violations-max=") {
            kpi_starvation_violations_max = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-throughput-improve-min-pct=") {
            kpi_scheduler_throughput_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-loop-p99-max-regress-pct=") {
            kpi_scheduler_loop_p99_max_regress_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-local-hit-min=") {
            kpi_scheduler_local_hit_min = value.parse::<f64>().ok();
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
            command,
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
            output_format_json,
        }),
    }
}

fn is_command(arg: &str) -> bool {
    matches!(
        arg,
        "init"
            | "update"
            | "check"
            | "build"
            | "compile"
            | "verify-cert"
            | "run"
            | "dev"
            | "test"
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
    }

    #[test]
    fn parse_json_format_and_program_args() {
        let spec = parse(vec![
            "--format=json".to_string(),
            "run".to_string(),
            "apps/ledger-lite".to_string(),
            "--".to_string(),
            "--dry-run".to_string(),
            "value".to_string(),
        ]);
        match spec.parsed {
            ParsedCommandSpec::Ready(parsed) => {
                assert!(parsed.output_format_json);
                assert_eq!(parsed.command, "run");
                assert_eq!(parsed.path_arg.as_deref(), Some("apps/ledger-lite"));
                assert_eq!(
                    parsed.program_args,
                    vec!["--dry-run".to_string(), "value".to_string()]
                );
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
            "build",
            "compile",
            "verify-cert",
            "run",
            "dev",
            "test",
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
}
