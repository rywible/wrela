//! Owns standalone repro artifact execution and artifact creation helpers used
//! by certification and debugging flows.
//! Does not own CLI parsing, certification verdict policy, or the main test
//! harness selection logic.
//!
//! Key invariants:
//! - repro artifacts must round-trip the exact test case metadata needed to
//!   reproduce failures outside the original lane.
//! - autogen and fuzz repro paths share the same summary/report surface so users
//!   can compare them directly.
//! - generated standalone sources must stay aligned with the serialized repro
//!   schema version.
//!
//! Primary entrypoints:
//! - `run_repro_artifact`
//! - `write_autogen_repro_artifact`
//! - `write_fuzz_repro_artifact`
//!
//! Failure modes / common pitfalls:
//! - reading module sources relative to the wrong workspace root silently makes
//!   repro artifacts non-portable.
//! - letting repro-only summary code drift from the main test summary schema
//!   produces misleading debugging artifacts.

use super::super::super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE, OutputFormat};
use super::build_compile::BudgetPolicyV1;
use super::build_compile::now_unix_ms;
use super::test_eval_perf::{
    AutogenReproArtifact, DifferentialPipeline, FuzzReproArtifact, GeneratedCaseKind,
    HttpCassetteMode, REPRO_SCHEMA_VERSION, ReproArtifact, TEST_JSON_SUMMARY_SEED, TestCase,
    TestJsonCase, TestJsonRunMetadata, TestJsonSummary, TestJsonTimings, TestLane,
    autogen_standalone_entry_source, emit_test_json_summary, fuzz_standalone_entry_source,
    run_single_test, sanitize_test_path_component, shrink_autogen_call,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) fn run_repro_artifact(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
) -> i32 {
    let artifact_bytes = match fs::read(repro_artifact_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!(
                "repro error: failed to read artifact {}: {}",
                repro_artifact_path.display(),
                err
            );
            return EXIT_USAGE;
        }
    };
    let artifact: ReproArtifact = match serde_json::from_slice(&artifact_bytes) {
        Ok(value) => value,
        Err(err) => {
            let fallback: Result<serde_json::Value, _> = serde_json::from_slice(&artifact_bytes);
            let legacy_shape = fallback
                .ok()
                .is_some_and(|json| json.get("kind").is_none() || json.get("version").is_none());
            if legacy_shape {
                eprintln!(
                    "repro error: legacy repro artifacts are unsupported after schema v{REPRO_SCHEMA_VERSION}; regenerate with a fresh failing run and retry --repro ({})",
                    repro_artifact_path.display()
                );
                return EXIT_USAGE;
            }
            eprintln!(
                "repro error: invalid repro artifact {}: {}",
                repro_artifact_path.display(),
                err
            );
            return EXIT_USAGE;
        }
    };
    match artifact {
        ReproArtifact::Autogen(artifact) => run_autogen_repro(
            workspace_root,
            repro_artifact_path,
            timeout,
            output_format,
            http_mode,
            budget_policy,
            artifact,
        ),
        ReproArtifact::Fuzz(artifact) => run_fuzz_repro(
            workspace_root,
            repro_artifact_path,
            timeout,
            output_format,
            http_mode,
            budget_policy,
            artifact,
        ),
    }
}

fn run_autogen_repro(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
    artifact: AutogenReproArtifact,
) -> i32 {
    if artifact.version != REPRO_SCHEMA_VERSION {
        eprintln!(
            "repro error: autogen artifact version mismatch: expected {}, got {} ({})",
            REPRO_SCHEMA_VERSION,
            artifact.version,
            repro_artifact_path.display()
        );
        return EXIT_USAGE;
    }
    let replay_call = if artifact.replay_call.trim().is_empty() {
        artifact.original_call.clone()
    } else {
        artifact.replay_call.clone()
    };
    let module_source = match load_autogen_module_source(workspace_root, &artifact.module_path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("repro error: {err}");
            return EXIT_USAGE;
        }
    };
    let test = TestCase {
        id: format!("repro:{}", artifact.test_id),
        lane: TestLane::Spec,
        name: format!(
            "{}::{}::repro",
            artifact.module_path.as_str(),
            artifact.func_name.as_str()
        ),
        module_path: artifact.module_path.clone(),
        func_name: artifact.func_name.clone(),
        is_serial: false,
        allows_env_set: false,
        allows_fs_escape: false,
        has_oracle: true,
        generated_call_body: Some(replay_call.clone()),
        generated_case_kind: Some(GeneratedCaseKind::Autogen),
        generated_entry_source: Some(autogen_standalone_entry_source(
            &module_source,
            &replay_call,
        )),
        autogen_module_source: Some(module_source),
        autogen_seed: Some(artifact.seed),
        autogen_span: artifact.span.clone(),
        sim_seed: None,
        canonical_id: artifact.test_id.clone(),
    };
    run_typed_repro_case(
        workspace_root,
        repro_artifact_path,
        timeout,
        output_format,
        http_mode,
        budget_policy,
        &test,
        artifact.seed,
        "spec",
        "repro",
        &format!(
            "autogen failure: check={}::{} seed={} span={} call=`{}` repro={}",
            test.module_path,
            test.func_name,
            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
            test.autogen_span.as_deref().unwrap_or("unknown"),
            replay_call,
            repro_artifact_path.display()
        ),
    )
}

fn run_fuzz_repro(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
    artifact: FuzzReproArtifact,
) -> i32 {
    if artifact.version != REPRO_SCHEMA_VERSION {
        eprintln!(
            "repro error: fuzz artifact version mismatch: expected {}, got {} ({})",
            REPRO_SCHEMA_VERSION,
            artifact.version,
            repro_artifact_path.display()
        );
        return EXIT_USAGE;
    }
    let module_source = match load_autogen_module_source(workspace_root, &artifact.module_path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("repro error: {err}");
            return EXIT_USAGE;
        }
    };
    let call = artifact.call.clone();
    let test = TestCase {
        id: format!("repro:{}", artifact.test_id),
        lane: TestLane::Integration,
        name: format!(
            "{}::{}::fuzz_repro",
            artifact.module_path.as_str(),
            artifact.func_name.as_str()
        ),
        module_path: artifact.module_path.clone(),
        func_name: artifact.func_name.clone(),
        is_serial: false,
        allows_env_set: false,
        allows_fs_escape: false,
        has_oracle: true,
        generated_call_body: Some(call.clone()),
        generated_case_kind: Some(GeneratedCaseKind::Fuzz),
        generated_entry_source: Some(fuzz_standalone_entry_source(
            &module_source,
            &call,
            artifact.uses_bytes_helper,
        )),
        autogen_module_source: Some(module_source),
        autogen_seed: Some(artifact.seed),
        autogen_span: artifact.span.clone(),
        sim_seed: None,
        canonical_id: artifact.test_id.clone(),
    };
    run_typed_repro_case(
        workspace_root,
        repro_artifact_path,
        timeout,
        output_format,
        http_mode,
        budget_policy,
        &test,
        artifact.seed,
        "integration",
        "fuzz repro",
        &format!(
            "fuzz failure: target={}::{} seed={} span={} call=`{}` repro={}",
            test.module_path,
            test.func_name,
            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
            test.autogen_span.as_deref().unwrap_or("unknown"),
            call,
            repro_artifact_path.display()
        ),
    )
}

fn run_typed_repro_case(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
    test: &TestCase,
    seed: u64,
    lane_name: &str,
    success_label: &str,
    failure_detail: &str,
) -> i32 {
    let harness_stub = workspace_root
        .join("target")
        .join("wrela_tests")
        .join("repro")
        .join("harness_bin");
    let Some(parent) = harness_stub.parent() else {
        eprintln!(
            "repro error: invalid harness output path for {}",
            repro_artifact_path.display()
        );
        return EXIT_USAGE;
    };
    if let Err(err) = fs::create_dir_all(parent) {
        eprintln!(
            "repro error: failed to create {}: {}",
            parent.display(),
            err
        );
        return EXIT_USAGE;
    }
    let started = Instant::now();
    let result = run_single_test(
        &harness_stub,
        workspace_root,
        test,
        timeout,
        output_format,
        http_mode,
        DifferentialPipeline::Baseline,
        wrela::query_plan::DispatchBackend::Auto,
    );
    let duration_ms = started.elapsed().as_millis();
    match result {
        Ok(_) => {
            match output_format {
                OutputFormat::Pretty => {
                    println!(
                        "ok   {:?}  {} ({} {})",
                        Duration::from_millis(duration_ms as u64),
                        test.name,
                        success_label,
                        repro_artifact_path.display()
                    );
                }
                OutputFormat::Json => {
                    emit_test_json_summary(&TestJsonSummary {
                        run: TestJsonRunMetadata {
                            seed,
                            lane: lane_name.to_string(),
                            jobs: 1,
                            harness_cache_hit: false,
                            budgets_used: budget_policy.clone(),
                        },
                        tests: vec![TestJsonCase {
                            id: test.id.clone(),
                            name: test.name.clone(),
                            lane: lane_name.to_string(),
                            status: "ok".to_string(),
                            duration_ms,
                            error: None,
                        }],
                        timings: TestJsonTimings {
                            discovery_ms: 0,
                            selection_ms: 0,
                            compile_harness_ms: 0,
                            execution_ms: duration_ms,
                            total_ms: duration_ms,
                        },
                    });
                }
                OutputFormat::Sarif => {
                    println!(
                        "ok   {:?}  {} ({} {})",
                        Duration::from_millis(duration_ms as u64),
                        test.name,
                        success_label,
                        repro_artifact_path.display()
                    );
                }
            }
            EXIT_OK
        }
        Err(err) => {
            match output_format {
                OutputFormat::Pretty => {
                    println!(
                        "fail {:?}  {}  {} | {}",
                        started.elapsed(),
                        test.name,
                        err,
                        failure_detail
                    );
                }
                OutputFormat::Json => {
                    emit_test_json_summary(&TestJsonSummary {
                        run: TestJsonRunMetadata {
                            seed,
                            lane: lane_name.to_string(),
                            jobs: 1,
                            harness_cache_hit: false,
                            budgets_used: budget_policy.clone(),
                        },
                        tests: vec![TestJsonCase {
                            id: test.id.clone(),
                            name: test.name.clone(),
                            lane: lane_name.to_string(),
                            status: "fail".to_string(),
                            duration_ms,
                            error: Some(format!("{err} | {failure_detail}")),
                        }],
                        timings: TestJsonTimings {
                            discovery_ms: 0,
                            selection_ms: 0,
                            compile_harness_ms: 0,
                            execution_ms: duration_ms,
                            total_ms: duration_ms,
                        },
                    });
                }
                OutputFormat::Sarif => {
                    println!(
                        "fail {:?}  {}  {} | {}",
                        started.elapsed(),
                        test.name,
                        err,
                        failure_detail
                    );
                }
            }
            EXIT_CODEGEN
        }
    }
}

fn load_autogen_module_source(workspace_root: &Path, module_path: &str) -> Result<String, String> {
    let mut candidates = Vec::new();
    if module_path.starts_with("src/") || module_path.starts_with("tests/") {
        candidates.push(workspace_root.join(format!("{module_path}.wr")));
    } else {
        candidates.push(workspace_root.join(format!("{module_path}.wr")));
        candidates.push(workspace_root.join("src").join(format!("{module_path}.wr")));
        candidates.push(
            workspace_root
                .join("tests")
                .join(format!("{module_path}.wr")),
        );
    }
    for path in candidates {
        if path.is_file() {
            return fs::read_to_string(&path).map_err(|err| {
                format!(
                    "failed to read module source for repro {}: {}",
                    path.display(),
                    err
                )
            });
        }
    }
    Err(format!(
        "module source for repro not found under workspace: {}",
        module_path
    ))
}

pub(crate) fn write_autogen_repro_artifact(
    workspace_root: &Path,
    harness_exe_path: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    failure: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let Some(original_call) = test.generated_call_body.as_ref() else {
        return Err("missing generated call for autogen repro".to_string());
    };
    let shrunk_call = shrink_autogen_call(
        harness_exe_path,
        workspace_root,
        test,
        timeout,
        output_format,
        http_mode,
    );
    let replay_call = shrunk_call.clone().unwrap_or_else(|| original_call.clone());
    let seed = test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let check_key =
        sanitize_test_path_component(&format!("{}__{}", test.module_path, test.func_name));
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("autogen")
        .join(check_key);
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create autogen artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let artifact_path = artifact_dir.join(format!("{seed}.json"));
    let artifact = ReproArtifact::Autogen(AutogenReproArtifact {
        version: REPRO_SCHEMA_VERSION,
        generated_at_unix_ms: now_unix_ms() as u64,
        workspace_root: workspace_root.display().to_string(),
        test_id: test.id.clone(),
        module_path: test.module_path.clone(),
        func_name: test.func_name.clone(),
        seed,
        span: test.autogen_span.clone(),
        original_call: original_call.clone(),
        shrunk_call: shrunk_call.clone(),
        replay_call,
        failure: failure.to_string(),
    });
    let payload = serde_json::to_vec_pretty(&artifact).map_err(|err| err.to_string())?;
    fs::write(&artifact_path, payload).map_err(|err| {
        format!(
            "failed to write autogen repro artifact {}: {}",
            artifact_path.display(),
            err
        )
    })?;
    Ok((artifact_path, shrunk_call))
}

pub(crate) fn write_fuzz_repro_artifact(
    workspace_root: &Path,
    test: &TestCase,
    failure: &str,
) -> Result<PathBuf, String> {
    let Some(call) = test.generated_call_body.as_ref() else {
        return Err("missing generated call for fuzz repro".to_string());
    };
    let seed = test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let target_key =
        sanitize_test_path_component(&format!("{}__{}", test.module_path, test.func_name));
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("fuzz")
        .join(target_key);
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create fuzz artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let artifact_path = artifact_dir.join(format!("{seed}.json"));
    let artifact = ReproArtifact::Fuzz(FuzzReproArtifact {
        version: REPRO_SCHEMA_VERSION,
        generated_at_unix_ms: now_unix_ms() as u64,
        workspace_root: workspace_root.display().to_string(),
        test_id: test.id.clone(),
        module_path: test.module_path.clone(),
        func_name: test.func_name.clone(),
        seed,
        span: test.autogen_span.clone(),
        call: call.clone(),
        uses_bytes_helper: call.contains("get_bytes_from_list("),
        failure: failure.to_string(),
    });
    let payload = serde_json::to_vec_pretty(&artifact).map_err(|err| err.to_string())?;
    fs::write(&artifact_path, payload).map_err(|err| {
        format!(
            "failed to write fuzz repro artifact {}: {}",
            artifact_path.display(),
            err
        )
    })?;
    Ok(artifact_path)
}
