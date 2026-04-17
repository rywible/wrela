//! Owns CPU ray/shape tracing algorithms and their observability wiring.
//! Does not own query-plan selection or portable value semantics.
//!
//! Key invariants:
//! - trace-step accounting must reflect the exact traversal path taken.
//! - solver, cache, and dense fallbacks may interleave, but the resulting hit or
//!   miss must still satisfy the CPU oracle contract.
//!
//! Primary entrypoints:
//! - `DirectQueryOps::trace_shape_impl`
//! - tracing helpers on `DirectQueryOps`
//!
//! Failure modes / common pitfalls:
//! - recording fallback reasons out of order makes the trace look cheaper or
//!   safer than the path that actually ran.

use super::*;

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn trace_shape_impl(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.trace_shape_with_policy(
            shape,
            origin,
            direction,
            0.0,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            &TraceLoopPolicy::dense_only(shape.clone()),
            None,
        )
    }

    pub(crate) fn trace_shape_dense_certificate_only(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        policy: &TraceLoopPolicy,
        runtime_plan: Option<&RaySolverPlan>,
    ) -> Result<KernelValue, QueryExecError> {
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        let mut dense_fallback_recorded = false;

        if start_travel > 0.0 {
            let certificate = self.support_entry_jump_certificate(policy, shape, 0.0, start_travel);
            self.note_step_certificate(&certificate);
        }

        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_shape_distance(shape, point)?;
            let state = TraceLoopState {
                travel,
                distance,
                adaptive_epsilon: hit_epsilon,
                sample: IntervalSample {
                    t: travel,
                    distance,
                    adaptive_epsilon: hit_epsilon,
                },
                step_bound: distance.max(min_step),
                previous_distance: None,
                consecutive_small_steps: 0,
                non_improving_distance: false,
            };
            if distance <= hit_epsilon {
                if !dense_fallback_recorded && runtime_plan.is_some() {
                    self.note_solver_dense_fallback_reasons(
                        runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                    );
                }
                let certificate = self.dense_distance_hit_certificate(policy, shape, &state);
                self.note_step_certificate(&certificate);
                return self.shape_hit_value(shape, travel, point, steps);
            }

            if !dense_fallback_recorded && runtime_plan.is_some() {
                self.note_solver_dense_fallback_reasons(
                    runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                );
                dense_fallback_recorded = true;
            }
            let certificate = self.dense_distance_advance_certificate(policy, shape, &state);
            self.note_step_certificate(&certificate);
            travel = certificate.t_end;
            steps += 1;
        }

        Ok(default_hit(origin))
    }

    pub(crate) fn trace_shape_with_policy(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        policy: &TraceLoopPolicy,
        runtime_plan: Option<&RaySolverPlan>,
    ) -> Result<KernelValue, QueryExecError> {
        if !self.ctx.shape_names.contains(shape) {
            return Ok(default_hit(origin));
        }
        if policy.is_dense_only() {
            return self.trace_shape_dense_certificate_only(
                shape,
                origin,
                direction,
                start_travel,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
                policy,
                runtime_plan,
            );
        }
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        let mut previous_distance = None;
        let mut consecutive_small_steps = 0u32;
        let mut dense_fallback_recorded = false;
        let mut cached_sample: Option<IntervalSample> = None;

        if start_travel > 0.0 {
            let certificate = self.support_entry_jump_certificate(policy, shape, 0.0, start_travel);
            self.note_step_certificate(&certificate);
        }

        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let sample = match cached_sample.take() {
                Some(sample) if (sample.t - travel).abs() <= f32::EPSILON => sample,
                _ => self.interval_sample(shape, origin, direction, travel, hit_epsilon)?,
            };
            let distance = sample.distance;
            let adaptive_epsilon = sample.adaptive_epsilon;
            let non_improving_distance = previous_distance.is_some_and(|previous| {
                distance >= previous - (adaptive_epsilon * 0.25).max(min_step * 0.05)
            });
            let state = TraceLoopState {
                travel,
                distance,
                adaptive_epsilon,
                sample,
                step_bound: distance.max(min_step),
                previous_distance,
                consecutive_small_steps,
                non_improving_distance,
            };
            let decision = self.next_shape_trace_certificate(
                shape,
                origin,
                direction,
                max_distance,
                min_step,
                hit_epsilon,
                &state,
                policy,
            )?;

            match decision {
                TraceStepDecision::Advance {
                    certificate,
                    next_sample,
                } => {
                    if !dense_fallback_recorded
                        && matches!(certificate.kind, StepCertificateKind::DenseDistanceBound)
                        && runtime_plan.is_some()
                    {
                        self.note_solver_dense_fallback_reasons(
                            runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                        );
                        dense_fallback_recorded = true;
                    }
                    self.note_step_certificate(&certificate);
                    previous_distance = Some(distance);
                    let next_travel = certificate.t_end.max(travel);
                    if next_travel <= travel + f32::EPSILON {
                        self.note_solver_certificate_failure();
                        break;
                    }
                    let advance = next_travel - travel;
                    let stall_threshold = (min_step * 1.5).max(adaptive_epsilon * 8.0);
                    consecutive_small_steps = if advance <= stall_threshold {
                        consecutive_small_steps.saturating_add(1)
                    } else {
                        0
                    };
                    cached_sample =
                        next_sample.filter(|sample| (sample.t - next_travel).abs() <= f32::EPSILON);
                    travel = next_travel;
                    steps += 1;
                }
                TraceStepDecision::Hit(certificate) => {
                    if !dense_fallback_recorded
                        && matches!(certificate.kind, StepCertificateKind::DenseDistanceBound)
                        && runtime_plan.is_some()
                    {
                        self.note_solver_dense_fallback_reasons(
                            runtime_plan.expect("runtime plan").dense_fallback_reasons(),
                        );
                    }
                    self.note_step_certificate(&certificate);
                    if matches!(certificate.kind, StepCertificateKind::AnalyticHit) {
                        self.note_solver_analytic_hit();
                    }
                    let hit_point = [
                        origin[0] + direction[0] * certificate.t_end,
                        origin[1] + direction[1] * certificate.t_end,
                        origin[2] + direction[2] * certificate.t_end,
                    ];
                    return self.shape_hit_value(shape, certificate.t_end, hit_point, steps);
                }
                TraceStepDecision::Miss => {
                    break;
                }
            }
        }
        Ok(default_hit(origin))
    }

    pub(crate) fn solve_shape_ray(
        &self,
        solver_plan: &RaySolverPlan,
        artifact_contracts: &[ArtifactContract],
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        let start_travel = if start_travel <= 0.0 {
            match self.shape_cache_support_probe(
                shape,
                origin,
                direction,
                start_travel,
                max_distance,
            ) {
                RaySupportProbe::Interval(interval) => start_travel.max(interval.start_t.max(0.0)),
                RaySupportProbe::Rejected | RaySupportProbe::Unavailable => {
                    self.note_cache_dense_fallback();
                    start_travel
                }
            }
        } else {
            start_travel
        };
        let runtime_plan =
            self.runtime_shape_solver_plan(solver_plan, artifact_contracts, shape)?;
        let effective_methods = match self.trace_solver_mode {
            QueryTraceSolverMode::Hybrid => {
                runtime_plan.runtime_methods_for_observer(ArtifactObserver::Query)
            }
            QueryTraceSolverMode::DenseOnly => vec![RaySolverMethod::DenseSphereTracing],
        };
        let policy = match self.trace_solver_mode {
            QueryTraceSolverMode::Hybrid => TraceLoopPolicy::from_solver_plan(&runtime_plan),
            QueryTraceSolverMode::DenseOnly => {
                TraceLoopPolicy::dense_only(runtime_plan.subject.clone())
            }
        };
        self.note_solver_plan_with_methods(&runtime_plan, &effective_methods);
        if policy.method_enabled(RaySolverMethod::RepeatAwareTraversal) {
            self.note_solver_repeat_attempt();
            match self.try_repeat_linear_shape_hit(
                shape,
                origin,
                direction,
                start_travel,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
                &policy,
            )? {
                RepeatAwareTraceOutcome::Finished(hit) => {
                    self.note_solver_repeat_supported();
                    self.note_solver_repeat_aware_traversal();
                    return Ok(hit);
                }
                RepeatAwareTraceOutcome::Inapplicable(hit) => {
                    self.note_solver_repeat_inapplicable();
                    self.note_solver_repeat_aware_traversal();
                    return Ok(hit);
                }
                RepeatAwareTraceOutcome::Unsupported(reason) => {
                    self.note_solver_repeat_unsupported();
                    self.note_solver_repeat_unsupported_reason(reason);
                }
            }
        }
        self.trace_shape_with_policy(
            shape,
            origin,
            direction,
            start_travel,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            &policy,
            Some(&runtime_plan),
        )
    }

    pub(crate) fn runtime_shape_solver_plan(
        &self,
        solver_plan: &RaySolverPlan,
        artifact_contracts: &[ArtifactContract],
        shape: &SmolStr,
    ) -> Result<RaySolverPlan, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let evidence = match &scene.root {
            ShapeNode::Leaf(leaf) => {
                let field_evidence =
                    SemanticEvidence::for_field_scene(self.field_scene(&leaf.field)?)
                        .with_subject(shape.clone());
                let shape_evidence = SemanticEvidence::for_shape_scene(scene);
                field_evidence.refine_identity_with(
                    shape_evidence.identity.clone(),
                    "shape leaf identity overlay",
                )
            }
            _ => SemanticEvidence::for_shape_scene(scene),
        };
        let plan = RaySolverPlan::for_contract_with_subject(
            solver_plan.contract_id,
            shape.clone(),
            Some(evidence),
        )
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!(
                "contract '{}' is not a ray-shaped spatial solver contract",
                solver_plan.contract_id.as_str()
            ),
        })?;
        Ok(plan.with_artifact_reuse_resolution(
            Self::artifact_reuse_resolution_for_query_artifacts(artifact_contracts),
        ))
    }

    pub(crate) fn artifact_reuse_resolution_for_query_artifacts(
        artifact_contracts: &[ArtifactContract],
    ) -> RaySolverArtifactReuseResolution {
        let compatible_artifacts = artifact_contracts
            .iter()
            .filter_map(|artifact| match artifact.schema {
                ArtifactSchema::SupportSummary { .. } => Some("support-summary"),
                ArtifactSchema::CaptureCache { .. } => Some("capture-cache"),
                ArtifactSchema::CullingTable { .. } => Some("culling-table"),
                _ => None,
            })
            .collect::<Vec<_>>();
        if compatible_artifacts.is_empty() {
            return RaySolverArtifactReuseResolution {
                disposition: RaySolverIntentDisposition::Rejected,
                reasons: vec![SmolStr::new(
                    "query plan exposes no compatible solver artifacts for reuse",
                )],
            };
        }
        RaySolverArtifactReuseResolution {
            disposition: RaySolverIntentDisposition::Used,
            reasons: vec![
                SmolStr::new(format!(
                    "query plan requested compatible artifacts: {}",
                    compatible_artifacts.join(", ")
                )),
                SmolStr::new(
                    "artifact validity remains enforced by the query plan compatibility contract",
                ),
            ],
        }
    }

    pub(crate) fn next_shape_trace_certificate(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        state: &TraceLoopState,
        policy: &TraceLoopPolicy,
    ) -> Result<TraceStepDecision, QueryExecError> {
        if policy.method_enabled(RaySolverMethod::AnalyticPrimitiveIntersection) {
            match self.try_analytic_primitive_hit(
                shape,
                origin,
                direction,
                state.travel,
                max_distance,
                hit_epsilon,
                policy,
            )? {
                AnalyticRayHit::Hit(certificate) => {
                    self.note_analytic_transformed_hit();
                    self.note_interval_proof_success();
                    return Ok(TraceStepDecision::Hit(certificate));
                }
                AnalyticRayHit::VerificationFailed => {
                    self.note_solver_certificate_failure();
                }
                AnalyticRayHit::NotApplicable => {}
            }
        }

        if state.distance <= state.adaptive_epsilon {
            self.note_interval_proof_success();
            return Ok(TraceStepDecision::Hit(
                self.dense_distance_hit_certificate(policy, shape, state),
            ));
        }

        #[cfg(feature = "internal-learned-experiments")]
        if let Some(decision) =
            self.try_learned_trace_certificate(shape, origin, direction, state, policy)?
        {
            return Ok(decision);
        }

        if let Some(decision) = self.try_relaxed_or_interval_certificate(
            shape,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            state,
            policy,
        )? {
            return Ok(decision);
        }

        Ok(TraceStepDecision::Advance {
            certificate: self.dense_distance_advance_certificate(policy, shape, state),
            next_sample: None,
        })
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn try_learned_trace_certificate(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        state: &TraceLoopState,
        policy: &TraceLoopPolicy,
    ) -> Result<Option<TraceStepDecision>, QueryExecError> {
        let learned_selected = policy.method_enabled(RaySolverMethod::LearnedStepProposal);
        let learned_bound_enabled = policy.method_enabled(RaySolverMethod::ConservativeNeuralBound);
        if !learned_selected || !learned_bound_enabled {
            if learned_selected || learned_bound_enabled {
                self.note_learned_step_bypassed();
            }
            return Ok(None);
        }

        self.note_learned_step_selected();
        let point = [
            origin[0] + direction[0] * state.travel,
            origin[1] + direction[1] * state.travel,
            origin[2] + direction[2] * state.travel,
        ];
        let (proposal, bound) =
            propose_cpu_oracle_step(shape.clone(), point, direction, state.distance);
        let outcome = verify_learned_step(&proposal, &bound, state.distance);
        let dataset = build_cpu_oracle_dataset(
            shape.clone(),
            &proposal,
            &bound,
            &outcome,
            state.distance,
            Some(state.distance),
            Some([state.travel, state.travel + bound.conservative_step_bound]),
        );
        maybe_export_learned_oracle_dataset(&dataset).map_err(|err| {
            QueryExecError::Unsupported {
                message: format!("learned dataset export failed: {err}"),
            }
        })?;
        self.note_learned_step_verified();
        if outcome.accepted {
            self.note_learned_verifier_acceptance();
            return Ok(Some(TraceStepDecision::Advance {
                certificate: self.learned_conservative_advance_certificate(
                    policy, shape, state, &proposal, &bound,
                ),
                next_sample: None,
            }));
        }

        self.note_learned_step_rejected();
        self.note_learned_verifier_fallback();
        Ok(None)
    }

    #[cfg(feature = "internal-learned-experiments")]
    pub(crate) fn learned_conservative_advance_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        state: &TraceLoopState,
        proposal: &crate::acceleration::learned::LearnedStepProposal,
        bound: &crate::acceleration::learned::ConservativeNeuralBound,
    ) -> RayStepCertificate {
        let next_step = proposal.proposed_step.min(bound.conservative_step_bound);
        RayStepCertificate {
            kind: StepCertificateKind::RelaxedConservativeJump,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::ConservativeNoFalseMiss,
                policy,
                shape,
                RayStepCertificateSubjectKind::Interval,
                "learned-step-proposal",
                format!(
                    "proposal_step={:.6}; verifier_bound={:.6}; no_false_negative_intent={}",
                    proposal.proposed_step,
                    bound.conservative_step_bound,
                    bound.no_false_negative_intent
                ),
                CertificateReuseClass::RenderingAndCollision,
                vec![
                    SmolStr::new("learned proposal verified against conservative neural bound"),
                    SmolStr::new("distance semantics changed"),
                ],
            ),
            t_start: state.travel,
            t_end: state.travel + next_step,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        }
    }

    pub(crate) fn try_analytic_primitive_hit(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        hit_epsilon: f32,
        policy: &TraceLoopPolicy,
    ) -> Result<AnalyticRayHit, QueryExecError> {
        let Some(primitive_ray) = self.analytic_primitive_for_shape(shape, origin, direction)?
        else {
            return Ok(AnalyticRayHit::NotApplicable);
        };
        let travel = match primitive_ray.solve(start_travel, max_distance) {
            Some(travel) => travel,
            None => return Ok(AnalyticRayHit::NotApplicable),
        };
        let point = [
            origin[0] + direction[0] * travel,
            origin[1] + direction[1] * travel,
            origin[2] + direction[2] * travel,
        ];
        let adaptive_epsilon =
            self.shape_adaptive_hit_epsilon(shape, travel, point, hit_epsilon)?;
        let residual = self.eval_shape_distance(shape, point)?.abs();
        if residual > adaptive_epsilon {
            return Ok(AnalyticRayHit::VerificationFailed);
        }
        let certificate = RayStepCertificate {
            kind: StepCertificateKind::AnalyticHit,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Primitive,
                "analytic-primitive-hit",
                format!("hit_epsilon={hit_epsilon:.6}; adaptive_epsilon={adaptive_epsilon:.6}"),
                CertificateReuseClass::RenderingAndCollision,
                vec![
                    SmolStr::new("primitive arguments changed"),
                    SmolStr::new("safe transform chain changed"),
                    SmolStr::new("shape semantic identity changed"),
                ],
            ),
            t_start: start_travel.max(0.0),
            t_end: travel,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        };
        self.note_trace_steps(1);
        Ok(AnalyticRayHit::Hit(certificate))
    }

    pub(crate) fn support_entry_jump_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        t_start: f32,
        t_end: f32,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::SupportEntryJump,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::ConservativeNoFalseMiss,
                policy,
                shape,
                RayStepCertificateSubjectKind::SupportInterval,
                "support-entry-jump",
                "support-interval entry jump",
                CertificateReuseClass::RenderingAndCollision,
                vec![
                    SmolStr::new("support bounds changed"),
                    SmolStr::new("support semantics invalidated"),
                ],
            ),
            t_start,
            t_end,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        }
    }

    pub(crate) fn dense_distance_hit_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        state: &TraceLoopState,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::DenseDistanceBound,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Shape,
                "dense-distance-bound",
                format!(
                    "dense hit within adaptive epsilon {:.6}",
                    state.adaptive_epsilon
                ),
                CertificateReuseClass::RenderingAndCollision,
                vec![SmolStr::new("distance semantics changed")],
            ),
            t_start: state.travel,
            t_end: state.travel,
            no_hit_before_t_end: false,
            bracket: Some([state.travel, state.travel]),
            provenance: None,
        }
    }

    pub(crate) fn dense_distance_advance_certificate(
        &self,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        state: &TraceLoopState,
    ) -> RayStepCertificate {
        RayStepCertificate {
            kind: StepCertificateKind::DenseDistanceBound,
            metadata: self.certificate_metadata(
                RequiredGuaranteeClass::Exact,
                policy,
                shape,
                RayStepCertificateSubjectKind::Shape,
                "dense-distance-bound",
                format!("dense step {:.6}", state.step_bound),
                CertificateReuseClass::RenderingAndCollision,
                vec![SmolStr::new("distance semantics changed")],
            ),
            t_start: state.travel,
            t_end: state.travel + state.step_bound,
            no_hit_before_t_end: true,
            bracket: None,
            provenance: None,
        }
    }

    pub(crate) fn certificate_metadata(
        &self,
        guarantee: RequiredGuaranteeClass,
        policy: &TraceLoopPolicy,
        shape: &SmolStr,
        subject_kind: RayStepCertificateSubjectKind,
        proof_family: impl Into<SmolStr>,
        tolerance_context: impl Into<SmolStr>,
        reusable_by: CertificateReuseClass,
        invalidation_reasons: Vec<SmolStr>,
    ) -> RayStepCertificateMetadata {
        RayStepCertificateMetadata {
            guarantee,
            proof_family: proof_family.into(),
            subject: if policy.subject.is_empty() {
                shape.clone()
            } else {
                policy.subject.clone()
            },
            subject_kind,
            tolerance_context: tolerance_context.into(),
            reusable_by,
            invalidation_reasons,
        }
    }

    pub(crate) fn analytic_primitive_for_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<AnalyticPrimitiveRay>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let leaf = match &scene.root {
            ShapeNode::Leaf(leaf) => leaf,
            ShapeNode::Use { target } => {
                let target_scene = self.shape_scene(target)?;
                let ShapeNode::Leaf(leaf) = &target_scene.root else {
                    return Ok(None);
                };
                leaf
            }
            _ => return Ok(None),
        };
        let field = self.field_scene(&leaf.field)?;
        self.analytic_primitive_for_field_node(&field.root, origin, direction)
    }

    pub(crate) fn analytic_primitive_for_field_node(
        &self,
        node: &FieldNode,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<AnalyticPrimitiveRay>, QueryExecError> {
        match node {
            FieldNode::Use { target } => {
                let field = self.field_scene(target)?;
                self.analytic_primitive_for_field_node(&field.root, origin, direction)
            }
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.analytic_primitive_for_field_node(inner, origin, direction);
                };
                match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => {
                        let local_origin = self.eval_wrapped_point(*kind, param, origin)?;
                        let local_direction = self.eval_wrapped_vector(*kind, param, direction)?;
                        self.analytic_primitive_for_field_node(inner, local_origin, local_direction)
                    }
                    _ => Ok(None),
                }
            }
            FieldNode::Primitive { primitive, args } => {
                let primitive = match primitive {
                    hir::FieldPrimitive::Sphere => {
                        let radius = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "radius")?
                            .map(|value| expect_f32(Some(&value), "sphere radius"))
                            .transpose()?
                            .unwrap_or(1.0)
                            .abs();
                        AnalyticPrimitive::Sphere { radius }
                    }
                    hir::FieldPrimitive::Plane => {
                        let normal = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "normal")?
                            .map(|value| expect_vec3(Some(&value), "plane normal"))
                            .transpose()?
                            .unwrap_or([0.0, 1.0, 0.0]);
                        let offset = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "offset")?
                            .map(|value| expect_f32(Some(&value), "plane offset"))
                            .transpose()?
                            .unwrap_or(0.0);
                        AnalyticPrimitive::Plane { normal, offset }
                    }
                    hir::FieldPrimitive::Slab => {
                        let thickness = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "thickness")?
                            .map(|value| expect_f32(Some(&value), "slab thickness"))
                            .transpose()?
                            .unwrap_or(0.0)
                            .abs();
                        AnalyticPrimitive::Slab { thickness }
                    }
                    hir::FieldPrimitive::Box => {
                        let half = self
                            .eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "half")?
                            .map(|value| expect_vec3(Some(&value), "box half"))
                            .transpose()?
                            .unwrap_or([0.5, 0.5, 0.5]);
                        AnalyticPrimitive::Box { half }
                    }
                    hir::FieldPrimitive::Capsule => {
                        let a = self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "a")?;
                        let b = self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "b")?;
                        let radius =
                            self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "radius")?;
                        AnalyticPrimitive::Capsule {
                            a: expect_vec3(Some(&a), "capsule a")?,
                            b: expect_vec3(Some(&b), "capsule b")?,
                            radius: expect_f32(Some(&radius), "capsule radius")?.abs(),
                        }
                    }
                    hir::FieldPrimitive::Cylinder => {
                        let radius =
                            self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "radius")?;
                        let half_height = self
                            .eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "half_height")?;
                        AnalyticPrimitive::Cylinder {
                            radius: expect_f32(Some(&radius), "cylinder radius")?.abs(),
                            half_height: expect_f32(Some(&half_height), "cylinder half height")?
                                .abs(),
                        }
                    }
                    _ => return Ok(None),
                };
                Ok(Some(AnalyticPrimitiveRay {
                    primitive,
                    local_origin: origin,
                    local_direction: direction,
                }))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn try_repeat_linear_shape_hit(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
        _policy: &TraceLoopPolicy,
    ) -> Result<RepeatAwareTraceOutcome, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        let leaf = match &scene.root {
            ShapeNode::Leaf(leaf) => leaf,
            _ => {
                return Ok(RepeatAwareTraceOutcome::Unsupported(
                    RepeatAwareUnsupportedReason::Form,
                ));
            }
        };
        let field = self.field_scene(&leaf.field)?;
        let traversal = match &field.root {
            FieldNode::Repeat {
                kind: RepeatKind::RepeatLinear,
                param: Some(period_expr),
                inner,
            } => {
                let child_support = field
                    .support_node_record(field.root_support_id)
                    .and_then(|record| record.children.first().copied())
                    .ok_or(RepeatAwareUnsupportedReason::Bounds);
                let child_support = match child_support {
                    Ok(child_support) => child_support,
                    Err(reason) => return Ok(RepeatAwareTraceOutcome::Unsupported(reason)),
                };
                let Some(bounds) = self.field_support_bounds(field, child_support)? else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let period_value = self.eval_scene_constant(period_expr)?;
                let period = expect_vec3(Some(&period_value), "repeat linear period")?;
                RepeatLinearTraversal {
                    inner: inner.as_ref(),
                    local_origin: origin,
                    local_direction: direction,
                    bounds,
                    period,
                }
            }
            FieldNode::Transform {
                kind,
                param: Some(param),
                inner,
            } if matches!(
                kind,
                TransformKind::Translate | TransformKind::AffineTransform
            ) =>
            {
                let Some((local_origin, local_direction)) =
                    self.repeat_aware_local_ray(*kind, param, origin, direction)?
                else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Form,
                    ));
                };
                let FieldNode::Repeat {
                    kind: RepeatKind::RepeatLinear,
                    param: Some(period_expr),
                    inner,
                } = inner.as_ref()
                else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Form,
                    ));
                };
                let repeat_support = field
                    .support_node_record(field.root_support_id)
                    .and_then(|record| record.children.first().copied())
                    .and_then(|repeat_id| {
                        field
                            .support_node_record(repeat_id)
                            .and_then(|record| record.children.first().copied())
                    });
                let Some(child_support) = repeat_support else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let Some(bounds) = self.field_support_bounds(field, child_support)? else {
                    return Ok(RepeatAwareTraceOutcome::Unsupported(
                        RepeatAwareUnsupportedReason::Bounds,
                    ));
                };
                let period_value = self.eval_scene_constant(period_expr)?;
                let period = expect_vec3(Some(&period_value), "repeat linear period")?;
                RepeatLinearTraversal {
                    inner: inner.as_ref(),
                    local_origin,
                    local_direction,
                    bounds,
                    period,
                }
            }
            _ => {
                return Ok(RepeatAwareTraceOutcome::Unsupported(
                    RepeatAwareUnsupportedReason::Form,
                ));
            }
        };
        let Some(axis) = axis_aligned_repeat_axis(traversal.period) else {
            return Ok(RepeatAwareTraceOutcome::Unsupported(
                RepeatAwareUnsupportedReason::Form,
            ));
        };
        let cells = axis_aligned_repeat_linear_cells(
            traversal.bounds,
            axis,
            traversal.period[axis],
            traversal.local_origin,
            traversal.local_direction,
            start_travel.max(0.0),
            max_distance,
        );
        self.note_solver_repeat_cells_enumerated(cells.len().min(u32::MAX as usize) as u32);
        if cells.is_empty() {
            return Ok(RepeatAwareTraceOutcome::Inapplicable(default_hit(origin)));
        }

        for (offset, entry_t, exit_t) in cells {
            let local_origin = [
                traversal.local_origin[0] - offset[0],
                traversal.local_origin[1] - offset[1],
                traversal.local_origin[2] - offset[2],
            ];
            if let Some((travel, steps)) = self.trace_field_node_dense(
                traversal.inner,
                local_origin,
                traversal.local_direction,
                entry_t,
                exit_t,
                min_step,
                hit_epsilon,
                max_steps,
            )? {
                let point = [
                    origin[0] + direction[0] * travel,
                    origin[1] + direction[1] * travel,
                    origin[2] + direction[2] * travel,
                ];
                return self
                    .shape_hit_value(shape, travel, point, steps)
                    .map(RepeatAwareTraceOutcome::Finished);
            }
            self.note_repeat_cell_skip();
        }

        Ok(RepeatAwareTraceOutcome::Finished(default_hit(origin)))
    }

    pub(crate) fn repeat_aware_local_ray(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<([f32; 3], [f32; 3])>, QueryExecError> {
        let config = self.eval_scene_constant(param)?;
        match kind {
            TransformKind::Translate => {
                let translation = expect_vec3(Some(&config), "repeat-aware translation")?;
                Ok(Some((
                    [
                        origin[0] - translation[0],
                        origin[1] - translation[1],
                        origin[2] - translation[2],
                    ],
                    direction,
                )))
            }
            TransformKind::AffineTransform => {
                pure_translation_local_ray(&config, origin, direction)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn trace_field_node_dense(
        &self,
        node: &FieldNode,
        origin: [f32; 3],
        direction: [f32; 3],
        start_travel: f32,
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<Option<(f32, i32)>, QueryExecError> {
        let mut travel = start_travel.max(0.0);
        let mut steps = 0i32;
        while steps < max_steps && travel <= max_distance {
            self.note_trace_step();
            let point = [
                origin[0] + direction[0] * travel,
                origin[1] + direction[1] * travel,
                origin[2] + direction[2] * travel,
            ];
            let distance = self.eval_field_node(node, point)?;
            if distance <= hit_epsilon {
                return Ok(Some((travel, steps)));
            }
            travel += distance.max(min_step);
            steps += 1;
        }
        Ok(None)
    }

    pub(crate) fn try_relaxed_or_interval_certificate(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        state: &TraceLoopState,
        policy: &TraceLoopPolicy,
    ) -> Result<Option<TraceStepDecision>, QueryExecError> {
        let allow_bracket = self.shape_is_exact(shape)?;
        let lipschitz_bound = ray_parameter_lipschitz_bound(direction);

        if allow_bracket && policy.method_enabled(RaySolverMethod::LipschitzSafeStepping) {
            let factor = relaxed_step_factor(state.previous_distance, state.distance);
            let candidate_end = (state.travel + state.step_bound * factor).min(max_distance);
            let required_relaxed_end = state.travel + (state.step_bound * 1.5);
            let relaxed_admission = state.previous_distance.is_some()
                && (state.non_improving_distance || state.consecutive_small_steps >= 1);
            if relaxed_admission && candidate_end + f32::EPSILON >= required_relaxed_end {
                self.note_solver_relaxed_attempt();
                match self.prove_shape_interval(
                    shape,
                    origin,
                    direction,
                    state.sample,
                    candidate_end,
                    None,
                    hit_epsilon,
                    lipschitz_bound,
                    allow_bracket,
                    0,
                    2,
                )? {
                    IntervalProofOutcome::NoRoot { end_t, end_sample } => {
                        self.note_solver_relaxed_no_root_advance();
                        self.note_solver_lipschitz_step();
                        self.note_interval_proof_success();
                        self.note_accepted_relaxed_step();
                        return Ok(Some(TraceStepDecision::Advance {
                            certificate: RayStepCertificate {
                                kind: StepCertificateKind::RelaxedConservativeJump,
                                metadata: self.certificate_metadata(
                                    RequiredGuaranteeClass::IntervalBounded,
                                    policy,
                                    shape,
                                    RayStepCertificateSubjectKind::Interval,
                                    "lipschitz-relaxed-proof",
                                    format!("relaxed_factor={factor:.2}; lipschitz_bound={lipschitz_bound:.6}"),
                                    CertificateReuseClass::RenderingAndCollision,
                                    vec![
                                        SmolStr::new("lipschitz evidence changed"),
                                        SmolStr::new("distance semantics changed"),
                                    ],
                                ),
                                t_start: state.travel,
                                t_end: end_t,
                                no_hit_before_t_end: true,
                                bracket: None,
                                provenance: None,
                            },
                            next_sample: Some(end_sample),
                        }));
                    }
                    IntervalProofOutcome::Bracket { bracket } => {
                        self.note_solver_relaxed_bracket();
                        if let Some(certificate) = self.refine_shape_bracket(
                            shape,
                            origin,
                            direction,
                            bracket,
                            hit_epsilon,
                            policy,
                        )? {
                            return Ok(Some(TraceStepDecision::Hit(certificate)));
                        }
                        self.note_rejected_relaxed_step();
                        self.note_solver_relaxed_unresolved();
                        return Ok(None);
                    }
                    IntervalProofOutcome::Unresolved => {
                        self.note_rejected_relaxed_step();
                        self.note_solver_relaxed_unresolved();
                        return Ok(None);
                    }
                }
            }
        }

        let stall_step_threshold = (min_step * 2.0).max(state.adaptive_epsilon * 8.0);
        let hard_ray = state.consecutive_small_steps >= 2
            || (state.non_improving_distance && state.step_bound <= stall_step_threshold);
        if policy.method_enabled(RaySolverMethod::IntervalNewtonIsolation) && hard_ray {
            let candidate_end = (state.travel + state.step_bound * 2.0).min(max_distance);
            if candidate_end > state.travel + f32::EPSILON {
                self.note_solver_interval_attempt();
                match self.prove_shape_interval(
                    shape,
                    origin,
                    direction,
                    state.sample,
                    candidate_end,
                    None,
                    hit_epsilon,
                    lipschitz_bound,
                    allow_bracket,
                    0,
                    4,
                )? {
                    IntervalProofOutcome::NoRoot { end_t, end_sample } => {
                        self.note_solver_interval_no_root_advance();
                        self.note_solver_interval_skip();
                        self.note_interval_proof_success();
                        return Ok(Some(TraceStepDecision::Advance {
                            certificate: RayStepCertificate {
                                kind: StepCertificateKind::IntervalNoRootProof,
                                metadata: self.certificate_metadata(
                                    RequiredGuaranteeClass::IntervalBounded,
                                    policy,
                                    shape,
                                    RayStepCertificateSubjectKind::Interval,
                                    "interval-no-root-proof",
                                    format!("lipschitz_bound={lipschitz_bound:.6}; hard_ray=true"),
                                    CertificateReuseClass::RenderingAndCollision,
                                    vec![
                                        SmolStr::new("interval evidence changed"),
                                        SmolStr::new("distance semantics changed"),
                                    ],
                                ),
                                t_start: state.travel,
                                t_end: end_t,
                                no_hit_before_t_end: true,
                                bracket: None,
                                provenance: None,
                            },
                            next_sample: Some(end_sample),
                        }));
                    }
                    IntervalProofOutcome::Bracket { bracket } => {
                        self.note_solver_interval_bracket();
                        if let Some(certificate) = self.refine_shape_bracket(
                            shape,
                            origin,
                            direction,
                            bracket,
                            hit_epsilon,
                            policy,
                        )? {
                            return Ok(Some(TraceStepDecision::Hit(certificate)));
                        }
                        self.note_solver_interval_unresolved();
                        return Ok(None);
                    }
                    IntervalProofOutcome::Unresolved => {
                        self.note_solver_interval_unresolved();
                        self.note_solver_certificate_failure();
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    pub(crate) fn prove_shape_interval(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        start_sample: IntervalSample,
        end_t: f32,
        end_sample: Option<IntervalSample>,
        hit_epsilon: f32,
        lipschitz_bound: f32,
        allow_bracket: bool,
        depth: u32,
        max_depth: u32,
    ) -> Result<IntervalProofOutcome, QueryExecError> {
        self.note_interval_subdivision();
        if end_t <= start_sample.t + f32::EPSILON {
            return Ok(IntervalProofOutcome::Unresolved);
        }
        let span = end_t - start_sample.t;
        let end_sample_value = match end_sample {
            Some(sample) => sample,
            None => self.interval_sample(shape, origin, direction, end_t, hit_epsilon)?,
        };
        if end_sample_value.distance - lipschitz_bound * span > end_sample_value.adaptive_epsilon {
            return Ok(IntervalProofOutcome::NoRoot {
                end_t,
                end_sample: end_sample_value,
            });
        }
        if allow_bracket
            && (end_sample_value.distance.abs() <= end_sample_value.adaptive_epsilon
                || end_sample_value.distance < 0.0)
        {
            return Ok(IntervalProofOutcome::Bracket {
                bracket: BracketRefinement {
                    lo: start_sample,
                    hi: end_sample_value,
                },
            });
        }

        let mid_t = 0.5 * (start_sample.t + end_t);
        let mid_sample = self.interval_sample(shape, origin, direction, mid_t, hit_epsilon)?;
        let radius = 0.5 * span;
        if mid_sample.distance - lipschitz_bound * radius > mid_sample.adaptive_epsilon {
            return Ok(IntervalProofOutcome::NoRoot {
                end_t,
                end_sample: end_sample_value,
            });
        }

        if depth >= max_depth || radius <= hit_epsilon * 2.0 {
            return Ok(IntervalProofOutcome::Unresolved);
        }

        let left = self.prove_shape_interval(
            shape,
            origin,
            direction,
            start_sample,
            mid_t,
            Some(mid_sample),
            hit_epsilon,
            lipschitz_bound,
            allow_bracket,
            depth + 1,
            max_depth,
        )?;
        match left {
            IntervalProofOutcome::Bracket { .. } => return Ok(left),
            IntervalProofOutcome::NoRoot { .. } => {}
            IntervalProofOutcome::Unresolved => return Ok(IntervalProofOutcome::Unresolved),
        }

        let right = self.prove_shape_interval(
            shape,
            origin,
            direction,
            mid_sample,
            end_t,
            Some(end_sample_value),
            hit_epsilon,
            lipschitz_bound,
            allow_bracket,
            depth + 1,
            max_depth,
        )?;
        match right {
            IntervalProofOutcome::Bracket { .. } => Ok(right),
            IntervalProofOutcome::NoRoot { end_sample, .. } => {
                Ok(IntervalProofOutcome::NoRoot { end_t, end_sample })
            }
            IntervalProofOutcome::Unresolved => Ok(IntervalProofOutcome::Unresolved),
        }
    }

    pub(crate) fn refine_shape_bracket(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        bracket: BracketRefinement,
        hit_epsilon: f32,
        policy: &TraceLoopPolicy,
    ) -> Result<Option<RayStepCertificate>, QueryExecError> {
        if !policy.method_enabled(RaySolverMethod::SafeguardedNewtonRefinement) {
            return Ok(None);
        }
        self.note_solver_refinement_attempt();
        let mut lo = bracket.lo;
        let mut hi = bracket.hi;
        if hi.distance.abs() > hi.adaptive_epsilon && hi.distance > 0.0 {
            self.note_solver_refinement_failure();
            return Ok(None);
        }
        if hi.distance.abs() <= hi.adaptive_epsilon || (hi.t - lo.t).abs() <= hi.adaptive_epsilon {
            self.note_solver_newton_refinement();
            return Ok(Some(RayStepCertificate {
                kind: StepCertificateKind::RefinementBracket,
                metadata: self.certificate_metadata(
                    RequiredGuaranteeClass::IntervalBounded,
                    policy,
                    shape,
                    RayStepCertificateSubjectKind::Interval,
                    "safeguarded-newton-refinement",
                    format!("adaptive_epsilon={:.6}", hi.adaptive_epsilon),
                    CertificateReuseClass::RenderingAndCollision,
                    vec![
                        SmolStr::new("certified gradient changed"),
                        SmolStr::new("distance semantics changed"),
                    ],
                ),
                t_start: bracket.lo.t,
                t_end: hi.t,
                no_hit_before_t_end: true,
                bracket: Some([lo.t, hi.t]),
                provenance: None,
            }));
        }

        let mut current = hi;

        for _ in 0..8 {
            if current.distance.abs() <= current.adaptive_epsilon
                || (hi.t - lo.t).abs() <= current.adaptive_epsilon
            {
                self.note_solver_newton_refinement();
                return Ok(Some(RayStepCertificate {
                    kind: StepCertificateKind::RefinementBracket,
                    metadata: self.certificate_metadata(
                        RequiredGuaranteeClass::IntervalBounded,
                        policy,
                        shape,
                        RayStepCertificateSubjectKind::Interval,
                        "safeguarded-newton-refinement",
                        format!("adaptive_epsilon={:.6}", current.adaptive_epsilon),
                        CertificateReuseClass::RenderingAndCollision,
                        vec![
                            SmolStr::new("certified gradient changed"),
                            SmolStr::new("distance semantics changed"),
                        ],
                    ),
                    t_start: bracket.lo.t,
                    t_end: current.t,
                    no_hit_before_t_end: true,
                    bracket: Some([lo.t, hi.t]),
                    provenance: None,
                }));
            }

            if current.distance > 0.0 {
                lo = current;
            } else {
                hi = current;
            }

            let point = [
                origin[0] + direction[0] * current.t,
                origin[1] + direction[1] * current.t,
                origin[2] + direction[2] * current.t,
            ];
            let next = match self.certified_shape_directional_derivative(shape, point, direction)? {
                Some((derivative, _gradient_mag)) if derivative.abs() > 1e-5 => {
                    let candidate = current.t - current.distance / derivative;
                    if candidate > lo.t && candidate < hi.t {
                        candidate
                    } else {
                        0.5 * (lo.t + hi.t)
                    }
                }
                _ => {
                    self.note_solver_refinement_failure();
                    return Ok(None);
                }
            };
            current = self.interval_sample(shape, origin, direction, next, hit_epsilon)?;
        }

        if hi.distance.abs() <= hi.adaptive_epsilon {
            self.note_solver_newton_refinement();
            return Ok(Some(RayStepCertificate {
                kind: StepCertificateKind::RefinementBracket,
                metadata: self.certificate_metadata(
                    RequiredGuaranteeClass::IntervalBounded,
                    policy,
                    shape,
                    RayStepCertificateSubjectKind::Interval,
                    "safeguarded-newton-refinement",
                    format!("adaptive_epsilon={:.6}", hi.adaptive_epsilon),
                    CertificateReuseClass::RenderingAndCollision,
                    vec![
                        SmolStr::new("certified gradient changed"),
                        SmolStr::new("distance semantics changed"),
                    ],
                ),
                t_start: bracket.lo.t,
                t_end: hi.t,
                no_hit_before_t_end: true,
                bracket: Some([lo.t, hi.t]),
                provenance: None,
            }));
        }

        self.note_solver_refinement_failure();
        Ok(None)
    }

    pub(crate) fn interval_sample(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        travel: f32,
        hit_epsilon: f32,
    ) -> Result<IntervalSample, QueryExecError> {
        let point = [
            origin[0] + direction[0] * travel,
            origin[1] + direction[1] * travel,
            origin[2] + direction[2] * travel,
        ];
        let distance = self.eval_shape_distance(shape, point)?;
        let adaptive_epsilon =
            self.shape_adaptive_hit_epsilon(shape, travel, point, hit_epsilon)?;
        Ok(IntervalSample {
            t: travel,
            distance,
            adaptive_epsilon,
        })
    }

    pub(crate) fn shape_is_exact(&self, shape: &SmolStr) -> Result<bool, QueryExecError> {
        Ok(matches!(
            self.shape_scene(shape)?.semantics,
            DistanceSemantics::ExactSignedDistance
        ))
    }

    pub(crate) fn shape_scale_hint(&self, shape: &SmolStr) -> Result<f32, QueryExecError> {
        let Some((min, max)) = self.shape_support_bounds_world(shape)? else {
            return Ok(1.0);
        };
        Ok((0..3)
            .map(|axis| (max[axis] - min[axis]).abs())
            .fold(1.0f32, f32::max)
            .max(1.0))
    }

    pub(crate) fn shape_adaptive_hit_epsilon(
        &self,
        shape: &SmolStr,
        travel: f32,
        point: [f32; 3],
        base: f32,
    ) -> Result<f32, QueryExecError> {
        let gradient_mag = self
            .try_certified_shape_normal(shape, point)?
            .map(|evaluation| dot3(evaluation.normal, evaluation.normal).sqrt())
            .unwrap_or(1.0);
        let epsilon = adaptive_hit_epsilon_with_gradient(
            base,
            travel,
            self.shape_scale_hint(shape)?,
            gradient_mag,
        );
        self.note_solver_adaptive_epsilon();
        Ok(epsilon)
    }

    pub(crate) fn certified_shape_directional_derivative(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<Option<(f32, f32)>, QueryExecError> {
        let Some(evaluation) = self.try_certified_shape_normal(shape, point)? else {
            return Ok(None);
        };
        let gradient_mag = dot3(evaluation.normal, evaluation.normal).sqrt().max(1e-5);
        Ok(Some((dot3(evaluation.normal, direction), gradient_mag)))
    }

    pub(crate) fn shape_hit_value(
        &self,
        shape: &SmolStr,
        travel: f32,
        point: [f32; 3],
        steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        let normal = self.eval_shape_normal(shape, point)?;
        let winner = self.eval_shape_winner(shape, point)?;
        let feature_id = winner.feature_id;
        let (payload, local_position, local_normal, instance_id, repeat_id) = self
            .shape_leaf_from_winner(shape, feature_id, winner.leaf.as_ref())
            .map(|leaf| {
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let local_normal = self.eval_field_local_normal(&leaf.field, point)?;
                let payload = self
                    .eval_payload_body(&leaf.payload)
                    .unwrap_or_else(|_| default_payload());
                Ok::<_, QueryExecError>((
                    payload,
                    local_frame.point,
                    local_normal,
                    local_frame.instance_id,
                    local_frame.repeat_id,
                ))
            })
            .transpose()?
            .unwrap_or_else(|| (default_payload(), point, normal, 0, 0));
        Ok(hit_value(
            true,
            travel,
            point,
            normal,
            local_position,
            local_normal,
            steps,
            feature_id,
            instance_id,
            repeat_id,
            stable_shape_capture_id(shape),
            payload,
        ))
    }
}
