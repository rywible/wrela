fn init_project(path: &str) -> io::Result<()> {
    let root = Path::new(path);
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir)?;
    let main_path = src_dir.join("main.wr");
    if main_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "src/main.wr already exists",
        ));
    }
    fs::write(main_path, "fn run() -> Integer {\n    return 0\n}\n")?;
    Ok(())
}

const CERT_SCHEMA_VERSION: u32 = 3;
const CERT_GATE_VERSIONS_MARKER: &str = "wrela-cert-gates-v1";
const COVERAGE_SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const COVERAGE_INDEX_SCHEMA_VERSION: u32 = 2;
const MUTATION_CACHE_SCHEMA_VERSION: u32 = 1;
const MUTATION_KILL_HISTORY_SCHEMA_VERSION: u32 = 1;
const MUTATION_CACHE_ENGINE_TAG: &str = "wrela-mutation-cache-v1";
const RUNTIME_CARGO_TOML: &str = include_str!("../../../../runtime/Cargo.toml");
const BUDGET_POLICY_VERSION: u32 = 1;
const DEFAULT_TEST_JOBS: u64 = 1;
const DEFAULT_TEST_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_AUTOGEN_MAX_CASES: u64 = 16;
const DEFAULT_SIM_MAX_CASES: u64 = 256;
const DEFAULT_FUZZ_MAX_CASES: u64 = 128;
const DEFAULT_MUTATION_MAX_CASES: u64 = 32;
const DEFAULT_AUTOGEN_TIME_CAP_MS: u64 = 5_000;
const DEFAULT_SIM_TIME_CAP_MS: u64 = 10_000;
const DEFAULT_FUZZ_TIME_CAP_MS: u64 = 15_000;
const DEFAULT_MUTATION_TIME_CAP_MS: u64 = 20_000;
const CEILING_TEST_JOBS: u64 = 64;
const CEILING_TEST_TIMEOUT_MS: u64 = 120_000;
const CEILING_AUTOGEN_MAX_CASES: u64 = 1_024;
const CEILING_SIM_MAX_CASES: u64 = 4_096;
const CEILING_FUZZ_MAX_CASES: u64 = 4_096;
const CEILING_MUTATION_MAX_CASES: u64 = 512;
const CEILING_AUTOGEN_TIME_CAP_MS: u64 = 60_000;
const CEILING_SIM_TIME_CAP_MS: u64 = 120_000;
const CEILING_FUZZ_TIME_CAP_MS: u64 = 120_000;
const CEILING_MUTATION_TIME_CAP_MS: u64 = 180_000;
const PUBLIC_SURFACE_CURRENT_REL_PATH: &str = "tests/.artifacts/public_surface/current.json";
const PUBLIC_SURFACE_BASELINE_REL_PATH: &str = "tests/public_surface.baseline.json";
const TEST_HARNESS_META_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CertificationReport {
    cert_schema_version: u32,
    generated_at_unix_ms: u128,
    entry_path: String,
    workspace_root: String,
    artifact_path: String,
    tests_passed: bool,
    toolchain_version: String,
    compiler_version: String,
    compiler_git_sha: Option<String>,
    runtime_version: String,
    gate_versions_marker: String,
    source_hash: String,
    seeds_used: CertificationSeedsUsed,
    budgets_used: CertificationBudgetsUsed,
    coverage_summary_hash: Option<String>,
    mutation_summary_hash: Option<String>,
    differential_results_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    impact_manifest: Option<CertifiedImpactManifest>,
    binary_hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedImpactManifest {
    source_files: Vec<CertifiedSourceFileFingerprint>,
    src_modules: Vec<CertifiedSrcModuleSnapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedSourceFileFingerprint {
    rel_path: String,
    hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedSrcModuleSnapshot {
    module_path: String,
    rel_path: String,
    hash: String,
    uses: Vec<String>,
    runtime_sensitive: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PublicSurfaceSnapshot {
    version: u32,
    items: Vec<PublicSurfaceItem>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PublicSurfaceItem {
    qualified_name: String,
    signature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connector_literals: Vec<PublicSurfaceConnectorLiteral>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct PublicSurfaceConnectorLiteral {
    service: String,
    endpoint: String,
    method: String,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct CertificationSeedsUsed {
    sim: u64,
    autogen: u64,
    fuzz: u64,
}

#[derive(Serialize, Deserialize)]
struct CertificationBudgetsUsed {
    policy_version: u32,
    test_jobs: BudgetValue,
    test_timeout_ms: BudgetValue,
    autogen_max_cases: BudgetValue,
    sim_max_cases: BudgetValue,
    fuzz_max_cases: BudgetValue,
    mutation_max_cases: BudgetValue,
    autogen_time_cap_ms: BudgetValue,
    sim_time_cap_ms: BudgetValue,
    fuzz_time_cap_ms: BudgetValue,
    mutation_time_cap_ms: BudgetValue,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageCassette {
    request: ConnectorCoverageRequest,
    response: ConnectorCoverageResponse,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageRequest {
    service: String,
    endpoint: String,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageResponse {
    status: u16,
}

#[derive(Serialize)]
struct BuildPerfEvent {
    event: &'static str,
    perf: BuildPerfPayload,
}

#[derive(Serialize)]
struct BuildPerfPayload {
    cache: BuildPerfCache,
    timings: BuildPerfTimings,
}

#[derive(Serialize)]
struct BuildPerfCache {
    hit: bool,
    hash: String,
    reason: String,
}

#[derive(Serialize)]
struct BuildPerfTimings {
    certification_ms: u128,
    cert_collect_tests_ms: u128,
    cert_compile_harness_ms: u128,
    cert_determinism_ms: u128,
    cert_mutation_discovery_ms: u128,
    cert_mutation_execution_ms: u128,
    cert_diff_ms: u128,
    mir_compile_ms: u128,
    codegen_ms: u128,
    cert_report_ms: u128,
    total_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BudgetValue {
    pub(super) value: u64,
    default: u64,
    ceiling: u64,
    provenance: BudgetProvenance,
}

#[derive(Clone, Serialize, Deserialize)]
struct BudgetProvenance {
    source: String,
    key: String,
    requested: u64,
    clamped_to_ceiling: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BudgetPolicyV1 {
    policy_version: u32,
    pub(super) test_jobs: BudgetValue,
    pub(super) test_timeout_ms: BudgetValue,
    autogen_max_cases: BudgetValue,
    sim_max_cases: BudgetValue,
    fuzz_max_cases: BudgetValue,
    pub(super) mutation_max_cases: BudgetValue,
    autogen_time_cap_ms: BudgetValue,
    sim_time_cap_ms: BudgetValue,
    fuzz_time_cap_ms: BudgetValue,
    pub(super) mutation_time_cap_ms: BudgetValue,
}

#[derive(Serialize)]
struct TestMaintenanceSummary {
    version: u32,
    generated_at_unix_ms: u128,
    workspace_root: String,
    mode_record: bool,
    mode_update_public_surface: bool,
    exit_code: i32,
    deployable_artifacts_emitted: bool,
}

#[derive(Serialize)]
struct BuildCertCacheJsonEvent {
    event: &'static str,
    cache_hit: bool,
    cache_hash: String,
    cache_dir: String,
}

#[derive(Clone, Serialize)]
struct CertSelectionJsonEvent {
    event: &'static str,
    mode: String,
    changed_files: Vec<String>,
    changed_src_modules: Vec<String>,
    impacted_src_modules: Vec<String>,
    selected_test_count: usize,
    selected_stage_count: usize,
    stages: Vec<CertSelectionStage>,
    reasons: Vec<String>,
}

#[derive(Clone, Serialize)]
struct CertSelectionStage {
    lane: String,
    selected: bool,
    reason: String,
}

#[derive(Clone, Default)]
struct CertSelectionReport {
    mode: String,
    changed_files: Vec<String>,
    changed_src_modules: Vec<String>,
    impacted_src_modules: Vec<String>,
    stages: Vec<CertSelectionStage>,
    reasons: Vec<String>,
}

fn write_certification_report(
    entry_path: &Path,
    workspace_root: &Path,
    artifact_path: &Path,
    budgets_used: &BudgetPolicyV1,
    toolchain_version: &str,
    source_hash: &str,
    cache_hash: &str,
    differential_results_hash: Option<&str>,
    mutation_summary_hash: Option<&str>,
) -> Result<(), String> {
    let generated_at_unix_ms = now_unix_ms();
    let binary_hash = hash_file_fingerprint(artifact_path)?;
    let compiler_version = env!("CARGO_PKG_VERSION").to_string();
    let compiler_git_sha = resolve_compiler_git_sha();
    let runtime_version = resolve_runtime_version();
    let report = CertificationReport {
        cert_schema_version: CERT_SCHEMA_VERSION,
        generated_at_unix_ms,
        entry_path: entry_path.display().to_string(),
        workspace_root: workspace_root.display().to_string(),
        artifact_path: artifact_path.display().to_string(),
        tests_passed: true,
        toolchain_version: toolchain_version.to_string(),
        compiler_version,
        compiler_git_sha,
        runtime_version,
        gate_versions_marker: CERT_GATE_VERSIONS_MARKER.to_string(),
        source_hash: source_hash.to_string(),
        seeds_used: CertificationSeedsUsed {
            sim: 0x5A17,
            autogen: 0xA670,
            fuzz: 0xF022,
        },
        budgets_used: certification_budgets_used(budgets_used),
        coverage_summary_hash: None,
        mutation_summary_hash: mutation_summary_hash.map(str::to_string),
        differential_results_hash: differential_results_hash.map(str::to_string),
        impact_manifest: build_certified_impact_manifest(workspace_root).ok(),
        binary_hash: binary_hash.clone(),
    };
    let payload = serde_json::to_vec_pretty(&report).map_err(|err| err.to_string())?;
    let adjacent_path = artifact_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cert.json");
    fs::write(&adjacent_path, &payload).map_err(|err| {
        format!(
            "failed to write adjacent cert report {}: {}",
            adjacent_path.display(),
            err
        )
    })?;
    let cache_dir = workspace_root
        .join("target")
        .join("wrela_cert")
        .join(cache_hash);
    fs::create_dir_all(&cache_dir).map_err(|err| {
        format!(
            "failed to create cert cache {}: {}",
            cache_dir.display(),
            err
        )
    })?;
    let cache_path = cache_dir.join("cert.json");
    fs::write(&cache_path, &payload).map_err(|err| {
        format!(
            "failed to write cached cert report {}: {}",
            cache_path.display(),
            err
        )
    })?;
    if binary_hash != cache_hash {
        let compat_dir = workspace_root
            .join("target")
            .join("wrela_cert")
            .join(&binary_hash);
        fs::create_dir_all(&compat_dir).map_err(|err| {
            format!(
                "failed to create compatibility cert cache {}: {}",
                compat_dir.display(),
                err
            )
        })?;
        let compat_path = compat_dir.join("cert.json");
        fs::write(&compat_path, &payload).map_err(|err| {
            format!(
                "failed to write compatibility cached cert report {}: {}",
                compat_path.display(),
                err
            )
        })?;
    }
    let latest_success_path = workspace_root
        .join("target")
        .join("wrela_cert")
        .join("last_success_cert.json");
    fs::write(&latest_success_path, &payload).map_err(|err| {
        format!(
            "failed to write latest successful cert report {}: {}",
            latest_success_path.display(),
            err
        )
    })?;
    Ok(())
}

fn certification_budgets_used(policy: &BudgetPolicyV1) -> CertificationBudgetsUsed {
    CertificationBudgetsUsed {
        policy_version: policy.policy_version,
        test_jobs: policy.test_jobs.clone(),
        test_timeout_ms: policy.test_timeout_ms.clone(),
        autogen_max_cases: policy.autogen_max_cases.clone(),
        sim_max_cases: policy.sim_max_cases.clone(),
        fuzz_max_cases: policy.fuzz_max_cases.clone(),
        mutation_max_cases: policy.mutation_max_cases.clone(),
        autogen_time_cap_ms: policy.autogen_time_cap_ms.clone(),
        sim_time_cap_ms: policy.sim_time_cap_ms.clone(),
        fuzz_time_cap_ms: policy.fuzz_time_cap_ms.clone(),
        mutation_time_cap_ms: policy.mutation_time_cap_ms.clone(),
    }
}

fn resolve_compiler_git_sha() -> Option<String> {
    if let Some(sha) = option_env!("WRELA_GIT_SHA")
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }
    if let Some(sha) = std::env::var("WRELA_GIT_SHA")
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }
    if let Some(sha) = std::env::var("GITHUB_SHA")
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

fn resolve_runtime_version() -> String {
    if let Some(version) = parse_cargo_package_version(RUNTIME_CARGO_TOML) {
        return version;
    }
    "unknown".to_string()
}

fn resolve_toolchain_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn certification_cache_hash(source_hash: &str, toolchain_version: &str) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(b"wrela-cert-cache-v2");
    hasher.update(&[0]);
    hasher.update(b"source_hash:");
    hasher.update(source_hash.as_bytes());
    hasher.update(&[0]);
    hasher.update(b"toolchain_version:");
    hasher.update(toolchain_version.as_bytes());
    hasher.finish_hex()
}

fn emit_certification_cache_hit(output_format: OutputFormat, cache_hash: &str, cache_dir: &Path) {
    match output_format {
        OutputFormat::Pretty => {
            eprintln!("certification cache hit: {}", cache_hash);
        }
        OutputFormat::Json => {
            let event = BuildCertCacheJsonEvent {
                event: "certification_cache",
                cache_hit: true,
                cache_hash: cache_hash.to_string(),
                cache_dir: cache_dir.display().to_string(),
            };
            println!(
                "{}",
                serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Sarif => {
            eprintln!("certification cache hit: {}", cache_hash);
        }
    }
}

fn emit_build_perf_event(
    output_format: OutputFormat,
    cache_hit: bool,
    cache_hash: String,
    cache_reason: String,
    timings: BuildPerfTimings,
) {
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    let event = BuildPerfEvent {
        event: "build_perf",
        perf: BuildPerfPayload {
            cache: BuildPerfCache {
                hit: cache_hit,
                hash: cache_hash,
                reason: cache_reason,
            },
            timings,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
    );
}

fn emit_cert_selection_report(
    output_format: OutputFormat,
    report: &CertSelectionReport,
    selected_test_count: usize,
) {
    let selected_stage_count = report.stages.iter().filter(|stage| stage.selected).count();
    match output_format {
        OutputFormat::Pretty => {
            eprintln!(
                "certification selection: mode={} selected_tests={} selected_stages={}",
                report.mode, selected_test_count, selected_stage_count
            );
            if !report.changed_files.is_empty() {
                eprintln!("  changed_files: {}", report.changed_files.join(", "));
            }
            if !report.changed_src_modules.is_empty() {
                eprintln!(
                    "  changed_src_modules: {}",
                    report.changed_src_modules.join(", ")
                );
            }
            if !report.impacted_src_modules.is_empty() {
                eprintln!(
                    "  impacted_src_modules: {}",
                    report.impacted_src_modules.join(", ")
                );
            }
            for stage in &report.stages {
                eprintln!(
                    "  stage={} selected={} reason={}",
                    stage.lane, stage.selected, stage.reason
                );
            }
            for reason in &report.reasons {
                eprintln!("  reason: {reason}");
            }
        }
        OutputFormat::Json => {
            let event = CertSelectionJsonEvent {
                event: "certification_selection",
                mode: report.mode.clone(),
                changed_files: report.changed_files.clone(),
                changed_src_modules: report.changed_src_modules.clone(),
                impacted_src_modules: report.impacted_src_modules.clone(),
                selected_test_count,
                selected_stage_count,
                stages: report.stages.clone(),
                reasons: report.reasons.clone(),
            };
            println!(
                "{}",
                serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Sarif => {
            eprintln!(
                "certification selection: mode={} selected_tests={} selected_stages={}",
                report.mode, selected_test_count, selected_stage_count
            );
            if !report.changed_files.is_empty() {
                eprintln!("  changed_files: {}", report.changed_files.join(", "));
            }
            if !report.changed_src_modules.is_empty() {
                eprintln!(
                    "  changed_src_modules: {}",
                    report.changed_src_modules.join(", ")
                );
            }
            if !report.impacted_src_modules.is_empty() {
                eprintln!(
                    "  impacted_src_modules: {}",
                    report.impacted_src_modules.join(", ")
                );
            }
            for stage in &report.stages {
                eprintln!(
                    "  stage={} selected={} reason={}",
                    stage.lane, stage.selected, stage.reason
                );
            }
            for reason in &report.reasons {
                eprintln!("  reason: {reason}");
            }
        }
    }
}

fn resolve_certification_test_selection(
    workspace_root: &Path,
    output_format: OutputFormat,
) -> TestSelection {
    let mut selection = TestSelection::default();
    let latest_success_path = workspace_root
        .join("target")
        .join("wrela_cert")
        .join("last_success_cert.json");
    if !latest_success_path.is_file() {
        selection.cert_selection_report = Some(CertSelectionReport {
            mode: "full".to_string(),
            stages: vec![
                CertSelectionStage {
                    lane: "spec".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "integration".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "sim".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "model".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "default".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
            ],
            reasons: vec![
                "no previous successful cert manifest; running full certification suite"
                    .to_string(),
            ],
            ..CertSelectionReport::default()
        });
        return selection;
    }

    let previous_report = match read_certification_report(&latest_success_path) {
        Ok(report) => report,
        Err(err) => {
            selection.cert_selection_report = Some(CertSelectionReport {
                mode: "full".to_string(),
                stages: vec![
                    CertSelectionStage {
                        lane: "spec".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "integration".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "sim".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "model".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "default".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                ],
                reasons: vec![format!(
                    "failed to parse previous successful cert manifest ({}): {}",
                    latest_success_path.display(),
                    err
                )],
                ..CertSelectionReport::default()
            });
            return selection;
        }
    };

    let Some(previous_manifest) = previous_report.impact_manifest else {
        selection.cert_selection_report = Some(CertSelectionReport {
            mode: "full".to_string(),
            stages: vec![
                CertSelectionStage {
                    lane: "spec".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "integration".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "sim".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "model".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "default".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
            ],
            reasons: vec![
                "previous successful cert is missing impact manifest; running full suite"
                    .to_string(),
            ],
            ..CertSelectionReport::default()
        });
        return selection;
    };

    let current_manifest = match build_certified_impact_manifest(workspace_root) {
        Ok(manifest) => manifest,
        Err(err) => {
            selection.cert_selection_report = Some(CertSelectionReport {
                mode: "full".to_string(),
                stages: vec![
                    CertSelectionStage {
                        lane: "spec".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "integration".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "sim".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "model".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "default".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                ],
                reasons: vec![format!("failed to build current impact manifest: {err}")],
                ..CertSelectionReport::default()
            });
            return selection;
        }
    };

    let changed_files = diff_changed_files(&previous_manifest, &current_manifest);
    let changed_src_modules = diff_changed_src_modules(&previous_manifest, &current_manifest);
    let impacted_src_modules =
        impacted_src_modules_from_changed(&current_manifest.src_modules, &changed_src_modules);
    let runtime_sensitive_impacted = impacted_src_modules.iter().any(|module_path| {
        current_manifest
            .src_modules
            .iter()
            .find(|module| &module.module_path == module_path)
            .is_some_and(|module| module.runtime_sensitive)
    });

    let mut tests = Vec::new();
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() || collect_tests(&tests_root, &tests_root, &mut tests).is_err() {
        return selection;
    }
    tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));

    let integration_reachability = integration_reachability_to_impacted(
        workspace_root,
        &current_manifest,
        &impacted_src_modules,
    );
    let mut selected_ids = HashSet::new();
    for test in &tests {
        match test.lane {
            TestLane::Spec => {
                selected_ids.insert(test.id.clone());
            }
            TestLane::Integration => {
                if integration_reachability
                    .get(&test.module_path)
                    .copied()
                    .unwrap_or(false)
                {
                    selected_ids.insert(test.id.clone());
                }
            }
            TestLane::Sim => {
                if runtime_sensitive_impacted {
                    selected_ids.insert(test.id.clone());
                }
            }
            TestLane::Model | TestLane::Default => {
                selected_ids.insert(test.id.clone());
            }
        }
    }
    let lane_selected_ids = selected_ids.clone();

    let previous_index_hash = certification_cache_hash(
        &previous_report.source_hash,
        &previous_report.toolchain_version,
    );
    match load_function_test_coverage_index(workspace_root, &previous_index_hash) {
        Ok(index) if index.is_empty() => {
            if !changed_src_modules.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    "previous certification coverage index is empty; keeping lane-based selection"
                        .to_string(),
                );
            }
        }
        Ok(index) => {
            let changed_function_ids = changed_function_ids_from_modules(
                workspace_root,
                &current_manifest,
                &changed_src_modules,
            );
            if changed_function_ids.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    "no changed top-level functions/checks extracted from changed src modules; keeping lane-based selection"
                        .to_string(),
                );
            } else {
                let mut mapped_test_ids = BTreeSet::new();
                let mut unmapped_function_count = 0usize;
                for function_id in &changed_function_ids {
                    if let Some(test_ids) = index.get(function_id) {
                        for test_id in test_ids {
                            mapped_test_ids.insert(test_id.clone());
                        }
                    } else {
                        unmapped_function_count += 1;
                    }
                }
                if mapped_test_ids.is_empty() {
                    selection_reasons_push(
                        &mut selection,
                        format!(
                            "coverage index has no mapped tests for {} changed function ids (likely stale); keeping lane-based selection",
                            changed_function_ids.len()
                        ),
                    );
                } else {
                    let mut trimmed_ids = selected_ids
                        .iter()
                        .filter(|id| mapped_test_ids.contains(*id))
                        .cloned()
                        .collect::<HashSet<_>>();
                    if trimmed_ids.is_empty() {
                        selection_reasons_push(
                            &mut selection,
                            format!(
                                "coverage index mapping would prune all selected tests (lane_selected={} mapped={} changed_functions={}); keeping lane-based selection",
                                lane_selected_ids.len(),
                                mapped_test_ids.len(),
                                changed_function_ids.len()
                            ),
                        );
                    } else {
                        selected_ids.clear();
                        selected_ids.extend(trimmed_ids.drain());
                        selection_reasons_push(
                            &mut selection,
                            format!(
                                "coverage index trim applied: lane_selected={} trimmed={} changed_functions={} unmapped_functions={}",
                                lane_selected_ids.len(),
                                selected_ids.len(),
                                changed_function_ids.len(),
                                unmapped_function_count
                            ),
                        );
                    }
                }
            }
        }
        Err(err) => {
            if !changed_src_modules.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    format!(
                        "coverage index unavailable for previous cert hash {}: {}; keeping lane-based selection",
                        previous_index_hash, err
                    ),
                );
            }
        }
    }
    if selected_ids.is_empty() && !lane_selected_ids.is_empty() {
        selected_ids = lane_selected_ids.clone();
        selection_reasons_push(
            &mut selection,
            "selection safety guard restored lane-based selection to avoid empty certification set"
                .to_string(),
        );
    }

    let mut stage_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for test in &tests {
        *stage_counts.entry(test.lane.as_str()).or_insert(0) += 1;
    }
    let mut selected_stage_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for test in &tests {
        if selected_ids.contains(&test.id) {
            *selected_stage_counts.entry(test.lane.as_str()).or_insert(0) += 1;
        }
    }
    let stage_names = ["spec", "integration", "sim", "model", "default"];
    let stages = stage_names
        .iter()
        .map(|lane| {
            let total = stage_counts.get(lane).copied().unwrap_or(0);
            let selected = selected_stage_counts.get(lane).copied().unwrap_or(0);
            let reason = match *lane {
                "spec" => "always selected".to_string(),
                "integration" => format!(
                    "selected modules that transitively import impacted src modules ({selected}/{total})"
                ),
                "sim" => {
                    if runtime_sensitive_impacted {
                        "runtime-sensitive impacted src modules detected".to_string()
                    } else {
                        "no runtime-sensitive impacted src modules detected".to_string()
                    }
                }
                _ => "safe behavior: run all".to_string(),
            };
            CertSelectionStage {
                lane: (*lane).to_string(),
                selected: selected > 0 || total == 0,
                reason,
            }
        })
        .collect::<Vec<_>>();

    let mut reasons = Vec::new();
    if changed_files.is_empty() {
        reasons.push("no source file deltas observed between manifests".to_string());
    }
    reasons.push(format!(
        "changed_files={} changed_src_modules={} impacted_src_modules={}",
        changed_files.len(),
        changed_src_modules.len(),
        impacted_src_modules.len()
    ));
    if matches!(output_format, OutputFormat::Pretty) && changed_src_modules.is_empty() {
        reasons
            .push("no src module deltas; integration and sim lanes reduced by policy".to_string());
    }
    if let Some(report) = selection.cert_selection_report.as_ref() {
        reasons.extend(report.reasons.clone());
    }

    selection.include_ids = Some(selected_ids);
    selection.cert_selection_report = Some(CertSelectionReport {
        mode: "incremental".to_string(),
        changed_files,
        changed_src_modules,
        impacted_src_modules,
        stages,
        reasons,
    });
    selection
}

fn selection_reasons_push(selection: &mut TestSelection, reason: String) {
    let report = selection
        .cert_selection_report
        .get_or_insert_with(CertSelectionReport::default);
    report.reasons.push(reason);
}

fn changed_function_ids_from_modules(
    workspace_root: &Path,
    current_manifest: &CertifiedImpactManifest,
    changed_src_modules: &[String],
) -> BTreeSet<String> {
    use wrela::parser::ast::AstNode;

    let module_to_rel_path: BTreeMap<&str, &str> = current_manifest
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.rel_path.as_str()))
        .collect();
    let mut function_ids = BTreeSet::new();
    for module_path in changed_src_modules {
        let Some(rel_path) = module_to_rel_path.get(module_path.as_str()) else {
            continue;
        };
        let source_path = workspace_root.join(rel_path);
        let Ok(source) = fs::read_to_string(&source_path) else {
            continue;
        };
        let (syntax, parse_errors) = parser::parse_with_errors(&source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let lowered = hir::lower::lower(root);
        for (_, function) in lowered.functions.iter() {
            if matches!(
                function.kind,
                hir::FunctionKind::Function | hir::FunctionKind::Check
            ) {
                let qualified_identity =
                    qualified_function_identity(module_path, function.name.as_str());
                function_ids.insert(stable_function_id(&qualified_identity));
            }
        }
    }
    function_ids
}

fn read_certification_report(path: &Path) -> Result<CertificationReport, String> {
    let payload = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    serde_json::from_str(&payload)
        .map_err(|err| format!("failed to parse {} as cert json: {}", path.display(), err))
}

fn diff_changed_files(
    previous: &CertifiedImpactManifest,
    current: &CertifiedImpactManifest,
) -> Vec<String> {
    let previous_map: BTreeMap<&str, &str> = previous
        .source_files
        .iter()
        .map(|file| (file.rel_path.as_str(), file.hash.as_str()))
        .collect();
    let current_map: BTreeMap<&str, &str> = current
        .source_files
        .iter()
        .map(|file| (file.rel_path.as_str(), file.hash.as_str()))
        .collect();
    let all_paths: BTreeSet<&str> = previous_map
        .keys()
        .copied()
        .chain(current_map.keys().copied())
        .collect();
    all_paths
        .into_iter()
        .filter(|path| previous_map.get(path) != current_map.get(path))
        .map(|path| path.to_string())
        .collect()
}

fn diff_changed_src_modules(
    previous: &CertifiedImpactManifest,
    current: &CertifiedImpactManifest,
) -> Vec<String> {
    let previous_map: BTreeMap<&str, &str> = previous
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.hash.as_str()))
        .collect();
    let current_map: BTreeMap<&str, &str> = current
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.hash.as_str()))
        .collect();
    let all_modules: BTreeSet<&str> = previous_map
        .keys()
        .copied()
        .chain(current_map.keys().copied())
        .collect();
    all_modules
        .into_iter()
        .filter(|module| previous_map.get(module) != current_map.get(module))
        .map(|module| module.to_string())
        .collect()
}

fn impacted_src_modules_from_changed(
    src_modules: &[CertifiedSrcModuleSnapshot],
    changed_src_modules: &[String],
) -> Vec<String> {
    let module_set: HashSet<&str> = src_modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for module in src_modules {
        for dep in &module.uses {
            if module_set.contains(dep.as_str()) {
                reverse
                    .entry(dep.as_str())
                    .or_default()
                    .push(module.module_path.as_str());
            }
        }
    }
    let mut queue = VecDeque::new();
    let mut impacted = BTreeSet::new();
    for module in changed_src_modules {
        if module_set.contains(module.as_str()) {
            impacted.insert(module.clone());
            queue.push_back(module.clone());
        }
    }
    while let Some(module) = queue.pop_front() {
        if let Some(users) = reverse.get(module.as_str()) {
            for user in users {
                if impacted.insert((*user).to_string()) {
                    queue.push_back((*user).to_string());
                }
            }
        }
    }
    impacted.into_iter().collect()
}

fn integration_reachability_to_impacted(
    workspace_root: &Path,
    manifest: &CertifiedImpactManifest,
    impacted_src_modules: &[String],
) -> HashMap<String, bool> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return HashMap::new();
    }
    let mut module_sources = Vec::new();
    if collect_wr_modules(&tests_root, &tests_root, "tests", &mut module_sources).is_err() {
        return HashMap::new();
    }

    let src_module_set: HashSet<&str> = manifest
        .src_modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let test_module_set: HashSet<&str> = module_sources
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let known_modules: HashSet<&str> = src_module_set
        .iter()
        .copied()
        .chain(test_module_set.iter().copied())
        .collect();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for module in &manifest.src_modules {
        let deps = module
            .uses
            .iter()
            .filter(|dep| known_modules.contains(dep.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        adjacency.insert(module.module_path.clone(), deps);
    }
    for module in &module_sources {
        let deps = module
            .uses
            .iter()
            .filter(|dep| known_modules.contains(dep.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        adjacency.insert(module.module_path.clone(), deps);
    }

    let impacted: HashSet<&str> = impacted_src_modules
        .iter()
        .map(|module| module.as_str())
        .collect();
    let mut result = HashMap::new();
    for module in module_sources {
        if infer_test_lane(&module.module_path) != TestLane::Integration {
            continue;
        }
        result.insert(
            module.module_path.clone(),
            module_reaches_impacted(&module.module_path, &adjacency, &impacted),
        );
    }
    result
}

fn module_reaches_impacted(
    start: &str,
    adjacency: &HashMap<String, Vec<String>>,
    impacted: &HashSet<&str>,
) -> bool {
    let mut queue = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    queue.push_back(start.to_string());
    seen.insert(start.to_string());
    while let Some(module) = queue.pop_front() {
        if impacted.contains(module.as_str()) {
            return true;
        }
        if let Some(deps) = adjacency.get(&module) {
            for dep in deps {
                if seen.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    false
}

fn parse_cargo_package_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for raw in cargo_toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        let trimmed = value.trim();
        let unquoted = trimmed.strip_prefix('"')?.strip_suffix('"')?;
        return Some(unquoted.to_string());
    }
    None
}

pub(super) fn resolve_budget_policy_v1(
    test_jobs: Option<usize>,
    test_timeout_ms: Option<u64>,
) -> BudgetPolicyV1 {
    BudgetPolicyV1 {
        policy_version: BUDGET_POLICY_VERSION,
        test_jobs: resolve_budget_value(
            DEFAULT_TEST_JOBS,
            CEILING_TEST_JOBS,
            test_jobs.map(|v| (v as u64, "--jobs")),
            "WRELA_BUDGET_TEST_JOBS",
        ),
        test_timeout_ms: resolve_budget_value(
            DEFAULT_TEST_TIMEOUT_MS,
            CEILING_TEST_TIMEOUT_MS,
            test_timeout_ms.map(|v| (v, "--test-timeout-ms")),
            "WRELA_BUDGET_TEST_TIMEOUT_MS",
        ),
        autogen_max_cases: resolve_budget_value(
            DEFAULT_AUTOGEN_MAX_CASES,
            CEILING_AUTOGEN_MAX_CASES,
            None,
            "WRELA_BUDGET_AUTOGEN_MAX_CASES",
        ),
        sim_max_cases: resolve_budget_value(
            DEFAULT_SIM_MAX_CASES,
            CEILING_SIM_MAX_CASES,
            None,
            "WRELA_BUDGET_SIM_MAX_CASES",
        ),
        fuzz_max_cases: resolve_budget_value(
            DEFAULT_FUZZ_MAX_CASES,
            CEILING_FUZZ_MAX_CASES,
            None,
            "WRELA_BUDGET_FUZZ_MAX_CASES",
        ),
        mutation_max_cases: resolve_budget_value(
            DEFAULT_MUTATION_MAX_CASES,
            CEILING_MUTATION_MAX_CASES,
            None,
            "WRELA_BUDGET_MUTATION_MAX_CASES",
        ),
        autogen_time_cap_ms: resolve_budget_value(
            DEFAULT_AUTOGEN_TIME_CAP_MS,
            CEILING_AUTOGEN_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_AUTOGEN_TIME_CAP_MS",
        ),
        sim_time_cap_ms: resolve_budget_value(
            DEFAULT_SIM_TIME_CAP_MS,
            CEILING_SIM_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_SIM_TIME_CAP_MS",
        ),
        fuzz_time_cap_ms: resolve_budget_value(
            DEFAULT_FUZZ_TIME_CAP_MS,
            CEILING_FUZZ_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_FUZZ_TIME_CAP_MS",
        ),
        mutation_time_cap_ms: resolve_budget_value(
            DEFAULT_MUTATION_TIME_CAP_MS,
            CEILING_MUTATION_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_MUTATION_TIME_CAP_MS",
        ),
    }
}

fn resolve_budget_value(
    default: u64,
    ceiling: u64,
    cli_override: Option<(u64, &str)>,
    env_key: &str,
) -> BudgetValue {
    if let Some((requested, key)) = cli_override {
        return budget_value(default, ceiling, requested, "cli", key);
    }
    if let Some(requested) = parse_budget_env_u64(env_key) {
        return budget_value(default, ceiling, requested, "env", env_key);
    }
    budget_value(default, ceiling, default, "default", "hardcoded")
}

fn parse_budget_env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse::<u64>().ok()
}

fn budget_value(
    default: u64,
    ceiling: u64,
    requested: u64,
    source: &str,
    key: &str,
) -> BudgetValue {
    let requested = requested.max(1);
    BudgetValue {
        value: requested.min(ceiling),
        default,
        ceiling,
        provenance: BudgetProvenance {
            source: source.to_string(),
            key: key.to_string(),
            requested,
            clamped_to_ceiling: requested > ceiling,
        },
    }
}

fn verify_certification_report(cert_path: &Path) -> Result<(), String> {
    if !cert_path.exists() {
        return Err(format!(
            "verify-cert failed:\n  - cert path not found: {}",
            cert_path.display()
        ));
    }

    let payload = fs::read_to_string(cert_path).map_err(|err| {
        format!(
            "verify-cert failed:\n  - failed to read cert {}: {}",
            cert_path.display(),
            err
        )
    })?;
    let cert_json: serde_json::Value = serde_json::from_str(&payload).map_err(|err| {
        format!(
            "verify-cert failed:\n  - invalid cert JSON at {}: {}",
            cert_path.display(),
            err
        )
    })?;
    let cert_schema_version = cert_json
        .get("cert_schema_version")
        .and_then(serde_json::Value::as_u64);
    if cert_schema_version != Some(CERT_SCHEMA_VERSION as u64) {
        let got = cert_schema_version
            .map(|value| value.to_string())
            .unwrap_or_else(|| "missing".to_string());
        return Err(format!(
            "verify-cert failed:\n  - schema mismatch: expected {} but got {}",
            CERT_SCHEMA_VERSION, got
        ));
    }

    let report: CertificationReport = serde_json::from_value(cert_json).map_err(|err| {
        format!(
            "verify-cert failed:\n  - cert schema {} parse error: {}",
            CERT_SCHEMA_VERSION, err
        )
    })?;

    let mut failures: Vec<String> = Vec::new();
    if report.gate_versions_marker != CERT_GATE_VERSIONS_MARKER {
        failures.push(format!(
            "gate versions marker mismatch: expected '{}' but got '{}'",
            CERT_GATE_VERSIONS_MARKER, report.gate_versions_marker
        ));
    }
    if report.compiler_version.trim().is_empty() {
        failures.push("compiler version is empty".to_string());
    }
    if report.runtime_version.trim().is_empty() {
        failures.push("runtime version is empty".to_string());
    }

    let cert_dir = cert_path.parent().unwrap_or_else(|| Path::new("."));
    let artifact_path = resolve_cert_path(&report.artifact_path, cert_dir);
    if !artifact_path.exists() {
        failures.push(format!("binary path missing: {}", artifact_path.display()));
    } else {
        match hash_file_fingerprint(&artifact_path) {
            Ok(actual_binary_hash) => {
                if actual_binary_hash != report.binary_hash {
                    failures.push(format!(
                        "binary hash mismatch: expected {} but got {} ({})",
                        report.binary_hash,
                        actual_binary_hash,
                        artifact_path.display()
                    ));
                }
            }
            Err(err) => failures.push(format!("binary hash failed: {err}")),
        }
    }

    let workspace_root = resolve_cert_path(&report.workspace_root, cert_dir);
    if workspace_root.exists() {
        match hash_source_fingerprint(&workspace_root) {
            Ok(actual_source_hash) => {
                if actual_source_hash != report.source_hash {
                    failures.push(format!(
                        "source hash mismatch: expected {} but got {} ({})",
                        report.source_hash,
                        actual_source_hash,
                        workspace_root.display()
                    ));
                }
            }
            Err(err) => failures.push(format!("source hash failed: {err}")),
        }
    } else if !report.workspace_root.trim().is_empty() {
        failures.push(format!(
            "workspace root missing for source hash verification: {}",
            workspace_root.display()
        ));
    }

    if failures.is_empty() {
        return Ok(());
    }
    let body = failures
        .into_iter()
        .map(|line| format!("  - {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("verify-cert failed:\n{body}"))
}

fn resolve_cert_path(raw: &str, cert_dir: &Path) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cert_dir.join(path)
    }
}

pub(super) fn write_test_maintenance_summary(
    workspace_root: &Path,
    mode_record: bool,
    mode_update_public_surface: bool,
    exit_code: i32,
) -> Result<(), String> {
    let generated_at_unix_ms = now_unix_ms();
    let summary = TestMaintenanceSummary {
        version: 1,
        generated_at_unix_ms,
        workspace_root: workspace_root.display().to_string(),
        mode_record,
        mode_update_public_surface,
        exit_code,
        deployable_artifacts_emitted: false,
    };
    let payload = serde_json::to_vec_pretty(&summary).map_err(|err| err.to_string())?;
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("maintenance");
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create maintenance artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let summary_path = artifact_dir.join(format!("maintenance-{}.json", generated_at_unix_ms));
    let latest_path = artifact_dir.join("maintenance-latest.json");
    fs::write(&summary_path, &payload).map_err(|err| {
        format!(
            "failed to write maintenance summary {}: {}",
            summary_path.display(),
            err
        )
    })?;
    fs::write(&latest_path, payload).map_err(|err| {
        format!(
            "failed to write maintenance latest summary {}: {}",
            latest_path.display(),
            err
        )
    })?;
    Ok(())
}

fn hash_file_fingerprint(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read hash input {}: {}", path.display(), err))?;
    Ok(fnv1a64_hex(&bytes))
}

fn hash_source_fingerprint(workspace_root: &Path) -> Result<String, String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_hash_files(&workspace_root.join("src"), "src", "wr", &mut files)?;
    collect_hash_files(&workspace_root.join("tests"), "tests", "wr", &mut files)?;
    collect_hash_files(
        &workspace_root.join("tests").join("cassettes"),
        "tests/cassettes",
        "json",
        &mut files,
    )?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Fnv1a64::new();
    for (rel, path) in files {
        hasher.update(b"file:");
        hasher.update(rel.as_bytes());
        hasher.update(&[0]);
        let bytes = fs::read(&path).map_err(|err| {
            format!(
                "failed to read source hash input {}: {}",
                path.display(),
                err
            )
        })?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finish_hex())
}

fn collect_hash_files(
    dir: &Path,
    dir_label: &str,
    extension: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read source directory {}: {}", dir.display(), err))?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list source directory {}: {}", dir.display(), err))?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|a| path_sort_key(a));
    for child in children {
        if child.is_dir() {
            let child_name = child
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("non-utf8 path in source tree: {}", child.display()))?;
            let next_label = format!("{dir_label}/{child_name}");
            collect_hash_files(&child, &next_label, extension, out)?;
        } else if child.is_file() {
            if child.extension().and_then(|ext| ext.to_str()) != Some(extension) {
                continue;
            }
            let child_name = child
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("non-utf8 path in source tree: {}", child.display()))?;
            out.push((format!("{dir_label}/{child_name}"), child));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct WrModuleSource {
    module_path: String,
    rel_path: String,
    source: String,
    hash: String,
    uses: Vec<String>,
}

fn build_certified_impact_manifest(
    workspace_root: &Path,
) -> Result<CertifiedImpactManifest, String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_hash_files(&workspace_root.join("src"), "src", "wr", &mut files)?;
    collect_hash_files(&workspace_root.join("tests"), "tests", "wr", &mut files)?;
    collect_hash_files(
        &workspace_root.join("tests").join("cassettes"),
        "tests/cassettes",
        "json",
        &mut files,
    )?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let source_files = files
        .iter()
        .map(|(rel_path, path)| {
            let hash = hash_file_fingerprint(path)?;
            Ok(CertifiedSourceFileFingerprint {
                rel_path: rel_path.clone(),
                hash,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let src_root = workspace_root.join("src");
    let mut src_modules = Vec::new();
    collect_wr_modules(&src_root, &src_root, "src", &mut src_modules)?;
    let mut src_snapshots = src_modules
        .into_iter()
        .map(|module| CertifiedSrcModuleSnapshot {
            module_path: module.module_path,
            rel_path: module.rel_path,
            hash: module.hash,
            uses: module.uses,
            runtime_sensitive: source_looks_runtime_sensitive(&module.source),
        })
        .collect::<Vec<_>>();
    src_snapshots.sort_by(|a, b| a.module_path.cmp(&b.module_path));

    Ok(CertifiedImpactManifest {
        source_files,
        src_modules: src_snapshots,
    })
}

fn collect_wr_modules(
    root: &Path,
    strip_root: &Path,
    root_label: &str,
    out: &mut Vec<WrModuleSource>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(root).map_err(|err| {
        format!(
            "failed to read source directory {}: {}",
            root.display(),
            err
        )
    })?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "failed to list source directory {}: {}",
                root.display(),
                err
            )
        })?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|a| path_sort_key(a));
    for child in children {
        if child.is_dir() {
            collect_wr_modules(&child, strip_root, root_label, out)?;
            continue;
        }
        if child.extension().and_then(|ext| ext.to_str()) != Some("wr") {
            continue;
        }
        let source = fs::read_to_string(&child)
            .map_err(|err| format!("failed to read source file {}: {}", child.display(), err))?;
        let hash = fnv1a64_hex(source.as_bytes());
        let module_path = module_path_for_wr_file(&child, strip_root, root_label)?;
        let rel = child.strip_prefix(strip_root).map_err(|_| {
            format!(
                "file {} must live under {}",
                child.display(),
                strip_root.display()
            )
        })?;
        let rel_path = format!(
            "{}/{}",
            root_label,
            rel.to_string_lossy().replace('\\', "/")
        );
        let uses = parse_wr_use_edges(&source);
        out.push(WrModuleSource {
            module_path,
            rel_path,
            source,
            hash,
            uses,
        });
    }
    Ok(())
}

fn module_path_for_wr_file(path: &Path, root: &Path, root_label: &str) -> Result<String, String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| format!("file {} must live under {}", path.display(), root.display()))?;
    let mut rel = rel.to_path_buf();
    rel.set_extension("");
    let parts: Vec<String> = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .filter(|segment| !segment.is_empty())
        .collect();
    if root_label == "tests" {
        Ok(format!("tests/{}", parts.join("/")))
    } else {
        Ok(parts.join("/"))
    }
}

fn parse_wr_use_edges(source: &str) -> Vec<String> {
    use wrela::parser::ast::AstNode;

    let (syntax, parse_errors) = parser::parse_with_errors(source);
    if !parse_errors.is_empty() {
        return Vec::new();
    }
    let Some(root) = parser::ast::Root::cast(syntax) else {
        return Vec::new();
    };
    let module = hir::lower::lower(root);
    let mut uses = module
        .uses
        .iter()
        .map(|use_stmt| use_stmt.module.to_string())
        .filter(|module| !module.trim().is_empty())
        .collect::<Vec<_>>();
    uses.sort();
    uses.dedup();
    uses
}

fn source_looks_runtime_sensitive(source: &str) -> bool {
    let normalized = source.to_ascii_lowercase();
    [
        "actor", "pool", "runtime", "__wr_", "detach", "mailbox", "sched_",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn enforce_public_surface_gate(workspace_root: &Path) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let current_path = workspace_root.join(PUBLIC_SURFACE_CURRENT_REL_PATH);
    write_public_surface_snapshot(&current_path, &snapshot)?;
    let baseline_path = workspace_root.join(PUBLIC_SURFACE_BASELINE_REL_PATH);
    if !baseline_path.is_file() {
        return Ok(());
    }
    let baseline = load_public_surface_snapshot(&baseline_path)?;
    if baseline == snapshot {
        return Ok(());
    }
    let summary = summarize_public_surface_diff(&baseline, &snapshot);
    Err(format!(
        "public surface gate failed:\n  baseline: {}\n  current: {}\n{}\nrun `wrela test --update-public-surface` to accept the new public surface",
        baseline_path.display(),
        current_path.display(),
        summary
    ))
}

fn enforce_importable_coverage_gate(
    workspace_root: &Path,
    function_coverage: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let mut uncovered = snapshot
        .items
        .iter()
        .filter(|item| is_importable_coverage_target(&item.qualified_name))
        .filter_map(|item| {
            let hits = function_coverage_hits_for_qualified_name(
                function_coverage,
                &item.qualified_name,
            );
            (hits == 0).then_some(item.qualified_name.clone())
        })
        .collect::<Vec<_>>();
    uncovered.sort();
    if uncovered.is_empty() {
        return Ok(());
    }
    let details = uncovered
        .iter()
        .map(|name| format!("  - {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "coverage gate failed: expected 100% function coverage for importable items under src/domain/** and src/application/**.\nuncovered importable functions/checks ({}):\n{}\naction: add tests that execute each uncovered item, or mark it private if it is internal-only",
        uncovered.len(),
        details
    ))
}

fn is_importable_coverage_target(qualified_name: &str) -> bool {
    qualified_name.starts_with("domain/") || qualified_name.starts_with("application/")
}

fn function_coverage_hits_for_qualified_name(
    function_coverage: &BTreeMap<String, u64>,
    qualified_name: &str,
) -> u64 {
    let qualified_id = stable_function_id(qualified_name);
    if let Some(hits) = function_coverage.get(&qualified_id) {
        return *hits;
    }
    let short_name = qualified_name
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or(qualified_name);
    let short_id = stable_function_id(short_name);
    function_coverage.get(&short_id).copied().unwrap_or(0)
}

pub(super) fn update_public_surface_baseline(workspace_root: &Path) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let current_path = workspace_root.join(PUBLIC_SURFACE_CURRENT_REL_PATH);
    write_public_surface_snapshot(&current_path, &snapshot)?;
    let baseline_path = workspace_root.join(PUBLIC_SURFACE_BASELINE_REL_PATH);
    write_public_surface_snapshot(&baseline_path, &snapshot)?;
    println!(
        "public surface baseline updated: {}",
        baseline_path.display()
    );
    Ok(())
}

fn build_public_surface_snapshot(workspace_root: &Path) -> Result<PublicSurfaceSnapshot, String> {
    use wrela::parser::ast::AstNode;

    let src_root = workspace_root.join("src");
    let mut modules = Vec::new();
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;
    modules.sort_by(|a, b| a.module_path.cmp(&b.module_path));
    let mut items = Vec::new();
    for module in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let lowered = hir::lower::lower(root);
        for (_, function) in lowered.functions.iter() {
            if function.visibility != hir::Visibility::Public {
                continue;
            }
            if !matches!(
                function.kind,
                hir::FunctionKind::Function | hir::FunctionKind::Check
            ) {
                continue;
            }
            let qualified_name = format!("{}::{}", module.module_path, function.name);
            let signature = render_public_function_signature(function);
            let connector_literals = function
                .body
                .as_ref()
                .map(collect_public_surface_connector_literals)
                .unwrap_or_default();
            items.push(PublicSurfaceItem {
                qualified_name,
                signature,
                connector_literals,
            });
        }
    }
    items.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    Ok(PublicSurfaceSnapshot { version: 1, items })
}

fn render_public_function_signature(function: &hir::Function) -> String {
    let params = function
        .params
        .iter()
        .map(|param| {
            let ty = param
                .ty
                .as_ref()
                .map(render_public_surface_type)
                .unwrap_or_else(|| "_".to_string());
            format!("{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>();
    let ret = function
        .ret_type
        .as_ref()
        .map(render_public_surface_type)
        .unwrap_or_else(|| "Nothing".to_string());
    format!("({}) -> {ret}", params.join(", "))
}

fn render_public_surface_type(ty: &hir::TypeRef) -> String {
    if ty.args.is_empty() {
        return ty.name.to_string();
    }
    let args = ty
        .args
        .iter()
        .map(render_public_surface_type)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}[{args}]", ty.name)
}

fn collect_public_surface_connector_literals(
    body: &hir::Body,
) -> Vec<PublicSurfaceConnectorLiteral> {
    let mut literals = BTreeSet::new();
    collect_public_surface_connector_literals_from_stmts(body, &body.root_stmts, &mut literals);
    literals.into_iter().collect()
}

fn collect_public_surface_connector_literals_from_stmts(
    body: &hir::Body,
    stmts: &[hir::arena::Idx<hir::Stmt>],
    out: &mut BTreeSet<PublicSurfaceConnectorLiteral>,
) {
    for stmt_idx in stmts {
        match &body.stmts[*stmt_idx] {
            hir::Stmt::Expr(expr)
            | hir::Stmt::IgnoreResult { expr }
            | hir::Stmt::Capture { value: expr, .. }
            | hir::Stmt::Require {
                condition: expr, ..
            } => {
                collect_public_surface_connector_literals_from_expr(body, *expr, out);
            }
            hir::Stmt::Assert { expr, .. } => {
                collect_public_surface_connector_literals_from_expr(body, *expr, out);
            }
            hir::Stmt::Let { value, .. } | hir::Stmt::Assign { value, .. } => {
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
            hir::Stmt::Optimize { body: block, .. } => {
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *condition, out);
                collect_public_surface_connector_literals_from_stmts(body, then_branch, out);
                if let Some(else_branch) = else_branch {
                    collect_public_surface_connector_literals_from_stmts(body, else_branch, out);
                }
            }
            hir::Stmt::For {
                iterable,
                body: block,
                ..
            } => {
                collect_public_surface_connector_literals_from_expr(body, *iterable, out);
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *subject, out);
                for case in cases {
                    collect_public_surface_connector_literals_from_stmts(body, &case.body, out);
                }
                if let Some(otherwise) = otherwise {
                    collect_public_surface_connector_literals_from_stmts(body, otherwise, out);
                }
            }
            hir::Stmt::While {
                condition,
                body: block,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *condition, out);
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::Return(Some(value)) | hir::Stmt::Defer { expr: value } => {
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
            hir::Stmt::Return(None)
            | hir::Stmt::Use { .. }
            | hir::Stmt::Break
            | hir::Stmt::Continue => {}
        }
    }
}

fn collect_public_surface_connector_literals_from_expr(
    body: &hir::Body,
    expr_idx: hir::arena::Idx<hir::Expr>,
    out: &mut BTreeSet<PublicSurfaceConnectorLiteral>,
) {
    match &body.exprs[expr_idx] {
        hir::Expr::Literal(_) | hir::Expr::Variable(_) => {}
        hir::Expr::Detach { target, .. }
        | hir::Expr::Unary { expr: target, .. }
        | hir::Expr::TypeApply { callee: target, .. }
        | hir::Expr::Crash { expr: target } => {
            collect_public_surface_connector_literals_from_expr(body, *target, out);
        }
        hir::Expr::Binary { lhs, rhs, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *lhs, out);
            collect_public_surface_connector_literals_from_expr(body, *rhs, out);
        }
        hir::Expr::Call { callee, args, .. } => {
            if is_try_to_http_call(body, *callee)
                && let Some(literal) = extract_try_to_http_literal_tuple(body, args)
            {
                out.insert(literal);
            }
            collect_public_surface_connector_literals_from_expr(body, *callee, out);
            for arg in args {
                match arg {
                    hir::Arg::Positional { value, .. } | hir::Arg::Named { value, .. } => {
                        collect_public_surface_connector_literals_from_expr(body, *value, out);
                    }
                }
            }
        }
        hir::Expr::Member { object, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *object, out);
        }
        hir::Expr::Index { object, index, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *object, out);
            collect_public_surface_connector_literals_from_expr(body, *index, out);
        }
        hir::Expr::List(items) => {
            for item in items {
                collect_public_surface_connector_literals_from_expr(body, *item, out);
            }
        }
        hir::Expr::Map(entries) => {
            for (key, value) in entries {
                collect_public_surface_connector_literals_from_expr(body, *key, out);
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
        }
        hir::Expr::StringInterp(parts) => {
            for part in parts {
                if let hir::StringPart::Expr(expr) = part {
                    collect_public_surface_connector_literals_from_expr(body, *expr, out);
                }
            }
        }
        hir::Expr::Closure { body: closure_body, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *closure_body, out);
        }
    }
}

fn is_try_to_http_call(body: &hir::Body, callee: hir::arena::Idx<hir::Expr>) -> bool {
    match &body.exprs[callee] {
        hir::Expr::Variable(name) => name == "try_to_http_call",
        hir::Expr::TypeApply { callee, .. } => is_try_to_http_call(body, *callee),
        _ => false,
    }
}

fn extract_try_to_http_literal_tuple(
    body: &hir::Body,
    args: &[hir::Arg],
) -> Option<PublicSurfaceConnectorLiteral> {
    let mut service_named = None;
    let mut endpoint_named = None;
    let mut method_named = None;
    let mut url_named = None;
    for arg in args {
        if let hir::Arg::Named { name, value, .. } = arg {
            match name.as_str() {
                "service" => service_named = extract_literal_string(body, *value),
                "endpoint" => endpoint_named = extract_literal_string(body, *value),
                "method" => method_named = extract_literal_string(body, *value),
                "url" => url_named = extract_literal_string(body, *value),
                _ => {}
            }
        }
    }
    if let (Some(service), Some(endpoint), Some(method), Some(url)) =
        (service_named, endpoint_named, method_named, url_named)
    {
        return Some(PublicSurfaceConnectorLiteral {
            service,
            endpoint,
            method,
            url,
        });
    }

    let positional = args
        .iter()
        .filter_map(|arg| match arg {
            hir::Arg::Positional { value, .. } => Some(*value),
            hir::Arg::Named { .. } => None,
        })
        .collect::<Vec<_>>();
    if positional.len() < 4 {
        return None;
    }
    let service = extract_literal_string(body, positional[0])?;
    let endpoint = extract_literal_string(body, positional[1])?;
    let method = extract_literal_string(body, positional[2])?;
    let url = extract_literal_string(body, positional[3])?;
    Some(PublicSurfaceConnectorLiteral {
        service,
        endpoint,
        method,
        url,
    })
}

fn extract_literal_string(body: &hir::Body, expr: hir::arena::Idx<hir::Expr>) -> Option<String> {
    match &body.exprs[expr] {
        hir::Expr::Literal(hir::Literal::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn write_public_surface_snapshot(
    path: &Path,
    snapshot: &PublicSurfaceSnapshot,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|err| err.to_string())?;
    fs::write(path, bytes).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

fn load_public_surface_snapshot(path: &Path) -> Result<PublicSurfaceSnapshot, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    serde_json::from_slice::<PublicSurfaceSnapshot>(&bytes)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))
}

fn summarize_public_surface_diff(
    baseline: &PublicSurfaceSnapshot,
    current: &PublicSurfaceSnapshot,
) -> String {
    let baseline_by_name = baseline
        .items
        .iter()
        .map(|item| (item.qualified_name.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let current_by_name = current
        .items
        .iter()
        .map(|item| (item.qualified_name.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let added = current_by_name
        .keys()
        .filter(|name| !baseline_by_name.contains_key(*name))
        .copied()
        .collect::<Vec<_>>();
    let removed = baseline_by_name
        .keys()
        .filter(|name| !current_by_name.contains_key(*name))
        .copied()
        .collect::<Vec<_>>();
    let changed = baseline_by_name
        .iter()
        .filter_map(|(name, baseline_item)| {
            let current_item = current_by_name.get(name)?;
            if *baseline_item == *current_item {
                None
            } else {
                Some((*name, *baseline_item, *current_item))
            }
        })
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    if !added.is_empty() {
        lines.push(format!("  added importable items ({}):", added.len()));
        lines.extend(added.into_iter().map(|name| format!("    + {name}")));
    }
    if !removed.is_empty() {
        lines.push(format!("  removed importable items ({}):", removed.len()));
        lines.extend(removed.into_iter().map(|name| format!("    - {name}")));
    }
    if !changed.is_empty() {
        lines.push(format!("  changed importable items ({}):", changed.len()));
        for (name, baseline_item, current_item) in changed {
            lines.push(format!("    ~ {name}"));
            if baseline_item.signature != current_item.signature {
                lines.push(format!(
                    "      signature: {} -> {}",
                    baseline_item.signature, current_item.signature
                ));
            }
            if baseline_item.connector_literals != current_item.connector_literals {
                lines.push(format!(
                    "      connector_literals: {} -> {}",
                    baseline_item.connector_literals.len(),
                    current_item.connector_literals.len()
                ));
            }
        }
    }
    if lines.is_empty() {
        lines.push("  public surface changed (unable to summarize details)".to_string());
    }
    lines.join("\n")
}

pub(super) fn evaluate_connector_contract_gate(workspace_root: &Path) -> Result<(), String> {
    let root = fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let cassette_root = root.join("tests").join("cassettes");
    if !cassette_root.is_dir() {
        return Ok(());
    }
    let mut cassette_files = Vec::new();
    collect_json_files_recursive(&cassette_root, &mut cassette_files)?;
    if cassette_files.is_empty() {
        return Ok(());
    }
    let mut coverage: std::collections::BTreeMap<(String, String), (bool, bool)> =
        std::collections::BTreeMap::new();
    for file in cassette_files {
        let bytes = fs::read(&file)
            .map_err(|err| format!("failed to read cassette {}: {err}", file.display()))?;
        let cassette: ConnectorCoverageCassette = serde_json::from_slice(&bytes)
            .map_err(|err| format!("invalid cassette schema in {}: {err}", file.display()))?;
        let key = (
            cassette.request.service.clone(),
            cassette.request.endpoint.clone(),
        );
        let entry = coverage.entry(key).or_insert((false, false));
        if cassette.response.status < 400 {
            entry.0 = true;
        } else {
            entry.1 = true;
        }
    }

    let mut missing = Vec::new();
    for ((service, endpoint), (has_success, has_failure)) in coverage {
        if !has_success || !has_failure {
            missing.push(format!(
                "  - {service}/{endpoint}: success_replay={} failure_replay={}",
                has_success, has_failure
            ));
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "connector contract coverage requires both success and failure replay cassettes per endpoint:\n{}",
        missing.join("\n")
    ))
}

fn collect_json_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", dir.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|a| path_sort_key(a));
    for child in children {
        if child.is_dir() {
            collect_json_files_recursive(&child, out)?;
        } else if child.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(child);
        }
    }
    Ok(())
}

fn path_sort_key(path: &Path) -> (usize, String) {
    let rank = match (path.is_file(), path.is_dir()) {
        (true, _) => 0,
        (_, true) => 1,
        _ => 2,
    };
    (rank, path.to_string_lossy().to_string())
}

const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: FNV1A64_OFFSET_BASIS,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= *byte as u64;
            self.state = self.state.wrapping_mul(FNV1A64_PRIME);
        }
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }

    fn finish_u64(&self) -> u64 {
        self.state
    }
}

pub(super) fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_u64()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

fn project_record(
    kind: wrela::diag::catalog::ProjectDiagKind,
    severity: DiagSeverity,
    message: String,
    path: String,
    span: SourceSpan,
) -> DiagRecord {
    let desc = project_descriptor(kind);
    DiagRecord::new(desc.stage, severity, message, path, span)
        .with_code(Some(desc.code.to_string()))
        .with_help(Some(desc.help_template.to_string()))
}

fn conservative_naming_fixes(err: &hir::naming::NamingError, path: &str) -> Vec<DiagFix> {
    let span = err.primary_span();
    let span = DiagSpan {
        path: path.to_string(),
        offset: span.offset(),
        len: span.len(),
    };
    match err {
        hir::naming::NamingError::SnakeCaseRequired { name, .. } => {
            let replacement = to_snake_case(name.as_str());
            if replacement.is_empty() || replacement == name.as_str() {
                Vec::new()
            } else {
                vec![DiagFix {
                    replacement,
                    span,
                    expected_source: None,
                    rationale: "convert to ASCII snake_case".to_string(),
                    confidence: 0.99,
                    safety_tier: "safe".to_string(),
                    reason_code: "naming.snake_case_transform".to_string(),
                }]
            }
        }
        hir::naming::NamingError::PascalCaseRequired { name, .. } => {
            let replacement = to_pascal_case(name.as_str());
            if replacement.is_empty() || replacement == name.as_str() {
                Vec::new()
            } else {
                vec![DiagFix {
                    replacement,
                    span,
                    expected_source: None,
                    rationale: "convert to ASCII PascalCase".to_string(),
                    confidence: 0.99,
                    safety_tier: "safe".to_string(),
                    reason_code: "naming.pascal_case_transform".to_string(),
                }]
            }
        }
        hir::naming::NamingError::BooleanPrefixRequired { name, .. } => {
            if name.starts_with("is_") || name.starts_with("has_") {
                Vec::new()
            } else {
                vec![DiagFix {
                    replacement: format!("is_{}", to_snake_case(name.as_str())),
                    span,
                    expected_source: None,
                    rationale: "boolean identifiers should start with `is_` or `has_`".to_string(),
                    confidence: 0.96,
                    safety_tier: "safe".to_string(),
                    reason_code: "naming.boolean_prefix_transform".to_string(),
                }]
            }
        }
        _ => Vec::new(),
    }
}

fn to_snake_case(input: &str) -> String {
    let mut out = String::new();
    let mut prev_was_sep = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if !out.is_empty() && !prev_was_sep {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch.to_ascii_lowercase());
            }
            prev_was_sep = false;
        } else if !prev_was_sep {
            out.push('_');
            prev_was_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

fn to_pascal_case(input: &str) -> String {
    let mut out = String::new();
    let mut cap = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if cap {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch.to_ascii_lowercase());
            }
            cap = false;
        } else {
            cap = true;
        }
    }
    out
}

fn resolve_path_from_owner_spans(
    span: SourceSpan,
    provenance: &hir::project::ProjectProvenance,
    default_path: &str,
) -> String {
    let offset = span.offset();
    let mut candidates = provenance
        .function_owner_span_by_id
        .iter()
        .filter_map(|(func_id, owner_span)| {
            let start = usize::from(owner_span.start());
            let end = usize::from(owner_span.end());
            if offset >= start && offset <= end {
                provenance
                    .function_owner_path_by_id
                    .get(func_id)
                    .map(|path| (end.saturating_sub(start), path.display().to_string()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(width, _)| *width);
    candidates
        .first()
        .map(|(_, path)| path.clone())
        .unwrap_or_else(|| default_path.to_string())
}

pub(super) enum TestTarget {
    ProjectRoot(PathBuf),
    SingleFile(PathBuf),
}

pub(super) fn resolve_test_target(path_arg: Option<&str>) -> Result<TestTarget, String> {
    let path = PathBuf::from(path_arg.unwrap_or("."));
    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("wr") {
            return Ok(TestTarget::SingleFile(path));
        }
        return Err(format!(
            "test file must have .wr extension: {}",
            path.display()
        ));
    }
    if path.is_dir() {
        return Ok(TestTarget::ProjectRoot(path));
    }
    Err("test target must be an existing project-root directory or .wr file".to_string())
}

pub(super) fn resolve_benchmark_manifest_path(
    target: &TestTarget,
    override_path: Option<String>,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(PathBuf::from(path));
    }
    let TestTarget::ProjectRoot(root) = target else {
        return None;
    };
    let candidate = root.join("bench.toml");
    candidate.is_file().then_some(candidate)
}

pub(super) fn load_benchmark_manifest(path: &Path) -> Result<BenchmarkManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let manifest: BenchmarkManifest =
        toml::from_str(&text).map_err(|err| format!("failed to parse bench.toml: {err}"))?;
    if manifest.version != 1 {
        return Err(format!(
            "unsupported benchmark manifest version {}; expected 1",
            manifest.version
        ));
    }
    if manifest.scenarios.is_empty() {
        return Err("benchmark manifest must define at least one scenario".to_string());
    }
    for scenario in &manifest.scenarios {
        let func_name = scenario
            .test_name
            .rsplit("::")
            .next()
            .unwrap_or(scenario.test_name.as_str());
        let expected_suffix = format!("_ops_{}", scenario.ops);
        if !func_name.ends_with(expected_suffix.as_str()) {
            return Err(format!(
                "scenario `{}` test `{}` must end with `{}`",
                scenario.id, scenario.test_name, expected_suffix
            ));
        }
    }
    Ok(manifest)
}
