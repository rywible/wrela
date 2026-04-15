use serde::Serialize;
use smol_str::SmolStr;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnedMethodPolicy {
    ProposalOnly,
    ConservativeNoFalseNegative,
}

pub fn learned_method_policy_name(policy: LearnedMethodPolicy) -> &'static str {
    match policy {
        LearnedMethodPolicy::ProposalOnly => "proposal-only",
        LearnedMethodPolicy::ConservativeNoFalseNegative => "conservative-no-false-negative",
    }
}

pub const INTERNAL_LEARNED_POLICY_ENV: &str = "WRELA_INTERNAL_LEARNED_POLICY";
pub const INTERNAL_LEARNED_DATASET_OUT_ENV: &str = "WRELA_INTERNAL_LEARNED_DATASET_OUT";

#[derive(Debug, Clone, PartialEq)]
pub struct LearnedStepProposal {
    pub subject: SmolStr,
    pub point: [f32; 3],
    pub direction: [f32; 3],
    pub proposed_step: f32,
    pub no_false_negative_intent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConservativeNeuralBound {
    pub subject: SmolStr,
    pub point: [f32; 3],
    pub conservative_step_bound: f32,
    pub no_false_negative_intent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedVerifierOutcome {
    pub selected: bool,
    pub verified: bool,
    pub accepted: bool,
    pub rejected: bool,
    pub bypassed: bool,
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearnedOracleSample {
    pub subject: SmolStr,
    pub point: [f32; 3],
    pub direction: [f32; 3],
    pub conservative_distance: f32,
    pub dense_oracle_hit_distance: Option<f32>,
    pub candidate_support_interval: Option<[f32; 2]>,
    pub proposed_step: f32,
    pub selected: bool,
    pub verified: bool,
    pub accepted: bool,
    pub rejected: bool,
    pub bypassed: bool,
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearnedOracleDataset {
    pub subject: SmolStr,
    pub samples: Vec<LearnedOracleSample>,
}

#[derive(Debug, Serialize)]
struct LearnedOracleSampleExport {
    subject: String,
    point: [f32; 3],
    direction: [f32; 3],
    conservative_distance: f32,
    dense_oracle_hit_distance: Option<f32>,
    candidate_support_interval: Option<[f32; 2]>,
    proposed_step: f32,
    selected: bool,
    verified: bool,
    accepted: bool,
    rejected: bool,
    bypassed: bool,
    fallback: bool,
}

#[derive(Debug, Serialize)]
struct LearnedOracleDatasetExport {
    subject: String,
    samples: Vec<LearnedOracleSampleExport>,
}

pub fn propose_cpu_oracle_step(
    subject: impl Into<SmolStr>,
    point: [f32; 3],
    direction: [f32; 3],
    conservative_distance: f32,
) -> (LearnedStepProposal, ConservativeNeuralBound) {
    let subject = subject.into();
    let seed = stable_seed(subject.as_str(), point, direction);
    let conservative_bound = conservative_distance.max(0.0);
    let proposed_step = if seed % 5 == 0 {
        conservative_bound * 1.05
    } else {
        conservative_bound * 0.85
    };
    (
        LearnedStepProposal {
            subject: subject.clone(),
            point,
            direction,
            proposed_step,
            no_false_negative_intent: false,
        },
        ConservativeNeuralBound {
            subject,
            point,
            conservative_step_bound: conservative_bound,
            no_false_negative_intent: true,
        },
    )
}

pub fn verify_learned_step(
    proposal: &LearnedStepProposal,
    bound: &ConservativeNeuralBound,
    oracle_distance: f32,
) -> LearnedVerifierOutcome {
    let selected = true;
    let verified =
        bound.no_false_negative_intent && bound.conservative_step_bound <= oracle_distance;
    let accepted = verified && proposal.proposed_step <= bound.conservative_step_bound;
    let rejected = selected && verified && !accepted;
    let fallback = selected && verified && !accepted;
    LearnedVerifierOutcome {
        selected,
        verified,
        accepted,
        rejected,
        bypassed: false,
        fallback,
    }
}

pub fn bypass_learned_step() -> LearnedVerifierOutcome {
    LearnedVerifierOutcome {
        selected: false,
        verified: false,
        accepted: false,
        rejected: false,
        bypassed: true,
        fallback: false,
    }
}

pub fn build_cpu_oracle_dataset(
    subject: impl Into<SmolStr>,
    proposal: &LearnedStepProposal,
    bound: &ConservativeNeuralBound,
    outcome: &LearnedVerifierOutcome,
    conservative_distance: f32,
    dense_oracle_hit_distance: Option<f32>,
    candidate_support_interval: Option<[f32; 2]>,
) -> LearnedOracleDataset {
    let subject = subject.into();
    LearnedOracleDataset {
        subject,
        samples: vec![LearnedOracleSample {
            subject: proposal.subject.clone(),
            point: proposal.point,
            direction: proposal.direction,
            conservative_distance: conservative_distance.max(0.0),
            dense_oracle_hit_distance,
            candidate_support_interval: candidate_support_interval
                .or(Some([0.0, bound.conservative_step_bound])),
            proposed_step: proposal.proposed_step,
            selected: outcome.selected,
            verified: outcome.verified,
            accepted: outcome.accepted,
            rejected: outcome.rejected,
            bypassed: outcome.bypassed,
            fallback: outcome.fallback,
        }],
    }
}

pub fn learned_method_policy_rejection(
    observer: crate::artifact_contract::ArtifactObserver,
    policy: LearnedMethodPolicy,
) -> Option<SmolStr> {
    if matches!(
        observer,
        crate::artifact_contract::ArtifactObserver::Collision
    ) && matches!(policy, LearnedMethodPolicy::ProposalOnly)
    {
        return Some(SmolStr::new(
            "collision learned methods must be conservative and verifier-backed",
        ));
    }
    None
}

pub fn parse_learned_method_policy(value: &str) -> Option<LearnedMethodPolicy> {
    match value {
        "proposal-only" => Some(LearnedMethodPolicy::ProposalOnly),
        "conservative-no-false-negative" => Some(LearnedMethodPolicy::ConservativeNoFalseNegative),
        _ => None,
    }
}

pub fn resolved_learned_method_policy(
    observer: crate::artifact_contract::ArtifactObserver,
) -> Option<LearnedMethodPolicy> {
    let value = env::var(INTERNAL_LEARNED_POLICY_ENV).ok()?;
    let policy = parse_learned_method_policy(value.trim())?;
    if learned_method_policy_rejection(observer, policy).is_some() {
        return None;
    }
    Some(policy)
}

pub fn export_learned_oracle_dataset(
    path: impl AsRef<Path>,
    dataset: &LearnedOracleDataset,
) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let export = LearnedOracleDatasetExport {
        subject: dataset.subject.to_string(),
        samples: dataset
            .samples
            .iter()
            .map(|sample| LearnedOracleSampleExport {
                subject: sample.subject.to_string(),
                point: sample.point,
                direction: sample.direction,
                conservative_distance: sample.conservative_distance,
                dense_oracle_hit_distance: sample.dense_oracle_hit_distance,
                candidate_support_interval: sample.candidate_support_interval,
                proposed_step: sample.proposed_step,
                selected: sample.selected,
                verified: sample.verified,
                accepted: sample.accepted,
                rejected: sample.rejected,
                bypassed: sample.bypassed,
                fallback: sample.fallback,
            })
            .collect(),
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, &export)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    writeln!(file)?;
    Ok(())
}

pub fn maybe_export_learned_oracle_dataset(
    dataset: &LearnedOracleDataset,
) -> io::Result<Option<PathBuf>> {
    let path = match env::var_os(INTERNAL_LEARNED_DATASET_OUT_ENV) {
        Some(path) => PathBuf::from(path),
        None => return Ok(None),
    };
    export_learned_oracle_dataset(&path, dataset)?;
    Ok(Some(path))
}

fn stable_seed(subject: &str, point: [f32; 3], direction: [f32; 3]) -> u64 {
    let mut seed = 0xcbf29ce484222325u64;
    for byte in subject.as_bytes() {
        seed ^= *byte as u64;
        seed = seed.wrapping_mul(0x100000001b3);
    }
    for component in point.into_iter().chain(direction) {
        for byte in component.to_bits().to_le_bytes() {
            seed ^= byte as u64;
            seed = seed.wrapping_mul(0x100000001b3);
        }
    }
    seed
}
