//! Owns the `DirectQueryEvaluator` wrapper plus observability/accounting helpers
//! shared by CPU query execution paths.
//! Does not own the world/batch traversal algorithms themselves.
//!
//! Key invariants:
//! - observability mutations must describe executed behavior, not intended
//!   behavior.
//! - helper counters stay saturating/monotonic so repeated execution can be
//!   merged safely.
//!
//! Primary entrypoints:
//! - `DirectQueryEvaluator::new`
//! - `DirectQueryOps` observability helpers in this module
//!
//! Failure modes / common pitfalls:
//! - double-counting a fallback or dispatch here distorts closure and perf
//!   reports far away from the query that caused it.

use super::*;

impl<'a> std::ops::Deref for DirectQueryEvaluator<'a> {
    type Target = DirectQueryOps<'a>;

    fn deref(&self) -> &Self::Target {
        &self.ops
    }
}

impl<'a> DirectQueryEvaluator<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, None, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
    ) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, snapshot, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        let observability = Rc::new(RefCell::new(QueryExecutionObservability::default()));
        Self {
            ops: DirectQueryOps::with_observability_and_snapshot_and_solver_mode(
                ctx,
                snapshot,
                observability,
                solver_mode,
            ),
        }
    }

    pub(crate) fn snapshot_observability(&self) -> QueryExecutionObservability {
        self.ops.snapshot_observability()
    }

    pub(crate) fn note_dispatch(&self) {
        self.ops.note_dispatch();
    }
}

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn new(ctx: &'a QueryExecContext) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, None, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
    ) -> Self {
        Self::new_with_snapshot_and_solver_mode(ctx, snapshot, QueryTraceSolverMode::Hybrid)
    }

    pub(crate) fn new_with_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            snapshot,
            Rc::new(RefCell::new(QueryExecutionObservability::default())),
            solver_mode,
        )
    }

    pub(crate) fn with_observability(
        ctx: &'a QueryExecContext,
        observability: Rc<RefCell<QueryExecutionObservability>>,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            None,
            observability,
            QueryTraceSolverMode::Hybrid,
        )
    }

    pub(crate) fn with_observability_and_snapshot(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        observability: Rc<RefCell<QueryExecutionObservability>>,
    ) -> Self {
        Self::with_observability_and_snapshot_and_solver_mode(
            ctx,
            snapshot,
            observability,
            QueryTraceSolverMode::Hybrid,
        )
    }

    pub(crate) fn with_observability_and_snapshot_and_solver_mode(
        ctx: &'a QueryExecContext,
        snapshot: Option<&WorldSnapshotHandle>,
        observability: Rc<RefCell<QueryExecutionObservability>>,
        solver_mode: QueryTraceSolverMode,
    ) -> Self {
        Self {
            ctx,
            snapshot: snapshot.cloned(),
            trace_solver_mode: solver_mode,
            observability,
            world_acceleration_cache: Rc::new(RefCell::new(HashMap::new())),
            shape_union_cache: Rc::new(RefCell::new(HashMap::new())),
        }
        .with_seeded_cache_diagnostics()
    }

    pub(crate) fn with_seeded_cache_diagnostics(self) -> Self {
        let shared_snapshot_artifacts = ready_shared_cache_artifact_count(self.ctx);
        let observer_local_artifacts = 0;
        self.update_observability(|observability| {
            observability.cache_resident_shared_snapshot_artifacts = shared_snapshot_artifacts;
            observability.cache_resident_observer_local_artifacts = observer_local_artifacts;
            observability.cache_upload_attempts = shared_snapshot_artifacts;
            observability.cache_upload_rejections = observer_local_artifacts;
        });
        self
    }

    pub(crate) fn snapshot_observability(&self) -> QueryExecutionObservability {
        self.observability.borrow().clone()
    }

    pub(crate) fn context(&self) -> &'a QueryExecContext {
        self.ctx
    }

    pub(crate) fn authoritative_snapshot(
        &self,
        kind: SnapshotCaptureKind,
    ) -> Option<&WorldSnapshotHandle> {
        self.snapshot
            .as_ref()
            .filter(|snapshot| snapshot.kind() == kind)
    }

    pub(crate) fn ensure_snapshot_epoch(
        &self,
        kind: &'static str,
        name: &SmolStr,
        handle: &WorldSnapshotHandle,
        found_epoch: u32,
    ) -> Result<(), QueryExecError> {
        let expected = handle.portable_epoch();
        if expected == found_epoch {
            return Ok(());
        }
        Err(QueryExecError::SnapshotEpochMismatch {
            kind,
            name: name.clone(),
            expected,
            found: found_epoch,
        })
    }

    pub(crate) fn update_observability<F>(&self, update: F)
    where
        F: FnOnce(&mut QueryExecutionObservability),
    {
        let mut observability = self.observability.borrow_mut();
        update(&mut observability);
    }

    pub(crate) fn note_dispatch(&self) {
        self.update_observability(|observability| observability.dispatch_count += 1);
    }

    pub(crate) fn note_candidate_count(&self, count: u32) {
        self.update_observability(|observability| {
            observability.candidate_count += count;
            observability.candidates_after_pruning += count;
            observability.candidates_before_pruning += count;
        });
    }

    pub(crate) fn note_support_pruned_candidates(&self, count: u32) {
        self.update_observability(|observability| {
            observability.support_pruned_candidates += count;
            observability.candidates_before_pruning += count;
        });
    }

    pub(crate) fn note_batch_dispatch_shape(&self, items: u32, world_batch: bool) {
        self.note_batch_dispatch_grid(items, items.max(1), 1, 1, world_batch);
    }

    pub(crate) fn note_world_batch_items(&self, items: u32) {
        self.update_observability(|observability| {
            observability.world_batch_item_count += items;
            observability.screen_sample_count += items;
        });
    }

    pub(crate) fn note_batch_dispatch_grid(
        &self,
        items: u32,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
        world_batch: bool,
    ) {
        self.update_observability(|observability| {
            observability.dispatch_items += items;
            observability.dispatch_workgroups_x =
                observability.dispatch_workgroups_x.max(workgroups_x);
            observability.dispatch_workgroups_y =
                observability.dispatch_workgroups_y.max(workgroups_y);
            observability.dispatch_workgroups_z =
                observability.dispatch_workgroups_z.max(workgroups_z);
            if world_batch {
                observability.world_batch_item_count += items;
                observability.screen_sample_count += items;
            }
        });
    }

    pub(crate) fn note_batch_execution_mode(&self, semantic_pruned: bool) {
        self.update_observability(|observability| {
            if semantic_pruned {
                observability.semantic_pruned_batches += 1;
            } else {
                observability.dense_compatibility_batches += 1;
            }
        });
    }

    pub(crate) fn note_solver_plan(&self, plan: &RaySolverPlan) {
        self.note_solver_plan_with_methods(plan, &plan.runtime_methods());
    }

    pub(crate) fn note_solver_plan_with_methods(
        &self,
        plan: &RaySolverPlan,
        methods: &[RaySolverMethod],
    ) {
        self.update_observability(|observability| {
            observability.solver_plan_id = Some(plan.id.clone());
            observability.solver_subject = Some(plan.subject.clone());
            for method in methods {
                if !observability.solver_methods.contains(method) {
                    observability.solver_methods.push(*method);
                }
            }
            observability.observer_continuation_seed_hits += plan
                .continuation_intents
                .iter()
                .filter(|intent| {
                    matches!(
                        intent.disposition,
                        crate::query_solver::RaySolverIntentDisposition::Used
                    )
                })
                .count() as u32;
        });
    }

    pub(crate) fn note_step_certificate(&self, certificate: &RayStepCertificate) {
        self.update_observability(|observability| {
            *observability
                .step_certificate_kinds
                .entry(certificate.kind)
                .or_insert(0) += 1;
            if !observability
                .step_certificate_metadata
                .iter()
                .any(|metadata| {
                    same_observability_certificate_metadata(metadata, &certificate.metadata)
                })
            {
                observability
                    .step_certificate_metadata
                    .push(certificate.metadata.clone());
            }
        });
    }

    pub(crate) fn note_acceleration_node_visit(&self) {
        self.update_observability(|observability| observability.acceleration_node_visits += 1);
    }

    pub(crate) fn note_shape_leaf_visit(&self) {
        self.update_observability(|observability| observability.shape_leaf_visits += 1);
    }

    pub(crate) fn note_acceleration_pruned_node(&self) {
        self.update_observability(|observability| observability.acceleration_pruned_nodes += 1);
    }

    pub(crate) fn note_union_cluster_visit(&self) {
        self.update_observability(|observability| observability.union_cluster_visits += 1);
    }

    pub(crate) fn note_ray_support_interval_rejection(&self) {
        self.update_observability(|observability| {
            observability.ray_support_interval_rejections += 1
        });
    }

    pub(crate) fn note_ray_support_entry_jump(&self) {
        self.update_observability(|observability| observability.ray_support_entry_jumps += 1);
    }

    pub(crate) fn note_repeat_cell_skip(&self) {
        self.update_observability(|observability| observability.repeat_cell_skips += 1);
    }

    pub(crate) fn note_solver_repeat_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_repeat_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::RepeatAwareTraversal)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::RepeatAwareTraversal);
            }
        });
    }

    pub(crate) fn note_solver_repeat_supported(&self) {
        self.update_observability(|observability| observability.solver_repeat_supported += 1);
    }

    pub(crate) fn note_solver_repeat_inapplicable(&self) {
        self.update_observability(|observability| observability.solver_repeat_inapplicable += 1);
    }

    pub(crate) fn note_solver_repeat_unsupported(&self) {
        self.update_observability(|observability| observability.solver_repeat_unsupported += 1);
    }

    pub(crate) fn note_solver_repeat_unsupported_reason(
        &self,
        reason: RepeatAwareUnsupportedReason,
    ) {
        self.update_observability(|observability| match reason {
            RepeatAwareUnsupportedReason::Form => observability.solver_repeat_unsupported_form += 1,
            RepeatAwareUnsupportedReason::Bounds => {
                observability.solver_repeat_unsupported_bounds += 1
            }
        });
    }

    pub(crate) fn note_solver_repeat_cells_enumerated(&self, count: u32) {
        self.update_observability(|observability| {
            observability.solver_repeat_cells_enumerated = observability
                .solver_repeat_cells_enumerated
                .saturating_add(count);
        });
    }

    pub(crate) fn note_cache_brick_visit(&self) {
        self.update_observability(|observability| observability.cache_brick_visits += 1);
    }

    pub(crate) fn note_cache_brick_hit(&self) {
        self.update_observability(|observability| observability.cache_brick_hits += 1);
    }

    pub(crate) fn note_cache_brick_miss(&self) {
        self.update_observability(|observability| observability.cache_brick_misses += 1);
    }

    pub(crate) fn note_cache_interval_advance(&self) {
        self.update_observability(|observability| observability.cache_interval_advances += 1);
    }

    pub(crate) fn note_cache_budget_rejection(&self) {
        self.update_observability(|observability| observability.cache_budget_rejections += 1);
    }

    pub(crate) fn note_cache_dense_fallback(&self) {
        self.update_observability(|observability| observability.cache_dense_fallback_rays += 1);
    }

    pub(crate) fn note_cache_disable_reasons(&self, reasons: &[CacheDisableReason]) {
        if reasons
            .iter()
            .copied()
            .any(cache_disable_reason_is_budget_pressure)
        {
            self.note_cache_budget_rejection();
        }
    }

    pub(crate) fn note_accepted_relaxed_step(&self) {
        self.update_observability(|observability| observability.accepted_relaxed_steps += 1);
    }

    pub(crate) fn note_rejected_relaxed_step(&self) {
        self.update_observability(|observability| observability.rejected_relaxed_steps += 1);
    }

    pub(crate) fn note_solver_relaxed_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::LipschitzSafeStepping)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::LipschitzSafeStepping);
            }
        });
    }

    pub(crate) fn note_solver_relaxed_no_root_advance(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_no_root_advances += 1;
        });
    }

    pub(crate) fn note_solver_relaxed_bracket(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_brackets += 1;
        });
    }

    pub(crate) fn note_solver_relaxed_unresolved(&self) {
        self.update_observability(|observability| {
            observability.solver_relaxed_unresolved += 1;
        });
    }

    pub(crate) fn note_analytic_transformed_hit(&self) {
        self.update_observability(|observability| observability.analytic_transformed_hits += 1);
    }

    pub(crate) fn note_interval_subdivision(&self) {
        self.update_observability(|observability| observability.interval_subdivisions += 1);
    }

    pub(crate) fn note_interval_proof_success(&self) {
        self.update_observability(|observability| observability.interval_proof_successes += 1);
    }

    pub(crate) fn note_solver_interval_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::IntervalNewtonIsolation)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::IntervalNewtonIsolation);
            }
        });
    }

    pub(crate) fn note_solver_interval_no_root_advance(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_no_root_advances += 1;
        });
    }

    pub(crate) fn note_solver_interval_bracket(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_brackets += 1;
        });
    }

    pub(crate) fn note_solver_interval_unresolved(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_unresolved += 1;
        });
    }

    pub(crate) fn note_solver_dense_fallback(&self, reason: RaySolverFallbackReason) {
        self.update_observability(|observability| {
            observability.solver_dense_fallback_rays += 1;
            match reason {
                RaySolverFallbackReason::ContractRequiresDenseOracle => {
                    observability.solver_fallback_contract_dense += 1;
                }
                RaySolverFallbackReason::MissingFieldFacts => {
                    observability.solver_fallback_missing_facts += 1;
                }
                RaySolverFallbackReason::AnalyticUnsupported => {
                    observability.solver_fallback_analytic_unsupported += 1;
                }
                RaySolverFallbackReason::VerificationFailed => {
                    observability.solver_fallback_verification_failed += 1;
                }
                RaySolverFallbackReason::UnsupportedBackend => {
                    observability.solver_fallback_unsupported_backend += 1;
                }
            }
        });
    }

    pub(crate) fn note_solver_dense_fallback_reasons(&self, reasons: &[RaySolverFallbackReason]) {
        if reasons.is_empty() {
            self.note_solver_dense_fallback(RaySolverFallbackReason::ContractRequiresDenseOracle);
            return;
        }
        self.update_observability(|observability| {
            // Invariant: this counts one solver ray that fell back to dense
            // execution. The per-reason counters may fan out below, but the top
            // level fallback rate must stay comparable to ray traffic.
            observability.solver_dense_fallback_rays += 1;
            for reason in reasons {
                match reason {
                    RaySolverFallbackReason::ContractRequiresDenseOracle => {
                        observability.solver_fallback_contract_dense += 1;
                    }
                    RaySolverFallbackReason::MissingFieldFacts => {
                        observability.solver_fallback_missing_facts += 1;
                    }
                    RaySolverFallbackReason::AnalyticUnsupported => {
                        observability.solver_fallback_analytic_unsupported += 1;
                    }
                    RaySolverFallbackReason::VerificationFailed => {
                        observability.solver_fallback_verification_failed += 1;
                    }
                    RaySolverFallbackReason::UnsupportedBackend => {
                        observability.solver_fallback_unsupported_backend += 1;
                    }
                }
            }
        });
    }

    pub(crate) fn note_solver_generated_dense_fallback(&self, plan: &RaySolverPlan) {
        self.note_solver_plan(plan);
        self.update_observability(|observability| {
            observability.solver_generated_dense_fallback_rays += 1;
            observability.solver_fallback_unsupported_backend += 1;
        });
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_step_selected(&self) {
        self.update_observability(|observability| observability.learned_step_selected += 1);
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_step_verified(&self) {
        self.update_observability(|observability| observability.learned_step_verified += 1);
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_step_rejected(&self) {
        self.update_observability(|observability| observability.learned_step_rejected += 1);
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_step_bypassed(&self) {
        self.update_observability(|observability| observability.learned_step_bypassed += 1);
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_verifier_acceptance(&self) {
        self.update_observability(|observability| observability.learned_verifier_acceptances += 1);
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn note_learned_verifier_fallback(&self) {
        self.update_observability(|observability| observability.learned_verifier_fallbacks += 1);
    }

    pub(crate) fn note_solver_analytic_hit(&self) {
        self.update_observability(|observability| {
            observability.solver_analytic_hits += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::AnalyticPrimitiveIntersection)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::AnalyticPrimitiveIntersection);
            }
        });
    }

    pub(crate) fn note_solver_support_rejection(&self) {
        self.update_observability(|observability| {
            observability.solver_support_rejections += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SupportBoundCandidateRejection)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SupportBoundCandidateRejection);
            }
        });
    }

    pub(crate) fn note_solver_lipschitz_step(&self) {
        self.update_observability(|observability| {
            observability.solver_lipschitz_steps += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::LipschitzSafeStepping)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::LipschitzSafeStepping);
            }
        });
    }

    pub(crate) fn note_solver_interval_skip(&self) {
        self.update_observability(|observability| {
            observability.solver_interval_skips += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::IntervalNewtonIsolation)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::IntervalNewtonIsolation);
            }
        });
    }

    pub(crate) fn note_solver_newton_refinement(&self) {
        self.update_observability(|observability| {
            observability.solver_newton_refinements += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SafeguardedNewtonRefinement)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SafeguardedNewtonRefinement);
            }
        });
    }

    pub(crate) fn note_solver_refinement_attempt(&self) {
        self.update_observability(|observability| {
            observability.solver_refinement_attempts += 1;
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::SafeguardedNewtonRefinement)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::SafeguardedNewtonRefinement);
            }
        });
    }

    pub(crate) fn note_solver_refinement_failure(&self) {
        self.update_observability(|observability| {
            observability.solver_refinement_failures += 1;
        });
    }

    pub(crate) fn note_solver_repeat_aware_traversal(&self) {
        self.update_observability(|observability| {
            if !observability
                .solver_methods
                .contains(&RaySolverMethod::RepeatAwareTraversal)
            {
                observability
                    .solver_methods
                    .push(RaySolverMethod::RepeatAwareTraversal);
            }
        });
    }

    pub(crate) fn note_solver_adaptive_epsilon(&self) {
        self.update_observability(|observability| {
            observability.solver_adaptive_epsilon_uses += 1;
        });
    }

    pub(crate) fn note_solver_certificate_failure(&self) {
        self.update_observability(|observability| {
            observability.solver_certificate_failures += 1;
        });
    }

    pub(crate) fn note_hit_result(&self, hit: bool, steps: u32) {
        self.update_observability(|observability| {
            if hit {
                observability.hit_count += 1;
            } else {
                observability.miss_count += 1;
            }
            observability.trace_steps_max = observability.trace_steps_max.max(steps);
        });
    }

    pub(crate) fn note_branch_visit(&self) {
        self.update_observability(|observability| observability.branch_visits += 1);
    }

    pub(crate) fn note_artifact_load(&self) {
        self.update_observability(|observability| observability.artifact_loads += 1);
    }

    pub(crate) fn note_opaque_fallback(&self) {
        self.update_observability(|observability| observability.opaque_fallbacks += 1);
    }

    pub(crate) fn note_trace_step(&self) {
        self.note_trace_steps(1);
    }

    pub(crate) fn note_trace_steps(&self, count: u32) {
        self.update_observability(|observability| {
            observability.trace_steps = observability.trace_steps.saturating_add(count);
            observability.trace_steps_max = observability.trace_steps_max.max(count);
        });
    }

    pub(crate) fn note_field_sample(&self) {
        self.update_observability(|observability| observability.field_samples += 1);
    }

    pub(crate) fn note_normal_role(&self, role: NormalRole) {
        self.update_observability(|observability| {
            observability.normal_role = Some(SmolStr::new(role.observability_tag()));
        });
    }

    pub(crate) fn note_contract_validation_failure(&self) {
        self.update_observability(|observability| observability.contract_validation_failures += 1);
    }
}
