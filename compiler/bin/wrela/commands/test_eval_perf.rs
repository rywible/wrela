use super::build_compile::{
    BudgetPolicyV1, COVERAGE_INDEX_SCHEMA_VERSION, COVERAGE_SNAPSHOT_SCHEMA_VERSION,
    CertSelectionReport, DEFAULT_TEST_TIMEOUT_MS, Fnv1a64, MUTATION_CACHE_ENGINE_TAG,
    MUTATION_CACHE_SCHEMA_VERSION, MUTATION_KILL_HISTORY_SCHEMA_VERSION,
    TEST_HARNESS_META_SCHEMA_VERSION, TestTarget, build_public_surface_snapshot,
    collect_wr_modules, conservative_naming_fixes, emit_cert_selection_report, fnv1a64,
    fnv1a64_hex, hash_source_fingerprint, is_importable_coverage_target, load_benchmark_manifest,
    now_unix_ms, path_sort_key, project_record, resolve_path_from_owner_spans,
    resolve_toolchain_version,
};
use super::shared::{
    naming_policy_severity, naming_policy_tier, project_naming_diagnostics, repro,
};
use super::{
    AstNode, BTreeMap, BTreeSet, Command, CommandSpec, Deserialize, DiagFix, DiagRecord,
    DiagSeverity, DiagSpan, DiagStage, Duration, EXIT_CODEGEN, EXIT_OK, EXIT_PARSE,
    EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, HashMap, HashSet, Instant, Output, OutputFormat,
    ParsedCommandSpec, Path, PathBuf, Serialize, SmolStr, SourceSpan, SystemTime, UNIX_EPOCH,
    VecDeque, ast, cert_engine, dedupe_records, diag_emit, env, fs, hir, hir_lower, io, mir,
    mir_descriptor, parser, perf_engine, project_descriptor, replay_trace, suppress_cascades,
};

pub(crate) fn discover_tests_for_target(target: &TestTarget) -> Result<Vec<TestCase>, String> {
    match target {
        TestTarget::ProjectRoot(root) => {
            let tests_root = root.join("tests");
            let mut tests = Vec::new();
            collect_tests(&tests_root, &tests_root, &mut tests).map_err(|err| err.to_string())?;
            tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            Ok(tests)
        }
        TestTarget::SingleFile(_) => {
            Err("benchmark manifests require project-root targets with tests/".to_string())
        }
    }
}

pub(crate) fn build_benchmark_selection(
    target: &TestTarget,
    manifest_path: &Path,
    profile: PerfProfile,
) -> Result<HashSet<String>, String> {
    let manifest = load_benchmark_manifest(manifest_path)?;
    let tests = discover_tests_for_target(target)?;
    let test_by_name: HashMap<&str, &TestCase> = tests
        .iter()
        .map(|test| (test.name.as_str(), test))
        .collect();
    let mut include_ids = HashSet::new();
    for scenario in manifest.scenarios_for_profile(profile) {
        let Some(test) = test_by_name.get(scenario.test_name.as_str()) else {
            return Err(format!(
                "scenario `{}` references unknown test `{}`",
                scenario.id, scenario.test_name
            ));
        };
        include_ids.insert(test.id.clone());
    }
    if include_ids.is_empty() {
        return Err("benchmark profile selected zero scenarios".to_string());
    }
    Ok(include_ids)
}

#[derive(Clone)]
pub(crate) struct TestCase {
    pub(crate) id: String,
    pub(crate) lane: TestLane,
    pub(crate) name: String,
    pub(crate) module_path: String,
    pub(crate) func_name: String,
    pub(crate) is_serial: bool,
    pub(crate) allows_env_set: bool,
    pub(crate) allows_fs_escape: bool,
    pub(crate) has_oracle: bool,
    pub(crate) generated_call_body: Option<String>,
    pub(crate) generated_case_kind: Option<GeneratedCaseKind>,
    pub(crate) generated_entry_source: Option<String>,
    pub(crate) autogen_module_source: Option<String>,
    pub(crate) autogen_seed: Option<u64>,
    pub(crate) autogen_span: Option<String>,
    pub(crate) sim_seed: Option<u64>,
    pub(crate) canonical_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestLane {
    Spec,
    Integration,
    Sim,
    Model,
    Default,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestLanePreset {
    Fast,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestLaneSelection {
    Single(TestLane),
    Preset(TestLanePreset),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneratedCaseKind {
    Autogen,
    Fuzz,
}

impl TestLane {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TestLane::Spec => "spec",
            TestLane::Integration => "integration",
            TestLane::Sim => "sim",
            TestLane::Model => "model",
            TestLane::Default => "default",
        }
    }
}

impl TestLaneSelection {
    pub(crate) fn matches(self, lane: TestLane) -> bool {
        match self {
            TestLaneSelection::Single(selected) => lane == selected,
            TestLaneSelection::Preset(TestLanePreset::Fast) => {
                matches!(lane, TestLane::Spec | TestLane::Default)
            }
            TestLaneSelection::Preset(TestLanePreset::Full) => true,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum HttpCassetteMode {
    Replay,
    Record,
}

#[derive(Clone, Copy)]
pub(crate) enum DifferentialPipeline {
    Baseline,
    Alt,
}

impl DifferentialPipeline {
    pub(crate) fn as_env_value(self) -> &'static str {
        match self {
            DifferentialPipeline::Baseline => "baseline",
            DifferentialPipeline::Alt => "alt",
        }
    }
}

#[derive(Clone)]
pub(crate) struct AutogenCheckDecl {
    pub(crate) module_path: String,
    pub(crate) func_name: String,
    pub(crate) params: Vec<AutogenCheckParam>,
    pub(crate) module_source: String,
    pub(crate) source_span: Option<String>,
}

pub(crate) const REPRO_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum ReproArtifact {
    Autogen(AutogenReproArtifact),
    Fuzz(FuzzReproArtifact),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AutogenReproArtifact {
    pub(crate) version: u32,
    pub(crate) generated_at_unix_ms: u64,
    pub(crate) workspace_root: String,
    pub(crate) test_id: String,
    pub(crate) module_path: String,
    pub(crate) func_name: String,
    pub(crate) seed: u64,
    pub(crate) span: Option<String>,
    pub(crate) original_call: String,
    pub(crate) shrunk_call: Option<String>,
    pub(crate) replay_call: String,
    pub(crate) failure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FuzzReproArtifact {
    pub(crate) version: u32,
    pub(crate) generated_at_unix_ms: u64,
    pub(crate) workspace_root: String,
    pub(crate) test_id: String,
    pub(crate) module_path: String,
    pub(crate) func_name: String,
    pub(crate) seed: u64,
    pub(crate) span: Option<String>,
    pub(crate) call: String,
    pub(crate) uses_bytes_helper: bool,
    pub(crate) failure: String,
}

#[derive(Clone)]
pub(crate) struct AutogenCheckParam {
    pub(crate) name: String,
    pub(crate) ty: AutogenScalarType,
}

#[derive(Clone)]
pub(crate) struct FuzzTargetDecl {
    pub(crate) module_path: String,
    pub(crate) func_name: String,
    pub(crate) param_name: String,
    pub(crate) param_ty: FuzzParamType,
    pub(crate) module_source: String,
    pub(crate) source_span: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FuzzParamType {
    String,
    Bytes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutogenScalarType {
    Integer,
    Boolean,
    String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MutationGateReport {
    pub(crate) version: u32,
    pub(crate) generated_at_unix_ms: u128,
    pub(crate) discovery_ms: u128,
    pub(crate) execution_ms: u128,
    pub(crate) compile_total_ms: u128,
    pub(crate) test_run_total_ms: u128,
    pub(crate) parallel_workers: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_invalidations: usize,
    pub(crate) total_mutants: usize,
    pub(crate) valid_mutants: usize,
    pub(crate) invalid_mutants: usize,
    pub(crate) killed_mutants: usize,
    pub(crate) survived_mutants: usize,
    pub(crate) no_covering_tests_mutants: usize,
    pub(crate) kill_rate_pct: f64,
    pub(crate) domain_application_kill_rate_pct: Option<f64>,
    pub(crate) mutants: Vec<MutationMutantResult>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MutationMutantResult {
    pub(crate) function: String,
    pub(crate) function_id: String,
    pub(crate) mutation_type: String,
    pub(crate) tests_ran: Vec<String>,
    pub(crate) compile_ms: u128,
    pub(crate) test_run_ms: u128,
    pub(crate) status: String,
    pub(crate) reason: Option<String>,
}

pub(crate) struct MutationGateOutcome {
    pub(crate) summary_hash: Option<String>,
    pub(crate) discovery_ms: u128,
    pub(crate) execution_ms: u128,
}

pub(crate) struct MutationExecutionResult {
    pub(crate) job_index: usize,
    pub(crate) mutant: MutationMutantResult,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_invalidations: usize,
}

#[derive(Clone)]
pub(crate) struct MutationCandidateJob {
    pub(crate) job_index: usize,
    pub(crate) candidate: MirMutationCandidate,
    pub(crate) tests_to_run: Vec<TestCase>,
}

#[derive(Clone)]
pub(crate) struct MutationExecutionContext {
    pub(crate) workspace_root: PathBuf,
    pub(crate) source_hash: String,
    pub(crate) toolchain_version: String,
    pub(crate) cache_root: PathBuf,
    pub(crate) cache_enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MutationCacheMetadata {
    pub(crate) schema_version: u32,
    pub(crate) toolchain_version: String,
    pub(crate) source_hash: String,
    pub(crate) candidate_key: String,
    pub(crate) mutant_binary_path: String,
    pub(crate) build_status: String,
    pub(crate) invalid_reason: Option<String>,
    pub(crate) compile_ms: u128,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct MutationKillHistoryArtifact {
    pub(crate) schema_version: u32,
    pub(crate) entries: BTreeMap<String, MutationKillHistoryEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct MutationKillHistoryEntry {
    pub(crate) kills: u64,
    pub(crate) attempts: u64,
    pub(crate) last_seen_unix_ms: u128,
}

pub(crate) struct MutantCompileSuccess {
    pub(crate) exe_path: PathBuf,
    pub(crate) compile_ms: u128,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_invalidations: usize,
}

pub(crate) struct MutantCompileFailure {
    pub(crate) reason: String,
    pub(crate) compile_ms: u128,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) cache_invalidations: usize,
}

impl HttpCassetteMode {
    pub(crate) fn as_env_value(self) -> &'static str {
        match self {
            HttpCassetteMode::Replay => "replay",
            HttpCassetteMode::Record => "record",
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct TestSelection {
    pub(crate) list: bool,
    pub(crate) id: Option<String>,
    pub(crate) filter: Option<String>,
    pub(crate) lane: Option<TestLaneSelection>,
    pub(crate) include_ids: Option<HashSet<String>>,
    pub(crate) cert_selection_report: Option<CertSelectionReport>,
}

pub(crate) fn test_selection_has_filters(selection: &TestSelection) -> bool {
    selection.list || selection.id.is_some() || selection.filter.is_some()
}

pub(crate) fn set_test_selection_include_ids(
    selection: &mut TestSelection,
    include_ids: HashSet<String>,
) {
    selection.include_ids = Some(include_ids);
}

pub(crate) fn budget_jobs_timeout(budget_policy: &BudgetPolicyV1) -> (usize, Duration) {
    (
        budget_policy.test_jobs.value as usize,
        Duration::from_millis(budget_policy.test_timeout_ms.value),
    )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct MetricsDump {
    pub(crate) messages_sent: u64,
    pub(crate) messages_dropped: u64,
    pub(crate) pending_resolved: u64,
    pub(crate) pending_dropped: u64,
    pub(crate) mailbox_high_water: u64,
    pub(crate) rc_inc: u64,
    pub(crate) rc_dec: u64,
    pub(crate) alloc_list: u64,
    pub(crate) alloc_map: u64,
    pub(crate) alloc_string: u64,
    pub(crate) alloc_bytes: u64,
    pub(crate) alloc_result: u64,
    pub(crate) alloc_pending: u64,
    pub(crate) mailbox_enqueue_ok: u64,
    pub(crate) mailbox_enqueue_fail: u64,
    pub(crate) mailbox_dequeue: u64,
    #[serde(default)]
    pub(crate) sched_dispatched: u64,
    #[serde(default)]
    pub(crate) sched_skipped_no_credit: u64,
    #[serde(default)]
    pub(crate) sched_profile_switch: u64,
    #[serde(default)]
    pub(crate) sched_starvation_violation: u64,
    #[serde(default)]
    pub(crate) sched_cross_shard_migration: u64,
    #[serde(default)]
    pub(crate) abi_typed_lane: u64,
    #[serde(default)]
    pub(crate) abi_boxed_lane: u64,
    #[serde(default)]
    pub(crate) queue_cas_retry_total: u64,
    #[serde(default)]
    pub(crate) mailbox_wake_coalesced_count: u64,
    #[serde(default)]
    pub(crate) mailbox_rescue_wake_count: u64,
    #[serde(default)]
    pub(crate) sched_local_dispatch_count: u64,
    #[serde(default)]
    pub(crate) sched_global_dispatch_count: u64,
    #[serde(default)]
    pub(crate) sched_plan_recompute_count: u64,
    #[serde(default)]
    pub(crate) sched_steal_attempts: u64,
    #[serde(default)]
    pub(crate) sched_steal_success: u64,
    #[serde(default)]
    pub(crate) sched_migration_blocked_hysteresis: u64,
    #[serde(default)]
    pub(crate) sched_migration_blocked_cooldown: u64,
    #[serde(default)]
    pub(crate) queue_enqueue_p99_ns: u128,
    #[serde(default)]
    pub(crate) queue_dequeue_p99_ns: u128,
    #[serde(default)]
    pub(crate) queue_age_p99_ns: u128,
    #[serde(default)]
    pub(crate) sched_dispatch_loop_ns_p99: u128,
    #[serde(default)]
    pub(crate) queue_burst_drain_avg: f64,
    #[serde(default)]
    pub(crate) scene_trace: u64,
    #[serde(default)]
    pub(crate) field_sample: u64,
    #[serde(default)]
    pub(crate) scene_trace_support_pruned_branch: u64,
    #[serde(default)]
    pub(crate) scene_trace_candidate_branch: u64,
    #[serde(default)]
    pub(crate) scene_trace_exact_path: u64,
    #[serde(default)]
    pub(crate) scene_trace_conservative_path: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_count: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_steps_total: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_field_samples_total: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_1: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_4: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_8: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_16: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_gt_16: u64,
    #[serde(default)]
    pub(crate) scene_trace_blend_cost: u64,
    #[serde(default)]
    pub(crate) scene_trace_deformation_cost: u64,
    #[serde(default)]
    pub(crate) function_coverage: BTreeMap<String, u64>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MetricsTotals {
    pub(crate) messages_sent: u64,
    pub(crate) messages_dropped: u64,
    pub(crate) pending_resolved: u64,
    pub(crate) pending_dropped: u64,
    pub(crate) mailbox_high_water: u64,
    pub(crate) rc_inc: u64,
    pub(crate) rc_dec: u64,
    pub(crate) alloc_list: u64,
    pub(crate) alloc_map: u64,
    pub(crate) alloc_string: u64,
    pub(crate) alloc_bytes: u64,
    pub(crate) alloc_result: u64,
    pub(crate) alloc_pending: u64,
    pub(crate) mailbox_enqueue_ok: u64,
    pub(crate) mailbox_enqueue_fail: u64,
    pub(crate) mailbox_dequeue: u64,
    pub(crate) sched_dispatched: u64,
    pub(crate) sched_skipped_no_credit: u64,
    #[serde(default)]
    pub(crate) sched_profile_switch: u64,
    #[serde(default)]
    pub(crate) sched_starvation_violation: u64,
    #[serde(default)]
    pub(crate) sched_cross_shard_migration: u64,
    #[serde(default)]
    pub(crate) abi_typed_lane: u64,
    #[serde(default)]
    pub(crate) abi_boxed_lane: u64,
    #[serde(default)]
    pub(crate) queue_cas_retry_total: u64,
    #[serde(default)]
    pub(crate) mailbox_wake_coalesced_count: u64,
    #[serde(default)]
    pub(crate) mailbox_rescue_wake_count: u64,
    #[serde(default)]
    pub(crate) sched_local_dispatch_count: u64,
    #[serde(default)]
    pub(crate) sched_global_dispatch_count: u64,
    #[serde(default)]
    pub(crate) sched_plan_recompute_count: u64,
    #[serde(default)]
    pub(crate) sched_steal_attempts: u64,
    #[serde(default)]
    pub(crate) sched_steal_success: u64,
    #[serde(default)]
    pub(crate) sched_migration_blocked_hysteresis: u64,
    #[serde(default)]
    pub(crate) sched_migration_blocked_cooldown: u64,
    #[serde(default)]
    pub(crate) queue_enqueue_p99_ns: u128,
    #[serde(default)]
    pub(crate) queue_dequeue_p99_ns: u128,
    #[serde(default)]
    pub(crate) queue_age_p99_ns: u128,
    #[serde(default)]
    pub(crate) sched_dispatch_loop_ns_p99: u128,
    #[serde(default)]
    pub(crate) queue_burst_drain_avg: f64,
    #[serde(default)]
    pub(crate) scene_trace: u64,
    #[serde(default)]
    pub(crate) field_sample: u64,
    #[serde(default)]
    pub(crate) scene_trace_support_pruned_branch: u64,
    #[serde(default)]
    pub(crate) scene_trace_candidate_branch: u64,
    #[serde(default)]
    pub(crate) scene_trace_exact_path: u64,
    #[serde(default)]
    pub(crate) scene_trace_conservative_path: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_count: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_steps_total: u64,
    #[serde(default)]
    pub(crate) scene_trace_hit_field_samples_total: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_1: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_4: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_8: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_le_16: u64,
    #[serde(default)]
    pub(crate) scene_trace_steps_gt_16: u64,
    #[serde(default)]
    pub(crate) scene_trace_blend_cost: u64,
    #[serde(default)]
    pub(crate) scene_trace_deformation_cost: u64,
    #[serde(default)]
    pub(crate) function_coverage: BTreeMap<String, u64>,
}

impl MetricsTotals {
    pub(crate) fn add(&mut self, metrics: &MetricsDump) {
        self.messages_sent += metrics.messages_sent;
        self.messages_dropped += metrics.messages_dropped;
        self.pending_resolved += metrics.pending_resolved;
        self.pending_dropped += metrics.pending_dropped;
        self.mailbox_high_water = self.mailbox_high_water.max(metrics.mailbox_high_water);
        self.rc_inc += metrics.rc_inc;
        self.rc_dec += metrics.rc_dec;
        self.alloc_list += metrics.alloc_list;
        self.alloc_map += metrics.alloc_map;
        self.alloc_string += metrics.alloc_string;
        self.alloc_bytes += metrics.alloc_bytes;
        self.alloc_result += metrics.alloc_result;
        self.alloc_pending += metrics.alloc_pending;
        self.mailbox_enqueue_ok += metrics.mailbox_enqueue_ok;
        self.mailbox_enqueue_fail += metrics.mailbox_enqueue_fail;
        self.mailbox_dequeue += metrics.mailbox_dequeue;
        self.sched_dispatched += metrics.sched_dispatched;
        self.sched_skipped_no_credit += metrics.sched_skipped_no_credit;
        self.sched_profile_switch += metrics.sched_profile_switch;
        self.sched_starvation_violation += metrics.sched_starvation_violation;
        self.sched_cross_shard_migration += metrics.sched_cross_shard_migration;
        self.abi_typed_lane += metrics.abi_typed_lane;
        self.abi_boxed_lane += metrics.abi_boxed_lane;
        self.queue_cas_retry_total += metrics.queue_cas_retry_total;
        self.mailbox_wake_coalesced_count += metrics.mailbox_wake_coalesced_count;
        self.mailbox_rescue_wake_count += metrics.mailbox_rescue_wake_count;
        self.sched_local_dispatch_count += metrics.sched_local_dispatch_count;
        self.sched_global_dispatch_count += metrics.sched_global_dispatch_count;
        self.sched_plan_recompute_count += metrics.sched_plan_recompute_count;
        self.sched_steal_attempts += metrics.sched_steal_attempts;
        self.sched_steal_success += metrics.sched_steal_success;
        self.sched_migration_blocked_hysteresis += metrics.sched_migration_blocked_hysteresis;
        self.sched_migration_blocked_cooldown += metrics.sched_migration_blocked_cooldown;
        self.queue_enqueue_p99_ns = self.queue_enqueue_p99_ns.max(metrics.queue_enqueue_p99_ns);
        self.queue_dequeue_p99_ns = self.queue_dequeue_p99_ns.max(metrics.queue_dequeue_p99_ns);
        self.queue_age_p99_ns = self.queue_age_p99_ns.max(metrics.queue_age_p99_ns);
        self.sched_dispatch_loop_ns_p99 = self
            .sched_dispatch_loop_ns_p99
            .max(metrics.sched_dispatch_loop_ns_p99);
        self.queue_burst_drain_avg = self
            .queue_burst_drain_avg
            .max(metrics.queue_burst_drain_avg);
        self.scene_trace += metrics.scene_trace;
        self.field_sample += metrics.field_sample;
        self.scene_trace_support_pruned_branch += metrics.scene_trace_support_pruned_branch;
        self.scene_trace_candidate_branch += metrics.scene_trace_candidate_branch;
        self.scene_trace_exact_path += metrics.scene_trace_exact_path;
        self.scene_trace_conservative_path += metrics.scene_trace_conservative_path;
        self.scene_trace_hit_count += metrics.scene_trace_hit_count;
        self.scene_trace_hit_steps_total += metrics.scene_trace_hit_steps_total;
        self.scene_trace_hit_field_samples_total += metrics.scene_trace_hit_field_samples_total;
        self.scene_trace_steps_le_1 += metrics.scene_trace_steps_le_1;
        self.scene_trace_steps_le_4 += metrics.scene_trace_steps_le_4;
        self.scene_trace_steps_le_8 += metrics.scene_trace_steps_le_8;
        self.scene_trace_steps_le_16 += metrics.scene_trace_steps_le_16;
        self.scene_trace_steps_gt_16 += metrics.scene_trace_steps_gt_16;
        self.scene_trace_blend_cost += metrics.scene_trace_blend_cost;
        self.scene_trace_deformation_cost += metrics.scene_trace_deformation_cost;
        for (function_id, hits) in &metrics.function_coverage {
            *self
                .function_coverage
                .entry(function_id.clone())
                .or_insert(0) += *hits;
        }
    }

    pub(crate) fn total_allocs(&self) -> u64 {
        self.alloc_list
            + self.alloc_map
            + self.alloc_string
            + self.alloc_bytes
            + self.alloc_result
            + self.alloc_pending
    }
}

pub(crate) struct TestExecution {
    pub(crate) exit: i32,
    pub(crate) summary: Option<PerfSummary>,
    pub(crate) differential_results_hash: Option<String>,
    pub(crate) mutation_summary_hash: Option<String>,
    pub(crate) cert_timings: CertPerfTimings,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct CertPerfTimings {
    pub(crate) collect_tests_ms: u128,
    pub(crate) compile_harness_ms: u128,
    pub(crate) determinism_ms: u128,
    pub(crate) mutation_discovery_ms: u128,
    pub(crate) mutation_execution_ms: u128,
    pub(crate) differential_ms: u128,
}

pub(crate) struct TestRun {
    pub(crate) metrics: Option<MetricsDump>,
    pub(crate) runtime_ns: u128,
}

#[derive(Clone)]
pub(crate) struct TestHarness {
    pub(crate) exe_path: PathBuf,
    pub(crate) compile_ns: u128,
    pub(crate) cache_hit: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TestHarnessMeta {
    pub(crate) schema_version: u32,
    pub(crate) compiler_version: String,
    pub(crate) selected_tests_fingerprint: String,
    pub(crate) source_fingerprint: String,
}

#[derive(Default)]
pub(crate) struct RunOnceTimings {
    pub(crate) collect_tests_ms: u128,
    pub(crate) compile_harness_ms: u128,
}

#[derive(Serialize)]
pub(crate) struct TestJsonSummary {
    pub(crate) run: TestJsonRunMetadata,
    pub(crate) tests: Vec<TestJsonCase>,
    pub(crate) timings: TestJsonTimings,
}

#[derive(Serialize)]
pub(crate) struct TestJsonRunMetadata {
    pub(crate) seed: u64,
    pub(crate) lane: String,
    pub(crate) jobs: usize,
    pub(crate) harness_cache_hit: bool,
    pub(crate) budgets_used: BudgetPolicyV1,
}

#[derive(Serialize)]
pub(crate) struct TestJsonCase {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) lane: String,
    pub(crate) status: String,
    pub(crate) duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct TestJsonTimings {
    pub(crate) discovery_ms: u128,
    pub(crate) selection_ms: u128,
    pub(crate) compile_harness_ms: u128,
    pub(crate) execution_ms: u128,
    pub(crate) total_ms: u128,
}

pub(crate) const TEST_JSON_SUMMARY_SEED: u64 = 0x5A17;

#[derive(Clone)]
pub(crate) struct DeterminismSignature {
    pub(crate) hash: String,
    pub(crate) outcomes: Vec<DeterminismOutcome>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DeterminismOutcome {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) lane: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PerfCaseSample {
    #[serde(default)]
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) compile_ns: u128,
    pub(crate) runtime_ns: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metrics: Option<MetricsDump>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PerfSummary {
    pub(crate) sample_count: usize,
    pub(crate) compile_throughput_tests_per_sec: f64,
    pub(crate) runtime_p50_ns: u128,
    pub(crate) runtime_p95_ns: u128,
    pub(crate) runtime_p99_ns: u128,
    pub(crate) allocs_per_request: f64,
    pub(crate) rc_inc: u64,
    pub(crate) rc_dec: u64,
    pub(crate) rc_ops_total: u64,
    pub(crate) dispatch_hit_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check_fallback_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) avg_check_batch_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check_oracle_eval_ns_p50: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check_oracle_eval_ns_p95: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) effect_annihilation_rewrite_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_dispatch_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_starvation_violations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rewrite_compile_overhead_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rewrite_applied_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor_msgs_per_sec_p50: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor_msgs_per_sec_p95: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queue_enqueue_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queue_dequeue_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queue_age_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mailbox_wake_coalesced_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mailbox_rescue_wake_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queue_cas_retry_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cases: Option<Vec<PerfCaseSample>>,
    pub(crate) metrics: MetricsTotals,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub(crate) struct KpiThresholds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check_fallback_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check_batch_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_p99_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rewrite_overhead_max_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actor_throughput_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) queue_age_p99_max_regress_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) starvation_violations_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_throughput_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_loop_p99_max_regress_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scheduler_local_hit_min: Option<f64>,
}

impl KpiThresholds {
    pub(crate) fn any_set(&self) -> bool {
        self.check_fallback_max.is_some()
            || self.check_batch_min.is_some()
            || self.scheduler_p99_improve_min_pct.is_some()
            || self.rewrite_overhead_max_pct.is_some()
            || self.actor_throughput_improve_min_pct.is_some()
            || self.queue_age_p99_max_regress_pct.is_some()
            || self.starvation_violations_max.is_some()
            || self.scheduler_throughput_improve_min_pct.is_some()
            || self.scheduler_loop_p99_max_regress_pct.is_some()
            || self.scheduler_local_hit_min.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PerfCv {
    pub(crate) compile_throughput_pct: f64,
    pub(crate) runtime_p50_pct: f64,
    pub(crate) runtime_p95_pct: f64,
    pub(crate) runtime_p99_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PerfReport {
    pub(crate) version: u32,
    pub(crate) generated_at_unix_ms: u128,
    pub(crate) runs: usize,
    pub(crate) cv: PerfCv,
    pub(crate) summary: PerfSummary,
    pub(crate) samples: Vec<PerfSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) closure: Option<wrela::perf_target::PerfClosureReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) presentation_reports: Option<Vec<PresentationBenchmarkReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) whole_frame_reports: Option<Vec<WholeFrameBenchmarkReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) collision_reports: Option<Vec<CollisionBenchmarkReport>>,
}

#[derive(Debug, Clone)]
pub(crate) struct PerfGateConfig {
    pub(crate) baseline_path: PathBuf,
    pub(crate) max_regression_pct: f64,
    pub(crate) kpi_thresholds: KpiThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PerfProfile {
    Smoke,
    Standard,
    Deep,
    Closure1080p120,
}

impl PerfProfile {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "smoke" => Some(Self::Smoke),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            "1080p120" | "canonical_1080p120" | "closure" | "realtime_120" => {
                Some(Self::Closure1080p120)
            }
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Standard => "standard",
            Self::Deep => "deep",
            Self::Closure1080p120 => "1080p120",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BenchmarkManifest {
    pub(crate) version: u32,
    pub(crate) suite: String,
    #[serde(default)]
    pub(crate) optional: bool,
    #[serde(default)]
    pub(crate) profiles: BenchmarkProfiles,
    pub(crate) scenarios: Vec<BenchmarkScenario>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct BenchmarkProfiles {
    #[serde(default)]
    pub(crate) smoke: Option<BenchmarkProfileConfig>,
    #[serde(default)]
    pub(crate) standard: Option<BenchmarkProfileConfig>,
    #[serde(default)]
    pub(crate) deep: Option<BenchmarkProfileConfig>,
    #[serde(default)]
    pub(crate) closure_1080p120: Option<BenchmarkProfileConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BenchmarkProfileConfig {
    pub(crate) warmup_pairs: usize,
    pub(crate) measure_pairs: usize,
    pub(crate) coverage: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BenchmarkScenario {
    pub(crate) id: String,
    pub(crate) test_name: String,
    pub(crate) ops: u64,
    pub(crate) class: String,
    #[serde(default)]
    pub(crate) min_runtime_ms: Option<u64>,
    #[serde(default)]
    pub(crate) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(crate) allow_unstable: bool,
    #[serde(default)]
    pub(crate) presentation: Option<BenchmarkPresentationSpec>,
    #[serde(default)]
    pub(crate) collision: Option<BenchmarkCollisionSpec>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct BenchmarkPresentationSpec {
    pub(crate) view: String,
    pub(crate) region: String,
    #[serde(default)]
    pub(crate) entry: Option<String>,
    #[serde(default)]
    pub(crate) domain: Option<String>,
    #[serde(default)]
    pub(crate) width: Option<u32>,
    #[serde(default)]
    pub(crate) height: Option<u32>,
    #[serde(default)]
    pub(crate) frames: Option<u32>,
    pub(crate) camera_position: [f32; 3],
    pub(crate) camera_forward: [f32; 3],
    pub(crate) camera_up: [f32; 3],
    pub(crate) vertical_fov_degrees: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct BenchmarkCollisionSpec {
    #[serde(default)]
    pub(crate) entry: Option<String>,
    pub(crate) region: String,
    pub(crate) domain: String,
    pub(crate) workload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PresentationWgslWorkgroupComparison {
    pub(crate) selected_workgroup_size: u32,
    pub(crate) candidate_workgroup_sizes: Vec<u32>,
    pub(crate) candidate_frame_time_ns: Vec<u128>,
    pub(crate) frame_time_ns_delta_vs_selected: Vec<i128>,
    pub(crate) frame_time_ns_delta_vs_selected_pct: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PresentationBenchmarkReport {
    pub(crate) scenario_id: String,
    pub(crate) test_name: String,
    pub(crate) view: String,
    pub(crate) region: String,
    pub(crate) domain: String,
    pub(crate) backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) observed_adapter_name: Option<String>,
    pub(crate) query_trace_solver_mode: String,
    #[serde(default)]
    pub(crate) selected_workgroup_size: u32,
    pub(crate) frames_executed: u32,
    pub(crate) frame_time_ns: u128,
    #[serde(default)]
    pub(crate) steady_state_fps: f64,
    pub(crate) field_samples: u32,
    pub(crate) quality_tier: String,
    pub(crate) target_fps: u32,
    pub(crate) internal_resolution_scale: f32,
    pub(crate) reconstructed_output: bool,
    pub(crate) quality_history: Vec<String>,
    pub(crate) internal_resolution_history: Vec<f32>,
    pub(crate) bottleneck_pass: Option<String>,
    pub(crate) active_acceleration_artifacts: Vec<String>,
    pub(crate) performance_gain_sources: Vec<String>,
    pub(crate) frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) frame_cost_history: Vec<wrela::presentation_exec::PresentationFrameCostReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) wgsl_workgroup_comparison: Option<PresentationWgslWorkgroupComparison>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ab_comparison: Option<PresentationBenchmarkComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PresentationBenchmarkComparison {
    pub(crate) dense_only_query_trace_solver_mode: String,
    pub(crate) dense_only_frame_time_ns: u128,
    pub(crate) frame_time_ns_delta_vs_dense_only: i128,
    pub(crate) frame_time_ns_delta_vs_dense_only_pct: f64,
    pub(crate) dense_only_average_trace_steps: f32,
    pub(crate) average_trace_steps_delta_vs_dense_only: f32,
    pub(crate) dense_only_field_samples: u32,
    pub(crate) field_samples_delta_vs_dense_only: i64,
    pub(crate) dense_only_candidate_count_before_pruning: u32,
    pub(crate) candidate_count_before_pruning_delta_vs_dense_only: i64,
    pub(crate) dense_only_candidate_count_after_pruning: u32,
    pub(crate) candidate_count_after_pruning_delta_vs_dense_only: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CollisionBenchmarkReport {
    pub(crate) suite: String,
    pub(crate) backend: String,
    pub(crate) command: String,
    pub(crate) query_count_total: u64,
    pub(crate) total_runtime_ns: u128,
    pub(crate) queries_per_sec: f64,
    pub(crate) average_candidate_count: f64,
    pub(crate) average_rejected_candidate_count: f64,
    pub(crate) average_pruned_node_count: f64,
    pub(crate) average_interval_subdivisions: f64,
    pub(crate) average_interval_refinements: f64,
    pub(crate) average_certificate_successes: f64,
    pub(crate) witness_reuse_rate: f64,
    pub(crate) fallback_rate: f64,
    pub(crate) available_count_total: u64,
    pub(crate) consumed_count_total: u64,
    pub(crate) rejected_count_total: u64,
    pub(crate) unavailable_count_total: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) executions: Vec<CollisionBenchmarkExecutionReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CollisionBenchmarkExecutionReport {
    pub(crate) name: String,
    pub(crate) plan_name: String,
    pub(crate) contract_id: String,
    #[serde(default)]
    pub(crate) query_count: u64,
    pub(crate) runtime_ns: u128,
    pub(crate) queries_per_sec: f64,
    pub(crate) broadphase_candidate_count: u32,
    pub(crate) broadphase_rejected_candidate_count: u32,
    pub(crate) broadphase_pruned_node_count: u32,
    #[serde(default)]
    pub(crate) candidate_reduction_effectiveness: f32,
    pub(crate) interval_subdivisions: u32,
    pub(crate) interval_refinements: u32,
    pub(crate) certificate_successes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) interval_bracket: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) fallback_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contact_normal_provenance: Option<String>,
    #[serde(default)]
    pub(crate) wgsl_dispatch_count: u32,
    #[serde(default)]
    pub(crate) wgsl_dispatch_items: u32,
    #[serde(default)]
    pub(crate) wgsl_selected_workgroup_size: u32,
    #[serde(default)]
    pub(crate) wgsl_resident_shared_snapshot_artifacts: u32,
    #[serde(default)]
    pub(crate) cpu_certification_query_count: u32,
    pub(crate) available_count: u32,
    pub(crate) consumed_count: u32,
    pub(crate) rejected_count: u32,
    pub(crate) unavailable_count: u32,
    pub(crate) witness_reuse_rate: f64,
    pub(crate) fallback_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct WholeFrameBenchmarkReport {
    pub(crate) scenario_id: String,
    pub(crate) test_name: String,
    pub(crate) presentation_frame_time_ns: u128,
    pub(crate) collision_runtime_ns: u128,
    pub(crate) total_runtime_ns: u128,
    pub(crate) steady_state_fps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) presentation_bottleneck_pass: Option<String>,
    pub(crate) collision_fallback_rate: f64,
    pub(crate) collision_witness_reuse_rate: f64,
}

impl BenchmarkManifest {
    pub(crate) fn scenarios_for_profile(&self, profile: PerfProfile) -> Vec<&BenchmarkScenario> {
        let coverage = self
            .profiles
            .config_for(profile)
            .map(|cfg| cfg.coverage.to_ascii_lowercase())
            .unwrap_or_else(|| "all".to_string());
        if coverage == "critical" {
            self.scenarios
                .iter()
                .filter(|scenario| scenario.class.eq_ignore_ascii_case("critical"))
                .collect()
        } else {
            self.scenarios.iter().collect()
        }
    }
}

impl BenchmarkProfiles {
    pub(crate) fn config_for(&self, profile: PerfProfile) -> Option<&BenchmarkProfileConfig> {
        match profile {
            PerfProfile::Smoke => self.smoke.as_ref(),
            PerfProfile::Standard => self.standard.as_ref(),
            PerfProfile::Deep => self.deep.as_ref(),
            PerfProfile::Closure1080p120 => self.closure_1080p120.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PerfCmpConfig {
    pub(crate) baseline_ref: String,
    pub(crate) candidate_ref: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) benchmark_root: PathBuf,
    pub(crate) profile: PerfProfile,
    pub(crate) warmup_pairs_override: Option<usize>,
    pub(crate) measure_pairs_override: Option<usize>,
    pub(crate) min_effect_pct: f64,
    pub(crate) confidence_pct: f64,
    pub(crate) output_json: PathBuf,
    pub(crate) output_format: OutputFormat,
    pub(crate) test_timeout_ms: Option<u64>,
    pub(crate) perf_debug: bool,
}

pub(crate) struct EvalCommandInput {
    pub(crate) trace: bool,
    pub(crate) path_arg: Option<String>,
    pub(crate) program_args: Vec<String>,
    pub(crate) runs: Option<usize>,
    pub(crate) output_format: OutputFormat,
}

pub(crate) const EVAL_ONE_SHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EvalOneShotCommandKind {
    Check,
    Test,
}

impl EvalOneShotCommandKind {
    pub(crate) fn as_cli_command(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Test => "test",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct EvalOneShotManifest {
    pub(crate) schema_version: u32,
    pub(crate) suite_id: String,
    pub(crate) cases: Vec<EvalOneShotCase>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct EvalOneShotCase {
    pub(crate) id: String,
    pub(crate) workspace_dir: String,
    pub(crate) command: EvalOneShotCommandKind,
    pub(crate) target: String,
    #[serde(default = "default_eval_one_shot_max_loops")]
    pub(crate) max_loops: u32,
    pub(crate) attempts: Vec<EvalOneShotAttempt>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct EvalOneShotAttempt {
    pub(crate) id: String,
    #[serde(default = "default_eval_one_shot_visible_to_agent")]
    pub(crate) visible_to_agent: bool,
    #[serde(default)]
    pub(crate) machine_applicable: bool,
    #[serde(default)]
    pub(crate) writes: Vec<EvalOneShotWrite>,
    #[serde(default)]
    pub(crate) deletes: Vec<String>,
    #[serde(default)]
    pub(crate) noop: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct EvalOneShotWrite {
    pub(crate) path: String,
    pub(crate) content: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct EvalOneShotCaseReport {
    pub(crate) passed: bool,
    pub(crate) id: String,
    pub(crate) loops_to_green: Option<u32>,
    pub(crate) parse_survived: bool,
    pub(crate) machine_applicable_fix_applied: bool,
    pub(crate) retries: u32,
    pub(crate) hidden_retries: u32,
    pub(crate) execution_ms_total: u128,
}

#[derive(Serialize)]
pub(crate) struct EvalOneShotCaseReportStable {
    pub(crate) id: String,
    pub(crate) passed: bool,
    pub(crate) loops_to_green: Option<u32>,
    pub(crate) parse_survived: bool,
    pub(crate) machine_applicable_fix_applied: bool,
    pub(crate) retries: u32,
    pub(crate) hidden_retries: u32,
}

#[derive(Serialize)]
pub(crate) struct EvalOneShotReport {
    pub(crate) schema_version: u32,
    pub(crate) suite_id: String,
    pub(crate) command: String,
    pub(crate) runs: usize,
    pub(crate) sample_size: usize,
    pub(crate) pass_rate: f64,
    pub(crate) median_loops_to_green: f64,
    pub(crate) parse_survival_rate: f64,
    pub(crate) machine_applicable_fix_apply_rate: f64,
    pub(crate) retries_observed: u32,
    pub(crate) hidden_retry_failures: usize,
    pub(crate) cases: Vec<EvalOneShotCaseReport>,
    pub(crate) corpus_hash: String,
    pub(crate) report_hash: String,
}

pub(crate) fn default_eval_one_shot_max_loops() -> u32 {
    3
}

pub(crate) fn default_eval_one_shot_visible_to_agent() -> bool {
    true
}

pub(crate) fn execute_eval_command(input: EvalCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command eval");
    }
    let Some(eval_kind) = input.path_arg.as_deref() else {
        eprintln!("error: missing eval kind (expected `one-shot`)");
        return EXIT_USAGE;
    };
    if eval_kind != "one-shot" {
        eprintln!("error: unsupported eval kind `{eval_kind}` (expected `one-shot`)");
        return EXIT_USAGE;
    }
    if input.program_args.is_empty() {
        eprintln!("error: missing one-shot corpus path");
        return EXIT_USAGE;
    }
    if input.program_args.len() > 1 {
        eprintln!("error: unexpected extra arguments after one-shot corpus path");
        return EXIT_USAGE;
    }
    let runs = input.runs.unwrap_or(1).max(1);
    let corpus_path = PathBuf::from(&input.program_args[0]);
    let (manifest, corpus_hash) = match load_eval_one_shot_manifest(&corpus_path) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("eval error: {err}");
            return EXIT_USAGE;
        }
    };
    let cases = match run_eval_one_shot_cases(&corpus_path, &manifest) {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!("eval error: {err}");
            return EXIT_CODEGEN;
        }
    };
    let report = summarize_eval_one_shot(&manifest, &cases, runs, corpus_hash);
    match input.output_format {
        OutputFormat::Pretty => {
            println!("eval one-shot: {}", corpus_path.display());
            println!("runs: {}", report.runs);
            println!("schema_version: {}", report.schema_version);
            println!("suite_id: {}", report.suite_id);
            println!("sample_size: {}", report.sample_size);
            println!("pass_rate: {:.4}", report.pass_rate);
            println!("median_loops_to_green: {:.2}", report.median_loops_to_green);
            println!("parse_survival_rate: {:.4}", report.parse_survival_rate);
            println!(
                "machine_applicable_fix_apply_rate: {:.4}",
                report.machine_applicable_fix_apply_rate
            );
            println!("retries_observed: {}", report.retries_observed);
            println!("hidden_retry_failures: {}", report.hidden_retry_failures);
            println!("corpus_hash: {}", report.corpus_hash);
            println!("report_hash: {}", report.report_hash);
            for case in &report.cases {
                println!(
                    "case={} passed={} loops_to_green={} parse_survived={} machine_applicable_fix_applied={} retries={} hidden_retries={} execution_ms_total={}",
                    case.id,
                    case.passed,
                    case.loops_to_green
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "null".to_string()),
                    case.parse_survived,
                    case.machine_applicable_fix_applied,
                    case.retries,
                    case.hidden_retries,
                    case.execution_ms_total
                );
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Sarif => {
            let sarif = serde_json::json!({
                "version": "2.1.0",
                "runs": [{
                    "tool": {
                        "driver": {
                            "name": "wrela-eval",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    },
                    "results": [{
                        "level": "note",
                        "message": {
                            "text": format!(
                                "one-shot suite={} pass_rate={:.4} sample_size={} report_hash={}",
                                report.suite_id, report.pass_rate, report.sample_size, report.report_hash
                            )
                        }
                    }]
                }]
            });
            println!(
                "{}",
                serde_json::to_string(&sarif).unwrap_or_else(|_| "{}".to_string())
            );
        }
    }
    EXIT_OK
}

pub(crate) fn load_eval_one_shot_manifest(
    path: &Path,
) -> Result<(EvalOneShotManifest, String), String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read one-shot corpus {}: {err}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| format!("invalid one-shot corpus json: {err}"))?;
    if value.is_array() {
        return Err(
            "unsupported one-shot corpus schema v1 (array fixture); expected v2 object with `schema_version: 2` and `cases`"
                .to_string(),
        );
    }
    let manifest: EvalOneShotManifest = serde_json::from_value(value)
        .map_err(|err| format!("invalid one-shot corpus v2 manifest: {err}"))?;
    validate_eval_one_shot_manifest(&manifest)?;
    let corpus_bytes = serde_json::to_vec(&manifest)
        .map_err(|err| format!("failed to canonicalize one-shot corpus v2 manifest: {err}"))?;
    Ok((manifest, fnv1a64_hex(&corpus_bytes)))
}

pub(crate) fn validate_eval_one_shot_manifest(
    manifest: &EvalOneShotManifest,
) -> Result<(), String> {
    if manifest.schema_version != EVAL_ONE_SHOT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported one-shot schema_version {}; expected {}",
            manifest.schema_version, EVAL_ONE_SHOT_SCHEMA_VERSION
        ));
    }
    if manifest.suite_id.trim().is_empty() {
        return Err("one-shot suite_id must be non-empty".to_string());
    }
    if manifest.cases.is_empty() {
        return Err("one-shot corpus v2 must contain at least one case".to_string());
    }
    let mut case_ids = HashSet::new();
    for case in &manifest.cases {
        if case.id.trim().is_empty() {
            return Err("one-shot case id must be non-empty".to_string());
        }
        if !case_ids.insert(case.id.clone()) {
            return Err(format!("duplicate one-shot case id `{}`", case.id));
        }
        if !eval_one_shot_relative_path_is_safe(&case.workspace_dir, true) {
            return Err(format!(
                "one-shot case `{}` has unsafe workspace_dir `{}`",
                case.id, case.workspace_dir
            ));
        }
        if !eval_one_shot_relative_path_is_safe(&case.target, true) {
            return Err(format!(
                "one-shot case `{}` has unsafe target `{}`",
                case.id, case.target
            ));
        }
        if case.max_loops == 0 {
            return Err(format!(
                "one-shot case `{}` must declare max_loops >= 1",
                case.id
            ));
        }
        if case.attempts.is_empty() {
            return Err(format!(
                "one-shot case `{}` must declare at least one attempt",
                case.id
            ));
        }
        let mut attempt_ids = HashSet::new();
        for attempt in &case.attempts {
            if attempt.id.trim().is_empty() {
                return Err(format!(
                    "one-shot case `{}` has attempt with empty id",
                    case.id
                ));
            }
            if !attempt_ids.insert(attempt.id.clone()) {
                return Err(format!(
                    "one-shot case `{}` has duplicate attempt id `{}`",
                    case.id, attempt.id
                ));
            }
            if !attempt.noop && attempt.writes.is_empty() && attempt.deletes.is_empty() {
                return Err(format!(
                    "one-shot case `{}` attempt `{}` must define writes/deletes or set noop=true",
                    case.id, attempt.id
                ));
            }
            for write in &attempt.writes {
                if !eval_one_shot_relative_path_is_safe(&write.path, false) {
                    return Err(format!(
                        "one-shot case `{}` attempt `{}` has unsafe write path `{}`",
                        case.id, attempt.id, write.path
                    ));
                }
            }
            for delete in &attempt.deletes {
                if !eval_one_shot_relative_path_is_safe(delete, false) {
                    return Err(format!(
                        "one-shot case `{}` attempt `{}` has unsafe delete path `{}`",
                        case.id, attempt.id, delete
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn eval_one_shot_relative_path_is_safe(path: &str, allow_dot: bool) -> bool {
    if path.trim().is_empty() {
        return false;
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return false;
    }
    let mut saw_normal = false;
    for component in candidate.components() {
        match component {
            std::path::Component::Normal(_) => saw_normal = true,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
        }
    }
    allow_dot || saw_normal
}

pub(crate) fn run_eval_one_shot_cases(
    manifest_path: &Path,
    manifest: &EvalOneShotManifest,
) -> Result<Vec<EvalOneShotCaseReport>, String> {
    let manifest_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let eval_root = std::env::current_dir()
        .map_err(|err| format!("failed to resolve cwd for eval staging: {err}"))?
        .join("target")
        .join("wrela_eval");
    fs::create_dir_all(&eval_root).map_err(|err| {
        format!(
            "failed to create eval staging root {}: {err}",
            eval_root.display()
        )
    })?;
    let run_root = eval_root.join(format!(
        "{}_{}_{}",
        sanitize_test_path_component(&manifest.suite_id),
        std::process::id(),
        now_unix_ms()
    ));
    fs::create_dir_all(&run_root).map_err(|err| {
        format!(
            "failed to create eval run root {}: {err}",
            run_root.display()
        )
    })?;
    let cli_path = env::current_exe()
        .map_err(|err| format!("failed to resolve current wrela executable path: {err}"))?;
    let mut reports = Vec::with_capacity(manifest.cases.len());
    for case in &manifest.cases {
        let workspace_source = manifest_dir.join(&case.workspace_dir);
        if !workspace_source.is_dir() {
            return Err(format!(
                "one-shot case `{}` workspace_dir does not exist: {}",
                case.id,
                workspace_source.display()
            ));
        }
        let staged_case_root = run_root.join(sanitize_test_path_component(&case.id));
        if staged_case_root.exists() {
            fs::remove_dir_all(&staged_case_root).map_err(|err| {
                format!(
                    "failed to clear prior eval staging for case `{}` at {}: {err}",
                    case.id,
                    staged_case_root.display()
                )
            })?;
        }
        copy_dir_recursive_sorted(&workspace_source, &staged_case_root)?;
        let max_attempts = std::cmp::min(case.max_loops as usize, case.attempts.len());
        let mut passed = false;
        let mut loops_to_green = None;
        let mut parse_survived = true;
        let mut machine_applicable_fix_applied = false;
        let mut execution_ms_total = 0u128;
        let mut visible_history = Vec::new();
        let mut executed_attempts = 0usize;
        for (idx, attempt) in case.attempts.iter().take(max_attempts).enumerate() {
            apply_eval_one_shot_attempt(case, attempt, &staged_case_root)?;
            visible_history.push(attempt.visible_to_agent);
            executed_attempts += 1;
            let (output, elapsed_ms) =
                run_eval_one_shot_attempt(&cli_path, &staged_case_root, case.command, &case.target)
                    .map_err(|err| {
                        format!(
                            "one-shot case `{}` attempt `{}` execution failed: {err}",
                            case.id, attempt.id
                        )
                    })?;
            execution_ms_total += elapsed_ms;
            if eval_one_shot_attempt_has_parse_failure(&output) {
                parse_survived = false;
            }
            if output.status.success() {
                passed = true;
                loops_to_green = Some((idx + 1) as u32);
                machine_applicable_fix_applied = attempt.machine_applicable;
                break;
            }
        }
        let retries = executed_attempts.saturating_sub(1) as u32;
        let hidden_retries = visible_history
            .iter()
            .take(visible_history.len().saturating_sub(1))
            .filter(|visible| !**visible)
            .count() as u32;
        reports.push(EvalOneShotCaseReport {
            id: case.id.clone(),
            passed,
            loops_to_green,
            parse_survived,
            machine_applicable_fix_applied,
            retries,
            hidden_retries,
            execution_ms_total,
        });
    }
    Ok(reports)
}

pub(crate) fn copy_dir_recursive_sorted(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("source directory missing: {}", src.display()));
    }
    fs::create_dir_all(dst).map_err(|err| {
        format!(
            "failed to create destination directory {}: {err}",
            dst.display()
        )
    })?;
    let entries =
        fs::read_dir(src).map_err(|err| format!("failed to read {}: {err}", src.display()))?;
    let mut children = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", src.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    children.sort_by_key(|path| path_sort_key(path));
    for child in children {
        let Some(name) = child.file_name() else {
            continue;
        };
        let target = dst.join(name);
        if child.is_dir() {
            copy_dir_recursive_sorted(&child, &target)?;
        } else if child.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!(
                        "failed to create destination parent {}: {err}",
                        parent.display()
                    )
                })?;
            }
            fs::copy(&child, &target).map_err(|err| {
                format!(
                    "failed to copy {} to {}: {err}",
                    child.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(crate) fn apply_eval_one_shot_attempt(
    case: &EvalOneShotCase,
    attempt: &EvalOneShotAttempt,
    workspace_root: &Path,
) -> Result<(), String> {
    for path in &attempt.deletes {
        let absolute = workspace_root.join(path);
        if absolute.is_dir() {
            fs::remove_dir_all(&absolute).map_err(|err| {
                format!(
                    "one-shot case `{}` attempt `{}` failed to delete directory {}: {err}",
                    case.id,
                    attempt.id,
                    absolute.display()
                )
            })?;
        } else if absolute.is_file() {
            fs::remove_file(&absolute).map_err(|err| {
                format!(
                    "one-shot case `{}` attempt `{}` failed to delete file {}: {err}",
                    case.id,
                    attempt.id,
                    absolute.display()
                )
            })?;
        }
    }
    for write in &attempt.writes {
        let absolute = workspace_root.join(&write.path);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "one-shot case `{}` attempt `{}` failed to create {}: {err}",
                    case.id,
                    attempt.id,
                    parent.display()
                )
            })?;
        }
        fs::write(&absolute, &write.content).map_err(|err| {
            format!(
                "one-shot case `{}` attempt `{}` failed to write {}: {err}",
                case.id,
                attempt.id,
                absolute.display()
            )
        })?;
    }
    Ok(())
}

pub(crate) fn run_eval_one_shot_attempt(
    cli_path: &Path,
    workspace_root: &Path,
    command: EvalOneShotCommandKind,
    target: &str,
) -> Result<(Output, u128), String> {
    let mut cmd = Command::new(cli_path);
    cmd.current_dir(workspace_root)
        .arg(command.as_cli_command())
        .arg("--error-format=json")
        .arg(target);
    let started = Instant::now();
    let output = cmd.output().map_err(|err| {
        format!(
            "failed to execute `{}` in {}: {err}",
            command.as_cli_command(),
            workspace_root.display()
        )
    })?;
    Ok((output, started.elapsed().as_millis()))
}

pub(crate) fn eval_one_shot_attempt_has_parse_failure(output: &Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
            && value.get("stage").and_then(|stage| stage.as_str()) == Some("parse")
            && value
                .get("severity")
                .and_then(|severity| severity.as_str())
                .map(|severity| severity == "error")
                .unwrap_or(true)
        {
            return true;
        }
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    stderr.contains("parse-invalid")
        || stderr.contains("parse error")
        || stderr.contains("lexical error")
}

pub(crate) fn summarize_eval_one_shot(
    manifest: &EvalOneShotManifest,
    case_reports: &[EvalOneShotCaseReport],
    runs: usize,
    corpus_hash: String,
) -> EvalOneShotReport {
    let sample_size = case_reports.len();
    let pass_count = case_reports
        .iter()
        .filter(|case| case.passed && case.hidden_retries == 0)
        .count();
    let parse_survived_count = case_reports
        .iter()
        .filter(|case| case.parse_survived)
        .count();
    let machine_fix_count = case_reports
        .iter()
        .filter(|case| case.machine_applicable_fix_applied)
        .count();
    let retries_observed = case_reports.iter().map(|case| case.retries).sum::<u32>();
    let hidden_retry_failures = case_reports
        .iter()
        .filter(|case| case.hidden_retries > 0)
        .count();
    let mut loops = case_reports
        .iter()
        .filter_map(|case| case.loops_to_green)
        .collect::<Vec<_>>();
    loops.sort_unstable();
    let median_loops_to_green = if loops.is_empty() {
        0.0
    } else if loops.len() % 2 == 1 {
        loops[loops.len() / 2] as f64
    } else {
        let hi = loops.len() / 2;
        let lo = hi - 1;
        (loops[lo] as f64 + loops[hi] as f64) / 2.0
    };
    let stable_cases = case_reports
        .iter()
        .map(|case| EvalOneShotCaseReportStable {
            id: case.id.clone(),
            passed: case.passed,
            loops_to_green: case.loops_to_green,
            parse_survived: case.parse_survived,
            machine_applicable_fix_applied: case.machine_applicable_fix_applied,
            retries: case.retries,
            hidden_retries: case.hidden_retries,
        })
        .collect::<Vec<_>>();
    let report_without_hash = serde_json::json!({
        "schema_version": EVAL_ONE_SHOT_SCHEMA_VERSION,
        "suite_id": manifest.suite_id,
        "command": "one-shot",
        "runs": runs,
        "sample_size": sample_size,
        "pass_rate": if sample_size == 0 { 0.0 } else { pass_count as f64 / sample_size as f64 },
        "median_loops_to_green": median_loops_to_green,
        "parse_survival_rate": if sample_size == 0 { 0.0 } else { parse_survived_count as f64 / sample_size as f64 },
        "machine_applicable_fix_apply_rate": if sample_size == 0 { 0.0 } else { machine_fix_count as f64 / sample_size as f64 },
        "retries_observed": retries_observed,
        "hidden_retry_failures": hidden_retry_failures,
        "cases": stable_cases,
        "corpus_hash": corpus_hash
    });
    let report_hash = fnv1a64_hex(&serde_json::to_vec(&report_without_hash).unwrap_or_default());
    EvalOneShotReport {
        schema_version: EVAL_ONE_SHOT_SCHEMA_VERSION,
        suite_id: manifest.suite_id.clone(),
        command: "one-shot".to_string(),
        runs,
        sample_size,
        pass_rate: if sample_size == 0 {
            0.0
        } else {
            pass_count as f64 / sample_size as f64
        },
        median_loops_to_green,
        parse_survival_rate: if sample_size == 0 {
            0.0
        } else {
            parse_survived_count as f64 / sample_size as f64
        },
        machine_applicable_fix_apply_rate: if sample_size == 0 {
            0.0
        } else {
            machine_fix_count as f64 / sample_size as f64
        },
        retries_observed,
        hidden_retry_failures,
        cases: case_reports.to_vec(),
        corpus_hash,
        report_hash,
    }
}

pub(crate) fn run_tests_once(
    target: &TestTarget,
    budget_policy: &BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_lane: bool,
    selection: &TestSelection,
    emit_json_summary: bool,
    emit_pretty_output: bool,
    http_mode: HttpCassetteMode,
    sim_seed_override: Option<u64>,
    query_backend: wrela::query_plan::DispatchBackend,
    certify_mode: bool,
    pipeline: DifferentialPipeline,
    mut run_timing_out: Option<&mut RunOnceTimings>,
    harness_cache: Option<&mut HashMap<String, TestHarness>>,
) -> (i32, Option<PerfSummary>, Option<DeterminismSignature>) {
    configure_runtime_for_test_lane(perf_lane, perf_debug);
    let total_start = Instant::now();
    let discovery_start = Instant::now();
    let mut tests = Vec::new();
    let (workspace_root, compile_root, tests_root, missing_path_msg) = match target {
        TestTarget::ProjectRoot(root) => {
            let workspace_root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            let src_root = workspace_root.join("src");
            let tests_root = workspace_root.join("tests");
            let tests_root_opt = if tests_root.is_dir() {
                if let Err(err) = collect_tests(&tests_root, &tests_root, &mut tests) {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
                Some(tests_root.clone())
            } else {
                None
            };
            match collect_autogen_spec_tests(
                &workspace_root,
                budget_policy.autogen_max_cases.value,
                budget_policy.autogen_time_cap_ms.value,
            ) {
                Ok(mut generated) => tests.append(&mut generated),
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            }
            if certify_mode {
                match collect_fuzz_tests(
                    &workspace_root,
                    budget_policy.fuzz_max_cases.value,
                    budget_policy.fuzz_time_cap_ms.value,
                ) {
                    Ok(mut generated) => tests.append(&mut generated),
                    Err(err) => {
                        eprintln!("test discovery error: {err}");
                        return (EXIT_USAGE, None, None);
                    }
                }
            }
            let missing_path_msg = tests_root_opt
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| workspace_root.display().to_string());
            (workspace_root, src_root, tests_root_opt, missing_path_msg)
        }
        TestTarget::SingleFile(path) => {
            let Some(parent) = path.parent() else {
                eprintln!("test discovery error: file has no parent directory");
                return (EXIT_USAGE, None, None);
            };
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            };
            let module_path = match module_path_for_single_file(path) {
                Ok(module_path) => module_path,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            };
            if let Err(err) =
                collect_tests_from_source(&source, &module_path, path, false, &mut tests)
            {
                eprintln!("test discovery error: {err}");
                return (EXIT_USAGE, None, None);
            }
            let workspace_root = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            (
                workspace_root.clone(),
                workspace_root,
                None,
                path.display().to_string(),
            )
        }
    };
    let discovery_ms = discovery_start.elapsed().as_millis();

    let selection_start = Instant::now();
    tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let selected_tests = select_tests(tests, selection);
    let canonical_authored_selected: Vec<TestCase> = selected_tests
        .iter()
        .filter(|test| test.generated_case_kind.is_none() && test.sim_seed.is_none())
        .cloned()
        .collect();
    if certify_mode
        && !selection.list
        && let Err(err) = enforce_serial_test_cap(&canonical_authored_selected)
    {
        eprintln!("serial gate failed: {err}");
        return (EXIT_CODEGEN, None, None);
    }
    let tests = if selection.list {
        selected_tests
    } else {
        expand_sim_seed_cases(selected_tests, sim_seed_override, certify_mode)
    };
    let selection_ms = selection_start.elapsed().as_millis();
    if let Some(timing) = run_timing_out.as_deref_mut() {
        timing.collect_tests_ms = discovery_ms + selection_ms;
    }
    if (emit_pretty_output || emit_json_summary)
        && let Some(report) = selection.cert_selection_report.as_ref()
    {
        emit_cert_selection_report(output_format, report, tests.len());
    }

    if tests.is_empty() {
        if selection.id.is_some() || selection.filter.is_some() {
            eprintln!("no tests matched selection at {}", missing_path_msg);
        } else {
            eprintln!("no tests found at {}", missing_path_msg);
        }
        return (EXIT_OK, None, None);
    }

    if selection.list {
        match output_format {
            OutputFormat::Pretty => list_tests(&tests),
            OutputFormat::Json => {
                let summary = TestJsonSummary {
                    run: TestJsonRunMetadata {
                        seed: TEST_JSON_SUMMARY_SEED,
                        lane: summarize_run_lane(&tests),
                        jobs,
                        harness_cache_hit: false,
                        budgets_used: budget_policy.clone(),
                    },
                    tests: tests
                        .iter()
                        .map(|test| TestJsonCase {
                            id: test.id.clone(),
                            name: test.name.clone(),
                            lane: test.lane.as_str().to_string(),
                            status: "listed".to_string(),
                            duration_ms: 0,
                            error: None,
                        })
                        .collect(),
                    timings: TestJsonTimings {
                        discovery_ms,
                        selection_ms,
                        compile_harness_ms: 0,
                        execution_ms: 0,
                        total_ms: total_start.elapsed().as_millis(),
                    },
                };
                emit_test_json_summary(&summary);
            }
            OutputFormat::Sarif => list_tests(&tests),
        }
        return (EXIT_OK, None, None);
    }

    let missing_oracles: Vec<&TestCase> = tests.iter().filter(|test| !test.has_oracle).collect();
    if !missing_oracles.is_empty() {
        eprintln!(
            "oracle gate failed: test functions must contain at least one `assert` or `require`"
        );
        for test in missing_oracles {
            eprintln!("  - {}: no assertion oracle found", test.name);
        }
        return (EXIT_CODEGEN, None, None);
    }

    let harness = match compile_test_harness(
        &workspace_root,
        &compile_root,
        tests_root.as_deref(),
        &tests,
        output_format,
        query_backend,
        harness_cache,
    ) {
        Ok(harness) => harness,
        Err(err) => {
            eprintln!("test harness error: {err}");
            return (EXIT_CODEGEN, None, None);
        }
    };
    let compile_harness_ms = harness.compile_ns / 1_000_000;
    let harness_cache_hit = harness.cache_hit;
    if let Some(timing) = run_timing_out {
        timing.compile_harness_ms = compile_harness_ms;
    }

    let total_tests = tests.len();
    let base_compile_ns = harness.compile_ns / total_tests as u128;
    let compile_ns_remainder = harness.compile_ns % total_tests as u128;

    let execution_start = Instant::now();
    let (serial_tests, parallel_tests): (Vec<TestCase>, Vec<TestCase>) =
        tests.into_iter().partition(|test| test.is_serial);
    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        parallel_tests,
    )));
    let (tx, rx) =
        std::sync::mpsc::channel::<(TestCase, bool, Duration, String, Option<TestRun>)>();
    let mut handles = Vec::new();
    let worker_count = jobs.max(1);
    for _ in 0..worker_count {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let harness_exe_path = harness.exe_path.clone();
        let workspace_root = workspace_root.clone();
        handles.push(std::thread::spawn(move || {
            loop {
                let next = {
                    let mut guard = queue.lock().expect("test queue");
                    guard.pop_front()
                };
                let Some(test) = next else { break };
                let start = Instant::now();
                let (ok, err, run) = execute_test_case(
                    &harness_exe_path,
                    &workspace_root,
                    &test,
                    timeout,
                    output_format,
                    http_mode,
                    pipeline,
                    certify_mode,
                    query_backend,
                );
                let _ = tx.send((test, ok, start.elapsed(), err, run));
            }
        }));
    }
    drop(tx);

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut compile_ns: Vec<u128> = Vec::new();
    let mut runtime_ns: Vec<u128> = Vec::new();
    let mut cases: Vec<PerfCaseSample> = Vec::new();
    let mut metrics_totals = MetricsTotals::default();
    let mut metrics_count = 0usize;
    let mut json_cases = Vec::new();
    let mut completed = 0usize;
    for (test, ok, dur, err, run) in rx.iter() {
        let compile_slice_ns = if completed < compile_ns_remainder as usize {
            base_compile_ns + 1
        } else {
            base_compile_ns
        };
        completed += 1;
        compile_ns.push(compile_slice_ns);

        if let Some(run) = run.as_ref() {
            runtime_ns.push(run.runtime_ns);
            if let Some(metrics) = run.metrics.as_ref() {
                metrics_totals.add(metrics);
                metrics_count += 1;
            }
            cases.push(PerfCaseSample {
                id: test.id.clone(),
                name: test.name.clone(),
                compile_ns: compile_slice_ns,
                runtime_ns: run.runtime_ns,
                metrics: run.metrics.clone(),
            });
        }
        json_cases.push(TestJsonCase {
            id: test.id,
            name: test.name.clone(),
            lane: test.lane.as_str().to_string(),
            status: if ok {
                "ok".to_string()
            } else {
                "fail".to_string()
            },
            duration_ms: dur.as_millis(),
            error: if ok { None } else { Some(err.clone()) },
        });
        if ok {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("ok   {:>7?}  {}", dur, test.name);
            }
            ok_count += 1;
        } else {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("fail {:>7?}  {}  {}", dur, test.name, err);
            }
            fail_count += 1;
        }
    }
    for test in serial_tests {
        let start = Instant::now();
        let (ok, err, run) = execute_test_case(
            &harness.exe_path,
            &workspace_root,
            &test,
            timeout,
            output_format,
            http_mode,
            pipeline,
            certify_mode,
            query_backend,
        );
        let dur = start.elapsed();
        let compile_slice_ns = if completed < compile_ns_remainder as usize {
            base_compile_ns + 1
        } else {
            base_compile_ns
        };
        completed += 1;
        compile_ns.push(compile_slice_ns);
        if let Some(run) = run.as_ref() {
            runtime_ns.push(run.runtime_ns);
            if let Some(metrics) = run.metrics.as_ref() {
                metrics_totals.add(metrics);
                metrics_count += 1;
            }
            cases.push(PerfCaseSample {
                id: test.id.clone(),
                name: test.name.clone(),
                compile_ns: compile_slice_ns,
                runtime_ns: run.runtime_ns,
                metrics: run.metrics.clone(),
            });
        }
        json_cases.push(TestJsonCase {
            id: test.id,
            name: test.name.clone(),
            lane: test.lane.as_str().to_string(),
            status: if ok {
                "ok".to_string()
            } else {
                "fail".to_string()
            },
            duration_ms: dur.as_millis(),
            error: if ok { None } else { Some(err.clone()) },
        });
        if ok {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("ok   {:>7?}  {}", dur, test.name);
            }
            ok_count += 1;
        } else {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("fail {:>7?}  {}  {}", dur, test.name, err);
            }
            fail_count += 1;
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    json_cases.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.lane.cmp(&b.lane))
    });
    let execution_ms = execution_start.elapsed().as_millis();
    let total_ms = total_start.elapsed().as_millis();
    let summary_lane = summarize_run_lane_from_json_cases(&json_cases);
    if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
        println!("tests: {} passed, {} failed", ok_count, fail_count);
    }
    let signature = build_determinism_signature(&json_cases);
    if matches!(output_format, OutputFormat::Json) && emit_json_summary {
        let summary = TestJsonSummary {
            run: TestJsonRunMetadata {
                seed: TEST_JSON_SUMMARY_SEED,
                lane: summary_lane,
                jobs,
                harness_cache_hit,
                budgets_used: budget_policy.clone(),
            },
            tests: json_cases,
            timings: TestJsonTimings {
                discovery_ms,
                selection_ms,
                compile_harness_ms,
                execution_ms,
                total_ms,
            },
        };
        emit_test_json_summary(&summary);
    }
    if fail_count != 0 || runtime_ns.is_empty() {
        return (EXIT_CODEGEN, None, Some(signature));
    }
    let mut summary = build_perf_summary(&compile_ns, &runtime_ns, metrics_count, &metrics_totals);
    // Attach per-test samples so perf consumers (macrobench) can compute per-scenario
    // percentiles without changing the core gate logic.
    summary.cases = Some(cases);
    if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
        print_perf_summary(&summary, perf_debug);
    }
    (EXIT_OK, Some(summary), Some(signature))
}

pub(crate) fn build_determinism_signature(cases: &[TestJsonCase]) -> DeterminismSignature {
    let outcomes: Vec<DeterminismOutcome> = cases
        .iter()
        .map(|case| DeterminismOutcome {
            id: case.id.clone(),
            name: case.name.clone(),
            lane: case.lane.clone(),
            status: case.status.clone(),
            error: case.error.clone(),
        })
        .collect();
    let payload = serde_json::to_vec(&(TEST_JSON_SUMMARY_SEED, &outcomes))
        .unwrap_or_else(|_| TEST_JSON_SUMMARY_SEED.to_le_bytes().to_vec());
    let hash = fnv1a64_hex(&payload);
    DeterminismSignature { hash, outcomes }
}

pub(crate) fn first_signature_mismatch_detail(
    first: &[DeterminismOutcome],
    second: &[DeterminismOutcome],
) -> Option<String> {
    if first.len() != second.len() {
        return Some(format!(
            "case count differs: first={} replay={}",
            first.len(),
            second.len()
        ));
    }
    for (lhs, rhs) in first.iter().zip(second.iter()) {
        if lhs != rhs {
            return Some(format!(
                "{} => first(status={}, error={:?}) replay(status={}, error={:?})",
                lhs.name, lhs.status, lhs.error, rhs.status, rhs.error
            ));
        }
    }
    None
}

pub(crate) fn configure_runtime_for_test_lane(perf_lane: bool, _perf_debug: bool) {
    if !perf_lane {
        return;
    }
    if env::var_os("WRELA_RUNTIME_PROFILE").is_none() {
        // Perf lanes should exercise release runtime defaults.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_PROFILE", "release") };
    }
    if env::var_os("WRELA_RUNTIME_METRICS").is_none() {
        // KPI-gated matrix lanes require runtime metrics to be emitted.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_METRICS", "1") };
    }
}

pub(crate) fn build_perf_summary(
    compile_ns: &[u128],
    runtime_ns: &[u128],
    metrics_count: usize,
    metrics_totals: &MetricsTotals,
) -> PerfSummary {
    let compile_total_ns: u128 = compile_ns.iter().copied().sum();
    let compile_throughput_tests_per_sec = if compile_total_ns == 0 {
        0.0
    } else {
        compile_ns.len() as f64 / (compile_total_ns as f64 / 1_000_000_000.0)
    };

    let mut runtime_sorted = runtime_ns.to_vec();
    runtime_sorted.sort_unstable();
    let runtime_p50_ns = percentile(&runtime_sorted, 0.50);
    let runtime_p95_ns = percentile(&runtime_sorted, 0.95);
    let runtime_p99_ns = percentile(&runtime_sorted, 0.99);

    let allocs_per_request = if metrics_count == 0 {
        0.0
    } else {
        metrics_totals.total_allocs() as f64 / metrics_count as f64
    };
    let dispatch_total = metrics_totals.sched_dispatched + metrics_totals.sched_skipped_no_credit;
    let dispatch_hit_ratio = if dispatch_total == 0 {
        1.0
    } else {
        metrics_totals.sched_dispatched as f64 / dispatch_total as f64
    };
    let rc_ops_total = metrics_totals.rc_inc + metrics_totals.rc_dec;
    let runtime_total_ns: u128 = runtime_ns.iter().copied().sum();
    let actor_msgs_per_sec = if runtime_total_ns == 0 || metrics_totals.mailbox_dequeue == 0 {
        None
    } else {
        Some(
            metrics_totals.mailbox_dequeue as f64
                / (runtime_total_ns as f64 / 1_000_000_000.0).max(f64::EPSILON),
        )
    };
    PerfSummary {
        sample_count: runtime_ns.len(),
        compile_throughput_tests_per_sec,
        runtime_p50_ns,
        runtime_p95_ns,
        runtime_p99_ns,
        allocs_per_request,
        rc_inc: metrics_totals.rc_inc,
        rc_dec: metrics_totals.rc_dec,
        rc_ops_total,
        dispatch_hit_ratio,
        check_fallback_rate: None,
        avg_check_batch_size: None,
        check_oracle_eval_ns_p50: None,
        check_oracle_eval_ns_p95: None,
        effect_annihilation_rewrite_count: None,
        scheduler_dispatch_p99_ns: (metrics_totals.sched_dispatch_loop_ns_p99 > 0)
            .then_some(metrics_totals.sched_dispatch_loop_ns_p99),
        scheduler_starvation_violations: Some(metrics_totals.sched_starvation_violation),
        rewrite_compile_overhead_pct: None,
        rewrite_applied_count: None,
        actor_msgs_per_sec_p50: actor_msgs_per_sec,
        actor_msgs_per_sec_p95: actor_msgs_per_sec,
        queue_enqueue_p99_ns: (metrics_totals.queue_enqueue_p99_ns > 0)
            .then_some(metrics_totals.queue_enqueue_p99_ns),
        queue_dequeue_p99_ns: (metrics_totals.queue_dequeue_p99_ns > 0)
            .then_some(metrics_totals.queue_dequeue_p99_ns),
        queue_age_p99_ns: (metrics_totals.queue_age_p99_ns > 0)
            .then_some(metrics_totals.queue_age_p99_ns),
        mailbox_wake_coalesced_count: Some(metrics_totals.mailbox_wake_coalesced_count),
        mailbox_rescue_wake_count: Some(metrics_totals.mailbox_rescue_wake_count),
        queue_cas_retry_total: Some(metrics_totals.queue_cas_retry_total),
        cases: None,
        metrics: metrics_totals.clone(),
    }
}

pub(crate) fn overlay_perf_summary_runtime_cases(
    base: &PerfSummary,
    cases: &[(String, String, u128)],
) -> PerfSummary {
    let mut summary = base.clone();
    let mut runtime_sorted: Vec<u128> =
        cases.iter().map(|(_, _, runtime_ns)| *runtime_ns).collect();
    runtime_sorted.sort_unstable();
    summary.sample_count = runtime_sorted.len();
    if runtime_sorted.is_empty() {
        summary.runtime_p50_ns = 0;
        summary.runtime_p95_ns = 0;
        summary.runtime_p99_ns = 0;
    } else {
        summary.runtime_p50_ns = percentile(&runtime_sorted, 0.50);
        summary.runtime_p95_ns = percentile(&runtime_sorted, 0.95);
        summary.runtime_p99_ns = percentile(&runtime_sorted, 0.99);
    }
    summary.cases = Some(
        cases
            .iter()
            .map(|(id, name, runtime_ns)| PerfCaseSample {
                id: id.clone(),
                name: name.clone(),
                compile_ns: 0,
                runtime_ns: *runtime_ns,
                metrics: None,
            })
            .collect(),
    );
    summary
}

pub(crate) fn emit_perf_summary(summary: &PerfSummary, perf_debug: bool) {
    print_perf_summary(summary, perf_debug);
}

pub(crate) fn print_perf_summary(summary: &PerfSummary, perf_debug: bool) {
    println!(
        "perf: compile_tps={:.2} p50_ns={} p95_ns={} p99_ns={} allocs/request={:.2} rc_ops={} dispatch_hit_ratio={:.4}",
        summary.compile_throughput_tests_per_sec,
        summary.runtime_p50_ns,
        summary.runtime_p95_ns,
        summary.runtime_p99_ns,
        summary.allocs_per_request,
        summary.rc_ops_total,
        summary.dispatch_hit_ratio
    );
    let check_lane = check_lane_kpis_from_summary(summary);
    println!(
        "check-lane: typed_total={} boxed_total={} typed_ratio={:.4}",
        check_lane.typed_lane_total, check_lane.boxed_lane_total, check_lane.typed_lane_ratio
    );
    let scene_trace = scene_trace_kpis_from_summary(summary);
    if scene_trace.trace_total > 0
        || scene_trace.candidate_total > 0
        || scene_trace.pruned_total > 0
        || scene_trace.hit_total > 0
    {
        println!(
            "scene-trace: traces={} field_samples={} candidates={} pruned={} prune_ratio={:.4} exact={} conservative={} hits={} avg_hit_steps={:.2}",
            scene_trace.trace_total,
            scene_trace.field_sample_total,
            scene_trace.candidate_total,
            scene_trace.pruned_total,
            scene_trace.prune_ratio,
            scene_trace.exact_total,
            scene_trace.conservative_total,
            scene_trace.hit_total,
            scene_trace.avg_hit_steps
        );
    }
    if perf_debug {
        println!(
            "perf-debug: rc_inc={} rc_dec={} mailbox_enqueue_ok={} mailbox_enqueue_fail={} mailbox_dequeue={} mailbox_high_water={} alloc_list={} alloc_map={} alloc_string={} alloc_bytes={} alloc_result={} alloc_pending={} messages_sent={} messages_dropped={} pending_resolved={} pending_dropped={} sched_dispatched={} sched_skipped_no_credit={} sched_profile_switch={} sched_starvation_violation={} sched_cross_shard_migration={} abi_typed_lane={} abi_boxed_lane={}",
            summary.metrics.rc_inc,
            summary.metrics.rc_dec,
            summary.metrics.mailbox_enqueue_ok,
            summary.metrics.mailbox_enqueue_fail,
            summary.metrics.mailbox_dequeue,
            summary.metrics.mailbox_high_water,
            summary.metrics.alloc_list,
            summary.metrics.alloc_map,
            summary.metrics.alloc_string,
            summary.metrics.alloc_bytes,
            summary.metrics.alloc_result,
            summary.metrics.alloc_pending,
            summary.metrics.messages_sent,
            summary.metrics.messages_dropped,
            summary.metrics.pending_resolved,
            summary.metrics.pending_dropped,
            summary.metrics.sched_dispatched,
            summary.metrics.sched_skipped_no_credit,
            summary.metrics.sched_profile_switch,
            summary.metrics.sched_starvation_violation,
            summary.metrics.sched_cross_shard_migration,
            summary.metrics.abi_typed_lane,
            summary.metrics.abi_boxed_lane
        );
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct CheckLaneKpis {
    pub(crate) typed_lane_total: u64,
    pub(crate) boxed_lane_total: u64,
    pub(crate) typed_lane_ratio: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct SceneTraceKpis {
    pub(crate) trace_total: u64,
    pub(crate) field_sample_total: u64,
    pub(crate) candidate_total: u64,
    pub(crate) pruned_total: u64,
    pub(crate) prune_ratio: f64,
    pub(crate) exact_total: u64,
    pub(crate) conservative_total: u64,
    pub(crate) hit_total: u64,
    pub(crate) avg_hit_steps: f64,
}

pub(crate) fn check_lane_kpis_from_summary(summary: &PerfSummary) -> CheckLaneKpis {
    let typed = summary.metrics.abi_typed_lane;
    let boxed = summary.metrics.abi_boxed_lane;
    let total = typed + boxed;
    let typed_lane_ratio = if total == 0 {
        1.0
    } else {
        typed as f64 / total as f64
    };
    CheckLaneKpis {
        typed_lane_total: typed,
        boxed_lane_total: boxed,
        typed_lane_ratio,
    }
}

pub(crate) fn scene_trace_kpis_from_summary(summary: &PerfSummary) -> SceneTraceKpis {
    let candidate_total = summary.metrics.scene_trace_candidate_branch;
    let pruned_total = summary.metrics.scene_trace_support_pruned_branch;
    let total_considered = candidate_total.saturating_add(pruned_total);
    let prune_ratio = if total_considered == 0 {
        0.0
    } else {
        pruned_total as f64 / total_considered as f64
    };
    let avg_hit_steps = if summary.metrics.scene_trace_hit_count == 0 {
        0.0
    } else {
        summary.metrics.scene_trace_hit_steps_total as f64
            / summary.metrics.scene_trace_hit_count as f64
    };
    SceneTraceKpis {
        trace_total: summary.metrics.scene_trace,
        field_sample_total: summary.metrics.field_sample,
        candidate_total,
        pruned_total,
        prune_ratio,
        exact_total: summary.metrics.scene_trace_exact_path,
        conservative_total: summary.metrics.scene_trace_conservative_path,
        hit_total: summary.metrics.scene_trace_hit_count,
        avg_hit_steps,
    }
}

pub(crate) fn aggregate_perf_samples(samples: &[PerfSummary]) -> PerfSummary {
    if samples.len() == 1 {
        return samples[0].clone();
    }
    let len = samples.len() as f64;
    let mut metrics = MetricsTotals::default();
    for sample in samples {
        metrics.messages_sent += sample.metrics.messages_sent;
        metrics.messages_dropped += sample.metrics.messages_dropped;
        metrics.pending_resolved += sample.metrics.pending_resolved;
        metrics.pending_dropped += sample.metrics.pending_dropped;
        metrics.mailbox_high_water = metrics
            .mailbox_high_water
            .max(sample.metrics.mailbox_high_water);
        metrics.rc_inc += sample.metrics.rc_inc;
        metrics.rc_dec += sample.metrics.rc_dec;
        metrics.alloc_list += sample.metrics.alloc_list;
        metrics.alloc_map += sample.metrics.alloc_map;
        metrics.alloc_string += sample.metrics.alloc_string;
        metrics.alloc_bytes += sample.metrics.alloc_bytes;
        metrics.alloc_result += sample.metrics.alloc_result;
        metrics.alloc_pending += sample.metrics.alloc_pending;
        metrics.mailbox_enqueue_ok += sample.metrics.mailbox_enqueue_ok;
        metrics.mailbox_enqueue_fail += sample.metrics.mailbox_enqueue_fail;
        metrics.mailbox_dequeue += sample.metrics.mailbox_dequeue;
        metrics.sched_dispatched += sample.metrics.sched_dispatched;
        metrics.sched_skipped_no_credit += sample.metrics.sched_skipped_no_credit;
        metrics.sched_profile_switch += sample.metrics.sched_profile_switch;
        metrics.sched_starvation_violation += sample.metrics.sched_starvation_violation;
        metrics.sched_cross_shard_migration += sample.metrics.sched_cross_shard_migration;
        metrics.abi_typed_lane += sample.metrics.abi_typed_lane;
        metrics.abi_boxed_lane += sample.metrics.abi_boxed_lane;
        metrics.queue_cas_retry_total += sample.metrics.queue_cas_retry_total;
        metrics.mailbox_wake_coalesced_count += sample.metrics.mailbox_wake_coalesced_count;
        metrics.mailbox_rescue_wake_count += sample.metrics.mailbox_rescue_wake_count;
        metrics.sched_local_dispatch_count += sample.metrics.sched_local_dispatch_count;
        metrics.sched_global_dispatch_count += sample.metrics.sched_global_dispatch_count;
        metrics.sched_plan_recompute_count += sample.metrics.sched_plan_recompute_count;
        metrics.sched_steal_attempts += sample.metrics.sched_steal_attempts;
        metrics.sched_steal_success += sample.metrics.sched_steal_success;
        metrics.sched_migration_blocked_hysteresis +=
            sample.metrics.sched_migration_blocked_hysteresis;
        metrics.sched_migration_blocked_cooldown += sample.metrics.sched_migration_blocked_cooldown;
        metrics.queue_enqueue_p99_ns = metrics
            .queue_enqueue_p99_ns
            .max(sample.metrics.queue_enqueue_p99_ns);
        metrics.queue_dequeue_p99_ns = metrics
            .queue_dequeue_p99_ns
            .max(sample.metrics.queue_dequeue_p99_ns);
        metrics.queue_age_p99_ns = metrics
            .queue_age_p99_ns
            .max(sample.metrics.queue_age_p99_ns);
        metrics.sched_dispatch_loop_ns_p99 = metrics
            .sched_dispatch_loop_ns_p99
            .max(sample.metrics.sched_dispatch_loop_ns_p99);
        metrics.queue_burst_drain_avg = metrics
            .queue_burst_drain_avg
            .max(sample.metrics.queue_burst_drain_avg);
        metrics.scene_trace += sample.metrics.scene_trace;
        metrics.field_sample += sample.metrics.field_sample;
        metrics.scene_trace_support_pruned_branch +=
            sample.metrics.scene_trace_support_pruned_branch;
        metrics.scene_trace_candidate_branch += sample.metrics.scene_trace_candidate_branch;
        metrics.scene_trace_exact_path += sample.metrics.scene_trace_exact_path;
        metrics.scene_trace_conservative_path += sample.metrics.scene_trace_conservative_path;
        metrics.scene_trace_hit_count += sample.metrics.scene_trace_hit_count;
        metrics.scene_trace_hit_steps_total += sample.metrics.scene_trace_hit_steps_total;
        metrics.scene_trace_hit_field_samples_total +=
            sample.metrics.scene_trace_hit_field_samples_total;
        metrics.scene_trace_steps_le_1 += sample.metrics.scene_trace_steps_le_1;
        metrics.scene_trace_steps_le_4 += sample.metrics.scene_trace_steps_le_4;
        metrics.scene_trace_steps_le_8 += sample.metrics.scene_trace_steps_le_8;
        metrics.scene_trace_steps_le_16 += sample.metrics.scene_trace_steps_le_16;
        metrics.scene_trace_steps_gt_16 += sample.metrics.scene_trace_steps_gt_16;
        metrics.scene_trace_blend_cost += sample.metrics.scene_trace_blend_cost;
        metrics.scene_trace_deformation_cost += sample.metrics.scene_trace_deformation_cost;
    }
    let mut runtime_p50: Vec<u128> = samples.iter().map(|s| s.runtime_p50_ns).collect();
    let mut runtime_p95: Vec<u128> = samples.iter().map(|s| s.runtime_p95_ns).collect();
    let mut runtime_p99: Vec<u128> = samples.iter().map(|s| s.runtime_p99_ns).collect();
    runtime_p50.sort_unstable();
    runtime_p95.sort_unstable();
    runtime_p99.sort_unstable();
    PerfSummary {
        sample_count: samples.iter().map(|s| s.sample_count).sum(),
        compile_throughput_tests_per_sec: samples
            .iter()
            .map(|s| s.compile_throughput_tests_per_sec)
            .sum::<f64>()
            / len,
        runtime_p50_ns: runtime_p50[runtime_p50.len() / 2],
        runtime_p95_ns: runtime_p95[runtime_p95.len() / 2],
        runtime_p99_ns: runtime_p99[runtime_p99.len() / 2],
        allocs_per_request: samples.iter().map(|s| s.allocs_per_request).sum::<f64>() / len,
        rc_inc: (samples.iter().map(|s| s.rc_inc as f64).sum::<f64>() / len).round() as u64,
        rc_dec: (samples.iter().map(|s| s.rc_dec as f64).sum::<f64>() / len).round() as u64,
        rc_ops_total: (samples.iter().map(|s| s.rc_ops_total as f64).sum::<f64>() / len).round()
            as u64,
        dispatch_hit_ratio: samples.iter().map(|s| s.dispatch_hit_ratio).sum::<f64>() / len,
        check_fallback_rate: average_optional_f64(samples, |s| s.check_fallback_rate),
        avg_check_batch_size: average_optional_f64(samples, |s| s.avg_check_batch_size),
        check_oracle_eval_ns_p50: median_optional_u128(samples, |s| s.check_oracle_eval_ns_p50),
        check_oracle_eval_ns_p95: median_optional_u128(samples, |s| s.check_oracle_eval_ns_p95),
        effect_annihilation_rewrite_count: average_optional_u64(samples, |s| {
            s.effect_annihilation_rewrite_count
        }),
        scheduler_dispatch_p99_ns: median_optional_u128(samples, |s| s.scheduler_dispatch_p99_ns),
        scheduler_starvation_violations: average_optional_u64(samples, |s| {
            s.scheduler_starvation_violations
        }),
        rewrite_compile_overhead_pct: average_optional_f64(samples, |s| {
            s.rewrite_compile_overhead_pct
        }),
        rewrite_applied_count: average_optional_u64(samples, |s| s.rewrite_applied_count),
        actor_msgs_per_sec_p50: average_optional_f64(samples, |s| s.actor_msgs_per_sec_p50),
        actor_msgs_per_sec_p95: average_optional_f64(samples, |s| s.actor_msgs_per_sec_p95),
        queue_enqueue_p99_ns: median_optional_u128(samples, |s| s.queue_enqueue_p99_ns),
        queue_dequeue_p99_ns: median_optional_u128(samples, |s| s.queue_dequeue_p99_ns),
        queue_age_p99_ns: median_optional_u128(samples, |s| s.queue_age_p99_ns),
        mailbox_wake_coalesced_count: average_optional_u64(samples, |s| {
            s.mailbox_wake_coalesced_count
        }),
        mailbox_rescue_wake_count: average_optional_u64(samples, |s| s.mailbox_rescue_wake_count),
        queue_cas_retry_total: average_optional_u64(samples, |s| s.queue_cas_retry_total),
        cases: None,
        metrics,
    }
}

pub(crate) fn average_optional_f64(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<f64>,
) -> Option<f64> {
    let values: Vec<f64> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

pub(crate) fn average_optional_u64(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<u64>,
) -> Option<u64> {
    let values: Vec<u64> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        Some((values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64).round() as u64)
    }
}

pub(crate) fn median_optional_u128(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<u128>,
) -> Option<u128> {
    let mut values: Vec<u128> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        values.sort_unstable();
        Some(values[values.len() / 2])
    }
}

pub(crate) fn coefficient_of_variation(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean.abs() <= f64::EPSILON {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| {
            let d = *value - mean;
            d * d
        })
        .sum::<f64>()
        / values.len() as f64;
    (variance.sqrt() / mean) * 100.0
}

pub(crate) fn compute_cv(samples: &[PerfSummary]) -> PerfCv {
    let cv_samples: &[PerfSummary] = if samples.len() > 3 {
        &samples[1..]
    } else {
        samples
    };
    let compile: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.compile_throughput_tests_per_sec)
        .collect();
    let p50: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p50_ns as f64)
        .collect();
    let p95: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p95_ns as f64)
        .collect();
    let p99: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p99_ns as f64)
        .collect();
    PerfCv {
        compile_throughput_pct: coefficient_of_variation(&compile),
        runtime_p50_pct: coefficient_of_variation(&p50),
        runtime_p95_pct: coefficient_of_variation(&p95),
        runtime_p99_pct: coefficient_of_variation(&p99),
    }
}

pub(crate) fn load_perf_baseline_summary(path: &Path) -> Result<PerfSummary, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    if let Ok(report) = serde_json::from_slice::<PerfReport>(&bytes) {
        return Ok(report.summary);
    }
    serde_json::from_slice::<PerfSummary>(&bytes).map_err(|err| err.to_string())
}

pub(crate) fn evaluate_perf_gate(
    current: &PerfSummary,
    baseline: &PerfSummary,
    max_regression_pct: f64,
    kpi_thresholds: &KpiThresholds,
) -> Vec<String> {
    let mut failures = Vec::new();
    let up = 1.0 + (max_regression_pct / 100.0);
    let down = 1.0 - (max_regression_pct / 100.0);

    let runtime_p50_limit = baseline.runtime_p50_ns as f64 * up;
    if current.runtime_p50_ns as f64 > runtime_p50_limit {
        failures.push(format!(
            "runtime_p50_ns {} > {:.0}",
            current.runtime_p50_ns, runtime_p50_limit
        ));
    }
    let runtime_p95_limit = baseline.runtime_p95_ns as f64 * up;
    if current.runtime_p95_ns as f64 > runtime_p95_limit {
        failures.push(format!(
            "runtime_p95_ns {} > {:.0}",
            current.runtime_p95_ns, runtime_p95_limit
        ));
    }
    let runtime_p99_limit = baseline.runtime_p99_ns as f64 * up;
    if current.runtime_p99_ns as f64 > runtime_p99_limit {
        failures.push(format!(
            "runtime_p99_ns {} > {:.0}",
            current.runtime_p99_ns, runtime_p99_limit
        ));
    }
    let compile_min = baseline.compile_throughput_tests_per_sec * down;
    if float_below_limit(current.compile_throughput_tests_per_sec, compile_min) {
        failures.push(format!(
            "compile_tps {:.2} < {:.2}",
            current.compile_throughput_tests_per_sec, compile_min
        ));
    }
    let allocs_max = baseline.allocs_per_request * up;
    if float_exceeds_limit(current.allocs_per_request, allocs_max) {
        failures.push(format!(
            "allocs/request {:.2} > {:.2}",
            current.allocs_per_request, allocs_max
        ));
    }
    let dispatch_min = baseline.dispatch_hit_ratio * down;
    if float_below_limit(current.dispatch_hit_ratio, dispatch_min) {
        failures.push(format!(
            "dispatch_hit_ratio {:.4} < {:.4}",
            current.dispatch_hit_ratio, dispatch_min
        ));
    }
    if let (Some(current_value), Some(limit)) = (
        current.check_fallback_rate,
        kpi_thresholds.check_fallback_max,
    ) && current_value > limit
    {
        failures.push(format!(
            "check_fallback_rate {:.4} > {:.4}",
            current_value, limit
        ));
    }
    if let (Some(current_value), Some(min)) =
        (current.avg_check_batch_size, kpi_thresholds.check_batch_min)
        && current_value < min
    {
        failures.push(format!(
            "avg_check_batch_size {:.2} < {:.2}",
            current_value, min
        ));
    }
    if let (Some(current_value), Some(baseline_value), Some(min_improve_pct)) = (
        current.scheduler_dispatch_p99_ns,
        baseline.scheduler_dispatch_p99_ns,
        kpi_thresholds.scheduler_p99_improve_min_pct,
    ) && baseline_value > 0
    {
        let improvement_pct =
            ((baseline_value as f64 - current_value as f64) / baseline_value as f64) * 100.0;
        if improvement_pct < min_improve_pct {
            failures.push(format!(
                "scheduler_dispatch_p99_ns improvement {:.2}% < {:.2}%",
                improvement_pct, min_improve_pct
            ));
        }
    }
    if let (Some(current_value), Some(limit)) = (
        current.rewrite_compile_overhead_pct,
        kpi_thresholds.rewrite_overhead_max_pct,
    ) && current_value > limit
    {
        failures.push(format!(
            "rewrite_compile_overhead_pct {:.2} > {:.2}",
            current_value, limit
        ));
    }
    if let (Some(current_value), Some(baseline_value), Some(min_improve_pct)) = (
        current.actor_msgs_per_sec_p50,
        baseline.actor_msgs_per_sec_p50,
        kpi_thresholds.actor_throughput_improve_min_pct,
    ) && baseline_value > 0.0
    {
        let improvement_pct = ((current_value - baseline_value) / baseline_value) * 100.0;
        if improvement_pct < min_improve_pct {
            failures.push(format!(
                "actor_msgs_per_sec_p50 improvement {:.2}% < {:.2}%",
                improvement_pct, min_improve_pct
            ));
        }
    }
    if let (Some(current_value), Some(baseline_value), Some(max_regress_pct)) = (
        current.queue_age_p99_ns,
        baseline.queue_age_p99_ns,
        kpi_thresholds.queue_age_p99_max_regress_pct,
    ) && baseline_value > 0
    {
        let regress_pct =
            ((current_value as f64 - baseline_value as f64) / baseline_value as f64) * 100.0;
        if regress_pct > max_regress_pct {
            failures.push(format!(
                "queue_age_p99_ns regression {:.2}% > {:.2}%",
                regress_pct, max_regress_pct
            ));
        }
    }
    if let Some(max_violations) = kpi_thresholds.starvation_violations_max {
        let current_violations = current
            .scheduler_starvation_violations
            .unwrap_or(current.metrics.sched_starvation_violation);
        if current_violations as f64 > max_violations {
            failures.push(format!(
                "scheduler_starvation_violations {} > {:.0}",
                current_violations, max_violations
            ));
        }
    }
    if let Some(min_improve_pct) = kpi_thresholds.scheduler_throughput_improve_min_pct {
        let baseline_value = baseline.metrics.sched_dispatched as f64;
        let current_value = current.metrics.sched_dispatched as f64;
        if baseline_value > 0.0 {
            let improvement_pct = ((current_value - baseline_value) / baseline_value) * 100.0;
            if improvement_pct < min_improve_pct {
                failures.push(format!(
                    "scheduler_dispatched improvement {:.2}% < {:.2}%",
                    improvement_pct, min_improve_pct
                ));
            }
        }
    }
    if let Some(max_regress_pct) = kpi_thresholds.scheduler_loop_p99_max_regress_pct
        && let (Some(current_p99), Some(baseline_p99)) = (
            current.scheduler_dispatch_p99_ns,
            baseline.scheduler_dispatch_p99_ns,
        )
        && baseline_p99 > 0
    {
        let regress_pct =
            ((current_p99 as f64 - baseline_p99 as f64) / baseline_p99 as f64) * 100.0;
        if regress_pct > max_regress_pct {
            failures.push(format!(
                "scheduler_dispatch_p99_ns regression {:.2}% > {:.2}%",
                regress_pct, max_regress_pct
            ));
        }
    }
    if let Some(min_ratio) = kpi_thresholds.scheduler_local_hit_min {
        let local = current.metrics.sched_local_dispatch_count as f64;
        let global = current.metrics.sched_global_dispatch_count as f64;
        let total = local + global;
        if total > 0.0 {
            let ratio = local / total;
            if ratio < min_ratio {
                failures.push(format!(
                    "scheduler_local_dispatch_ratio {:.4} < {:.4}",
                    ratio, min_ratio
                ));
            }
        }
    }
    failures
}

pub(crate) fn float_exceeds_limit(current: f64, limit: f64) -> bool {
    current - limit > float_compare_tolerance(current, limit)
}

pub(crate) fn float_below_limit(current: f64, limit: f64) -> bool {
    limit - current > float_compare_tolerance(current, limit)
}

pub(crate) fn float_compare_tolerance(left: f64, right: f64) -> f64 {
    1e-9_f64.max(left.abs().max(right.abs()) * 1e-9)
}

pub(crate) fn percentile(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let n = samples.len();
    let rank = (pct * (n as f64 - 1.0)).ceil() as usize;
    samples[rank.min(n - 1)]
}

pub(crate) fn collect_tests(
    root: &Path,
    tests_root: &Path,
    out: &mut Vec<TestCase>,
) -> io::Result<()> {
    let mut children: Vec<PathBuf> = fs::read_dir(root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|a| path_sort_key(a));
    for path in children {
        if path.is_dir() {
            if is_generated_test_wrapper_dir(&path, tests_root) {
                continue;
            }
            collect_tests(&path, tests_root, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("wr") {
            continue;
        }
        enforce_test_file_suffix(&path)?;
        let source = fs::read_to_string(&path)?;
        let module_path = module_path_for_test_file(&path, tests_root)?;
        collect_tests_from_source(&source, &module_path, &path, true, out)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    }
    Ok(())
}

#[cfg(test)]
mod perf_gate_tests {
    use super::*;

    fn perf_summary_fixture() -> PerfSummary {
        PerfSummary {
            sample_count: 1,
            compile_throughput_tests_per_sec: 0.0,
            runtime_p50_ns: 100,
            runtime_p95_ns: 200,
            runtime_p99_ns: 300,
            allocs_per_request: 12.4,
            rc_inc: 0,
            rc_dec: 0,
            rc_ops_total: 0,
            dispatch_hit_ratio: 1.0,
            check_fallback_rate: None,
            avg_check_batch_size: None,
            check_oracle_eval_ns_p50: None,
            check_oracle_eval_ns_p95: None,
            effect_annihilation_rewrite_count: None,
            scheduler_dispatch_p99_ns: None,
            scheduler_starvation_violations: None,
            rewrite_compile_overhead_pct: None,
            rewrite_applied_count: None,
            actor_msgs_per_sec_p50: None,
            actor_msgs_per_sec_p95: None,
            queue_enqueue_p99_ns: None,
            queue_dequeue_p99_ns: None,
            queue_age_p99_ns: None,
            mailbox_wake_coalesced_count: None,
            mailbox_rescue_wake_count: None,
            queue_cas_retry_total: None,
            cases: None,
            metrics: MetricsTotals::default(),
        }
    }

    #[test]
    fn evaluate_perf_gate_ignores_float_rounding_noise() {
        let baseline = perf_summary_fixture();
        let mut current = perf_summary_fixture();
        current.allocs_per_request = 12.400000000000004;
        let failures = evaluate_perf_gate(&current, &baseline, 0.0, &KpiThresholds::default());
        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("allocs/request")),
            "{failures:?}"
        );
    }
}

pub(crate) fn is_generated_test_wrapper_dir(path: &Path, tests_root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(tests_root) else {
        return false;
    };
    matches!(
        rel.components()
            .next()
            .and_then(|component| component.as_os_str().to_str()),
        Some("wrela_harness" | "wrela_mutation")
    )
}

pub(crate) fn enforce_test_file_suffix(path: &Path) -> io::Result<()> {
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid test file name: {}", path.display()),
        )
    })?;
    if !name.ends_with("_test.wr") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("test file must end with `_test.wr`: {}", path.display()),
        ));
    }
    Ok(())
}

pub(crate) fn module_path_for_test_file(path: &Path, tests_root: &Path) -> io::Result<String> {
    let rel = path.strip_prefix(tests_root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("test file must live under {}", tests_root.display()),
        )
    })?;
    let mut rel_path = rel.to_path_buf();
    rel_path.set_extension("");
    let mut parts: Vec<String> = rel_path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(last) = parts.last_mut()
        && let Some(stripped) = last.strip_suffix("_test")
    {
        *last = stripped.to_string();
    }
    Ok(format!("tests/{}", parts.join("/")))
}

pub(crate) fn module_path_for_single_file(path: &Path) -> io::Result<String> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid test file name: {}", path.display()),
        )
    })?;
    Ok(stem.to_string())
}

pub(crate) fn collect_tests_from_source(
    source: &str,
    module_path: &str,
    source_path: &Path,
    enforce_function_name_contract: bool,
    out: &mut Vec<TestCase>,
) -> Result<(), String> {
    use wrela::parser::ast::AstNode;

    let (syntax, parse_errors) = parser::parse_with_errors(source);
    if !parse_errors.is_empty() {
        const MAX_DIAGNOSTICS: usize = 5;
        let mut diagnostics = parse_errors
            .iter()
            .take(MAX_DIAGNOSTICS)
            .map(|err| {
                format!(
                    "{}:{}:{}: {}",
                    source_path.display(),
                    err.span.offset(),
                    err.span.len(),
                    err.message
                )
            })
            .collect::<Vec<_>>();
        if parse_errors.len() > MAX_DIAGNOSTICS {
            diagnostics.push(format!(
                "... and {} more parse errors",
                parse_errors.len() - MAX_DIAGNOSTICS
            ));
        }
        return Err(format!(
            "parse-invalid test file detected during discovery:\n{}",
            diagnostics.join("\n")
        ));
    }
    let root = parser::ast::Root::cast(syntax)
        .ok_or_else(|| "internal parser error: expected root syntax node".to_string())?;
    let module = hir::lower::lower(root);
    let lane = infer_test_lane(module_path);
    let mut discovered = Vec::new();
    for (_, func) in module.functions.iter() {
        if func.kind != hir::FunctionKind::Function {
            continue;
        }
        let func_name = func.name.to_string();
        if enforce_function_name_contract
            && func_name.starts_with("test")
            && !is_test_function_name(&func_name)
        {
            return Err(format!(
                "test naming error: {}::{} must start with `test_`",
                module_path, func_name
            ));
        }
        if !is_test_function_name(&func_name) {
            continue;
        }
        let attrs = parse_test_attributes(func);
        if !attrs.unknown.is_empty() {
            return Err(format!(
                "test attribute error: {}::{} uses unsupported attributes [{}]; allowed attributes are @serial, @allows_env_set, @allows_fs_escape",
                module_path,
                func_name,
                attrs.unknown.join(", ")
            ));
        }
        if lane == TestLane::Spec && (attrs.allows_env_set || attrs.allows_fs_escape) {
            return Err(format!(
                "teacher: spec lane forbids capability exceptions; remove @allows_* from {}::{} or move the test under tests/integration/**",
                module_path, func_name
            ));
        }
        if lane != TestLane::Integration && (attrs.allows_env_set || attrs.allows_fs_escape) {
            return Err(format!(
                "test attribute error: capability exceptions are only allowed in integration lane; move {}::{} under tests/integration/**",
                module_path, func_name
            ));
        }
        let stable_id = stable_test_id(module_path, &func_name);
        discovered.push(TestCase {
            id: stable_id.clone(),
            lane,
            name: format!("{module_path}::{func_name}"),
            module_path: module_path.to_string(),
            func_name,
            is_serial: attrs.serial,
            allows_env_set: attrs.allows_env_set,
            allows_fs_escape: attrs.allows_fs_escape,
            has_oracle: function_has_oracle(func),
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: stable_id,
        });
    }
    if enforce_function_name_contract && discovered.is_empty() {
        return Err(format!(
            "test discovery error: {} must define at least one `test_` function",
            module_path
        ));
    }
    discovered.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out.extend(discovered);
    Ok(())
}

#[derive(Default)]
pub(crate) struct ParsedTestAttributes {
    pub(crate) serial: bool,
    pub(crate) allows_env_set: bool,
    pub(crate) allows_fs_escape: bool,
    pub(crate) unknown: Vec<String>,
}

pub(crate) fn parse_test_attributes(func: &hir::Function) -> ParsedTestAttributes {
    let mut parsed = ParsedTestAttributes::default();
    for attr in &func.attributes {
        match attr.name.as_str() {
            "serial" => parsed.serial = true,
            "allows_env_set" => parsed.allows_env_set = true,
            "allows_fs_escape" => parsed.allows_fs_escape = true,
            other => parsed.unknown.push(format!("@{other}")),
        }
    }
    parsed
}

pub(crate) fn collect_autogen_spec_tests(
    workspace_root: &Path,
    max_cases: u64,
    time_cap_ms: u64,
) -> Result<Vec<TestCase>, String> {
    let max_cases = max_cases as usize;
    if max_cases == 0 {
        return Ok(Vec::new());
    }
    let checks = discover_autogen_checks(workspace_root)?;
    Ok(generate_autogen_spec_tests(&checks, max_cases, time_cap_ms))
}

pub(crate) fn discover_autogen_checks(
    workspace_root: &Path,
) -> Result<Vec<AutogenCheckDecl>, String> {
    use wrela::parser::ast::AstNode;

    let mut modules = Vec::new();
    let src_root = workspace_root.join("src");
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;
    let tests_root = workspace_root.join("tests");
    let spec_root = tests_root.join("spec");
    collect_wr_modules(&spec_root, &tests_root, "tests", &mut modules)?;

    let mut discovered = Vec::new();
    for module_source in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module_source.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let module = hir::lower::lower(root);
        for (_, func) in module.functions.iter() {
            if !matches!(
                func.kind,
                hir::FunctionKind::Check | hir::FunctionKind::Function
            ) {
                continue;
            }
            if func.name.as_str() == "run" || is_test_function_name(func.name.as_str()) {
                continue;
            }
            let Some(check) = autogen_check_decl_from_function(
                &module_source.module_path,
                func.name.as_str(),
                func,
                module_source.source.as_str(),
            ) else {
                continue;
            };
            discovered.push(check);
        }
    }
    discovered.sort_by(|a, b| {
        a.module_path
            .cmp(&b.module_path)
            .then(a.func_name.cmp(&b.func_name))
    });
    Ok(discovered)
}

pub(crate) fn autogen_check_decl_from_function(
    module_path: &str,
    func_name: &str,
    func: &hir::Function,
    module_source: &str,
) -> Option<AutogenCheckDecl> {
    let ret = func.ret_type.as_ref()?;
    if !autogen_type_ref_is_scalar(ret, AutogenScalarType::Boolean) {
        return None;
    }
    let mut params = Vec::with_capacity(func.params.len());
    for param in &func.params {
        let ty = param.ty.as_ref()?;
        let scalar = autogen_scalar_type_from_ref(ty)?;
        params.push(AutogenCheckParam {
            name: param.name.to_string(),
            ty: scalar,
        });
    }
    Some(AutogenCheckDecl {
        module_path: module_path.to_string(),
        func_name: func_name.to_string(),
        params,
        module_source: module_source.to_string(),
        source_span: func
            .name_span
            .map(|span| format!("{}..{}", u32::from(span.start()), u32::from(span.end()))),
    })
}

pub(crate) fn autogen_scalar_type_from_ref(ty: &hir::TypeRef) -> Option<AutogenScalarType> {
    if !ty.args.is_empty() {
        return None;
    }
    match ty.name.as_str() {
        "Integer" => Some(AutogenScalarType::Integer),
        "Boolean" => Some(AutogenScalarType::Boolean),
        "String" => Some(AutogenScalarType::String),
        _ => None,
    }
}

pub(crate) fn autogen_type_ref_is_scalar(ty: &hir::TypeRef, expected: AutogenScalarType) -> bool {
    autogen_scalar_type_from_ref(ty) == Some(expected)
}

pub(crate) fn generate_autogen_spec_tests(
    checks: &[AutogenCheckDecl],
    max_cases: usize,
    time_cap_ms: u64,
) -> Vec<TestCase> {
    let mut generated = Vec::new();
    if checks.is_empty() || max_cases == 0 {
        return generated;
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let mut case_index = 0usize;
    while generated.len() < max_cases && started.elapsed() < time_cap {
        let before = generated.len();
        for check in checks {
            if generated.len() >= max_cases {
                break;
            }
            if started.elapsed() >= time_cap {
                break;
            }
            let case_seed = fnv1a64(
                format!("{}::{}::{case_index}", check.module_path, check.func_name).as_bytes(),
            );
            let call_body = autogen_given_call(check, case_index);
            generated.push(TestCase {
                id: stable_autogen_test_id(&check.module_path, &check.func_name, case_index),
                lane: TestLane::Spec,
                name: format!(
                    "{}::{}::autogen_case_{:04}",
                    check.module_path, check.func_name, case_index
                ),
                module_path: check.module_path.clone(),
                func_name: check.func_name.clone(),
                is_serial: false,
                allows_env_set: false,
                allows_fs_escape: false,
                has_oracle: true,
                generated_call_body: Some(call_body.clone()),
                generated_case_kind: Some(GeneratedCaseKind::Autogen),
                generated_entry_source: Some(autogen_standalone_entry_source(
                    &check.module_source,
                    &call_body,
                )),
                autogen_module_source: Some(check.module_source.clone()),
                autogen_seed: Some(case_seed),
                autogen_span: check.source_span.clone(),
                sim_seed: None,
                canonical_id: stable_autogen_test_id(
                    &check.module_path,
                    &check.func_name,
                    case_index,
                ),
            });
        }
        if generated.len() == before {
            break;
        }
        case_index = case_index.saturating_add(1);
    }
    generated
}

pub(crate) fn stable_autogen_test_id(
    module_path: &str,
    func_name: &str,
    case_index: usize,
) -> String {
    format!(
        "autogen:{}",
        fnv1a64_hex(format!("{module_path}::{func_name}::{case_index}").as_bytes())
    )
}

pub(crate) fn autogen_given_call(check: &AutogenCheckDecl, case_index: usize) -> String {
    if check.params.is_empty() {
        return format!("{}()", check.func_name);
    }
    let mut args = Vec::with_capacity(check.params.len());
    for (param_index, param) in check.params.iter().enumerate() {
        let value = autogen_scalar_literal(
            param.ty,
            &check.module_path,
            &check.func_name,
            case_index,
            param_index,
        );
        args.push(format!("{}={value}", param.name));
    }
    format!("{}({})", check.func_name, args.join(", "))
}

pub(crate) fn autogen_standalone_entry_source(module_source: &str, call_body: &str) -> String {
    let rewritten = module_source.replacen("fn run(", "fn autogen_hidden_run(", 1);
    format!(
        "{rewritten}\n\nfn run() -> Integer {{\n    assert value ({call_body}) == true\n    return 0\n}}\n"
    )
}

pub(crate) fn autogen_scalar_literal(
    ty: AutogenScalarType,
    module_path: &str,
    func_name: &str,
    case_index: usize,
    param_index: usize,
) -> String {
    let boundary_index = case_index / 2 + param_index;
    if case_index.is_multiple_of(2) {
        return autogen_boundary_literal(ty, boundary_index);
    }
    let seed =
        fnv1a64(format!("{module_path}::{func_name}::{case_index}::{param_index}").as_bytes());
    autogen_random_literal(ty, seed)
}

pub(crate) fn autogen_boundary_literal(ty: AutogenScalarType, boundary_index: usize) -> String {
    match ty {
        AutogenScalarType::Integer => {
            let values = ["0", "1", "-1", "2147483647", "-2147483648"];
            values[boundary_index % values.len()].to_string()
        }
        AutogenScalarType::Boolean => {
            if boundary_index.is_multiple_of(2) {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        AutogenScalarType::String => {
            let values = ["\"\"", "\"a\"", "\"edge\"", "\"hello0\"", "\"z9\""];
            values[boundary_index % values.len()].to_string()
        }
    }
}

pub(crate) fn autogen_random_literal(ty: AutogenScalarType, seed: u64) -> String {
    let mut state = autogen_mix64(seed ^ 0xA670);
    match ty {
        AutogenScalarType::Integer => {
            state = autogen_mix64(state);
            let value = (state % 2001) as i64 - 1000;
            value.to_string()
        }
        AutogenScalarType::Boolean => {
            state = autogen_mix64(state);
            if state.is_multiple_of(2) {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        AutogenScalarType::String => {
            state = autogen_mix64(state);
            let len = ((state % 8) + 1) as usize;
            let mut out = String::with_capacity(len + 2);
            out.push('"');
            for _ in 0..len {
                state = autogen_mix64(state);
                let ch = match state % 36 {
                    value @ 0..=25 => (b'a' + value as u8) as char,
                    value => (b'0' + (value as u8 - 26)) as char,
                };
                out.push(ch);
            }
            out.push('"');
            out
        }
    }
}

pub(crate) fn autogen_mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(crate) fn collect_fuzz_tests(
    workspace_root: &Path,
    max_cases: u64,
    time_cap_ms: u64,
) -> Result<Vec<TestCase>, String> {
    let max_cases = max_cases as usize;
    if max_cases == 0 {
        return Ok(Vec::new());
    }
    let targets = discover_fuzz_targets(workspace_root)?;
    Ok(generate_fuzz_tests(&targets, max_cases, time_cap_ms))
}

pub(crate) fn discover_fuzz_targets(workspace_root: &Path) -> Result<Vec<FuzzTargetDecl>, String> {
    use wrela::parser::ast::AstNode;

    let mut modules = Vec::new();
    let src_root = workspace_root.join("src");
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;

    let mut discovered = Vec::new();
    for module_source in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module_source.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let module = hir::lower::lower(root);
        for (_, func) in module.functions.iter() {
            if func.kind != hir::FunctionKind::Function {
                continue;
            }
            let Some(target) = fuzz_target_decl_from_function(
                &module_source.module_path,
                func.name.as_str(),
                func,
                module_source.source.as_str(),
            ) else {
                continue;
            };
            discovered.push(target);
        }
    }
    discovered.sort_by(|a, b| {
        a.module_path
            .cmp(&b.module_path)
            .then(a.func_name.cmp(&b.func_name))
    });
    Ok(discovered)
}

pub(crate) fn fuzz_target_decl_from_function(
    module_path: &str,
    func_name: &str,
    func: &hir::Function,
    module_source: &str,
) -> Option<FuzzTargetDecl> {
    let is_target = func_name.starts_with("try_to_parse_")
        || func_name.starts_with("try_to_decode_")
        || func_name.starts_with("try_to_deserialize_");
    if !is_target {
        return None;
    }
    if func.params.len() != 1 {
        return None;
    }
    let param = &func.params[0];
    let ty = param.ty.as_ref()?;
    let param_ty = fuzz_param_type_from_ref(ty)?;
    Some(FuzzTargetDecl {
        module_path: module_path.to_string(),
        func_name: func_name.to_string(),
        param_name: param.name.to_string(),
        param_ty,
        module_source: module_source.to_string(),
        source_span: func
            .name_span
            .map(|span| format!("{}..{}", u32::from(span.start()), u32::from(span.end()))),
    })
}

pub(crate) fn fuzz_param_type_from_ref(ty: &hir::TypeRef) -> Option<FuzzParamType> {
    if !ty.args.is_empty() {
        return None;
    }
    match ty.name.as_str() {
        "String" => Some(FuzzParamType::String),
        "Bytes" => Some(FuzzParamType::Bytes),
        _ => None,
    }
}

pub(crate) fn generate_fuzz_tests(
    targets: &[FuzzTargetDecl],
    max_cases: usize,
    time_cap_ms: u64,
) -> Vec<TestCase> {
    let mut generated = Vec::new();
    if targets.is_empty() || max_cases == 0 {
        return generated;
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let mut case_index = 0usize;
    while generated.len() < max_cases && started.elapsed() < time_cap {
        let before = generated.len();
        for target in targets {
            if generated.len() >= max_cases || started.elapsed() >= time_cap {
                break;
            }
            let seed = fnv1a64(
                format!(
                    "fuzz::{}::{}::{case_index}",
                    target.module_path, target.func_name
                )
                .as_bytes(),
            );
            let call_body = fuzz_given_call(target, seed, case_index);
            let case_id = stable_fuzz_test_id(&target.module_path, &target.func_name, case_index);
            generated.push(TestCase {
                id: case_id.clone(),
                lane: TestLane::Integration,
                name: format!(
                    "{}::{}::fuzz_case_{:04}",
                    target.module_path, target.func_name, case_index
                ),
                module_path: target.module_path.clone(),
                func_name: target.func_name.clone(),
                is_serial: false,
                allows_env_set: false,
                allows_fs_escape: false,
                has_oracle: true,
                generated_call_body: Some(call_body.clone()),
                generated_case_kind: Some(GeneratedCaseKind::Fuzz),
                generated_entry_source: Some(fuzz_standalone_entry_source(
                    &target.module_source,
                    &call_body,
                    target.param_ty == FuzzParamType::Bytes,
                )),
                autogen_module_source: Some(target.module_source.clone()),
                autogen_seed: Some(seed),
                autogen_span: target.source_span.clone(),
                sim_seed: None,
                canonical_id: case_id,
            });
        }
        if generated.len() == before {
            break;
        }
        case_index = case_index.saturating_add(1);
    }
    generated
}

pub(crate) fn stable_fuzz_test_id(module_path: &str, func_name: &str, case_index: usize) -> String {
    format!(
        "fuzz:{}",
        fnv1a64_hex(format!("{module_path}::{func_name}::{case_index}").as_bytes())
    )
}

pub(crate) fn fuzz_given_call(target: &FuzzTargetDecl, seed: u64, case_index: usize) -> String {
    let values = fuzz_input_bytes(seed, case_index);
    let arg = match target.param_ty {
        FuzzParamType::String => fuzz_string_literal(&values),
        FuzzParamType::Bytes => format!(
            "get_bytes_from_list(items=[{}])",
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!("{}({}={arg})", target.func_name, target.param_name)
}

pub(crate) fn fuzz_input_bytes(seed: u64, case_index: usize) -> Vec<u8> {
    let mut state = autogen_mix64(seed ^ 0xF022_9E37 ^ case_index as u64);
    let len = ((state % 24) + 1) as usize;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state = autogen_mix64(state);
        out.push((state % 256) as u8);
    }
    out
}

pub(crate) fn fuzz_string_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    for byte in bytes {
        let c = match byte {
            b'"' => "\\\"".to_string(),
            b'\\' => "\\\\".to_string(),
            32..=126 => (*byte as char).to_string(),
            _ => {
                let mapped = b'a' + (byte % 26);
                (mapped as char).to_string()
            }
        };
        out.push_str(&c);
    }
    out.push('"');
    out
}

pub(crate) fn fuzz_standalone_entry_source(
    module_source: &str,
    call_body: &str,
    include_bytes_helper: bool,
) -> String {
    let rewritten = module_source.replacen("fn run(", "fn fuzz_hidden_run(", 1);
    let bytes_use = if include_bytes_helper {
        "use get_bytes_from_list from bytes\n\n"
    } else {
        ""
    };
    format!(
        "{rewritten}\n\n{bytes_use}fn run() -> Integer {{\n    ignore result {call_body}\n    return 0\n}}\n"
    )
}

pub(crate) fn is_test_function_name(name: &str) -> bool {
    name.starts_with("test_")
}

pub(crate) fn function_has_oracle(func: &hir::Function) -> bool {
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    body_has_oracle(body, &body.root_stmts)
}

pub(crate) fn body_has_oracle(body: &hir::Body, stmts: &[hir::Idx<hir::Stmt>]) -> bool {
    for stmt_id in stmts {
        match &body.stmts[*stmt_id] {
            hir::Stmt::Assert { .. } | hir::Stmt::Require { .. } => return true,
            hir::Stmt::Optimize { body: nested, .. } => {
                if body_has_oracle(body, nested) {
                    return true;
                }
            }
            hir::Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                if body_has_oracle(body, then_branch)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| body_has_oracle(body, branch))
                {
                    return true;
                }
            }
            hir::Stmt::For {
                body: loop_body, ..
            }
            | hir::Stmt::While {
                body: loop_body, ..
            } => {
                if body_has_oracle(body, loop_body) {
                    return true;
                }
            }
            hir::Stmt::Match {
                cases, otherwise, ..
            } => {
                if cases.iter().any(|case| body_has_oracle(body, &case.body))
                    || otherwise
                        .as_ref()
                        .is_some_and(|branch| body_has_oracle(body, branch))
                {
                    return true;
                }
            }
            hir::Stmt::Expr(_)
            | hir::Stmt::Let { .. }
            | hir::Stmt::Assign { .. }
            | hir::Stmt::IgnoreResult { .. }
            | hir::Stmt::Capture { .. }
            | hir::Stmt::Defer { .. }
            | hir::Stmt::Return(_)
            | hir::Stmt::Use { .. }
            | hir::Stmt::Break
            | hir::Stmt::Continue => {}
        }
    }
    false
}

pub(crate) fn stable_test_id(module_path: &str, func_name: &str) -> String {
    fnv1a64_hex(format!("{module_path}::{func_name}").as_bytes())
}

pub(crate) fn stable_function_id(function_identity: &str) -> String {
    fnv1a64(function_identity.as_bytes()).to_string()
}

pub(crate) fn qualified_function_identity(module_path: &str, function_name: &str) -> String {
    format!("{module_path}::{function_name}")
}

pub(crate) fn infer_test_lane(module_path: &str) -> TestLane {
    let canonical = module_path.replace('\\', "/").to_ascii_lowercase();
    let segments: Vec<&str> = canonical
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let lane_segment = segments
        .windows(2)
        .find(|window| window[0] == "tests")
        .map(|window| window[1])
        .or_else(|| segments.first().copied())
        .unwrap_or_default();
    match lane_segment {
        "spec" => TestLane::Spec,
        "integration" => TestLane::Integration,
        "sim" => TestLane::Sim,
        "model" => TestLane::Model,
        _ => TestLane::Default,
    }
}

pub(crate) fn parse_test_lane_filter(value: &str) -> Option<TestLaneSelection> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fast" => Some(TestLaneSelection::Preset(TestLanePreset::Fast)),
        "full" => Some(TestLaneSelection::Preset(TestLanePreset::Full)),
        "spec" => Some(TestLaneSelection::Single(TestLane::Spec)),
        "integration" => Some(TestLaneSelection::Single(TestLane::Integration)),
        "sim" => Some(TestLaneSelection::Single(TestLane::Sim)),
        "model" => Some(TestLaneSelection::Single(TestLane::Model)),
        "default" => Some(TestLaneSelection::Single(TestLane::Default)),
        _ => None,
    }
}

pub(crate) fn enforce_serial_test_cap(tests: &[TestCase]) -> Result<(), String> {
    let total = tests.len();
    if total == 0 {
        return Ok(());
    }
    let serial_count = tests.iter().filter(|test| test.is_serial).count();
    if serial_count == 0 {
        return Ok(());
    }
    let pct_cap = ((total as f64) * 0.05).ceil() as usize;
    let pct_cap = pct_cap.max(1);
    if serial_count <= pct_cap && serial_count <= 10 {
        return Ok(());
    }
    Err(format!(
        "serial test cap exceeded: {} serial tests out of {} total. policy is <=5% (cap {}) and <=10 absolute. reduce @serial usage or redesign tests to run in parallel",
        serial_count, total, pct_cap
    ))
}

pub(crate) fn select_tests(mut tests: Vec<TestCase>, selection: &TestSelection) -> Vec<TestCase> {
    if let Some(include_ids) = selection.include_ids.as_ref() {
        tests.retain(|test| include_ids.contains(&test.id));
    }
    if let Some(id) = selection.id.as_ref() {
        tests.retain(|test| test.id == *id);
    }
    if let Some(pattern) = selection.filter.as_ref() {
        tests.retain(|test| {
            test.name.contains(pattern)
                || test.id.contains(pattern)
                || test.module_path.contains(pattern)
                || test.lane.as_str().contains(pattern)
        });
    }
    if let Some(lane) = selection.lane {
        tests.retain(|test| lane.matches(test.lane));
    }
    tests
}

pub(crate) fn expand_sim_seed_cases(
    tests: Vec<TestCase>,
    sim_seed_override: Option<u64>,
    certify_mode: bool,
) -> Vec<TestCase> {
    let mut expanded = Vec::new();
    for test in tests {
        if test.lane != TestLane::Sim && test.lane != TestLane::Model {
            expanded.push(test);
            continue;
        }
        if let Some(seed) = sim_seed_override {
            expanded.push(sim_seed_variant(&test, seed));
            continue;
        }
        if certify_mode {
            let max_seed = if test.lane == TestLane::Sim {
                256u64
            } else {
                64u64
            };
            for seed in 0..max_seed {
                expanded.push(sim_seed_variant(&test, seed));
            }
            continue;
        }
        expanded.push(sim_seed_variant(&test, TEST_JSON_SUMMARY_SEED));
    }
    expanded
}

pub(crate) fn sim_seed_variant(test: &TestCase, seed: u64) -> TestCase {
    let mut variant = test.clone();
    variant.sim_seed = Some(seed);
    variant.id = format!("{}::seed:{seed}", test.id);
    variant.name = format!("{} [seed={}]", test.name, seed);
    variant
}

pub(crate) fn list_tests(tests: &[TestCase]) {
    for test in tests {
        let mut attrs = Vec::new();
        if test.is_serial {
            attrs.push("@serial");
        }
        if test.allows_env_set {
            attrs.push("@allows_env_set");
        }
        if test.allows_fs_escape {
            attrs.push("@allows_fs_escape");
        }
        let attrs_suffix = if attrs.is_empty() {
            String::new()
        } else {
            format!(" attrs={}", attrs.join(","))
        };
        println!(
            "id={} lane={} name={}{}",
            test.id,
            test.lane.as_str(),
            test.name,
            attrs_suffix
        );
    }
    println!("tests: {} listed", tests.len());
}

pub(crate) fn summarize_run_lane(tests: &[TestCase]) -> String {
    let Some(first) = tests.first() else {
        return "none".to_string();
    };
    let first_lane = first.lane.as_str();
    if tests.iter().all(|test| test.lane.as_str() == first_lane) {
        first_lane.to_string()
    } else {
        "mixed".to_string()
    }
}

pub(crate) fn summarize_run_lane_from_json_cases(cases: &[TestJsonCase]) -> String {
    let Some(first) = cases.first() else {
        return "none".to_string();
    };
    let first_lane = first.lane.as_str();
    if cases.iter().all(|case| case.lane == first_lane) {
        first_lane.to_string()
    } else {
        "mixed".to_string()
    }
}

pub(crate) fn emit_test_json_summary(summary: &TestJsonSummary) {
    println!(
        "{}",
        serde_json::to_string(summary).unwrap_or_else(|_| "{}".to_string())
    );
}

pub(crate) fn compile_test_harness(
    workspace_root: &Path,
    compile_root: &Path,
    tests_root: Option<&Path>,
    tests: &[TestCase],
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
    harness_cache: Option<&mut HashMap<String, TestHarness>>,
) -> Result<TestHarness, String> {
    let temp_dir = workspace_root.join("target").join("wrela_tests");
    fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create test temp directory: {err}"))?;
    let mut cache_key_hasher = Fnv1a64::new();
    cache_key_hasher.update(compile_root.to_string_lossy().as_bytes());
    cache_key_hasher.update(&[0]);
    if let Some(root) = tests_root {
        cache_key_hasher.update(root.to_string_lossy().as_bytes());
        cache_key_hasher.update(&[0]);
    }
    for test in tests {
        cache_key_hasher.update(test.id.as_bytes());
        cache_key_hasher.update(&[0]);
        cache_key_hasher.update(test.module_path.as_bytes());
        cache_key_hasher.update(&[0]);
        cache_key_hasher.update(test.func_name.as_bytes());
        cache_key_hasher.update(&[0]);
    }
    let harness_key = format!("harness_{}", cache_key_hasher.finish_hex());
    if let Some(cache) = harness_cache.as_deref()
        && let Some(existing) = cache.get(&harness_key)
    {
        return Ok(TestHarness {
            exe_path: existing.exe_path.clone(),
            compile_ns: 0,
            cache_hit: true,
        });
    }
    let run_dir = temp_dir.join(&harness_key);
    fs::create_dir_all(&run_dir)
        .map_err(|err| format!("failed to create harness directory: {err}"))?;
    let entry_path = run_dir.join("entry.wr");
    let exe_path = run_dir.join("harness_bin");
    let meta_path = run_dir.join("harness.meta.json");
    let expected_meta = TestHarnessMeta {
        schema_version: TEST_HARNESS_META_SCHEMA_VERSION,
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        selected_tests_fingerprint: selected_test_fingerprint(tests),
        source_fingerprint: harness_source_fingerprint(compile_root, tests_root)?,
    };
    if exe_path.is_file()
        && let Ok(bytes) = fs::read(&meta_path)
        && let Ok(meta) = serde_json::from_slice::<TestHarnessMeta>(&bytes)
        && meta == expected_meta
    {
        let harness = TestHarness {
            exe_path: exe_path.clone(),
            compile_ns: 0,
            cache_hit: true,
        };
        if let Some(cache) = harness_cache {
            cache.insert(harness_key, harness.clone());
        }
        return Ok(harness);
    }

    let mut source = String::new();
    let harness_tests: Vec<&TestCase> = tests
        .iter()
        .filter(|test| test.generated_entry_source.is_none())
        .collect();
    let mut dispatch_arms: Vec<(String, String)> = Vec::with_capacity(harness_tests.len());

    let use_wrappers = tests_root.is_some() && has_duplicate_test_function_names(&harness_tests);
    let mut wrappers_root: Option<PathBuf> = None;
    if use_wrappers {
        let tests_root = tests_root.expect("project tests root");
        let wrappers_dir = tests_root
            .join("wrela_harness")
            .join(&harness_key)
            .join("cases");
        fs::create_dir_all(&wrappers_dir).map_err(|err| {
            format!(
                "failed to create harness cases directory {}: {err}",
                wrappers_dir.display()
            )
        })?;
        wrappers_root = Some(tests_root.join("wrela_harness").join(&harness_key));
        for (idx, test) in harness_tests.iter().enumerate() {
            let wrapper_func = format!("run_case_{idx}");
            let wrapper_module = format!("tests/wrela_harness/{harness_key}/cases/case_{idx}_test");
            let import_module = harness_import_module_path(&test.module_path);
            let wrapper_source = format!(
                "use {func} from {module}\n\nfn {wrapper_func}() -> Nothing {{\n    {dispatch}\n}}\n",
                func = test.func_name,
                module = import_module,
                dispatch = test_case_dispatch_stmt(test)
            );
            let wrapper_path = wrappers_dir.join(format!("case_{idx}_test.wr"));
            fs::write(&wrapper_path, wrapper_source)
                .map_err(|err| format!("failed to write harness case wrapper: {err}"))?;
            source.push_str(&format!("use {wrapper_func} from {wrapper_module}\n"));
            dispatch_arms.push((test.id.clone(), wrapper_func));
        }
    } else {
        let mut helpers = String::new();
        for (idx, test) in harness_tests.iter().enumerate() {
            let dispatch_func = format!("run_case_{idx}");
            let import_module = harness_import_module_path(&test.module_path);
            source.push_str(&format!(
                "use {func} from {module}\n",
                func = test.func_name,
                module = import_module
            ));
            helpers.push_str(&format!(
                "fn {dispatch_func}() -> Nothing {{\n    {dispatch}\n}}\n",
                dispatch = test_case_dispatch_stmt(test)
            ));
            dispatch_arms.push((test.id.clone(), dispatch_func));
        }
        source.push('\n');
        source.push_str(&helpers);
    }
    source.push('\n');
    source.push_str("fn run() -> Integer {\n");
    source.push_str("    selected_value = __wr_env_get(\"WRELA_TEST_ID\")\n");
    source.push_str("    mutable selected = \"\"\n");
    source.push_str("    match selected_value {\n");
    source.push_str("        String(selected_text) {\n");
    source.push_str("            selected = selected_text\n");
    source.push_str("        }\n");
    source.push_str("        default {\n");
    source.push_str("            selected = \"\"\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    for (id, dispatch_func) in &dispatch_arms {
        source.push_str(&format!("    if selected == \"{id}\" {{\n"));
        source.push_str(&format!("        {dispatch_func}()\n"));
        source.push_str("        return 0\n");
        source.push_str("    }\n");
    }
    source.push_str("    return 4\n");
    source.push_str("}\n");

    fs::write(&entry_path, source).map_err(|err| format!("failed to write test harness: {err}"))?;

    let trace = std::env::var("WRELA_BUILD_TRACE").is_ok();
    if trace {
        eprintln!(
            "build: test harness compile start ({} dispatched tests)",
            harness_tests.len()
        );
    }
    let compile_start = Instant::now();
    let mir_module = compile_to_mir_with_root(
        &entry_path,
        compile_root,
        tests_root,
        output_format,
        query_backend,
    )
    .map_err(|_| "compile failed".to_string())?;
    wrela::backend::cranelift::compile_to_executable(&mir_module, &exe_path)
        .map_err(|err| format!("codegen error: {}", err.0))?;
    let compile_ns = compile_start.elapsed().as_nanos();
    if trace {
        eprintln!(
            "build: test harness compile done ({:.2?})",
            compile_start.elapsed()
        );
    }
    if let Some(path) = wrappers_root {
        let _ = fs::remove_dir_all(path);
    }
    let meta_json = serde_json::to_vec_pretty(&expected_meta)
        .map_err(|err| format!("failed to serialize harness metadata: {err}"))?;
    fs::write(&meta_path, meta_json)
        .map_err(|err| format!("failed to write harness metadata: {err}"))?;
    let harness = TestHarness {
        exe_path,
        compile_ns,
        cache_hit: false,
    };
    if let Some(cache) = harness_cache {
        cache.insert(harness_key, harness.clone());
    }
    Ok(harness)
}

pub(crate) fn selected_test_fingerprint(tests: &[TestCase]) -> String {
    let mut hasher = Fnv1a64::new();
    for test in tests {
        hasher.update(test.id.as_bytes());
        hasher.update(&[0]);
        hasher.update(test.module_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(test.func_name.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finish_hex()
}

pub(crate) fn harness_source_fingerprint(
    compile_root: &Path,
    tests_root: Option<&Path>,
) -> Result<String, String> {
    let mut files = Vec::new();
    collect_harness_source_files(compile_root, &mut files)?;
    if let Some(root) = tests_root {
        collect_harness_source_files(root, &mut files)?;
    }
    files.sort();
    files.dedup();
    let mut hasher = Fnv1a64::new();
    for path in files {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(&[0]);
        let meta = fs::metadata(&path)
            .map_err(|err| format!("failed to stat harness source {}: {err}", path.display()))?;
        hasher.update(&meta.len().to_le_bytes());
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|stamp| stamp.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        hasher.update(modified_ns.to_string().as_bytes());
        hasher.update(&[0xff]);
    }
    Ok(hasher.finish_hex())
}

pub(crate) fn collect_harness_source_files(
    root: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    let entries =
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", root.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by_key(|a| path_sort_key(a));
    for child in children {
        if child.is_dir() {
            let skip = child
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    matches!(
                        name,
                        ".git" | "target" | "node_modules" | "wrela_harness" | "wrela_mutation"
                    )
                })
                .unwrap_or(false);
            if skip {
                continue;
            }
            collect_harness_source_files(&child, out)?;
            continue;
        }
        if child.extension().and_then(|ext| ext.to_str()) == Some("wr") {
            out.push(fs::canonicalize(&child).unwrap_or(child));
        }
    }
    Ok(())
}

pub(crate) fn test_case_dispatch_stmt(test: &TestCase) -> String {
    if let Some(call_body) = test.generated_call_body.as_ref() {
        format!("assert value ({call_body}) == true")
    } else {
        format!("{}()", test.func_name)
    }
}

pub(crate) fn harness_import_module_path(module_path: &str) -> String {
    if let Some(stripped) = module_path.strip_prefix("tests/default/") {
        format!("tests/{stripped}")
    } else {
        module_path.to_string()
    }
}

pub(crate) fn has_duplicate_test_function_names(tests: &[&TestCase]) -> bool {
    let mut names = HashSet::new();
    for test in tests {
        if !names.insert(test.func_name.clone()) {
            return true;
        }
    }
    false
}

pub(crate) fn run_single_test(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    pipeline: DifferentialPipeline,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<TestRun, String> {
    let temp_dir = harness_exe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let _ = fs::create_dir_all(&temp_dir);
    let file_stem = test.id.replace(['/', ':'], "_");
    let metrics_path = temp_dir.join(format!("{}_metrics.json", file_stem));
    let _ = fs::remove_file(&metrics_path);
    let test_temp_dir = temp_dir
        .join("cases")
        .join(sanitize_test_path_component(&test.id));
    fs::create_dir_all(&test_temp_dir)
        .map_err(|err| format!("failed to create per-test temp directory: {err}"))?;
    let runtime_start = Instant::now();
    if let Some(delay_ms) = synthetic_slowdown_ms() {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let mut extra_env_owned: Vec<(String, String)> = vec![
        ("WRELA_TEST_ID".to_string(), test.id.clone()),
        (
            "WRELA_TEST_TEMP".to_string(),
            test_temp_dir.to_string_lossy().to_string(),
        ),
        (
            "WRELA_WORKSPACE_ROOT".to_string(),
            workspace_root.to_string_lossy().to_string(),
        ),
        (
            "WRELA_HTTP_MODE".to_string(),
            http_mode.as_env_value().to_string(),
        ),
        (
            "WRELA_DIFF_PIPELINE".to_string(),
            pipeline.as_env_value().to_string(),
        ),
        ("WRELA_RUNTIME_DETERMINISTIC".to_string(), "1".to_string()),
    ];
    if test.lane == TestLane::Spec || test.lane == TestLane::Sim {
        extra_env_owned.push(("WRELA_TEST_VIRTUAL_TIME".to_string(), "1".to_string()));
        extra_env_owned.push(("WRELA_VIRTUAL_TIME_START_NS".to_string(), "0".to_string()));
    }
    if test.lane == TestLane::Spec {
        extra_env_owned.push((
            "WRELA_SPEC_FS_ROOT".to_string(),
            test_temp_dir.to_string_lossy().to_string(),
        ));
    }
    if let Some(seed) = test.sim_seed {
        let seed_value = seed.to_string();
        if test.lane == TestLane::Sim {
            extra_env_owned.push(("WRELA_SCHED_SEED".to_string(), seed_value.clone()));
        }
        if test.lane == TestLane::Model {
            extra_env_owned.push(("WRELA_MODEL_SEED".to_string(), seed_value));
        }
    }
    let extra_env: Vec<(&str, &str)> = extra_env_owned
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let local_autogen_exe = if let Some(source) = test.generated_entry_source.as_ref() {
        let entry_path = test_temp_dir.join("autogen_entry.wr");
        fs::write(&entry_path, source)
            .map_err(|err| format!("failed to write autogen test entry: {err}"))?;
        let autogen_exe = test_temp_dir.join("autogen_bin");
        let src_root = workspace_root.join("src");
        let tests_root = workspace_root.join("tests");
        let mir_module = compile_to_mir_with_root(
            &entry_path,
            &src_root,
            tests_root.is_dir().then_some(tests_root.as_path()),
            output_format,
            query_backend,
        )
        .map_err(|_| format!("autogen compile failed: {}", test.name))?;
        wrela::backend::cranelift::compile_to_executable(&mir_module, &autogen_exe)
            .map_err(|err| format!("autogen codegen error: {}", err.0))?;
        Some(autogen_exe)
    } else {
        None
    };
    let exec_path = local_autogen_exe.as_deref().unwrap_or(harness_exe_path);

    run_with_timeout(
        exec_path,
        timeout,
        Some(&metrics_path),
        Some(&test_temp_dir),
        &[],
        &extra_env,
    )?;
    let runtime_ns = runtime_start.elapsed().as_nanos();
    let metrics = read_metrics_dump(&metrics_path);
    Ok(TestRun {
        metrics,
        runtime_ns,
    })
}

pub(crate) fn run_single_test_with_timeout_retry(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    pipeline: DifferentialPipeline,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<TestRun, String> {
    // Keep retries for replay/normal execution, but use a single bounded
    // record-mode attempt so cassette capture does not become non-deterministic
    // under timeout retries.
    let multipliers: &[u32] = if matches!(http_mode, HttpCassetteMode::Record) {
        &[4]
    } else {
        &[1, 2, 4]
    };
    for multiplier in multipliers {
        let attempt_timeout = timeout.checked_mul(*multiplier).unwrap_or(timeout);
        match run_single_test(
            harness_exe_path,
            workspace_root,
            test,
            attempt_timeout,
            output_format,
            http_mode,
            pipeline,
            query_backend,
        ) {
            Ok(run) => return Ok(run),
            Err(err) if err == "timeout" => continue,
            Err(err) => return Err(err),
        }
    }
    Err("timeout".to_string())
}

pub(crate) fn execute_test_case(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    pipeline: DifferentialPipeline,
    certify_mode: bool,
    query_backend: wrela::query_plan::DispatchBackend,
) -> (bool, String, Option<TestRun>) {
    let result = run_single_test_with_timeout_retry(
        harness_exe_path,
        workspace_root,
        test,
        timeout,
        output_format,
        http_mode,
        pipeline,
        query_backend,
    );
    match result {
        Ok(run) => (true, String::new(), Some(run)),
        Err(msg) => {
            let mut detail = String::new();
            let mut failure_msg = msg;
            if (test.lane == TestLane::Sim || test.lane == TestLane::Model)
                && let Some(seed) = test.sim_seed
            {
                if certify_mode && test.lane == TestLane::Sim {
                    let mut replay_ok = 0usize;
                    for _ in 0..3 {
                        if run_single_test(
                            harness_exe_path,
                            workspace_root,
                            test,
                            timeout,
                            output_format,
                            http_mode,
                            pipeline,
                            query_backend,
                        )
                        .is_ok()
                        {
                            replay_ok += 1;
                        }
                    }
                    if replay_ok > 0 {
                        failure_msg.push_str(&format!(
                                " | determinism confirmation failed: {replay_ok}/3 reruns passed unexpectedly"
                            ));
                    }
                }
                let replay_hint = format!(
                    "wrela test --lane={} --seed={seed} --id={} .",
                    test.lane.as_str(),
                    test.canonical_id
                );
                detail.push_str(&format!(" replay=`{replay_hint}`"));
                let trace_path = if test.lane == TestLane::Sim {
                    write_sim_trace_artifact(workspace_root, test, &failure_msg)
                } else {
                    write_model_trace_artifact(workspace_root, test, &failure_msg)
                };
                if let Ok(path) = trace_path {
                    detail.push_str(&format!(" trace={}", path.display()));
                }
            }
            if let Some(call) = test.generated_call_body.as_ref() {
                match test.generated_case_kind {
                    Some(GeneratedCaseKind::Autogen) => {
                        detail.push_str(&format!(
                            " | autogen failure: check={}::{} seed={} span={} call=`{}`",
                            test.module_path,
                            test.func_name,
                            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
                            test.autogen_span.as_deref().unwrap_or("unknown"),
                            call
                        ));
                        match repro::write_autogen_repro_artifact(
                            workspace_root,
                            harness_exe_path,
                            test,
                            timeout,
                            output_format,
                            http_mode,
                            &failure_msg,
                        ) {
                            Ok((path, shrunk_call)) => {
                                if let Some(shrunk) = shrunk_call {
                                    detail.push_str(&format!(" shrunk_call=`{shrunk}`"));
                                }
                                detail.push_str(&format!(" repro={}", path.display()));
                            }
                            Err(err) => {
                                detail
                                    .push_str(&format!(" repro_error={}", err.replace('\n', " ")));
                            }
                        }
                    }
                    Some(GeneratedCaseKind::Fuzz) => {
                        detail.push_str(&format!(
                            " | fuzz failure: target={}::{} seed={} span={} call=`{}`",
                            test.module_path,
                            test.func_name,
                            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
                            test.autogen_span.as_deref().unwrap_or("unknown"),
                            call
                        ));
                        match repro::write_fuzz_repro_artifact(workspace_root, test, &failure_msg) {
                            Ok(path) => {
                                detail.push_str(&format!(" repro={}", path.display()));
                            }
                            Err(err) => {
                                detail
                                    .push_str(&format!(" repro_error={}", err.replace('\n', " ")));
                            }
                        }
                    }
                    None => {}
                }
            }
            (false, format!("{failure_msg}{detail}"), None)
        }
    }
}

pub(crate) fn write_sim_trace_artifact(
    workspace_root: &Path,
    test: &TestCase,
    failure: &str,
) -> Result<PathBuf, String> {
    let seed = test.sim_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let input = replay_trace::ReplayTraceInput {
        test_id: &test.id,
        canonical_test_id: &test.canonical_id,
        lane: test.lane.as_str(),
        seed,
        failure,
    };
    replay_trace::write_failure_trace_artifact(
        workspace_root,
        "sim",
        &sanitize_test_path_component(&test.canonical_id),
        now_unix_ms(),
        &input,
    )
}

pub(crate) fn write_model_trace_artifact(
    workspace_root: &Path,
    test: &TestCase,
    failure: &str,
) -> Result<PathBuf, String> {
    let seed = test.sim_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let input = replay_trace::ReplayTraceInput {
        test_id: &test.id,
        canonical_test_id: &test.canonical_id,
        lane: test.lane.as_str(),
        seed,
        failure,
    };
    replay_trace::write_failure_trace_artifact(
        workspace_root,
        "model",
        &sanitize_test_path_component(&test.canonical_id),
        now_unix_ms(),
        &input,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutogenValue {
    Integer(i64),
    Boolean(bool),
    String(String),
    List(Vec<AutogenValue>),
    Raw(String),
}

pub(crate) fn shrink_autogen_call(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
) -> Option<String> {
    let call = test.generated_call_body.as_ref()?;
    let (func_name, mut args) = parse_autogen_call(call)?;
    if args.is_empty() {
        return None;
    }
    let mut changed = false;
    let mut attempts = 0usize;
    for idx in 0..args.len() {
        loop {
            if attempts >= 128 {
                break;
            }
            let candidates = shrink_value_candidates(&args[idx].1);
            let mut improved = false;
            for candidate in candidates {
                if candidate == args[idx].1 {
                    continue;
                }
                let mut trial_args = args.clone();
                trial_args[idx].1 = candidate;
                let trial_call = render_autogen_call(&func_name, &trial_args);
                if autogen_call_still_fails(
                    harness_exe_path,
                    workspace_root,
                    test,
                    timeout,
                    output_format,
                    http_mode,
                    &trial_call,
                    wrela::query_plan::DispatchBackend::Auto,
                ) {
                    args = trial_args;
                    changed = true;
                    improved = true;
                    attempts += 1;
                    break;
                }
                attempts += 1;
                if attempts >= 128 {
                    break;
                }
            }
            if !improved {
                break;
            }
        }
    }
    if changed {
        Some(render_autogen_call(&func_name, &args))
    } else {
        None
    }
}

pub(crate) fn autogen_call_still_fails(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    candidate_call: &str,
    query_backend: wrela::query_plan::DispatchBackend,
) -> bool {
    let Some(candidate_test) = autogen_test_with_call(test, candidate_call) else {
        return false;
    };
    run_single_test(
        harness_exe_path,
        workspace_root,
        &candidate_test,
        timeout,
        output_format,
        http_mode,
        DifferentialPipeline::Baseline,
        query_backend,
    )
    .is_err()
}

pub(crate) fn autogen_test_with_call(test: &TestCase, call_body: &str) -> Option<TestCase> {
    let module_source = test.autogen_module_source.as_ref()?;
    let mut candidate = test.clone();
    candidate.generated_call_body = Some(call_body.to_string());
    candidate.generated_entry_source =
        Some(autogen_standalone_entry_source(module_source, call_body));
    Some(candidate)
}

pub(crate) fn parse_autogen_call(call: &str) -> Option<(String, Vec<(String, AutogenValue)>)> {
    let trimmed = call.trim();
    let lparen = trimmed.find('(')?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let func_name = trimmed[..lparen].trim().to_string();
    if func_name.is_empty() {
        return None;
    }
    let args_raw = &trimmed[lparen + 1..trimmed.len() - 1];
    if args_raw.trim().is_empty() {
        return Some((func_name, Vec::new()));
    }
    let mut args = Vec::new();
    for chunk in split_top_level(args_raw, ',') {
        let trimmed = chunk.trim();
        let (name, value_raw) = trimmed.split_once('=')?;
        let value_raw = value_raw.trim();
        args.push((name.trim().to_string(), parse_autogen_value(value_raw)));
    }
    Some((func_name, args))
}

pub(crate) fn parse_autogen_value(raw: &str) -> AutogenValue {
    let trimmed = raw.trim();
    if trimmed == "true" {
        return AutogenValue::Boolean(true);
    }
    if trimmed == "false" {
        return AutogenValue::Boolean(false);
    }
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return AutogenValue::String(trimmed[1..trimmed.len() - 1].to_string());
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.trim().is_empty() {
            return AutogenValue::List(Vec::new());
        }
        let elements = split_top_level(inner, ',')
            .into_iter()
            .map(|part| parse_autogen_value(part.trim()))
            .collect();
        return AutogenValue::List(elements);
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return AutogenValue::Integer(value);
    }
    AutogenValue::Raw(trimmed.to_string())
}

pub(crate) fn render_autogen_call(func_name: &str, args: &[(String, AutogenValue)]) -> String {
    if args.is_empty() {
        return format!("{func_name}()");
    }
    let rendered_args = args
        .iter()
        .map(|(name, value)| format!("{name}={}", render_autogen_value(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{func_name}({rendered_args})")
}

pub(crate) fn render_autogen_value(value: &AutogenValue) -> String {
    match value {
        AutogenValue::Integer(v) => v.to_string(),
        AutogenValue::Boolean(v) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        AutogenValue::String(v) => format!("\"{}\"", v.replace('\"', "\\\"")),
        AutogenValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(render_autogen_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AutogenValue::Raw(v) => v.clone(),
    }
}

pub(crate) fn shrink_value_candidates(value: &AutogenValue) -> Vec<AutogenValue> {
    let mut candidates = Vec::new();
    match value {
        AutogenValue::Integer(v) => {
            if *v != 0 {
                candidates.push(AutogenValue::Integer(0));
                let half = v / 2;
                if half != *v && half != 0 {
                    candidates.push(AutogenValue::Integer(half));
                }
                if *v > 1 {
                    candidates.push(AutogenValue::Integer(1));
                } else if *v < -1 {
                    candidates.push(AutogenValue::Integer(-1));
                }
            }
        }
        AutogenValue::String(v) => {
            if !v.is_empty() {
                candidates.push(AutogenValue::String(String::new()));
                let half_len = v.chars().count() / 2;
                if half_len > 0 {
                    let shorter = v.chars().take(half_len).collect::<String>();
                    if shorter.len() < v.len() {
                        candidates.push(AutogenValue::String(shorter));
                    }
                }
            }
        }
        AutogenValue::List(items) => {
            if !items.is_empty() {
                candidates.push(AutogenValue::List(Vec::new()));
                if items.len() > 1 {
                    candidates.push(AutogenValue::List(items[..items.len() / 2].to_vec()));
                }
                candidates.push(AutogenValue::List(items[..items.len() - 1].to_vec()));
            }
        }
        AutogenValue::Boolean(true) => {
            candidates.push(AutogenValue::Boolean(false));
        }
        AutogenValue::Boolean(false) | AutogenValue::Raw(_) => {}
    }
    dedupe_autogen_values(candidates)
}

pub(crate) fn dedupe_autogen_values(values: Vec<AutogenValue>) -> Vec<AutogenValue> {
    let mut out = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

pub(crate) fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        match ch {
            '"' => {
                let escaped = idx > 0 && bytes[idx - 1] == b'\\';
                if !escaped {
                    in_string = !in_string;
                }
            }
            '[' if !in_string => depth = depth.saturating_add(1),
            ']' if !in_string && depth > 0 => depth -= 1,
            _ if ch == delimiter && !in_string && depth == 0 => {
                parts.push(input[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
        idx += ch.len_utf8();
    }
    parts.push(input[start..].to_string());
    parts
}

pub(crate) fn synthetic_slowdown_ms() -> Option<u64> {
    let raw = env::var("WRELA_TEST_SLOWDOWN_MS").ok()?;
    raw.parse::<u64>().ok()
}

pub(crate) fn sanitize_test_path_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "test".to_string()
    } else {
        out
    }
}

pub(crate) fn inherited_test_env_keys() -> &'static [&'static str] {
    &[
        "PATH",
        "HOME",
        "USERPROFILE",
        "TMPDIR",
        "TMP",
        "TEMP",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "TZ",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ]
}

pub(crate) fn read_metrics_dump(path: &Path) -> Option<MetricsDump> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub(crate) fn write_function_coverage_snapshot(
    path: &Path,
    snapshot: &BTreeMap<String, u64>,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct FunctionCoverageSnapshotArtifact<'a> {
        schema_version: u32,
        function_coverage: &'a BTreeMap<String, u64>,
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec(&FunctionCoverageSnapshotArtifact {
        schema_version: COVERAGE_SNAPSHOT_SCHEMA_VERSION,
        function_coverage: snapshot,
    })
    .map_err(|err| {
        format!(
            "failed to serialize function coverage snapshot {}: {}",
            path.display(),
            err
        )
    })?;
    fs::write(path, payload).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

pub(crate) fn load_function_coverage_snapshot(
    path: &Path,
) -> Result<BTreeMap<String, u64>, String> {
    #[derive(Deserialize)]
    struct FunctionCoverageSnapshotArtifact {
        schema_version: u32,
        function_coverage: BTreeMap<String, u64>,
    }
    let payload =
        fs::read(path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let artifact: FunctionCoverageSnapshotArtifact = serde_json::from_slice(&payload)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))?;
    if artifact.schema_version != COVERAGE_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "stale function coverage snapshot schema in {}: expected {}, got {}",
            path.display(),
            COVERAGE_SNAPSHOT_SCHEMA_VERSION,
            artifact.schema_version
        ));
    }
    Ok(artifact.function_coverage)
}

pub(crate) fn certification_coverage_index_path(
    workspace_root: &Path,
    cert_cache_hash: &str,
) -> PathBuf {
    workspace_root
        .join("target")
        .join("wrela_cert")
        .join("index")
        .join(format!("{cert_cache_hash}.json"))
}

pub(crate) fn write_function_test_coverage_index(
    path: &Path,
    index: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct FunctionCoverageIndexArtifact<'a> {
        schema_version: u32,
        function_to_tests: &'a BTreeMap<String, Vec<String>>,
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec(&FunctionCoverageIndexArtifact {
        schema_version: COVERAGE_INDEX_SCHEMA_VERSION,
        function_to_tests: index,
    })
    .map_err(|err| {
        format!(
            "failed to serialize function test coverage index {}: {}",
            path.display(),
            err
        )
    })?;
    fs::write(path, payload).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

pub(crate) fn load_function_test_coverage_index(
    workspace_root: &Path,
    cert_cache_hash: &str,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    #[derive(Deserialize)]
    struct FunctionCoverageIndexArtifact {
        schema_version: u32,
        function_to_tests: BTreeMap<String, Vec<String>>,
    }
    let path = certification_coverage_index_path(workspace_root, cert_cache_hash);
    let payload =
        fs::read(&path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let artifact: FunctionCoverageIndexArtifact = serde_json::from_slice(&payload)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))?;
    if artifact.schema_version != COVERAGE_INDEX_SCHEMA_VERSION {
        return Err(format!(
            "stale function coverage index schema in {}: expected {}, got {}",
            path.display(),
            COVERAGE_INDEX_SCHEMA_VERSION,
            artifact.schema_version
        ));
    }
    Ok(artifact.function_to_tests)
}

pub(crate) fn build_function_test_coverage_index(
    summary: Option<&PerfSummary>,
) -> BTreeMap<String, Vec<String>> {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(cases) = summary.and_then(|value| value.cases.as_ref()) else {
        return BTreeMap::new();
    };
    for case in cases {
        let test_id = if case.id.is_empty() {
            match case.name.rsplit_once("::") {
                Some((module_path, func_name)) => stable_test_id(module_path, func_name),
                None => continue,
            }
        } else {
            case.id.clone()
        };
        let Some(metrics) = case.metrics.as_ref() else {
            continue;
        };
        for (function_id, hits) in &metrics.function_coverage {
            if *hits == 0 {
                continue;
            }
            let canonical_function_id = normalize_function_coverage_key(function_id);
            grouped
                .entry(canonical_function_id)
                .or_default()
                .insert(test_id.clone());
        }
    }
    grouped
        .into_iter()
        .map(|(function_id, test_ids)| (function_id, test_ids.into_iter().collect()))
        .collect()
}

pub(crate) fn canonicalize_function_coverage(
    function_coverage: &BTreeMap<String, u64>,
) -> BTreeMap<String, u64> {
    let mut normalized: BTreeMap<String, u64> = BTreeMap::new();
    for (function_id, hits) in function_coverage {
        let canonical_function_id = normalize_function_coverage_key(function_id);
        normalized
            .entry(canonical_function_id)
            .and_modify(|current| *current = (*current).max(*hits))
            .or_insert(*hits);
    }
    normalized
}

pub(crate) fn normalize_function_coverage_key(function_id: &str) -> String {
    if function_id.chars().all(|ch| ch.is_ascii_digit()) {
        return function_id.to_string();
    }
    if function_id.contains("::") {
        return stable_function_id(function_id);
    }
    function_id.to_string()
}

pub(crate) fn run_mutation_gate(
    workspace_root: &Path,
    summary: &PerfSummary,
    max_cases: usize,
    time_cap_ms: u64,
) -> Result<MutationGateOutcome, String> {
    if max_cases == 0 {
        return Ok(MutationGateOutcome {
            summary_hash: None,
            discovery_ms: 0,
            execution_ms: 0,
        });
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let discovery_start = Instant::now();
    let coverage_index = build_function_test_coverage_index(Some(summary));
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let authored_tests = discover_authored_tests_for_mutation(workspace_root)?;
    let mut importable_by_module: BTreeMap<String, BTreeMap<String, ImportableFunctionInfo>> =
        BTreeMap::new();
    for item in snapshot
        .items
        .iter()
        .filter(|item| is_importable_coverage_target(&item.qualified_name))
    {
        let Some((module_path, function_name)) = item.qualified_name.rsplit_once("::") else {
            continue;
        };
        let function_id = stable_function_id(&item.qualified_name);
        importable_by_module
            .entry(module_path.to_string())
            .or_default()
            .insert(
                function_name.to_string(),
                ImportableFunctionInfo {
                    qualified_name: item.qualified_name.clone(),
                    function_id,
                },
            );
    }
    let mir_module = compile_mutation_discovery_module(workspace_root, &importable_by_module)?;
    let mut candidates = Vec::new();
    let mut seen_candidates = BTreeSet::new();
    for functions in importable_by_module.values() {
        for candidate in discover_mir_mutation_candidates(&mir_module, functions) {
            let key = mutation_candidate_key(&candidate);
            if seen_candidates.insert(key) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then(a.function_name.cmp(&b.function_name))
            .then(a.op_index.cmp(&b.op_index))
            .then(a.mutation_type.cmp(b.mutation_type))
    });
    let discovery_ms = discovery_start.elapsed().as_millis();
    let authored_by_id: HashMap<String, TestCase> = authored_tests
        .into_iter()
        .map(|test| (test.id.clone(), test))
        .collect();
    let source_hash = hash_source_fingerprint(workspace_root)
        .map_err(|err| format!("mutation cache source hash error: {err}"))?;
    let toolchain_version = resolve_toolchain_version();
    let cache_enabled = mutation_cache_enabled();
    let cache_root = mutation_cache_root(workspace_root);
    if cache_enabled {
        let _ = fs::create_dir_all(&cache_root);
    }
    let history_path = mutation_kill_history_path(&cache_root);
    let mut history = load_mutation_kill_history(&history_path);

    let execution_start = Instant::now();
    let mut queued_jobs = Vec::new();
    let mut ordered_results = Vec::new();
    let mut total = 0usize;
    let mutation_cap = max_cases.min(candidates.len());
    for (job_index, candidate) in candidates.into_iter().take(mutation_cap).enumerate() {
        if started.elapsed() >= time_cap {
            break;
        }
        total += 1;
        let selected_ids = select_covering_test_ids_for_mutation(&coverage_index, &candidate);
        let tests_to_run: Vec<TestCase> = selected_ids
            .iter()
            .filter_map(|id| authored_by_id.get(id).cloned())
            .collect();
        if tests_to_run.is_empty() {
            ordered_results.push((
                job_index,
                MutationMutantResult {
                    function: candidate.qualified_name.clone(),
                    function_id: candidate.function_id.clone(),
                    mutation_type: candidate.mutation_type.to_string(),
                    tests_ran: Vec::new(),
                    compile_ms: 0,
                    test_run_ms: 0,
                    status: "survived".to_string(),
                    reason: Some("no-covering-tests".to_string()),
                },
                0usize,
                0usize,
                0usize,
            ));
            continue;
        }
        let ordered_tests = order_tests_for_mutation_candidate(&candidate, tests_to_run, &history);
        queued_jobs.push(MutationCandidateJob {
            job_index,
            candidate,
            tests_to_run: ordered_tests,
        });
    }

    let mutation_workers = resolve_mutation_workers();
    let worker_count = mutation_workers.min(queued_jobs.len().max(1));
    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        queued_jobs,
    )));
    let (tx, rx) = std::sync::mpsc::channel::<MutationExecutionResult>();
    let context = std::sync::Arc::new(MutationExecutionContext {
        workspace_root: workspace_root.to_path_buf(),
        source_hash,
        toolchain_version,
        cache_root,
        cache_enabled,
    });
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let worker_context = std::sync::Arc::clone(&context);
        handles.push(std::thread::spawn(move || {
            loop {
                let next_job = match queue.lock() {
                    Ok(mut guard) => guard.pop_front(),
                    Err(_) => None,
                };
                let Some(job) = next_job else { break };
                let (mutant, cache_hits, cache_misses, cache_invalidations) =
                    run_mutation_job(&worker_context, &job);
                let _ = tx.send(MutationExecutionResult {
                    job_index: job.job_index,
                    mutant,
                    cache_hits,
                    cache_misses,
                    cache_invalidations,
                });
            }
        }));
    }
    drop(tx);
    for result in rx {
        ordered_results.push((
            result.job_index,
            result.mutant,
            result.cache_hits,
            result.cache_misses,
            result.cache_invalidations,
        ));
    }
    for handle in handles {
        if handle.join().is_err() {
            return Err(
                "mutation gate worker panic: mutation execution aborted before report completion"
                    .to_string(),
            );
        }
    }
    ordered_results.sort_by_key(|(job_index, _, _, _, _)| *job_index);
    let cache_hits: usize = ordered_results.iter().map(|(_, _, hits, _, _)| *hits).sum();
    let cache_misses: usize = ordered_results
        .iter()
        .map(|(_, _, _, misses, _)| *misses)
        .sum();
    let cache_invalidations: usize = ordered_results
        .iter()
        .map(|(_, _, _, _, invalidations)| *invalidations)
        .sum();
    let mutants: Vec<MutationMutantResult> = ordered_results
        .into_iter()
        .map(|(_, mutant, _, _, _)| mutant)
        .collect();
    update_mutation_kill_history_from_mutants(&mut history, &mutants);
    let _ = write_mutation_kill_history(&history_path, &history);

    let invalid = mutants
        .iter()
        .filter(|mutant| mutant.status == "invalid-mutant")
        .count();
    let survived = mutants
        .iter()
        .filter(|mutant| mutant.status == "survived")
        .count();
    let killed = mutants
        .iter()
        .filter(|mutant| mutant.status == "killed")
        .count();
    let no_covering = mutants
        .iter()
        .filter(|mutant| mutant.reason.as_deref() == Some("no-covering-tests"))
        .count();
    let valid = killed + survived;
    let kill_rate_pct = if valid == 0 {
        100.0
    } else {
        (killed as f64 / valid as f64) * 100.0
    };
    let domain_kill_rate_pct = if valid == 0 {
        None
    } else {
        Some(kill_rate_pct)
    };

    let report = MutationGateReport {
        version: 4,
        generated_at_unix_ms: now_unix_ms(),
        discovery_ms,
        execution_ms: execution_start.elapsed().as_millis(),
        compile_total_ms: mutants.iter().map(|mutant| mutant.compile_ms).sum(),
        test_run_total_ms: mutants.iter().map(|mutant| mutant.test_run_ms).sum(),
        parallel_workers: worker_count.max(1),
        cache_hits,
        cache_misses,
        cache_invalidations,
        total_mutants: total,
        valid_mutants: valid,
        invalid_mutants: invalid,
        killed_mutants: killed,
        survived_mutants: survived,
        no_covering_tests_mutants: no_covering,
        kill_rate_pct,
        domain_application_kill_rate_pct: domain_kill_rate_pct,
        mutants,
    };
    let report_path = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec_pretty(&report)
        .map_err(|err| format!("failed to serialize mutation report: {err}"))?;
    fs::write(&report_path, &payload).map_err(|err| {
        format!(
            "failed to write mutation report {}: {}",
            report_path.display(),
            err
        )
    })?;
    let summary_hash = fnv1a64_hex(&payload);
    let execution_ms = report.execution_ms;

    let mut failures = Vec::new();
    if report.survived_mutants > 0 {
        let survivors = report
            .mutants
            .iter()
            .filter(|mutant| mutant.status == "survived")
            .map(|mutant| {
                let reason = mutant.reason.as_deref().unwrap_or("tests-passed");
                format!("  - {} [{}]", mutant.function, reason)
            })
            .collect::<Vec<_>>()
            .join("\n");
        failures.push(format!(
            "mutation gate failed: {} survived mutants detected under src/domain/** and src/application/**.\nsurvivors:\n{}\naction: add assertions/tests that kill these mutants",
            report.survived_mutants,
            survivors
        ));
    }
    if let Some(rate) = report.domain_application_kill_rate_pct
        && rate < 85.0
    {
        failures.push(format!(
            "domain/application mutation kill rate {:.2}% is below required 85.00%",
            rate
        ));
    }
    if !failures.is_empty() {
        return Err(format!(
            "{}\nmutation report: {}",
            failures.join("\n"),
            report_path.display()
        ));
    }
    Ok(MutationGateOutcome {
        summary_hash: Some(summary_hash),
        discovery_ms,
        execution_ms,
    })
}

pub(crate) fn compile_mutation_discovery_module(
    workspace_root: &Path,
    importable_by_module: &BTreeMap<String, BTreeMap<String, ImportableFunctionInfo>>,
) -> Result<mir::ir::MirModule, String> {
    let mut source = String::new();
    for (module_path, functions) in importable_by_module {
        if functions.is_empty() {
            continue;
        }
        let imports = functions.keys().cloned().collect::<Vec<_>>().join(", ");
        source.push_str(&format!("use {imports} from {module_path}\n"));
    }
    source.push_str("\nfn run() -> Integer {\n    return 0\n}\n");

    let discovery_root = workspace_root
        .join("target")
        .join("wrela_mutation")
        .join("discovery");
    fs::create_dir_all(&discovery_root)
        .map_err(|err| format!("failed to create {}: {}", discovery_root.display(), err))?;
    let entry_path = discovery_root.join(format!("project_{}.wr", fnv1a64_hex(source.as_bytes())));
    fs::write(&entry_path, source).map_err(|err| {
        format!(
            "mutation gate failed to write discovery entry {}: {}",
            entry_path.display(),
            err
        )
    })?;

    let src_root = workspace_root.join("src");
    let tests_root = workspace_root.join("tests");
    compile_to_mir_with_root(
        &entry_path,
        &src_root,
        tests_root.is_dir().then_some(tests_root.as_path()),
        OutputFormat::Pretty,
        wrela::query_plan::DispatchBackend::Auto,
    )
    .map_err(|code| {
        format!(
            "mutation gate failed to compile MIR discovery entry {} (exit code {code})",
            entry_path.display()
        )
    })
}

#[derive(Clone, Copy)]
pub(crate) enum MutationSite {
    Branch { block_idx: usize },
    Comparison { block_idx: usize, stmt_idx: usize },
    IntegerLiteralUse { block_idx: usize, stmt_idx: usize },
    IntegerLiteralBinaryLhs { block_idx: usize, stmt_idx: usize },
    IntegerLiteralBinaryRhs { block_idx: usize, stmt_idx: usize },
    ResultGuard { block_idx: usize, stmt_idx: usize },
}

#[derive(Clone)]
pub(crate) struct MirMutationCandidate {
    pub(crate) qualified_name: String,
    pub(crate) function_name: String,
    pub(crate) function_id: String,
    pub(crate) mutation_type: &'static str,
    pub(crate) op_index: usize,
    pub(crate) site: MutationSite,
}

#[derive(Clone)]
pub(crate) struct ImportableFunctionInfo {
    pub(crate) qualified_name: String,
    pub(crate) function_id: String,
}

pub(crate) fn discover_mir_mutation_candidates(
    module: &mir::ir::MirModule,
    importable_functions: &BTreeMap<String, ImportableFunctionInfo>,
) -> Vec<MirMutationCandidate> {
    let mut candidates = Vec::new();
    for function in &module.functions {
        let function_name = function.name.to_string();
        let Some(importable) = importable_functions.get(&function_name) else {
            continue;
        };
        let mut op_index = 0usize;
        for (block_idx, block) in function.blocks.iter().enumerate() {
            if let mir::ir::Terminator::Branch { .. } = block.terminator {
                candidates.push(MirMutationCandidate {
                    qualified_name: importable.qualified_name.clone(),
                    function_name: function_name.clone(),
                    function_id: importable.function_id.clone(),
                    mutation_type: "conditional_branch_inversion",
                    op_index,
                    site: MutationSite::Branch { block_idx },
                });
                op_index += 1;
            }
            for (stmt_idx, stmt) in block.stmts.iter().enumerate() {
                let mir::ir::Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                match value {
                    mir::ir::Rvalue::Binary { op, lhs, rhs } => {
                        if invertible_comparison(*op).is_some() {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "comparison_inversion",
                                op_index,
                                site: MutationSite::Comparison {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                        if matches!(lhs, mir::ir::Value::Const(hir::Literal::Integer(_))) {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "integer_literal_perturbation",
                                op_index,
                                site: MutationSite::IntegerLiteralBinaryLhs {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                        if matches!(rhs, mir::ir::Value::Const(hir::Literal::Integer(_))) {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "integer_literal_perturbation",
                                op_index,
                                site: MutationSite::IntegerLiteralBinaryRhs {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                    }
                    mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Integer(_))) => {
                        candidates.push(MirMutationCandidate {
                            qualified_name: importable.qualified_name.clone(),
                            function_name: function_name.clone(),
                            function_id: importable.function_id.clone(),
                            mutation_type: "integer_literal_perturbation",
                            op_index,
                            site: MutationSite::IntegerLiteralUse {
                                block_idx,
                                stmt_idx,
                            },
                        });
                        op_index += 1;
                    }
                    mir::ir::Rvalue::ResultIsOk { .. } => {
                        candidates.push(MirMutationCandidate {
                            qualified_name: importable.qualified_name.clone(),
                            function_name: function_name.clone(),
                            function_id: importable.function_id.clone(),
                            mutation_type: "result_guard_perturbation",
                            op_index,
                            site: MutationSite::ResultGuard {
                                block_idx,
                                stmt_idx,
                            },
                        });
                        op_index += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    candidates
}

pub(crate) fn discover_authored_tests_for_mutation(
    workspace_root: &Path,
) -> Result<Vec<TestCase>, String> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut discovered = Vec::new();
    collect_tests(&tests_root, &tests_root, &mut discovered).map_err(|err| err.to_string())?;
    discovered.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(expand_sim_seed_cases(discovered, None, true))
}

pub(crate) fn mutation_candidate_key(candidate: &MirMutationCandidate) -> String {
    let site = match candidate.site {
        MutationSite::Branch { block_idx } => format!("branch:{block_idx}"),
        MutationSite::Comparison {
            block_idx,
            stmt_idx,
        } => {
            format!("comparison:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralUse {
            block_idx,
            stmt_idx,
        } => {
            format!("int_use:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralBinaryLhs {
            block_idx,
            stmt_idx,
        } => {
            format!("int_lhs:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralBinaryRhs {
            block_idx,
            stmt_idx,
        } => {
            format!("int_rhs:{block_idx}:{stmt_idx}")
        }
        MutationSite::ResultGuard {
            block_idx,
            stmt_idx,
        } => {
            format!("result_guard:{block_idx}:{stmt_idx}")
        }
    };
    format!(
        "{}|{}|{}|{}",
        candidate.qualified_name, candidate.function_name, candidate.mutation_type, site
    )
}

pub(crate) fn select_covering_test_ids_for_mutation(
    coverage_index: &BTreeMap<String, Vec<String>>,
    candidate: &MirMutationCandidate,
) -> Vec<String> {
    let mut selected = BTreeSet::new();
    if let Some(test_ids) = coverage_index.get(&candidate.function_id) {
        for test_id in test_ids {
            selected.insert(test_id.clone());
        }
    }
    let short_name_id = stable_function_id(&candidate.function_name);
    if let Some(test_ids) = coverage_index.get(&short_name_id) {
        for test_id in test_ids {
            selected.insert(test_id.clone());
        }
    }
    selected.into_iter().collect()
}

pub(crate) fn resolve_mutation_workers() -> usize {
    const DEFAULT_WORKER_CAP: usize = 4;
    const ABSOLUTE_WORKER_CAP: usize = 16;
    let default_workers = std::thread::available_parallelism()
        .map(|value| value.get().min(DEFAULT_WORKER_CAP))
        .unwrap_or(1);
    let requested = std::env::var("WRELA_MUTATION_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default_workers);
    requested.clamp(1, ABSOLUTE_WORKER_CAP)
}

pub(crate) fn run_mutation_job(
    context: &MutationExecutionContext,
    job: &MutationCandidateJob,
) -> (MutationMutantResult, usize, usize, usize) {
    let compile_attempt_start = Instant::now();
    let mut tests_ran = Vec::new();
    let compile_result =
        compile_mutant_binary_for_tests(context, &job.candidate, &job.tests_to_run);
    let compile = match compile_result {
        Ok(outcome) => outcome,
        Err(failure) => {
            if context.cache_enabled {
                let _ = persist_invalid_mutation_cache_entry(
                    context,
                    &job.candidate,
                    &failure.reason,
                    failure.compile_ms,
                );
            }
            return (
                MutationMutantResult {
                    function: job.candidate.qualified_name.clone(),
                    function_id: job.candidate.function_id.clone(),
                    mutation_type: job.candidate.mutation_type.to_string(),
                    tests_ran,
                    compile_ms: failure
                        .compile_ms
                        .max(compile_attempt_start.elapsed().as_millis()),
                    test_run_ms: 0,
                    status: "invalid-mutant".to_string(),
                    reason: Some(failure.reason),
                },
                failure.cache_hits,
                failure.cache_misses,
                failure.cache_invalidations,
            );
        }
    };

    let timeout = Duration::from_millis(DEFAULT_TEST_TIMEOUT_MS);
    let run_start = Instant::now();
    let mut killed = false;
    for test in &job.tests_to_run {
        tests_ran.push(test.id.clone());
        let run = run_single_test_with_timeout_retry(
            &compile.exe_path,
            &context.workspace_root,
            test,
            timeout,
            OutputFormat::Pretty,
            HttpCassetteMode::Replay,
            DifferentialPipeline::Baseline,
            wrela::query_plan::DispatchBackend::Auto,
        );
        if run.is_err() {
            killed = true;
            break;
        }
    }
    let test_run_ms = run_start.elapsed().as_millis();
    (
        MutationMutantResult {
            function: job.candidate.qualified_name.clone(),
            function_id: job.candidate.function_id.clone(),
            mutation_type: job.candidate.mutation_type.to_string(),
            tests_ran,
            compile_ms: compile.compile_ms,
            test_run_ms,
            status: if killed {
                "killed".to_string()
            } else {
                "survived".to_string()
            },
            reason: None,
        },
        compile.cache_hits,
        compile.cache_misses,
        compile.cache_invalidations,
    )
}

pub(crate) fn compile_mutant_binary_for_tests(
    context: &MutationExecutionContext,
    candidate: &MirMutationCandidate,
    tests: &[TestCase],
) -> Result<MutantCompileSuccess, MutantCompileFailure> {
    let candidate_key = mutation_candidate_key(candidate);
    let cache_key = mutation_cache_key(
        &context.source_hash,
        &context.toolchain_version,
        &candidate_key,
    );
    let cache_entry_dir = context.cache_root.join(&cache_key);
    let cache_metadata_path = cache_entry_dir.join("metadata.json");
    let cache_bin_path = cache_entry_dir.join("mutant_bin");
    let mut cache_invalidations = 0usize;
    if context.cache_enabled
        && let Some(metadata) = load_mutation_cache_metadata(&cache_metadata_path)
    {
        let valid_metadata = metadata.schema_version == MUTATION_CACHE_SCHEMA_VERSION
            && metadata.toolchain_version == context.toolchain_version
            && metadata.source_hash == context.source_hash
            && metadata.candidate_key == candidate_key;
        if valid_metadata && metadata.build_status == "ok" && cache_bin_path.is_file() {
            return Ok(MutantCompileSuccess {
                exe_path: cache_bin_path,
                compile_ms: 0,
                cache_hits: 1,
                cache_misses: 0,
                cache_invalidations: 0,
            });
        }
        if valid_metadata && metadata.build_status == "invalid" {
            return Err(MutantCompileFailure {
                reason: metadata
                    .invalid_reason
                    .unwrap_or_else(|| "cached invalid mutant".to_string()),
                compile_ms: 0,
                cache_hits: 1,
                cache_misses: 0,
                cache_invalidations: 0,
            });
        }
        cache_invalidations += 1;
        let _ = fs::remove_dir_all(&cache_entry_dir);
    }

    let mutation_key = sanitize_test_path_component(&format!(
        "{}__{}__{}",
        candidate.function_name, candidate.mutation_type, candidate.op_index
    ));
    let mutation_root = context
        .workspace_root
        .join("target")
        .join("wrela_mutation")
        .join(&mutation_key);
    fs::create_dir_all(&mutation_root).map_err(|err| MutantCompileFailure {
        reason: format!("failed to create mutation temp directory: {err}"),
        compile_ms: 0,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    let entry_path = mutation_root.join("entry.wr");
    let exe_path = if context.cache_enabled {
        if let Err(err) = fs::create_dir_all(&cache_entry_dir) {
            return Err(MutantCompileFailure {
                reason: format!(
                    "failed to create mutation cache directory {}: {err}",
                    cache_entry_dir.display()
                ),
                compile_ms: 0,
                cache_hits: 0,
                cache_misses: usize::from(context.cache_enabled),
                cache_invalidations,
            });
        }
        cache_bin_path.clone()
    } else {
        mutation_root.join("mutant_bin")
    };

    let (entry_source, wrappers_root) =
        mutation_dispatch_entry_source(&context.workspace_root, &mutation_key, tests).map_err(
            |err| MutantCompileFailure {
                reason: err,
                compile_ms: 0,
                cache_hits: 0,
                cache_misses: usize::from(context.cache_enabled),
                cache_invalidations,
            },
        )?;
    fs::write(&entry_path, entry_source).map_err(|err| MutantCompileFailure {
        reason: format!("failed to write mutation harness entry: {err}"),
        compile_ms: 0,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;

    let compile_start = Instant::now();
    let src_root = context.workspace_root.join("src");
    let tests_root = context.workspace_root.join("tests");
    let mut module = compile_to_mir_with_root(
        &entry_path,
        &src_root,
        tests_root.is_dir().then_some(tests_root.as_path()),
        OutputFormat::Pretty,
        wrela::query_plan::DispatchBackend::Auto,
    )
    .map_err(|code| MutantCompileFailure {
        reason: format!("mutant compile failed before mutation (exit code {code})"),
        compile_ms: compile_start.elapsed().as_millis(),
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    apply_mir_mutation(&mut module, candidate).map_err(|err| MutantCompileFailure {
        reason: err,
        compile_ms: compile_start.elapsed().as_millis(),
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    wrela::backend::cranelift::compile_to_executable(&module, &exe_path).map_err(|err| {
        MutantCompileFailure {
            reason: format!("mutant codegen error: {}", err.0),
            compile_ms: compile_start.elapsed().as_millis(),
            cache_hits: 0,
            cache_misses: usize::from(context.cache_enabled),
            cache_invalidations,
        }
    })?;
    let compile_ms = compile_start.elapsed().as_millis();
    let _ = fs::remove_dir_all(wrappers_root);

    if context.cache_enabled {
        let metadata = MutationCacheMetadata {
            schema_version: MUTATION_CACHE_SCHEMA_VERSION,
            toolchain_version: context.toolchain_version.clone(),
            source_hash: context.source_hash.clone(),
            candidate_key,
            mutant_binary_path: exe_path.display().to_string(),
            build_status: "ok".to_string(),
            invalid_reason: None,
            compile_ms,
        };
        let _ = write_json_atomic(&cache_metadata_path, &metadata);
    }

    Ok(MutantCompileSuccess {
        exe_path,
        compile_ms,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })
}

pub(crate) fn mutation_cache_enabled() -> bool {
    match std::env::var("WRELA_MUTATION_CACHE") {
        Ok(value) => !matches!(value.to_ascii_lowercase().as_str(), "off" | "false" | "0"),
        Err(_) => true,
    }
}

pub(crate) fn mutation_cache_root(workspace_root: &Path) -> PathBuf {
    workspace_root.join("target").join("wrela_mutation_cache")
}

pub(crate) fn mutation_kill_history_path(cache_root: &Path) -> PathBuf {
    cache_root.join("kill_history.json")
}

pub(crate) fn mutation_cache_key(
    source_hash: &str,
    toolchain_version: &str,
    candidate_key: &str,
) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(MUTATION_CACHE_ENGINE_TAG.as_bytes());
    hasher.update(&[0]);
    hasher.update(source_hash.as_bytes());
    hasher.update(&[0]);
    hasher.update(toolchain_version.as_bytes());
    hasher.update(&[0]);
    hasher.update(candidate_key.as_bytes());
    hasher.finish_hex()
}

pub(crate) fn persist_invalid_mutation_cache_entry(
    context: &MutationExecutionContext,
    candidate: &MirMutationCandidate,
    reason: &str,
    compile_ms: u128,
) -> Result<(), String> {
    let candidate_key = mutation_candidate_key(candidate);
    let cache_key = mutation_cache_key(
        &context.source_hash,
        &context.toolchain_version,
        &candidate_key,
    );
    let entry_dir = context.cache_root.join(cache_key);
    let metadata_path = entry_dir.join("metadata.json");
    let metadata = MutationCacheMetadata {
        schema_version: MUTATION_CACHE_SCHEMA_VERSION,
        toolchain_version: context.toolchain_version.clone(),
        source_hash: context.source_hash.clone(),
        candidate_key,
        mutant_binary_path: entry_dir.join("mutant_bin").display().to_string(),
        build_status: "invalid".to_string(),
        invalid_reason: Some(reason.to_string()),
        compile_ms,
    };
    write_json_atomic(&metadata_path, &metadata)
}

pub(crate) fn load_mutation_cache_metadata(path: &Path) -> Option<MutationCacheMetadata> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn load_mutation_kill_history(path: &Path) -> MutationKillHistoryArtifact {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return MutationKillHistoryArtifact {
                schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
                entries: BTreeMap::new(),
            };
        }
    };
    let artifact: MutationKillHistoryArtifact = match serde_json::from_slice(&bytes) {
        Ok(artifact) => artifact,
        Err(_) => {
            return MutationKillHistoryArtifact {
                schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
                entries: BTreeMap::new(),
            };
        }
    };
    if artifact.schema_version != MUTATION_KILL_HISTORY_SCHEMA_VERSION {
        return MutationKillHistoryArtifact {
            schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        };
    }
    artifact
}

pub(crate) fn write_mutation_kill_history(
    path: &Path,
    history: &MutationKillHistoryArtifact,
) -> Result<(), String> {
    write_json_atomic(path, history)
}

pub(crate) fn mutation_history_key(
    function_id: &str,
    mutation_type: &str,
    test_id: &str,
) -> String {
    format!("{function_id}|{mutation_type}|{test_id}")
}

pub(crate) fn order_tests_for_mutation_candidate(
    candidate: &MirMutationCandidate,
    mut tests: Vec<TestCase>,
    history: &MutationKillHistoryArtifact,
) -> Vec<TestCase> {
    tests.sort_by(|a, b| {
        let key_a = mutation_history_key(&candidate.function_id, candidate.mutation_type, &a.id);
        let key_b = mutation_history_key(&candidate.function_id, candidate.mutation_type, &b.id);
        let score_a = history.entries.get(&key_a);
        let score_b = history.entries.get(&key_b);
        let kills_a = score_a.map(|entry| entry.kills).unwrap_or(0);
        let attempts_a = score_a.map(|entry| entry.attempts).unwrap_or(0);
        let kills_b = score_b.map(|entry| entry.kills).unwrap_or(0);
        let attempts_b = score_b.map(|entry| entry.attempts).unwrap_or(0);
        let rate_lhs = (kills_a as u128) * (attempts_b.max(1) as u128);
        let rate_rhs = (kills_b as u128) * (attempts_a.max(1) as u128);
        rate_rhs
            .cmp(&rate_lhs)
            .then(attempts_b.cmp(&attempts_a))
            .then(a.id.cmp(&b.id))
    });
    tests
}

pub(crate) fn update_mutation_kill_history_from_mutants(
    history: &mut MutationKillHistoryArtifact,
    mutants: &[MutationMutantResult],
) {
    history.schema_version = MUTATION_KILL_HISTORY_SCHEMA_VERSION;
    let seen_at = now_unix_ms();
    for mutant in mutants {
        if mutant.status != "killed" && mutant.status != "survived" {
            continue;
        }
        let killer = (mutant.status == "killed")
            .then(|| mutant.tests_ran.last().cloned())
            .flatten();
        for test_id in &mutant.tests_ran {
            let key = mutation_history_key(&mutant.function_id, &mutant.mutation_type, test_id);
            let entry = history
                .entries
                .entry(key)
                .or_insert(MutationKillHistoryEntry {
                    kills: 0,
                    attempts: 0,
                    last_seen_unix_ms: seen_at,
                });
            entry.attempts = entry.attempts.saturating_add(1);
            if killer.as_deref() == Some(test_id.as_str()) {
                entry.kills = entry.kills.saturating_add(1);
            }
            entry.last_seen_unix_ms = seen_at;
        }
    }
}

pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("failed to serialize {}: {}", path.display(), err))?;
    write_bytes_atomic(path, &bytes)
}

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!(
            "atomic write target has no parent: {}",
            path.display()
        ));
    };
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    let tmp_path = parent.join(format!(".tmp-{}-{}", std::process::id(), now_unix_ms()));
    fs::write(&tmp_path, bytes).map_err(|err| {
        format!(
            "failed to write temporary file {}: {}",
            tmp_path.display(),
            err
        )
    })?;
    fs::rename(&tmp_path, path).map_err(|err| {
        format!(
            "failed to atomically rename {} -> {}: {}",
            tmp_path.display(),
            path.display(),
            err
        )
    })?;
    Ok(())
}

pub(crate) fn mutation_dispatch_entry_source(
    workspace_root: &Path,
    mutation_key: &str,
    tests: &[TestCase],
) -> Result<(String, PathBuf), String> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return Err("mutation harness generation requires tests/ directory".to_string());
    }
    let wrappers_root = tests_root.join("wrela_mutation").join(mutation_key);
    let wrappers_dir = wrappers_root.join("cases");
    fs::create_dir_all(&wrappers_dir).map_err(|err| {
        format!(
            "failed to create mutation wrapper directory {}: {err}",
            wrappers_dir.display()
        )
    })?;
    let mut source = String::new();
    let mut dispatch_arms = Vec::with_capacity(tests.len());
    for (idx, test) in tests.iter().enumerate() {
        let wrapper_func = format!("run_case_{idx}");
        let wrapper_module = format!("tests/wrela_mutation/{mutation_key}/cases/case_{idx}_test");
        let wrapper_source = format!(
            "use {func} from {module}\n\nfn {wrapper_func}() -> Nothing {{\n    {dispatch}\n}}\n",
            func = test.func_name,
            module = test.module_path,
            dispatch = test_case_dispatch_stmt(test)
        );
        let wrapper_path = wrappers_dir.join(format!("case_{idx}_test.wr"));
        fs::write(&wrapper_path, wrapper_source).map_err(|err| {
            format!(
                "failed to write mutation wrapper {}: {}",
                wrapper_path.display(),
                err
            )
        })?;
        source.push_str(&format!("use {wrapper_func} from {wrapper_module}\n"));
        dispatch_arms.push((test.id.clone(), wrapper_func));
    }
    source.push('\n');
    source.push_str("fn run() -> Integer {\n");
    source.push_str("    selected_value = __wr_env_get(\"WRELA_TEST_ID\")\n");
    source.push_str("    mutable selected = \"\"\n");
    source.push_str("    match selected_value {\n");
    source.push_str("        String {\n");
    source.push_str("            selected = selected_value\n");
    source.push_str("        }\n");
    source.push_str("        default {\n");
    source.push_str("            selected = \"\"\n");
    source.push_str("        }\n");
    source.push_str("    }\n");
    for (id, dispatch_func) in &dispatch_arms {
        source.push_str(&format!("    if selected == \"{id}\" {{\n"));
        source.push_str(&format!("        {dispatch_func}()\n"));
        source.push_str("        return 0\n");
        source.push_str("    }\n");
    }
    source.push_str("    return 4\n");
    source.push_str("}\n");
    Ok((source, wrappers_root))
}

pub(crate) fn apply_mir_mutation(
    module: &mut mir::ir::MirModule,
    candidate: &MirMutationCandidate,
) -> Result<(), String> {
    let mut matching_indices = module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, func)| {
            (func.name.as_str() == candidate.function_name).then_some(index)
        })
        .collect::<Vec<_>>();
    if matching_indices.is_empty() {
        return Err(format!(
            "function '{}' not found while applying mutant",
            candidate.function_name
        ));
    }
    if matching_indices.len() > 1 {
        return Err(format!(
            "ambiguous mutation target '{}': {} MIR functions match by name",
            candidate.function_name,
            matching_indices.len()
        ));
    }
    let function_index = matching_indices.pop().unwrap_or(0);
    let function = &mut module.functions[function_index];
    match candidate.site {
        MutationSite::Branch { block_idx } => {
            let block = function
                .blocks
                .get_mut(block_idx)
                .ok_or_else(|| format!("invalid branch mutation block index {}", block_idx))?;
            let mir::ir::Terminator::Branch {
                then_target,
                else_target,
                ..
            } = &mut block.terminator
            else {
                return Err("branch mutation site no longer contains a branch".to_string());
            };
            std::mem::swap(then_target, else_target);
        }
        MutationSite::Comparison {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { op, .. } = stmt else {
                return Err("comparison mutation site no longer contains a binary op".to_string());
            };
            *op = invertible_comparison(*op)
                .ok_or_else(|| "comparison mutation site is not invertible".to_string())?;
        }
        MutationSite::IntegerLiteralUse {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Integer(value))) = stmt
            else {
                return Err(
                    "integer mutation site no longer contains a constant literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::IntegerLiteralBinaryLhs {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { lhs, .. } = stmt else {
                return Err("integer mutation lhs site no longer contains a binary op".to_string());
            };
            let mir::ir::Value::Const(hir::Literal::Integer(value)) = lhs else {
                return Err(
                    "integer mutation lhs site no longer contains an integer literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::IntegerLiteralBinaryRhs {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { rhs, .. } = stmt else {
                return Err("integer mutation rhs site no longer contains a binary op".to_string());
            };
            let mir::ir::Value::Const(hir::Literal::Integer(value)) = rhs else {
                return Err(
                    "integer mutation rhs site no longer contains an integer literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::ResultGuard {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            if !matches!(stmt, mir::ir::Rvalue::ResultIsOk { .. }) {
                return Err("result-guard mutation site no longer contains ResultIsOk".to_string());
            }
            *stmt = mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Boolean(true)));
        }
    }
    Ok(())
}

pub(crate) fn mutation_assign_stmt(
    function: &mut mir::ir::MirFunction,
    block_idx: usize,
    stmt_idx: usize,
) -> Result<&mut mir::ir::Rvalue, String> {
    let block = function
        .blocks
        .get_mut(block_idx)
        .ok_or_else(|| format!("invalid mutation block index {}", block_idx))?;
    let stmt = block
        .stmts
        .get_mut(stmt_idx)
        .ok_or_else(|| format!("invalid mutation stmt index {}", stmt_idx))?;
    let mir::ir::Stmt::Assign { value, .. } = stmt else {
        return Err("mutation site no longer contains an assignment".to_string());
    };
    Ok(value)
}

pub(crate) fn invertible_comparison(op: hir::BinaryOp) -> Option<hir::BinaryOp> {
    match op {
        hir::BinaryOp::Eq => Some(hir::BinaryOp::Ne),
        hir::BinaryOp::Ne => Some(hir::BinaryOp::Eq),
        hir::BinaryOp::Lt => Some(hir::BinaryOp::Ge),
        hir::BinaryOp::Gt => Some(hir::BinaryOp::Le),
        hir::BinaryOp::Le => Some(hir::BinaryOp::Gt),
        hir::BinaryOp::Ge => Some(hir::BinaryOp::Lt),
        _ => None,
    }
}

pub(crate) fn perturb_integer(value: i64) -> i64 {
    if value >= 0 {
        value.saturating_add(1)
    } else {
        value.saturating_sub(1)
    }
}

pub(crate) fn run_with_timeout(
    exe: &Path,
    timeout: Duration,
    metrics_path: Option<&Path>,
    cwd: Option<&Path>,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<(), String> {
    let exe_path = if exe.is_absolute() {
        exe.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|err| format!("failed to read current directory: {err}"))?
            .join(exe)
    };
    let mut command = Command::new(exe_path);
    command.args(args);
    if let Some(path) = cwd {
        command.current_dir(path);
    }
    command.env_clear();
    for key in inherited_test_env_keys() {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    if let Some(path) = metrics_path {
        command.env("WRELA_METRICS_PATH", path);
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(|e| format!("failed to run: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait failed: {e}"))? {
            if status.success() {
                return Ok(());
            }
            return Err(format!("exit code {}", status.code().unwrap_or(1)));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            return Err("timeout".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn compile_to_mir_with_root(
    entry_path: &Path,
    root_dir: &Path,
    tests_dir: Option<&Path>,
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<mir::ir::MirModule, i32> {
    let project = match hir::project::load_project_with_roots(
        entry_path,
        root_dir,
        tests_dir.map(|p| p.to_path_buf()),
        true,
    ) {
        Ok(project) => project,
        Err(errors) => {
            let mut records = Vec::new();
            for err in errors {
                let record = project_record(
                    err.kind,
                    DiagSeverity::Error,
                    err.message,
                    err.path.display().to_string(),
                    err.span,
                );
                records.push((record, err.source));
            }
            diag_emit::emit_deduped_records_with_sources(output_format, records);
            return Err(EXIT_PARSE);
        }
    };
    let module = project.module.clone();
    let source = project.entry_source.clone();
    let source_name = entry_path.display().to_string();
    let mut source_by_path = project.module_sources.clone();
    let provenance = project.provenance.clone();
    let naming_errors = project_naming_diagnostics(&project);
    source_by_path
        .entry(entry_path.to_path_buf())
        .or_insert_with(|| source.clone());
    let default_source = source.clone();
    let default_path = source_name.clone();
    for warn in project.warnings {
        let record = project_record(
            warn.kind,
            DiagSeverity::Warning,
            warn.message,
            warn.path.display().to_string(),
            warn.span,
        );
        diag_emit::emit_diag_record(output_format, &record, &warn.source);
    }
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    let mut had_errors = false;
    let mut records = Vec::new();
    for err in type_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &default_path);
        let record = DiagRecord::from_diagnostic(
            DiagStage::Type,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        );
        records.push(record);
        had_errors = true;
    }
    let strict_naming = false;
    for (path, _source_for_path, err) in naming_errors {
        let path = path.display().to_string();
        let fixes = conservative_naming_fixes(&err, &path);
        let severity = naming_policy_severity(&err, strict_naming);
        let record = DiagRecord::from_diagnostic(
            DiagStage::Naming,
            severity,
            &err,
            path,
            err.primary_span(),
        )
        .with_fixes(fixes)
        .with_data(Some(serde_json::json!({
            "kind": "naming_policy",
            "tier": naming_policy_tier(&err)
        })));
        records.push(record);
        if matches!(severity, DiagSeverity::Error) {
            had_errors = true;
        }
    }
    for record in suppress_cascades(dedupe_records(records)) {
        let source_for_record = source_by_path
            .get(std::path::Path::new(
                &record
                    .labels
                    .first()
                    .map(|label| label.span.path.clone())
                    .unwrap_or_else(|| default_path.clone()),
            ))
            .cloned()
            .unwrap_or_else(|| default_source.clone());
        diag_emit::emit_diag_record(output_format, &record, &source_for_record);
    }
    if had_errors {
        return Err(EXIT_TYPE);
    }
    let mir_module =
        mir::lower::lower_module_with_types_and_backend(&module, &type_info, query_backend);
    had_errors = false;
    for err in mir::validate::validate_module(&mir_module) {
        let desc = mir_descriptor(err.kind);
        let record = DiagRecord::new(
            desc.stage,
            DiagSeverity::Error,
            err.message,
            source_name.clone(),
            err.span
                .unwrap_or_else(|| SourceSpan::from((0usize, 0usize))),
        )
        .with_code(Some(desc.code.to_string()))
        .with_help(Some(desc.help_template.to_string()));
        diag_emit::emit_diag_record(output_format, &record, &source);
        had_errors = true;
    }
    if had_errors {
        Err(EXIT_CODEGEN)
    } else {
        Ok(mir_module)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum DiagnosticScope {
    Workspace,
    TargetFile { normalized_path: String },
}

impl DiagnosticScope {
    pub(crate) fn from_entrypoint(entry_path: &Path, workspace_diagnostics: bool) -> Self {
        if workspace_diagnostics {
            return DiagnosticScope::Workspace;
        }
        DiagnosticScope::TargetFile {
            normalized_path: normalize_path_key(entry_path),
        }
    }

    pub(crate) fn allows_path(&self, path: &Path) -> bool {
        match self {
            DiagnosticScope::Workspace => true,
            DiagnosticScope::TargetFile { normalized_path } => {
                normalize_path_key(path) == *normalized_path
            }
        }
    }

    pub(crate) fn allows_path_str(&self, path: &str) -> bool {
        self.allows_path(Path::new(path))
    }
}

pub(crate) fn normalize_path_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}
