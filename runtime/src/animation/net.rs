const RECONCILE_SMOOTHING_DIVISOR: i64 = 4;
const MAX_APPLIED_DELTA_Q16: i64 = 4 * 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetPhaseState {
    pub offset_q16: i32,
    pub last_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseSample {
    pub local_tick: u64,
    pub authoritative_tick: u64,
    pub authoritative_phase_q16: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileResult {
    pub error_q16: i32,
    pub applied_delta_q16: i32,
    pub next_offset_q16: i32,
}

pub fn reconcile_phase(state: &mut NetPhaseState, sample: PhaseSample) -> ReconcileResult {
    let tick_delta_q16 =
        ((sample.authoritative_tick as i128 - sample.local_tick as i128) * 65_536) as i64;
    let raw_error_q16 =
        tick_delta_q16 + i64::from(sample.authoritative_phase_q16) - i64::from(state.offset_q16);
    let smoothed_delta_q16 = raw_error_q16 / RECONCILE_SMOOTHING_DIVISOR;
    let clamped_delta_q16 = smoothed_delta_q16.clamp(-MAX_APPLIED_DELTA_Q16, MAX_APPLIED_DELTA_Q16);

    state.offset_q16 = state.offset_q16.saturating_add(clamped_delta_q16 as i32);
    state.last_tick = sample.local_tick.max(state.last_tick);

    ReconcileResult {
        error_q16: raw_error_q16.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        applied_delta_q16: clamped_delta_q16 as i32,
        next_offset_q16: state.offset_q16,
    }
}

#[cfg(test)]
mod tests {
    use super::{NetPhaseState, PhaseSample, reconcile_phase};

    #[test]
    fn phase_reconcile_deterministic() {
        let samples = [
            PhaseSample {
                local_tick: 100,
                authoritative_tick: 102,
                authoritative_phase_q16: 8_192,
            },
            PhaseSample {
                local_tick: 101,
                authoritative_tick: 103,
                authoritative_phase_q16: 12_288,
            },
            PhaseSample {
                local_tick: 102,
                authoritative_tick: 103,
                authoritative_phase_q16: 4_096,
            },
            PhaseSample {
                local_tick: 103,
                authoritative_tick: 104,
                authoritative_phase_q16: 7_168,
            },
        ];

        let mut state_a = NetPhaseState::default();
        let mut state_b = NetPhaseState::default();

        let mut trace_a = Vec::with_capacity(samples.len());
        let mut trace_b = Vec::with_capacity(samples.len());
        for sample in samples {
            trace_a.push(reconcile_phase(&mut state_a, sample));
        }
        for sample in samples {
            trace_b.push(reconcile_phase(&mut state_b, sample));
        }

        assert_eq!(trace_a, trace_b);
        assert_eq!(state_a, state_b);
    }
}
