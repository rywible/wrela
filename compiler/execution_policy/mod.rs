use crate::query_contract::DispatchBackend;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequiredGuaranteeClass {
    Exact,
    ConservativeNoFalseMiss,
    IntervalBounded,
    BestEffort,
}

impl RequiredGuaranteeClass {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::ConservativeNoFalseMiss => "conservative_no_false_miss",
            Self::IntervalBounded => "interval_bounded",
            Self::BestEffort => "best_effort",
        }
    }

    pub const fn id(self) -> u32 {
        match self {
            Self::Exact => 0,
            Self::ConservativeNoFalseMiss => 1,
            Self::IntervalBounded => 2,
            Self::BestEffort => 3,
        }
    }

    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(Self::Exact),
            1 => Some(Self::ConservativeNoFalseMiss),
            2 => Some(Self::IntervalBounded),
            3 => Some(Self::BestEffort),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SelectedMethodClass {
    ExactOracle,
    ConservativeSolver,
    IntervalSolver,
    HeuristicSolver,
}

impl SelectedMethodClass {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ExactOracle => "exact_oracle",
            Self::ConservativeSolver => "conservative_solver",
            Self::IntervalSolver => "interval_solver",
            Self::HeuristicSolver => "heuristic_solver",
        }
    }

    pub const fn id(self) -> u32 {
        match self {
            Self::ExactOracle => 0,
            Self::ConservativeSolver => 1,
            Self::IntervalSolver => 2,
            Self::HeuristicSolver => 3,
        }
    }

    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(Self::ExactOracle),
            1 => Some(Self::ConservativeSolver),
            2 => Some(Self::IntervalSolver),
            3 => Some(Self::HeuristicSolver),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayBudgetPolicy {
    pub max_distance: f32,
    pub min_step: f32,
    pub hit_epsilon: f32,
    pub max_steps: i32,
}

impl std::hash::Hash for RayBudgetPolicy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.max_distance.to_bits(), state);
        Hash::hash(&self.min_step.to_bits(), state);
        Hash::hash(&self.hit_epsilon.to_bits(), state);
        Hash::hash(&self.max_steps, state);
    }
}

impl Eq for RayBudgetPolicy {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryExecutionPolicy {
    pub backend_preference: DispatchBackend,
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
    pub ray_budget: Option<RayBudgetPolicy>,
}

impl std::hash::Hash for QueryExecutionPolicy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.backend_preference, state);
        Hash::hash(&self.required_guarantee, state);
        Hash::hash(&self.selected_method, state);
        Hash::hash(&self.ray_budget, state);
    }
}

impl QueryExecutionPolicy {
    pub const fn new(
        backend_preference: DispatchBackend,
        required_guarantee: RequiredGuaranteeClass,
        selected_method: SelectedMethodClass,
        ray_budget: Option<RayBudgetPolicy>,
    ) -> Self {
        Self {
            backend_preference,
            required_guarantee,
            selected_method,
            ray_budget,
        }
    }

    pub const fn conservative(
        backend_preference: DispatchBackend,
        ray_budget: Option<RayBudgetPolicy>,
    ) -> Self {
        Self::new(
            backend_preference,
            RequiredGuaranteeClass::ConservativeNoFalseMiss,
            SelectedMethodClass::ConservativeSolver,
            ray_budget,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationExecutionPolicy {
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
    pub primary_rays: RayBudgetPolicy,
}

impl std::hash::Hash for PresentationExecutionPolicy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.required_guarantee, state);
        Hash::hash(&self.selected_method, state);
        Hash::hash(&self.primary_rays, state);
    }
}

impl PresentationExecutionPolicy {
    pub const fn new(
        required_guarantee: RequiredGuaranteeClass,
        selected_method: SelectedMethodClass,
        primary_rays: RayBudgetPolicy,
    ) -> Self {
        Self {
            required_guarantee,
            selected_method,
            primary_rays,
        }
    }

    pub const fn conservative(primary_rays: RayBudgetPolicy) -> Self {
        Self::new(
            RequiredGuaranteeClass::ConservativeNoFalseMiss,
            SelectedMethodClass::ConservativeSolver,
            primary_rays,
        )
    }
}
