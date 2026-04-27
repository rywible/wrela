//! Physics execution plan (RFC 0011 Phase 67).

use crate::physics_contract::PhysicsBodyDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsBackend {
    CpuOracle,
    CollisionBacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsIntegrator {
    Xpbd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsSubstepPolicy {
    pub requested_substeps_per_tick: u32,
    pub max_substeps_per_tick: u32,
    pub positional_iterations: u32,
    pub velocity_iterations: u32,
}

impl Default for PhysicsSubstepPolicy {
    fn default() -> Self {
        Self {
            requested_substeps_per_tick: 2,
            max_substeps_per_tick: 4,
            positional_iterations: 4,
            velocity_iterations: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsCcdPolicy {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsPlan {
    pub backend: PhysicsBackend,
    pub integrator: PhysicsIntegrator,
    pub substeps: PhysicsSubstepPolicy,
    pub ccd: PhysicsCcdPolicy,
    pub contact_readback_budget_bytes: u64,
    pub bodies: Vec<PhysicsBodyDescriptor>,
}

impl PhysicsPlan {
    pub fn new(backend: PhysicsBackend, bodies: Vec<PhysicsBodyDescriptor>) -> Self {
        Self {
            backend,
            integrator: PhysicsIntegrator::Xpbd,
            substeps: PhysicsSubstepPolicy::default(),
            ccd: PhysicsCcdPolicy { enabled: true },
            contact_readback_budget_bytes: 4096,
            bodies,
        }
    }

    pub fn cpu(bodies: Vec<PhysicsBodyDescriptor>) -> Self {
        Self::new(PhysicsBackend::CpuOracle, bodies)
    }

    pub fn collision_backed(bodies: Vec<PhysicsBodyDescriptor>) -> Self {
        Self::new(PhysicsBackend::CollisionBacked, bodies)
    }
}
