impl FunctionLowerer {
    pub(crate) fn parse_field_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<FieldQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_at" => FieldQueryKind::Distance,
            "normal_at" => FieldQueryKind::Normal,
            "radiance_at" => FieldQueryKind::Radiance,
            "medium_at" => FieldQueryKind::Medium,
            _ => return None,
        };

        let mut capture = None;
        let mut point = None;
        let mut sample = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "point" if !matches!(kind, FieldQueryKind::Radiance) => point = Some(*value),
                "sample" if matches!(kind, FieldQueryKind::Radiance) => sample = Some(*value),
                _ => return None,
            }
        }

        Some(FieldQuerySpec {
            kind,
            capture: capture?,
            point,
            sample,
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

    pub(crate) fn parse_shape_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ShapeQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape" => ShapeQueryKind::Trace,
            "surface_at" => ShapeQueryKind::Surface,
            _ => return None,
        };

        let mut capture = None;
        let mut ray = None;
        let mut hit = None;

        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "ray" if matches!(kind, ShapeQueryKind::Trace) => ray = Some(*value),
                "hit" => hit = Some(*value),
                _ => return None,
            }
        }

        Some(ShapeQuerySpec {
            kind,
            capture: capture?,
            ray,
            hit,
        })
    }

    pub(crate) fn parse_world_point_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<WorldPointQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_world" => WorldPointQueryKind::Distance,
            "normal_world" => WorldPointQueryKind::Normal,
            "radiance_world" => WorldPointQueryKind::Radiance,
            "medium_world" => WorldPointQueryKind::Medium,
            _ => return None,
        };
        let mut capture = None;
        let mut domain = None;
        let mut point = None;
        let mut sample = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "domain" => domain = Some(*value),
                "point" if !matches!(kind, WorldPointQueryKind::Radiance) => point = Some(*value),
                "sample" if matches!(kind, WorldPointQueryKind::Radiance) => sample = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(WorldPointQuerySpec {
            kind,
            capture: capture?,
            domain: domain?,
            point,
            sample,
            backend,
        })
    }

    pub(crate) fn parse_world_shape_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<WorldShapeQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_world" => WorldShapeQueryKind::Trace,
            "surface_world" => WorldShapeQueryKind::Surface,
            _ => return None,
        };
        let mut capture = None;
        let mut domain = None;
        let mut ray = None;
        let mut hit = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "domain" => domain = Some(*value),
                "ray" if matches!(kind, WorldShapeQueryKind::Trace) => ray = Some(*value),
                "hit" => hit = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(WorldShapeQuerySpec {
            kind,
            capture: capture?,
            domain: domain?,
            ray,
            hit,
            backend,
        })
    }

    pub(crate) fn parse_shape_batch_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<ShapeBatchQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape_batch" => ShapeBatchQueryKind::Trace,
            "surface_at_batch" => ShapeBatchQueryKind::Surface,
            "occluded_batch" => ShapeBatchQueryKind::Occluded,
            _ => return None,
        };
        let mut capture = None;
        let mut items = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "rays"
                    if matches!(
                        kind,
                        ShapeBatchQueryKind::Trace | ShapeBatchQueryKind::Occluded
                    ) =>
                {
                    items = Some(*value)
                }
                "hits" if matches!(kind, ShapeBatchQueryKind::Surface) => items = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(ShapeBatchQuerySpec {
            kind,
            capture: capture?,
            items: items?,
            backend: backend?,
        })
    }

    pub(crate) fn parse_field_batch_query(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<FieldBatchQuerySpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_at_batch" => FieldBatchQueryKind::Distance,
            "normal_at_batch" => FieldBatchQueryKind::Normal,
            _ => return None,
        };
        let mut capture = None;
        let mut items = None;
        let mut backend = None;
        for arg in args {
            let hir::Arg::Named { name, value, .. } = arg else {
                return None;
            };
            match name.as_str() {
                "capture" => capture = Some(*value),
                "points" => items = Some(*value),
                "backend" => backend = Some(*value),
                _ => return None,
            }
        }
        Some(FieldBatchQuerySpec {
            kind,
            capture: capture?,
            items: items?,
            backend: backend?,
        })
    }

    pub(crate) fn lower_field_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &FieldQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let plan = self.build_capture_query_plan(body, spec);
        let kernel_plan = lower_capture_query_plan(&plan);
        debug_assert!(
            validate_capture_query_plan(&kernel_plan).is_ok(),
            "compiler-generated capture query plans must stay kernel-valid"
        );
        match spec.kind {
            FieldQueryKind::Distance => {
                let point = self.lower_expr(body, spec.point.expect("distance_at missing point"));
                self.lower_call_temp(MirType::Float, plan.helper_name, vec![capture, point], span)
            }
            FieldQueryKind::Normal => {
                let point = self.lower_expr(body, spec.point.expect("normal_at missing point"));
                self.lower_call_temp(MirType::Vec3, plan.helper_name, vec![capture, point], span)
            }
            FieldQueryKind::Radiance => {
                let sample =
                    self.lower_expr(body, spec.sample.expect("radiance_at missing sample"));
                self.lower_call_temp(MirType::Vec3, plan.helper_name, vec![capture, sample], span)
            }
            FieldQueryKind::Medium => {
                let point = self.lower_expr(body, spec.point.expect("medium_at missing point"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Medium")),
                    plan.helper_name,
                    vec![capture, point],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_shape_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ShapeQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let plan = self.build_shape_capture_query_plan(body, spec);
        let kernel_plan = lower_capture_query_plan(&plan);
        debug_assert!(
            validate_capture_query_plan(&kernel_plan).is_ok(),
            "compiler-generated shape capture query plans must stay kernel-valid"
        );
        match spec.kind {
            ShapeQueryKind::Trace => {
                let ray = self.lower_expr(body, spec.ray.expect("trace_shape missing ray"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    plan.helper_name,
                    vec![capture, ray],
                    span,
                )
            }
            ShapeQueryKind::Surface => {
                let hit = self.lower_expr(body, spec.hit.expect("surface_at missing hit"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    plan.helper_name,
                    vec![capture, hit],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_world_point_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &WorldPointQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let domain = self.lower_expr(body, spec.domain);
        let backend = self.lower_world_query_backend_value(body, spec.backend, span);
        let plan = self.build_world_point_query_plan(body, spec);
        let kernel_plan = lower_world_query_plan(&plan);
        debug_assert!(
            validate_world_query_plan(&kernel_plan).is_ok(),
            "compiler-generated world point query plans must stay kernel-valid"
        );
        match spec.kind {
            WorldPointQueryKind::Distance => {
                let point = self.lower_expr(body, spec.point.expect("distance_world missing point"));
                self.lower_call_temp(
                    MirType::Float,
                    plan.helper_name,
                    vec![capture, domain, point, backend],
                    span,
                )
            }
            WorldPointQueryKind::Normal => {
                let point = self.lower_expr(body, spec.point.expect("normal_world missing point"));
                self.lower_call_temp(
                    MirType::Vec3,
                    plan.helper_name,
                    vec![capture, domain, point, backend],
                    span,
                )
            }
            WorldPointQueryKind::Radiance => {
                let sample =
                    self.lower_expr(body, spec.sample.expect("radiance_world missing sample"));
                self.lower_call_temp(
                    MirType::Vec3,
                    plan.helper_name,
                    vec![capture, domain, sample, backend],
                    span,
                )
            }
            WorldPointQueryKind::Medium => {
                let point = self.lower_expr(body, spec.point.expect("medium_world missing point"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Medium")),
                    plan.helper_name,
                    vec![capture, domain, point, backend],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_world_shape_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &WorldShapeQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let domain = self.lower_expr(body, spec.domain);
        let backend = self.lower_world_query_backend_value(body, spec.backend, span);
        let plan = self.build_world_shape_query_plan(body, spec);
        let kernel_plan = lower_world_query_plan(&plan);
        debug_assert!(
            validate_world_query_plan(&kernel_plan).is_ok(),
            "compiler-generated world shape query plans must stay kernel-valid"
        );
        match spec.kind {
            WorldShapeQueryKind::Trace => {
                let ray = self.lower_expr(body, spec.ray.expect("trace_world missing ray"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Hit3")),
                    plan.helper_name,
                    vec![capture, domain, ray, backend],
                    span,
                )
            }
            WorldShapeQueryKind::Surface => {
                let hit = self.lower_expr(body, spec.hit.expect("surface_world missing hit"));
                self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    plan.helper_name,
                    vec![capture, domain, hit, backend],
                    span,
                )
            }
        }
    }

    pub(crate) fn build_shape_batch_query_plan(
        &self,
        body: &hir::Body,
        spec: &ShapeBatchQuerySpec,
    ) -> BatchQueryPlan {
        let kind = match spec.kind {
            ShapeBatchQueryKind::Trace => BatchQueryKind::Trace,
            ShapeBatchQueryKind::Surface => BatchQueryKind::Surface,
            ShapeBatchQueryKind::Occluded => BatchQueryKind::Occluded,
        };
        let backend = self
            .parse_dispatch_backend_builtin(body, spec.backend)
            .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
            .unwrap_or(DispatchBackend::Auto);
        let scene = self.batch_capture_scene_summary(body, spec.capture, CaptureKind::Shape);
        BatchQueryPlan::for_shape_query(kind, backend, scene)
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

    pub(crate) fn build_capture_query_plan(
        &self,
        body: &hir::Body,
        spec: &FieldQuerySpec,
    ) -> CaptureQueryPlan {
        let capture_kind = self.capture_kind_for_expr(body, spec.capture);
        let kind = match spec.kind {
            FieldQueryKind::Distance => CaptureQueryKind::Distance,
            FieldQueryKind::Normal => CaptureQueryKind::Normal,
            FieldQueryKind::Radiance => CaptureQueryKind::Radiance,
            FieldQueryKind::Medium => CaptureQueryKind::Medium,
        };
        let scene = self.batch_capture_scene_summary(body, spec.capture, capture_kind);
        CaptureQueryPlan::for_query(kind, capture_kind, scene)
            .expect("capture query plan must match capture type")
    }

    pub(crate) fn build_shape_capture_query_plan(
        &self,
        body: &hir::Body,
        spec: &ShapeQuerySpec,
    ) -> CaptureQueryPlan {
        let kind = match spec.kind {
            ShapeQueryKind::Trace => CaptureQueryKind::Trace,
            ShapeQueryKind::Surface => CaptureQueryKind::Surface,
        };
        let scene = self.batch_capture_scene_summary(body, spec.capture, CaptureKind::Shape);
        CaptureQueryPlan::for_query(kind, CaptureKind::Shape, scene)
            .expect("shape query plan must use shape captures")
    }

    pub(crate) fn build_world_point_query_plan(
        &self,
        body: &hir::Body,
        spec: &WorldPointQuerySpec,
    ) -> WorldQueryPlan {
        let kind = match spec.kind {
            WorldPointQueryKind::Distance => WorldQueryKind::Distance,
            WorldPointQueryKind::Normal => WorldQueryKind::Normal,
            WorldPointQueryKind::Radiance => WorldQueryKind::Radiance,
            WorldPointQueryKind::Medium => WorldQueryKind::Medium,
        };
        let backend = self.world_query_plan_backend(body, spec.backend);
        WorldQueryPlan::for_query_with_backend(kind, backend)
    }

    pub(crate) fn build_world_shape_query_plan(
        &self,
        body: &hir::Body,
        spec: &WorldShapeQuerySpec,
    ) -> WorldQueryPlan {
        let kind = match spec.kind {
            WorldShapeQueryKind::Trace => WorldQueryKind::Trace,
            WorldShapeQueryKind::Surface => WorldQueryKind::Surface,
        };
        let backend = self.world_query_plan_backend(body, spec.backend);
        WorldQueryPlan::for_query_with_backend(kind, backend)
    }

    pub(crate) fn build_field_batch_query_plan(
        &self,
        body: &hir::Body,
        spec: &FieldBatchQuerySpec,
    ) -> BatchQueryPlan {
        let capture_kind = self.capture_kind_for_expr(body, spec.capture);
        let kind = match spec.kind {
            FieldBatchQueryKind::Distance => BatchQueryKind::Distance,
            FieldBatchQueryKind::Normal => BatchQueryKind::Normal,
        };
        let backend = self
            .parse_dispatch_backend_builtin(body, spec.backend)
            .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
            .unwrap_or(DispatchBackend::Auto);
        let scene = self.batch_capture_scene_summary(body, spec.capture, capture_kind);
        BatchQueryPlan::for_field_query(kind, capture_kind, backend, scene)
    }

    pub(crate) fn lower_shape_batch_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ShapeBatchQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let items = self.lower_expr(body, spec.items);
        let backend_value = self.lower_expr(body, spec.backend);
        let backend = self.lower_dispatch_backend_id(backend_value, span);
        let plan = self.build_shape_batch_query_plan(body, spec);
        self.lower_call_temp(
            MirType::Named(SmolStr::new("List")),
            plan.helper_name.clone(),
            vec![capture, items, backend],
            span,
        )
    }

    pub(crate) fn lower_field_batch_query_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &FieldBatchQuerySpec,
    ) -> Value {
        let capture = self.lower_expr(body, spec.capture);
        let items = self.lower_expr(body, spec.items);
        let backend_value = self.lower_expr(body, spec.backend);
        let backend = self.lower_dispatch_backend_id(backend_value, span);
        let plan = self.build_field_batch_query_plan(body, spec);
        self.lower_call_temp(
            MirType::Named(SmolStr::new("List")),
            plan.helper_name.clone(),
            vec![capture, items, backend],
            span,
        )
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
                plan.kind,
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
            other => panic!("batch helpers do not support executor {other:?}"),
        }
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
                    let mode = self.shape_point_batch_execution_mode(plan.kind, &name);
                    self.lower_shape_distance_call_with_mode(&name, point.clone(), span, mode)
                }
                PlanExecutor::FieldNormalCapture => {
                    self.lower_field_normal_call(&name, point.clone(), span)
                }
                PlanExecutor::ShapeNormalCapture => {
                    let mode = self.shape_point_batch_execution_mode(plan.kind, &name);
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
            QueryResultKind::Hit3 | QueryResultKind::Surface => value,
            QueryResultKind::OcclusionResult => self.build_occlusion_result_value(value, span),
            other => panic!("batch helpers do not support result kind {other:?}"),
        }
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
