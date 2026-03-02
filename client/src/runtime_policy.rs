#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedStepPlan {
    pub next_last_now_ms: Option<f64>,
    pub next_accumulator_ms: f64,
    pub substeps: u32,
}

pub fn plan_fixed_step_update(
    last_now_ms: Option<f64>,
    accumulator_ms: f64,
    now_ms: f64,
    step_ms: f64,
    max_substeps_per_frame: u32,
) -> FixedStepPlan {
    if !now_ms.is_finite() || !step_ms.is_finite() || step_ms <= 0.0 {
        return FixedStepPlan {
            next_last_now_ms: last_now_ms,
            next_accumulator_ms: accumulator_ms.max(0.0),
            substeps: 0,
        };
    }
    let max_substeps = max_substeps_per_frame.max(1);
    let Some(previous_now_ms) = last_now_ms else {
        return FixedStepPlan {
            next_last_now_ms: Some(now_ms),
            next_accumulator_ms: accumulator_ms.max(0.0),
            substeps: 0,
        };
    };

    let elapsed_ms = (now_ms - previous_now_ms).max(0.0);
    let max_accumulated_ms = step_ms * f64::from(max_substeps);
    let mut accumulated_ms = (accumulator_ms.max(0.0) + elapsed_ms).min(max_accumulated_ms);
    let mut substeps = 0u32;
    while accumulated_ms + f64::EPSILON >= step_ms && substeps < max_substeps {
        accumulated_ms = (accumulated_ms - step_ms).max(0.0);
        substeps = substeps.saturating_add(1);
    }

    FixedStepPlan {
        next_last_now_ms: Some(now_ms),
        next_accumulator_ms: accumulated_ms,
        substeps,
    }
}

pub fn should_publish_heavy_metrics(
    last_heavy_publish_ms: f64,
    now_ms: f64,
    throttle_ms: f64,
    force: bool,
) -> bool {
    if force {
        return true;
    }
    if !now_ms.is_finite() || !throttle_ms.is_finite() || throttle_ms <= 0.0 {
        return false;
    }
    last_heavy_publish_ms <= 0.0 || now_ms - last_heavy_publish_ms >= throttle_ms
}

pub fn reconcile_pending_inputs<T, F>(
    pending_inputs: &mut Vec<T>,
    ack: u64,
    authoritative_applied: bool,
    seq_of: F,
) -> Vec<T>
where
    T: Clone,
    F: Fn(&T) -> u64,
{
    pending_inputs.retain(|input| seq_of(input) > ack);
    if authoritative_applied {
        pending_inputs.clone()
    } else {
        Vec::new()
    }
}

pub fn queue_pending_input_after_send<T>(
    pending_inputs: &mut Vec<T>,
    input: T,
    send_succeeded: bool,
) {
    if send_succeeded {
        pending_inputs.push(input);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        plan_fixed_step_update, queue_pending_input_after_send, reconcile_pending_inputs,
        should_publish_heavy_metrics,
    };

    const STEP_MS: f64 = 16.0;
    const MAX_SUBSTEPS: u32 = 8;

    #[derive(Clone, Debug)]
    struct PendingMock {
        seq: u64,
    }

    fn simulate_substeps_for_profile(frame_ms: f64, frames: usize) -> u64 {
        let mut last_now = None::<f64>;
        let mut accumulator = 0.0_f64;
        let mut now = 0.0_f64;
        let mut total_substeps = 0_u64;
        for _ in 0..frames {
            now += frame_ms;
            let plan = plan_fixed_step_update(last_now, accumulator, now, STEP_MS, MAX_SUBSTEPS);
            last_now = plan.next_last_now_ms;
            accumulator = plan.next_accumulator_ms;
            total_substeps = total_substeps.saturating_add(u64::from(plan.substeps));
        }
        total_substeps
    }

    #[test]
    fn fixed_step_plan_processes_expected_substeps_for_100ms_jump() {
        let init = plan_fixed_step_update(None, 0.0, 0.0, STEP_MS, MAX_SUBSTEPS);
        assert_eq!(init.substeps, 0);
        assert_eq!(init.next_last_now_ms, Some(0.0));

        let plan = plan_fixed_step_update(
            init.next_last_now_ms,
            init.next_accumulator_ms,
            100.0,
            STEP_MS,
            MAX_SUBSTEPS,
        );
        assert_eq!(plan.substeps, 6);
        assert!((plan.next_accumulator_ms - 4.0).abs() < 0.0001);
    }

    #[test]
    fn fixed_step_plan_is_cadence_invariant_across_30_60_120fps_profiles() {
        let substeps_30 = simulate_substeps_for_profile(1000.0 / 30.0, 30);
        let substeps_60 = simulate_substeps_for_profile(1000.0 / 60.0, 60);
        let substeps_120 = simulate_substeps_for_profile(1000.0 / 120.0, 120);

        let max_substeps = substeps_30.max(substeps_60).max(substeps_120);
        let min_substeps = substeps_30.min(substeps_60).min(substeps_120);
        assert!(
            max_substeps - min_substeps <= 1,
            "substeps should be cadence-invariant: 30fps={substeps_30} 60fps={substeps_60} 120fps={substeps_120}"
        );

        let movement_per_tick = 240.0 * (STEP_MS / 1000.0);
        let d30 = substeps_30 as f64 * movement_per_tick;
        let d60 = substeps_60 as f64 * movement_per_tick;
        let d120 = substeps_120 as f64 * movement_per_tick;
        let max_d = d30.max(d60).max(d120);
        let min_d = d30.min(d60).min(d120);
        assert!(
            (max_d - min_d) <= movement_per_tick + 1e-6,
            "movement parity should hold across cadences: d30={d30} d60={d60} d120={d120}"
        );
    }

    #[test]
    fn fixed_step_plan_is_cadence_invariant_across_additional_profiles() {
        let profiles = [
            (1000.0 / 24.0, 24_usize),
            (1000.0 / 30.0, 30_usize),
            (1000.0 / 48.0, 48_usize),
            (1000.0 / 60.0, 60_usize),
            (1000.0 / 90.0, 90_usize),
            (1000.0 / 120.0, 120_usize),
        ];

        let mut totals = Vec::new();
        for (frame_ms, frames) in profiles {
            totals.push(simulate_substeps_for_profile(frame_ms, frames));
        }

        let max = totals.iter().copied().max().unwrap_or(0);
        let min = totals.iter().copied().min().unwrap_or(0);
        assert!(
            max - min <= 2,
            "substeps should remain cadence-invariant across profiles: {totals:?}"
        );
    }

    #[test]
    fn fixed_step_plan_clamps_to_max_substeps_per_frame() {
        let init = plan_fixed_step_update(None, 0.0, 0.0, STEP_MS, MAX_SUBSTEPS);
        let plan = plan_fixed_step_update(
            init.next_last_now_ms,
            init.next_accumulator_ms,
            1_000.0,
            STEP_MS,
            MAX_SUBSTEPS,
        );
        assert_eq!(plan.substeps, MAX_SUBSTEPS);
        assert!(plan.next_accumulator_ms <= STEP_MS + 1e-6);
    }

    #[test]
    fn fixed_step_plan_ignores_non_finite_or_invalid_parameters() {
        let plan_nan_now = plan_fixed_step_update(Some(10.0), 5.0, f64::NAN, STEP_MS, MAX_SUBSTEPS);
        assert_eq!(plan_nan_now.substeps, 0);
        assert_eq!(plan_nan_now.next_last_now_ms, Some(10.0));
        assert_eq!(plan_nan_now.next_accumulator_ms, 5.0);

        let plan_nan_step =
            plan_fixed_step_update(Some(10.0), 5.0, 12.0, f64::NAN, MAX_SUBSTEPS);
        assert_eq!(plan_nan_step.substeps, 0);
        assert_eq!(plan_nan_step.next_last_now_ms, Some(10.0));
        assert_eq!(plan_nan_step.next_accumulator_ms, 5.0);

        let plan_zero_step = plan_fixed_step_update(Some(10.0), 5.0, 12.0, 0.0, MAX_SUBSTEPS);
        assert_eq!(plan_zero_step.substeps, 0);
        assert_eq!(plan_zero_step.next_last_now_ms, Some(10.0));
        assert_eq!(plan_zero_step.next_accumulator_ms, 5.0);
    }

    #[test]
    fn fixed_step_plan_clamps_negative_elapsed_time_and_negative_accumulator() {
        let plan = plan_fixed_step_update(Some(100.0), -8.0, 90.0, STEP_MS, MAX_SUBSTEPS);
        assert_eq!(plan.substeps, 0);
        assert_eq!(plan.next_last_now_ms, Some(90.0));
        assert_eq!(plan.next_accumulator_ms, 0.0);
    }

    #[test]
    fn pending_reconciliation_replays_only_unacked_inputs_after_authoritative_update() {
        let mut pending = vec![
            PendingMock { seq: 1 },
            PendingMock { seq: 2 },
            PendingMock { seq: 3 },
        ];
        let replay = reconcile_pending_inputs(&mut pending, 2, true, |input| input.seq);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].seq, 3);
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, 3);
    }

    #[test]
    fn pending_reconciliation_ack_only_prunes_without_replay() {
        let mut pending = vec![PendingMock { seq: 1 }, PendingMock { seq: 3 }];
        let replay = reconcile_pending_inputs(&mut pending, 1, false, |input| input.seq);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].seq, 3);
        assert!(replay.is_empty());
    }

    #[test]
    fn pending_reconciliation_prunes_all_when_ack_catches_up() {
        let mut pending = vec![
            PendingMock { seq: 10 },
            PendingMock { seq: 11 },
            PendingMock { seq: 12 },
        ];
        let replay = reconcile_pending_inputs(&mut pending, 12, true, |input| input.seq);
        assert!(pending.is_empty());
        assert!(replay.is_empty());
    }

    #[test]
    fn pending_reconciliation_preserves_unacked_order_in_replay() {
        let mut pending = vec![
            PendingMock { seq: 2 },
            PendingMock { seq: 5 },
            PendingMock { seq: 7 },
            PendingMock { seq: 9 },
        ];
        let replay = reconcile_pending_inputs(&mut pending, 4, true, |input| input.seq);
        let replay_seqs: Vec<u64> = replay.into_iter().map(|item| item.seq).collect();
        let pending_seqs: Vec<u64> = pending.into_iter().map(|item| item.seq).collect();
        assert_eq!(replay_seqs, vec![5, 7, 9]);
        assert_eq!(pending_seqs, vec![5, 7, 9]);
    }

    #[test]
    fn queue_pending_input_requires_successful_send() {
        let mut pending = vec![PendingMock { seq: 1 }];
        queue_pending_input_after_send(&mut pending, PendingMock { seq: 2 }, false);
        assert_eq!(pending.len(), 1);
        queue_pending_input_after_send(&mut pending, PendingMock { seq: 2 }, true);
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[1].seq, 2);
    }

    #[test]
    fn heavy_metrics_publication_is_throttled_unless_forced() {
        assert!(should_publish_heavy_metrics(0.0, 0.0, 200.0, false));
        assert!(!should_publish_heavy_metrics(1000.0, 1100.0, 200.0, false));
        assert!(should_publish_heavy_metrics(1000.0, 1200.0, 200.0, false));
        assert!(should_publish_heavy_metrics(1000.0, 1050.0, 200.0, true));
    }

    #[test]
    fn heavy_metrics_publication_rejects_invalid_timing_inputs() {
        assert!(!should_publish_heavy_metrics(1000.0, f64::NAN, 200.0, false));
        assert!(!should_publish_heavy_metrics(1000.0, 1200.0, f64::NAN, false));
        assert!(!should_publish_heavy_metrics(1000.0, 1200.0, 0.0, false));
    }
}
