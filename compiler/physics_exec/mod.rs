//! CPU XPBD-like physics executor (RFC 0011 Phase 67).

#![forbid(unsafe_code)]

use crate::collision_contract::{
    COLLISION_SPHERE_OVERLAP_WORLD, COLLISION_SPHERE_SWEEP_TRANSITION,
    COLLISION_TIME_OF_IMPACT_TRANSITION, CollisionSnapshotTransitionInput,
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
use crate::state_advance::ChangeClass;
use smol_str::SmolStr;
use std::collections::HashMap;
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
    pub findings: Vec<String>,
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
}

impl PhysicsSolver {
    pub fn new(plan: PhysicsPlan, bodies: Vec<PhysicsBodyState>) -> Self {
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
        }
    }

    pub fn backend(&self) -> PhysicsBackend {
        self.plan.backend
    }

    pub fn bodies(&self) -> &[PhysicsBodyState] {
        &self.bodies
    }

    pub fn step(&mut self, dt: f32) -> Result<PhysicsFrameReport, PhysicsError> {
        let requested = self.plan.substeps.requested_substeps_per_tick;
        let max = self.plan.substeps.max_substeps_per_tick.max(1);
        let substeps = requested.min(max).max(1);
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
        let sub_dt = dt / substeps as f32;
        for substep_index in 0..substeps {
            self.integrate(sub_dt, &mut report)?;
            if self.plan.backend == PhysicsBackend::CollisionBacked {
                self.record_collision_workload_intent(substep_index, sub_dt, &mut report)?;
            }
            let mut contacts = self.detect_ground_contacts()?;
            let body_pairs = self.broadphase_pairs();
            for (i, j) in body_pairs {
                if let Some(contact) = self.sphere_sphere_contact(i, j)? {
                    contacts.push(contact);
                }
            }
            report.contacts_detected = report
                .contacts_detected
                .saturating_add(contacts.len() as u32);
            self.record_contact_readback(contacts.len(), &mut report);
            self.warm_start_contacts(&contacts);
            self.solve_contacts(&contacts, &mut report)?;
            self.recompute_velocities_from_positions(sub_dt);
            self.solve_velocity_constraints(&contacts, &mut report)?;
        }
        Ok(report)
    }

    /// Produce the unique pairs of body indices that should be passed to the
    /// narrowphase. This is the analytic O(n^2) sweep that future GPU /
    /// `CollisionWorkloadBatch` backends will replace.
    fn broadphase_pairs(&self) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        let n = self.bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let a = &self.bodies[i];
                let b = &self.bodies[j];
                let dx = b.position[0] - a.position[0];
                let dy = b.position[1] - a.position[1];
                let dz = b.position[2] - a.position[2];
                let d2 = dx * dx + dy * dy + dz * dz;
                let r_a = self.descriptor(a.id).map(|d| d.radius).unwrap_or(0.0);
                let r_b = self.descriptor(b.id).map(|d| d.radius).unwrap_or(0.0);
                let sum_r = r_a + r_b;
                if d2 <= sum_r * sum_r {
                    pairs.push((i, j));
                }
            }
        }
        pairs
    }

    fn sphere_sphere_contact(
        &self,
        i: usize,
        j: usize,
    ) -> Result<Option<PhysicsContact>, PhysicsError> {
        let body_a = &self.bodies[i];
        let body_b = &self.bodies[j];
        let r_a = self.descriptor(body_a.id)?.radius;
        let r_b = self.descriptor(body_b.id)?.radius;
        let dx = body_b.position[0] - body_a.position[0];
        let dy = body_b.position[1] - body_a.position[1];
        let dz = body_b.position[2] - body_a.position[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        let sum_r = r_a + r_b;
        if d2 >= sum_r * sum_r {
            return Ok(None);
        }
        let d = d2.sqrt().max(1e-6);
        // Normal points from a -> b in world space.
        let normal = [dx / d, dy / d, dz / d];
        let penetration = sum_r - d;
        Ok(Some(PhysicsContact {
            body: body_a.id,
            other: Some(body_b.id),
            normal_world: normal,
            penetration,
            generated_by_ccd: false,
        }))
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
                "physics.broadphase",
                substep_index,
                CollisionQueryKind::SphereSweepTransition,
                COLLISION_SPHERE_SWEEP_TRANSITION,
                sweep_items,
            ));
        }
        if !toi_items.is_empty() {
            report.collision_batches.push(collision_batch(
                "physics.contact_readback",
                substep_index,
                CollisionQueryKind::SphereTimeOfImpactTransition,
                COLLISION_TIME_OF_IMPACT_TRANSITION,
                toi_items,
            ));
        }
        Ok(())
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

    fn detect_ground_contacts(&self) -> Result<Vec<PhysicsContact>, PhysicsError> {
        let mut contacts = Vec::new();
        for body in &self.bodies {
            let descriptor = self.descriptor(body.id)?;
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
        Ok(contacts)
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
        // SAFETY: `i != j` because `broadphase_pairs` only emits `i < j`.
        debug_assert!(i != j);
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
        }
        report.contacts_resolved = report.contacts_resolved.saturating_add(1);
    }
}

fn collision_batch(
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
        CollisionPlan::for_query_with_backend(kind, DispatchBackend::Wgsl),
        contract_id,
        format!("physics_substep_{substep_index}"),
        KernelValue::Capture(SmolStr::new("physics_body_state")),
        KernelValue::Capture(SmolStr::new("physics_domain")),
        CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        CollisionCertificationPolicy::CpuOracleParity,
        items,
        64,
    )
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

fn push_unique_finding(report: &mut PhysicsFrameReport, finding: &str) {
    if !report.findings.iter().any(|existing| existing == finding) {
        report.findings.push(finding.to_string());
    }
}
