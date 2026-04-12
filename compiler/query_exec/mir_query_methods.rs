enum ParsedQueryCall<'a> {
    Legacy {
        name: &'a SmolStr,
    },
    Family {
        family: query_contract::QueryFamilyId,
        member: &'a SmolStr,
    },
}

impl FunctionLowerer {
    pub(crate) fn parse_scalar_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ScalarQueryInvocationSpec> {
        let (query, args) = self.parse_query_call(body, expr_id)?;
        let named = self.collect_named_query_args(args)?;
        let capture = *named.get("capture")?;
        let surface = if named.contains_key("domain") {
            QuerySurfaceKind::WorldScalar
        } else {
            QuerySurfaceKind::CaptureScalar
        };
        let capture_kind = if surface == QuerySurfaceKind::WorldScalar {
            CaptureKind::Region
        } else {
            self.capture_kind_for_expr(body, capture)
        };
        let (descriptor, _binding) = match query {
            ParsedQueryCall::Legacy { name } => {
                query_contract::query_contract_bundle_for_legacy_builtin(
                    name.as_str(),
                    surface,
                    capture_kind,
                )?
            }
            ParsedQueryCall::Family { family, member } => {
                query_contract::query_contract_bundle_for_family_member_internal(
                    family,
                    member.as_str(),
                    surface,
                    capture_kind,
                )?
            }
        };
        let item_arg = scalar_item_arg_name(descriptor);
        let item = item_arg.and_then(|name| named.get(name).copied());
        self.require_query_args(
            &named,
            &[Some("capture"), item_arg, scalar_domain_arg(descriptor)],
            &[
                Some("capture"),
                item_arg,
                scalar_domain_arg(descriptor),
                scalar_backend_arg(descriptor),
            ],
        )?;
        Some(ScalarQueryInvocationSpec {
            contract_id: descriptor.id,
            capture,
            domain: scalar_domain_arg(descriptor).and_then(|name| named.get(name).copied()),
            item,
            backend: scalar_backend_arg(descriptor).and_then(|name| named.get(name).copied()),
        })
    }

    pub(crate) fn parse_capture_builtin(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<SmolStr> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        if name.as_str() != "capture" {
            return None;
        }
        let mut positional_target = None;
        for arg in args {
            match arg {
                hir::Arg::Named { name, value, .. } => {
                    if name.as_str() != "scene" {
                        continue;
                    }
                    let Expr::Variable(target) = &body.exprs[*value] else {
                        return None;
                    };
                    if self.shape_names.contains(target)
                        || self.field_names.contains(target)
                        || matches!(
                            self.expr_type(body, expr_id),
                            MirType::Named(ref name) if name.as_str() == "RegionCapture"
                        )
                    {
                        return Some(target.clone());
                    }
                    return None;
                }
                hir::Arg::Positional { value, .. } => {
                    positional_target = Some(*value);
                }
            };
        }
        if let Some(value) = positional_target {
            let Expr::Variable(target) = &body.exprs[value] else {
                return None;
            };
            if self.shape_names.contains(target)
                || self.field_names.contains(target)
                || matches!(
                    self.expr_type(body, expr_id),
                    MirType::Named(ref name) if name.as_str() == "RegionCapture"
                )
            {
                return Some(target.clone());
            }
        }
        None
    }

    pub(crate) fn parse_dispatch_backend_builtin(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<i64> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        if !args.is_empty() {
            return None;
        }
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        match name.as_str() {
            "dispatch_backend_cpu" => Some(0),
            "dispatch_backend_virtual_gpu" => Some(1),
            "dispatch_backend_wgsl" => Some(2),
            "dispatch_backend_auto" => Some(3),
            _ => None,
        }
    }

    pub(crate) fn parse_batch_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<BatchQueryInvocationSpec> {
        let (query, args) = self.parse_query_call(body, expr_id)?;
        let named = self.collect_named_query_args(args)?;
        let capture = *named.get("capture")?;
        let target = if named.contains_key("domain") {
            query_contract::QueryTargetKind::World
        } else {
            query_contract::QueryTargetKind::Capture
        };
        let surface =
            QuerySurfaceKind::from_axes(target, query_contract::QueryCardinality::Batch);
        let capture_kind = if matches!(target, query_contract::QueryTargetKind::World) {
            CaptureKind::Region
        } else {
            self.capture_kind_for_expr(body, capture)
        };
        let (descriptor, _binding) = match query {
            ParsedQueryCall::Legacy { name } => {
                query_contract::query_contract_bundle_for_legacy_builtin_capture_candidates(
                    name.as_str(),
                    surface,
                    &[capture_kind, CaptureKind::Region, CaptureKind::Shape, CaptureKind::Field],
                )?
            }
            ParsedQueryCall::Family { family, member } => {
                query_contract::query_contract_bundle_for_family_member_internal(
                    family,
                    member.as_str(),
                    surface,
                    capture_kind,
                )?
            }
        };
        let items_arg = batch_items_arg_name(descriptor)?;
        let domain_arg = descriptor.domain_contract.map(|_| "domain");
        self.require_query_args(
            &named,
            &[Some("capture"), domain_arg, Some(items_arg), Some("backend")],
            &[Some("capture"), domain_arg, Some(items_arg), Some("backend")],
        )?;
        Some(BatchQueryInvocationSpec {
            contract_id: descriptor.id,
            capture,
            domain: domain_arg.and_then(|name| named.get(name).copied()),
            items: *named.get(items_arg)?,
            backend: *named.get("backend")?,
        })
    }

    fn parse_query_call<'a>(
        &self,
        body: &'a hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<(ParsedQueryCall<'a>, &'a [hir::Arg])> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        match &body.exprs[*callee] {
            Expr::Variable(name) => Some((ParsedQueryCall::Legacy { name }, args.as_slice())),
            Expr::Member { object, member, .. } => {
                let family = self.query_family_expr(body, *object)?;
                query_contract::query_family_member(family, member.as_str())?;
                Some((ParsedQueryCall::Family { family, member }, args.as_slice()))
            }
            _ => None,
        }
    }

    fn query_family_expr(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<query_contract::QueryFamilyId> {
        let Some(ty) = self
            .type_info
            .as_ref()
            .and_then(|info| info.expr_type(body, expr_id))
        else {
            return None;
        };
        match ty {
            crate::hir::typeck::Type::QueryFamily(family) => Some(*family),
            _ => None,
        }
    }

    fn collect_named_query_args(
        &self,
        args: &[hir::Arg],
    ) -> Option<HashMap<SmolStr, hir::Idx<Expr>>> {
        let mut named = HashMap::new();
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            if named.insert(name.clone(), *value).is_some() {
                return None;
            }
        }
        Some(named)
    }

    fn require_query_args(
        &self,
        named: &HashMap<SmolStr, hir::Idx<Expr>>,
        required: &[Option<&str>],
        allowed: &[Option<&str>],
    ) -> Option<()> {
        let allowed = allowed.iter().flatten().copied().collect::<HashSet<_>>();
        if named.keys().all(|name| allowed.contains(name.as_str()))
            && required
                .iter()
                .flatten()
                .all(|name| named.contains_key(*name))
        {
            Some(())
        } else {
            None
        }
    }

    pub(crate) fn lower_scalar_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        ret_ty: MirType,
        spec: &ScalarQueryInvocationSpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let descriptor =
            query_contract::query_contract(spec.contract_id).expect("scalar query descriptor");
        match descriptor.surface {
            QuerySurfaceKind::CaptureScalar => {
                let mut args = vec![capture];
                if let Some(item) = spec.item {
                    args.push(self.lower_expr(body, item));
                }
                let plan = self.build_scalar_capture_query_plan(body, spec);
                let kernel_plan = lower_capture_query_plan(&plan);
                debug_assert!(
                    validate_capture_query_plan(&kernel_plan).is_ok(),
                    "compiler-generated capture query plans must stay kernel-valid"
                );
                self.lower_call_temp(ret_ty, plan.helper_name, args, span)
            }
            QuerySurfaceKind::WorldScalar => {
                let domain =
                    self.lower_expr(body, spec.domain.expect("world query missing domain"));
                let backend = self.lower_world_query_backend_value(body, spec.backend, span);
                let mut args = vec![capture, domain];
                if let Some(item) = spec.item {
                    args.push(self.lower_expr(body, item));
                }
                args.push(backend);
                let plan = self.build_scalar_world_query_plan(body, spec);
                let kernel_plan = lower_world_query_plan(&plan);
                debug_assert!(
                    validate_world_query_plan(&kernel_plan).is_ok(),
                    "compiler-generated world query plans must stay kernel-valid"
                );
                self.lower_call_temp(ret_ty, plan.helper_name, args, span)
            }
            QuerySurfaceKind::CaptureBatch | QuerySurfaceKind::WorldBatch => {
                panic!("scalar query lowering received batch contract")
            }
        }
    }

    pub(crate) fn lower_batch_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &BatchQueryInvocationSpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let domain = spec.domain.map(|domain| self.lower_expr(body, domain));
        let items = self.lower_expr(body, spec.items);
        let backend_value = self.lower_expr(body, spec.backend);
        let backend = self.lower_dispatch_backend_id(backend_value, span);
        let plan = self.build_batch_query_plan(body, spec);
        let mut args = vec![capture];
        if let Some(domain) = domain {
            args.push(domain);
        }
        args.push(items);
        args.push(backend);
        self.lower_call_temp(
            MirType::Named(SmolStr::new("List")),
            plan.helper_name.clone(),
            args,
            span,
        )
    }

    pub(crate) fn build_scalar_capture_query_plan(
        &self,
        body: &hir::Body,
        spec: &ScalarQueryInvocationSpec,
    ) -> CaptureQueryPlan {
        let descriptor =
            query_contract::query_contract(spec.contract_id).expect("capture query descriptor");
        let scene = self.batch_capture_scene_summary(body, spec.capture, descriptor.capture_kind);
        CaptureQueryPlan::for_contract(spec.contract_id, scene)
            .expect("capture query plan must match descriptor")
    }

    pub(crate) fn build_scalar_world_query_plan(
        &self,
        body: &hir::Body,
        spec: &ScalarQueryInvocationSpec,
    ) -> WorldQueryPlan {
        let backend = self.world_query_plan_backend(body, spec.backend);
        WorldQueryPlan::for_contract_with_backend(spec.contract_id, backend)
            .expect("world query plan must match descriptor")
    }

    pub(crate) fn build_batch_query_plan(
        &self,
        body: &hir::Body,
        spec: &BatchQueryInvocationSpec,
    ) -> BatchQueryPlan {
        let descriptor =
            query_contract::query_contract(spec.contract_id).expect("batch query descriptor");
        let backend = self
            .parse_dispatch_backend_builtin(body, spec.backend)
            .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
            .unwrap_or(DispatchBackend::Auto);
        let scene = self.batch_capture_scene_summary(body, spec.capture, descriptor.capture_kind);
        BatchQueryPlan::for_contract(spec.contract_id, backend, scene)
            .expect("batch query plan must match descriptor")
    }

    pub(crate) fn build_field_scene_index(&self) -> BTreeMap<SmolStr, scene_ir::FieldScene> {
        self.field_scenes.clone()
    }

    pub(crate) fn build_shape_graph_index(&self) -> BTreeMap<SmolStr, hir::ShapeGraph> {
        self.shape_graphs
            .iter()
            .map(|(name, graph)| (name.clone(), graph.clone()))
            .collect()
    }

    pub(crate) fn field_scene(&self, field: &SmolStr) -> Option<&scene_ir::FieldScene> {
        self.field_scenes.get(field)
    }

    pub(crate) fn shape_scene(&self, shape: &SmolStr) -> Option<&scene_ir::ShapeScene> {
        self.shape_scenes.get(shape)
    }

    pub(crate) fn batch_capture_scene_summary(
        &self,
        body: &hir::Body,
        capture_expr: hir::Idx<Expr>,
        capture_kind: CaptureKind,
    ) -> Option<SceneSummary> {
        let target = self.parse_capture_builtin(body, capture_expr)?;
        match capture_kind {
            CaptureKind::Field => self.field_scene(&target).map(|scene| {
                self.scene_summary_from_field(target.clone(), scene)
            }),
            CaptureKind::Shape => self.shape_scene(&target).map(|scene| {
                self.scene_summary_from_shape(target.clone(), scene)
            }),
            CaptureKind::Region => None,
        }
    }

    pub(crate) fn shape_scene_summary(&self, shape: &SmolStr) -> Option<SceneSummary> {
        self.shape_scene(shape)
            .map(|scene| self.scene_summary_from_shape(shape.clone(), scene))
    }

    fn scene_summary_from_field(
        &self,
        name: SmolStr,
        scene: &scene_ir::FieldScene,
    ) -> SceneSummary {
        SceneSummary {
            name: Some(name),
            semantics: scene.semantics,
            support_class: scene.support_class,
            can_coarse_support_pruning: scene.can_coarse_support_pruning,
            opaque_boundary: scene.opaque_boundary,
            evidence_summary: crate::semantic_evidence::SemanticEvidence::for_field_scene(scene)
                .summary(),
            semantic_root: scene.root_node_id.0,
            support_root: scene.root_support_id.0,
            node_count: scene.node_records.len() as u32,
            support_node_count: scene.support_records.len() as u32,
            leaf_count: 0,
            identity_source_count: scene.identity_sources.len() as u32,
        }
    }

    fn scene_summary_from_shape(
        &self,
        name: SmolStr,
        scene: &scene_ir::ShapeScene,
    ) -> SceneSummary {
        let identity_source_count = scene
            .feature_leaves
            .values()
            .filter_map(|leaf_ref| self.shape_scene(&leaf_ref.scene).and_then(|shape_scene| {
                shape_scene
                    .leaves
                    .get(&leaf_ref.leaf)
                    .and_then(|leaf| self.field_scene(&leaf.field))
            }))
            .map(|field| field.identity_sources.len() as u32)
            .sum();
        SceneSummary {
            name: Some(name),
            semantics: scene.semantics,
            support_class: scene.support_class,
            can_coarse_support_pruning: scene.can_coarse_support_pruning,
            opaque_boundary: scene.opaque_boundary,
            evidence_summary: crate::semantic_evidence::SemanticEvidence::for_shape_scene(scene)
                .summary(),
            semantic_root: scene.root_node_id.0,
            support_root: scene.root_support_id.0,
            node_count: scene.node_records.len() as u32,
            support_node_count: scene.support_records.len() as u32,
            leaf_count: scene.feature_leaves.len() as u32,
            identity_source_count,
        }
    }

    pub(crate) fn shape_execution_mode_from_plan_artifacts(
        pruning_strategy: crate::query_plan::PruningStrategy,
        artifact_contracts: &[crate::query_plan::ArtifactContract],
    ) -> ShapeExecutionMode {
        if artifact_contracts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                crate::query_plan::ArtifactSchema::OpaquePessimizationBoundary { .. }
            )
        }) {
            return ShapeExecutionMode::Conservative;
        }
        if artifact_contracts.iter().any(|artifact| {
            matches!(
                artifact.schema,
                crate::query_plan::ArtifactSchema::CullingTable { .. }
            )
        }) {
            return ShapeExecutionMode::SupportPruned;
        }
        match pruning_strategy {
            crate::query_plan::PruningStrategy::SupportLowerBound
            | crate::query_plan::PruningStrategy::CullingTable => {
                ShapeExecutionMode::SupportPruned
            }
            crate::query_plan::PruningStrategy::None
            | crate::query_plan::PruningStrategy::ConservativeTraversal
            | crate::query_plan::PruningStrategy::OpaquePessimizationBoundary => {
                ShapeExecutionMode::Conservative
            }
        }
    }

    pub(crate) fn shape_batch_execution_mode(
        &self,
        kind: BatchQueryKind,
        shape: &SmolStr,
    ) -> ShapeExecutionMode {
        let plan = BatchQueryPlan::for_shape_query(
            kind,
            DispatchBackend::Auto,
            self.shape_scene_summary(shape),
        );
        Self::shape_execution_mode_from_plan_artifacts(plan.pruning_strategy(), &plan.artifact_contracts)
    }

    pub(crate) fn shape_point_batch_execution_mode(
        &self,
        kind: BatchQueryKind,
        shape: &SmolStr,
    ) -> ShapeExecutionMode {
        let plan = BatchQueryPlan::for_field_query(
            kind,
            CaptureKind::Shape,
            DispatchBackend::Auto,
            self.shape_scene_summary(shape),
        );
        Self::shape_execution_mode_from_plan_artifacts(plan.pruning_strategy(), &plan.artifact_contracts)
    }

    pub(crate) fn shape_capture_execution_mode(
        &self,
        kind: CaptureQueryKind,
        shape: &SmolStr,
    ) -> ShapeExecutionMode {
        let plan =
            CaptureQueryPlan::for_query(kind, CaptureKind::Shape, self.shape_scene_summary(shape))
                .expect("shape capture execution mode requires a valid shape capture plan");
        Self::shape_execution_mode_from_plan_artifacts(
            plan.pruning_strategy(),
            &plan.artifact_contracts,
        )
    }

    pub(crate) fn capture_kind_for_expr(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> CaptureKind {
        match self.expr_type(body, expr_id) {
            MirType::Named(name) if name.as_str() == "ShapeCapture" => CaptureKind::Shape,
            MirType::Named(name) if name.as_str() == "RegionCapture" => CaptureKind::Region,
            _ => CaptureKind::Field,
        }
    }

    pub(crate) fn lower_batch_query_loop(
        &mut self,
        plan: &BatchQueryPlan,
        items: Value,
        capture: Value,
        result_local: LocalId,
        span: TextRange,
        use_virtual_gpu: bool,
        merge_block: BlockId,
    ) {
        let kernel_plan = lower_batch_query_plan(plan);
        debug_assert!(
            validate_batch_query_plan(&kernel_plan).is_ok(),
            "compiler-generated batch query plans must stay kernel-valid"
        );
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        if use_virtual_gpu && kernel_plan.requires_virtual_gpu_dispatch() {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_begin"),
                vec![
                    len.clone(),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Integer(1)),
                    Value::Const(Literal::Nil),
                ],
                span,
            );
        }
        let index = self.new_local(
            SmolStr::new(format!("$batch_query_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        if use_virtual_gpu && kernel_plan.requires_virtual_gpu_dispatch() {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_select_invocation"),
                vec![Value::Local(index)],
                span,
            );
        }

        let mut inputs = BatchQueryLoopInputs::default();
        let mut execution_state = BatchQueryExecutionState::default();
        let mut execution_value = None;
        let mut appended_result = false;
        for stage in &kernel_plan.stages {
            match stage {
                KernelPlanStage::SelectBackend
                | KernelPlanStage::LoadCapture
                | KernelPlanStage::BeginVirtualGpuDispatch
                | KernelPlanStage::EndVirtualGpuDispatch => {}
                KernelPlanStage::LoadDerivedArtifact { artifact } => match artifact {
                    crate::query_plan::DerivedArtifact::OpaquePessimizationBoundary => {
                        debug_assert!(plan.has_opaque_pessimization_boundary());
                    }
                    crate::query_plan::DerivedArtifact::CullingTable {
                        candidate_strategy,
                        pruning_strategy,
                    } => {
                        debug_assert!(plan.artifact_contracts.iter().any(|contract| matches!(
                            contract.schema,
                            crate::query_plan::ArtifactSchema::CullingTable {
                                candidate_strategy: contract_candidate_strategy,
                                pruning_strategy: contract_pruning_strategy,
                                ..
                            } if contract_candidate_strategy == *candidate_strategy
                                && contract_pruning_strategy == *pruning_strategy
                        )));
                    }
                    crate::query_plan::DerivedArtifact::SupportSummary {
                        semantics,
                        support_class,
                        can_coarse_support_pruning,
                    } => {
                        debug_assert!(plan.artifact_contracts.iter().any(|contract| matches!(
                            contract.schema,
                            crate::query_plan::ArtifactSchema::SupportSummary {
                                semantics: contract_semantics,
                                support_class: contract_support_class,
                                can_coarse_support_pruning: contract_pruning,
                                ..
                            } if contract_semantics == *semantics
                                && contract_support_class == *support_class
                                && contract_pruning == *can_coarse_support_pruning
                        )));
                        if *can_coarse_support_pruning {
                            debug_assert!(matches!(
                                plan.candidate_strategy(),
                                CandidateStrategy::SupportAcceleratedShapeTraversal
                                    | CandidateStrategy::ShapeBranchTraversal
                            ));
                        }
                    }
                    crate::query_plan::DerivedArtifact::CaptureCache { capture_kind } => {
                        debug_assert!(plan.artifact_contracts.iter().any(|contract| matches!(
                            contract.schema,
                            crate::query_plan::ArtifactSchema::CaptureCache {
                                capture_kind: contract_capture_kind,
                                ..
                            } if contract_capture_kind == *capture_kind
                        )));
                    }
                },
                KernelPlanStage::IterateItems { item_kind } => {
                    inputs = self.lower_batch_query_item_inputs(
                        *item_kind,
                        items.clone(),
                        Value::Local(index),
                        span,
                    );
                }
                KernelPlanStage::GenerateCandidates { strategy } => {
                    debug_assert_eq!(*strategy, plan.candidate_strategy());
                    execution_state.candidate_strategy = Some(*strategy);
                }
                KernelPlanStage::PruneCandidates { strategy } => {
                    debug_assert_eq!(*strategy, plan.pruning_strategy());
                    execution_state.pruning_strategy = Some(*strategy);
                }
                KernelPlanStage::LoadDomainFlags | KernelPlanStage::SelectParticipants { .. } => {
                    panic!("batch helpers do not support world/domain participant stages");
                }
                KernelPlanStage::Execute { executor } => {
                    execution_value = Some(self.lower_batch_query_executor_value(
                        plan,
                        *executor,
                        capture.clone(),
                        &inputs,
                        &execution_state,
                        span,
                    ));
                }
                KernelPlanStage::AssembleHitContext => {
                    debug_assert!(plan.preserves_local_hit_context);
                    debug_assert!(matches!(
                        plan.result_kind,
                        QueryResultKind::Hit3 | QueryResultKind::OcclusionResult
                    ));
                }
                KernelPlanStage::AppendResult { result_kind } => {
                    let result_value = self.lower_batch_query_result_value(
                        *result_kind,
                        execution_value
                            .clone()
                            .expect("batch plan must execute before appending a result"),
                        span,
                    );
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_list_push"),
                        vec![Value::Local(result_local), result_value],
                        span,
                    );
                    appended_result = true;
                }
            }
        }
        debug_assert!(appended_result);
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        if use_virtual_gpu && kernel_plan.requires_virtual_gpu_dispatch() {
            let _ = self.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_gpu_dispatch_end"),
                Vec::new(),
                span,
            );
        }
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    pub(crate) fn lower_world_batch_query_loop(
        &mut self,
        plan: &BatchQueryPlan,
        items: Value,
        capture: Value,
        domain: Value,
        backend: Value,
        result_local: LocalId,
        span: TextRange,
        merge_block: BlockId,
    ) {
        let len = self.lower_call_temp(
            MirType::Integer,
            SmolStr::new("__wr_list_len"),
            vec![items.clone()],
            span,
        );
        let index = self.new_local(
            SmolStr::new(format!("$world_batch_query_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = head;
        let cond = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            Value::Local(index),
            len,
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond,
            then_target: body_block,
            else_target: exit,
            span,
        });
        self.current_block = body_block;
        let inputs =
            self.lower_batch_query_item_inputs(plan.item_kind, items, Value::Local(index), span);
        let execution_value = self.lower_world_batch_scalar_execution_value(
            plan,
            capture,
            domain,
            backend,
            &inputs,
            span,
        );
        let result_value =
            self.lower_batch_query_result_value(plan.result_kind, execution_value, span);
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_list_push"),
            vec![Value::Local(result_local), result_value],
            span,
        );
        let next = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Add,
            Value::Local(index),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.assign_use(Place::Local(index), next, span);
        self.set_terminator(Terminator::Jump { target: head, span });
        self.current_block = exit;
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    pub(crate) fn lower_batch_query_item_inputs(
        &mut self,
        item_kind: QueryItemKind,
        items: Value,
        index: Value,
        span: TextRange,
    ) -> BatchQueryLoopInputs {
        match item_kind {
            QueryItemKind::Unit => BatchQueryLoopInputs::default(),
            QueryItemKind::PointQuery => {
                let point_query = self.lower_call_temp(
                    MirType::Named(SmolStr::new("PointQuery")),
                    SmolStr::new("__wr_list_get"),
                    vec![items, index],
                    span,
                );
                let point = self.new_temp(MirType::Vec3);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(point),
                    value: Rvalue::GetField {
                        base: point_query,
                        field: SmolStr::new("point"),
                        slot: self.field_slot("PointQuery", "point"),
                    },
                    span,
                });
                BatchQueryLoopInputs {
                    point: Some(Value::Temp(point)),
                    ..BatchQueryLoopInputs::default()
                }
            }
            QueryItemKind::PointDirectionQuery => {
                let sample = self.lower_call_temp(
                    MirType::Named(SmolStr::new("PointDirectionQuery")),
                    SmolStr::new("__wr_list_get"),
                    vec![items, index],
                    span,
                );
                let point = self.new_temp(MirType::Vec3);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(point),
                    value: Rvalue::GetField {
                        base: sample.clone(),
                        field: SmolStr::new("point"),
                        slot: self.field_slot("PointDirectionQuery", "point"),
                    },
                    span,
                });
                let direction = self.new_temp(MirType::Vec3);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(direction),
                    value: Rvalue::GetField {
                        base: sample,
                        field: SmolStr::new("direction"),
                        slot: self.field_slot("PointDirectionQuery", "direction"),
                    },
                    span,
                });
                BatchQueryLoopInputs {
                    point: Some(Value::Temp(point)),
                    direction: Some(Value::Temp(direction)),
                    ..BatchQueryLoopInputs::default()
                }
            }
            QueryItemKind::RayQuery => {
                let ray = self.lower_call_temp(
                    MirType::Named(SmolStr::new("RayQuery")),
                    SmolStr::new("__wr_list_get"),
                    vec![items, index],
                    span,
                );
                let origin = self.new_temp(MirType::Vec3);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(origin),
                    value: Rvalue::GetField {
                        base: ray.clone(),
                        field: SmolStr::new("origin"),
                        slot: self.field_slot("RayQuery", "origin"),
                    },
                    span,
                });
                let direction = self.new_temp(MirType::Vec3);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(direction),
                    value: Rvalue::GetField {
                        base: ray.clone(),
                        field: SmolStr::new("direction"),
                        slot: self.field_slot("RayQuery", "direction"),
                    },
                    span,
                });
                let max_distance = self.new_temp(MirType::Float);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(max_distance),
                    value: Rvalue::GetField {
                        base: ray.clone(),
                        field: SmolStr::new("max_distance"),
                        slot: self.field_slot("RayQuery", "max_distance"),
                    },
                    span,
                });
                let min_step = self.new_temp(MirType::Float);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(min_step),
                    value: Rvalue::GetField {
                        base: ray.clone(),
                        field: SmolStr::new("min_step"),
                        slot: self.field_slot("RayQuery", "min_step"),
                    },
                    span,
                });
                let hit_epsilon = self.new_temp(MirType::Float);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(hit_epsilon),
                    value: Rvalue::GetField {
                        base: ray.clone(),
                        field: SmolStr::new("hit_epsilon"),
                        slot: self.field_slot("RayQuery", "hit_epsilon"),
                    },
                    span,
                });
                let max_steps = self.new_temp(MirType::Integer);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(max_steps),
                    value: Rvalue::GetField {
                        base: ray,
                        field: SmolStr::new("max_steps"),
                        slot: self.field_slot("RayQuery", "max_steps"),
                    },
                    span,
                });
                BatchQueryLoopInputs {
                    origin: Some(Value::Temp(origin)),
                    direction: Some(Value::Temp(direction)),
                    max_distance: Some(Value::Temp(max_distance)),
                    min_step: Some(Value::Temp(min_step)),
                    hit_epsilon: Some(Value::Temp(hit_epsilon)),
                    max_steps: Some(Value::Temp(max_steps)),
                    ..BatchQueryLoopInputs::default()
                }
            }
            QueryItemKind::Hit3 => {
                let hit = self.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    SmolStr::new("__wr_list_get"),
                    vec![items, index],
                    span,
                );
                BatchQueryLoopInputs {
                    hit: Some(hit),
                    ..BatchQueryLoopInputs::default()
                }
            }
        }
    }

    pub(crate) fn lower_batch_query_executor_value(
        &mut self,
        plan: &BatchQueryPlan,
        executor: PlanExecutor,
        capture: Value,
        inputs: &BatchQueryLoopInputs,
        execution_state: &BatchQueryExecutionState,
        span: TextRange,
    ) -> Value {
        debug_assert_eq!(
            execution_state.candidate_strategy,
            Some(plan.candidate_strategy())
        );
        debug_assert_eq!(
            execution_state.pruning_strategy,
            Some(plan.pruning_strategy())
        );
        match executor {
            PlanExecutor::FieldDistanceCapture
            | PlanExecutor::ShapeDistanceCapture
            | PlanExecutor::FieldNormalCapture
            | PlanExecutor::ShapeNormalCapture => self.lower_batch_point_query_executor_value(
                plan,
                executor,
                capture,
                inputs
                    .point
                    .clone()
                    .expect("point-query batch plan must load a point before executing"),
                span,
            ),
            PlanExecutor::SceneTraceCapture => self.lower_batch_shape_trace_executor_value(
                batch_query_kind_for_contract_id(plan.contract_id)
                    .expect("batch query plan contract id must resolve"),
                capture,
                inputs
                    .origin
                    .clone()
                    .expect("trace batch plan must load an origin before executing"),
                inputs
                    .direction
                    .clone()
                    .expect("trace batch plan must load a direction before executing"),
                inputs
                    .max_distance
                    .clone()
                    .expect("trace batch plan must load max_distance before executing"),
                inputs
                    .min_step
                    .clone()
                    .expect("trace batch plan must load min_step before executing"),
                inputs
                    .hit_epsilon
                    .clone()
                    .expect("trace batch plan must load hit_epsilon before executing"),
                inputs
                    .max_steps
                    .clone()
                    .expect("trace batch plan must load max_steps before executing"),
                span,
            ),
            PlanExecutor::SceneSurfaceCapture => self.lower_batch_shape_surface_executor_value(
                capture,
                inputs
                    .hit
                    .clone()
                    .expect("surface batch plan must load a hit before executing"),
                span,
            ),
            PlanExecutor::WorldDistanceCapture
            | PlanExecutor::WorldNormalCapture
            | PlanExecutor::WorldTraceCapture
            | PlanExecutor::WorldSurfaceCapture
            | PlanExecutor::WorldRadianceCapture
            | PlanExecutor::WorldMediumCapture => {
                panic!("world batch helpers route through scalar world helper calls")
            }
            other => panic!("batch helpers do not support executor {other:?}"),
        }
    }

    fn lower_world_batch_scalar_execution_value(
        &mut self,
        plan: &BatchQueryPlan,
        capture: Value,
        domain: Value,
        backend: Value,
        inputs: &BatchQueryLoopInputs,
        span: TextRange,
    ) -> Value {
        let kind = batch_query_kind_for_contract_id(plan.contract_id)
            .expect("world batch query plan contract id must resolve");
        let (helper, ret_ty, item) = match kind {
            BatchQueryKind::Distance => (
                "__wr_world_distance_capture",
                MirType::Float,
                inputs
                    .point
                    .clone()
                    .expect("world distance batch plan must load a point before executing"),
            ),
            BatchQueryKind::Normal => (
                "__wr_world_normal_capture",
                MirType::Vec3,
                inputs
                    .point
                    .clone()
                    .expect("world normal batch plan must load a point before executing"),
            ),
            BatchQueryKind::Nearest | BatchQueryKind::Trace => (
                "__wr_world_trace_capture",
                MirType::Named(SmolStr::new("Hit3")),
                self.build_ray_query_value(
                    inputs
                        .origin
                        .clone()
                        .expect("world trace batch plan must load an origin"),
                    inputs
                        .direction
                        .clone()
                        .expect("world trace batch plan must load a direction"),
                    inputs
                        .max_distance
                        .clone()
                        .expect("world trace batch plan must load max_distance"),
                    inputs
                        .min_step
                        .clone()
                        .expect("world trace batch plan must load min_step"),
                    inputs
                        .hit_epsilon
                        .clone()
                        .expect("world trace batch plan must load hit_epsilon"),
                    inputs
                        .max_steps
                        .clone()
                        .expect("world trace batch plan must load max_steps"),
                    span,
                ),
            ),
            BatchQueryKind::Occluded => (
                "__wr_world_occluded_capture",
                MirType::Named(SmolStr::new("OcclusionResult")),
                self.build_ray_query_value(
                    inputs
                        .origin
                        .clone()
                        .expect("world occluded batch plan must load an origin"),
                    inputs
                        .direction
                        .clone()
                        .expect("world occluded batch plan must load a direction"),
                    inputs
                        .max_distance
                        .clone()
                        .expect("world occluded batch plan must load max_distance"),
                    inputs
                        .min_step
                        .clone()
                        .expect("world occluded batch plan must load min_step"),
                    inputs
                        .hit_epsilon
                        .clone()
                        .expect("world occluded batch plan must load hit_epsilon"),
                    inputs
                        .max_steps
                        .clone()
                        .expect("world occluded batch plan must load max_steps"),
                    span,
                ),
            ),
            BatchQueryKind::Surface => (
                "__wr_world_surface_capture",
                MirType::Named(SmolStr::new("Surface")),
                inputs
                    .hit
                    .clone()
                    .expect("world surface batch plan must load a hit before executing"),
            ),
            BatchQueryKind::Radiance => (
                "__wr_world_radiance_capture",
                MirType::Vec3,
                self.build_point_direction_query_value(
                    inputs
                        .point
                        .clone()
                        .expect("world radiance batch plan must load a point"),
                    inputs
                        .direction
                        .clone()
                        .expect("world radiance batch plan must load a direction"),
                    span,
                ),
            ),
            BatchQueryKind::Medium => (
                "__wr_world_medium_capture",
                MirType::Named(SmolStr::new("Medium")),
                inputs
                    .point
                    .clone()
                    .expect("world medium batch plan must load a point before executing"),
            ),
        };
        self.lower_call_temp(ret_ty, SmolStr::new(helper), vec![capture, domain, item, backend], span)
    }

    pub(crate) fn lower_capture_scene_id_value(
        &mut self,
        capture_type_name: &str,
        capture: Value,
        span: TextRange,
    ) -> Value {
        let scene_id = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(scene_id),
            value: Rvalue::GetField {
                base: capture,
                field: SmolStr::new("scene_id"),
                slot: self.field_slot(capture_type_name, "scene_id"),
            },
            span,
        });
        Value::Temp(scene_id)
    }

    pub(crate) fn lower_capture_root_feature_id_value(&mut self, capture: Value, span: TextRange) -> Value {
        let root_feature_id = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(root_feature_id),
            value: Rvalue::GetField {
                base: capture,
                field: SmolStr::new("root_feature_id"),
                slot: self.field_slot("ShapeCapture", "root_feature_id"),
            },
            span,
        });
        Value::Temp(root_feature_id)
    }

    pub(crate) fn lower_batch_point_query_executor_value(
        &mut self,
        plan: &BatchQueryPlan,
        executor: PlanExecutor,
        capture: Value,
        point: Value,
        span: TextRange,
    ) -> Value {
        let (capture_type_name, default_value, invalid_message) = match executor {
            PlanExecutor::FieldDistanceCapture => (
                "FieldCapture",
                Value::Const(Literal::Float(0.0)),
                "distance_at requires a capture created by `capture`",
            ),
            PlanExecutor::ShapeDistanceCapture => (
                "ShapeCapture",
                Value::Const(Literal::Float(0.0)),
                "distance_at requires a capture created by `capture`",
            ),
            PlanExecutor::FieldNormalCapture => (
                "FieldCapture",
                self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(1.0)),
                    ],
                    span,
                ),
                "normal_at requires a capture created by `capture`",
            ),
            PlanExecutor::ShapeNormalCapture => (
                "ShapeCapture",
                self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(1.0)),
                    ],
                    span,
                ),
                "normal_at requires a capture created by `capture`",
            ),
            other => panic!("unsupported point-query batch executor {other:?}"),
        };
        let result_type = match executor {
            PlanExecutor::FieldDistanceCapture | PlanExecutor::ShapeDistanceCapture => {
                MirType::Float
            }
            PlanExecutor::FieldNormalCapture | PlanExecutor::ShapeNormalCapture => MirType::Vec3,
            _ => unreachable!(),
        };
        let result_local = self.new_local(
            SmolStr::new(format!("$batch_query_exec{}", self.locals.len())),
            true,
            result_type,
        );
        self.assign_use(Place::Local(result_local), default_value, span);
        let capture_id = self.lower_capture_scene_id_value(capture_type_name, capture, span);
        let merge_block = self.new_block();
        let invalid_block = self.new_block();
        let mut dispatch_block = self.new_block();
        self.set_terminator(Terminator::Jump {
            target: dispatch_block,
            span,
        });

        let mut field_names = self.field_graphs.keys().cloned().collect::<Vec<_>>();
        field_names.sort();
        let mut shape_names = self.shape_graphs.keys().cloned().collect::<Vec<_>>();
        shape_names.sort();
        let names = match executor {
            PlanExecutor::FieldDistanceCapture | PlanExecutor::FieldNormalCapture => field_names,
            PlanExecutor::ShapeDistanceCapture | PlanExecutor::ShapeNormalCapture => shape_names,
            _ => unreachable!(),
        };

        for name in names {
            let match_block = self.new_block();
            let next_block = self.new_block();
            self.current_block = dispatch_block;
            let capture_id_value = match executor {
                PlanExecutor::FieldDistanceCapture | PlanExecutor::FieldNormalCapture => {
                    stable_field_scene_capture_id(&name)
                }
                PlanExecutor::ShapeDistanceCapture | PlanExecutor::ShapeNormalCapture => {
                    stable_shape_scene_capture_id(&name)
                }
                _ => unreachable!(),
            };
            let matched = self.lower_binary_temp(
                MirType::Boolean,
                BinaryOp::Eq,
                capture_id.clone(),
                Value::Const(Literal::Integer(capture_id_value)),
                span,
            );
            self.set_terminator(Terminator::Branch {
                cond: matched,
                then_target: match_block,
                else_target: next_block,
                span,
            });
            self.current_block = match_block;
            let value = match executor {
                PlanExecutor::FieldDistanceCapture => {
                    self.lower_field_distance_call(&name, point.clone(), span)
                }
                PlanExecutor::ShapeDistanceCapture => {
                    let mode = self.shape_point_batch_execution_mode(
                        batch_query_kind_for_contract_id(plan.contract_id)
                            .expect("batch query plan contract id must resolve"),
                        &name,
                    );
                    self.lower_shape_distance_call_with_mode(&name, point.clone(), span, mode)
                }
                PlanExecutor::FieldNormalCapture => {
                    self.lower_field_normal_call(&name, point.clone(), span)
                }
                PlanExecutor::ShapeNormalCapture => {
                    let mode = self.shape_point_batch_execution_mode(
                        batch_query_kind_for_contract_id(plan.contract_id)
                            .expect("batch query plan contract id must resolve"),
                        &name,
                    );
                    self.lower_shape_normal_call_with_mode(&name, point.clone(), span, mode)
                }
                _ => unreachable!(),
            };
            self.assign_use(Place::Local(result_local), value, span);
            self.set_terminator(Terminator::Jump {
                target: merge_block,
                span,
            });
            dispatch_block = next_block;
        }

        self.current_block = dispatch_block;
        self.set_terminator(Terminator::Jump {
            target: invalid_block,
            span,
        });
        self.current_block = invalid_block;
        let crash_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(invalid_message))),
            },
            span,
        });
        self.set_terminator(Terminator::Return {
            value: Some(Value::Temp(crash_temp)),
            span,
        });
        self.current_block = merge_block;
        Value::Local(result_local)
    }

    pub(crate) fn lower_batch_shape_trace_executor_value(
        &mut self,
        kind: BatchQueryKind,
        capture: Value,
        origin: Value,
        direction: Value,
        max_distance: Value,
        min_step: Value,
        hit_epsilon: Value,
        max_steps: Value,
        span: TextRange,
    ) -> Value {
        let result_local = self.new_local(
            SmolStr::new(format!("$batch_query_trace{}", self.locals.len())),
            true,
            MirType::Named(SmolStr::new("Hit3")),
        );
        let default_hit = self.build_default_hit(origin.clone(), span);
        self.assign_use(Place::Local(result_local), default_hit, span);
        let root_feature_id = self.lower_capture_root_feature_id_value(capture, span);
        let merge_block = self.new_block();
        let invalid_block = self.new_block();
        let mut dispatch_block = self.new_block();
        self.set_terminator(Terminator::Jump {
            target: dispatch_block,
            span,
        });
        let mut shape_names = self.shape_graphs.keys().cloned().collect::<Vec<_>>();
        shape_names.sort();
        for name in shape_names {
            let match_block = self.new_block();
            let next_block = self.new_block();
            self.current_block = dispatch_block;
            let matched = self.lower_binary_temp(
                MirType::Boolean,
                BinaryOp::Eq,
                root_feature_id.clone(),
                Value::Const(Literal::Integer(stable_shape_capture_id(&name))),
                span,
            );
            self.set_terminator(Terminator::Branch {
                cond: matched,
                then_target: match_block,
                else_target: next_block,
                span,
            });
            self.current_block = match_block;
            let mode = self.shape_batch_execution_mode(kind, &name);
            let hit = self.lower_shape_trace_call_with_mode(
                &name,
                origin.clone(),
                direction.clone(),
                max_distance.clone(),
                min_step.clone(),
                hit_epsilon.clone(),
                max_steps.clone(),
                span,
                mode,
            );
            self.assign_use(Place::Local(result_local), hit, span);
            self.set_terminator(Terminator::Jump {
                target: merge_block,
                span,
            });
            dispatch_block = next_block;
        }
        self.current_block = dispatch_block;
        self.set_terminator(Terminator::Jump {
            target: invalid_block,
            span,
        });
        self.current_block = invalid_block;
        let crash_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "trace_shape requires a capture created by `capture`",
                ))),
            },
            span,
        });
        self.set_terminator(Terminator::Return {
            value: Some(Value::Temp(crash_temp)),
            span,
        });
        self.current_block = merge_block;
        Value::Local(result_local)
    }

    pub(crate) fn lower_batch_shape_surface_executor_value(
        &mut self,
        capture: Value,
        hit: Value,
        span: TextRange,
    ) -> Value {
        let result_local = self.new_local(
            SmolStr::new(format!("$batch_query_surface{}", self.locals.len())),
            true,
            MirType::Named(SmolStr::new("Surface")),
        );
        let default_surface = self.build_default_surface(span);
        self.assign_use(Place::Local(result_local), default_surface, span);
        let root_feature_id = self.lower_capture_root_feature_id_value(capture, span);
        let merge_block = self.new_block();
        let invalid_block = self.new_block();
        let mut dispatch_block = self.new_block();
        self.set_terminator(Terminator::Jump {
            target: dispatch_block,
            span,
        });
        let mut shape_names = self.shape_graphs.keys().cloned().collect::<Vec<_>>();
        shape_names.sort();
        for name in shape_names {
            let match_block = self.new_block();
            let next_block = self.new_block();
            self.current_block = dispatch_block;
            let matched = self.lower_binary_temp(
                MirType::Boolean,
                BinaryOp::Eq,
                root_feature_id.clone(),
                Value::Const(Literal::Integer(stable_shape_capture_id(&name))),
                span,
            );
            self.set_terminator(Terminator::Branch {
                cond: matched,
                then_target: match_block,
                else_target: next_block,
                span,
            });
            self.current_block = match_block;
            let surface = self.lower_shape_surface_call(&name, hit.clone(), span);
            self.assign_use(Place::Local(result_local), surface, span);
            self.set_terminator(Terminator::Jump {
                target: merge_block,
                span,
            });
            dispatch_block = next_block;
        }
        self.current_block = dispatch_block;
        self.set_terminator(Terminator::Jump {
            target: invalid_block,
            span,
        });
        self.current_block = invalid_block;
        let crash_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "surface_at requires a capture created by `capture`",
                ))),
            },
            span,
        });
        self.set_terminator(Terminator::Return {
            value: Some(Value::Temp(crash_temp)),
            span,
        });
        self.current_block = merge_block;
        Value::Local(result_local)
    }

    pub(crate) fn lower_batch_query_result_value(
        &mut self,
        result_kind: QueryResultKind,
        value: Value,
        span: TextRange,
    ) -> Value {
        match result_kind {
            QueryResultKind::DistanceResult => self.build_distance_result_value(value, span),
            QueryResultKind::NormalResult => self.build_normal_result_value(value, span),
            QueryResultKind::Hit3
            | QueryResultKind::Surface
            | QueryResultKind::RadianceResult
            | QueryResultKind::MediumResult => value,
            QueryResultKind::OcclusionResult => self.build_occlusion_result_value(value, span),
            other => panic!("batch helpers do not support result kind {other:?}"),
        }
    }

    pub(crate) fn build_point_direction_query_value(
        &mut self,
        point: Value,
        direction: Value,
        span: TextRange,
    ) -> Value {
        let mut class = self.synthetic_class_target_info("PointDirectionQuery");
        Self::set_class_field_value(&mut class, "point", point);
        Self::set_class_field_value(&mut class, "direction", direction);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_distance_result_value(&mut self, distance: Value, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("DistanceResult");
        Self::set_class_field_value(&mut class, "distance", distance);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_normal_result_value(&mut self, normal: Value, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("NormalResult");
        Self::set_class_field_value(&mut class, "normal", normal);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_occlusion_result_value(&mut self, hit: Value, span: TextRange) -> Value {
        let hit_flag = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(hit_flag),
            value: Rvalue::GetField {
                base: hit.clone(),
                field: SmolStr::new("hit"),
                slot: self.field_slot("Hit3", "hit"),
            },
            span,
        });
        let distance = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(distance),
            value: Rvalue::GetField {
                base: hit.clone(),
                field: SmolStr::new("distance"),
                slot: self.field_slot("Hit3", "distance"),
            },
            span,
        });
        let steps = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(steps),
            value: Rvalue::GetField {
                base: hit,
                field: SmolStr::new("steps"),
                slot: self.field_slot("Hit3", "steps"),
            },
            span,
        });
        let mut class = self.synthetic_class_target_info("OcclusionResult");
        Self::set_class_field_value(&mut class, "occluded", Value::Temp(hit_flag));
        Self::set_class_field_value(&mut class, "distance", Value::Temp(distance));
        Self::set_class_field_value(&mut class, "steps", Value::Temp(steps));
        self.build_class_instance(&class, span)
    }
}

fn scalar_item_arg_name(descriptor: &QueryContractDescriptor) -> Option<&'static str> {
    match descriptor.item_kind {
        QueryItemKind::PointQuery => Some("point"),
        QueryItemKind::PointDirectionQuery => Some("sample"),
        QueryItemKind::RayQuery => Some("ray"),
        QueryItemKind::Hit3 => Some("hit"),
        QueryItemKind::Unit => None,
    }
}

fn batch_items_arg_name(descriptor: &QueryContractDescriptor) -> Option<&'static str> {
    match descriptor.item_kind {
        QueryItemKind::PointQuery => Some("points"),
        QueryItemKind::RayQuery => Some("rays"),
        QueryItemKind::Hit3 => Some("hits"),
        QueryItemKind::PointDirectionQuery => Some("samples"),
        QueryItemKind::Unit => Some("items"),
    }
}

fn scalar_domain_arg(descriptor: &QueryContractDescriptor) -> Option<&'static str> {
    descriptor.domain_contract.map(|_| "domain")
}

fn scalar_backend_arg(descriptor: &QueryContractDescriptor) -> Option<&'static str> {
    (descriptor.target == query_contract::QueryTargetKind::World
        && descriptor.cardinality == query_contract::QueryCardinality::Scalar)
        .then_some("backend")
}
