use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardKeyTelemetrySample {
    pub relation: String,
    pub key_spec: String,
    pub shard_id: u32,
    pub read_count: u64,
    pub write_count: u64,
    pub distinct_keys_observed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardKeyTelemetryProfile {
    pub relation: String,
    pub key_spec: String,
    pub shard_count: usize,
    pub total_reads: u64,
    pub total_writes: u64,
    pub total_observations: u64,
    pub hottest_shard: u32,
    pub hottest_shard_ops: u64,
    pub coldest_shard: u32,
    pub coldest_shard_ops: u64,
    pub skew_per_mille: u64,
    pub cardinality_ratio_per_mille: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorRecommendation {
    pub relation: String,
    pub current_key_spec: String,
    pub suggested_key_spec: String,
    pub reasons: Vec<String>,
    pub risk_score_per_mille: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorExplain {
    pub recommendation: AdvisorRecommendation,
    pub profile: ShardKeyTelemetryProfile,
    pub evidence: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerLint {
    pub code: &'static str,
    pub severity: LintSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvisorError {
    EmptySamples,
    MixedRelation,
    MixedKeySpec,
}

pub fn profile(
    samples: &[ShardKeyTelemetrySample],
) -> Result<ShardKeyTelemetryProfile, AdvisorError> {
    if samples.is_empty() {
        return Err(AdvisorError::EmptySamples);
    }
    let relation = &samples[0].relation;
    let key_spec = &samples[0].key_spec;
    if samples.iter().any(|s| s.relation != *relation) {
        return Err(AdvisorError::MixedRelation);
    }
    if samples.iter().any(|s| s.key_spec != *key_spec) {
        return Err(AdvisorError::MixedKeySpec);
    }

    let mut by_shard: BTreeMap<u32, u64> = BTreeMap::new();
    let mut total_reads = 0u64;
    let mut total_writes = 0u64;
    let mut distinct_keys = 0u64;

    for sample in samples {
        let ops = sample.read_count.saturating_add(sample.write_count);
        *by_shard.entry(sample.shard_id).or_default() = by_shard
            .get(&sample.shard_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(ops);
        total_reads = total_reads.saturating_add(sample.read_count);
        total_writes = total_writes.saturating_add(sample.write_count);
        distinct_keys = distinct_keys.saturating_add(sample.distinct_keys_observed);
    }

    let (&hottest_shard, &hottest_ops) = by_shard
        .iter()
        .max_by_key(|(shard, ops)| (*ops, std::cmp::Reverse(**shard)))
        .expect("non-empty shard profile");
    let (&coldest_shard, &coldest_ops) = by_shard
        .iter()
        .min_by_key(|(shard, ops)| (*ops, *shard))
        .expect("non-empty shard profile");

    let total_ops = total_reads.saturating_add(total_writes);
    let skew_per_mille = if total_ops == 0 {
        0
    } else {
        hottest_ops.saturating_mul(1000).saturating_div(total_ops)
    };

    let cardinality_ratio_per_mille = if total_ops == 0 {
        0
    } else {
        distinct_keys.saturating_mul(1000).saturating_div(total_ops)
    };

    Ok(ShardKeyTelemetryProfile {
        relation: relation.clone(),
        key_spec: key_spec.clone(),
        shard_count: by_shard.len(),
        total_reads,
        total_writes,
        total_observations: total_ops,
        hottest_shard,
        hottest_shard_ops: hottest_ops,
        coldest_shard,
        coldest_shard_ops: coldest_ops,
        skew_per_mille,
        cardinality_ratio_per_mille,
    })
}

pub fn recommend(profile: &ShardKeyTelemetryProfile) -> AdvisorRecommendation {
    let mut reasons = Vec::new();
    let mut risk = 0u64;

    if profile.skew_per_mille >= 450 {
        reasons.push(format!(
            "hotspot detected: shard {} carries {}‰ of operations",
            profile.hottest_shard, profile.skew_per_mille
        ));
        risk = risk.saturating_add((profile.skew_per_mille - 400).min(400));
    }

    if profile.cardinality_ratio_per_mille <= 120 {
        reasons.push(format!(
            "low observed key cardinality: {}‰ distinct/ops",
            profile.cardinality_ratio_per_mille
        ));
        risk = risk
            .saturating_add((150u64.saturating_sub(profile.cardinality_ratio_per_mille)).min(300));
    }

    let suggested_key_spec = if reasons.is_empty() {
        profile.key_spec.clone()
    } else {
        format!("{}+suffix(hash(region,id))", profile.key_spec)
    };

    AdvisorRecommendation {
        relation: profile.relation.clone(),
        current_key_spec: profile.key_spec.clone(),
        suggested_key_spec,
        reasons,
        risk_score_per_mille: risk.min(1000),
    }
}

pub fn explain(
    recommendation: AdvisorRecommendation,
    profile: &ShardKeyTelemetryProfile,
) -> AdvisorExplain {
    let mut evidence = BTreeMap::new();
    evidence.insert("relation".to_string(), profile.relation.clone());
    evidence.insert("current_key".to_string(), profile.key_spec.clone());
    evidence.insert(
        "total_ops".to_string(),
        profile.total_observations.to_string(),
    );
    evidence.insert(
        "hottest_shard".to_string(),
        profile.hottest_shard.to_string(),
    );
    evidence.insert(
        "skew_per_mille".to_string(),
        profile.skew_per_mille.to_string(),
    );
    evidence.insert(
        "cardinality_ratio_per_mille".to_string(),
        profile.cardinality_ratio_per_mille.to_string(),
    );

    AdvisorExplain {
        recommendation,
        profile: profile.clone(),
        evidence,
    }
}

pub fn compiler_feedback(
    relation: &str,
    candidate_key_spec: &str,
    waiver_reason: Option<&str>,
) -> Vec<CompilerLint> {
    let mut lints = Vec::new();
    let lower = candidate_key_spec.to_ascii_lowercase();
    let likely_single_field = !lower.contains(',') && !lower.contains('+');
    let low_entropy_hint =
        lower.contains("status") || lower.contains("region") || lower.contains("tier");

    if likely_single_field && low_entropy_hint {
        let waived = waiver_reason.is_some_and(|w| !w.trim().is_empty());
        if waived {
            lints.push(CompilerLint {
                code: "SHARD_KEY_LOW_ENTROPY_WAIVED",
                severity: LintSeverity::Warning,
                message: format!(
                    "relation `{relation}` uses potentially low-entropy shard key `{candidate_key_spec}` with waiver"
                ),
            });
        } else {
            lints.push(CompilerLint {
                code: "SHARD_KEY_LOW_ENTROPY",
                severity: LintSeverity::Error,
                message: format!(
                    "relation `{relation}` requires composite shard key or waiver; candidate `{candidate_key_spec}` is high hotspot risk"
                ),
            });
        }
    }

    lints
}

pub fn conformance_gate(
    profiles: &[ShardKeyTelemetryProfile],
    max_risk_per_mille: u64,
) -> Result<(), Vec<AdvisorRecommendation>> {
    let mut offenders = Vec::new();
    for p in profiles {
        let rec = recommend(p);
        if rec.risk_score_per_mille > max_risk_per_mille {
            offenders.push(rec);
        }
    }
    if offenders.is_empty() {
        Ok(())
    } else {
        offenders.sort_by(|a, b| b.risk_score_per_mille.cmp(&a.risk_score_per_mille));
        Err(offenders)
    }
}

pub fn profile_coverage(samples: &[ShardKeyTelemetrySample]) -> BTreeMap<String, usize> {
    let mut by_relation: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for sample in samples {
        by_relation
            .entry(sample.relation.clone())
            .or_default()
            .insert(sample.shard_id);
    }
    by_relation
        .into_iter()
        .map(|(relation, shards)| (relation, shards.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<ShardKeyTelemetrySample> {
        vec![
            ShardKeyTelemetrySample {
                relation: "orders".into(),
                key_spec: "region".into(),
                shard_id: 1,
                read_count: 900,
                write_count: 200,
                distinct_keys_observed: 12,
            },
            ShardKeyTelemetrySample {
                relation: "orders".into(),
                key_spec: "region".into(),
                shard_id: 2,
                read_count: 50,
                write_count: 20,
                distinct_keys_observed: 8,
            },
        ]
    }

    #[test]
    fn profile_and_recommendation_capture_hotspot_risk() {
        let p = profile(&samples()).expect("profile");
        assert!(p.skew_per_mille > 900);

        let r = recommend(&p);
        assert!(r.risk_score_per_mille > 0);
        assert!(!r.reasons.is_empty());
        assert_ne!(r.current_key_spec, r.suggested_key_spec);
    }

    #[test]
    fn low_entropy_single_field_requires_waiver() {
        let errs = compiler_feedback("orders", "region", None);
        assert!(errs.iter().any(|l| l.code == "SHARD_KEY_LOW_ENTROPY"));

        let waived = compiler_feedback("orders", "region", Some("geo-only tenancy"));
        assert!(
            waived
                .iter()
                .any(|l| l.code == "SHARD_KEY_LOW_ENTROPY_WAIVED")
        );
    }
}
