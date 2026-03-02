pub(super) fn run_repro_artifact(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    cassette_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
) -> i32 {
    repro::run_repro_artifact(
        workspace_root,
        repro_artifact_path,
        timeout,
        output_format,
        cassette_mode,
        budget_policy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::tempdir;

    fn perf_summary_with_kpis() -> PerfSummary {
        PerfSummary {
            sample_count: 1,
            compile_throughput_tests_per_sec: 100.0,
            runtime_p50_ns: 100,
            runtime_p95_ns: 150,
            runtime_p99_ns: 200,
            allocs_per_request: 1.0,
            rc_inc: 0,
            rc_dec: 0,
            rc_ops_total: 0,
            dispatch_hit_ratio: 1.0,
            check_fallback_rate: Some(0.10),
            avg_check_batch_size: Some(8.0),
            check_oracle_eval_ns_p50: Some(50),
            check_oracle_eval_ns_p95: Some(90),
            effect_annihilation_rewrite_count: Some(2),
            scheduler_dispatch_p99_ns: Some(800),
            scheduler_starvation_violations: Some(0),
            rewrite_compile_overhead_pct: Some(4.0),
            rewrite_applied_count: Some(10),
            actor_msgs_per_sec_p50: Some(1000.0),
            actor_msgs_per_sec_p95: Some(900.0),
            queue_enqueue_p99_ns: Some(100),
            queue_dequeue_p99_ns: Some(120),
            queue_age_p99_ns: Some(150),
            mailbox_wake_coalesced_count: Some(2),
            mailbox_rescue_wake_count: Some(0),
            queue_cas_retry_total: Some(1),
            cases: None,
            metrics: MetricsTotals::default(),
        }
    }

    #[test]
    fn evaluate_perf_gate_applies_kpi_thresholds() {
        let baseline = perf_summary_with_kpis();
        let mut current = perf_summary_with_kpis();
        current.check_fallback_rate = Some(0.25);
        current.avg_check_batch_size = Some(4.0);
        current.scheduler_dispatch_p99_ns = Some(950);
        current.rewrite_compile_overhead_pct = Some(7.5);
        current.actor_msgs_per_sec_p50 = Some(900.0);
        current.queue_age_p99_ns = Some(220);
        current.scheduler_starvation_violations = Some(2);
        let thresholds = KpiThresholds {
            check_fallback_max: Some(0.20),
            check_batch_min: Some(6.0),
            scheduler_p99_improve_min_pct: Some(10.0),
            rewrite_overhead_max_pct: Some(5.0),
            actor_throughput_improve_min_pct: Some(0.0),
            queue_age_p99_max_regress_pct: Some(10.0),
            starvation_violations_max: Some(0.0),
            scheduler_throughput_improve_min_pct: Some(0.0),
            scheduler_loop_p99_max_regress_pct: Some(20.0),
            scheduler_local_hit_min: Some(0.0),
        };

        let failures = evaluate_perf_gate(&current, &baseline, 5.0, &thresholds);

        assert!(
            failures
                .iter()
                .any(|line| line.contains("check_fallback_rate"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("avg_check_batch_size"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("scheduler_dispatch_p99_ns improvement"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("rewrite_compile_overhead_pct"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("actor_msgs_per_sec_p50 improvement"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("queue_age_p99_ns regression"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("scheduler_starvation_violations"))
        );
    }

    #[test]
    fn evaluate_perf_gate_ignores_missing_optional_kpis() {
        let mut baseline = perf_summary_with_kpis();
        let mut current = perf_summary_with_kpis();
        baseline.scheduler_dispatch_p99_ns = None;
        current.scheduler_dispatch_p99_ns = None;
        current.check_fallback_rate = None;
        current.avg_check_batch_size = None;
        current.rewrite_compile_overhead_pct = None;
        let thresholds = KpiThresholds {
            check_fallback_max: Some(0.20),
            check_batch_min: Some(6.0),
            scheduler_p99_improve_min_pct: Some(10.0),
            rewrite_overhead_max_pct: Some(5.0),
            actor_throughput_improve_min_pct: None,
            queue_age_p99_max_regress_pct: None,
            starvation_violations_max: None,
            scheduler_throughput_improve_min_pct: None,
            scheduler_loop_p99_max_regress_pct: None,
            scheduler_local_hit_min: None,
        };

        let failures = evaluate_perf_gate(&current, &baseline, 5.0, &thresholds);

        assert!(failures.is_empty());
    }

    #[test]
    fn sim_seed_expansion_uses_256_seeds_in_cert_mode() {
        let base = TestCase {
            id: "sim-id".to_string(),
            lane: TestLane::Sim,
            name: "tests/sim/foo::test_bar".to_string(),
            module_path: "tests/sim/foo".to_string(),
            func_name: "test_bar".to_string(),
            is_serial: false,
            allows_env_set: false,
            allows_fs_escape: false,
            has_oracle: true,
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: "sim-id".to_string(),
        };
        let expanded = expand_sim_seed_cases(vec![base], None, true);
        assert_eq!(expanded.len(), 256);
        assert_eq!(expanded.first().and_then(|t| t.sim_seed), Some(0));
        assert_eq!(expanded.last().and_then(|t| t.sim_seed), Some(255));
    }

    #[test]
    fn model_seed_expansion_uses_multiple_seeds_in_cert_mode() {
        let base = TestCase {
            id: "model-id".to_string(),
            lane: TestLane::Model,
            name: "tests/model/foo::test_bar".to_string(),
            module_path: "tests/model/foo".to_string(),
            func_name: "test_bar".to_string(),
            is_serial: false,
            allows_env_set: false,
            allows_fs_escape: false,
            has_oracle: true,
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: "model-id".to_string(),
        };
        let expanded = expand_sim_seed_cases(vec![base], None, true);
        assert_eq!(expanded.len(), 64);
        assert_eq!(expanded.first().and_then(|t| t.sim_seed), Some(0));
        assert_eq!(expanded.last().and_then(|t| t.sim_seed), Some(63));
    }

    #[test]
    fn perfcmp_profile_parse_accepts_known_values() {
        assert_eq!(PerfProfile::parse("smoke"), Some(PerfProfile::Smoke));
        assert_eq!(PerfProfile::parse("standard"), Some(PerfProfile::Standard));
        assert_eq!(PerfProfile::parse("deep"), Some(PerfProfile::Deep));
        assert_eq!(PerfProfile::parse("invalid"), None);
    }

    #[test]
    fn manifest_rejects_mismatched_ops_suffix() {
        let path = env::temp_dir().join(format!("wrela-bench-{}.toml", now_unix_ms()));
        let mut file = fs::File::create(&path).expect("create temp manifest");
        writeln!(
            file,
            "version = 1\nsuite = \"micro\"\n\n[[scenarios]]\nid = \"a\"\ntest_name = \"tests/default/micro::test_demo_ops_10\"\nops = 20\nclass = \"critical\"\nallow_unstable = false\n"
        )
        .expect("write manifest");
        let err = load_benchmark_manifest(&path).expect_err("expected ops suffix mismatch");
        assert!(err.contains("must end with"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn integration_mode_entry_path_guard_allows_only_expected_layout() {
        let allowed_composition_entry =
            PathBuf::from("/tmp/wrela-project/src/application/composition/main.wr");
        assert!(integration_mode_entry_path_is_allowed(
            allowed_composition_entry.as_path()
        ));

        let allowed_integration_entry =
            PathBuf::from("/tmp/wrela-project/src/infrastructure/integrations/http_server.wr");
        assert!(integration_mode_entry_path_is_allowed(
            allowed_integration_entry.as_path()
        ));

        let disallowed_application_entry =
            PathBuf::from("/tmp/wrela-project/src/application/service.wr");
        assert!(!integration_mode_entry_path_is_allowed(
            disallowed_application_entry.as_path()
        ));

        let disallowed_root_entry = PathBuf::from("/tmp/wrela-project/main.wr");
        assert!(!integration_mode_entry_path_is_allowed(
            disallowed_root_entry.as_path()
        ));
    }

    #[test]
    fn parse_autogen_call_accepts_function_call_syntax() {
        let (func, args) =
            parse_autogen_call("ready(value=7, enabled=true)").expect("parse canonical call");
        assert_eq!(func, "ready");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].0, "value");
        assert!(matches!(args[0].1, AutogenValue::Integer(7)));
        assert_eq!(args[1].0, "enabled");
        assert!(matches!(args[1].1, AutogenValue::Boolean(true)));
    }

    #[test]
    fn render_autogen_call_uses_function_call_syntax() {
        let rendered = render_autogen_call(
            "ready",
            &[
                ("value".to_string(), AutogenValue::Integer(7)),
                ("enabled".to_string(), AutogenValue::Boolean(true)),
            ],
        );
        assert_eq!(rendered, "ready(value=7, enabled=true)");
        assert_eq!(render_autogen_call("ready", &[]), "ready()");
    }

    #[test]
    fn resolve_format_targets_prefers_src_and_tests_directories() {
        let dir = tempdir().expect("tempdir");
        let src_main = dir.path().join("src").join("main.wr");
        let test_file = dir.path().join("tests").join("sample_test.wr");
        let ignored = dir.path().join("notes").join("scratch.wr");
        std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
        std::fs::create_dir_all(test_file.parent().expect("tests parent")).expect("create tests");
        std::fs::create_dir_all(ignored.parent().expect("ignored parent")).expect("create ignored");
        std::fs::write(&src_main, "to run() -> Integer:\n    return 0\n").expect("write src");
        std::fs::write(&test_file, "to test_sample() -> Nothing:\n    return\n")
            .expect("write test");
        std::fs::write(&ignored, "to run() -> Integer:\n    return 1\n").expect("write ignored");

        let targets = resolve_format_targets(Some(dir.path().to_str().expect("utf8 path")))
            .expect("resolve targets");
        assert!(
            targets.contains(&src_main),
            "expected src target: {targets:?}"
        );
        assert!(
            targets.contains(&test_file),
            "expected test target: {targets:?}"
        );
        assert!(
            !targets.contains(&ignored),
            "expected non src/tests file to be excluded when src/tests exist: {targets:?}"
        );
    }

    #[test]
    fn resolve_format_targets_includes_language_and_apps_for_workspace_root() {
        let dir = tempdir().expect("tempdir");
        let src_main = dir.path().join("src").join("main.wr");
        let test_file = dir.path().join("tests").join("sample_test.wr");
        let language_file = dir.path().join("language").join("spec").join("grammar.wr");
        let app_file = dir.path().join("apps").join("demo").join("main.wr");
        let ignored = dir.path().join("notes").join("scratch.wr");
        std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
        std::fs::create_dir_all(test_file.parent().expect("tests parent")).expect("create tests");
        std::fs::create_dir_all(language_file.parent().expect("language parent"))
            .expect("create language");
        std::fs::create_dir_all(app_file.parent().expect("apps parent")).expect("create apps");
        std::fs::create_dir_all(ignored.parent().expect("ignored parent")).expect("create ignored");
        std::fs::create_dir_all(dir.path().join("compiler")).expect("create compiler marker");
        std::fs::create_dir_all(dir.path().join("runtime")).expect("create runtime marker");
        std::fs::write(&src_main, "to run() -> Integer:\n    return 0\n").expect("write src");
        std::fs::write(&test_file, "to test_sample() -> Nothing:\n    return\n")
            .expect("write tests");
        std::fs::write(&language_file, "to parse() -> Integer:\n    return 1\n")
            .expect("write language");
        std::fs::write(&app_file, "to run_app() -> Integer:\n    return 2\n").expect("write app");
        std::fs::write(&ignored, "to ignored() -> Integer:\n    return 3\n")
            .expect("write ignored");

        let targets = resolve_format_targets(Some(dir.path().to_str().expect("utf8 path")))
            .expect("resolve targets");
        assert!(
            targets.contains(&src_main),
            "expected src target: {targets:?}"
        );
        assert!(
            targets.contains(&test_file),
            "expected tests target: {targets:?}"
        );
        assert!(
            targets.contains(&language_file),
            "expected language target: {targets:?}"
        );
        assert!(
            targets.contains(&app_file),
            "expected app target: {targets:?}"
        );
        assert!(
            !targets.contains(&ignored),
            "expected non-owned file to be excluded when owned roots exist: {targets:?}"
        );
    }

    #[test]
    fn resolve_format_targets_file_input_returns_single_entry() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("src").join("main.wr");
        std::fs::create_dir_all(file.parent().expect("parent")).expect("create parent");
        std::fs::write(&file, "to run() -> Integer:\n    return 0\n").expect("write source");

        let targets =
            resolve_format_targets(Some(file.to_str().expect("utf8 path"))).expect("targets");
        assert_eq!(targets, vec![file]);
    }

    #[test]
    fn boundary_generic_fix_requires_exact_type_name_span_match() {
        let source =
            "to run(items: List[Integer]) -> Bytes:\n    return __wr_bytes_from_list(items)\n";
        let offset = source
            .find("__w")
            .expect("expected builtin call in source text");
        let err = hir::typeck::TypeError::BoundaryMissingTypeArgs {
            name: smol_str::SmolStr::new("Map"),
            span: SourceSpan::from((offset, 3usize)),
        };
        let (_payload, fixes) = type_payload_and_fixes(&err, "src/main.wr", source);
        assert!(
            fixes.is_empty(),
            "unexpected boundary generic rewrite for mismatched span text: {fixes:?}"
        );
    }

    #[test]
    fn boundary_generic_fix_emits_when_span_matches_type_name() {
        let source = "to run(value: Map) -> Integer:\n    return 1\n";
        let offset = source.find("Map").expect("expected `Map` in source text");
        let err = hir::typeck::TypeError::BoundaryMissingTypeArgs {
            name: smol_str::SmolStr::new("Map"),
            span: SourceSpan::from((offset, "Map".len())),
        };
        let (_payload, fixes) = type_payload_and_fixes(&err, "src/main.wr", source);
        assert_eq!(fixes.len(), 1, "expected exactly one boundary rewrite fix");
        assert_eq!(fixes[0].replacement, "Map[String, Integer]");
    }

    #[test]
    fn named_args_rewrite_handles_clean_positional_spans() {
        let source = "push(out, byte_value)";
        let call_start = source.find("push").expect("call start");
        let call_span = SourceSpan::from((call_start, source.len() - call_start));
        let out_start = source.find("out").expect("out arg");
        let value_start = source.find("byte_value").expect("value arg");
        let param_names = vec![
            smol_str::SmolStr::new("items"),
            smol_str::SmolStr::new("value"),
        ];
        let arg_spans = vec![
            SourceSpan::from((out_start, "out".len())),
            SourceSpan::from((value_start, "byte_value".len())),
        ];

        let rewritten = build_named_args_replacement(source, call_span, &param_names, &arg_spans);
        assert_eq!(
            rewritten,
            Some(("push(items=out, value=byte_value)".to_string(), true))
        );
    }

    #[test]
    fn named_args_rewrite_rejects_misaligned_arg_spans() {
        let source = "push(out, byte_value)";
        let call_start = source.find("push").expect("call start");
        let call_span = SourceSpan::from((call_start, source.len() - call_start));
        let open_paren = source.find('(').expect("open paren");
        let comma = source.find(',').expect("comma");
        let param_names = vec![
            smol_str::SmolStr::new("items"),
            smol_str::SmolStr::new("value"),
        ];
        let arg_spans = vec![
            SourceSpan::from((open_paren, 4usize)),
            SourceSpan::from((comma, 11usize)),
        ];

        let rewritten = build_named_args_replacement(source, call_span, &param_names, &arg_spans);
        assert!(
            rewritten.is_none(),
            "misaligned spans must not produce a rewrite"
        );
    }

    #[test]
    fn named_args_rewrite_rejects_already_named_args() {
        let source = "push(items=out, value=byte_value)";
        let call_start = source.find("push").expect("call start");
        let call_span = SourceSpan::from((call_start, source.len() - call_start));
        let first = source.find("items=out").expect("first arg");
        let second = source.find("value=byte_value").expect("second arg");
        let param_names = vec![
            smol_str::SmolStr::new("items"),
            smol_str::SmolStr::new("value"),
        ];
        let arg_spans = vec![
            SourceSpan::from((first, "items=out".len())),
            SourceSpan::from((second, "value=byte_value".len())),
        ];

        let rewritten = build_named_args_replacement(source, call_span, &param_names, &arg_spans);
        assert!(
            rewritten.is_none(),
            "already named args should not be rewritten"
        );
    }

    #[test]
    fn apply_source_fixes_requires_expected_source() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("main.wr");
        std::fs::write(&file, "to run() -> Integer:\n    return 1\n").expect("write source");
        let source = std::fs::read_to_string(&file).expect("read source");
        let offset = source.find("1").expect("find literal");
        let fixes = vec![DiagFix {
            replacement: "2".to_string(),
            span: DiagSpan {
                path: file.display().to_string(),
                offset,
                len: 1,
            },
            expected_source: None,
            rationale: "test".to_string(),
            confidence: 1.0,
            safety_tier: "safe".to_string(),
            reason_code: "test_missing_expected_source".to_string(),
        }];
        let err = apply_source_fixes(&fixes).expect_err("expected strict expected_source failure");
        assert!(
            err.message.contains("missing expected source"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn apply_source_fixes_rejects_expected_source_mismatch() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("main.wr");
        std::fs::write(&file, "to run() -> Integer:\n    return 1\n").expect("write source");
        let source = std::fs::read_to_string(&file).expect("read source");
        let offset = source.find("1").expect("find literal");
        let fixes = vec![DiagFix {
            replacement: "2".to_string(),
            span: DiagSpan {
                path: file.display().to_string(),
                offset,
                len: 1,
            },
            expected_source: Some("9".to_string()),
            rationale: "test".to_string(),
            confidence: 1.0,
            safety_tier: "safe".to_string(),
            reason_code: "test_expected_source_mismatch".to_string(),
        }];
        let err = apply_source_fixes(&fixes).expect_err("expected span mismatch failure");
        assert!(
            err.message.contains("source span mismatch"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn apply_source_fixes_rejects_overlapping_ranges() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("main.wr");
        std::fs::write(&file, "to run() -> Integer:\n    return 10\n").expect("write source");
        let source = std::fs::read_to_string(&file).expect("read source");
        let ten_offset = source.find("10").expect("find ten");
        let zero_offset = source.find('0').expect("find zero");
        let fixes = vec![
            DiagFix {
                replacement: "20".to_string(),
                span: DiagSpan {
                    path: file.display().to_string(),
                    offset: ten_offset,
                    len: 2,
                },
                expected_source: Some("10".to_string()),
                rationale: "test overlap".to_string(),
                confidence: 1.0,
                safety_tier: "safe".to_string(),
                reason_code: "test_overlap_first".to_string(),
            },
            DiagFix {
                replacement: "9".to_string(),
                span: DiagSpan {
                    path: file.display().to_string(),
                    offset: zero_offset,
                    len: 1,
                },
                expected_source: Some("0".to_string()),
                rationale: "test overlap".to_string(),
                confidence: 1.0,
                safety_tier: "safe".to_string(),
                reason_code: "test_overlap_second".to_string(),
            },
        ];
        let err = apply_source_fixes(&fixes).expect_err("expected overlap guard failure");
        assert!(
            err.message.contains("overlapping fixes"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn typed_hole_payload_includes_rank_metadata_and_code_actions() {
        let err = hir::semantic::SemanticError::TypedHole {
            name: smol_str::SmolStr::new("_todo"),
            candidates: vec![
                smol_str::SmolStr::new("alpha"),
                smol_str::SmolStr::new("beta"),
            ],
            span: SourceSpan::from((4usize, 5usize)),
        };
        let mut binding_types = HashMap::new();
        binding_types.insert("alpha".to_string(), "Integer".to_string());
        binding_types.insert("beta".to_string(), "Integer".to_string());

        let (payload, fixes) = semantic_hole_payload_and_fixes(&err, "src/main.wr", &binding_types);
        let payload = payload.expect("typed hole payload");

        assert_eq!(
            payload.get("hole_id").and_then(|value| value.as_str()),
            Some("src/main.wr:4:5:_todo")
        );
        assert_eq!(
            payload
                .get("ranking_strategy")
                .and_then(|value| value.as_str()),
            Some("lexicographic_binding_name")
        );
        assert_eq!(
            payload
                .get("expected_type")
                .and_then(|value| value.as_str()),
            Some("Integer")
        );
        let ranked = payload
            .get("ranked_candidates")
            .and_then(|value| value.as_array())
            .expect("ranked candidates");
        assert_eq!(ranked.len(), 2);
        assert_eq!(
            ranked[0].get("name").and_then(|value| value.as_str()),
            Some("alpha")
        );
        let actions = payload
            .get("code_actions")
            .and_then(|value| value.as_array())
            .expect("code actions");
        assert!(!actions.is_empty(), "expected at least one code action");
        assert_eq!(
            actions[0].get("kind").and_then(|value| value.as_str()),
            Some("fill_typed_hole")
        );
        assert!(!fixes.is_empty(), "expected typed-hole fixes");
    }
}
