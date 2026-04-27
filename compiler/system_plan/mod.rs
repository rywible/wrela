//! System program planning and validation (RFC 0011 Phase 65).

use crate::system_contract::{
    SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPlan {
    pub id: SystemId,
    pub contract: SystemContractId,
    pub phase: SystemPhase,
    pub access: SystemAccessSummary,
    pub mir_function_id: u32,
    pub runs_before: BTreeSet<SystemId>,
    pub runs_after: BTreeSet<SystemId>,
}

impl SystemPlan {
    pub fn new(
        id: SystemId,
        contract: SystemContractId,
        phase: SystemPhase,
        access: SystemAccessSummary,
        mir_function_id: u32,
    ) -> Self {
        Self {
            id,
            contract,
            phase,
            access,
            mir_function_id,
            runs_before: BTreeSet::new(),
            runs_after: BTreeSet::new(),
        }
    }

    pub fn runs_before(mut self, system: SystemId) -> Self {
        self.runs_before.insert(system);
        self
    }

    pub fn runs_after(mut self, system: SystemId) -> Self {
        self.runs_after.insert(system);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventRoutingTable {
    pub one_tick_deferred: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemProgram {
    pub phases: [Vec<SystemPlan>; 3],
    pub event_table: EventRoutingTable,
}

impl SystemProgram {
    pub fn new(plans: impl IntoIterator<Item = SystemPlan>) -> Result<Self, SystemPlanError> {
        let mut phases: [Vec<SystemPlan>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for plan in plans {
            phases[plan.phase.index()].push(plan);
        }
        validate_unique_system_ids(&phases)?;
        validate_aliasing_writers(&phases)?;
        for phase in SystemPhase::ALL {
            let idx = phase.index();
            let slice = std::mem::take(&mut phases[idx]);
            phases[idx] = ordered_plans_for_phase(phase, slice)?;
        }
        let program = Self {
            phases,
            event_table: EventRoutingTable {
                one_tick_deferred: true,
            },
        };
        validate_system_program(&program)?;
        Ok(program)
    }

    pub fn phase(&self, phase: SystemPhase) -> &[SystemPlan] {
        &self.phases[phase.index()]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SystemPlanError {
    #[error("aliasing system writers in phase {phase:?}: {left:?} and {right:?}")]
    AliasingWriters {
        phase: SystemPhase,
        left: SystemId,
        right: SystemId,
    },
    #[error("duplicate system id {id:?}")]
    DuplicateSystemId { id: SystemId },
    #[error(
        "read/write system conflict in phase {phase:?} requires explicit ordering: {left:?} and {right:?} both access {resource:?}"
    )]
    MissingExplicitOrdering {
        phase: SystemPhase,
        left: SystemId,
        right: SystemId,
        resource: SystemResourceId,
    },
    #[error("system schedule cycle in phase {phase:?}")]
    ScheduleCycle { phase: SystemPhase },
}

fn ordered_plans_for_phase(
    phase: SystemPhase,
    plans: Vec<SystemPlan>,
) -> Result<Vec<SystemPlan>, SystemPlanError> {
    if plans.len() <= 1 {
        return Ok(plans);
    }
    let n = plans.len();
    let mut indegree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let a = &plans[i];
            let b = &plans[j];
            if declared_runs_before(a, b) {
                adj[i].push(j);
                indegree[j] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&idx| indegree[idx] == 0).collect::<Vec<_>>();
    ready.sort_unstable();
    let mut queue: VecDeque<usize> = ready.into();
    let mut order = Vec::with_capacity(n);
    while let Some(i) = queue.pop_front() {
        order.push(i);
        let mut neighbors = adj[i].clone();
        neighbors.sort_unstable();
        let mut freed = Vec::new();
        for j in neighbors {
            indegree[j] = indegree[j].saturating_sub(1);
            if indegree[j] == 0 {
                freed.push(j);
            }
        }
        freed.sort_unstable();
        for j in freed {
            queue.push_back(j);
        }
    }
    if order.len() != n {
        return Err(SystemPlanError::ScheduleCycle { phase });
    }
    Ok(order.into_iter().map(|idx| plans[idx].clone()).collect())
}

pub fn declared_runs_before(left: &SystemPlan, right: &SystemPlan) -> bool {
    left.runs_before.contains(&right.id) || right.runs_after.contains(&left.id)
}

fn systems_are_ordered(left: &SystemPlan, right: &SystemPlan) -> bool {
    declared_runs_before(left, right) || declared_runs_before(right, left)
}

pub fn validate_system_program(program: &SystemProgram) -> Result<(), SystemPlanError> {
    validate_unique_system_ids(&program.phases)?;
    validate_aliasing_writers(&program.phases)?;
    for phase in SystemPhase::ALL {
        let plans = program.phase(phase);
        for i in 0..plans.len() {
            for j in (i + 1)..plans.len() {
                let left = &plans[i];
                let right = &plans[j];
                for resource in left.access.writes.intersection(&right.access.reads) {
                    if !systems_are_ordered(left, right) {
                        return Err(SystemPlanError::MissingExplicitOrdering {
                            phase,
                            left: left.id.clone(),
                            right: right.id.clone(),
                            resource: resource.clone(),
                        });
                    }
                }
                for resource in left.access.reads.intersection(&right.access.writes) {
                    if !systems_are_ordered(left, right) {
                        return Err(SystemPlanError::MissingExplicitOrdering {
                            phase,
                            left: left.id.clone(),
                            right: right.id.clone(),
                            resource: resource.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_aliasing_writers(phases: &[Vec<SystemPlan>; 3]) -> Result<(), SystemPlanError> {
    for phase in SystemPhase::ALL {
        let mut writers: BTreeMap<SystemResourceId, SystemId> = BTreeMap::new();
        for plan in &phases[phase.index()] {
            for resource in &plan.access.writes {
                if let Some(left) = writers.get(resource) {
                    return Err(SystemPlanError::AliasingWriters {
                        phase,
                        left: left.clone(),
                        right: plan.id.clone(),
                    });
                }
                writers.insert(resource.clone(), plan.id.clone());
            }
        }
    }
    Ok(())
}

fn validate_unique_system_ids(phases: &[Vec<SystemPlan>; 3]) -> Result<(), SystemPlanError> {
    let mut seen = BTreeSet::<SystemId>::new();
    for phase in SystemPhase::ALL {
        for plan in &phases[phase.index()] {
            if !seen.insert(plan.id.clone()) {
                return Err(SystemPlanError::DuplicateSystemId {
                    id: plan.id.clone(),
                });
            }
        }
    }
    Ok(())
}
