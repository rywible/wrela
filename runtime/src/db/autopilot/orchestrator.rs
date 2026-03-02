use std::collections::VecDeque;

pub const DEFAULT_AUDIT_RING_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStep {
    Simulate,
    Compliance,
    Apply,
    Verify,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStepStatus {
    Passed,
    Blocked,
    Noop,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionReasonCode {
    SimulationPass,
    IntentConflictDetected,
    HotMetaGuardrailExceeded,
    TieringBoundaryViolation,
    UnsafeActionFailClosed,
    ApplyNoop,
    VerifyPass,
    RollbackNotRequired,
}

impl ActionReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SimulationPass => "SIMULATION_PASS",
            Self::IntentConflictDetected => "INTENT_CONFLICT_DETECTED",
            Self::HotMetaGuardrailExceeded => "HOTMETA_GUARDRAIL_EXCEEDED",
            Self::TieringBoundaryViolation => "TIERING_BOUNDARY_VIOLATION",
            Self::UnsafeActionFailClosed => "UNSAFE_ACTION_FAIL_CLOSED",
            Self::ApplyNoop => "APPLY_NOOP",
            Self::VerifyPass => "VERIFY_PASS",
            Self::RollbackNotRequired => "ROLLBACK_NOT_REQUIRED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionStepResult {
    pub step: ActionStep,
    pub status: ActionStepStatus,
    pub reason_code: ActionReasonCode,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentEffective {
    pub mode: String,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub autoscale_enabled: bool,
    pub active_groups: u32,
    pub logical_shards: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentConflict {
    pub reason_code: ActionReasonCode,
    pub blocking: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TieringState {
    pub active_tier: String,
    pub within_policy_boundary: bool,
    pub observed_live_bytes: u64,
    pub boundary_min_live_bytes: u64,
    pub boundary_max_live_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    pub reason_code: ActionReasonCode,
    pub severity: u8,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutopilotAuditRow {
    pub action_id: u64,
    pub source: String,
    pub epoch_ms: u64,
    pub final_reason_code: ActionReasonCode,
    pub summary: String,
    pub steps: Vec<ActionStepResult>,
}

#[derive(Debug, Clone)]
pub struct AuditRingBuffer {
    capacity: usize,
    rows: VecDeque<AutopilotAuditRow>,
}

impl AuditRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            rows: VecDeque::new(),
        }
    }

    pub fn push(&mut self, row: AutopilotAuditRow) {
        self.rows.push_back(row);
        while self.rows.len() > self.capacity {
            let _ = self.rows.pop_front();
        }
    }

    pub fn recent(&self, limit: usize) -> Vec<AutopilotAuditRow> {
        let limit = limit.max(1);
        self.rows.iter().rev().take(limit).cloned().collect()
    }
}

impl Default for AuditRingBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_AUDIT_RING_CAPACITY)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerInput {
    pub action_id: u64,
    pub source: String,
    pub now_epoch_ms: u64,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub autoscale_enabled: bool,
    pub active_groups: u32,
    pub logical_shards: u32,
    pub observed_live_bytes: u64,
    pub hot_meta_write_ops: u64,
    pub hot_meta_max_write_ops: u64,
    pub tiering_boundary_min_live_bytes: u64,
    pub tiering_boundary_max_live_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerOutput {
    pub intent_effective: IntentEffective,
    pub intent_conflicts: Vec<IntentConflict>,
    pub tiering_state: TieringState,
    pub recommendations: Vec<Recommendation>,
    pub audit_row: AutopilotAuditRow,
}

pub fn execute_controller_tick(input: ControllerInput) -> ControllerOutput {
    let intent_effective = IntentEffective {
        mode: "full_auto".to_string(),
        replication_factor: input.replication_factor,
        write_quorum: input.write_quorum,
        autoscale_enabled: input.autoscale_enabled,
        active_groups: input.active_groups,
        logical_shards: input.logical_shards,
    };

    let (active_tier, within_policy_boundary, tier_reason) =
        if input.observed_live_bytes < input.tiering_boundary_min_live_bytes {
            (
                "hot".to_string(),
                false,
                format!(
                    "observed live bytes {} below tiering minimum {}",
                    input.observed_live_bytes, input.tiering_boundary_min_live_bytes
                ),
            )
        } else if input.observed_live_bytes > input.tiering_boundary_max_live_bytes {
            (
                "cold".to_string(),
                false,
                format!(
                    "observed live bytes {} above tiering maximum {}",
                    input.observed_live_bytes, input.tiering_boundary_max_live_bytes
                ),
            )
        } else {
            (
                "warm".to_string(),
                true,
                "tiering within policy boundary".to_string(),
            )
        };

    let tiering_state = TieringState {
        active_tier,
        within_policy_boundary,
        observed_live_bytes: input.observed_live_bytes,
        boundary_min_live_bytes: input.tiering_boundary_min_live_bytes,
        boundary_max_live_bytes: input.tiering_boundary_max_live_bytes,
        reason: tier_reason,
    };

    let mut conflicts = Vec::new();
    if input.write_quorum == 0 || input.write_quorum > input.replication_factor.max(1) {
        conflicts.push(IntentConflict {
            reason_code: ActionReasonCode::IntentConflictDetected,
            blocking: true,
            reason: format!(
                "write quorum {} exceeds replication factor {}",
                input.write_quorum,
                input.replication_factor.max(1)
            ),
        });
    }
    if input.hot_meta_write_ops > input.hot_meta_max_write_ops {
        conflicts.push(IntentConflict {
            reason_code: ActionReasonCode::HotMetaGuardrailExceeded,
            blocking: true,
            reason: format!(
                "hot meta write ops {} exceed guardrail {}",
                input.hot_meta_write_ops, input.hot_meta_max_write_ops
            ),
        });
    }
    if !tiering_state.within_policy_boundary {
        conflicts.push(IntentConflict {
            reason_code: ActionReasonCode::TieringBoundaryViolation,
            blocking: true,
            reason: tiering_state.reason.clone(),
        });
    }

    let mut recommendations = Vec::new();
    if conflicts.is_empty() {
        recommendations.push(Recommendation {
            reason_code: ActionReasonCode::SimulationPass,
            severity: 0,
            summary: "no-op apply path is healthy".to_string(),
        });
    } else {
        for conflict in &conflicts {
            recommendations.push(Recommendation {
                reason_code: conflict.reason_code,
                severity: if conflict.blocking { 2 } else { 1 },
                summary: conflict.reason.clone(),
            });
        }
    }

    let mut steps = Vec::new();
    if conflicts.is_empty() {
        steps.push(ActionStepResult {
            step: ActionStep::Simulate,
            status: ActionStepStatus::Passed,
            reason_code: ActionReasonCode::SimulationPass,
            reason: "simulation passed".to_string(),
        });
        steps.push(ActionStepResult {
            step: ActionStep::Compliance,
            status: ActionStepStatus::Passed,
            reason_code: ActionReasonCode::SimulationPass,
            reason: "compliance checks passed".to_string(),
        });
        steps.push(ActionStepResult {
            step: ActionStep::Apply,
            status: ActionStepStatus::Noop,
            reason_code: ActionReasonCode::ApplyNoop,
            reason: "deterministic no-op scaffolding apply".to_string(),
        });
    } else {
        let compliance_reason = conflicts
            .first()
            .map(|conflict| conflict.reason.clone())
            .unwrap_or_else(|| "intent conflict detected".to_string());
        steps.push(ActionStepResult {
            step: ActionStep::Simulate,
            status: ActionStepStatus::Blocked,
            reason_code: ActionReasonCode::IntentConflictDetected,
            reason: "simulation surfaced intent conflicts".to_string(),
        });
        steps.push(ActionStepResult {
            step: ActionStep::Compliance,
            status: ActionStepStatus::Blocked,
            reason_code: conflicts
                .first()
                .map(|conflict| conflict.reason_code)
                .unwrap_or(ActionReasonCode::IntentConflictDetected),
            reason: compliance_reason,
        });
        steps.push(ActionStepResult {
            step: ActionStep::Apply,
            status: ActionStepStatus::Blocked,
            reason_code: ActionReasonCode::UnsafeActionFailClosed,
            reason: "unsafe apply path blocked (fail closed)".to_string(),
        });
    }

    steps.push(ActionStepResult {
        step: ActionStep::Verify,
        status: ActionStepStatus::Passed,
        reason_code: ActionReasonCode::VerifyPass,
        reason: "verify completed".to_string(),
    });
    steps.push(ActionStepResult {
        step: ActionStep::Rollback,
        status: ActionStepStatus::Skipped,
        reason_code: ActionReasonCode::RollbackNotRequired,
        reason: "rollback not required".to_string(),
    });

    let final_reason_code = if conflicts.is_empty() {
        ActionReasonCode::ApplyNoop
    } else {
        ActionReasonCode::UnsafeActionFailClosed
    };
    let summary = if conflicts.is_empty() {
        "autopilot completed deterministic no-op".to_string()
    } else {
        "autopilot blocked unsafe action (fail closed)".to_string()
    };

    ControllerOutput {
        intent_effective,
        intent_conflicts: conflicts,
        tiering_state,
        recommendations,
        audit_row: AutopilotAuditRow {
            action_id: input.action_id,
            source: input.source,
            epoch_ms: input.now_epoch_ms,
            final_reason_code,
            summary,
            steps,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_input() -> ControllerInput {
        ControllerInput {
            action_id: 1,
            source: "test".to_string(),
            now_epoch_ms: 100,
            replication_factor: 3,
            write_quorum: 2,
            autoscale_enabled: true,
            active_groups: 2,
            logical_shards: 16,
            observed_live_bytes: 128 * 1024 * 1024,
            hot_meta_write_ops: 10,
            hot_meta_max_write_ops: 100,
            tiering_boundary_min_live_bytes: 64 * 1024 * 1024,
            tiering_boundary_max_live_bytes: 512 * 1024 * 1024,
        }
    }

    #[test]
    fn controller_stays_noop_when_guards_pass() {
        let output = execute_controller_tick(safe_input());
        assert!(output.intent_conflicts.is_empty());
        assert_eq!(
            output.audit_row.final_reason_code,
            ActionReasonCode::ApplyNoop
        );
        assert_eq!(
            output
                .audit_row
                .steps
                .iter()
                .find(|step| step.step == ActionStep::Apply)
                .expect("apply step")
                .status,
            ActionStepStatus::Noop
        );
    }

    #[test]
    fn controller_fails_closed_when_intent_is_unsafe() {
        let mut input = safe_input();
        input.write_quorum = 4;

        let output = execute_controller_tick(input);
        assert!(!output.intent_conflicts.is_empty());
        assert!(
            output
                .intent_conflicts
                .iter()
                .any(|conflict| conflict.blocking),
            "expected blocking conflict"
        );
        assert_eq!(
            output.audit_row.final_reason_code,
            ActionReasonCode::UnsafeActionFailClosed
        );
    }

    #[test]
    fn audit_ring_buffer_keeps_latest_rows() {
        let mut ring = AuditRingBuffer::new(2);
        for id in 1..=3 {
            ring.push(AutopilotAuditRow {
                action_id: id,
                source: "test".to_string(),
                epoch_ms: id,
                final_reason_code: ActionReasonCode::ApplyNoop,
                summary: "row".to_string(),
                steps: Vec::new(),
            });
        }

        let rows = ring.recent(8);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action_id, 3);
        assert_eq!(rows[1].action_id, 2);
    }
}
