use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsistencyWindowV1 {
    pub max_staleness_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffContinuityEvidenceV1 {
    pub island_id: u64,
    pub from_epoch: u64,
    pub to_epoch: u64,
    pub from_tick: u64,
    pub to_tick: u64,
    pub handoff_latency_ms: u64,
}

pub fn verify_handoff_continuity(
    evidence: HandoffContinuityEvidenceV1,
    window: ConsistencyWindowV1,
) -> Result<(), String> {
    if evidence.to_epoch != evidence.from_epoch.saturating_add(1) {
        return Err(format!(
            "handoff epoch continuity violated for island {}: from {}, to {}",
            evidence.island_id, evidence.from_epoch, evidence.to_epoch
        ));
    }
    if evidence.to_tick < evidence.from_tick {
        return Err(format!(
            "handoff tick regressed for island {}: from {}, to {}",
            evidence.island_id, evidence.from_tick, evidence.to_tick
        ));
    }
    if evidence.handoff_latency_ms > window.max_staleness_ms {
        return Err(format!(
            "handoff staleness exceeded for island {}: latency {} > {}",
            evidence.island_id, evidence.handoff_latency_ms, window.max_staleness_ms
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConsistencyWindowV1, HandoffContinuityEvidenceV1, verify_handoff_continuity};

    #[test]
    fn continuity_accepts_valid_handoff() {
        let evidence = HandoffContinuityEvidenceV1 {
            island_id: 5,
            from_epoch: 1,
            to_epoch: 2,
            from_tick: 900,
            to_tick: 901,
            handoff_latency_ms: 15,
        };
        verify_handoff_continuity(
            evidence,
            ConsistencyWindowV1 {
                max_staleness_ms: 25,
            },
        )
        .expect("handoff should be valid");
    }

    #[test]
    fn continuity_rejects_epoch_gap() {
        let evidence = HandoffContinuityEvidenceV1 {
            island_id: 5,
            from_epoch: 1,
            to_epoch: 3,
            from_tick: 900,
            to_tick: 901,
            handoff_latency_ms: 10,
        };
        let err = verify_handoff_continuity(
            evidence,
            ConsistencyWindowV1 {
                max_staleness_ms: 25,
            },
        )
        .expect_err("handoff must fail");
        assert!(err.contains("epoch continuity"));
    }
}
