//! Owns CPU execution of world and batch query plans on `DirectQueryOps`.
//! Does not own tracing internals, portable expression evaluation, or backend
//! dispatch selection.
//!
//! Key invariants:
//! - query execution must honor the contract kind and semantics resolved by the
//!   plan before touching shape-specific helpers.
//! - batch/world helpers may share infrastructure, but they cannot blur the
//!   item-shape guarantees each contract advertises.
//!
//! Primary entrypoints:
//! - `DirectQueryOps::execute_world_query`
//! - `DirectQueryOps::execute_batch_query`
//!
//! Failure modes / common pitfalls:
//! - mixing capture/domain argument assumptions between query kinds produces
//!   valid-looking but semantically wrong CPU results.

use super::*;

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn execute_world_query(
        &self,
        plan: &KernelWorldQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let kind = world_kind_for_plan(plan)?;
        let semantics = world_query_semantics_for_contract(plan.contract_id);
        let capture = self.resolve_region_capture(args.first())?;
        let domain = expect_struct(args.get(1), "SceneDomain")?;
        let detail = self.validate_world_domain(&capture, domain, semantics.query_name)?;
        match kind {
            WorldQueryKind::Distance => {
                let point = expect_vec3(args.get(2), "point")?;
                Ok(KernelValue::F32(
                    self.eval_world_distance(&capture, detail, point)?,
                ))
            }
            WorldQueryKind::Normal => {
                let point = expect_vec3(args.get(2), "point")?;
                let mut backend = CpuWorldNormalBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    point,
                };
                Ok(KernelValue::Vec3(execute_world_normal(&mut backend)?))
            }
            WorldQueryKind::SupportSummary => self.support_summary_for_region(&capture, detail),
            WorldQueryKind::Nearest | WorldQueryKind::Trace => {
                let ray = expect_struct(args.get(2), "RayQuery")?;
                self.execute_world_ray_hit(plan, &capture, detail, ray, WorldQueryKind::Nearest)
            }
            WorldQueryKind::Occluded => {
                let ray = expect_struct(args.get(2), "RayQuery")?;
                let hit = self.execute_world_ray_hit(
                    plan,
                    &capture,
                    detail,
                    ray,
                    WorldQueryKind::Occluded,
                )?;
                let hit = expect_struct_ref(&hit, "Hit3")?;
                Ok(occlusion_result(
                    expect_struct_bool(hit, "hit")?,
                    expect_struct_f32(hit, "distance")?,
                    expect_struct_i32(hit, "steps")?,
                ))
            }
            WorldQueryKind::Surface => {
                let hit = expect_struct(args.get(2), "Hit3")?;
                let mut backend = CpuWorldSurfaceBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    hit: hit.clone(),
                    root_shape_id: expect_struct_u32(hit, "root_shape_id")?,
                    result: default_surface(),
                };
                execute_world_surface(&mut backend)?;
                Ok(backend.result)
            }
            WorldQueryKind::Radiance => {
                let sample = expect_struct(args.get(2), "PointDirectionQuery")?;
                let point = expect_struct_vec3(sample, "point")?;
                let direction = expect_struct_vec3(sample, "direction")?;
                let mut backend = CpuWorldRadianceBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    point,
                    direction,
                    result: [0.0, 0.0, 0.0],
                };
                execute_world_radiance(&mut backend)?;
                Ok(KernelValue::Vec3(backend.result))
            }
            WorldQueryKind::Medium => {
                let point = expect_vec3(args.get(2), "point")?;
                let mut backend = CpuWorldMediumBackend {
                    evaluator: self,
                    capture: &capture,
                    detail,
                    domain,
                    point,
                    density: 0.0,
                    emission: [0.0, 0.0, 0.0],
                    anisotropy: 0.0,
                };
                execute_world_medium(&mut backend)?;
                Ok(medium_value(
                    backend.density,
                    backend.emission,
                    backend.anisotropy,
                ))
            }
        }
    }

    pub(crate) fn validate_world_domain(
        &self,
        capture: &SmolStr,
        domain: &KernelStructValue,
        query_name: &str,
    ) -> Result<i32, QueryExecError> {
        let capture_scene_id = self.ctx.region_scene_id(capture);
        let domain_scene_id = expect_struct_u32(domain, "scene_id")?;
        if capture_scene_id != domain_scene_id {
            return Err(QueryExecError::Unsupported {
                message: world_domain_mismatch_message(query_name),
            });
        }
        let spatial = expect_struct_ref(struct_field(domain, "spatial")?, "SpatialDomainContract")?;
        expect_struct_i32(spatial, "geometry_detail")
    }

    pub(crate) fn world_domain_flag_enabled(
        &self,
        domain: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<bool, QueryExecError> {
        let Some(flag) = world_query_semantics(kind).domain_flag else {
            return Ok(true);
        };
        let (contract_field, contract_name) = match flag {
            "material" => ("surface", "SurfaceDomainContract"),
            "radiance" | "media" => ("participants", "ParticipantDomainContract"),
            _ => {
                return Err(QueryExecError::Unsupported {
                    message: format!("unknown SceneDomain flag '{flag}'"),
                });
            }
        };
        let contract = expect_struct_ref(struct_field(domain, contract_field)?, contract_name)?;
        expect_struct_bool(contract, flag)
    }

    pub(crate) fn execute_batch_query(
        &self,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let kind = batch_kind_for_plan(plan)?;
        if plan.capture_kind == crate::query_plan::CaptureKind::Region {
            return self.execute_world_batch_query(plan, args);
        }
        let capture = match kind {
            BatchQueryKind::Distance | BatchQueryKind::Normal => {
                self.resolve_field_or_shape_capture(args.first())
            }
            BatchQueryKind::Nearest
            | BatchQueryKind::Trace
            | BatchQueryKind::Surface
            | BatchQueryKind::Occluded
            | BatchQueryKind::Radiance
            | BatchQueryKind::Medium => self.resolve_shape_capture(args.first()),
        }?;
        let items = expect_array(
            args.get(1),
            if matches!(kind, BatchQueryKind::Distance | BatchQueryKind::Normal) {
                "points"
            } else if matches!(kind, BatchQueryKind::Surface) {
                "hits"
            } else {
                "rays"
            },
        )?;
        self.note_candidate_count(items.len() as u32);
        self.note_batch_dispatch_shape(items.len() as u32, false);
        self.note_batch_execution_mode(!matches!(
            plan.pruning_strategy,
            crate::query_plan::PruningStrategy::None
                | crate::query_plan::PruningStrategy::ConservativeTraversal
        ));
        let mut out = Vec::with_capacity(items.len());
        let capture_value = args.first().cloned().unwrap_or_else(|| {
            self.ctx
                .snapshot_handle_for_kind(snapshot_capture_kind(plan.capture_kind), &capture)
                .expect("resolved capture must have a snapshot handle")
                .capture_value()
        });
        for item in items {
            out.push(execute_batch_item_contract(
                self,
                &plan.item_contract,
                Some(&capture_value),
                item,
            )?);
        }
        Ok(KernelValue::Array(out))
    }

    pub(crate) fn execute_world_batch_query(
        &self,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let capture = self.resolve_region_capture(args.first())?;
        let domain = expect_struct(args.get(1), "SceneDomain")?.clone();
        let items = expect_array(args.get(2), "world batch items")?;
        self.note_batch_dispatch_shape(items.len() as u32, true);
        self.note_batch_execution_mode(!matches!(
            plan.pruning_strategy,
            crate::query_plan::PruningStrategy::None
                | crate::query_plan::PruningStrategy::ConservativeTraversal
        ));

        let KernelBatchItemContract::WorldQuery { plan: world_plan } = &plan.item_contract else {
            return Err(QueryExecError::Unsupported {
                message: "world-batch plans require a world-query item contract".to_string(),
            });
        };
        let capture_value = args.first().cloned().unwrap_or_else(|| {
            self.ctx
                .region_snapshot_handle(&capture)
                .expect("resolved region capture must have a snapshot handle")
                .capture_value()
        });
        let domain_value = KernelValue::Struct(domain);
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let world_args =
                build_world_batch_args(world_plan, &capture_value, &domain_value, item)?;
            let value = self.execute_world_query(world_plan, &world_args)?;
            out.push(wrap_world_batch_result(world_plan, value)?);
        }
        Ok(KernelValue::Array(out))
    }

    pub(crate) fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) => {
                if self.ctx.field_names.contains(name) || self.ctx.shape_names.contains(name) {
                    Ok(name.clone())
                } else {
                    Err(QueryExecError::MissingCaptureTarget {
                        kind: "field-or-shape capture",
                    })
                }
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "FieldCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let name = self
                    .ctx
                    .field_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownFieldCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Field)
                    .filter(|handle| {
                        handle.capture_name() == &name && handle.portable_scene_id() == scene_id
                    })
                    .or_else(|| self.ctx.field_snapshot_handle(&name))
                    .expect("field scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("field", &name, handle, epoch)?;
                Ok(name)
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let root_feature_id = expect_struct_u32(value, "root_feature_id")?;
                let name = self
                    .ctx
                    .shape_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Shape)
                    .filter(|handle| {
                        handle.capture_name() == &name
                            && handle.portable_scene_id() == scene_id
                            && handle.portable_root_feature_id() == root_feature_id
                    })
                    .or_else(|| self.ctx.shape_snapshot_handle(&name))
                    .expect("shape scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("shape", &name, handle, epoch)?;
                if handle.portable_root_feature_id() != root_feature_id {
                    return Err(QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}:{root_feature_id}")),
                    });
                }
                Ok(name)
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "field-or-shape capture",
            }),
        }
    }

    pub(crate) fn execute_world_ray_hit(
        &self,
        plan: &KernelWorldQueryPlan,
        capture: &SmolStr,
        detail: i32,
        ray: &KernelStructValue,
        kind: WorldQueryKind,
    ) -> Result<KernelValue, QueryExecError> {
        let solver_plan = plan
            .ray_solver
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!(
                    "world ray contract '{}' is missing a RaySolverPlan",
                    plan.contract_id.as_str()
                ),
            })?;
        let origin = expect_struct_vec3(ray, "origin")?;
        let direction = expect_struct_vec3(ray, "direction")?;
        let max_distance = expect_struct_f32(ray, "max_distance")?;
        let min_step = expect_struct_f32(ray, "min_step")?;
        let hit_epsilon = expect_struct_f32(ray, "hit_epsilon")?;
        let max_steps = expect_struct_i32(ray, "max_steps")?;
        let mut backend = CpuWorldTraceBackend {
            evaluator: self,
            capture,
            detail,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            solver_plan,
            artifact_contracts: &plan.artifact_contracts,
            result: default_hit(origin),
            best_distance: f32::INFINITY,
            cache_start_t: 0.0,
        };
        execute_world_ray(
            &mut backend,
            kind,
            match kind {
                WorldQueryKind::Occluded => {
                    "occluded_world requires a capture created from a region declaration"
                }
                WorldQueryKind::Nearest => {
                    "nearest_world requires a capture created from a region declaration"
                }
                _ => "trace_world requires a capture created from a region declaration",
            },
        )?;
        if let Ok(hit) = expect_struct_ref(&backend.result, "Hit3") {
            self.note_hit_result(
                expect_struct_bool(hit, "hit").unwrap_or(false),
                expect_struct_i32(hit, "steps").unwrap_or_default().max(0) as u32,
            );
        }
        Ok(backend.result)
    }

    pub(crate) fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) if self.ctx.shape_names.contains(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "ShapeCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let root_feature_id = expect_struct_u32(value, "root_feature_id")?;
                let name = self
                    .ctx
                    .shape_name_for_root_feature_id(root_feature_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{root_feature_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Shape)
                    .filter(|handle| {
                        handle.capture_name() == &name
                            && handle.portable_scene_id() == scene_id
                            && handle.portable_root_feature_id() == root_feature_id
                    })
                    .or_else(|| self.ctx.shape_snapshot_handle(&name))
                    .expect("shape root-feature index must point at a snapshot handle");
                self.ensure_snapshot_epoch("shape", &name, handle, epoch)?;
                if handle.portable_scene_id() != scene_id {
                    return Err(QueryExecError::UnknownShapeCapture {
                        name: SmolStr::new(format!("{scene_id}:{root_feature_id}")),
                    });
                }
                Ok(name)
            }
            Some(KernelValue::Capture(name)) => {
                Err(QueryExecError::UnknownShapeCapture { name: name.clone() })
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "shape capture",
            }),
        }
    }

    pub(crate) fn resolve_region_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.note_artifact_load();
        match capture {
            // Legacy compatibility: core execution now prefers typed capture structs, but
            // name-only captures remain accepted for older callers and tests.
            Some(KernelValue::Capture(name)) if self.ctx.regions_by_name.contains_key(name) => {
                Ok(name.clone())
            }
            Some(KernelValue::Struct(value)) if value.name.as_str() == "RegionCapture" => {
                let scene_id = expect_struct_u32(value, "scene_id")?;
                let epoch = expect_struct_u32(value, "epoch")?;
                let name = self
                    .ctx
                    .region_name_for_scene_id(scene_id)
                    .cloned()
                    .ok_or_else(|| QueryExecError::UnknownRegionCapture {
                        name: SmolStr::new(format!("{scene_id}")),
                    })?;
                let handle = self
                    .authoritative_snapshot(SnapshotCaptureKind::Region)
                    .filter(|handle| {
                        handle.capture_name() == &name && handle.portable_scene_id() == scene_id
                    })
                    .or_else(|| self.ctx.region_snapshot_handle(&name))
                    .expect("region scene index must point at a snapshot handle");
                self.ensure_snapshot_epoch("region", &name, handle, epoch)?;
                Ok(name)
            }
            Some(KernelValue::Capture(name)) => {
                Err(QueryExecError::UnknownRegionCapture { name: name.clone() })
            }
            _ => Err(QueryExecError::MissingCaptureTarget {
                kind: "region capture",
            }),
        }
    }

    pub(crate) fn eval_capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        match capture_kind {
            crate::query_plan::CaptureKind::Field => self.eval_field_distance(capture, point),
            crate::query_plan::CaptureKind::Shape => self.eval_shape_distance(capture, point),
            crate::query_plan::CaptureKind::Region => Err(QueryExecError::Unsupported {
                message: "region captures are only valid for world queries".to_string(),
            }),
        }
    }

    pub(crate) fn eval_capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        let evaluation = match capture_kind {
            crate::query_plan::CaptureKind::Field => {
                self.eval_field_normal_with_role(capture, point)?
            }
            crate::query_plan::CaptureKind::Shape => {
                self.eval_shape_normal_with_role(capture, point)?
            }
            crate::query_plan::CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures are only valid for world queries".to_string(),
                });
            }
        };
        self.note_normal_role(evaluation.role);
        Ok(evaluation.normal)
    }

    pub(crate) fn support_summary_for_capture(
        &self,
        capture: &SmolStr,
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<KernelValue, QueryExecError> {
        self.note_artifact_load();
        let summary = match capture_kind {
            crate::query_plan::CaptureKind::Field => {
                let scene = self.field_scene(capture)?;
                let bounds = self.field_support_bounds(scene, scene.root_support_id)?;
                SupportSummaryParts {
                    support_class: scene.support_class,
                    semantics: scene.semantics,
                    has_bounds: bounds.is_some(),
                    opaque_boundary: scene.opaque_boundary,
                    can_coarse_support_prune: scene.can_coarse_support_pruning,
                    bounds: bounds.unwrap_or_else(empty_support_bounds),
                }
            }
            crate::query_plan::CaptureKind::Shape => {
                let scene = self.shape_scene(capture)?;
                let bounds = self.shape_support_bounds(scene, scene.root_support_id)?;
                SupportSummaryParts {
                    support_class: scene.support_class,
                    semantics: scene.semantics,
                    has_bounds: bounds.is_some(),
                    opaque_boundary: scene.opaque_boundary,
                    can_coarse_support_prune: scene.can_coarse_support_pruning,
                    bounds: bounds.unwrap_or_else(empty_support_bounds),
                }
            }
            crate::query_plan::CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures require support_summary_world".to_string(),
                });
            }
        };
        Ok(support_summary_value(summary))
    }

    pub(crate) fn support_summary_for_region(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.note_artifact_load();
        let shapes = self.resolve_world_shapes(capture, detail, None)?;
        let mut shape_summaries = Vec::with_capacity(shapes.len());
        for shape in shapes {
            let scene = self.shape_scene(&shape)?;
            let bounds = self.shape_support_bounds(scene, scene.root_support_id)?;
            shape_summaries.push(SupportSummaryParts {
                support_class: scene.support_class,
                semantics: scene.semantics,
                has_bounds: bounds.is_some(),
                opaque_boundary: scene.opaque_boundary,
                can_coarse_support_prune: scene.can_coarse_support_pruning,
                bounds: bounds.unwrap_or_else(empty_support_bounds),
            });
        }
        Ok(support_summary_value(merge_world_support_summaries(
            &shape_summaries,
        )))
    }

    pub(crate) fn eval_field_distance(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_field_sample();
        let scene = self.field_scene(field)?;
        if scene.root.contains_opaque_leaf() {
            return self.eval_opaque_field_distance(field, point);
        }
        self.eval_field_node(&scene.root, point)
    }

    pub(crate) fn eval_field_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        Ok(self.eval_field_normal_with_role(field, point)?.normal)
    }

    pub(crate) fn eval_shape_distance(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_field_sample();
        let scene =
            self.ctx
                .scene
                .shapes
                .get(shape)
                .ok_or_else(|| QueryExecError::MissingShape {
                    name: shape.clone(),
                })?;
        if let ShapeNode::Union { items } = &scene.root
            && let Some(tree) = self.shape_root_union_tree(shape)?
        {
            self.note_union_cluster_visit();
            return self.eval_shape_union_tree(items, &tree, point);
        }
        self.note_cache_dense_fallback();
        self.eval_shape_node(&scene.root, point)
    }

    pub(crate) fn eval_shape_normal(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        Ok(self.eval_shape_normal_with_role(shape, point)?.normal)
    }

    pub(crate) fn eval_field_normal_with_role(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<NormalEvaluation, QueryExecError> {
        if let Some(normal) = self.try_certified_field_normal(field, point)? {
            return Ok(normal);
        }
        Ok(NormalEvaluation {
            normal: self.finite_difference_normal(point, |sample_point| {
                self.eval_field_distance(field, sample_point)
            })?,
            role: NormalRole::HeuristicShadingNormal,
        })
    }

    pub(crate) fn eval_shape_normal_with_role(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<NormalEvaluation, QueryExecError> {
        if let Some(normal) = self.try_certified_shape_normal(shape, point)? {
            return Ok(normal);
        }
        Ok(NormalEvaluation {
            normal: self.finite_difference_normal(point, |sample_point| {
                self.eval_shape_distance(shape, sample_point)
            })?,
            role: NormalRole::HeuristicShadingNormal,
        })
    }

    pub(crate) fn finite_difference_normal<F>(
        &self,
        point: [f32; 3],
        mut sample: F,
    ) -> Result<[f32; 3], QueryExecError>
    where
        F: FnMut([f32; 3]) -> Result<f32, QueryExecError>,
    {
        let eps = 0.001f32;
        let dx = sample([point[0] + eps, point[1], point[2]])?
            - sample([point[0] - eps, point[1], point[2]])?;
        let dy = sample([point[0], point[1] + eps, point[2]])?
            - sample([point[0], point[1] - eps, point[2]])?;
        let dz = sample([point[0], point[1], point[2] + eps])?
            - sample([point[0], point[1], point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    pub(crate) fn try_certified_field_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let scene = self.field_scene(field)?;
        if scene.opaque_boundary
            || !matches!(
                scene.analysis.differential_support,
                crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
            )
        {
            return Ok(None);
        }
        self.try_certified_field_normal_node(&scene.root, point)
    }

    pub(crate) fn try_certified_field_normal_node(
        &self,
        node: &FieldNode,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        match node {
            FieldNode::Use { target } => self.try_certified_field_normal(target, point),
            FieldNode::Primitive { primitive, args } => match primitive {
                hir::FieldPrimitive::Sphere => Ok(Some(NormalEvaluation {
                    normal: normalize3(point),
                    role: NormalRole::CertifiedFieldGradient,
                })),
                hir::FieldPrimitive::Plane => {
                    let Some(normal) =
                        self.eval_scene_named_arg_opt(args.as_deref().unwrap_or(&[]), "normal")?
                    else {
                        return Ok(None);
                    };
                    let normal = expect_vec3(Some(&normal), "plane normal")?;
                    if dot3(normal, normal).sqrt() <= f32::EPSILON {
                        return Ok(None);
                    }
                    Ok(Some(NormalEvaluation {
                        normal: normalize3(normal),
                        role: NormalRole::CertifiedFieldGradient,
                    }))
                }
                hir::FieldPrimitive::Torus => {
                    let major_radius =
                        self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "major_radius")?;
                    let minor_radius =
                        self.eval_scene_named_arg(args.as_deref().unwrap_or(&[]), "minor_radius")?;
                    let major_radius = expect_f32(Some(&major_radius), "torus major_radius")?;
                    let minor_radius = expect_f32(Some(&minor_radius), "torus minor_radius")?;
                    if minor_radius <= f32::EPSILON {
                        return Ok(None);
                    }
                    let radial = (point[0] * point[0] + point[2] * point[2]).sqrt();
                    if radial <= f32::EPSILON {
                        return Ok(None);
                    }
                    let tube = radial - major_radius;
                    let normal = [point[0] * tube / radial, point[1], point[2] * tube / radial];
                    if dot3(normal, normal).sqrt() <= f32::EPSILON {
                        return Ok(None);
                    }
                    Ok(Some(NormalEvaluation {
                        normal: normalize3(normal),
                        role: NormalRole::CertifiedFieldGradient,
                    }))
                }
                _ => Ok(None),
            },
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.try_certified_field_normal_node(inner, point);
                };
                match kind {
                    TransformKind::Translate
                    | TransformKind::Rotate
                    | TransformKind::UniformScale => {
                        let local_point = self.eval_wrapped_point(*kind, param, point)?;
                        let Some(mut inner) =
                            self.try_certified_field_normal_node(inner, local_point)?
                        else {
                            return Ok(None);
                        };
                        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
                        inner.normal = transform_certified_normal(*kind, &config, inner.normal)?;
                        Ok(Some(NormalEvaluation {
                            normal: normalize3(inner.normal),
                            role: inner.role,
                        }))
                    }
                    _ => Ok(None),
                }
            }
            FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => {
                let Some(first) = items.first() else {
                    return Ok(None);
                };
                let smoothing_value = smoothing
                    .as_ref()
                    .map(|expr| self.eval_scene_value_expr(expr, &HashMap::new()))
                    .transpose()?
                    .unwrap_or(KernelValue::F32(0.0));
                let smoothing = expect_f32(Some(&smoothing_value), "smoothing")?;
                if smoothing <= 0.0 {
                    return Ok(None);
                }
                let Some(first_normal) = self.try_certified_field_normal_node(first, point)? else {
                    return Ok(None);
                };
                let mut current_distance = self.eval_field_node(first, point)?;
                let mut current_normal = first_normal.normal;
                match kind {
                    SmoothKind::Union | SmoothKind::Intersection => {
                        for item in items.iter().skip(1) {
                            let Some(rhs_normal) =
                                self.try_certified_field_normal_node(item, point)?
                            else {
                                return Ok(None);
                            };
                            let rhs_distance = self.eval_field_node(item, point)?;
                            current_normal = smooth_blended_normal(
                                *kind,
                                smoothing,
                                current_distance,
                                current_normal,
                                rhs_distance,
                                rhs_normal.normal,
                            );
                            current_distance = match kind {
                                SmoothKind::Union => runtime_ternary_f32(
                                    smoothing,
                                    current_distance,
                                    rhs_distance,
                                    wr_smooth_union,
                                )?,
                                SmoothKind::Intersection => runtime_ternary_f32(
                                    smoothing,
                                    current_distance,
                                    rhs_distance,
                                    wr_smooth_intersection,
                                )?,
                                SmoothKind::Subtract => unreachable!(),
                            };
                        }
                    }
                    SmoothKind::Subtract => {
                        let Some(rhs) = items.get(1) else {
                            return Ok(None);
                        };
                        let Some(rhs_normal) = self.try_certified_field_normal_node(rhs, point)?
                        else {
                            return Ok(None);
                        };
                        let rhs_distance = self.eval_field_node(rhs, point)?;
                        current_normal = smooth_blended_normal(
                            *kind,
                            smoothing,
                            current_distance,
                            current_normal,
                            rhs_distance,
                            rhs_normal.normal,
                        );
                    }
                }
                Ok(Some(NormalEvaluation {
                    normal: normalize3(current_normal),
                    role: NormalRole::CertifiedFieldGradient,
                }))
            }
            FieldNode::Repeat { .. }
            | FieldNode::Union { .. }
            | FieldNode::Intersection { .. }
            | FieldNode::Subtract { .. }
            | FieldNode::Extrude { .. }
            | FieldNode::Revolve { .. }
            | FieldNode::Sweep { .. }
            | FieldNode::Loft { .. }
            | FieldNode::OpaqueLeaf => Ok(None),
        }
    }

    pub(crate) fn try_certified_shape_normal(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        if scene.opaque_boundary
            || !matches!(
                scene.analysis.differential_support,
                crate::scene_ir::SceneDifferentialSupport::CertifiedGradient
            )
        {
            return Ok(None);
        }
        match &scene.root {
            ShapeNode::Use { target } => self.try_certified_shape_normal(target, point),
            ShapeNode::Leaf(leaf) => {
                let Some(mut field_normal) = self.try_certified_field_normal(&leaf.field, point)?
                else {
                    return Ok(None);
                };
                field_normal.role = NormalRole::FeatureNormal;
                Ok(Some(field_normal))
            }
            ShapeNode::Union { .. }
            | ShapeNode::Intersection { .. }
            | ShapeNode::Subtract { .. } => Ok(None),
        }
    }

    pub(crate) fn try_certified_world_normal(
        &self,
        capture: &SmolStr,
        detail: i32,
        point: [f32; 3],
    ) -> Result<Option<NormalEvaluation>, QueryExecError> {
        let shapes = self.resolve_world_shapes(capture, detail, None)?;
        // Keep the certified world path conservative: only single-shape regions
        // with a certifiable shape leaf can skip finite differences.
        let [shape] = shapes.as_slice() else {
            return Ok(None);
        };
        let result = self.try_certified_shape_normal(shape, point)?;
        if result.is_some() {
            self.note_interval_proof_success();
        }
        Ok(result)
    }

    pub(crate) fn eval_field_node(
        &self,
        node: &FieldNode,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_branch_visit();
        self.note_acceleration_node_visit();
        match node {
            FieldNode::Use { target } => self.eval_field_distance(target, point),
            FieldNode::Primitive { primitive, args } => {
                self.eval_field_primitive(*primitive, args.as_deref().unwrap_or(&[]), point)
            }
            FieldNode::Union { items } => {
                self.note_union_cluster_visit();
                let mut current = 1_000_000.0f32;
                for item in items {
                    current = runtime_binary_f32(
                        current,
                        self.eval_field_node(item, point)?,
                        wr_field_union,
                    )?;
                }
                Ok(current)
            }
            FieldNode::Intersection { items } => {
                self.note_union_cluster_visit();
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Ok(1_000_000.0);
                };
                let mut current = self.eval_field_node(first, point)?;
                for item in iter {
                    current = runtime_binary_f32(
                        current,
                        self.eval_field_node(item, point)?,
                        wr_field_intersection,
                    )?;
                }
                Ok(current)
            }
            FieldNode::Subtract { left, right } => Ok(runtime_binary_f32(
                self.eval_field_node(left, point)?,
                self.eval_field_node(right, point)?,
                wr_field_subtract,
            )?),
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_node(inner, point);
                };
                let local_point = self.eval_wrapped_point(*kind, param, point)?;
                let inner_distance = self.eval_field_node(inner, local_point)?;
                if matches!(kind, TransformKind::UniformScale) {
                    let scale = self.eval_scene_value_expr(param, &HashMap::new())?;
                    Ok(inner_distance * expect_abs_scalar(&scale)?)
                } else {
                    Ok(inner_distance)
                }
            }
            FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_node(inner, point);
                };
                self.note_repeat_cell_skip();
                let local_point = self.eval_repeat_point(*kind, param, point)?;
                self.eval_field_node(inner, local_point)
            }
            FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => {
                let Some(first) = items.first() else {
                    return Ok(1_000_000.0);
                };
                let smoothing_value = smoothing
                    .as_ref()
                    .map(|expr| self.eval_scene_value_expr(expr, &HashMap::new()))
                    .transpose()?
                    .unwrap_or(KernelValue::F32(0.0));
                let smoothing = expect_f32(Some(&smoothing_value), "smoothing")?;
                let mut current = self.eval_field_node(first, point)?;
                match kind {
                    SmoothKind::Union => {
                        for item in items.iter().skip(1) {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(item, point)?,
                                wr_smooth_union,
                            )?;
                        }
                    }
                    SmoothKind::Intersection => {
                        for item in items.iter().skip(1) {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(item, point)?,
                                wr_smooth_intersection,
                            )?;
                        }
                    }
                    SmoothKind::Subtract => {
                        if items.len() >= 2 {
                            current = runtime_ternary_f32(
                                smoothing,
                                current,
                                self.eval_field_node(&items[1], point)?,
                                wr_smooth_subtract,
                            )?;
                        }
                    }
                }
                Ok(current)
            }
            FieldNode::OpaqueLeaf => Ok(1_000_000.0),
            FieldNode::Extrude { height, profile } => {
                let (Some(height), Some(profile)) = (height.as_ref(), profile.as_ref()) else {
                    return Ok(1_000_000.0);
                };
                let height_value = self.eval_scene_value_expr(height, &HashMap::new())?;
                let abs_height = expect_abs_scalar(&height_value)?;
                let half_height = abs_height * 0.5;
                let profile_distance = self.eval_profile_expr(profile, [point[0], point[2]])?;
                let axial = point[1].abs() - half_height;
                Ok(self.eval_profile_cap_distance(profile_distance, axial))
            }
            FieldNode::Revolve { profile } => {
                let Some(profile) = profile.as_ref() else {
                    return Ok(1_000_000.0);
                };
                let radial = (point[0] * point[0] + point[2] * point[2]).sqrt();
                self.eval_profile_expr(profile, [radial, point[1]])
            }
            FieldNode::Sweep { path, profile } => {
                let (Some(path), Some(profile)) = (path.as_ref(), profile.as_ref()) else {
                    return Ok(1_000_000.0);
                };
                let path_value = self.eval_scene_value_expr(path, &HashMap::new())?;
                let coords = runtime_binary_value(
                    path_value.clone(),
                    KernelValue::Vec3(point),
                    wr_field_sweep_coords,
                )?;
                let coords = expect_vec3(Some(&coords), "field_sweep_coords")?;
                let profile_distance = self.eval_profile_expr(profile, [coords[0], coords[1]])?;
                let path_length = length_of(&path_value)?;
                let axial = coords[2].abs() - path_length * 0.5;
                Ok(self.eval_profile_cap_distance(profile_distance, axial))
            }
            FieldNode::Loft { height, from, to } => {
                let (Some(height), Some(from), Some(to)) =
                    (height.as_ref(), from.as_ref(), to.as_ref())
                else {
                    return Ok(1_000_000.0);
                };
                let height_value = self.eval_scene_value_expr(height, &HashMap::new())?;
                let abs_height = expect_abs_scalar(&height_value)?;
                let half_height = abs_height * 0.5;
                let safe_height = abs_height.max(0.0001);
                let profile_point = [point[0], point[2]];
                let from_distance = self.eval_profile_expr(from, profile_point)?;
                let to_distance = self.eval_profile_expr(to, profile_point)?;
                let t = ((point[1] + half_height) / safe_height).clamp(0.0, 1.0);
                let mixed = from_distance + (to_distance - from_distance) * t;
                let axial = point[1].abs() - half_height;
                Ok(self.eval_profile_cap_distance(mixed, axial))
            }
        }
    }

    pub(crate) fn eval_shape_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_branch_visit();
        self.note_acceleration_node_visit();
        match node {
            ShapeNode::Use { target } => self.eval_shape_distance(target, point),
            ShapeNode::Leaf(leaf) => {
                self.note_shape_leaf_visit();
                self.eval_field_distance(&leaf.field, point)
            }
            ShapeNode::Union { items } => {
                self.note_union_cluster_visit();
                let mut current = 1_000_000.0f32;
                for item in items {
                    current = runtime_binary_f32(
                        current,
                        self.eval_shape_node(item, point)?,
                        wr_field_union,
                    )?;
                }
                Ok(current)
            }
            ShapeNode::Intersection { items } => {
                self.note_union_cluster_visit();
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Ok(1_000_000.0);
                };
                let mut current = self.eval_shape_node(first, point)?;
                for item in iter {
                    current = runtime_binary_f32(
                        current,
                        self.eval_shape_node(item, point)?,
                        wr_field_intersection,
                    )?;
                }
                Ok(current)
            }
            ShapeNode::Subtract { left, right } => Ok(runtime_binary_f32(
                self.eval_shape_node(left, point)?,
                self.eval_shape_node(right, point)?,
                wr_field_subtract,
            )?),
        }
    }

    pub(crate) fn eval_shape_union_tree(
        &self,
        items: &[ShapeNode],
        tree: &CpuAccelerationTree<usize>,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let mut best = f32::INFINITY;
        let mut stack = vec![CpuPointTraversal {
            node_index: tree.root,
            lower_bound: f32::NEG_INFINITY,
        }];

        while let Some(current) = pop_best_point_traversal(&mut stack) {
            self.note_acceleration_node_visit();
            if current.lower_bound > best {
                self.note_acceleration_pruned_node();
                self.note_support_pruned_candidates(tree.leaf_count(current.node_index));
                continue;
            }
            let Some(node) = tree.node(current.node_index) else {
                continue;
            };
            if let Some(child_index) = node.leaf {
                let distance = self.eval_shape_node(&items[child_index], point)?;
                if distance < best {
                    best = distance;
                }
                continue;
            }

            let mut pending = Vec::new();
            for child_index in tree.children_of(current.node_index) {
                let Some(child) = tree.node(*child_index) else {
                    continue;
                };
                let lower_bound = child
                    .bounds
                    .map(|bounds| support_box_lower_bound(bounds.min, bounds.max, point))
                    .transpose()?
                    .unwrap_or(f32::NEG_INFINITY);
                if lower_bound > best {
                    self.note_acceleration_pruned_node();
                    self.note_support_pruned_candidates(child.leaf_count);
                    continue;
                }
                pending.push(CpuPointTraversal {
                    node_index: *child_index,
                    lower_bound,
                });
            }
            push_ordered_point_traversals(&mut stack, pending);
        }

        if best.is_finite() {
            Ok(best)
        } else {
            Ok(1_000_000.0)
        }
    }

    pub(crate) fn eval_wrapped_point(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            TransformKind::Translate => runtime_binary_value(config, point_value, wr_translate)?,
            TransformKind::Rotate => runtime_binary_value(config, point_value, wr_rotate)?,
            TransformKind::UniformScale => {
                runtime_binary_value(config, point_value, wr_uniform_scale)?
            }
            TransformKind::AffineTransform => {
                runtime_binary_value(config, point_value, wr_affine_transform)?
            }
            TransformKind::Warp => runtime_binary_value(config, point_value, wr_warp)?,
            TransformKind::Bend => runtime_binary_value(config, point_value, wr_bend)?,
            TransformKind::Twist => runtime_binary_value(config, point_value, wr_twist)?,
            TransformKind::Taper => runtime_binary_value(config, point_value, wr_taper)?,
            TransformKind::Displace => runtime_binary_value(config, point_value, wr_displace)?,
        };
        expect_vec3(Some(&value), "wrapped point")
    }

    pub(crate) fn eval_wrapped_vector(
        &self,
        kind: TransformKind,
        param: &SceneValueExpr,
        vector: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let vector_value = KernelValue::Vec3(vector);
        let value = match kind {
            TransformKind::Translate => return Ok(vector),
            TransformKind::Rotate => runtime_binary_value(config, vector_value, wr_rotate)?,
            TransformKind::UniformScale => {
                runtime_binary_value(config, vector_value, wr_uniform_scale)?
            }
            TransformKind::AffineTransform
            | TransformKind::Warp
            | TransformKind::Bend
            | TransformKind::Twist
            | TransformKind::Taper
            | TransformKind::Displace => {
                return Err(QueryExecError::Unsupported {
                    message: format!("ray support vector wrapper is unavailable for {kind:?}"),
                });
            }
        };
        expect_vec3(Some(&value), "wrapped vector")
    }

    pub(crate) fn eval_repeat_point(
        &self,
        kind: RepeatKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            RepeatKind::RepeatLinear => {
                runtime_binary_value(config, point_value, wr_repeat_linear)?
            }
            RepeatKind::RepeatGrid => runtime_binary_value(config, point_value, wr_repeat_grid)?,
            RepeatKind::RadialRepeat => {
                runtime_binary_value(config, point_value, wr_radial_repeat)?
            }
            RepeatKind::MirrorArray => runtime_binary_value(config, point_value, wr_mirror_array)?,
            RepeatKind::InstanceArray => {
                runtime_binary_value(config, point_value, wr_instance_array)?
            }
        };
        expect_vec3(Some(&value), "repeat point")
    }

    pub(crate) fn eval_repeat_identity(
        &self,
        kind: RepeatKind,
        param: &SceneValueExpr,
        point: [f32; 3],
    ) -> Result<u32, QueryExecError> {
        let config = self.eval_scene_value_expr(param, &HashMap::new())?;
        let point_value = KernelValue::Vec3(point);
        let value = match kind {
            RepeatKind::RepeatLinear => {
                runtime_binary_value(config, point_value, wr_repeat_linear_identity)?
            }
            RepeatKind::RepeatGrid => {
                runtime_binary_value(config, point_value, wr_repeat_grid_identity)?
            }
            RepeatKind::RadialRepeat => {
                runtime_binary_value(config, point_value, wr_radial_repeat_identity)?
            }
            RepeatKind::MirrorArray => {
                runtime_binary_value(config, point_value, wr_mirror_array_identity)?
            }
            RepeatKind::InstanceArray => {
                runtime_binary_value(config, point_value, wr_instance_array_identity)?
            }
        };
        match value {
            KernelValue::U32(value) => Ok(value),
            KernelValue::I32(value) => Ok(value as u32),
            other => Err(QueryExecError::TypeMismatch {
                expected: "repeat identity: U32".to_string(),
                found: format!("{other:?}"),
            }),
        }
    }

    pub(crate) fn eval_profile_expr(
        &self,
        profile: &SceneProfileExpr,
        point: [f32; 2],
    ) -> Result<f32, QueryExecError> {
        match profile {
            SceneProfileExpr::Primitive { primitive, args } => {
                let point_value = KernelValue::Vec2(point);
                match primitive {
                    hir::ProfilePrimitive::Circle2 => {
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_binary_f32_from_values(point_value, radius, wr_circle2)
                    }
                    hir::ProfilePrimitive::Rect2 => {
                        let half = self.eval_scene_named_arg(args, "half")?;
                        runtime_binary_f32_from_values(point_value, half, wr_rect2)
                    }
                    hir::ProfilePrimitive::RoundedRect2 => {
                        let half = self.eval_scene_named_arg(args, "half")?;
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_ternary_f32_from_values(point_value, half, radius, wr_rounded_rect2)
                    }
                    hir::ProfilePrimitive::Capsule2 => {
                        let a = self.eval_scene_named_arg(args, "a")?;
                        let b = self.eval_scene_named_arg(args, "b")?;
                        let radius = self.eval_scene_named_arg(args, "radius")?;
                        runtime_quaternary_f32(point_value, a, b, radius, wr_capsule2)
                    }
                    hir::ProfilePrimitive::Segment2 => {
                        let a = self.eval_scene_named_arg(args, "a")?;
                        let b = self.eval_scene_named_arg(args, "b")?;
                        runtime_ternary_f32_from_values(point_value, a, b, wr_segment2)
                    }
                    hir::ProfilePrimitive::Polygon2 => {
                        let vertices = self.eval_scene_named_arg(args, "vertices")?;
                        polygon_profile_distance(point, &vertices, true)
                    }
                    hir::ProfilePrimitive::Polyline2 => {
                        let vertices = self.eval_scene_named_arg(args, "vertices")?;
                        polygon_profile_distance(point, &vertices, false)
                    }
                }
            }
        }
    }

    pub(crate) fn eval_profile_cap_distance(
        &self,
        profile_distance: f32,
        axial_distance: f32,
    ) -> f32 {
        let outside_x = profile_distance.max(0.0);
        let outside_y = axial_distance.max(0.0);
        let outside_len = (outside_x * outside_x + outside_y * outside_y).sqrt();
        let inside = profile_distance.max(axial_distance).min(0.0);
        inside + outside_len
    }

    pub(crate) fn eval_opaque_field_distance(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        self.note_opaque_fallback();
        let scene = self.field_scene(field)?;
        let bounds_expr =
            scene
                .authored_bounds
                .as_ref()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("opaque field '{field}' is missing authored bounds"),
                })?;
        let bounds_value = self.eval_scene_value_expr(bounds_expr, &HashMap::new())?;
        let bounds = expect_struct_ref(&bounds_value, "Bounds3")?;
        let min = expect_struct_vec3(bounds, "min")?;
        let max = expect_struct_vec3(bounds, "max")?;
        let center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        let half = [
            (max[0] - min[0]) * 0.5,
            (max[1] - min[1]) * 0.5,
            (max[2] - min[2]) * 0.5,
        ];
        runtime_binary_f32_from_values(
            KernelValue::Vec3([
                point[0] - center[0],
                point[1] - center[1],
                point[2] - center[2],
            ]),
            KernelValue::Vec3(half),
            wr_box,
        )
    }

    pub(crate) fn eval_field_primitive(
        &self,
        primitive: hir::FieldPrimitive,
        args: &[SceneArgExpr],
        point: [f32; 3],
    ) -> Result<f32, QueryExecError> {
        let point = KernelValue::Vec3(point);
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_binary_f32_from_values(point, radius, wr_sphere)
            }
            hir::FieldPrimitive::Box => {
                let half = self
                    .eval_scene_named_arg_opt(args, "half")?
                    .or_else(|| {
                        self.eval_scene_named_arg_opt(args, "half_size")
                            .ok()
                            .flatten()
                    })
                    .ok_or_else(|| QueryExecError::MissingCaptureTarget { kind: "box half" })?;
                runtime_binary_f32_from_values(point, half, wr_box)
            }
            hir::FieldPrimitive::Capsule => {
                let a = self.eval_scene_named_arg(args, "a")?;
                let b = self.eval_scene_named_arg(args, "b")?;
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_quaternary_f32(point, a, b, radius, wr_capsule)
            }
            hir::FieldPrimitive::Cylinder => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, radius, half_height, wr_cylinder)
            }
            hir::FieldPrimitive::Plane => {
                let normal = self.eval_scene_named_arg(args, "normal")?;
                let offset = self.eval_scene_named_arg(args, "offset")?;
                runtime_ternary_f32_from_values(point, normal, offset, wr_plane)
            }
            hir::FieldPrimitive::Torus => {
                let major_radius = self.eval_scene_named_arg(args, "major_radius")?;
                let minor_radius = self.eval_scene_named_arg(args, "minor_radius")?;
                runtime_ternary_f32_from_values(point, major_radius, minor_radius, wr_torus)
            }
            hir::FieldPrimitive::RoundedBox => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let radius = self.eval_scene_named_arg(args, "radius")?;
                runtime_ternary_f32_from_values(point, half, radius, wr_rounded_box)
            }
            hir::FieldPrimitive::Ellipsoid => {
                let radii = self.eval_scene_named_arg(args, "radii")?;
                runtime_binary_f32_from_values(point, radii, wr_ellipsoid)
            }
            hir::FieldPrimitive::Cone => {
                let radius = self.eval_scene_named_arg(args, "radius")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, radius, half_height, wr_cone)
            }
            hir::FieldPrimitive::CappedCone => {
                let radius_bottom = self.eval_scene_named_arg(args, "radius_bottom")?;
                let radius_top = self.eval_scene_named_arg(args, "radius_top")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_quaternary_f32(
                    point,
                    radius_bottom,
                    radius_top,
                    half_height,
                    wr_capped_cone,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let thickness = self.eval_scene_named_arg(args, "thickness")?;
                runtime_ternary_f32_from_values(point, half, thickness, wr_box_frame)
            }
            hir::FieldPrimitive::Slab => {
                let thickness = self.eval_scene_named_arg(args, "thickness")?;
                runtime_binary_f32_from_values(point, thickness, wr_slab)
            }
            hir::FieldPrimitive::TrianglePrism => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, half, half_height, wr_triangle_prism)
            }
            hir::FieldPrimitive::HexPrism => {
                let half = self.eval_scene_named_arg(args, "half")?;
                let half_height = self.eval_scene_named_arg(args, "half_height")?;
                runtime_ternary_f32_from_values(point, half, half_height, wr_hex_prism)
            }
        }
    }

    pub(crate) fn field_scene(
        &self,
        field: &SmolStr,
    ) -> Result<&crate::scene_ir::FieldScene, QueryExecError> {
        self.ctx
            .scene
            .fields
            .get(field)
            .ok_or_else(|| QueryExecError::MissingField {
                name: field.clone(),
            })
    }

    pub(crate) fn shape_scene(
        &self,
        shape: &SmolStr,
    ) -> Result<&crate::scene_ir::ShapeScene, QueryExecError> {
        self.ctx
            .scene
            .shapes
            .get(shape)
            .ok_or_else(|| QueryExecError::MissingShape {
                name: shape.clone(),
            })
    }

    pub(crate) fn eval_shape_winner(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<ShapeWinner, QueryExecError> {
        let scene = self.shape_scene(shape)?;
        self.eval_shape_winner_node(shape, &scene.root, scene.provenance.as_ref(), point)
    }

    pub(crate) fn eval_shape_winner_node(
        &self,
        scene_name: &SmolStr,
        node: &ShapeNode,
        provenance: Option<&ShapeProvenanceExpr>,
        point: [f32; 3],
    ) -> Result<ShapeWinner, QueryExecError> {
        self.note_branch_visit();
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_winner_node(target, &scene.root, scene.provenance.as_ref(), point)
            }
            ShapeNode::Leaf(leaf) => {
                self.note_shape_leaf_visit();
                Ok(ShapeWinner {
                    distance: self.eval_field_distance(&leaf.field, point)?,
                    feature_id: leaf.feature_id,
                    leaf: Some(ShapeLeafRef {
                        scene: scene_name.clone(),
                        leaf: leaf.id,
                    }),
                })
            }
            ShapeNode::Union { items } => {
                let merge_policy = match provenance {
                    Some(ShapeProvenanceExpr::Union { provenance, .. }) => *provenance,
                    _ => ShapeMergeProvenancePolicy::Nearest,
                };
                let provenance_items = match provenance {
                    Some(ShapeProvenanceExpr::Union { items, .. }) => Some(items.as_slice()),
                    _ => None,
                };
                let mut iter = items.iter().enumerate();
                let Some((idx, first)) = iter.next() else {
                    return Ok(default_shape_winner());
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(idx)),
                    point,
                )?;
                for (idx, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(idx)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = runtime_binary_f32(
                                current.distance,
                                next.distance,
                                wr_field_union,
                            )?;
                        }
                        ShapeMergeProvenancePolicy::Nearest => {
                            if next.distance < current.distance {
                                current = next;
                            }
                        }
                    }
                }
                Ok(current)
            }
            ShapeNode::Intersection { items } => {
                let merge_policy = match provenance {
                    Some(ShapeProvenanceExpr::Intersection { provenance, .. }) => *provenance,
                    _ => ShapeMergeProvenancePolicy::Nearest,
                };
                let provenance_items = match provenance {
                    Some(ShapeProvenanceExpr::Intersection { items, .. }) => Some(items.as_slice()),
                    _ => None,
                };
                let mut iter = items.iter().enumerate();
                let Some((idx, first)) = iter.next() else {
                    return Ok(default_shape_winner());
                };
                let mut current = self.eval_shape_winner_node(
                    scene_name,
                    first,
                    provenance_items.and_then(|items| items.get(idx)),
                    point,
                )?;
                for (idx, item) in iter {
                    let next = self.eval_shape_winner_node(
                        scene_name,
                        item,
                        provenance_items.and_then(|items| items.get(idx)),
                        point,
                    )?;
                    match merge_policy {
                        ShapeMergeProvenancePolicy::Ordered => {
                            current.distance = runtime_binary_f32(
                                current.distance,
                                next.distance,
                                wr_field_intersection,
                            )?;
                        }
                        ShapeMergeProvenancePolicy::Nearest => {
                            if next.distance > current.distance {
                                current = next;
                            }
                        }
                    }
                }
                Ok(current)
            }
            ShapeNode::Subtract { left, right } => {
                let (subtract_policy, left_provenance, right_provenance) = match provenance {
                    Some(ShapeProvenanceExpr::Subtract {
                        provenance,
                        left,
                        right,
                    }) => (*provenance, Some(left.as_ref()), Some(right.as_ref())),
                    _ => (ShapeSubtractProvenancePolicy::Left, None, None),
                };
                let left = self.eval_shape_winner_node(scene_name, left, left_provenance, point)?;
                let right =
                    self.eval_shape_winner_node(scene_name, right, right_provenance, point)?;
                let neg_right = -right.distance;
                if left.distance >= neg_right {
                    Ok(left)
                } else {
                    Ok(ShapeWinner {
                        distance: neg_right,
                        feature_id: match subtract_policy {
                            ShapeSubtractProvenancePolicy::Left => left.feature_id,
                            ShapeSubtractProvenancePolicy::Right => right.feature_id,
                        },
                        leaf: match subtract_policy {
                            ShapeSubtractProvenancePolicy::Left => left.leaf,
                            ShapeSubtractProvenancePolicy::Right => right.leaf,
                        },
                    })
                }
            }
        }
    }

    pub(crate) fn eval_field_local_frame<'scene>(
        &'scene self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<FieldLocalFrame<'scene>, QueryExecError> {
        let scene = self.field_scene(field)?;
        self.eval_field_local_frame_node(field.clone(), &scene.root, point, 0, 0)
    }

    pub(crate) fn eval_field_local_frame_node<'scene>(
        &'scene self,
        field_name: SmolStr,
        node: &'scene FieldNode,
        point: [f32; 3],
        instance_id: u32,
        repeat_id: u32,
    ) -> Result<FieldLocalFrame<'scene>, QueryExecError> {
        self.note_acceleration_node_visit();
        match node {
            FieldNode::Use { target } => {
                let scene = self.field_scene(target)?;
                self.eval_field_local_frame_node(
                    target.clone(),
                    &scene.root,
                    point,
                    instance_id,
                    repeat_id,
                )
            }
            FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_local_frame_node(
                        field_name,
                        inner,
                        point,
                        instance_id,
                        repeat_id,
                    );
                };
                let local_point = self.eval_wrapped_point(*kind, param, point)?;
                self.eval_field_local_frame_node(
                    field_name,
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                )
            }
            FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param else {
                    return self.eval_field_local_frame_node(
                        field_name,
                        inner,
                        point,
                        instance_id,
                        repeat_id,
                    );
                };
                self.note_repeat_cell_skip();
                let component = self.eval_repeat_identity(*kind, param, point)?;
                let local_point = self.eval_repeat_point(*kind, param, point)?;
                let (next_instance_id, next_repeat_id) = match kind {
                    RepeatKind::InstanceArray => {
                        (chain_identity_component(instance_id, component), repeat_id)
                    }
                    _ => (instance_id, chain_identity_component(repeat_id, component)),
                };
                self.eval_field_local_frame_node(
                    field_name,
                    inner,
                    local_point,
                    next_instance_id,
                    next_repeat_id,
                )
            }
            _ => Ok(FieldLocalFrame {
                field_name,
                node,
                point,
                instance_id,
                repeat_id,
            }),
        }
    }

    pub(crate) fn eval_field_local_normal(
        &self,
        field: &SmolStr,
        point: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        let frame = self.eval_field_local_frame(field, point)?;
        let eps = 0.001f32;
        let sample = |sample_point: [f32; 3]| match frame.node {
            FieldNode::OpaqueLeaf => {
                self.eval_opaque_field_distance(&frame.field_name, sample_point)
            }
            _ => self.eval_field_node(frame.node, sample_point),
        };
        let dx = sample([frame.point[0] + eps, frame.point[1], frame.point[2]])?
            - sample([frame.point[0] - eps, frame.point[1], frame.point[2]])?;
        let dy = sample([frame.point[0], frame.point[1] + eps, frame.point[2]])?
            - sample([frame.point[0], frame.point[1] - eps, frame.point[2]])?;
        let dz = sample([frame.point[0], frame.point[1], frame.point[2] + eps])?
            - sample([frame.point[0], frame.point[1], frame.point[2] - eps])?;
        Ok(normalize3([dx, dy, dz]))
    }

    pub(crate) fn eval_shape_radiance_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<[f32; 3], QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_radiance_node(&scene.root, point, direction)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(radiance) = &leaf.radiance else {
                    return Ok([0.0, 0.0, 0.0]);
                };
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let value = self.execute_portable_function(
                    radiance,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::Vec3(direction),
                        KernelValue::U32(leaf.feature_id),
                    ],
                )?;
                expect_vec3(Some(&value), "radiance")
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = [0.0, 0.0, 0.0];
                for item in items {
                    total = add3(
                        total,
                        self.eval_shape_radiance_node(item, point, direction)?,
                    );
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => Ok(add3(
                self.eval_shape_radiance_node(left, point, direction)?,
                self.eval_shape_radiance_node(right, point, direction)?,
            )),
        }
    }

    pub(crate) fn eval_shape_medium_node(
        &self,
        node: &ShapeNode,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        match node {
            ShapeNode::Use { target } => {
                let scene = self.shape_scene(target)?;
                self.eval_shape_medium_node(&scene.root, point)
            }
            ShapeNode::Leaf(leaf) => {
                let Some(volume) = &leaf.volume else {
                    return Ok(default_medium());
                };
                let local_frame = self.eval_field_local_frame(&leaf.field, point)?;
                let local_surface_distance =
                    self.eval_field_node(local_frame.node, local_frame.point)?;
                self.execute_portable_function(
                    volume,
                    vec![
                        KernelValue::Vec3(local_frame.point),
                        KernelValue::F32(local_surface_distance),
                    ],
                )
            }
            ShapeNode::Union { items } | ShapeNode::Intersection { items } => {
                let mut total = default_medium();
                for item in items {
                    total =
                        combine_medium_values(total, self.eval_shape_medium_node(item, point)?)?;
                }
                Ok(total)
            }
            ShapeNode::Subtract { left, right } => combine_medium_values(
                self.eval_shape_medium_node(left, point)?,
                self.eval_shape_medium_node(right, point)?,
            ),
        }
    }
}
