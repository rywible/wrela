use crate::presentation_contract::{RealtimeQualityContract, RealtimeQualityState};
use crate::presentation_exec::cost::PresentationFrameCostReport;

#[derive(Debug, Clone, PartialEq)]
pub struct AdaptivePresentationController {
    contract: RealtimeQualityContract,
    quality: RealtimeQualityState,
    moving_average_window: usize,
    frame_time_samples_ms: Vec<f32>,
    recovery_streak: u32,
}

impl AdaptivePresentationController {
    pub fn new(contract: RealtimeQualityContract) -> Self {
        Self {
            quality: contract.initial_state(),
            contract,
            moving_average_window: 6,
            frame_time_samples_ms: Vec::new(),
            recovery_streak: 0,
        }
    }

    pub fn with_window(mut self, moving_average_window: usize) -> Self {
        self.moving_average_window = moving_average_window.max(1);
        self
    }

    pub fn contract(&self) -> &RealtimeQualityContract {
        &self.contract
    }

    pub fn quality(&self) -> &RealtimeQualityState {
        &self.quality
    }

    pub fn observe_frame(&mut self, report: &PresentationFrameCostReport) -> bool {
        // First-use pipeline compilation should not drive the adaptive quality
        // controller. Closure benchmarking warms those variants separately, and
        // any remaining cache miss in the measured lane is setup churn rather
        // than steady-state frame cost.
        if report.gpu_runtime.pipeline_cache_misses > 0 {
            self.recovery_streak = 0;
            return false;
        }
        let frame_time_ms = if report.passes.is_empty() {
            if report.quality.target_fps == 0 {
                0.0
            } else {
                1000.0 / report.quality.target_fps as f32
            }
        } else {
            report
                .passes
                .iter()
                .map(|pass| pass.elapsed_micros as f32 / 1000.0)
                .sum()
        };
        self.observe_frame_time_ms(frame_time_ms)
    }

    pub fn observe_frame_time_ms(&mut self, frame_time_ms: f32) -> bool {
        self.frame_time_samples_ms.push(frame_time_ms.max(0.0));
        if self.frame_time_samples_ms.len() > self.moving_average_window {
            self.frame_time_samples_ms.remove(0);
        }
        let average_ms = self.average_frame_time_ms();
        let target_ms = if self.contract.target_fps == 0 {
            f32::INFINITY
        } else {
            1000.0 / self.contract.target_fps as f32
        };
        if average_ms > target_ms * 1.05 {
            self.recovery_streak = 0;
            return self.quality.step_down(&self.contract);
        }
        if average_ms < target_ms * 0.80 {
            self.recovery_streak += 1;
            if self.recovery_streak >= 3 {
                self.recovery_streak = 0;
                return self.quality.step_up(&self.contract);
            }
            return false;
        }
        self.recovery_streak = 0;
        false
    }

    pub fn average_frame_time_ms(&self) -> f32 {
        if self.frame_time_samples_ms.is_empty() {
            return 0.0;
        }
        self.frame_time_samples_ms.iter().sum::<f32>() / self.frame_time_samples_ms.len() as f32
    }
}
