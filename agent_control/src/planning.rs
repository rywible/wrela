use sha2::{Digest, Sha256};

use crate::types::{
    AgentDeterministicExecutionContractV2, AgentExecutionPlanV2, AgentFidelityModeV2,
    AgentPipelineV2, AgentRunIntentV2, AgentTaskSpecV2, AgentVariantScoreDecisionV1,
    AgentVariantScoringReportV1,
};

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn digest_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

pub fn default_seed_for_prompt(prompt: &str, app_path: &str) -> u64 {
    let digest = digest_bytes(&[prompt.as_bytes(), &[0], app_path.as_bytes()]);
    let mut seed_bytes = [0u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(seed_bytes)
}

pub fn build_run_id(now_epoch_ms: u128, pid: u32, intent_hash: &str) -> String {
    format!("run-{now_epoch_ms}-{pid}-{intent_hash}")
}

pub fn build_deterministic_run_id(intent_hash: &str) -> String {
    format!("run-intent-{intent_hash}")
}

pub fn hash_intent(intent: &AgentRunIntentV2) -> String {
    let encoded = serde_json::to_vec(intent)
        .expect("serializing AgentRunIntentV2 should not fail for valid UTF-8 strings");
    let digest = digest_bytes(&[&encoded]);
    to_hex(&digest)
}

fn pipeline_flag(pipeline: &AgentPipelineV2) -> &'static str {
    match pipeline {
        AgentPipelineV2::CompilerFirstOneShot => "compiler-first-one-shot",
    }
}

fn fidelity_mode_flag(mode: &AgentFidelityModeV2) -> &'static str {
    match mode {
        AgentFidelityModeV2::Fast => "fast",
        AgentFidelityModeV2::Balanced => "balanced",
        AgentFidelityModeV2::Strict => "strict",
    }
}

#[derive(serde::Serialize)]
struct TaskGraphHashProjection<'a> {
    id: &'a str,
    depends_on: &'a [String],
    command: &'a str,
    args: Vec<String>,
    gate: &'a str,
}

fn canonicalize_task_args_for_hash(args: &[String]) -> Vec<String> {
    let mut canonical = Vec::with_capacity(args.len());
    let mut index = 0usize;
    while index < args.len() {
        let value = args[index].as_str();
        if value == "--run-id" {
            canonical.push("--run-id".to_string());
            canonical.push("__RUN_ID__".to_string());
            index = index.saturating_add(2);
            continue;
        }
        canonical.push(value.to_string());
        index += 1;
    }
    canonical
}

fn hash_task_graph(tasks: &[AgentTaskSpecV2]) -> String {
    let projection: Vec<TaskGraphHashProjection<'_>> = tasks
        .iter()
        .map(|task| TaskGraphHashProjection {
            id: task.id.as_str(),
            depends_on: task.depends_on.as_slice(),
            command: task.command.as_str(),
            args: canonicalize_task_args_for_hash(task.args.as_slice()),
            gate: task.gate.as_str(),
        })
        .collect();
    let encoded = serde_json::to_vec(&projection)
        .expect("serializing canonical task graph projection should not fail");
    let digest = digest_bytes(&[&encoded]);
    to_hex(&digest)
}

fn build_execution_contract(
    intent: &AgentRunIntentV2,
    tasks: &[AgentTaskSpecV2],
) -> AgentDeterministicExecutionContractV2 {
    AgentDeterministicExecutionContractV2 {
        pipeline: intent.execution_contract.pipeline.clone(),
        deterministic: true,
        stage_order: tasks.iter().map(|task| task.id.clone()).collect(),
        task_graph_hash: hash_task_graph(tasks),
        max_retries: intent.execution_contract.max_retries,
        fallback_enabled: intent.execution_contract.fallback_enabled,
    }
}

fn compile_plan_with_run_id(intent: &AgentRunIntentV2, run_id: String) -> AgentExecutionPlanV2 {
    let intent_hash = hash_intent(intent);
    let tasks = vec![
        AgentTaskSpecV2 {
            id: "compile-intent".to_string(),
            step: "Compile and validate run intent".to_string(),
            depends_on: Vec::new(),
            command: "agentctl".to_string(),
            args: vec![
                "compile-intent".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
                "--intent-hash".to_string(),
                intent_hash.clone(),
                "--pipeline".to_string(),
                pipeline_flag(&intent.execution_contract.pipeline).to_string(),
            ],
            gate: "always".to_string(),
        },
        AgentTaskSpecV2 {
            id: "compile-tool-contracts".to_string(),
            step: "Compile tool contracts for one-shot execution".to_string(),
            depends_on: vec!["compile-intent".to_string()],
            command: "agentctl".to_string(),
            args: vec![
                "compile-tool-contracts".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
                "--tool-contract-count".to_string(),
                intent.tool_contracts.len().to_string(),
                "--approval-mode".to_string(),
                intent.policy_profile.approval_mode.clone(),
                "--sandbox-mode".to_string(),
                intent.policy_profile.sandbox_mode.clone(),
            ],
            gate: "on-success".to_string(),
        },
        AgentTaskSpecV2 {
            id: "execute-one-shot".to_string(),
            step: "Execute compiler-first one-shot pipeline".to_string(),
            depends_on: vec!["compile-tool-contracts".to_string()],
            command: "agentctl".to_string(),
            args: vec![
                "execute-one-shot".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
                "--max-input-tokens".to_string(),
                intent.budget_spec.max_input_tokens.to_string(),
                "--max-output-tokens".to_string(),
                intent.budget_spec.max_output_tokens.to_string(),
                "--max-tool-calls".to_string(),
                intent.budget_spec.max_tool_calls.to_string(),
                "--max-wall-time-ms".to_string(),
                intent.budget_spec.max_wall_time_ms.to_string(),
                "--vibe-style".to_string(),
                intent.vibe_spec.style.clone(),
            ],
            gate: "on-success".to_string(),
        },
        AgentTaskSpecV2 {
            id: "verify-fidelity".to_string(),
            step: "Verify fidelity and reproducibility requirements".to_string(),
            depends_on: vec!["execute-one-shot".to_string()],
            command: "agentctl".to_string(),
            args: vec![
                "verify-fidelity".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
                "--mode".to_string(),
                fidelity_mode_flag(&intent.fidelity_spec.mode).to_string(),
                "--min-test-coverage".to_string(),
                intent.fidelity_spec.min_test_coverage.to_string(),
                "--require-reproducible-artifacts".to_string(),
                intent
                    .fidelity_spec
                    .require_reproducible_artifacts
                    .to_string(),
            ],
            gate: "on-success".to_string(),
        },
        AgentTaskSpecV2 {
            id: "publish-artifacts".to_string(),
            step: "Publish artifacts and finalize run summary".to_string(),
            depends_on: vec!["verify-fidelity".to_string()],
            command: "agentctl".to_string(),
            args: vec![
                "publish-artifacts".to_string(),
                "--run-id".to_string(),
                run_id.clone(),
                "--target-profile".to_string(),
                intent.target_profile.clone(),
                "--network-access".to_string(),
                intent.policy_profile.network_access.to_string(),
                "--fallback-enabled".to_string(),
                intent.execution_contract.fallback_enabled.to_string(),
            ],
            gate: "on-success".to_string(),
        },
    ];
    let task_count = tasks.len();
    let execution_contract = build_execution_contract(intent, &tasks);

    AgentExecutionPlanV2 {
        schema_version: 2,
        kind: "agent_execution_plan_v2".to_string(),
        run_id,
        intent_hash,
        created_from_intent_kind: intent.kind.clone(),
        execution_contract,
        task_count,
        tasks,
    }
}

pub fn compile_plan(
    intent: &AgentRunIntentV2,
    now_epoch_ms: u128,
    pid: u32,
) -> AgentExecutionPlanV2 {
    let intent_hash = hash_intent(intent);
    let run_id = build_run_id(now_epoch_ms, pid, &intent_hash);
    compile_plan_with_run_id(intent, run_id)
}

pub fn compile_plan_deterministic(intent: &AgentRunIntentV2) -> AgentExecutionPlanV2 {
    let intent_hash = hash_intent(intent);
    let run_id = build_deterministic_run_id(&intent_hash);
    compile_plan_with_run_id(intent, run_id)
}

pub fn build_variant_scoring_report(
    intent: &AgentRunIntentV2,
    run_id: &str,
) -> AgentVariantScoringReportV1 {
    let max_variants = if intent.asset_factory_intent.enabled {
        intent.asset_factory_intent.variant_count.max(1)
    } else {
        1
    };
    let mut decisions = Vec::with_capacity(max_variants as usize);
    for index in 0..max_variants {
        let digest = digest_bytes(&[
            run_id.as_bytes(),
            &[0],
            intent.prompt.as_bytes(),
            &[0],
            intent.target_profile.as_bytes(),
            &[0],
            index.to_string().as_bytes(),
        ]);
        let score_raw = u16::from_be_bytes([digest[0], digest[1]]);
        let score = f64::from(score_raw) / f64::from(u16::MAX);
        decisions.push(AgentVariantScoreDecisionV1 {
            variant_id: format!("variant-{index}"),
            score,
            rank: 0,
            reasons: vec![
                format!("seed-lock={}", intent.asset_factory_intent.seed_lock),
                format!(
                    "strict-provenance={}",
                    intent.asset_factory_intent.strict_provenance
                ),
            ],
        });
    }
    decisions.sort_by(|lhs, rhs| {
        rhs.score
            .partial_cmp(&lhs.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| lhs.variant_id.cmp(&rhs.variant_id))
    });
    for (idx, decision) in decisions.iter_mut().enumerate() {
        decision.rank = (idx as u32) + 1;
    }
    let selected_variant = decisions
        .first()
        .map(|decision| decision.variant_id.clone())
        .unwrap_or_else(|| "variant-0".to_string());
    AgentVariantScoringReportV1 {
        schema_version: 1,
        kind: "agent_variant_scoring_report_v1".to_string(),
        run_id: run_id.to_string(),
        selected_variant,
        decisions,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_deterministic_run_id, build_run_id, build_variant_scoring_report, compile_plan,
        compile_plan_deterministic, default_seed_for_prompt, hash_intent,
    };
    use crate::types::{
        AgentBudgetSpecV2, AgentExecutionContractSpecV2, AgentFidelityModeV2, AgentFidelitySpecV2,
        AgentPipelineV2, AgentPolicyProfileV2, AgentRunIntentV2, AgentToolContractV2,
        AgentVibeSpecV2, AssetFactoryIntentV1,
    };

    fn sample_intent() -> AgentRunIntentV2 {
        AgentRunIntentV2 {
            schema_version: 2,
            kind: "agent_run_intent_v2".to_string(),
            prompt: "Implement deterministic planning".to_string(),
            app_path: "/tmp/app".to_string(),
            seed: 42,
            target_profile: "ship".to_string(),
            execution_profile: "default".to_string(),
            constraints: vec!["no-network".to_string(), "ascii-only".to_string()],
            vibe_spec: AgentVibeSpecV2 {
                style: "focused".to_string(),
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
                max_tool_calls: 32,
                max_wall_time_ms: 120_000,
            },
            policy_profile: AgentPolicyProfileV2 {
                sandbox_mode: "danger-full-access".to_string(),
                network_access: false,
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
                enabled: true,
                mode: "full-factory".to_string(),
                providers: vec![
                    "image-default".to_string(),
                    "mesh-default".to_string(),
                    "audio-default".to_string(),
                ],
                strict_provenance: true,
                variant_count: 3,
                seed_lock: true,
            },
        }
    }

    #[test]
    fn seed_is_deterministic() {
        let a = default_seed_for_prompt("hello", "/path/to/app");
        let b = default_seed_for_prompt("hello", "/path/to/app");
        let c = default_seed_for_prompt("hello!", "/path/to/app");

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn run_id_is_deterministic() {
        let h = "abcd1234";
        let a = build_run_id(1_700_000_000_000, 1234, h);
        let b = build_run_id(1_700_000_000_000, 1234, h);
        let c = build_run_id(1_700_000_000_001, 1234, h);

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn intent_hash_is_deterministic() {
        let i = sample_intent();
        let mut j = sample_intent();

        let a = hash_intent(&i);
        let b = hash_intent(&j);
        j.vibe_spec.creativity += 1;
        let c = hash_intent(&j);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn plan_has_expected_topology_and_order() {
        let intent = sample_intent();
        let plan = compile_plan(&intent, 1_700_000_000_123, 9001);

        assert_eq!(plan.schema_version, 2);
        assert_eq!(plan.kind, "agent_execution_plan_v2");
        assert_eq!(plan.created_from_intent_kind, intent.kind.as_str());
        assert_eq!(plan.task_count, 5);
        assert_eq!(plan.tasks.len(), 5);

        assert_eq!(plan.tasks[0].id, "compile-intent");
        assert_eq!(plan.tasks[0].depends_on, Vec::<String>::new());

        assert_eq!(plan.tasks[1].id, "compile-tool-contracts");
        assert_eq!(plan.tasks[1].depends_on, vec!["compile-intent"]);

        assert_eq!(plan.tasks[2].id, "execute-one-shot");
        assert_eq!(plan.tasks[2].depends_on, vec!["compile-tool-contracts"]);

        assert_eq!(plan.tasks[3].id, "verify-fidelity");
        assert_eq!(plan.tasks[3].depends_on, vec!["execute-one-shot"]);

        assert_eq!(plan.tasks[4].id, "publish-artifacts");
        assert_eq!(plan.tasks[4].depends_on, vec!["verify-fidelity"]);

        let expected_hash = hash_intent(&intent);
        let expected_run_id = build_run_id(1_700_000_000_123, 9001, &expected_hash);
        assert_eq!(plan.intent_hash, expected_hash);
        assert_eq!(plan.run_id, expected_run_id);
        assert!(plan.execution_contract.deterministic);
        assert_eq!(
            plan.execution_contract.pipeline,
            AgentPipelineV2::CompilerFirstOneShot
        );
        assert_eq!(
            plan.execution_contract.stage_order,
            vec![
                "compile-intent",
                "compile-tool-contracts",
                "execute-one-shot",
                "verify-fidelity",
                "publish-artifacts",
            ]
        );
        assert_eq!(plan.execution_contract.task_graph_hash.len(), 64);
        assert_eq!(
            plan.execution_contract.max_retries,
            intent.execution_contract.max_retries
        );
        assert_eq!(
            plan.execution_contract.fallback_enabled,
            intent.execution_contract.fallback_enabled
        );
    }

    #[test]
    fn plan_is_deterministic_for_same_inputs() {
        let intent = sample_intent();
        let a = compile_plan(&intent, 1_700_000_000_123, 9001);
        let b = compile_plan(&intent, 1_700_000_000_123, 9001);

        assert_eq!(a, b);
    }

    #[test]
    fn deterministic_plan_ignores_runtime_clock_and_pid() {
        let intent = sample_intent();
        let expected_hash = hash_intent(&intent);
        let expected_run_id = build_deterministic_run_id(&expected_hash);

        let a = compile_plan_deterministic(&intent);
        let b = compile_plan_deterministic(&intent);

        assert_eq!(a, b);
        assert_eq!(a.run_id, expected_run_id);
        assert_eq!(a.created_from_intent_kind, intent.kind.as_str());
        assert_eq!(a.task_count, a.tasks.len());
        assert!(a.execution_contract.deterministic);
    }

    #[test]
    fn task_graph_hash_is_stable_across_runtime_run_ids() {
        let intent = sample_intent();
        let a = compile_plan(&intent, 1_700_000_000_123, 9001);
        let b = compile_plan(&intent, 1_700_000_123_456, 4242);

        assert_ne!(a.run_id, b.run_id);
        assert_eq!(
            a.execution_contract.task_graph_hash,
            b.execution_contract.task_graph_hash
        );
    }

    #[test]
    fn variant_scoring_is_deterministic_and_ranked() {
        let intent = sample_intent();
        let run_id = "run-intent-deadbeef";
        let a = build_variant_scoring_report(&intent, run_id);
        let b = build_variant_scoring_report(&intent, run_id);
        assert_eq!(a, b);
        assert_eq!(a.schema_version, 1);
        assert_eq!(a.kind, "agent_variant_scoring_report_v1");
        assert_eq!(a.decisions.len(), 3);
        assert_eq!(a.decisions[0].rank, 1);
        assert_eq!(a.selected_variant, a.decisions[0].variant_id);
    }
}
