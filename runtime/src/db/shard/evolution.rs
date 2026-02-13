use crate::db::schema_evolution::{
    BackfillProgress, CutoverReadiness, CutoverReadinessGateInput, ReindexWorker,
    ReindexWorkerConfig, ReindexWorkerState, RemediationAction, ValidationMismatch,
    evaluate_cutover_readiness,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionPlanInput {
    pub relation: String,
    pub from_key: String,
    pub to_key: String,
    pub current_qps: u64,
    pub current_write_qps: u64,
    pub estimated_backfill_rows: u64,
    pub estimated_distinct_new_key_values: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionRisk {
    pub score_per_mille: u64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionPlan {
    pub relation: String,
    pub from_key: String,
    pub to_key: String,
    pub predicted_copy_seconds: u64,
    pub predicted_dual_write_overhead_per_mille: u64,
    pub risk: EvolutionRisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPhase {
    Copy,
    DualWrite,
    Cutover,
    Rollback,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionState {
    pub relation: String,
    pub phase: ExecutionPhase,
    pub copied_rows: u64,
    pub total_rows: u64,
    pub dual_write_acks: u64,
    pub cutover_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvolutionError {
    InvalidKeySpec,
    InvalidTransition,
}

pub fn plan(input: &EvolutionPlanInput) -> Result<EvolutionPlan, EvolutionError> {
    if input.from_key.trim().is_empty() || input.to_key.trim().is_empty() {
        return Err(EvolutionError::InvalidKeySpec);
    }
    if input.from_key == input.to_key {
        return Err(EvolutionError::InvalidKeySpec);
    }

    let copy_rows_per_second = (input.current_qps / 4).max(1);
    let predicted_copy_seconds = input
        .estimated_backfill_rows
        .saturating_add(copy_rows_per_second - 1)
        .saturating_div(copy_rows_per_second);

    let dual_write_overhead = if input.current_write_qps == 0 {
        0
    } else {
        (input.current_write_qps / 2).min(1000)
    };

    let mut reasons = Vec::new();
    let mut score = 0u64;
    if predicted_copy_seconds > 3600 {
        reasons.push("copy window exceeds 1 hour".to_string());
        score = score.saturating_add(350);
    }
    if dual_write_overhead > 450 {
        reasons.push("dual-write overhead exceeds 45%".to_string());
        score = score.saturating_add(300);
    }
    if input.estimated_distinct_new_key_values < (input.estimated_backfill_rows / 100).max(1) {
        reasons.push("new shard key appears low-cardinality for row volume".to_string());
        score = score.saturating_add(250);
    }

    Ok(EvolutionPlan {
        relation: input.relation.clone(),
        from_key: input.from_key.clone(),
        to_key: input.to_key.clone(),
        predicted_copy_seconds,
        predicted_dual_write_overhead_per_mille: dual_write_overhead,
        risk: EvolutionRisk {
            score_per_mille: score.min(1000),
            reasons,
        },
    })
}

pub fn simulate(plan: &EvolutionPlan) -> BTreeMap<&'static str, u64> {
    let mut out = BTreeMap::new();
    out.insert("predicted_copy_seconds", plan.predicted_copy_seconds);
    out.insert(
        "predicted_dual_write_overhead_per_mille",
        plan.predicted_dual_write_overhead_per_mille,
    );
    out.insert("risk_score_per_mille", plan.risk.score_per_mille);
    out
}

impl ExecutionState {
    pub fn new(relation: impl Into<String>, total_rows: u64) -> Self {
        Self {
            relation: relation.into(),
            phase: ExecutionPhase::Copy,
            copied_rows: 0,
            total_rows,
            dual_write_acks: 0,
            cutover_epoch: 0,
        }
    }

    pub fn advance_copy(&mut self, rows: u64) -> Result<(), EvolutionError> {
        if self.phase != ExecutionPhase::Copy {
            return Err(EvolutionError::InvalidTransition);
        }
        self.copied_rows = self.copied_rows.saturating_add(rows).min(self.total_rows);
        if self.copied_rows == self.total_rows {
            self.phase = ExecutionPhase::DualWrite;
        }
        Ok(())
    }

    pub fn ack_dual_write(&mut self, n: u64) -> Result<(), EvolutionError> {
        if self.phase != ExecutionPhase::DualWrite {
            return Err(EvolutionError::InvalidTransition);
        }
        self.dual_write_acks = self.dual_write_acks.saturating_add(n);
        Ok(())
    }

    pub fn cutover(&mut self, epoch: u64) -> Result<(), EvolutionError> {
        if self.phase != ExecutionPhase::DualWrite {
            return Err(EvolutionError::InvalidTransition);
        }
        self.phase = ExecutionPhase::Cutover;
        self.cutover_epoch = epoch;
        Ok(())
    }

    pub fn finalize(&mut self) -> Result<(), EvolutionError> {
        if self.phase != ExecutionPhase::Cutover {
            return Err(EvolutionError::InvalidTransition);
        }
        self.phase = ExecutionPhase::Complete;
        Ok(())
    }

    pub fn rollback(&mut self) {
        self.phase = ExecutionPhase::Rollback;
    }
}

pub fn evaluate_readiness(
    backfill_total_rows: u64,
    backfill_completed_rows: u64,
    in_flight_reindex_rows: u64,
    mismatches: Vec<ValidationMismatch>,
    pending_actions: Vec<RemediationAction>,
) -> CutoverReadiness {
    let mut progress = BackfillProgress::new(backfill_total_rows);
    progress
        .record_completed_rows(backfill_completed_rows.min(backfill_total_rows))
        .expect("bounded backfill");

    let worker = ReindexWorker::new(ReindexWorkerConfig::new(512, 4096));
    let mut state = ReindexWorkerState::new();
    let mut remaining = in_flight_reindex_rows;
    while remaining > 0 {
        let step = worker.step(&mut state, in_flight_reindex_rows);
        if step.assigned_rows() == 0 {
            break;
        }
        let ack = step.assigned_rows().min(remaining / 2 + 1);
        state.ack_completed_rows(ack).expect("ack");
        remaining = remaining.saturating_sub(ack);
    }

    let input = CutoverReadinessGateInput {
        backfill_progress: progress,
        reindex_state: state,
        pending_mismatches: mismatches,
        pending_actions,
    };
    evaluate_cutover_readiness(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_simulator_and_executor_are_deterministic() {
        let p = plan(&EvolutionPlanInput {
            relation: "orders".to_string(),
            from_key: "region".to_string(),
            to_key: "region,user_id".to_string(),
            current_qps: 8_000,
            current_write_qps: 2_000,
            estimated_backfill_rows: 1_600_000,
            estimated_distinct_new_key_values: 20_000,
        })
        .expect("plan");
        let sim = simulate(&p);
        assert!(sim["predicted_copy_seconds"] > 0);

        let mut st = ExecutionState::new("orders", 100);
        st.advance_copy(99).expect("copy");
        assert_eq!(st.phase, ExecutionPhase::Copy);
        st.advance_copy(1).expect("copy complete");
        assert_eq!(st.phase, ExecutionPhase::DualWrite);
        st.ack_dual_write(42).expect("dual write");
        st.cutover(7).expect("cutover");
        st.finalize().expect("finalize");
        assert_eq!(st.phase, ExecutionPhase::Complete);
    }
}
