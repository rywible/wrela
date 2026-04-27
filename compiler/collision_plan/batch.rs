use super::CollisionPlan;
use crate::collision_contract::{
    CollisionContractId, CollisionInputKind, CollisionRayInput, CollisionSnapshotTransitionInput,
    CollisionSphereSweepInput, CollisionTargetKind, collision_contract,
};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::state_advance::ChangeClass;
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionCertificationPolicy {
    MetricsOnly,
    CpuOracleParity,
    ExactRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollisionCandidateGroupingPolicy {
    PerItem,
    SharedCandidateDigest,
    SharedBroadphaseRegion,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollisionBatchItem {
    PointOccupancy {
        point: [f32; 3],
    },
    RayCast {
        ray: CollisionRayInput,
    },
    SphereOverlap {
        center: [f32; 3],
        radius: f32,
    },
    SphereSweep {
        transition: CollisionSnapshotTransitionInput,
        sweep: CollisionSphereSweepInput,
    },
    SphereTimeOfImpact {
        transition: CollisionSnapshotTransitionInput,
        sweep: CollisionSphereSweepInput,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionBatchValidationError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionWorkloadBatch {
    pub name: SmolStr,
    pub workload_id: SmolStr,
    pub scenario_id: SmolStr,
    pub plan: CollisionPlan,
    pub contract_id: CollisionContractId,
    pub snapshot_id: SmolStr,
    pub capture: KernelValue,
    pub domain: KernelValue,
    pub candidate_grouping: CollisionCandidateGroupingPolicy,
    pub certification_policy: CollisionCertificationPolicy,
    pub items: Vec<CollisionBatchItem>,
    pub chunk_size: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CollisionBatchExecutionReport {
    pub workload: SmolStr,
    pub plan_name: SmolStr,
    pub contract_id: SmolStr,
    pub query_count: u64,
    pub batch_count: u32,
    pub dispatch_count: u32,
    pub dispatch_items: u32,
    pub average_items_per_dispatch: f32,
    pub timestamps_supported: bool,
    pub timestamped_pass_count: u32,
    pub gpu_time_total_micros: u128,
    pub gpu_time_max_micros: u128,
    pub hot_path_readback_bytes: u64,
    pub queue_submit_count: u32,
    pub scene_reupload_bytes: u64,
    pub wgsl_selected_workgroup_size: u32,
    pub wgsl_resident_shared_snapshot_artifacts: u32,
    pub cpu_certification_query_count: u32,
    pub fallback_count: u32,
    pub witness_reuse_rate: f64,
    pub candidate_table_overflow_fallback_count: u32,
    pub total_candidate_count: u64,
    pub total_rejected_candidate_count: u64,
    pub total_pruned_node_count: u64,
    pub total_candidate_reduction_effectiveness: f64,
    pub total_interval_subdivisions: u64,
    pub total_interval_refinements: u64,
    pub total_certificate_successes: u64,
    pub available_count_total: u64,
    pub consumed_count_total: u64,
    pub rejected_count_total: u64,
    pub unavailable_count_total: u64,
    pub last_interval_bracket: Option<[f32; 2]>,
    pub contact_normal_provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionBatchResult {
    pub results: Vec<crate::collision_contract::CollisionResult>,
    pub report: CollisionBatchExecutionReport,
}

impl CollisionBatchItem {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::PointOccupancy { .. } => "point_occupancy",
            Self::RayCast { .. } => "ray_cast",
            Self::SphereOverlap { .. } => "sphere_overlap",
            Self::SphereSweep { .. } => "sphere_sweep",
            Self::SphereTimeOfImpact { .. } => "time_of_impact",
        }
    }

    pub fn input_kind(&self) -> CollisionInputKind {
        match self {
            Self::PointOccupancy { .. } => CollisionInputKind::Point,
            Self::RayCast { .. } => CollisionInputKind::Ray,
            Self::SphereOverlap { .. } => CollisionInputKind::SphereProbe,
            Self::SphereSweep { .. } | Self::SphereTimeOfImpact { .. } => {
                CollisionInputKind::SphereSweep
            }
        }
    }

    pub fn to_kernel_args(&self) -> Vec<KernelValue> {
        match self {
            Self::PointOccupancy { point } => vec![point_kernel_value(*point)],
            Self::RayCast { ray } => vec![ray_kernel_value(*ray)],
            Self::SphereOverlap { center, radius } => {
                vec![sphere_probe_kernel_value(*center, *radius)]
            }
            Self::SphereSweep { transition, sweep }
            | Self::SphereTimeOfImpact { transition, sweep } => vec![
                transition_kernel_value(*transition),
                sweep_kernel_value(*sweep),
            ],
        }
    }
}

fn item_matches_contract(item: &CollisionBatchItem, contract_id: CollisionContractId) -> bool {
    match item {
        CollisionBatchItem::PointOccupancy { .. } => {
            contract_id == crate::collision_contract::COLLISION_POINT_OCCUPANCY_WORLD
        }
        CollisionBatchItem::RayCast { .. } => {
            contract_id == crate::collision_contract::COLLISION_RAY_CAST_WORLD
        }
        CollisionBatchItem::SphereOverlap { .. } => {
            contract_id == crate::collision_contract::COLLISION_SPHERE_OVERLAP_WORLD
        }
        CollisionBatchItem::SphereSweep { .. } => {
            contract_id == crate::collision_contract::COLLISION_SPHERE_SWEEP_TRANSITION
        }
        CollisionBatchItem::SphereTimeOfImpact { .. } => {
            contract_id == crate::collision_contract::COLLISION_TIME_OF_IMPACT_TRANSITION
        }
    }
}

impl CollisionWorkloadBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<SmolStr>,
        workload_id: impl Into<SmolStr>,
        scenario_id: impl Into<SmolStr>,
        plan: CollisionPlan,
        contract_id: CollisionContractId,
        snapshot_id: impl Into<SmolStr>,
        capture: KernelValue,
        domain: KernelValue,
        candidate_grouping: CollisionCandidateGroupingPolicy,
        certification_policy: CollisionCertificationPolicy,
        items: Vec<CollisionBatchItem>,
        chunk_size: usize,
    ) -> Self {
        Self {
            name: name.into(),
            workload_id: workload_id.into(),
            scenario_id: scenario_id.into(),
            plan,
            contract_id,
            snapshot_id: snapshot_id.into(),
            capture,
            domain,
            candidate_grouping,
            certification_policy,
            items,
            chunk_size,
        }
    }

    pub fn checked(self) -> Result<Self, Vec<CollisionBatchValidationError>> {
        let errors = self.validate();
        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn query_count(&self) -> usize {
        self.items.len()
    }

    pub fn chunks(&self) -> std::slice::Chunks<'_, CollisionBatchItem> {
        self.items.chunks(self.chunk_size.max(1))
    }

    pub fn args_for_item(&self, item: &CollisionBatchItem) -> Vec<KernelValue> {
        let mut args = Vec::with_capacity(4);
        args.push(self.capture.clone());
        args.push(self.domain.clone());
        args.extend(item.to_kernel_args());
        args
    }

    pub fn validate(&self) -> Vec<CollisionBatchValidationError> {
        let mut errors = self
            .plan
            .validate()
            .into_iter()
            .map(|error| CollisionBatchValidationError {
                message: error.message,
            })
            .collect::<Vec<_>>();

        if self.plan.contract_id != self.contract_id {
            errors.push(validation_error(format!(
                "collision batch '{}' contract '{}' does not match plan contract '{}'",
                self.name, self.contract_id, self.plan.contract_id
            )));
        }
        if self.name.as_str().trim().is_empty() {
            errors.push(validation_error(
                "collision batch is missing a workload name",
            ));
        }
        if self.workload_id.as_str().trim().is_empty() {
            errors.push(validation_error("collision batch is missing a workload id"));
        }
        if self.scenario_id.as_str().trim().is_empty() {
            errors.push(validation_error("collision batch is missing a scenario id"));
        }
        if self.snapshot_id.as_str().trim().is_empty() {
            errors.push(validation_error("collision batch is missing a snapshot id"));
        }
        if self.chunk_size == 0 {
            errors.push(validation_error(
                "collision batch chunk_size must be greater than zero",
            ));
        }
        if self.items.is_empty() {
            errors.push(validation_error(
                "collision batch must contain at least one item",
            ));
        }
        if let Some(descriptor) = collision_contract(self.contract_id) {
            if matches!(descriptor.target, CollisionTargetKind::WorldTransition) {
                for (index, item) in self.items.iter().enumerate() {
                    if !matches!(
                        item,
                        CollisionBatchItem::SphereSweep { .. }
                            | CollisionBatchItem::SphereTimeOfImpact { .. }
                    ) {
                        errors.push(validation_error(format!(
                            "collision batch '{}' item {} kind '{}' does not match transition contract '{}'",
                            self.name,
                            index,
                            item.kind_name(),
                            descriptor.id
                        )));
                    }
                }
            }
            for (index, item) in self.items.iter().enumerate() {
                if !item_matches_contract(item, descriptor.id) {
                    errors.push(validation_error(format!(
                        "collision batch '{}' item {} kind '{}' does not match contract '{}'",
                        self.name,
                        index,
                        item.kind_name(),
                        descriptor.id
                    )));
                }
            }
        }

        errors
    }
}

impl CollisionBatchExecutionReport {
    pub fn new(batch: &CollisionWorkloadBatch) -> Self {
        Self {
            workload: batch.workload_id.clone(),
            plan_name: batch.plan.name.clone(),
            contract_id: batch.contract_id.as_str().into(),
            query_count: batch.items.len() as u64,
            batch_count: 1,
            ..Self::default()
        }
    }

    pub fn record_dispatch(&mut self, dispatch_items: usize) {
        self.dispatch_count = self.dispatch_count.saturating_add(1);
        self.dispatch_items = self.dispatch_items.saturating_add(dispatch_items as u32);
    }

    pub fn record_candidate_table(
        &mut self,
        table: &crate::collision_plan::CollisionCandidateTable,
    ) {
        self.total_candidate_count = self
            .total_candidate_count
            .saturating_add(table.total_candidate_count);
        self.total_rejected_candidate_count = self
            .total_rejected_candidate_count
            .saturating_add(table.total_rejected_candidate_count);
        self.total_pruned_node_count = self
            .total_pruned_node_count
            .saturating_add(table.total_pruned_node_count);
        self.candidate_table_overflow_fallback_count = self
            .candidate_table_overflow_fallback_count
            .saturating_add(table.overflow_fallback_item_count);
    }

    pub fn record_trace(&mut self, trace: &crate::collision_plan::CollisionExecutionTrace) {
        self.total_candidate_count = self
            .total_candidate_count
            .saturating_add(u64::from(trace.broadphase_candidate_count));
        self.total_rejected_candidate_count = self
            .total_rejected_candidate_count
            .saturating_add(u64::from(trace.broadphase_rejected_candidate_count));
        self.total_pruned_node_count = self
            .total_pruned_node_count
            .saturating_add(u64::from(trace.broadphase_pruned_node_count));
        if let Some(metrics) = &trace.wgsl_metrics {
            self.total_candidate_reduction_effectiveness +=
                f64::from(metrics.candidate_reduction_effectiveness);
        }
        self.cpu_certification_query_count = self.cpu_certification_query_count.saturating_add(
            trace
                .executed_query_contracts
                .iter()
                .filter(|contract| {
                    matches!(
                        **contract,
                        crate::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE
                            | crate::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE
                            | crate::query_contract::SPATIAL_DISTANCE_WORLD
                            | crate::query_contract::SPATIAL_NORMAL_WORLD
                            | crate::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE
                    )
                })
                .count() as u32,
        );
        self.fallback_count = self.fallback_count.saturating_add(trace.fallback_count);
        self.total_interval_subdivisions = self
            .total_interval_subdivisions
            .saturating_add(u64::from(trace.interval_subdivisions));
        self.total_interval_refinements = self
            .total_interval_refinements
            .saturating_add(u64::from(trace.interval_refinements));
        self.total_certificate_successes = self
            .total_certificate_successes
            .saturating_add(u64::from(trace.certificate_successes));
        self.available_count_total = self
            .available_count_total
            .saturating_add(u64::from(trace.reuse_metrics.available_count));
        self.consumed_count_total = self
            .consumed_count_total
            .saturating_add(u64::from(trace.reuse_metrics.consumed_count));
        self.rejected_count_total = self
            .rejected_count_total
            .saturating_add(u64::from(trace.reuse_metrics.rejected_count));
        self.unavailable_count_total = self
            .unavailable_count_total
            .saturating_add(u64::from(trace.reuse_metrics.unavailable_count));
        if let Some(bracket) = trace.interval_bracket {
            self.last_interval_bracket = Some(match self.last_interval_bracket {
                Some(current) => [current[0].min(bracket[0]), current[1].max(bracket[1])],
                None => bracket,
            });
        }
        let provenance = trace
            .contact_normal_provenance
            .map(crate::collision_contract::collision_contact_normal_provenance_name)
            .map(str::to_string);
        match (
            self.contact_normal_provenance.as_deref(),
            provenance.as_deref(),
        ) {
            (None, Some(_)) => self.contact_normal_provenance = provenance,
            (Some(existing), Some(observed)) if existing == observed => {}
            (Some("mixed"), _) => {}
            (Some(_), Some(_)) => self.contact_normal_provenance = Some("mixed".to_string()),
            _ => {}
        }
        self.witness_reuse_rate += if trace.reuse_metrics.consumed_count > 0 {
            1.0
        } else {
            0.0
        };
    }

    pub fn record_gpu_runtime(&mut self, runtime: &crate::gpu_runtime::GpuRuntimeMetrics) {
        self.hot_path_readback_bytes = self
            .hot_path_readback_bytes
            .saturating_add(runtime.readback_bytes);
        self.queue_submit_count = self
            .queue_submit_count
            .saturating_add(runtime.queue_submit_count);
        self.scene_reupload_bytes = self
            .scene_reupload_bytes
            .saturating_add(runtime.scene_reupload_bytes);
    }

    pub fn record_gpu_timings(&mut self, timestamps_supported: bool, pass_elapsed_micros: &[u128]) {
        self.timestamps_supported |= timestamps_supported;
        self.timestamped_pass_count = self
            .timestamped_pass_count
            .saturating_add(pass_elapsed_micros.len() as u32);
        for elapsed in pass_elapsed_micros {
            self.gpu_time_total_micros = self.gpu_time_total_micros.saturating_add(*elapsed);
            self.gpu_time_max_micros = self.gpu_time_max_micros.max(*elapsed);
        }
    }

    pub fn record_gpu_observability(
        &mut self,
        observability: &crate::query_exec::QueryExecutionObservability,
    ) {
        self.wgsl_selected_workgroup_size = self
            .wgsl_selected_workgroup_size
            .max(observability.wgsl_selected_workgroup_size);
        self.wgsl_resident_shared_snapshot_artifacts = self
            .wgsl_resident_shared_snapshot_artifacts
            .saturating_add(observability.cache_resident_shared_snapshot_artifacts);
    }

    pub fn finish(&mut self) {
        self.average_items_per_dispatch = if self.dispatch_count == 0 {
            0.0
        } else {
            self.dispatch_items as f32 / self.dispatch_count as f32
        };
        if self.total_candidate_count > 0 || self.total_rejected_candidate_count > 0 {
            let total = self.total_candidate_count + self.total_rejected_candidate_count;
            self.total_candidate_reduction_effectiveness = if total == 0 {
                0.0
            } else {
                self.total_rejected_candidate_count as f64 / total as f64
            };
        } else if self.query_count > 0 {
            self.total_candidate_reduction_effectiveness /= self.query_count as f64;
        }
        self.witness_reuse_rate = if self.query_count == 0 {
            0.0
        } else {
            self.witness_reuse_rate / self.query_count as f64
        };
    }
}

fn validation_error(message: impl Into<String>) -> CollisionBatchValidationError {
    CollisionBatchValidationError {
        message: message.into(),
    }
}

fn point_kernel_value(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn ray_kernel_value(ray: CollisionRayInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionRayInput"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(ray.origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(ray.direction)),
            (
                SmolStr::new("max_distance"),
                KernelValue::F32(ray.max_distance),
            ),
            (SmolStr::new("min_step"), KernelValue::F32(ray.min_step)),
            (
                SmolStr::new("hit_epsilon"),
                KernelValue::F32(ray.hit_epsilon),
            ),
            (SmolStr::new("max_steps"), KernelValue::I32(ray.max_steps)),
        ],
    })
}

fn sphere_probe_kernel_value(center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (SmolStr::new("center"), KernelValue::Vec3(center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
        ],
    })
}

fn transition_kernel_value(transition: CollisionSnapshotTransitionInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSnapshotTransitionInput"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                KernelValue::U32(transition.current_snapshot_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                KernelValue::U32(transition.previous_snapshot_epoch),
            ),
            (
                SmolStr::new("change_class"),
                KernelValue::U32(match transition.change_class {
                    ChangeClass::None => 0,
                    ChangeClass::Presentation => 1,
                    ChangeClass::Structural => 2,
                    ChangeClass::Topology => 3,
                    ChangeClass::Identity => 4,
                    ChangeClass::Behavior => 5,
                    ChangeClass::Incompatible => 6,
                }),
            ),
        ],
    })
}

fn sweep_kernel_value(sweep: CollisionSphereSweepInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereSweepInput"),
        fields: vec![
            (
                SmolStr::new("start_center"),
                KernelValue::Vec3(sweep.start_center),
            ),
            (
                SmolStr::new("end_center"),
                KernelValue::Vec3(sweep.end_center),
            ),
            (SmolStr::new("radius"), KernelValue::F32(sweep.radius)),
            (
                SmolStr::new("contact_tolerance"),
                KernelValue::F32(sweep.contact_tolerance),
            ),
            (
                SmolStr::new("max_iterations"),
                KernelValue::I32(sweep.max_iterations),
            ),
        ],
    })
}
