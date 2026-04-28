//! CPU XPBD-like physics executor (RFC 0011 Phase 67).

#![forbid(unsafe_code)]

use crate::collision_contract::{
    COLLISION_SPHERE_OVERLAP_WORLD, COLLISION_SPHERE_SWEEP_TRANSITION,
    COLLISION_TIME_OF_IMPACT_TRANSITION, CollisionResult, CollisionSnapshotTransitionInput,
    CollisionSphereSweepInput,
};
use crate::collision_exec::{
    CollisionBatchItem, CollisionCandidateGroupingPolicy, CollisionCertificationPolicy,
    CollisionWorkloadBatch,
};
use crate::collision_plan::{CollisionPlan, CollisionQueryKind};
use crate::kernel::KernelValue;
use crate::physics_contract::{PhysicsBodyClass, PhysicsBodyDescriptor, PhysicsBodyId};
use crate::physics_plan::{PhysicsBackend, PhysicsPlan};
use crate::query_contract::DispatchBackend;
use crate::query_exec::QueryExecContext;
use crate::state_advance::ChangeClass;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

const CONTACT_READBACK_BYTES: u64 = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsBodyState {
    pub id: PhysicsBodyId,
    pub position: [f32; 3],
    pub previous_position: [f32; 3],
    pub linear_velocity: [f32; 3],
    pub pending_force: [f32; 3],
}

impl PhysicsBodyState {
    pub fn new(id: PhysicsBodyId, position: [f32; 3]) -> Self {
        Self {
            id,
            position,
            previous_position: position,
            linear_velocity: [0.0; 3],
            pending_force: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsContact {
    pub body: PhysicsBodyId,
    /// `None` for static-world contacts (ground/plane); `Some` for body-body
    /// pairs surfaced by the broadphase. RFC 0011 H5: makes the contact graph
    /// self-describing instead of implicitly "vs ground" everywhere.
    pub other: Option<PhysicsBodyId>,
    pub normal_world: [f32; 3],
    pub penetration: f32,
    pub generated_by_ccd: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PhysicsFrameReport {
    pub substeps: u32,
    pub integrations: u32,
    pub contacts_detected: u32,
    pub contacts_resolved: u32,
    pub ccd_swept_bodies: u32,
    pub fallback_count: u32,
    pub readback_bytes: u64,
    pub contact_readback_micros: u128,
    pub collision_batches: Vec<CollisionWorkloadBatch>,
    pub collision_batches_submitted: u32,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PhysicsStepFrame {
    report: PhysicsFrameReport,
    sub_dt: f32,
    substep_index: u32,
    contacts: Vec<PhysicsContact>,
    current_batch_start: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsCollisionWorld {
    pub capture: KernelValue,
    pub domain: KernelValue,
    pub backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsCollisionBatchExecution {
    pub submitted: bool,
    pub executor: SmolStr,
    pub used_cpu_oracle_fallback: bool,
    pub error: Option<String>,
    pub contacts: Vec<PhysicsContact>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct CollisionWorkloadContacts {
    contacts: Vec<PhysicsContact>,
    readback_contacts: usize,
}

pub trait PhysicsCollisionBatchExecutor: std::fmt::Debug + Send + Sync {
    fn submit_collision_batch(
        &self,
        _batch: &CollisionWorkloadBatch,
        bodies: &[PhysicsBodyState],
        descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution;
}

#[derive(Debug, Default)]
pub struct NoopPhysicsCollisionBatchExecutor;

impl PhysicsCollisionBatchExecutor for NoopPhysicsCollisionBatchExecutor {
    fn submit_collision_batch(
        &self,
        _batch: &CollisionWorkloadBatch,
        _bodies: &[PhysicsBodyState],
        _descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        PhysicsCollisionBatchExecution {
            submitted: false,
            executor: SmolStr::new("missing_collision_batch_executor"),
            used_cpu_oracle_fallback: false,
            error: None,
            contacts: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct CpuOraclePhysicsCollisionBatchExecutor;

impl PhysicsCollisionBatchExecutor for CpuOraclePhysicsCollisionBatchExecutor {
    fn submit_collision_batch(
        &self,
        batch: &CollisionWorkloadBatch,
        bodies: &[PhysicsBodyState],
        descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        let contacts = if batch.contract_id == COLLISION_SPHERE_OVERLAP_WORLD {
            cpu_oracle_world_contacts(bodies, descriptors)
        } else {
            Vec::new()
        };
        PhysicsCollisionBatchExecution {
            submitted: true,
            executor: SmolStr::new("explicit_cpu_oracle_collision_executor"),
            used_cpu_oracle_fallback: true,
            error: None,
            contacts,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollisionExecPhysicsCollisionBatchExecutor {
    ctx: Arc<QueryExecContext>,
    metrics_only: bool,
}

impl CollisionExecPhysicsCollisionBatchExecutor {
    pub fn new(ctx: Arc<QueryExecContext>) -> Self {
        Self {
            ctx,
            metrics_only: false,
        }
    }

    pub fn metrics_only(ctx: Arc<QueryExecContext>) -> Self {
        Self {
            ctx,
            metrics_only: true,
        }
    }
}

impl PhysicsCollisionBatchExecutor for CollisionExecPhysicsCollisionBatchExecutor {
    fn submit_collision_batch(
        &self,
        batch: &CollisionWorkloadBatch,
        bodies: &[PhysicsBodyState],
        descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        if self.metrics_only {
            return match crate::collision_exec::execute_batch_metrics_only(batch, &self.ctx) {
                Ok(_) => PhysicsCollisionBatchExecution {
                    submitted: true,
                    executor: SmolStr::new("collision_exec_metrics_only"),
                    used_cpu_oracle_fallback: false,
                    error: None,
                    contacts: Vec::new(),
                },
                Err(error) => PhysicsCollisionBatchExecution {
                    submitted: false,
                    executor: SmolStr::new("collision_exec_metrics_only_error"),
                    used_cpu_oracle_fallback: false,
                    error: Some(error.to_string()),
                    contacts: Vec::new(),
                },
            };
        }

        match crate::collision_exec::execute_batch(batch, &self.ctx, None) {
            Ok(result) => PhysicsCollisionBatchExecution {
                submitted: true,
                executor: SmolStr::new("collision_exec"),
                used_cpu_oracle_fallback: false,
                error: None,
                contacts: collision_results_to_physics_contacts(
                    batch,
                    &result.results,
                    bodies,
                    descriptors,
                ),
            },
            Err(error) => PhysicsCollisionBatchExecution {
                submitted: false,
                executor: SmolStr::new("collision_exec_error"),
                used_cpu_oracle_fallback: false,
                error: Some(error.to_string()),
                contacts: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PhysicsError {
    #[error("physics body admission full")]
    BodyAdmissionFull,
    #[error("missing physics body descriptor for {0:?}")]
    MissingDescriptor(PhysicsBodyId),
}

#[derive(Debug, Clone)]
pub struct PhysicsSolver {
    plan: PhysicsPlan,
    bodies: Vec<PhysicsBodyState>,
    body_index: HashMap<PhysicsBodyId, usize>,
    descriptors: HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    warm_start_lambdas: HashMap<(PhysicsBodyId, Option<PhysicsBodyId>), f32>,
    collision_executor: Arc<dyn PhysicsCollisionBatchExecutor>,
    collision_world: Option<PhysicsCollisionWorld>,
}

impl PhysicsSolver {
    pub fn new(plan: PhysicsPlan, bodies: Vec<PhysicsBodyState>) -> Self {
        let executor: Arc<dyn PhysicsCollisionBatchExecutor> =
            Arc::new(NoopPhysicsCollisionBatchExecutor);
        Self::with_collision_executor(plan, bodies, executor)
    }

    pub fn with_collision_executor(
        plan: PhysicsPlan,
        bodies: Vec<PhysicsBodyState>,
        collision_executor: Arc<dyn PhysicsCollisionBatchExecutor>,
    ) -> Self {
        let mut body_index = HashMap::with_capacity(bodies.len());
        for (idx, body) in bodies.iter().enumerate() {
            body_index.insert(body.id, idx);
        }
        let descriptors = plan
            .bodies
            .iter()
            .cloned()
            .map(|descriptor| (descriptor.id, descriptor))
            .collect();
        Self {
            plan,
            bodies,
            body_index,
            descriptors,
            warm_start_lambdas: HashMap::new(),
            collision_executor,
            collision_world: None,
        }
    }

    pub fn with_collision_world(mut self, world: PhysicsCollisionWorld) -> Self {
        self.collision_world = Some(world);
        self
    }

    pub fn backend(&self) -> PhysicsBackend {
        self.plan.backend
    }

    pub fn bodies(&self) -> &[PhysicsBodyState] {
        &self.bodies
    }

    pub(crate) fn planned_substeps_per_frame(&self) -> u32 {
        planned_substeps_for_plan(&self.plan)
    }

    pub fn step(&mut self, dt: f32) -> Result<PhysicsFrameReport, PhysicsError> {
        let mut frame = self.begin_frame(dt);
        for _ in 0..frame.report.substeps {
            self.stage_integrate(&mut frame)?;
            self.stage_broadphase(&mut frame)?;
            self.stage_detect_contacts(&mut frame)?;
            self.stage_solve_positions(&mut frame)?;
            self.stage_solve_velocities(&mut frame)?;
        }
        self.stage_move_fsm(&mut frame)?;
        Ok(Self::finish_frame(frame))
    }

    pub(crate) fn begin_frame(&self, dt: f32) -> PhysicsStepFrame {
        let requested = self.plan.substeps.requested_substeps_per_tick;
        let max = self.plan.substeps.max_substeps_per_tick.max(1);
        let substeps = planned_substeps_for_plan(&self.plan);
        let mut report = PhysicsFrameReport {
            substeps,
            ..PhysicsFrameReport::default()
        };
        if requested > max {
            report.findings.push("physics.substep_clamped".to_string());
            report
                .findings
                .push("physics.time_compression: substeps clamped; consider lowering simulation load or increasing max_substeps_per_tick".to_string());
        }
        PhysicsStepFrame {
            report,
            sub_dt: dt / substeps as f32,
            substep_index: 0,
            contacts: Vec::new(),
            current_batch_start: 0,
        }
    }

    pub(crate) fn stage_integrate(
        &mut self,
        frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        self.integrate(frame.sub_dt, &mut frame.report)
    }

    pub(crate) fn stage_broadphase(
        &mut self,
        frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        frame.current_batch_start = frame.report.collision_batches.len();
        frame.contacts.clear();
        if self.plan.backend == PhysicsBackend::CollisionBacked {
            self.record_collision_workload_intent(
                frame.substep_index,
                frame.sub_dt,
                &mut frame.report,
            )?;
        }
        Ok(())
    }

    pub(crate) fn stage_detect_contacts(
        &mut self,
        frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        let (contacts, readback_contacts) = if self.plan.backend == PhysicsBackend::CollisionBacked
        {
            let workload_contacts = self
                .submit_collision_workload_batches(frame.current_batch_start, &mut frame.report);
            (
                workload_contacts.contacts,
                workload_contacts.readback_contacts,
            )
        } else {
            (self.cpu_oracle_contacts()?, 0)
        };
        frame.report.contacts_detected = frame
            .report
            .contacts_detected
            .saturating_add(contacts.len() as u32);
        self.record_contact_readback(readback_contacts, &mut frame.report);
        frame.contacts = contacts;
        Ok(())
    }

    pub(crate) fn stage_solve_positions(
        &mut self,
        frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        self.warm_start_contacts(&frame.contacts);
        self.solve_contacts(&frame.contacts, &mut frame.report)
    }

    pub(crate) fn stage_solve_velocities(
        &mut self,
        frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        self.recompute_velocities_from_positions(frame.sub_dt);
        self.solve_velocity_constraints(&frame.contacts, &mut frame.report)?;
        frame.substep_index = frame.substep_index.saturating_add(1);
        Ok(())
    }

    pub(crate) fn stage_move_fsm(
        &mut self,
        _frame: &mut PhysicsStepFrame,
    ) -> Result<(), PhysicsError> {
        Ok(())
    }

    pub(crate) fn finish_frame(frame: PhysicsStepFrame) -> PhysicsFrameReport {
        frame.report
    }

    fn descriptor(&self, id: PhysicsBodyId) -> Result<&PhysicsBodyDescriptor, PhysicsError> {
        self.descriptors
            .get(&id)
            .ok_or(PhysicsError::MissingDescriptor(id))
    }

    fn record_contact_readback(&self, contacts: usize, report: &mut PhysicsFrameReport) {
        if self.plan.backend != PhysicsBackend::CollisionBacked || contacts == 0 {
            return;
        }
        let bytes = contacts as u64 * CONTACT_READBACK_BYTES;
        report.readback_bytes = report.readback_bytes.saturating_add(bytes);
        report.contact_readback_micros = report
            .contact_readback_micros
            .saturating_add(u128::from(bytes.div_ceil(64)));
        if report.readback_bytes > self.plan.contact_readback_budget_bytes {
            push_unique_finding(report, "physics.contact_readback_over_budget");
        }
    }

    fn record_collision_workload_intent(
        &self,
        substep_index: u32,
        sub_dt: f32,
        report: &mut PhysicsFrameReport,
    ) -> Result<(), PhysicsError> {
        let Some(world) = self.collision_world.as_ref() else {
            push_unique_finding(report, "physics.collision_world_unbound");
            return Ok(());
        };
        let overlap_items = self
            .bodies
            .iter()
            .map(|body| {
                let descriptor = self.descriptor(body.id)?;
                Ok(CollisionBatchItem::SphereOverlap {
                    center: body.position,
                    radius: descriptor.radius,
                })
            })
            .collect::<Result<Vec<_>, PhysicsError>>()?;
        if !overlap_items.is_empty() {
            report.collision_batches.push(collision_batch(
                world,
                "physics.detect_contacts",
                substep_index,
                CollisionQueryKind::SphereOverlapWorld,
                COLLISION_SPHERE_OVERLAP_WORLD,
                overlap_items,
            ));
        }

        if !self.plan.ccd.enabled {
            return Ok(());
        }

        let transition = CollisionSnapshotTransitionInput {
            current_snapshot_epoch: substep_index.saturating_add(1),
            previous_snapshot_epoch: substep_index,
            change_class: ChangeClass::Behavior,
        };
        let mut sweep_items = Vec::new();
        let mut toi_items = Vec::new();
        for body in &self.bodies {
            let descriptor = self.descriptor(body.id)?;
            let displacement = distance(body.previous_position, body.position);
            if displacement <= descriptor.ccd_threshold_per_substep.max(1e-4) * sub_dt.max(1e-4) {
                continue;
            }
            let sweep = CollisionSphereSweepInput {
                start_center: body.previous_position,
                end_center: body.position,
                radius: descriptor.radius,
                contact_tolerance: 0.001,
                max_iterations: 8,
            };
            sweep_items.push(CollisionBatchItem::SphereSweep { transition, sweep });
            toi_items.push(CollisionBatchItem::SphereTimeOfImpact { transition, sweep });
        }
        if !sweep_items.is_empty() {
            report.collision_batches.push(collision_batch(
                world,
                "physics.broadphase",
                substep_index,
                CollisionQueryKind::SphereSweepTransition,
                COLLISION_SPHERE_SWEEP_TRANSITION,
                sweep_items,
            ));
        }
        if !toi_items.is_empty() {
            report.collision_batches.push(collision_batch(
                world,
                "physics.contact_readback",
                substep_index,
                CollisionQueryKind::SphereTimeOfImpactTransition,
                COLLISION_TIME_OF_IMPACT_TRANSITION,
                toi_items,
            ));
        }
        Ok(())
    }

    fn submit_collision_workload_batches(
        &self,
        batch_start: usize,
        report: &mut PhysicsFrameReport,
    ) -> CollisionWorkloadContacts {
        let mut contacts = Vec::new();
        let mut readback_contacts: usize = 0;
        let batches = report.collision_batches[batch_start..].to_vec();
        for batch in &batches {
            let execution = self.collision_executor.submit_collision_batch(
                batch,
                &self.bodies,
                &self.descriptors,
            );
            if execution.submitted {
                report.collision_batches_submitted =
                    report.collision_batches_submitted.saturating_add(1);
            } else {
                push_unique_finding(report, "physics.collision_batch_not_submitted");
            }
            if execution.used_cpu_oracle_fallback {
                report.fallback_count = report.fallback_count.saturating_add(1);
                push_unique_finding(report, "physics.cpu_oracle_collision_fallback");
            }
            readback_contacts = readback_contacts.saturating_add(execution.contacts.len());
            contacts.extend(execution.contacts);
        }
        contacts.extend(cpu_oracle_body_body_contacts(
            &self.bodies,
            &self.descriptors,
        ));
        let oracle_contacts = cpu_oracle_collision_contacts(&self.bodies, &self.descriptors);
        if !contact_sets_equivalent(&contacts, &oracle_contacts) {
            push_unique_finding(report, "physics.cpu_oracle_divergence");
        }
        CollisionWorkloadContacts {
            contacts,
            readback_contacts,
        }
    }

    fn warm_start_contacts(&mut self, contacts: &[PhysicsContact]) {
        for contact in contacts {
            let key = contact_key(contact.body, contact.other);
            let lambda = self
                .warm_start_lambdas
                .get(&key)
                .copied()
                .unwrap_or(contact.penetration);
            self.warm_start_lambdas
                .insert(key, lambda.max(contact.penetration));
        }
    }

    fn recompute_velocities_from_positions(&mut self, dt: f32) {
        let inv_dt = 1.0 / dt.max(1e-6);
        for body in &mut self.bodies {
            for axis in 0..3 {
                body.linear_velocity[axis] =
                    (body.position[axis] - body.previous_position[axis]) * inv_dt;
            }
        }
    }

    fn solve_velocity_constraints(
        &mut self,
        contacts: &[PhysicsContact],
        _report: &mut PhysicsFrameReport,
    ) -> Result<(), PhysicsError> {
        for contact in contacts {
            let Some(&idx) = self.body_index.get(&contact.body) else {
                continue;
            };
            let restitution = self.descriptor(contact.body)?.restitution;
            let body = &mut self.bodies[idx];
            let v_dot_n = body.linear_velocity[0] * contact.normal_world[0]
                + body.linear_velocity[1] * contact.normal_world[1]
                + body.linear_velocity[2] * contact.normal_world[2];
            if v_dot_n < 0.0 {
                let impulse = v_dot_n * (1.0 + restitution);
                for axis in 0..3 {
                    body.linear_velocity[axis] -= impulse * contact.normal_world[axis];
                }
            }
        }
        Ok(())
    }

    fn integrate(&mut self, dt: f32, report: &mut PhysicsFrameReport) -> Result<(), PhysicsError> {
        for idx in 0..self.bodies.len() {
            let id = self.bodies[idx].id;
            let descriptor = self.descriptor(id)?.clone();
            if descriptor.class != PhysicsBodyClass::Dynamic {
                continue;
            }
            let body = &mut self.bodies[idx];
            body.previous_position = body.position;
            body.linear_velocity[1] -= 9.81 * dt;
            for axis in 0..3 {
                body.linear_velocity[axis] +=
                    body.pending_force[axis] * descriptor.inverse_mass * dt;
                body.position[axis] += body.linear_velocity[axis] * dt;
            }
            if self.plan.ccd.enabled {
                let body = &mut self.bodies[idx];
                Self::apply_ground_ccd(body, &descriptor, dt, report)?;
            }
            report.integrations = report.integrations.saturating_add(1);
        }
        Ok(())
    }

    /// Swept check against the analytic ground plane `y = 0` for sphere bottoms.
    fn apply_ground_ccd(
        body: &mut PhysicsBodyState,
        descriptor: &PhysicsBodyDescriptor,
        dt: f32,
        report: &mut PhysicsFrameReport,
    ) -> Result<(), PhysicsError> {
        let radius = descriptor.radius;
        let prev_bottom = body.previous_position[1] - radius;
        let cur_bottom = body.position[1] - radius;
        if !(prev_bottom > 0.0 && cur_bottom < 0.0) {
            return Ok(());
        }
        let disp = [
            body.position[0] - body.previous_position[0],
            body.position[1] - body.previous_position[1],
            body.position[2] - body.previous_position[2],
        ];
        let speed =
            (disp[0] * disp[0] + disp[1] * disp[1] + disp[2] * disp[2]).sqrt() / dt.max(1e-6);
        let threshold = radius * descriptor.ccd_threshold_per_substep.max(1e-4);
        if speed * dt <= threshold {
            return Ok(());
        }
        let y0 = prev_bottom;
        let y1 = cur_bottom;
        let denom = (y0 - y1).max(1e-6);
        let t = (y0 / denom).clamp(0.0, 1.0);
        for axis in 0..3 {
            let p0 = body.previous_position[axis];
            let p1 = body.position[axis];
            body.position[axis] = p0 + (p1 - p0) * t;
        }
        body.position[1] = body.position[1].max(radius);
        report.ccd_swept_bodies = report.ccd_swept_bodies.saturating_add(1);
        Ok(())
    }

    fn cpu_oracle_contacts(&self) -> Result<Vec<PhysicsContact>, PhysicsError> {
        for body in &self.bodies {
            let _ = self.descriptor(body.id)?;
        }
        Ok(cpu_oracle_collision_contacts(
            &self.bodies,
            &self.descriptors,
        ))
    }

    fn solve_contacts(
        &mut self,
        contacts: &[PhysicsContact],
        report: &mut PhysicsFrameReport,
    ) -> Result<(), PhysicsError> {
        for contact in contacts {
            match contact.other {
                None => self.resolve_static_contact(contact, report),
                Some(other) => self.resolve_pair_contact(contact, other, report),
            }
        }
        Ok(())
    }

    fn resolve_static_contact(
        &mut self,
        contact: &PhysicsContact,
        report: &mut PhysicsFrameReport,
    ) {
        let Some(body) = self
            .body_index
            .get(&contact.body)
            .and_then(|&idx| self.bodies.get_mut(idx))
        else {
            return;
        };
        // Push the body back along the contact normal; assumes the static
        // surface is the ground plane today, but the math generalises for any
        // analytic plane normal we publish in `normal_world`.
        for axis in 0..3 {
            body.position[axis] += contact.normal_world[axis] * contact.penetration;
        }
        // Cancel inbound velocity along the contact normal.
        let v_dot_n = body.linear_velocity[0] * contact.normal_world[0]
            + body.linear_velocity[1] * contact.normal_world[1]
            + body.linear_velocity[2] * contact.normal_world[2];
        if v_dot_n < 0.0 {
            for axis in 0..3 {
                body.linear_velocity[axis] -= v_dot_n * contact.normal_world[axis];
            }
        }
        report.contacts_resolved = report.contacts_resolved.saturating_add(1);
    }

    fn resolve_pair_contact(
        &mut self,
        contact: &PhysicsContact,
        other_id: PhysicsBodyId,
        report: &mut PhysicsFrameReport,
    ) {
        let (Some(&i), Some(&j)) = (
            self.body_index.get(&contact.body),
            self.body_index.get(&other_id),
        ) else {
            return;
        };
        // Pull descriptors before grabbing mutable borrows.
        let inv_a = self
            .descriptors
            .get(&contact.body)
            .map(|d| {
                if d.class == PhysicsBodyClass::Dynamic {
                    d.inverse_mass
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let inv_b = self
            .descriptors
            .get(&other_id)
            .map(|d| {
                if d.class == PhysicsBodyClass::Dynamic {
                    d.inverse_mass
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let total_inv = inv_a + inv_b;
        if total_inv <= 0.0 {
            return;
        }
        let n = contact.normal_world;
        let depth = contact.penetration;
        let push_a = -depth * inv_a / total_inv;
        let push_b = depth * inv_b / total_inv;
        if i == j {
            return;
        }
        // Apply position corrections.
        if i < j {
            let (left, right) = self.bodies.split_at_mut(j);
            let a = &mut left[i];
            let b = &mut right[0];
            for axis in 0..3 {
                a.position[axis] += n[axis] * push_a;
                b.position[axis] += n[axis] * push_b;
            }
            // Cancel inbound relative velocity along the normal.
            let rel_n = (b.linear_velocity[0] - a.linear_velocity[0]) * n[0]
                + (b.linear_velocity[1] - a.linear_velocity[1]) * n[1]
                + (b.linear_velocity[2] - a.linear_velocity[2]) * n[2];
            if rel_n < 0.0 {
                let coeff_a = rel_n * inv_a / total_inv;
                let coeff_b = rel_n * inv_b / total_inv;
                for axis in 0..3 {
                    a.linear_velocity[axis] += n[axis] * coeff_a;
                    b.linear_velocity[axis] -= n[axis] * coeff_b;
                }
            }
            report.contacts_resolved = report.contacts_resolved.saturating_add(1);
        } else {
            let (left, right) = self.bodies.split_at_mut(i);
            let b = &mut left[j];
            let a = &mut right[0];
            for axis in 0..3 {
                a.position[axis] += n[axis] * push_a;
                b.position[axis] += n[axis] * push_b;
            }
            // Cancel inbound relative velocity along the normal.
            let rel_n = (b.linear_velocity[0] - a.linear_velocity[0]) * n[0]
                + (b.linear_velocity[1] - a.linear_velocity[1]) * n[1]
                + (b.linear_velocity[2] - a.linear_velocity[2]) * n[2];
            if rel_n < 0.0 {
                let coeff_a = rel_n * inv_a / total_inv;
                let coeff_b = rel_n * inv_b / total_inv;
                for axis in 0..3 {
                    a.linear_velocity[axis] += n[axis] * coeff_a;
                    b.linear_velocity[axis] -= n[axis] * coeff_b;
                }
            }
            report.contacts_resolved = report.contacts_resolved.saturating_add(1);
        }
    }
}

fn collision_batch(
    world: &PhysicsCollisionWorld,
    phase: &str,
    substep_index: u32,
    kind: CollisionQueryKind,
    contract_id: crate::collision_contract::CollisionContractId,
    items: Vec<CollisionBatchItem>,
) -> CollisionWorkloadBatch {
    CollisionWorkloadBatch::new(
        phase,
        format!("{phase}.substep_{substep_index}"),
        "physics",
        CollisionPlan::for_query_with_backend(kind, world.backend),
        contract_id,
        format!("physics_substep_{substep_index}"),
        world.capture.clone(),
        world.domain.clone(),
        CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        CollisionCertificationPolicy::CpuOracleParity,
        items,
        64,
    )
}

fn planned_substeps_for_plan(plan: &PhysicsPlan) -> u32 {
    plan.substeps
        .requested_substeps_per_tick
        .min(plan.substeps.max_substeps_per_tick.max(1))
        .max(1)
}

fn contact_key(
    body: PhysicsBodyId,
    other: Option<PhysicsBodyId>,
) -> (PhysicsBodyId, Option<PhysicsBodyId>) {
    match other {
        Some(other) if other < body => (other, Some(body)),
        _ => (body, other),
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn cpu_oracle_collision_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = cpu_oracle_world_contacts(bodies, descriptors);
    contacts.extend(cpu_oracle_body_body_contacts(bodies, descriptors));
    contacts
}

fn cpu_oracle_world_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = Vec::new();
    for body in bodies {
        let Some(descriptor) = descriptors.get(&body.id) else {
            continue;
        };
        let bottom = body.position[1] - descriptor.radius;
        if bottom < 0.0 {
            contacts.push(PhysicsContact {
                body: body.id,
                other: None,
                normal_world: [0.0, 1.0, 0.0],
                penetration: -bottom,
                generated_by_ccd: false,
            });
        }
    }
    contacts
}

fn cpu_oracle_body_body_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = Vec::new();
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            let body_a = &bodies[i];
            let body_b = &bodies[j];
            let (Some(desc_a), Some(desc_b)) =
                (descriptors.get(&body_a.id), descriptors.get(&body_b.id))
            else {
                continue;
            };
            let dx = body_b.position[0] - body_a.position[0];
            let dy = body_b.position[1] - body_a.position[1];
            let dz = body_b.position[2] - body_a.position[2];
            let d2 = dx * dx + dy * dy + dz * dz;
            let sum_r = desc_a.radius + desc_b.radius;
            if d2 >= sum_r * sum_r {
                continue;
            }
            let d = d2.sqrt().max(1e-6);
            contacts.push(PhysicsContact {
                body: body_a.id,
                other: Some(body_b.id),
                normal_world: [dx / d, dy / d, dz / d],
                penetration: sum_r - d,
                generated_by_ccd: false,
            });
        }
    }
    contacts
}

fn collision_results_to_physics_contacts(
    batch: &CollisionWorkloadBatch,
    results: &[CollisionResult],
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    batch
        .items
        .iter()
        .zip(results)
        .filter_map(|(item, result)| {
            let body = body_for_collision_item(item, bodies, descriptors)?;
            match result {
                CollisionResult::SphereOverlap(value) if value.overlaps => Some(PhysicsContact {
                    body,
                    other: None,
                    normal_world: normalize_or_up(value.witness.world_normal),
                    penetration: (-value.witness.signed_separation).max(0.0),
                    generated_by_ccd: false,
                }),
                CollisionResult::Sweep(value) => value.witness.map(|witness| PhysicsContact {
                    body,
                    other: None,
                    normal_world: normalize_or_up(witness.contact_normal),
                    penetration: distance(witness.point_on_probe, witness.point_on_world),
                    generated_by_ccd: true,
                }),
                CollisionResult::TimeOfImpact(value) => {
                    value.witness.map(|witness| PhysicsContact {
                        body,
                        other: None,
                        normal_world: normalize_or_up(witness.contact_normal),
                        penetration: distance(witness.point_on_probe, witness.point_on_world),
                        generated_by_ccd: true,
                    })
                }
                _ => None,
            }
        })
        .collect()
}

fn body_for_collision_item(
    item: &CollisionBatchItem,
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Option<PhysicsBodyId> {
    match item {
        CollisionBatchItem::SphereOverlap { center, radius } => bodies
            .iter()
            .find(|body| {
                descriptors.get(&body.id).is_some_and(|descriptor| {
                    approx_eq(*radius, descriptor.radius) && approx_vec3(*center, body.position)
                })
            })
            .map(|body| body.id),
        CollisionBatchItem::SphereSweep { sweep, .. }
        | CollisionBatchItem::SphereTimeOfImpact { sweep, .. } => bodies
            .iter()
            .find(|body| {
                descriptors.get(&body.id).is_some_and(|descriptor| {
                    approx_eq(sweep.radius, descriptor.radius)
                        && approx_vec3(sweep.start_center, body.previous_position)
                        && approx_vec3(sweep.end_center, body.position)
                })
            })
            .map(|body| body.id),
        _ => None,
    }
}

fn normalize_or_up(vector: [f32; 3]) -> [f32; 3] {
    let len = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if len <= 1e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [vector[0] / len, vector[1] / len, vector[2] / len]
    }
}

fn approx_vec3(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| approx_eq(*left, right))
}

fn approx_eq(left: f32, right: f32) -> bool {
    (left - right).abs() <= 1e-4
}

fn contact_sets_equivalent(left: &[PhysicsContact], right: &[PhysicsContact]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut matched = vec![false; right.len()];
    'left_contacts: for left_contact in left {
        for (idx, right_contact) in right.iter().enumerate() {
            if !matched[idx] && contacts_equivalent(left_contact, right_contact) {
                matched[idx] = true;
                continue 'left_contacts;
            }
        }
        return false;
    }
    true
}

fn contacts_equivalent(left: &PhysicsContact, right: &PhysicsContact) -> bool {
    let left = canonical_contact(left);
    let right = canonical_contact(right);
    left.body == right.body
        && left.other == right.other
        && left.generated_by_ccd == right.generated_by_ccd
        && approx_vec3(left.normal_world, right.normal_world)
        && approx_eq(left.penetration, right.penetration)
}

fn canonical_contact(contact: &PhysicsContact) -> PhysicsContact {
    let Some(other) = contact.other else {
        return contact.clone();
    };
    if other >= contact.body {
        return contact.clone();
    }
    PhysicsContact {
        body: other,
        other: Some(contact.body),
        normal_world: [
            -contact.normal_world[0],
            -contact.normal_world[1],
            -contact.normal_world[2],
        ],
        penetration: contact.penetration,
        generated_by_ccd: contact.generated_by_ccd,
    }
}

fn push_unique_finding(report: &mut PhysicsFrameReport, finding: &str) {
    if !report.findings.iter().any(|existing| existing == finding) {
        report.findings.push(finding.to_string());
    }
}
