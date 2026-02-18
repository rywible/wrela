use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistogramBucket {
    pub upper_bound: u64,
    pub row_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub version: u32,
    pub histogram_buckets: Vec<HistogramBucket>,
    pub cardinality_estimate: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerStats {
    pub snapshot: StatsSnapshot,
    /// Selectivity in basis points: `0..=10_000` maps to `0%..=100%`.
    pub selectivity: u32,
    pub index_available: bool,
    pub stats_stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanKind {
    IndexLookup,
    FullScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCosts {
    pub index_lookup: u64,
    pub full_scan: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplainOutput {
    pub chosen_plan: PlanKind,
    pub costs: PlanCosts,
    pub stats_version: u32,
    pub stats_stale: bool,
    pub explain_schema_version: u16,
    pub decision_basis: DecisionBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionBasis {
    CostModelV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanBaseline {
    pub kind: PlanKind,
    pub latency_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanBaselineRegistry {
    baselines: HashMap<u64, PlanBaseline>,
}

impl PlanBaselineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, query_fingerprint: u64, baseline: PlanBaseline) {
        self.baselines.insert(query_fingerprint, baseline);
    }

    pub fn get(&self, query_fingerprint: u64) -> Option<PlanBaseline> {
        self.baselines.get(&query_fingerprint).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftObservation {
    pub kind: PlanKind,
    pub latency_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftGatePolicy {
    /// Maximum allowed latency increase in basis points (`100 = 1%`).
    pub max_latency_drift_bps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftGateFailure {
    PlanKindChanged {
        baseline: PlanKind,
        observed: PlanKind,
    },
    LatencyDriftExceeded {
        baseline_latency_ns: u64,
        observed_latency_ns: u64,
        allowed_max_latency_ns: u64,
    },
    MissingBaseline {
        query_fingerprint: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshPolicy {
    pub staleness_threshold: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshState {
    pub snapshot: StatsSnapshot,
    pub updates_since_refresh: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTrigger {
    StalenessThreshold,
    OnDemand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshOutcome {
    pub state: RefreshState,
    pub trigger: Option<RefreshTrigger>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsCodecError {
    Truncated,
    UnexpectedTrailingBytes,
    InvalidMagic,
    UnsupportedCodecVersion,
    InvalidSnapshotVersion,
    TooManyBuckets,
    NonMonotonicBuckets,
}

impl Display for StatsCodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("stats snapshot decode failed: truncated payload"),
            Self::UnexpectedTrailingBytes => {
                f.write_str("stats snapshot decode failed: unexpected trailing bytes")
            }
            Self::InvalidMagic => f.write_str("stats snapshot decode failed: invalid magic"),
            Self::UnsupportedCodecVersion => {
                f.write_str("stats snapshot decode failed: unsupported codec version")
            }
            Self::InvalidSnapshotVersion => {
                f.write_str("stats snapshot decode failed: invalid snapshot version")
            }
            Self::TooManyBuckets => {
                f.write_str("stats snapshot decode failed: too many histogram buckets")
            }
            Self::NonMonotonicBuckets => {
                f.write_str("stats snapshot decode failed: non-monotonic histogram buckets")
            }
        }
    }
}

impl Error for StatsCodecError {}

const SELECTIVITY_SCALE: u32 = 10_000;
const FULL_SCAN_ROW_COST: u64 = 10;
const INDEX_LOOKUP_SEEK_COST: u64 = 100;
const INDEX_LOOKUP_ROW_COST: u64 = 10;
const EXPLAIN_SCHEMA_VERSION: u16 = 2;
const STATS_CODEC_MAGIC: [u8; 4] = *b"WRST";
const STATS_CODEC_VERSION: u16 = 1;
const MAX_HISTOGRAM_BUCKETS: usize = 4096;

pub fn explain(stats: PlannerStats) -> ExplainOutput {
    let costs = PlanCosts {
        index_lookup: index_lookup_cost(&stats),
        full_scan: full_scan_cost(&stats),
    };

    let chosen_plan = choose_plan(costs);
    ExplainOutput {
        chosen_plan,
        costs,
        stats_version: stats.snapshot.version,
        stats_stale: stats.stats_stale,
        explain_schema_version: EXPLAIN_SCHEMA_VERSION,
        decision_basis: DecisionBasis::CostModelV1,
    }
}

pub fn evaluate_drift_gate(
    baselines: &PlanBaselineRegistry,
    query_fingerprint: u64,
    observed: DriftObservation,
    policy: DriftGatePolicy,
) -> Result<(), DriftGateFailure> {
    let Some(baseline) = baselines.get(query_fingerprint) else {
        return Err(DriftGateFailure::MissingBaseline { query_fingerprint });
    };

    if observed.kind != baseline.kind {
        return Err(DriftGateFailure::PlanKindChanged {
            baseline: baseline.kind,
            observed: observed.kind,
        });
    }

    let allowed_max_latency_ns = max_allowed_latency_ns(baseline.latency_ns, policy);
    if observed.latency_ns > allowed_max_latency_ns {
        return Err(DriftGateFailure::LatencyDriftExceeded {
            baseline_latency_ns: baseline.latency_ns,
            observed_latency_ns: observed.latency_ns,
            allowed_max_latency_ns,
        });
    }

    Ok(())
}

pub fn should_refresh(
    policy: RefreshPolicy,
    updates_since_refresh: u64,
    on_demand_refresh: bool,
) -> Option<RefreshTrigger> {
    if on_demand_refresh {
        return Some(RefreshTrigger::OnDemand);
    }

    if updates_since_refresh >= policy.staleness_threshold {
        return Some(RefreshTrigger::StalenessThreshold);
    }

    None
}

pub fn is_snapshot_stale(policy: RefreshPolicy, updates_since_refresh: u64) -> bool {
    updates_since_refresh >= policy.staleness_threshold
}

pub fn refresh_snapshot<F>(
    state: RefreshState,
    policy: RefreshPolicy,
    on_demand_refresh: bool,
    mut build_snapshot: F,
) -> RefreshOutcome
where
    F: FnMut(u32) -> StatsSnapshot,
{
    let trigger = should_refresh(policy, state.updates_since_refresh, on_demand_refresh);
    if let Some(reason) = trigger {
        let next_version = state.snapshot.version.saturating_add(1);
        let mut snapshot = build_snapshot(next_version);
        if snapshot.version < next_version {
            snapshot.version = next_version;
        }
        return RefreshOutcome {
            state: RefreshState {
                snapshot,
                updates_since_refresh: 0,
            },
            trigger: Some(reason),
        };
    }

    RefreshOutcome {
        state,
        trigger: None,
    }
}

pub fn encode_snapshot(snapshot: &StatsSnapshot) -> Vec<u8> {
    let bucket_count = snapshot.histogram_buckets.len() as u32;
    let mut encoded = Vec::with_capacity(22 + snapshot.histogram_buckets.len() * 16);

    encoded.extend_from_slice(&STATS_CODEC_MAGIC);
    encoded.extend_from_slice(&STATS_CODEC_VERSION.to_le_bytes());
    encoded.extend_from_slice(&snapshot.version.to_le_bytes());
    encoded.extend_from_slice(&snapshot.cardinality_estimate.to_le_bytes());
    encoded.extend_from_slice(&bucket_count.to_le_bytes());

    for bucket in &snapshot.histogram_buckets {
        encoded.extend_from_slice(&bucket.upper_bound.to_le_bytes());
        encoded.extend_from_slice(&bucket.row_count.to_le_bytes());
    }

    encoded
}

pub fn decode_snapshot(bytes: &[u8]) -> Result<StatsSnapshot, StatsCodecError> {
    let mut cursor = 0usize;
    let magic = read_array::<4>(bytes, &mut cursor)?;
    if magic != STATS_CODEC_MAGIC {
        return Err(StatsCodecError::InvalidMagic);
    }

    let codec_version = read_u16(bytes, &mut cursor)?;
    if codec_version != STATS_CODEC_VERSION {
        return Err(StatsCodecError::UnsupportedCodecVersion);
    }

    let version = read_u32(bytes, &mut cursor)?;
    if version == 0 {
        return Err(StatsCodecError::InvalidSnapshotVersion);
    }

    let cardinality_estimate = read_u64(bytes, &mut cursor)?;
    let bucket_count = read_u32(bytes, &mut cursor)? as usize;
    if bucket_count > MAX_HISTOGRAM_BUCKETS {
        return Err(StatsCodecError::TooManyBuckets);
    }

    let mut histogram_buckets = Vec::with_capacity(bucket_count);
    let mut previous_upper_bound = None;
    for _ in 0..bucket_count {
        let upper_bound = read_u64(bytes, &mut cursor)?;
        if let Some(previous) = previous_upper_bound
            && upper_bound <= previous
        {
            return Err(StatsCodecError::NonMonotonicBuckets);
        }
        previous_upper_bound = Some(upper_bound);
        histogram_buckets.push(HistogramBucket {
            upper_bound,
            row_count: read_u64(bytes, &mut cursor)?,
        });
    }

    if cursor != bytes.len() {
        return Err(StatsCodecError::UnexpectedTrailingBytes);
    }

    Ok(StatsSnapshot {
        version,
        histogram_buckets,
        cardinality_estimate,
    })
}

fn read_array<const N: usize>(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<[u8; N], StatsCodecError> {
    let end = cursor.saturating_add(N);
    let slice = bytes.get(*cursor..end).ok_or(StatsCodecError::Truncated)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    *cursor = end;
    Ok(out)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, StatsCodecError> {
    Ok(u16::from_le_bytes(read_array::<2>(bytes, cursor)?))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, StatsCodecError> {
    Ok(u32::from_le_bytes(read_array::<4>(bytes, cursor)?))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, StatsCodecError> {
    Ok(u64::from_le_bytes(read_array::<8>(bytes, cursor)?))
}

fn full_scan_cost(stats: &PlannerStats) -> u64 {
    stats
        .snapshot
        .cardinality_estimate
        .saturating_mul(FULL_SCAN_ROW_COST)
}

fn index_lookup_cost(stats: &PlannerStats) -> u64 {
    if !stats.index_available {
        return u64::MAX;
    }

    let bounded_selectivity = stats.selectivity.min(SELECTIVITY_SCALE);
    let estimated_rows =
        estimated_index_rows(stats.snapshot.cardinality_estimate, bounded_selectivity);
    INDEX_LOOKUP_SEEK_COST.saturating_add(estimated_rows.saturating_mul(INDEX_LOOKUP_ROW_COST))
}

fn estimated_index_rows(row_count: u64, selectivity: u32) -> u64 {
    if row_count == 0 {
        return 0;
    }

    let numerator = row_count.saturating_mul(selectivity as u64);
    let rounded_up = numerator.saturating_add((SELECTIVITY_SCALE - 1) as u64);
    let rows = rounded_up / SELECTIVITY_SCALE as u64;
    rows.max(1)
}

fn choose_plan(costs: PlanCosts) -> PlanKind {
    if costs.index_lookup <= costs.full_scan {
        PlanKind::IndexLookup
    } else {
        PlanKind::FullScan
    }
}

fn max_allowed_latency_ns(baseline_latency_ns: u64, policy: DriftGatePolicy) -> u64 {
    if baseline_latency_ns == 0 {
        return 0;
    }

    let drift_component = baseline_latency_ns
        .saturating_mul(policy.max_latency_drift_bps as u64)
        .saturating_add((SELECTIVITY_SCALE - 1) as u64)
        / SELECTIVITY_SCALE as u64;
    baseline_latency_ns.saturating_add(drift_component)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_snapshot(version: u32, cardinality_estimate: u64) -> StatsSnapshot {
        StatsSnapshot {
            version,
            cardinality_estimate,
            histogram_buckets: vec![
                HistogramBucket {
                    upper_bound: 100,
                    row_count: cardinality_estimate / 2,
                },
                HistogramBucket {
                    upper_bound: 1_000,
                    row_count: cardinality_estimate / 2,
                },
            ],
        }
    }

    #[test]
    fn chooses_index_lookup_when_index_is_available_and_more_selective() {
        let stats = PlannerStats {
            snapshot: test_snapshot(1, 1_000),
            selectivity: 100, // 1%
            index_available: true,
            stats_stale: false,
        };
        let output = explain(stats);

        assert_eq!(output.chosen_plan, PlanKind::IndexLookup);
        assert!(output.costs.index_lookup < output.costs.full_scan);
        assert_eq!(output.stats_version, 1);
        assert!(!output.stats_stale);
    }

    #[test]
    fn chooses_full_scan_when_index_is_not_available() {
        let stats = PlannerStats {
            snapshot: test_snapshot(7, 1_000),
            selectivity: 100,
            index_available: false,
            stats_stale: true,
        };
        let output = explain(stats);

        assert_eq!(output.chosen_plan, PlanKind::FullScan);
        assert_eq!(output.costs.index_lookup, u64::MAX);
        assert_eq!(output.stats_version, 7);
        assert!(output.stats_stale);
    }

    #[test]
    fn refresh_triggers_at_staleness_threshold() {
        let policy = RefreshPolicy {
            staleness_threshold: 8,
        };
        let state = RefreshState {
            snapshot: test_snapshot(3, 1_000),
            updates_since_refresh: 8,
        };
        let outcome = refresh_snapshot(state, policy, false, |next_version| {
            test_snapshot(next_version, 2_000)
        });

        assert_eq!(outcome.trigger, Some(RefreshTrigger::StalenessThreshold));
        assert_eq!(outcome.state.snapshot.version, 4);
        assert_eq!(outcome.state.snapshot.cardinality_estimate, 2_000);
        assert_eq!(outcome.state.updates_since_refresh, 0);
    }

    #[test]
    fn refresh_triggers_on_demand_before_threshold() {
        let policy = RefreshPolicy {
            staleness_threshold: 10,
        };
        let state = RefreshState {
            snapshot: test_snapshot(12, 800),
            updates_since_refresh: 1,
        };
        let outcome = refresh_snapshot(state, policy, true, |next_version| {
            test_snapshot(next_version, 900)
        });

        assert_eq!(outcome.trigger, Some(RefreshTrigger::OnDemand));
        assert_eq!(outcome.state.snapshot.version, 13);
        assert_eq!(outcome.state.snapshot.cardinality_estimate, 900);
        assert_eq!(outcome.state.updates_since_refresh, 0);
    }

    #[test]
    fn refresh_does_not_trigger_below_threshold_without_override() {
        let policy = RefreshPolicy {
            staleness_threshold: 10,
        };
        let state = RefreshState {
            snapshot: test_snapshot(5, 500),
            updates_since_refresh: 9,
        };
        let outcome = refresh_snapshot(state.clone(), policy, false, |next_version| {
            test_snapshot(next_version, 999)
        });

        assert_eq!(outcome.trigger, None);
        assert_eq!(outcome.state, state);
        assert!(!is_snapshot_stale(policy, 9));
        assert!(is_snapshot_stale(policy, 10));
    }

    #[test]
    fn persistence_roundtrip_and_deterministic_decode_validation() {
        let snapshot = StatsSnapshot {
            version: 11,
            cardinality_estimate: 4_096,
            histogram_buckets: vec![
                HistogramBucket {
                    upper_bound: 10,
                    row_count: 512,
                },
                HistogramBucket {
                    upper_bound: 100,
                    row_count: 2_048,
                },
            ],
        };
        let encoded = encode_snapshot(&snapshot);
        let decoded = decode_snapshot(&encoded).expect("roundtrip decode should succeed");
        assert_eq!(decoded, snapshot);

        let mut non_monotonic = encoded.clone();
        let bucket2_upper_bound_offset = 38usize;
        non_monotonic[bucket2_upper_bound_offset..bucket2_upper_bound_offset + 8]
            .copy_from_slice(&5u64.to_le_bytes());
        assert_eq!(
            decode_snapshot(&non_monotonic).unwrap_err(),
            StatsCodecError::NonMonotonicBuckets
        );

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            decode_snapshot(&trailing).unwrap_err(),
            StatsCodecError::UnexpectedTrailingBytes
        );
    }

    #[test]
    fn explain_is_stable_and_contains_stats_version_and_staleness_marker() {
        let stats = PlannerStats {
            snapshot: test_snapshot(9, 20),
            selectivity: 5_000, // estimated index rows = 10
            index_available: true,
            stats_stale: true,
        };
        let output = explain(stats.clone());

        assert_eq!(output.costs.index_lookup, output.costs.full_scan);
        assert_eq!(output.chosen_plan, PlanKind::IndexLookup);
        assert_eq!(output.stats_version, 9);
        assert!(output.stats_stale);
        assert_eq!(output.explain_schema_version, 2);
        assert_eq!(output.decision_basis, DecisionBasis::CostModelV1);

        for _ in 0..128 {
            assert_eq!(explain(stats.clone()), output);
        }
    }

    #[test]
    fn drift_gate_is_deterministic_for_missing_baseline() {
        let baselines = PlanBaselineRegistry::new();
        let policy = DriftGatePolicy {
            max_latency_drift_bps: 500,
        };
        let observed = DriftObservation {
            kind: PlanKind::IndexLookup,
            latency_ns: 1_000,
        };

        let expected = Err(DriftGateFailure::MissingBaseline {
            query_fingerprint: 0xBADC0FFE,
        });
        for _ in 0..128 {
            assert_eq!(
                evaluate_drift_gate(&baselines, 0xBADC0FFE, observed, policy),
                expected
            );
        }
    }

    #[test]
    fn drift_gate_fails_when_plan_kind_changes() {
        let mut baselines = PlanBaselineRegistry::new();
        baselines.upsert(
            7,
            PlanBaseline {
                kind: PlanKind::IndexLookup,
                latency_ns: 2_000,
            },
        );

        let result = evaluate_drift_gate(
            &baselines,
            7,
            DriftObservation {
                kind: PlanKind::FullScan,
                latency_ns: 1_900,
            },
            DriftGatePolicy {
                max_latency_drift_bps: 1_000,
            },
        );

        assert_eq!(
            result,
            Err(DriftGateFailure::PlanKindChanged {
                baseline: PlanKind::IndexLookup,
                observed: PlanKind::FullScan,
            })
        );
    }

    #[test]
    fn drift_gate_fails_when_latency_drift_exceeds_threshold() {
        let mut baselines = PlanBaselineRegistry::new();
        baselines.upsert(
            11,
            PlanBaseline {
                kind: PlanKind::IndexLookup,
                latency_ns: 1_000,
            },
        );

        let result = evaluate_drift_gate(
            &baselines,
            11,
            DriftObservation {
                kind: PlanKind::IndexLookup,
                latency_ns: 1_102,
            },
            DriftGatePolicy {
                max_latency_drift_bps: 1_000, // 10%
            },
        );

        assert_eq!(
            result,
            Err(DriftGateFailure::LatencyDriftExceeded {
                baseline_latency_ns: 1_000,
                observed_latency_ns: 1_102,
                allowed_max_latency_ns: 1_100,
            })
        );
    }

    #[test]
    fn drift_gate_allows_plan_when_latency_drift_is_within_threshold() {
        let mut baselines = PlanBaselineRegistry::new();
        baselines.upsert(
            19,
            PlanBaseline {
                kind: PlanKind::FullScan,
                latency_ns: 2_500,
            },
        );

        let result = evaluate_drift_gate(
            &baselines,
            19,
            DriftObservation {
                kind: PlanKind::FullScan,
                latency_ns: 2_625,
            },
            DriftGatePolicy {
                max_latency_drift_bps: 500, // 5%
            },
        );

        assert_eq!(result, Ok(()));
    }
}
