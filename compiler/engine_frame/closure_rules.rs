//! Engine-frame closure findings keyed by subsystem kind (RFC 0011 Phase 62.9).
//! `ClosureRuleTable` keeps rules extensible: new subsystems register rows without
//! editing a monolithic match.

use super::EngineSubsystemKind;
use crate::perf_target::{
    PerfClosureEngineFrameBudget, PerfClosureEngineFrameStatusReport, PerfClosureFinding,
};
use std::sync::OnceLock;

/// One budget rule contributing `PerfClosureFinding`s for a subsystem bucket.
pub trait EngineFrameClosureRule: Send + Sync {
    fn subsystem_kind(&self) -> EngineSubsystemKind;
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding>;
}

/// Table of closure rules (RFC 0011): register subsystem-scoped checks here.
///
/// Rules are indexed by [`EngineSubsystemKind`] so callers can ask for the
/// findings produced by a specific subsystem (RFC 0011 H4 acceptance:
/// rules filter by subsystem) without re-running unrelated rules. Calling
/// [`ClosureRuleTable::collect`] still runs every registered rule for
/// backward compatibility.
#[derive(Default)]
pub struct ClosureRuleTable {
    rules: Vec<Box<dyn EngineFrameClosureRule>>,
}

impl ClosureRuleTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, rule: Box<dyn EngineFrameClosureRule>) {
        self.rules.push(rule);
    }

    pub fn with_canonical_engine_frame_rules() -> Self {
        let mut table = Self::new();
        table.register(Box::new(FrameWallTimeRule));
        table.register(Box::new(PresentationBudgetRule));
        table.register(Box::new(CollisionBudgetRule));
        table.register(Box::new(StateAdvanceBudgetRule));
        table.register(Box::new(FutureSubsystemReserveRule));
        table.register(Box::new(QueueSubmitBudgetRule));
        table.register(Box::new(HotPathReadbackBudgetRule));
        table.register(Box::new(MotionToPhotonBudgetRule));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Presentation,
            "presentation",
            &[
                "presentation.fallback_to_vsync_fifo",
                "presentation.input_ring_overflow",
                "presentation.motion_to_photon_over_budget",
                "presentation.motion_to_photon_perf_lane_over_budget",
                "presentation.framerate_below_target",
            ],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Input,
            "input",
            &["input."],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::System,
            "system",
            &["system."],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Residency,
            "residency",
            &["residency."],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Physics,
            "physics",
            &[
                "physics.substep_over_budget",
                "physics.contact_readback_over_budget",
                "physics.substep_clamped",
                "physics.body_admission_full",
                "physics.cpu_oracle_divergence",
            ],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Audio,
            "audio",
            &[
                "audio.underrun",
                "audio.media_queries_over_budget",
                "audio.voice_count_over_cap",
                "audio.publish_latency",
            ],
        )));
        table.register(Box::new(ViolationPrefixRule::new(
            EngineSubsystemKind::Save,
            "save",
            &["save."],
        )));
        table
    }

    pub fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        for rule in &self.rules {
            findings.extend(rule.collect(budget, report));
        }
        findings
    }

    /// Run only the rules that are scoped to a particular subsystem. Useful
    /// for inspector panels and per-subsystem closure replays.
    pub fn collect_for_subsystem(
        &self,
        kind: &EngineSubsystemKind,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        for rule in &self.rules {
            if rule.subsystem_kind() == *kind {
                findings.extend(rule.collect(budget, report));
            }
        }
        findings
    }

    /// Subsystem kinds covered by the registered rules. Useful for tooling
    /// that wants to display "covered subsystems" without poking the trait.
    pub fn registered_subsystems(&self) -> Vec<EngineSubsystemKind> {
        let mut kinds: Vec<EngineSubsystemKind> = self
            .rules
            .iter()
            .map(|rule| rule.subsystem_kind())
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

fn canonical_engine_frame_rule_table() -> &'static ClosureRuleTable {
    static TABLE: OnceLock<ClosureRuleTable> = OnceLock::new();
    TABLE.get_or_init(ClosureRuleTable::with_canonical_engine_frame_rules)
}

struct FrameWallTimeRule;
impl EngineFrameClosureRule for FrameWallTimeRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::FutureReserve("engine_frame_wall".to_string())
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.frame_wall_time_median_ms
            && observed > budget.frame_wall_time_median_ms
        {
            findings.push(PerfClosureFinding {
                subsystem: "engine_frame".to_string(),
                focus: "frame_wall_time_budget".to_string(),
                summary: "the unified engine frame is still over the wall-time budget even after combining presentation and collision under one scheduler".to_string(),
                evidence: vec![
                    format!("frame_wall_time_median_ms={observed:.2}"),
                    format!(
                        "frame_wall_time_budget_ms={:.2}",
                        budget.frame_wall_time_median_ms
                    ),
                ],
                next_step:
                    "treat the engine frame as the canonical throughput unit and keep shaving the dominant subsystem until the whole frame fits".to_string(),
            });
        }
        findings
    }
}

struct PresentationBudgetRule;
impl EngineFrameClosureRule for PresentationBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::Presentation
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.presentation_median_ms
            && observed > budget.presentation_median_ms
        {
            findings.push(PerfClosureFinding {
                subsystem: "presentation".to_string(),
                focus: "engine_frame_presentation_budget".to_string(),
                summary: "presentation still dominates the engine frame budget, so subsystem wins are not yet enough for the full-frame target".to_string(),
                evidence: vec![
                    format!("presentation_median_ms={observed:.2}"),
                    format!(
                        "presentation_budget_ms={:.2}",
                        budget.presentation_median_ms
                    ),
                ],
                next_step:
                    "keep reducing the resident presentation critical path before tuning smaller contributors".to_string(),
            });
        }
        findings
    }
}

struct CollisionBudgetRule;
impl EngineFrameClosureRule for CollisionBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::Collision
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.collision_median_ms
            && observed > budget.collision_median_ms
        {
            findings.push(PerfClosureFinding {
                subsystem: "collision".to_string(),
                focus: "engine_frame_collision_budget".to_string(),
                summary: "collision is still too expensive inside the full engine frame, so the scheduler cannot hit the representative throughput target".to_string(),
                evidence: vec![
                    format!("collision_median_ms={observed:.2}"),
                    format!("collision_budget_ms={:.2}", budget.collision_median_ms),
                ],
                next_step:
                    "improve collision batching and certification pressure until the representative collision slice fits its frame budget".to_string(),
            });
        }
        findings
    }
}

struct StateAdvanceBudgetRule;
impl EngineFrameClosureRule for StateAdvanceBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::StateAdvance
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.state_advance_median_ms
            && observed > budget.state_advance_median_ms
        {
            findings.push(PerfClosureFinding {
                subsystem: "state_advance".to_string(),
                focus: "state_advance_budget".to_string(),
                summary: "the reserved state-advance slot is already consuming more time than the frame budget allows for future subsystems".to_string(),
                evidence: vec![
                    format!("state_advance_median_ms={observed:.2}"),
                    format!(
                        "state_advance_budget_ms={:.2}",
                        budget.state_advance_median_ms
                    ),
                ],
                next_step:
                    "keep the state-advance adapter minimal so future gameplay work still fits inside the reserved engine-frame slot".to_string(),
            });
        }
        findings
    }
}

struct FutureSubsystemReserveRule;
impl EngineFrameClosureRule for FutureSubsystemReserveRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::FutureReserve("future_subsystem_reserve".to_string())
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.future_subsystem_reserve_ms
            && observed < budget.future_subsystem_reserve_ms
        {
            findings.push(PerfClosureFinding {
                subsystem: "engine_frame".to_string(),
                focus: "future_subsystem_reserve".to_string(),
                summary: "the engine frame has consumed the reserve that was supposed to protect future subsystem work".to_string(),
                evidence: vec![
                    format!("future_subsystem_reserve_ms={observed:.2}"),
                    format!(
                        "required_future_subsystem_reserve_ms={:.2}",
                        budget.future_subsystem_reserve_ms
                    ),
                ],
                next_step:
                    "pull budget back out of the current frame so future subsystems can be added without immediately breaking throughput".to_string(),
            });
        }
        findings
    }
}

struct QueueSubmitBudgetRule;
impl EngineFrameClosureRule for QueueSubmitBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::GpuRuntime
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.queue_submit_count
            && observed > budget.max_queue_submit_count_per_frame
        {
            findings.push(PerfClosureFinding {
                subsystem: "engine_frame".to_string(),
                focus: "engine_frame_queue_submit_budget".to_string(),
                summary: "the engine frame is still fragmented across too many queue submissions".to_string(),
                evidence: vec![
                    format!("queue_submit_count={observed}"),
                    format!(
                        "max_queue_submit_count_per_frame={}",
                        budget.max_queue_submit_count_per_frame
                    ),
                ],
                next_step:
                    "keep presentation and collision on the same steady-state submission story so throughput reflects one frame, not a stack of micro-passes".to_string(),
            });
        }
        findings
    }
}

struct HotPathReadbackBudgetRule;
impl EngineFrameClosureRule for HotPathReadbackBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::FutureReserve("hot_path_readback".to_string())
    }
    fn collect(
        &self,
        budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let Some(observed) = report.hot_path_readback_bytes
            && observed > budget.max_hot_path_readback_bytes_per_frame
        {
            findings.push(PerfClosureFinding {
                subsystem: "engine_frame".to_string(),
                focus: "engine_frame_hot_path_readback_budget".to_string(),
                summary: "the engine frame still performs hot-path readback, so the representative closure lane is not actually GPU-resident end to end".to_string(),
                evidence: vec![
                    format!("hot_path_readback_bytes={observed}"),
                    format!(
                        "max_hot_path_readback_bytes_per_frame={}",
                        budget.max_hot_path_readback_bytes_per_frame
                    ),
                ],
                next_step:
                    "leave result readback to debug and oracle paths, and keep the closure lane on metrics-only tickets".to_string(),
            });
        }
        findings
    }
}

struct ViolationPrefixRule {
    kind: EngineSubsystemKind,
    subsystem: &'static str,
    prefixes: &'static [&'static str],
}

impl ViolationPrefixRule {
    fn new(
        kind: EngineSubsystemKind,
        subsystem: &'static str,
        prefixes: &'static [&'static str],
    ) -> Self {
        Self {
            kind,
            subsystem,
            prefixes,
        }
    }
}

impl EngineFrameClosureRule for ViolationPrefixRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        self.kind.clone()
    }

    fn collect(
        &self,
        _budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        for violation in &report.violations {
            if self.prefixes.iter().any(|prefix| {
                violation == prefix.trim_end_matches('.') || violation.starts_with(prefix)
            }) {
                findings.push(PerfClosureFinding {
                    subsystem: self.subsystem.to_string(),
                    focus: violation.clone(),
                    summary: format!(
                        "{} reported an RFC 0011 closure violation",
                        self.subsystem
                    ),
                    evidence: vec![format!("violation={violation}")],
                    next_step: "inspect the subsystem EngineFrameReport row and fix the reported runtime contract breach".to_string(),
                });
            }
        }
        findings
    }
}

/// Motion-to-photon sampled into `PerfClosureEngineFrameStatusReport` (benchmark lane).
struct MotionToPhotonBudgetRule;
impl EngineFrameClosureRule for MotionToPhotonBudgetRule {
    fn subsystem_kind(&self) -> EngineSubsystemKind {
        EngineSubsystemKind::Presentation
    }
    fn collect(
        &self,
        _budget: &PerfClosureEngineFrameBudget,
        report: &PerfClosureEngineFrameStatusReport,
    ) -> Vec<PerfClosureFinding> {
        let mut findings = Vec::new();
        if let (Some(observed), Some(limit)) = (
            report.motion_to_photon_median_ms,
            report.motion_to_photon_budget_ms,
        ) && observed > limit
        {
            findings.push(PerfClosureFinding {
                subsystem: "presentation".to_string(),
                focus: "motion_to_photon_budget".to_string(),
                summary: "motion-to-photon latency exceeds the interactive budget for this closure profile".to_string(),
                evidence: vec![
                    format!("motion_to_photon_median_ms={observed:.2}"),
                    format!("motion_to_photon_budget_ms={limit:.2}"),
                ],
                next_step:
                    "tighten present mode policy, reduce simulation work per frame, or profile GPU/CPU stages attributed in EngineFrameReport.latency".to_string(),
            });
        }
        findings
    }
}

/// Collect perf-closure findings using the canonical rule table (backward compatible entry).
pub fn collect_engine_frame_budget_findings(
    budget: &PerfClosureEngineFrameBudget,
    report: &PerfClosureEngineFrameStatusReport,
) -> Vec<PerfClosureFinding> {
    canonical_engine_frame_rule_table().collect(budget, report)
}
