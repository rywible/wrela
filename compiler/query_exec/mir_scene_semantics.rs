impl FunctionLowerer {
    pub(crate) fn build_default_actor_handle(&mut self, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("ActorHandle");
        Self::set_class_field_value(&mut class, "id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "generation", Value::Const(Literal::Integer(0)));
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_default_payload(&mut self, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("Payload");
        Self::set_class_field_value(&mut class, "entity_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "material_id", Value::Const(Literal::Integer(0)));
        let actor = self.build_default_actor_handle(span);
        Self::set_class_field_value(&mut class, "actor", actor);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_default_surface(&mut self, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let black = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), zero.clone(), zero.clone()],
            span,
        );
        let mut class = self.synthetic_class_target_info("Surface");
        for field in [
            "roughness",
            "metalness",
            "clearcoat",
            "clearcoat_roughness",
            "sheen",
        ] {
            Self::set_class_field_value(&mut class, field, zero.clone());
        }
        Self::set_class_field_value(&mut class, "albedo", black.clone());
        Self::set_class_field_value(&mut class, "emissive", black);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_default_medium(&mut self, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let black = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), zero.clone(), zero.clone()],
            span,
        );
        let mut class = self.synthetic_class_target_info("Medium");
        Self::set_class_field_value(&mut class, "density", zero.clone());
        Self::set_class_field_value(&mut class, "emission", black);
        Self::set_class_field_value(&mut class, "anisotropy", zero);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_default_hit(&mut self, origin: Value, span: TextRange) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let mut class = self.synthetic_class_target_info("Hit3");
        Self::set_class_field_value(&mut class, "hit", Value::Const(Literal::Boolean(false)));
        Self::set_class_field_value(&mut class, "distance", zero.clone());
        Self::set_class_field_value(&mut class, "position", origin.clone());
        Self::set_class_field_value(&mut class, "local_position", origin.clone());
        let normal = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![
                zero.clone(),
                zero.clone(),
                Value::Const(Literal::Float(1.0)),
            ],
            span,
        );
        Self::set_class_field_value(&mut class, "normal", normal.clone());
        Self::set_class_field_value(&mut class, "local_normal", normal.clone());
        let shading_frame = self.lower_stable_surface_frame(origin.clone(), normal, span);
        Self::set_class_field_value(&mut class, "shading_frame", shading_frame);
        Self::set_class_field_value(&mut class, "steps", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "feature_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "instance_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(&mut class, "repeat_id", Value::Const(Literal::Integer(0)));
        Self::set_class_field_value(
            &mut class,
            "root_shape_id",
            Value::Const(Literal::Integer(0)),
        );
        let payload = self.build_default_payload(span);
        Self::set_class_field_value(&mut class, "payload", payload);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_scene_capture_value(&mut self, shape_name: &SmolStr, span: TextRange) -> Value {
        let is_field = self.field_names.contains(shape_name);
        let is_shape = self.shape_names.contains(shape_name);
        let snapshot = if is_field {
            crate::query_exec::ids::stable_field_snapshot_handle(shape_name)
        } else if is_shape {
            crate::query_exec::ids::stable_shape_snapshot_handle(shape_name)
        } else {
            crate::query_exec::ids::stable_region_snapshot_handle(shape_name)
        };
        let mut class = self.synthetic_class_target_info(if is_field {
            "FieldCapture"
        } else if is_shape {
            "ShapeCapture"
        } else {
            "RegionCapture"
        });
        Self::set_class_field_value(
            &mut class,
            "scene_id",
            Value::Const(Literal::Integer(i64::from(snapshot.portable_scene_id()))),
        );
        Self::set_class_field_value(
            &mut class,
            "epoch",
            Value::Const(Literal::Integer(i64::from(snapshot.portable_epoch()))),
        );
        Self::set_class_field_value(
            &mut class,
            "root_feature_id",
            Value::Const(Literal::Integer(i64::from(
                snapshot.portable_root_feature_id(),
            ))),
        );
        self.build_class_instance(&class, span)
    }

    pub(crate) fn build_dispatch_backend_value(&mut self, mode: i64, span: TextRange) -> Value {
        let mut class = self.synthetic_class_target_info("DispatchBackend");
        Self::set_class_field_value(&mut class, "id", Value::Const(Literal::Integer(mode)));
        self.build_class_instance(&class, span)
    }

    pub(crate) fn lower_dispatch_backend_id(&mut self, backend: Value, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::GetField {
                base: backend,
                field: SmolStr::new("id"),
                slot: self.field_slot("DispatchBackend", "id"),
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_shape_payload_body_value(&mut self, payload: &hir::Body, span: TextRange) -> Value {
        if payload.root_stmts.is_empty() {
            return self.build_default_payload(span);
        }
        if payload.root_stmts.len() > 1 {
            self.lower_stmt_block(payload, &payload.root_stmts[..payload.root_stmts.len() - 1]);
        }
        let last = *payload.root_stmts.last().expect("shape payload stmt");
        match &payload.stmts[last] {
            HirStmt::Expr(expr) => self.lower_expr(payload, *expr),
            HirStmt::Return(Some(expr)) => self.lower_expr(payload, *expr),
            _ => {
                self.lower_stmt(payload, last);
                self.build_default_payload(span)
            }
        }
    }

    pub(crate) fn field_root_expr(&self, field_name: &SmolStr) -> Option<hir::FieldExpr> {
        self.field_graphs
            .get(field_name)
            .map(|graph| graph.root.clone())
    }

    pub(crate) fn field_body(&self, field_name: &SmolStr) -> Option<&hir::Body> {
        self.field_bodies.get(field_name)
    }

    pub(crate) fn field_root_is_opaque_custom(&self, field_name: &SmolStr) -> bool {
        matches!(
            self.field_graphs.get(field_name).map(|graph| &graph.root),
            Some(hir::FieldExpr::Custom { .. })
        )
    }

    pub(crate) fn unprunable_support_lower_bound(&self) -> Value {
        Value::Const(Literal::Float(-1_000_000.0))
    }

    pub(crate) fn lower_field_support_lower_bound_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let Some(scene) = self.field_scene(field).cloned() else {
            return self.unprunable_support_lower_bound();
        };
        if scene.opaque_boundary
            || matches!(scene.semantics, scene_ir::DistanceSemantics::UnknownOpaque)
            || self.field_root_is_opaque_custom(field)
        {
            return self.unprunable_support_lower_bound();
        }
        if let Some(bounds) = scene.authored_bounds.as_ref() {
            let bounds = self.lower_scene_value_expr(bounds, span);
            return self.lower_bounds_support_lower_bound_value(point, bounds, span);
        }
        if !scene.can_coarse_support_pruning {
            return self.unprunable_support_lower_bound();
        }
        self.lower_field_support_lower_bound_scene(&scene.root, point, span)
    }

    pub(crate) fn lower_field_support_lower_bound_scene(
        &mut self,
        node: &scene_ir::FieldNode,
        point: Value,
        span: TextRange,
    ) -> Value {
        match node {
            scene_ir::FieldNode::Use { target } => {
                self.lower_field_support_lower_bound_call(target, point, span)
            }
            scene_ir::FieldNode::Primitive { primitive, args } => {
                let Some(args) = args.as_ref() else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_field_primitive_support_lower_bound_scene(*primitive, args, point, span)
            }
            scene_ir::FieldNode::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_scene(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_field_support_lower_bound_scene(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::FieldNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_scene(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_field_support_lower_bound_scene(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::FieldNode::Subtract { left, .. } => {
                self.lower_field_support_lower_bound_scene(left, point, span)
            }
            scene_ir::FieldNode::Transform { kind, param, inner } => match kind {
                scene_ir::TransformKind::Translate => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    if !self
                        .scene_value_terminal_callee_name(param)
                        .is_some_and(|callee| callee.as_str() == "vec3")
                    {
                        return self.unprunable_support_lower_bound();
                    }
                    let local_point =
                        self.lower_scene_wrapped_support_point("translate", param, point, span);
                    self.lower_field_support_lower_bound_scene(inner, local_point, span)
                }
                scene_ir::TransformKind::Rotate => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    let local_point = self.lower_scene_wrapped_support_point(
                        "field_rotate_point",
                        param,
                        point,
                        span,
                    );
                    self.lower_field_support_lower_bound_scene(inner, local_point, span)
                }
                scene_ir::TransformKind::UniformScale => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    let scale = self.lower_scene_value_expr(param, span);
                    let local_point = self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("uniform_scale"),
                        vec![scale.clone(), point],
                        span,
                    );
                    let child =
                        self.lower_field_support_lower_bound_scene(inner, local_point, span);
                    let abs_scale = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("abs"),
                        vec![scale],
                        span,
                    );
                    self.lower_binary_temp(MirType::Float, BinaryOp::Mul, child, abs_scale, span)
                }
                scene_ir::TransformKind::AffineTransform
                | scene_ir::TransformKind::Warp
                | scene_ir::TransformKind::Bend
                | scene_ir::TransformKind::Twist
                | scene_ir::TransformKind::Taper
                | scene_ir::TransformKind::Displace => self.unprunable_support_lower_bound(),
            },
            scene_ir::FieldNode::Repeat { kind, param, inner } => match kind {
                scene_ir::RepeatKind::RepeatLinear => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    let local_point =
                        self.lower_scene_wrapped_support_point("repeat_linear", param, point, span);
                    self.lower_field_support_lower_bound_scene(inner, local_point, span)
                }
                scene_ir::RepeatKind::RepeatGrid => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    let local_point =
                        self.lower_scene_wrapped_support_point("repeat_grid", param, point, span);
                    self.lower_field_support_lower_bound_scene(inner, local_point, span)
                }
                scene_ir::RepeatKind::MirrorArray => {
                    let Some(param) = param.as_ref() else {
                        return self.unprunable_support_lower_bound();
                    };
                    let local_point =
                        self.lower_scene_wrapped_support_point("mirror_array", param, point, span);
                    self.lower_field_support_lower_bound_scene(inner, local_point, span)
                }
                scene_ir::RepeatKind::RadialRepeat | scene_ir::RepeatKind::InstanceArray => {
                    self.unprunable_support_lower_bound()
                }
            },
            scene_ir::FieldNode::Smooth { .. } => self.unprunable_support_lower_bound(),
            scene_ir::FieldNode::Extrude { height, profile } => {
                let (Some(height), Some(profile)) = (height.as_ref(), profile.as_ref()) else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(bounds4) = self.lower_scene_profile_bounds4(profile, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let height_value = self.lower_scene_value_expr(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_z = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_z = self.lower_vec_component_value(bounds4, 3, span);
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            scene_ir::FieldNode::Revolve { profile } => {
                let Some(profile) = profile.as_ref() else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(bounds4) = self.lower_scene_profile_bounds4(profile, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_y = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_y = self.lower_vec_component_value(bounds4, 3, span);
                let abs_min_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_x], span);
                let abs_max_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_x], span);
                let radial = self.lower_scalar_max(abs_min_x, abs_max_x, span);
                let neg_radial = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radial.clone(),
                    span,
                );
                let min = self.lower_vec3_value(neg_radial.clone(), min_y, neg_radial, span);
                let max = self.lower_vec3_value(radial.clone(), max_y, radial, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            scene_ir::FieldNode::Sweep { path, profile } => {
                let (Some(path), Some(profile)) = (path.as_ref(), profile.as_ref()) else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(bounds4) = self.lower_scene_profile_bounds4(profile, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let path_value = self.lower_scene_value_expr(path, span);
                let abs_path = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("abs"),
                    vec![path_value],
                    span,
                );
                let half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Mul,
                    abs_path,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let radius = self.lower_profile_radius_from_bounds4(bounds4, span);
                let radius_vec = self.lower_vec3_splat(radius, span);
                let zero_vec = self.lower_vec3_value(
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    span,
                );
                let neg_half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    zero_vec,
                    half_path.clone(),
                    span,
                );
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    neg_half_path,
                    radius_vec.clone(),
                    span,
                );
                let max = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Add,
                    half_path,
                    radius_vec,
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            scene_ir::FieldNode::Loft { height, from, to } => {
                let (Some(height), Some(from), Some(to)) =
                    (height.as_ref(), from.as_ref(), to.as_ref())
                else {
                    return self.unprunable_support_lower_bound();
                };
                let (Some(from_bounds4), Some(to_bounds4)) = (
                    self.lower_scene_profile_bounds4(from, span),
                    self.lower_scene_profile_bounds4(to, span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let from_min_x = self.lower_vec_component_value(from_bounds4.clone(), 0, span);
                let from_min_z = self.lower_vec_component_value(from_bounds4.clone(), 1, span);
                let from_max_x = self.lower_vec_component_value(from_bounds4.clone(), 2, span);
                let from_max_z = self.lower_vec_component_value(from_bounds4, 3, span);
                let to_min_x = self.lower_vec_component_value(to_bounds4.clone(), 0, span);
                let to_min_z = self.lower_vec_component_value(to_bounds4.clone(), 1, span);
                let to_max_x = self.lower_vec_component_value(to_bounds4.clone(), 2, span);
                let to_max_z = self.lower_vec_component_value(to_bounds4, 3, span);
                let min_x = self.lower_scalar_min(from_min_x, to_min_x, span);
                let min_z = self.lower_scalar_min(from_min_z, to_min_z, span);
                let max_x = self.lower_scalar_max(from_max_x, to_max_x, span);
                let max_z = self.lower_scalar_max(from_max_z, to_max_z, span);
                let height_value = self.lower_scene_value_expr(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            scene_ir::FieldNode::OpaqueLeaf => self.unprunable_support_lower_bound(),
        }
    }

    pub(crate) fn field_metadata_can_coarse_support_prune(metadata: &hir::FieldMetadata) -> bool {
        metadata.trace.can_coarse_support_pruning
    }

    pub(crate) fn lower_field_authored_bounds(
        &mut self,
        metadata: &hir::FieldMetadata,
        span: TextRange,
    ) -> Option<Value> {
        if !matches!(metadata.trace.support, FieldSupport::Bounded)
            || !matches!(metadata.trace.bounds, FieldBounds::Bounded)
        {
            return None;
        }
        if let Some(bounds) = metadata.authored_bounds.as_ref() {
            return Some(self.lower_wrapped_body_value(bounds, span));
        }
        metadata.authored_support.as_ref().map(|support| {
            let support_value = self.lower_wrapped_body_value(support, span);
            self.lower_get_named_field(
                support_value,
                "Support3",
                "bounds",
                MirType::Named(SmolStr::new("Bounds3")),
                span,
            )
        })
    }

    pub(crate) fn lower_bounds_support_lower_bound_value(
        &mut self,
        point: Value,
        bounds: Value,
        span: TextRange,
    ) -> Value {
        let min = self.lower_get_named_field(bounds.clone(), "Bounds3", "min", MirType::Vec3, span);
        let max = self.lower_get_named_field(bounds, "Bounds3", "max", MirType::Vec3, span);
        self.lower_bounds_box_support_lower_bound(point, min, max, span)
    }

    pub(crate) fn lower_field_support_lower_bound_expr(
        &mut self,
        expr: &hir::FieldExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::FieldExpr::Use { target } => {
                self.lower_field_support_lower_bound_call(target, point, span)
            }
            hir::FieldExpr::Primitive { primitive, args } => {
                self.lower_field_primitive_support_lower_bound(*primitive, args, body, point, span)
            }
            hir::FieldExpr::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_support_lower_bound_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_field_support_lower_bound_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_support_lower_bound_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Subtract { left, .. } => {
                self.lower_field_support_lower_bound_expr(left, body, point, span)
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                if !self.field_wrapper_body_returns_named_call(translate, "vec3") {
                    return self.unprunable_support_lower_bound();
                }
                let local_point =
                    self.lower_wrapped_support_point("translate", "offset", translate, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_rotate_point",
                    "rotation",
                    rotate,
                    point,
                    span,
                );
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let wrapper_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("uniform_scale"),
                    vec![wrapper_value.clone(), point],
                    span,
                );
                let child =
                    self.lower_field_support_lower_bound_expr(inner, body, local_point, span);
                let abs_scale = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![wrapper_value],
                    span,
                );
                self.lower_binary_temp(MirType::Float, BinaryOp::Mul, child, abs_scale, span)
            }
            hir::FieldExpr::AffineTransform { .. }
            | hir::FieldExpr::Warp { .. }
            | hir::FieldExpr::RadialRepeat { .. }
            | hir::FieldExpr::InstanceArray { .. }
            | hir::FieldExpr::SmoothUnion { .. }
            | hir::FieldExpr::SmoothIntersection { .. }
            | hir::FieldExpr::SmoothSubtract { .. }
            | hir::FieldExpr::Bend { .. }
            | hir::FieldExpr::Twist { .. }
            | hir::FieldExpr::Taper { .. }
            | hir::FieldExpr::Displace { .. } => self.unprunable_support_lower_bound(),
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "repeat_linear",
                    "period",
                    repeat,
                    point,
                    span,
                );
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("repeat_grid", "period", repeat, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("mirror_array", "mirror", mirror, point, span);
                self.lower_field_support_lower_bound_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Extrude { height, profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let height_value = self.lower_wrapped_body_value(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_z = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_z = self.lower_vec_component_value(bounds4, 3, span);
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Revolve { profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
                let min_y = self.lower_vec_component_value(bounds4.clone(), 1, span);
                let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
                let max_y = self.lower_vec_component_value(bounds4, 3, span);
                let abs_min_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_x], span);
                let abs_max_x =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_x], span);
                let radial = self.lower_scalar_max(abs_min_x, abs_max_x, span);
                let neg_radial = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radial.clone(),
                    span,
                );
                let min = self.lower_vec3_value(neg_radial.clone(), min_y, neg_radial, span);
                let max = self.lower_vec3_value(radial.clone(), max_y, radial, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Sweep { path, profile } => {
                let Some(bounds4) = self.lower_profile_bounds4(profile, body, span) else {
                    return self.unprunable_support_lower_bound();
                };
                let path_value = self.lower_wrapped_body_value(path, span);
                let abs_path = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("abs"),
                    vec![path_value],
                    span,
                );
                let half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Mul,
                    abs_path,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let radius = self.lower_profile_radius_from_bounds4(bounds4, span);
                let radius_vec = self.lower_vec3_splat(radius, span);
                let zero_vec = self.lower_vec3_value(
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    span,
                );
                let neg_half_path = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    zero_vec,
                    half_path.clone(),
                    span,
                );
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    neg_half_path,
                    radius_vec.clone(),
                    span,
                );
                let max = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Add,
                    half_path,
                    radius_vec,
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Loft { height, from, to } => {
                let (Some(from_bounds4), Some(to_bounds4)) = (
                    self.lower_profile_bounds4(from, body, span),
                    self.lower_profile_bounds4(to, body, span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let from_min_x = self.lower_vec_component_value(from_bounds4.clone(), 0, span);
                let from_min_z = self.lower_vec_component_value(from_bounds4.clone(), 1, span);
                let from_max_x = self.lower_vec_component_value(from_bounds4.clone(), 2, span);
                let from_max_z = self.lower_vec_component_value(from_bounds4, 3, span);
                let to_min_x = self.lower_vec_component_value(to_bounds4.clone(), 0, span);
                let to_min_z = self.lower_vec_component_value(to_bounds4.clone(), 1, span);
                let to_max_x = self.lower_vec_component_value(to_bounds4.clone(), 2, span);
                let to_max_z = self.lower_vec_component_value(to_bounds4, 3, span);
                let min_x = self.lower_scalar_min(from_min_x, to_min_x, span);
                let min_z = self.lower_scalar_min(from_min_z, to_min_z, span);
                let max_x = self.lower_scalar_max(from_max_x, to_max_x, span);
                let max_z = self.lower_scalar_max(from_max_z, to_max_z, span);
                let height_value = self.lower_wrapped_body_value(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let min_y = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_vec3_value(min_x, min_y, min_z, span);
                let max = self.lower_vec3_value(max_x, half_height, max_z, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldExpr::Custom { .. } => self.unprunable_support_lower_bound(),
        }
    }

    pub(crate) fn lower_profile_bounds4(
        &mut self,
        profile: &hir::ProfileExpr,
        body: &hir::Body,
        span: TextRange,
    ) -> Option<Value> {
        match profile {
            hir::ProfileExpr::Primitive { primitive, args } => match primitive {
                hir::ProfilePrimitive::Circle2 => {
                    let radius = self.lower_field_named_arg_value(args, body, "radius")?;
                    let neg_radius = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        radius.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(
                        neg_radius.clone(),
                        neg_radius,
                        radius.clone(),
                        radius,
                        span,
                    ))
                }
                hir::ProfilePrimitive::Rect2 => {
                    let half = self.lower_field_named_arg_value(args, body, "half")?;
                    let half_x = self.lower_vec_component_value(half.clone(), 0, span);
                    let half_y = self.lower_vec_component_value(half, 1, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_x.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_y.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(min_x, min_y, half_x, half_y, span))
                }
                hir::ProfilePrimitive::RoundedRect2 => {
                    let half = self.lower_field_named_arg_value(args, body, "half")?;
                    let radius = self.lower_field_named_arg_value(args, body, "radius")?;
                    let half_x = self.lower_vec_component_value(half.clone(), 0, span);
                    let half_y = self.lower_vec_component_value(half, 1, span);
                    let outer_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        half_x.clone(),
                        radius.clone(),
                        span,
                    );
                    let outer_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        half_y.clone(),
                        radius,
                        span,
                    );
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        outer_x.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        outer_y.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(min_x, min_y, outer_x, outer_y, span))
                }
                hir::ProfilePrimitive::Capsule2 => {
                    let (a, b, radius) = (
                        self.lower_field_named_arg_value(args, body, "a")?,
                        self.lower_field_named_arg_value(args, body, "b")?,
                        self.lower_field_named_arg_value(args, body, "radius")?,
                    );
                    let a_x = self.lower_vec_component_value(a.clone(), 0, span);
                    let a_y = self.lower_vec_component_value(a, 1, span);
                    let b_x = self.lower_vec_component_value(b.clone(), 0, span);
                    let b_y = self.lower_vec_component_value(b, 1, span);
                    let min_x = self.lower_scalar_min(a_x.clone(), b_x.clone(), span);
                    let min_y = self.lower_scalar_min(a_y.clone(), b_y.clone(), span);
                    let max_x = self.lower_scalar_max(a_x, b_x, span);
                    let max_y = self.lower_scalar_max(a_y, b_y, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        min_x,
                        radius.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        min_y,
                        radius.clone(),
                        span,
                    );
                    let max_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Add,
                        max_x,
                        radius.clone(),
                        span,
                    );
                    let max_y =
                        self.lower_binary_temp(MirType::Float, BinaryOp::Add, max_y, radius, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Segment2 => {
                    let (a, b) = (
                        self.lower_field_named_arg_value(args, body, "a")?,
                        self.lower_field_named_arg_value(args, body, "b")?,
                    );
                    let a_x = self.lower_vec_component_value(a.clone(), 0, span);
                    let a_y = self.lower_vec_component_value(a, 1, span);
                    let b_x = self.lower_vec_component_value(b.clone(), 0, span);
                    let b_y = self.lower_vec_component_value(b, 1, span);
                    let min_x = self.lower_scalar_min(a_x.clone(), b_x.clone(), span);
                    let min_y = self.lower_scalar_min(a_y.clone(), b_y.clone(), span);
                    let max_x = self.lower_scalar_max(a_x, b_x, span);
                    let max_y = self.lower_scalar_max(a_y, b_y, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Polygon2 | hir::ProfilePrimitive::Polyline2 => {
                    let vertices = self.lower_field_named_arg_value(args, body, "vertices")?;
                    Some(self.lower_call_temp(
                        MirType::Vec4,
                        SmolStr::new("field_profile_vertices_bounds4"),
                        vec![vertices],
                        span,
                    ))
                }
            },
        }
    }

    pub(crate) fn lower_profile_radius_from_bounds4(&mut self, bounds4: Value, span: TextRange) -> Value {
        let min_x = self.lower_vec_component_value(bounds4.clone(), 0, span);
        let min_y = self.lower_vec_component_value(bounds4.clone(), 1, span);
        let max_x = self.lower_vec_component_value(bounds4.clone(), 2, span);
        let max_y = self.lower_vec_component_value(bounds4, 3, span);
        let abs_min_x =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_x], span);
        let abs_min_y =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![min_y], span);
        let abs_max_x =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_x], span);
        let abs_max_y =
            self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![max_y], span);
        let radius_x = self.lower_scalar_max(abs_min_x, abs_max_x, span);
        let radius_y = self.lower_scalar_max(abs_min_y, abs_max_y, span);
        self.lower_scalar_max(radius_x, radius_y, span)
    }

    pub(crate) fn lower_wrapped_support_point(
        &mut self,
        callee_name: &str,
        _arg_name: &str,
        wrapped: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        let wrapper_value = self.lower_wrapped_body_value(wrapped, span);
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new(callee_name),
            vec![wrapper_value, point],
            span,
        )
    }

    pub(crate) fn scene_value_mir_type(&self, expr: &scene_ir::SceneValueExpr) -> MirType {
        match expr {
            scene_ir::SceneValueExpr::Literal(literal) => match literal {
                Literal::Integer(_) => MirType::Integer,
                Literal::Float(_) => MirType::Float,
                Literal::String(_) => MirType::String,
                Literal::Boolean(_) => MirType::Boolean,
                Literal::Nil => MirType::Nil,
            },
            scene_ir::SceneValueExpr::List(_) => MirType::Named(SmolStr::new("List")),
            scene_ir::SceneValueExpr::Unary { expr, .. } => self.scene_value_mir_type(expr),
            scene_ir::SceneValueExpr::Binary { op, lhs, .. } => match op {
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Le
                | BinaryOp::Ge
                | BinaryOp::And
                | BinaryOp::Or => MirType::Boolean,
                _ => self.scene_value_mir_type(lhs),
            },
            scene_ir::SceneValueExpr::Call { callee, .. } => match callee.as_str() {
                "vec2" => MirType::Vec2,
                "vec3" => MirType::Vec3,
                "vec4" => MirType::Vec4,
                "quat" => MirType::Quat,
                "mat3_identity" | "mat3_cols" => MirType::Mat3,
                "mat4_identity" | "mat4_cols" => MirType::Mat4,
                _ if self.type_tags.contains_key(callee) => MirType::Named(callee.clone()),
                _ if builtin_record_by_function(callee.as_str()).is_some() => {
                    MirType::Named(callee.clone())
                }
                _ => MirType::Unknown,
            },
        }
    }

    pub(crate) fn scene_value_terminal_callee_name(&self, expr: &scene_ir::SceneValueExpr) -> Option<SmolStr> {
        match expr {
            scene_ir::SceneValueExpr::Call { callee, .. } => Some(callee.clone()),
            _ => None,
        }
    }

    pub(crate) fn lower_scene_value_expr(
        &mut self,
        expr: &scene_ir::SceneValueExpr,
        span: TextRange,
    ) -> Value {
        match expr {
            scene_ir::SceneValueExpr::Literal(literal) => Value::Const(literal.clone()),
            scene_ir::SceneValueExpr::List(items) => {
                let temp = self.new_temp(MirType::Named(SmolStr::new("List")));
                let items = items
                    .iter()
                    .map(|item| self.lower_scene_value_expr(item, span))
                    .collect();
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items,
                        alloc: AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            scene_ir::SceneValueExpr::Unary { op, expr } => {
                let operand = self.lower_scene_value_expr(expr, span);
                self.lower_unary_temp(self.scene_value_mir_type(expr), *op, operand, span)
            }
            scene_ir::SceneValueExpr::Binary { lhs, op, rhs } => {
                let lhs_value = self.lower_scene_value_expr(lhs, span);
                let rhs_value = self.lower_scene_value_expr(rhs, span);
                self.lower_binary_temp(
                    self.scene_value_mir_type(expr),
                    *op,
                    lhs_value,
                    rhs_value,
                    span,
                )
            }
            scene_ir::SceneValueExpr::Call { callee, args } => {
                self.lower_scene_call_value(callee, args, span)
            }
        }
    }

    pub(crate) fn lower_scene_call_value(
        &mut self,
        callee: &SmolStr,
        args: &[scene_ir::SceneArgExpr],
        span: TextRange,
    ) -> Value {
        if self.type_tags.contains_key(callee) && self.class_fields.contains_key(callee) {
            let mut class = self.synthetic_class_target_info(callee.as_str());
            let mut positional_index = 0usize;
            for arg in args {
                match arg {
                    scene_ir::SceneArgExpr::Positional(value) => {
                        Self::set_class_field_value_at(
                            &mut class,
                            positional_index,
                            self.lower_scene_value_expr(value, span),
                        );
                        positional_index += 1;
                    }
                    scene_ir::SceneArgExpr::Named { name, value } => {
                        Self::set_class_field_value(
                            &mut class,
                            name.as_str(),
                            self.lower_scene_value_expr(value, span),
                        );
                    }
                }
            }
            return self.build_class_instance(&class, span);
        }

        let mut lowered_args = args
            .iter()
            .map(|arg| match arg {
                scene_ir::SceneArgExpr::Positional(value)
                | scene_ir::SceneArgExpr::Named { value, .. } => {
                    self.lower_scene_value_expr(value, span)
                }
            })
            .collect::<Vec<_>>();
        if matches!(
            callee.as_str(),
            "transform3_identity" | "compose_transform3" | "inverse_transform3"
        ) && let Some(class_id) = self.type_tags.get(&SmolStr::new("Transform3"))
        {
            lowered_args.insert(0, Value::Const(Literal::Integer(class_id.0 as i64)));
        }
        self.lower_call_temp(
            self.scene_value_mir_type(&scene_ir::SceneValueExpr::Call {
                callee: callee.clone(),
                args: args.to_vec(),
            }),
            callee.clone(),
            lowered_args,
            span,
        )
    }

    pub(crate) fn lower_scene_named_arg_value(
        &mut self,
        args: &[scene_ir::SceneArgExpr],
        name: &str,
        span: TextRange,
    ) -> Option<Value> {
        args.iter().find_map(|arg| match arg {
            scene_ir::SceneArgExpr::Named {
                name: arg_name,
                value,
            } if arg_name.as_str() == name => Some(self.lower_scene_value_expr(value, span)),
            _ => None,
        })
    }

    pub(crate) fn lower_scene_profile_bounds4(
        &mut self,
        profile: &scene_ir::SceneProfileExpr,
        span: TextRange,
    ) -> Option<Value> {
        match profile {
            scene_ir::SceneProfileExpr::Primitive { primitive, args } => match primitive {
                hir::ProfilePrimitive::Circle2 => {
                    let radius = self.lower_scene_named_arg_value(args, "radius", span)?;
                    let neg_radius = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        radius.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(
                        neg_radius.clone(),
                        neg_radius,
                        radius.clone(),
                        radius,
                        span,
                    ))
                }
                hir::ProfilePrimitive::Rect2 => {
                    let half = self.lower_scene_named_arg_value(args, "half", span)?;
                    let half_x = self.lower_vec_component_value(half.clone(), 0, span);
                    let half_y = self.lower_vec_component_value(half, 1, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_x.clone(),
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        half_y.clone(),
                        span,
                    );
                    Some(self.lower_vec4_value(min_x, min_y, half_x, half_y, span))
                }
                hir::ProfilePrimitive::RoundedRect2 => {
                    let half = self.lower_scene_named_arg_value(args, "half", span)?;
                    let radius = self.lower_scene_named_arg_value(args, "radius", span)?;
                    let radius_vec = self.lower_call_temp(
                        MirType::Vec2,
                        SmolStr::new("vec2"),
                        vec![radius.clone(), radius],
                        span,
                    );
                    let min = self.lower_binary_temp(
                        MirType::Vec2,
                        BinaryOp::Sub,
                        half.clone(),
                        radius_vec.clone(),
                        span,
                    );
                    let max = self.lower_binary_temp(
                        MirType::Vec2,
                        BinaryOp::Add,
                        half,
                        radius_vec,
                        span,
                    );
                    let min_x_component = self.lower_vec_component_value(min.clone(), 0, span);
                    let min_y_component = self.lower_vec_component_value(min, 1, span);
                    let min_x = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        min_x_component,
                        span,
                    );
                    let min_y = self.lower_binary_temp(
                        MirType::Float,
                        BinaryOp::Sub,
                        Value::Const(Literal::Float(0.0)),
                        min_y_component,
                        span,
                    );
                    let max_x = self.lower_vec_component_value(max.clone(), 0, span);
                    let max_y = self.lower_vec_component_value(max, 1, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Capsule2 => {
                    let a = self.lower_scene_named_arg_value(args, "a", span)?;
                    let b = self.lower_scene_named_arg_value(args, "b", span)?;
                    let radius = self.lower_scene_named_arg_value(args, "radius", span)?;
                    let radius_vec = self.lower_call_temp(
                        MirType::Vec2,
                        SmolStr::new("vec2"),
                        vec![radius.clone(), radius],
                        span,
                    );
                    let min_ab = self.lower_call_temp(
                        MirType::Vec2,
                        SmolStr::new("min"),
                        vec![a.clone(), b.clone()],
                        span,
                    );
                    let max_ab =
                        self.lower_call_temp(MirType::Vec2, SmolStr::new("max"), vec![a, b], span);
                    let min = self.lower_binary_temp(
                        MirType::Vec2,
                        BinaryOp::Sub,
                        min_ab,
                        radius_vec.clone(),
                        span,
                    );
                    let max = self.lower_binary_temp(
                        MirType::Vec2,
                        BinaryOp::Add,
                        max_ab,
                        radius_vec,
                        span,
                    );
                    let min_x = self.lower_vec_component_value(min.clone(), 0, span);
                    let min_y = self.lower_vec_component_value(min, 1, span);
                    let max_x = self.lower_vec_component_value(max.clone(), 0, span);
                    let max_y = self.lower_vec_component_value(max, 1, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Segment2 => {
                    let a = self.lower_scene_named_arg_value(args, "a", span)?;
                    let b = self.lower_scene_named_arg_value(args, "b", span)?;
                    let min = self.lower_call_temp(
                        MirType::Vec2,
                        SmolStr::new("min"),
                        vec![a.clone(), b.clone()],
                        span,
                    );
                    let max =
                        self.lower_call_temp(MirType::Vec2, SmolStr::new("max"), vec![a, b], span);
                    let min_x = self.lower_vec_component_value(min.clone(), 0, span);
                    let min_y = self.lower_vec_component_value(min, 1, span);
                    let max_x = self.lower_vec_component_value(max.clone(), 0, span);
                    let max_y = self.lower_vec_component_value(max, 1, span);
                    Some(self.lower_vec4_value(min_x, min_y, max_x, max_y, span))
                }
                hir::ProfilePrimitive::Polygon2 | hir::ProfilePrimitive::Polyline2 => {
                    let vertices = self.lower_scene_named_arg_value(args, "vertices", span)?;
                    Some(self.lower_call_temp(
                        MirType::Vec4,
                        SmolStr::new("field_profile_vertices_bounds4"),
                        vec![vertices],
                        span,
                    ))
                }
            },
        }
    }

    pub(crate) fn lower_scene_wrapped_support_point(
        &mut self,
        callee_name: &str,
        wrapped: &scene_ir::SceneValueExpr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let wrapper_value = self.lower_scene_value_expr(wrapped, span);
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new(callee_name),
            vec![wrapper_value, point],
            span,
        )
    }

    pub(crate) fn lower_wrapped_body_value(&mut self, body: &hir::Body, _span: TextRange) -> Value {
        if body.root_stmts.is_empty() {
            return Value::Const(Literal::Nil);
        }
        if body.root_stmts.len() > 1 {
            self.lower_stmt_block(body, &body.root_stmts[..body.root_stmts.len() - 1]);
        }
        let last = *body.root_stmts.last().expect("wrapped body stmt");
        match &body.stmts[last] {
            HirStmt::Expr(expr) => self.lower_expr(body, *expr),
            HirStmt::Return(Some(expr)) => self.lower_expr(body, *expr),
            _ => {
                self.lower_stmt(body, last);
                Value::Const(Literal::Nil)
            }
        }
    }

    pub(crate) fn field_wrapper_body_returns_named_call(&self, body: &hir::Body, name: &str) -> bool {
        self.field_wrapper_body_terminal_callee_name(body)
            .is_some_and(|callee_name| callee_name == name)
    }

    pub(crate) fn field_wrapper_body_terminal_callee_name(&self, body: &hir::Body) -> Option<SmolStr> {
        let Some(expr) = self.field_wrapper_body_terminal_expr(body) else {
            return None;
        };
        let Expr::Call { callee, .. } = &body.exprs[expr] else {
            return None;
        };
        match &body.exprs[*callee] {
            Expr::Variable(callee_name) => Some(callee_name.clone()),
            _ => None,
        }
    }

    pub(crate) fn field_wrapper_body_terminal_expr(&self, body: &hir::Body) -> Option<hir::Idx<Expr>> {
        let stmt = *body.root_stmts.last()?;
        match &body.stmts[stmt] {
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => Some(*expr),
            _ => None,
        }
    }

    pub(crate) fn lower_field_primitive_support_lower_bound(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[hir::Arg],
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let Some(radius) = self.lower_field_named_arg_value(args, body, "radius") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("sphere"),
                    vec![point, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Box => {
                let Some(half) = self.lower_field_named_arg_value(args, body, "half") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(MirType::Float, SmolStr::new("box"), vec![point, half], span)
            }
            hir::FieldPrimitive::Capsule => {
                let (Some(a), Some(b), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "a"),
                    self.lower_field_named_arg_value(args, body, "b"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let radius_vec = self.lower_vec3_splat(radius, span);
                let min_ab = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("min"),
                    vec![a.clone(), b.clone()],
                    span,
                );
                let max_ab =
                    self.lower_call_temp(MirType::Vec3, SmolStr::new("max"), vec![a, b], span);
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    min_ab,
                    radius_vec.clone(),
                    span,
                );
                let max =
                    self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, max_ab, radius_vec, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Cylinder => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_radius = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radius.clone(),
                    span,
                );
                let min_half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min_radius_z = min_radius.clone();
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_radius, min_half_height, min_radius_z],
                    span,
                );
                let radius_max = radius.clone();
                let half_height_max = half_height.clone();
                let radius_z = radius;
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![radius_max, half_height_max, radius_z],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Plane => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::Torus => {
                let (Some(major_radius), Some(minor_radius)) = (
                    self.lower_field_named_arg_value(args, body, "major_radius"),
                    self.lower_field_named_arg_value(args, body, "minor_radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Add,
                    major_radius.clone(),
                    minor_radius.clone(),
                    span,
                );
                let min_outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    outer.clone(),
                    span,
                );
                let min_minor = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    minor_radius.clone(),
                    span,
                );
                let min_outer_z = min_outer.clone();
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_outer, min_minor, min_outer_z],
                    span,
                );
                let max_outer_z = outer.clone();
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![outer, minor_radius, max_outer_z],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::RoundedBox => {
                let (Some(half), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("rounded_box"),
                    vec![point, half, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Ellipsoid => {
                let Some(radii) = self.lower_field_named_arg_value(args, body, "radii") else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("ellipsoid"),
                    vec![point, radii],
                    span,
                )
            }
            hir::FieldPrimitive::Cone => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cone"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::CappedCone => {
                let (Some(radius_bottom), Some(radius_top), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius_bottom"),
                    self.lower_field_named_arg_value(args, body, "radius_top"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capped_cone"),
                    vec![point, radius_bottom, radius_top, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let (Some(half), Some(thickness)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "thickness"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("box_frame"),
                    vec![point, half, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::Slab => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::TrianglePrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("triangle_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::HexPrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("hex_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_field_named_arg_value(
        &mut self,
        args: &[hir::Arg],
        body: &hir::Body,
        name: &str,
    ) -> Option<Value> {
        args.iter().find_map(|arg| match arg {
            hir::Arg::Named {
                name: arg_name,
                value,
                ..
            } if arg_name.as_str() == name => Some(self.lower_expr(body, *value)),
            _ => None,
        })
    }

    pub(crate) fn lower_field_primitive_support_lower_bound_scene(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[scene_ir::SceneArgExpr],
        point: Value,
        span: TextRange,
    ) -> Value {
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let Some(radius) = self.lower_scene_named_arg_value(args, "radius", span) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("sphere"),
                    vec![point, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Box => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(MirType::Float, SmolStr::new("box"), vec![point, half], span)
            }
            hir::FieldPrimitive::Capsule => {
                let (Some(a), Some(b), Some(radius)) = (
                    self.lower_scene_named_arg_value(args, "a", span),
                    self.lower_scene_named_arg_value(args, "b", span),
                    self.lower_scene_named_arg_value(args, "radius", span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let radius_vec = self.lower_vec3_splat(radius, span);
                let min_ab = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("min"),
                    vec![a.clone(), b.clone()],
                    span,
                );
                let max_ab =
                    self.lower_call_temp(MirType::Vec3, SmolStr::new("max"), vec![a, b], span);
                let min = self.lower_binary_temp(
                    MirType::Vec3,
                    BinaryOp::Sub,
                    min_ab,
                    radius_vec.clone(),
                    span,
                );
                let max =
                    self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, max_ab, radius_vec, span);
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Cylinder => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let min_radius = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    radius.clone(),
                    span,
                );
                let min_half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    half_height.clone(),
                    span,
                );
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_radius.clone(), min_half_height, min_radius],
                    span,
                );
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![radius.clone(), half_height, radius],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::Plane => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::Torus => {
                let (Some(major_radius), Some(minor_radius)) = (
                    self.lower_scene_named_arg_value(args, "major_radius", span),
                    self.lower_scene_named_arg_value(args, "minor_radius", span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                let outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Add,
                    major_radius.clone(),
                    minor_radius.clone(),
                    span,
                );
                let min_outer = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    outer.clone(),
                    span,
                );
                let min_minor = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    minor_radius.clone(),
                    span,
                );
                let min = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![min_outer.clone(), min_minor, min_outer],
                    span,
                );
                let max = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![outer.clone(), minor_radius, outer],
                    span,
                );
                self.lower_bounds_box_support_lower_bound(point, min, max, span)
            }
            hir::FieldPrimitive::RoundedBox => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(radius) = self.lower_scene_named_arg_value(args, "radius", span) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("rounded_box"),
                    vec![point, half, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Ellipsoid => {
                let Some(radii) = self.lower_scene_named_arg_value(args, "radii", span) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("ellipsoid"),
                    vec![point, radii],
                    span,
                )
            }
            hir::FieldPrimitive::Cone => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cone"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::CappedCone => {
                let (Some(radius_bottom), Some(radius_top), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius_bottom", span),
                    self.lower_scene_named_arg_value(args, "radius_top", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capped_cone"),
                    vec![point, radius_bottom, radius_top, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(thickness) = self.lower_scene_named_arg_value(args, "thickness", span)
                else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("box_frame"),
                    vec![point, half, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::Slab => self.unprunable_support_lower_bound(),
            hir::FieldPrimitive::TrianglePrism => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(half_height) = self.lower_scene_named_arg_value(args, "half_height", span)
                else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("triangle_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::HexPrism => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return self.unprunable_support_lower_bound();
                };
                let Some(half_height) = self.lower_scene_named_arg_value(args, "half_height", span)
                else {
                    return self.unprunable_support_lower_bound();
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("hex_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_field_primitive_distance_scene(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[scene_ir::SceneArgExpr],
        point: Value,
        span: TextRange,
    ) -> Value {
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let Some(radius) = self.lower_scene_named_arg_value(args, "radius", span) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("sphere"),
                    vec![point, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Box => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(MirType::Float, SmolStr::new("box"), vec![point, half], span)
            }
            hir::FieldPrimitive::Capsule => {
                let (Some(a), Some(b), Some(radius)) = (
                    self.lower_scene_named_arg_value(args, "a", span),
                    self.lower_scene_named_arg_value(args, "b", span),
                    self.lower_scene_named_arg_value(args, "radius", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capsule"),
                    vec![point, a, b, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Cylinder => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cylinder"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::Plane => {
                let (Some(normal), Some(offset)) = (
                    self.lower_scene_named_arg_value(args, "normal", span),
                    self.lower_scene_named_arg_value(args, "offset", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("plane"),
                    vec![point, normal, offset],
                    span,
                )
            }
            hir::FieldPrimitive::Torus => {
                let (Some(major_radius), Some(minor_radius)) = (
                    self.lower_scene_named_arg_value(args, "major_radius", span),
                    self.lower_scene_named_arg_value(args, "minor_radius", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("torus"),
                    vec![point, major_radius, minor_radius],
                    span,
                )
            }
            hir::FieldPrimitive::RoundedBox => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let Some(radius) = self.lower_scene_named_arg_value(args, "radius", span) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("rounded_box"),
                    vec![point, half, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Ellipsoid => {
                let Some(radii) = self.lower_scene_named_arg_value(args, "radii", span) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("ellipsoid"),
                    vec![point, radii],
                    span,
                )
            }
            hir::FieldPrimitive::Cone => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cone"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::CappedCone => {
                let (Some(radius_bottom), Some(radius_top), Some(half_height)) = (
                    self.lower_scene_named_arg_value(args, "radius_bottom", span),
                    self.lower_scene_named_arg_value(args, "radius_top", span),
                    self.lower_scene_named_arg_value(args, "half_height", span),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capped_cone"),
                    vec![point, radius_bottom, radius_top, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let Some(thickness) = self.lower_scene_named_arg_value(args, "thickness", span)
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("box_frame"),
                    vec![point, half, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::Slab => {
                let Some(thickness) = self.lower_scene_named_arg_value(args, "thickness", span)
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("slab"),
                    vec![point, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::TrianglePrism => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let Some(half_height) = self.lower_scene_named_arg_value(args, "half_height", span)
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("triangle_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::HexPrism => {
                let half = self
                    .lower_scene_named_arg_value(args, "half", span)
                    .or_else(|| self.lower_scene_named_arg_value(args, "half_size", span));
                let Some(half) = half else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let Some(half_height) = self.lower_scene_named_arg_value(args, "half_height", span)
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("hex_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_vec3_splat(&mut self, value: Value, span: TextRange) -> Value {
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![value.clone(), value.clone(), value],
            span,
        )
    }

    pub(crate) fn lower_vec2_value(&mut self, x: Value, y: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Vec2, SmolStr::new("vec2"), vec![x, y], span)
    }

    pub(crate) fn lower_vec3_value(&mut self, x: Value, y: Value, z: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Vec3, SmolStr::new("vec3"), vec![x, y, z], span)
    }

    pub(crate) fn lower_vec4_value(
        &mut self,
        x: Value,
        y: Value,
        z: Value,
        w: Value,
        span: TextRange,
    ) -> Value {
        self.lower_call_temp(MirType::Vec4, SmolStr::new("vec4"), vec![x, y, z, w], span)
    }

    pub(crate) fn lower_vec_component_value(&mut self, value: Value, index: i64, span: TextRange) -> Value {
        self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![value, Value::Const(Literal::Integer(index))],
            span,
        )
    }

    pub(crate) fn lower_get_component(
        &mut self,
        value: Value,
        ty: MirType,
        member: &str,
        span: TextRange,
    ) -> Value {
        let Some(index) = vector_component_index(ty, &SmolStr::new(member)) else {
            return Value::Const(Literal::Float(0.0));
        };
        self.lower_vec_component_value(value, index as i64, span)
    }

    pub(crate) fn lower_scalar_min(&mut self, left: Value, right: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Float, SmolStr::new("min"), vec![left, right], span)
    }

    pub(crate) fn lower_scalar_max(&mut self, left: Value, right: Value, span: TextRange) -> Value {
        self.lower_call_temp(MirType::Float, SmolStr::new("max"), vec![left, right], span)
    }

    pub(crate) fn lower_bounds_box_support_lower_bound(
        &mut self,
        point: Value,
        min: Value,
        max: Value,
        span: TextRange,
    ) -> Value {
        let center_sum =
            self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, min.clone(), max.clone(), span);
        let center = self.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Mul,
            center_sum,
            Value::Const(Literal::Float(0.5)),
            span,
        );
        let half_delta = self.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, max, min, span);
        let half = self.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Mul,
            half_delta,
            Value::Const(Literal::Float(0.5)),
            span,
        );
        let local_point = self.lower_binary_temp(MirType::Vec3, BinaryOp::Sub, point, center, span);
        self.lower_call_temp(
            MirType::Float,
            SmolStr::new("box"),
            vec![local_point, half],
            span,
        )
    }

    pub(crate) fn lower_field_primitive_distance(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[hir::Arg],
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match primitive {
            hir::FieldPrimitive::Sphere => {
                let Some(radius) = self.lower_field_named_arg_value(args, body, "radius") else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("sphere"),
                    vec![point, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Box => {
                let Some(half) = self.lower_field_named_arg_value(args, body, "half") else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(MirType::Float, SmolStr::new("box"), vec![point, half], span)
            }
            hir::FieldPrimitive::Capsule => {
                let (Some(a), Some(b), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "a"),
                    self.lower_field_named_arg_value(args, body, "b"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capsule"),
                    vec![point, a, b, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Cylinder => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cylinder"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::Plane => {
                let (Some(normal), Some(offset)) = (
                    self.lower_field_named_arg_value(args, body, "normal"),
                    self.lower_field_named_arg_value(args, body, "offset"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("plane"),
                    vec![point, normal, offset],
                    span,
                )
            }
            hir::FieldPrimitive::Torus => {
                let (Some(major_radius), Some(minor_radius)) = (
                    self.lower_field_named_arg_value(args, body, "major_radius"),
                    self.lower_field_named_arg_value(args, body, "minor_radius"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("torus"),
                    vec![point, major_radius, minor_radius],
                    span,
                )
            }
            hir::FieldPrimitive::RoundedBox => {
                let (Some(half), Some(radius)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "radius"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("rounded_box"),
                    vec![point, half, radius],
                    span,
                )
            }
            hir::FieldPrimitive::Ellipsoid => {
                let Some(radii) = self.lower_field_named_arg_value(args, body, "radii") else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("ellipsoid"),
                    vec![point, radii],
                    span,
                )
            }
            hir::FieldPrimitive::Cone => {
                let (Some(radius), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("cone"),
                    vec![point, radius, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::CappedCone => {
                let (Some(radius_bottom), Some(radius_top), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "radius_bottom"),
                    self.lower_field_named_arg_value(args, body, "radius_top"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("capped_cone"),
                    vec![point, radius_bottom, radius_top, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::BoxFrame => {
                let (Some(half), Some(thickness)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "thickness"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("box_frame"),
                    vec![point, half, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::Slab => {
                let Some(thickness) = self.lower_field_named_arg_value(args, body, "thickness")
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("slab"),
                    vec![point, thickness],
                    span,
                )
            }
            hir::FieldPrimitive::TrianglePrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("triangle_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
            hir::FieldPrimitive::HexPrism => {
                let (Some(half), Some(half_height)) = (
                    self.lower_field_named_arg_value(args, body, "half"),
                    self.lower_field_named_arg_value(args, body, "half_height"),
                ) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("hex_prism"),
                    vec![point, half, half_height],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_field_distance_expr(
        &mut self,
        expr: &hir::FieldExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::FieldExpr::Use { target } => self.lower_field_distance_call(target, point, span),
            hir::FieldExpr::Primitive { primitive, args } => {
                self.lower_field_primitive_distance(*primitive, args, body, point, span)
            }
            hir::FieldExpr::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let mut current = self.lower_field_distance_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_field_distance_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let mut current = self.lower_field_distance_expr(first, body, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_field_distance_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::Subtract { left, right } => {
                let left = self.lower_field_distance_expr(left, body, point.clone(), span);
                let right = self.lower_field_distance_expr(right, body, point, span);
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("field_subtract"),
                    vec![left, right],
                    span,
                )
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_translate_point",
                    "translate",
                    translate,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_rotate_point",
                    "rotate",
                    rotate,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let scale_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("field_uniform_scale_point"),
                    vec![scale_value.clone(), point],
                    span,
                );
                let scaled = self.lower_field_distance_expr(inner, body, local_point, span);
                let abs_scale = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![scale_value],
                    span,
                );
                self.lower_binary_temp(MirType::Float, BinaryOp::Mul, scaled, abs_scale, span)
            }
            hir::FieldExpr::AffineTransform {
                transform,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_affine_transform_point",
                    "transform",
                    transform,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Warp { warp, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("field_warp_point", "warp", warp, point, span);
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_repeat_linear_point",
                    "repeat",
                    repeat,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_repeat_grid_point",
                    "repeat",
                    repeat,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::RadialRepeat {
                radial,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_radial_repeat_point",
                    "radial",
                    radial,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_mirror_array_point",
                    "mirror",
                    mirror,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::InstanceArray {
                instance,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_instance_array_point",
                    "instance",
                    instance,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::SmoothUnion { smoothing, items } => {
                let Some(first) = items.first() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let smoothing_value = self.lower_wrapped_body_value(smoothing, span);
                let mut current = self.lower_field_distance_expr(first, body, point.clone(), span);
                for item in items.iter().skip(1) {
                    let rhs = self.lower_field_distance_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_smooth_union"),
                        vec![smoothing_value.clone(), current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::SmoothIntersection { smoothing, items } => {
                let Some(first) = items.first() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let smoothing_value = self.lower_wrapped_body_value(smoothing, span);
                let mut current = self.lower_field_distance_expr(first, body, point.clone(), span);
                for item in items.iter().skip(1) {
                    let rhs = self.lower_field_distance_expr(item, body, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_smooth_intersection"),
                        vec![smoothing_value.clone(), current, rhs],
                        span,
                    );
                }
                current
            }
            hir::FieldExpr::SmoothSubtract {
                smoothing,
                left,
                right,
            } => {
                let smoothing_value = self.lower_wrapped_body_value(smoothing, span);
                let left = self.lower_field_distance_expr(left, body, point.clone(), span);
                let right = self.lower_field_distance_expr(right, body, point, span);
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("field_smooth_subtract"),
                    vec![smoothing_value, left, right],
                    span,
                )
            }
            hir::FieldExpr::Bend { bend, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("field_bend_point", "bend", bend, point, span);
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Twist { twist, body: inner } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_twist_point",
                    "twist",
                    twist,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Taper { taper, body: inner } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_taper_point",
                    "taper",
                    taper,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Displace {
                displace,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_displace_point",
                    "displace",
                    displace,
                    point,
                    span,
                );
                self.lower_field_distance_expr(inner, body, local_point, span)
            }
            hir::FieldExpr::Extrude { height, profile } => {
                let height_value = self.lower_wrapped_body_value(height, span);
                let y = self.lower_get_component(point.clone(), MirType::Vec3, "y", span);
                let point_x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let point_z = self.lower_get_component(point, MirType::Vec3, "z", span);
                let profile_point = self.lower_vec2_value(point_x, point_z, span);
                let profile_distance =
                    self.lower_profile_distance_expr(profile, body, profile_point, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let abs_y =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![y], span);
                let axial =
                    self.lower_binary_temp(MirType::Float, BinaryOp::Sub, abs_y, half_height, span);
                self.lower_profile_cap_distance_value(profile_distance, axial, span)
            }
            hir::FieldExpr::Revolve { profile } => {
                let point_x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let point_z = self.lower_get_component(point.clone(), MirType::Vec3, "z", span);
                let radial_point = self.lower_vec2_value(point_x, point_z, span);
                let radial = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("length"),
                    vec![radial_point],
                    span,
                );
                let point_y = self.lower_get_component(point, MirType::Vec3, "y", span);
                let profile_point = self.lower_vec2_value(radial, point_y, span);
                self.lower_profile_distance_expr(profile, body, profile_point, span)
            }
            hir::FieldExpr::Sweep { path, profile } => {
                let path_value = self.lower_wrapped_body_value(path, span);
                let coords = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("field_sweep_coords"),
                    vec![path_value.clone(), point],
                    span,
                );
                let coords_x = self.lower_get_component(coords.clone(), MirType::Vec3, "x", span);
                let coords_y = self.lower_get_component(coords.clone(), MirType::Vec3, "y", span);
                let profile_point = self.lower_vec2_value(coords_x, coords_y, span);
                let profile_distance =
                    self.lower_profile_distance_expr(profile, body, profile_point, span);
                let coords_z = self.lower_get_component(coords, MirType::Vec3, "z", span);
                let path_length = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("length"),
                    vec![path_value],
                    span,
                );
                let half_length = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    path_length,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let abs_z =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![coords_z], span);
                let axial =
                    self.lower_binary_temp(MirType::Float, BinaryOp::Sub, abs_z, half_length, span);
                self.lower_profile_cap_distance_value(profile_distance, axial, span)
            }
            hir::FieldExpr::Loft { height, from, to } => {
                let height_value = self.lower_wrapped_body_value(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height.clone(),
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let safe_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("max"),
                    vec![abs_height, Value::Const(Literal::Float(0.0001))],
                    span,
                );
                let y = self.lower_get_component(point.clone(), MirType::Vec3, "y", span);
                let point_x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let point_z = self.lower_get_component(point, MirType::Vec3, "z", span);
                let profile_point = self.lower_vec2_value(point_x, point_z, span);
                let from_distance =
                    self.lower_profile_distance_expr(from, body, profile_point.clone(), span);
                let to_distance = self.lower_profile_distance_expr(to, body, profile_point, span);
                let y_plus_half = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Add,
                    y.clone(),
                    half_height.clone(),
                    span,
                );
                let unclamped_t = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Div,
                    y_plus_half,
                    safe_height,
                    span,
                );
                let t = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("clamp"),
                    vec![
                        unclamped_t,
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(1.0)),
                    ],
                    span,
                );
                let mixed = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("mix"),
                    vec![from_distance, to_distance, t],
                    span,
                );
                let abs_y =
                    self.lower_call_temp(MirType::Float, SmolStr::new("abs"), vec![y], span);
                let axial =
                    self.lower_binary_temp(MirType::Float, BinaryOp::Sub, abs_y, half_height, span);
                self.lower_profile_cap_distance_value(mixed, axial, span)
            }
            hir::FieldExpr::Custom { .. } => Value::Const(Literal::Float(1_000_000.0)),
        }
    }

    pub(crate) fn lower_profile_distance_expr(
        &mut self,
        profile: &hir::ProfileExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match profile {
            hir::ProfileExpr::Primitive { primitive, args } => match primitive {
                hir::ProfilePrimitive::Circle2 => {
                    let Some(radius) = self.lower_field_named_arg_value(args, body, "radius")
                    else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("circle2"),
                        vec![point, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Rect2 => {
                    let Some(half) = self.lower_field_named_arg_value(args, body, "half") else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("rect2"),
                        vec![point, half],
                        span,
                    )
                }
                hir::ProfilePrimitive::RoundedRect2 => {
                    let (Some(half), Some(radius)) = (
                        self.lower_field_named_arg_value(args, body, "half"),
                        self.lower_field_named_arg_value(args, body, "radius"),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("rounded_rect2"),
                        vec![point, half, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Capsule2 => {
                    let (Some(a), Some(b), Some(radius)) = (
                        self.lower_field_named_arg_value(args, body, "a"),
                        self.lower_field_named_arg_value(args, body, "b"),
                        self.lower_field_named_arg_value(args, body, "radius"),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("capsule2"),
                        vec![point, a, b, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Segment2 => {
                    let (Some(a), Some(b)) = (
                        self.lower_field_named_arg_value(args, body, "a"),
                        self.lower_field_named_arg_value(args, body, "b"),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("segment2"),
                        vec![point, a, b],
                        span,
                    )
                }
                hir::ProfilePrimitive::Polygon2 | hir::ProfilePrimitive::Polyline2 => {
                    let Some(vertices) = self.lower_field_named_arg_value(args, body, "vertices")
                    else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    let callee = match primitive {
                        hir::ProfilePrimitive::Polygon2 => "polygon2",
                        hir::ProfilePrimitive::Polyline2 => "polyline2",
                        _ => unreachable!(),
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new(callee),
                        vec![point, vertices],
                        span,
                    )
                }
            },
        }
    }

    pub(crate) fn lower_profile_cap_distance_value(
        &mut self,
        profile_distance: Value,
        axial_distance: Value,
        span: TextRange,
    ) -> Value {
        let d = self.lower_vec2_value(profile_distance, axial_distance, span);
        let zero = self.lower_vec2_value(
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            span,
        );
        let outside = self.lower_call_temp(
            MirType::Vec2,
            SmolStr::new("max"),
            vec![d.clone(), zero],
            span,
        );
        let d_x = self.lower_get_component(d.clone(), MirType::Vec2, "x", span);
        let d_y = self.lower_get_component(d, MirType::Vec2, "y", span);
        let max_xy =
            self.lower_call_temp(MirType::Float, SmolStr::new("max"), vec![d_x, d_y], span);
        let inside = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("min"),
            vec![max_xy, Value::Const(Literal::Float(0.0))],
            span,
        );
        let outside_len =
            self.lower_call_temp(MirType::Float, SmolStr::new("length"), vec![outside], span);
        self.lower_binary_temp(MirType::Float, BinaryOp::Add, inside, outside_len, span)
    }

    pub(crate) fn lower_shape_distance_call(
        &mut self,
        shape: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        self.lower_shape_distance_call_with_mode(
            shape,
            point,
            span,
            ShapeExecutionMode::SupportPruned,
        )
    }

    pub(crate) fn lower_shape_distance_call_with_mode(
        &mut self,
        shape: &SmolStr,
        point: Value,
        span: TextRange,
        mode: ShapeExecutionMode,
    ) -> Value {
        if !self.shape_names.contains(shape) {
            return Value::Const(Literal::Float(1_000_000.0));
        }
        self.lower_call_temp(
            MirType::Float,
            mode.distance_helper_name(shape),
            vec![point],
            span,
        )
    }

    pub(crate) fn lower_shape_normal_call(&mut self, shape: &SmolStr, point: Value, span: TextRange) -> Value {
        self.lower_shape_normal_call_with_mode(
            shape,
            point,
            span,
            ShapeExecutionMode::SupportPruned,
        )
    }

    pub(crate) fn lower_shape_normal_call_with_mode(
        &mut self,
        shape: &SmolStr,
        point: Value,
        span: TextRange,
        mode: ShapeExecutionMode,
    ) -> Value {
        let dx = self.lower_shape_axis_difference_with_mode(
            shape,
            point.clone(),
            [0.001, 0.0, 0.0],
            span,
            mode,
        );
        let dy = self.lower_shape_axis_difference_with_mode(
            shape,
            point.clone(),
            [0.0, 0.001, 0.0],
            span,
            mode,
        );
        let dz =
            self.lower_shape_axis_difference_with_mode(shape, point, [0.0, 0.0, 0.001], span, mode);
        let gradient =
            self.lower_call_temp(MirType::Vec3, SmolStr::new("vec3"), vec![dx, dy, dz], span);
        self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![gradient],
            span,
        )
    }

    pub(crate) fn lower_shape_trace_call(
        &mut self,
        shape: &SmolStr,
        origin: Value,
        direction: Value,
        max_distance: Value,
        min_step: Value,
        hit_epsilon: Value,
        max_steps: Value,
        span: TextRange,
    ) -> Value {
        self.lower_shape_trace_call_with_mode(
            shape,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
            span,
            ShapeExecutionMode::SupportPruned,
        )
    }

    pub(crate) fn lower_shape_trace_call_with_mode(
        &mut self,
        shape: &SmolStr,
        origin: Value,
        direction: Value,
        max_distance: Value,
        min_step: Value,
        hit_epsilon: Value,
        max_steps: Value,
        span: TextRange,
        mode: ShapeExecutionMode,
    ) -> Value {
        if !self.shape_names.contains(shape) {
            return self.build_default_hit(origin, span);
        }
        self.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            mode.trace_helper_name(shape),
            vec![
                origin,
                direction,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
            ],
            span,
        )
    }

    pub(crate) fn lower_shape_surface_call(&mut self, shape: &SmolStr, hit: Value, span: TextRange) -> Value {
        if !self.shape_names.contains(shape) {
            return self.build_default_surface(span);
        }
        self.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{shape}")),
            vec![hit],
            span,
        )
    }

    pub(crate) fn lower_shape_axis_difference(
        &mut self,
        shape: &SmolStr,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        self.lower_shape_axis_difference_with_mode(
            shape,
            point,
            offset,
            span,
            ShapeExecutionMode::SupportPruned,
        )
    }

    pub(crate) fn lower_shape_axis_difference_with_mode(
        &mut self,
        shape: &SmolStr,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
        mode: ShapeExecutionMode,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_shape_distance_call_with_mode(shape, plus_point, span, mode);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_shape_distance_call_with_mode(shape, minus_point, span, mode);
        self.lower_binary_temp(MirType::Float, BinaryOp::Sub, plus, minus, span)
    }

    pub(crate) fn lower_shape_merge_keep_current_scene(
        &mut self,
        provenance: scene_ir::ShapeMergeProvenancePolicy,
        current_dist: Value,
        next_dist: Value,
        prefer_larger: bool,
        _hit_epsilon: Value,
        span: TextRange,
    ) -> Value {
        match provenance {
            scene_ir::ShapeMergeProvenancePolicy::Nearest => self.lower_binary_temp(
                MirType::Boolean,
                if prefer_larger {
                    BinaryOp::Ge
                } else {
                    BinaryOp::Le
                },
                current_dist,
                next_dist,
                span,
            ),
            scene_ir::ShapeMergeProvenancePolicy::Ordered => {
                Value::Const(Literal::Boolean(true))
            }
        }
    }

    pub(crate) fn shape_node_has_opaque_boundary(&self, node: &scene_ir::ShapeNode) -> bool {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                self.shape_scene(target).is_none_or(|scene| scene.opaque_boundary)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                leaf.opaque_boundary
                    || matches!(
                        leaf.field_semantics,
                        scene_ir::DistanceSemantics::UnknownOpaque
                    )
                    || self
                        .field_scene(&leaf.field)
                        .is_none_or(|scene| scene.opaque_boundary)
            }
            scene_ir::ShapeNode::Union { items } | scene_ir::ShapeNode::Intersection { items } => {
                items.iter().any(|item| self.shape_node_has_opaque_boundary(item))
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                self.shape_node_has_opaque_boundary(left) || self.shape_node_has_opaque_boundary(right)
            }
        }
    }

    pub(crate) fn shape_node_can_coarse_support_prune(&self, node: &scene_ir::ShapeNode) -> bool {
        match node {
            scene_ir::ShapeNode::Use { target } => self.shape_scene(target).is_some_and(|scene| {
                !scene.opaque_boundary && scene.can_coarse_support_pruning
            }),
            scene_ir::ShapeNode::Leaf(leaf) => self.field_scene(&leaf.field).is_some_and(|scene| {
                !scene.opaque_boundary && scene.can_coarse_support_pruning
            }),
            scene_ir::ShapeNode::Union { items }
            | scene_ir::ShapeNode::Intersection { items } => {
                !items.is_empty()
                    && items
                        .iter()
                        .all(|item| self.shape_node_can_coarse_support_prune(item))
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                !self.shape_node_has_opaque_boundary(right)
                    && self.shape_node_can_coarse_support_prune(left)
            }
        }
    }

    pub(crate) fn lower_shape_support_lower_bound_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        point: Value,
        span: TextRange,
    ) -> Value {
        if !self.shape_node_can_coarse_support_prune(node) {
            return self.unprunable_support_lower_bound();
        }
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return self.unprunable_support_lower_bound();
                };
                if scene.opaque_boundary || !scene.can_coarse_support_pruning {
                    return self.unprunable_support_lower_bound();
                }
                self.lower_shape_support_lower_bound_scene(&scene.root, point, span)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                self.lower_field_support_lower_bound_call(&leaf.field, point, span)
            }
            scene_ir::ShapeNode::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_shape_support_lower_bound_scene(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_shape_support_lower_bound_scene(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::ShapeNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.unprunable_support_lower_bound();
                };
                let mut current =
                    self.lower_shape_support_lower_bound_scene(first, point.clone(), span);
                for item in iter {
                    let rhs = self.lower_shape_support_lower_bound_scene(item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::ShapeNode::Subtract { left, .. } => {
                self.lower_shape_support_lower_bound_scene(left, point, span)
            }
        }
    }

    pub(crate) fn lower_shape_distance_scene_in_mode(
        &mut self,
        node: &scene_ir::ShapeNode,
        point: Value,
        span: TextRange,
        mode: ShapeExecutionMode,
    ) -> Value {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                self.lower_shape_distance_call_with_mode(target, point, span, mode)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                self.lower_field_distance_call(&leaf.field, point, span)
            }
            scene_ir::ShapeNode::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let mut current =
                    self.lower_shape_distance_scene_in_mode(first, point.clone(), span, mode);
                for item in iter {
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                        vec![],
                        span,
                    );
                    if !mode.allows_support_pruning()
                        || !self.shape_node_can_coarse_support_prune(item)
                    {
                        let rhs =
                            self.lower_shape_distance_scene_in_mode(item, point.clone(), span, mode);
                        current = self.lower_call_temp(
                            MirType::Float,
                            SmolStr::new("field_union"),
                            vec![current, rhs],
                            span,
                        );
                        continue;
                    }
                    let support_lower_bound =
                        self.lower_shape_support_lower_bound_scene(item, point.clone(), span);
                    let keep_pruned = self.lower_binary_temp(
                        MirType::Boolean,
                        BinaryOp::Ge,
                        support_lower_bound,
                        current.clone(),
                        span,
                    );
                    let prune_block = self.new_block();
                    let eval_block = self.new_block();
                    let merge_block = self.new_block();
                    let dist_local =
                        self.new_local(SmolStr::new("$shape_union_dist"), true, MirType::Float);
                    self.assign_use(Place::Local(dist_local), current, span);
                    self.set_terminator(Terminator::Branch {
                        cond: keep_pruned,
                        then_target: prune_block,
                        else_target: eval_block,
                        span,
                    });
                    self.current_block = prune_block;
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch"),
                        vec![],
                        span,
                    );
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = eval_block;
                    let rhs =
                        self.lower_shape_distance_scene_in_mode(item, point.clone(), span, mode);
                    let next = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![Value::Local(dist_local), rhs],
                        span,
                    );
                    self.assign_use(Place::Local(dist_local), next, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                    current = Value::Local(dist_local);
                }
                current
            }
            scene_ir::ShapeNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let mut current =
                    self.lower_shape_distance_scene_in_mode(first, point.clone(), span, mode);
                for item in iter {
                    let _ = self.lower_call_temp(
                        MirType::Nil,
                        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                        vec![],
                        span,
                    );
                    let rhs =
                        self.lower_shape_distance_scene_in_mode(item, point.clone(), span, mode);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let lhs = self.lower_shape_distance_scene_in_mode(left, point.clone(), span, mode);
                let _ = self.lower_call_temp(
                    MirType::Nil,
                    SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
                    vec![],
                    span,
                );
                let rhs = self.lower_shape_distance_scene_in_mode(right, point, span, mode);
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("field_subtract"),
                    vec![lhs, rhs],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_shape_payload_selection_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        provenance: Option<&scene_ir::ShapeProvenanceExpr>,
        point: Value,
        hit_epsilon: Value,
        span: TextRange,
    ) -> (Value, Value, Value) {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                self.lower_shape_payload_selection_scene(
                    &scene.root,
                    scene.provenance.as_ref(),
                    point,
                    hit_epsilon,
                    span,
                )
            }
            scene_ir::ShapeNode::Leaf(leaf) => (
                self.lower_field_distance_call(&leaf.field, point, span),
                self.lower_shape_payload_body_value(&leaf.payload, span),
                Value::Const(Literal::Integer(i64::from(leaf.feature_id))),
            ),
            scene_ir::ShapeNode::Union { items } => {
                let (merge_policy, provenance_items) = match provenance {
                    Some(scene_ir::ShapeProvenanceExpr::Union { provenance, items }) => {
                        (*provenance, Some(items.as_slice()))
                    }
                    _ => (scene_ir::ShapeMergeProvenancePolicy::Nearest, None),
                };
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                let first_provenance = provenance_items.and_then(|items| items.first());
                let (first_dist, first_payload, first_feature_id) = self
                    .lower_shape_payload_selection_scene(
                        first,
                        first_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        span,
                    );
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                self.assign_use(Place::Local(dist_local), first_dist, span);
                self.assign_use(Place::Local(payload_local), first_payload, span);
                self.assign_use(Place::Local(feature_id_local), first_feature_id, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    let next_provenance = provenance_items.and_then(|items| items.get(idx));
                    let (next_dist, next_payload, next_feature_id) = self
                        .lower_shape_payload_selection_scene(
                            item,
                            next_provenance,
                            point.clone(),
                            hit_epsilon.clone(),
                            span,
                        );
                    match merge_policy {
                        scene_ir::ShapeMergeProvenancePolicy::Ordered => {
                            let composed_dist = self.lower_call_temp(
                                MirType::Float,
                                SmolStr::new("field_union"),
                                vec![Value::Local(dist_local), next_dist],
                                span,
                            );
                            self.assign_use(Place::Local(dist_local), composed_dist, span);
                        }
                        scene_ir::ShapeMergeProvenancePolicy::Nearest => {
                            let keep_current = self.lower_shape_merge_keep_current_scene(
                                merge_policy,
                                Value::Local(dist_local),
                                next_dist.clone(),
                                false,
                                hit_epsilon.clone(),
                                span,
                            );
                            let keep_block = self.new_block();
                            let replace_block = self.new_block();
                            let merge_block = self.new_block();
                            self.set_terminator(Terminator::Branch {
                                cond: keep_current,
                                then_target: keep_block,
                                else_target: replace_block,
                                span,
                            });
                            self.current_block = keep_block;
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = replace_block;
                            self.assign_use(Place::Local(dist_local), next_dist, span);
                            self.assign_use(Place::Local(payload_local), next_payload, span);
                            self.assign_use(Place::Local(feature_id_local), next_feature_id, span);
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = merge_block;
                        }
                    }
                }
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
            scene_ir::ShapeNode::Intersection { items } => {
                let (merge_policy, provenance_items) = match provenance {
                    Some(scene_ir::ShapeProvenanceExpr::Intersection { provenance, items }) => {
                        (*provenance, Some(items.as_slice()))
                    }
                    _ => (scene_ir::ShapeMergeProvenancePolicy::Nearest, None),
                };
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Float(1_000_000.0)),
                        self.build_default_payload(span),
                        Value::Const(Literal::Integer(0)),
                    );
                };
                let first_provenance = provenance_items.and_then(|items| items.first());
                let (first_dist, first_payload, first_feature_id) = self
                    .lower_shape_payload_selection_scene(
                        first,
                        first_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        span,
                    );
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                self.assign_use(Place::Local(dist_local), first_dist, span);
                self.assign_use(Place::Local(payload_local), first_payload, span);
                self.assign_use(Place::Local(feature_id_local), first_feature_id, span);
                for (idx, item) in iter.enumerate().map(|(idx, item)| (idx + 1, item)) {
                    let next_provenance = provenance_items.and_then(|items| items.get(idx));
                    let (next_dist, next_payload, next_feature_id) = self
                        .lower_shape_payload_selection_scene(
                            item,
                            next_provenance,
                            point.clone(),
                            hit_epsilon.clone(),
                            span,
                        );
                    match merge_policy {
                        scene_ir::ShapeMergeProvenancePolicy::Ordered => {
                            let composed_dist = self.lower_call_temp(
                                MirType::Float,
                                SmolStr::new("field_intersection"),
                                vec![Value::Local(dist_local), next_dist],
                                span,
                            );
                            self.assign_use(Place::Local(dist_local), composed_dist, span);
                        }
                        scene_ir::ShapeMergeProvenancePolicy::Nearest => {
                            let keep_current = self.lower_shape_merge_keep_current_scene(
                                merge_policy,
                                Value::Local(dist_local),
                                next_dist.clone(),
                                true,
                                hit_epsilon.clone(),
                                span,
                            );
                            let keep_block = self.new_block();
                            let replace_block = self.new_block();
                            let merge_block = self.new_block();
                            self.set_terminator(Terminator::Branch {
                                cond: keep_current,
                                then_target: keep_block,
                                else_target: replace_block,
                                span,
                            });
                            self.current_block = keep_block;
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = replace_block;
                            self.assign_use(Place::Local(dist_local), next_dist, span);
                            self.assign_use(Place::Local(payload_local), next_payload, span);
                            self.assign_use(Place::Local(feature_id_local), next_feature_id, span);
                            self.set_terminator(Terminator::Jump {
                                target: merge_block,
                                span,
                            });
                            self.current_block = merge_block;
                        }
                    }
                }
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let (subtract_policy, left_provenance, right_provenance) = match provenance {
                    Some(scene_ir::ShapeProvenanceExpr::Subtract {
                        provenance,
                        left,
                        right,
                    }) => (*provenance, Some(left.as_ref()), Some(right.as_ref())),
                    _ => (scene_ir::ShapeSubtractProvenancePolicy::Left, None, None),
                };
                let (left_dist, left_payload, left_feature_id) = self
                    .lower_shape_payload_selection_scene(
                        left,
                        left_provenance,
                        point.clone(),
                        hit_epsilon.clone(),
                        span,
                    );
                let (right_dist, right_payload, right_feature_id) = self
                    .lower_shape_payload_selection_scene(
                        right,
                        right_provenance,
                        point,
                        hit_epsilon,
                        span,
                    );
                let neg_right = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    Value::Const(Literal::Float(0.0)),
                    right_dist,
                    span,
                );
                let choose_left = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Ge,
                    left_dist.clone(),
                    neg_right.clone(),
                    span,
                );
                let dist_local = self.new_local(SmolStr::new("$shape_dist"), true, MirType::Float);
                let payload_local = self.new_local(
                    SmolStr::new("$shape_payload"),
                    true,
                    MirType::Named(SmolStr::new("Payload")),
                );
                let feature_id_local =
                    self.new_local(SmolStr::new("$shape_feature_id"), true, MirType::Integer);
                let left_block = self.new_block();
                let right_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: choose_left,
                    then_target: left_block,
                    else_target: right_block,
                    span,
                });
                self.current_block = left_block;
                self.assign_use(Place::Local(dist_local), left_dist, span);
                self.assign_use(Place::Local(payload_local), left_payload.clone(), span);
                self.assign_use(Place::Local(feature_id_local), left_feature_id.clone(), span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = right_block;
                self.assign_use(Place::Local(dist_local), neg_right, span);
                match subtract_policy {
                    scene_ir::ShapeSubtractProvenancePolicy::Left => {
                        self.assign_use(Place::Local(payload_local), left_payload, span);
                        self.assign_use(Place::Local(feature_id_local), left_feature_id, span);
                    }
                    scene_ir::ShapeSubtractProvenancePolicy::Right => {
                        self.assign_use(Place::Local(payload_local), right_payload, span);
                        self.assign_use(Place::Local(feature_id_local), right_feature_id, span);
                    }
                }
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (
                    Value::Local(dist_local),
                    Value::Local(payload_local),
                    Value::Local(feature_id_local),
                )
            }
        }
    }

    pub(crate) fn lower_shape_surface_selection_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        feature_id: Value,
        hit: Value,
        span: TextRange,
    ) -> (Value, Value) {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_surface(span),
                    );
                };
                self.lower_shape_surface_selection_scene(&scene.root, feature_id, hit, span)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id,
                    Value::Const(Literal::Integer(i64::from(leaf.feature_id))),
                    span,
                );
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface_leaf"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                let default_surface = self.build_default_surface(span);
                self.assign_use(
                    Place::Local(surface_local),
                    default_surface,
                    span,
                );
                let matched_block = self.new_block();
                let miss_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: matched.clone(),
                    then_target: matched_block,
                    else_target: miss_block,
                    span,
                });
                self.current_block = matched_block;
                let surface = self.lower_call_temp(
                    MirType::Named(SmolStr::new("Surface")),
                    leaf.material.clone(),
                    vec![hit],
                    span,
                );
                self.assign_use(Place::Local(surface_local), surface, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = miss_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (matched, Value::Local(surface_local))
            }
            scene_ir::ShapeNode::Union { items }
            | scene_ir::ShapeNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return (
                        Value::Const(Literal::Boolean(false)),
                        self.build_default_surface(span),
                    );
                };
                let (first_matched, first_surface) = self.lower_shape_surface_selection_scene(
                    first,
                    feature_id.clone(),
                    hit.clone(),
                    span,
                );
                let matched_local =
                    self.new_local(SmolStr::new("$shape_surface_match"), true, MirType::Boolean);
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(surface_local), first_surface, span);
                for item in iter {
                    let (next_matched, next_surface) = self.lower_shape_surface_selection_scene(
                        item,
                        feature_id.clone(),
                        hit.clone(),
                        span,
                    );
                    let already_matched = Value::Local(matched_local);
                    let keep_current = already_matched.clone();
                    let take_next = self.lower_unary_temp(
                        MirType::Boolean,
                        hir::UnaryOp::Not,
                        already_matched,
                        span,
                    );
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: keep_current,
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    let matched_block = self.new_block();
                    let miss_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: take_next,
                        then_target: matched_block,
                        else_target: miss_block,
                        span,
                    });
                    self.current_block = matched_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(surface_local), next_surface, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = miss_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (Value::Local(matched_local), Value::Local(surface_local))
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let (left_matched, left_surface) = self.lower_shape_surface_selection_scene(
                    left,
                    feature_id.clone(),
                    hit.clone(),
                    span,
                );
                let matched_local =
                    self.new_local(SmolStr::new("$shape_surface_match"), true, MirType::Boolean);
                let surface_local = self.new_local(
                    SmolStr::new("$shape_surface"),
                    true,
                    MirType::Named(SmolStr::new("Surface")),
                );
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(surface_local), left_surface, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                let (right_matched, right_surface) =
                    self.lower_shape_surface_selection_scene(right, feature_id, hit, span);
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(surface_local), right_surface, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (Value::Local(matched_local), Value::Local(surface_local))
            }
        }
    }

    pub(crate) fn lower_shape_default_hit_context_selection(
        &mut self,
        point: Value,
        span: TextRange,
    ) -> (Value, Value, Value, Value, Value) {
        let default_normal = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![
                Value::Const(Literal::Float(0.0)),
                Value::Const(Literal::Float(0.0)),
                Value::Const(Literal::Float(1.0)),
            ],
            span,
        );
        (
            Value::Const(Literal::Boolean(false)),
            point,
            default_normal,
            Value::Const(Literal::Integer(0)),
            Value::Const(Literal::Integer(0)),
        )
    }

    pub(crate) fn lower_shape_hit_context_selection_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        feature_id: Value,
        point: Value,
        span: TextRange,
    ) -> (Value, Value, Value, Value, Value) {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return self.lower_shape_default_hit_context_selection(point, span);
                };
                self.lower_shape_hit_context_selection_scene(&scene.root, feature_id, point, span)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                let matched = self.lower_binary_temp(
                    MirType::Boolean,
                    BinaryOp::Eq,
                    feature_id,
                    Value::Const(Literal::Integer(i64::from(leaf.feature_id))),
                    span,
                );
                let zero_id = Value::Const(Literal::Integer(0));
                let (local_point, instance_id, repeat_id) = self.lower_field_local_context_call(
                    &leaf.field,
                    point.clone(),
                    zero_id.clone(),
                    zero_id,
                    span,
                );
                let local_normal = self.lower_field_local_normal_call(&leaf.field, point, span);
                (matched, local_point, local_normal, instance_id, repeat_id)
            }
            scene_ir::ShapeNode::Union { items }
            | scene_ir::ShapeNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return self.lower_shape_default_hit_context_selection(point, span);
                };
                let (first_matched, first_point, first_normal, first_instance, first_repeat) = self
                    .lower_shape_hit_context_selection_scene(
                        first,
                        feature_id.clone(),
                        point.clone(),
                        span,
                    );
                let matched_local = self.new_local(
                    SmolStr::new("$shape_hit_context_match"),
                    true,
                    MirType::Boolean,
                );
                let point_local = self.new_local(
                    SmolStr::new("$shape_hit_context_point"),
                    true,
                    MirType::Vec3,
                );
                let normal_local = self.new_local(
                    SmolStr::new("$shape_hit_context_normal"),
                    true,
                    MirType::Vec3,
                );
                let instance_local = self.new_local(
                    SmolStr::new("$shape_hit_context_instance"),
                    true,
                    MirType::Integer,
                );
                let repeat_local = self.new_local(
                    SmolStr::new("$shape_hit_context_repeat"),
                    true,
                    MirType::Integer,
                );
                self.assign_use(Place::Local(matched_local), first_matched, span);
                self.assign_use(Place::Local(point_local), first_point, span);
                self.assign_use(Place::Local(normal_local), first_normal, span);
                self.assign_use(Place::Local(instance_local), first_instance, span);
                self.assign_use(Place::Local(repeat_local), first_repeat, span);
                for item in iter {
                    let (next_matched, next_point, next_normal, next_instance, next_repeat) = self
                        .lower_shape_hit_context_selection_scene(
                            item,
                            feature_id.clone(),
                            point.clone(),
                            span,
                        );
                    let already_matched = Value::Local(matched_local);
                    let keep_block = self.new_block();
                    let replace_block = self.new_block();
                    let merge_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: already_matched.clone(),
                        then_target: keep_block,
                        else_target: replace_block,
                        span,
                    });
                    self.current_block = keep_block;
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = replace_block;
                    self.assign_use(Place::Local(matched_local), next_matched, span);
                    self.assign_use(Place::Local(point_local), next_point, span);
                    self.assign_use(Place::Local(normal_local), next_normal, span);
                    self.assign_use(Place::Local(instance_local), next_instance, span);
                    self.assign_use(Place::Local(repeat_local), next_repeat, span);
                    self.set_terminator(Terminator::Jump {
                        target: merge_block,
                        span,
                    });
                    self.current_block = merge_block;
                }
                (
                    Value::Local(matched_local),
                    Value::Local(point_local),
                    Value::Local(normal_local),
                    Value::Local(instance_local),
                    Value::Local(repeat_local),
                )
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let (left_matched, left_point, left_normal, left_instance, left_repeat) = self
                    .lower_shape_hit_context_selection_scene(
                        left,
                        feature_id.clone(),
                        point.clone(),
                        span,
                    );
                let matched_local = self.new_local(
                    SmolStr::new("$shape_hit_context_match"),
                    true,
                    MirType::Boolean,
                );
                let point_local = self.new_local(
                    SmolStr::new("$shape_hit_context_point"),
                    true,
                    MirType::Vec3,
                );
                let normal_local = self.new_local(
                    SmolStr::new("$shape_hit_context_normal"),
                    true,
                    MirType::Vec3,
                );
                let instance_local = self.new_local(
                    SmolStr::new("$shape_hit_context_instance"),
                    true,
                    MirType::Integer,
                );
                let repeat_local = self.new_local(
                    SmolStr::new("$shape_hit_context_repeat"),
                    true,
                    MirType::Integer,
                );
                self.assign_use(Place::Local(matched_local), left_matched, span);
                self.assign_use(Place::Local(point_local), left_point, span);
                self.assign_use(Place::Local(normal_local), left_normal, span);
                self.assign_use(Place::Local(instance_local), left_instance, span);
                self.assign_use(Place::Local(repeat_local), left_repeat, span);
                let already_matched = Value::Local(matched_local);
                let keep_block = self.new_block();
                let replace_block = self.new_block();
                let merge_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond: already_matched.clone(),
                    then_target: keep_block,
                    else_target: replace_block,
                    span,
                });
                self.current_block = keep_block;
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = replace_block;
                let (right_matched, right_point, right_normal, right_instance, right_repeat) = self
                    .lower_shape_hit_context_selection_scene(right, feature_id, point, span);
                self.assign_use(Place::Local(matched_local), right_matched, span);
                self.assign_use(Place::Local(point_local), right_point, span);
                self.assign_use(Place::Local(normal_local), right_normal, span);
                self.assign_use(Place::Local(instance_local), right_instance, span);
                self.assign_use(Place::Local(repeat_local), right_repeat, span);
                self.set_terminator(Terminator::Jump {
                    target: merge_block,
                    span,
                });
                self.current_block = merge_block;
                (
                    Value::Local(matched_local),
                    Value::Local(point_local),
                    Value::Local(normal_local),
                    Value::Local(instance_local),
                    Value::Local(repeat_local),
                )
            }
        }
    }

    pub(crate) fn lower_shape_radiance_participation_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        point: Value,
        direction: Value,
        span: TextRange,
    ) -> Value {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("vec3"),
                        vec![
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                            Value::Const(Literal::Float(0.0)),
                        ],
                        span,
                    );
                };
                self.lower_shape_radiance_participation_scene(
                    &scene.root,
                    point,
                    direction,
                    span,
                )
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                let black = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                    ],
                    span,
                );
                let Some(radiance) = &leaf.radiance else {
                    return black;
                };
                let (local_point, _, _) = self.lower_field_local_context_call(
                    &leaf.field,
                    point,
                    Value::Const(Literal::Integer(0)),
                    Value::Const(Literal::Integer(0)),
                    span,
                );
                self.lower_radiance_call(
                    radiance,
                    local_point,
                    direction,
                    Value::Const(Literal::Integer(i64::from(leaf.feature_id))),
                    span,
                )
            }
            scene_ir::ShapeNode::Union { items }
            | scene_ir::ShapeNode::Intersection { items } => {
                let mut total = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(0.0)),
                    ],
                    span,
                );
                for item in items {
                    let next = self.lower_shape_radiance_participation_scene(
                        item,
                        point.clone(),
                        direction.clone(),
                        span,
                    );
                    total = self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, total, next, span);
                }
                total
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let left_value = self.lower_shape_radiance_participation_scene(
                    left,
                    point.clone(),
                    direction.clone(),
                    span,
                );
                let right_value =
                    self.lower_shape_radiance_participation_scene(right, point, direction, span);
                self.lower_binary_temp(MirType::Vec3, BinaryOp::Add, left_value, right_value, span)
            }
        }
    }

    pub(crate) fn lower_shape_medium_participation_scene(
        &mut self,
        node: &scene_ir::ShapeNode,
        point: Value,
        span: TextRange,
    ) -> Value {
        match node {
            scene_ir::ShapeNode::Use { target } => {
                let Some(scene) = self.shape_scene(target).cloned() else {
                    return self.build_default_medium(span);
                };
                self.lower_shape_medium_participation_scene(&scene.root, point, span)
            }
            scene_ir::ShapeNode::Leaf(leaf) => {
                let Some(volume) = &leaf.volume else {
                    return self.build_default_medium(span);
                };
                let (local_point, _, _) = self.lower_field_local_context_call(
                    &leaf.field,
                    point.clone(),
                    Value::Const(Literal::Integer(0)),
                    Value::Const(Literal::Integer(0)),
                    span,
                );
                let local_surface_distance =
                    self.lower_field_local_distance_call(&leaf.field, point, span);
                self.lower_volume_call(volume, local_point, local_surface_distance, span)
            }
            scene_ir::ShapeNode::Union { items }
            | scene_ir::ShapeNode::Intersection { items } => {
                let mut total = self.build_default_medium(span);
                for item in items {
                    let next =
                        self.lower_shape_medium_participation_scene(item, point.clone(), span);
                    total = self.lower_additive_medium_combine(total, next, span);
                }
                total
            }
            scene_ir::ShapeNode::Subtract { left, right } => {
                let left_value =
                    self.lower_shape_medium_participation_scene(left, point.clone(), span);
                let right_value =
                    self.lower_shape_medium_participation_scene(right, point, span);
                self.lower_additive_medium_combine(left_value, right_value, span)
            }
        }
    }

    pub(crate) fn lower_additive_medium_combine(
        &mut self,
        current: Value,
        next: Value,
        span: TextRange,
    ) -> Value {
        let current_density =
            self.lower_get_named_field(current.clone(), "Medium", "density", MirType::Float, span);
        let current_emission =
            self.lower_get_named_field(current.clone(), "Medium", "emission", MirType::Vec3, span);
        let current_anisotropy =
            self.lower_get_named_field(current, "Medium", "anisotropy", MirType::Float, span);
        let next_density =
            self.lower_get_named_field(next.clone(), "Medium", "density", MirType::Float, span);
        let next_emission =
            self.lower_get_named_field(next.clone(), "Medium", "emission", MirType::Vec3, span);
        let next_anisotropy =
            self.lower_get_named_field(next, "Medium", "anisotropy", MirType::Float, span);
        let density = self.lower_binary_temp(
            MirType::Float,
            BinaryOp::Add,
            current_density.clone(),
            next_density.clone(),
            span,
        );
        let emission = self.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Add,
            current_emission,
            next_emission,
            span,
        );
        let current_weighted = self.lower_binary_temp(
            MirType::Float,
            BinaryOp::Mul,
            current_anisotropy,
            current_density,
            span,
        );
        let next_weighted = self.lower_binary_temp(
            MirType::Float,
            BinaryOp::Mul,
            next_anisotropy,
            next_density,
            span,
        );
        let anisotropy_numerator = self.lower_binary_temp(
            MirType::Float,
            BinaryOp::Add,
            current_weighted,
            next_weighted,
            span,
        );
        let anisotropy_local =
            self.new_local(SmolStr::new("$medium_anisotropy"), true, MirType::Float);
        self.assign_use(
            Place::Local(anisotropy_local),
            Value::Const(Literal::Float(0.0)),
            span,
        );
        let has_density = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Gt,
            density.clone(),
            Value::Const(Literal::Float(0.0)),
            span,
        );
        let weighted_block = self.new_block();
        let default_block = self.new_block();
        let merge_block = self.new_block();
        self.set_terminator(Terminator::Branch {
            cond: has_density,
            then_target: weighted_block,
            else_target: default_block,
            span,
        });
        self.current_block = weighted_block;
        let weighted = self.lower_binary_temp(
            MirType::Float,
            BinaryOp::Div,
            anisotropy_numerator,
            density.clone(),
            span,
        );
        self.assign_use(Place::Local(anisotropy_local), weighted, span);
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
        self.current_block = default_block;
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
        self.current_block = merge_block;
        let mut class = self.synthetic_class_target_info("Medium");
        Self::set_class_field_value(&mut class, "density", density);
        Self::set_class_field_value(&mut class, "emission", emission);
        Self::set_class_field_value(&mut class, "anisotropy", Value::Local(anisotropy_local));
        self.build_class_instance(&class, span)
    }

    pub(crate) fn lower_identity_chain_component(
        &mut self,
        current: Value,
        component: Value,
        span: TextRange,
    ) -> Value {
        let result_local = self.new_local(SmolStr::new("$identity_chain"), true, MirType::Integer);
        self.assign_use(Place::Local(result_local), current.clone(), span);
        let component_is_zero = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            component.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        let skip_block = self.new_block();
        let apply_block = self.new_block();
        let merge_block = self.new_block();
        self.set_terminator(Terminator::Branch {
            cond: component_is_zero,
            then_target: skip_block,
            else_target: apply_block,
            span,
        });
        self.current_block = skip_block;
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
        self.current_block = apply_block;
        let current_is_zero = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            current.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        let take_component_block = self.new_block();
        let mix_block = self.new_block();
        self.set_terminator(Terminator::Branch {
            cond: current_is_zero,
            then_target: take_component_block,
            else_target: mix_block,
            span,
        });
        self.current_block = take_component_block;
        self.assign_use(Place::Local(result_local), component.clone(), span);
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
        self.current_block = mix_block;
        let xored =
            self.lower_binary_temp(MirType::Integer, BinaryOp::BitXor, current, component, span);
        let scaled = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::Mul,
            xored,
            Value::Const(Literal::Integer(16_777_619)),
            span,
        );
        let masked = self.lower_binary_temp(
            MirType::Integer,
            BinaryOp::BitAnd,
            scaled,
            Value::Const(Literal::Integer(0xffff_ffff)),
            span,
        );
        let masked_is_zero = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            masked.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        let masked_keep_block = self.new_block();
        let masked_fixup_block = self.new_block();
        let masked_merge_block = self.new_block();
        self.set_terminator(Terminator::Branch {
            cond: masked_is_zero,
            then_target: masked_fixup_block,
            else_target: masked_keep_block,
            span,
        });
        self.current_block = masked_keep_block;
        self.assign_use(Place::Local(result_local), masked, span);
        self.set_terminator(Terminator::Jump {
            target: masked_merge_block,
            span,
        });
        self.current_block = masked_fixup_block;
        self.assign_use(
            Place::Local(result_local),
            Value::Const(Literal::Integer(1)),
            span,
        );
        self.set_terminator(Terminator::Jump {
            target: masked_merge_block,
            span,
        });
        self.current_block = masked_merge_block;
        self.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
        self.current_block = merge_block;
        Value::Local(result_local)
    }

    pub(crate) fn lower_field_local_context_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        instance_id: Value,
        repeat_id: Value,
        span: TextRange,
    ) -> (Value, Value, Value) {
        let Some(scene) = self.field_scene(field).cloned() else {
            return (point, instance_id, repeat_id);
        };
        self.lower_field_local_context_scene(&scene.root, point, instance_id, repeat_id, span)
    }

    pub(crate) fn lower_field_local_context_expr(
        &mut self,
        expr: &hir::FieldExpr,
        point: Value,
        instance_id: Value,
        repeat_id: Value,
        span: TextRange,
    ) -> (Value, Value, Value) {
        match expr {
            hir::FieldExpr::Use { target } => {
                self.lower_field_local_context_call(target, point, instance_id, repeat_id, span)
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("translate", "offset", translate, point, span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_rotate_point",
                    "rotation",
                    rotate,
                    point,
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let wrapper_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("uniform_scale"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::AffineTransform {
                transform,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "affine_transform",
                    "transform",
                    transform,
                    point,
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Warp { warp, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("warp", "warp", warp, point.clone(), span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(repeat, span);
                let component = self.lower_call_temp(
                    MirType::Integer,
                    SmolStr::new("__wr_repeat_linear_identity"),
                    vec![wrapper_value.clone(), point.clone()],
                    span,
                );
                let chained_repeat =
                    self.lower_identity_chain_component(repeat_id, component, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("repeat_linear"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    chained_repeat,
                    span,
                )
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(repeat, span);
                let component = self.lower_call_temp(
                    MirType::Integer,
                    SmolStr::new("__wr_repeat_grid_identity"),
                    vec![wrapper_value.clone(), point.clone()],
                    span,
                );
                let chained_repeat =
                    self.lower_identity_chain_component(repeat_id, component, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("repeat_grid"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    chained_repeat,
                    span,
                )
            }
            hir::FieldExpr::RadialRepeat {
                radial,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(radial, span);
                let component = self.lower_call_temp(
                    MirType::Integer,
                    SmolStr::new("__wr_radial_repeat_identity"),
                    vec![wrapper_value.clone(), point.clone()],
                    span,
                );
                let chained_repeat =
                    self.lower_identity_chain_component(repeat_id, component, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("radial_repeat"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    chained_repeat,
                    span,
                )
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(mirror, span);
                let component = self.lower_call_temp(
                    MirType::Integer,
                    SmolStr::new("__wr_mirror_array_identity"),
                    vec![wrapper_value.clone(), point.clone()],
                    span,
                );
                let chained_repeat =
                    self.lower_identity_chain_component(repeat_id, component, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("mirror_array"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    chained_repeat,
                    span,
                )
            }
            hir::FieldExpr::InstanceArray {
                instance,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(instance, span);
                let component = self.lower_call_temp(
                    MirType::Integer,
                    SmolStr::new("__wr_instance_array_identity"),
                    vec![wrapper_value.clone(), point.clone()],
                    span,
                );
                let chained_instance =
                    self.lower_identity_chain_component(instance_id, component, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("instance_array"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    chained_instance,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Bend { bend, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("bend", "bend", bend, point, span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Twist { twist, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("twist", "twist", twist, point, span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Taper { taper, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("taper", "taper", taper, point, span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            hir::FieldExpr::Displace {
                displace,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("displace", "displace", displace, point, span);
                self.lower_field_local_context_expr(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            _ => (point, instance_id, repeat_id),
        }
    }

    pub(crate) fn lower_field_local_context_scene(
        &mut self,
        node: &scene_ir::FieldNode,
        point: Value,
        instance_id: Value,
        repeat_id: Value,
        span: TextRange,
    ) -> (Value, Value, Value) {
        match node {
            scene_ir::FieldNode::Use { target } => {
                self.lower_field_local_context_call(target, point, instance_id, repeat_id, span)
            }
            scene_ir::FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return (point, instance_id, repeat_id);
                };
                let local_point = match kind {
                    scene_ir::TransformKind::Translate => {
                        self.lower_scene_wrapped_support_point("translate", param, point, span)
                    }
                    scene_ir::TransformKind::Rotate => self.lower_scene_wrapped_support_point(
                        "field_rotate_point",
                        param,
                        point,
                        span,
                    ),
                    scene_ir::TransformKind::UniformScale => {
                        let wrapper_value = self.lower_scene_value_expr(param, span);
                        self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("uniform_scale"),
                            vec![wrapper_value, point],
                            span,
                        )
                    }
                    scene_ir::TransformKind::AffineTransform => self
                        .lower_scene_wrapped_support_point("affine_transform", param, point, span),
                    scene_ir::TransformKind::Warp => {
                        self.lower_scene_wrapped_support_point("warp", param, point, span)
                    }
                    scene_ir::TransformKind::Bend => {
                        self.lower_scene_wrapped_support_point("bend", param, point, span)
                    }
                    scene_ir::TransformKind::Twist => {
                        self.lower_scene_wrapped_support_point("twist", param, point, span)
                    }
                    scene_ir::TransformKind::Taper => {
                        self.lower_scene_wrapped_support_point("taper", param, point, span)
                    }
                    scene_ir::TransformKind::Displace => {
                        self.lower_scene_wrapped_support_point("displace", param, point, span)
                    }
                };
                self.lower_field_local_context_scene(
                    inner,
                    local_point,
                    instance_id,
                    repeat_id,
                    span,
                )
            }
            scene_ir::FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return (point, instance_id, repeat_id);
                };
                let wrapper_value = self.lower_scene_value_expr(param, span);
                match kind {
                    scene_ir::RepeatKind::RepeatLinear => {
                        let component = self.lower_call_temp(
                            MirType::Integer,
                            SmolStr::new("__wr_repeat_linear_identity"),
                            vec![wrapper_value.clone(), point.clone()],
                            span,
                        );
                        let chained_repeat =
                            self.lower_identity_chain_component(repeat_id, component, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("repeat_linear"),
                            vec![wrapper_value, point],
                            span,
                        );
                        self.lower_field_local_context_scene(
                            inner,
                            local_point,
                            instance_id,
                            chained_repeat,
                            span,
                        )
                    }
                    scene_ir::RepeatKind::RepeatGrid => {
                        let component = self.lower_call_temp(
                            MirType::Integer,
                            SmolStr::new("__wr_repeat_grid_identity"),
                            vec![wrapper_value.clone(), point.clone()],
                            span,
                        );
                        let chained_repeat =
                            self.lower_identity_chain_component(repeat_id, component, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("repeat_grid"),
                            vec![wrapper_value, point],
                            span,
                        );
                        self.lower_field_local_context_scene(
                            inner,
                            local_point,
                            instance_id,
                            chained_repeat,
                            span,
                        )
                    }
                    scene_ir::RepeatKind::RadialRepeat => {
                        let component = self.lower_call_temp(
                            MirType::Integer,
                            SmolStr::new("__wr_radial_repeat_identity"),
                            vec![wrapper_value.clone(), point.clone()],
                            span,
                        );
                        let chained_repeat =
                            self.lower_identity_chain_component(repeat_id, component, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("radial_repeat"),
                            vec![wrapper_value, point],
                            span,
                        );
                        self.lower_field_local_context_scene(
                            inner,
                            local_point,
                            instance_id,
                            chained_repeat,
                            span,
                        )
                    }
                    scene_ir::RepeatKind::MirrorArray => {
                        let component = self.lower_call_temp(
                            MirType::Integer,
                            SmolStr::new("__wr_mirror_array_identity"),
                            vec![wrapper_value.clone(), point.clone()],
                            span,
                        );
                        let chained_repeat =
                            self.lower_identity_chain_component(repeat_id, component, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("mirror_array"),
                            vec![wrapper_value, point],
                            span,
                        );
                        self.lower_field_local_context_scene(
                            inner,
                            local_point,
                            instance_id,
                            chained_repeat,
                            span,
                        )
                    }
                    scene_ir::RepeatKind::InstanceArray => {
                        let component = self.lower_call_temp(
                            MirType::Integer,
                            SmolStr::new("__wr_instance_array_identity"),
                            vec![wrapper_value.clone(), point.clone()],
                            span,
                        );
                        let chained_instance =
                            self.lower_identity_chain_component(instance_id, component, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("instance_array"),
                            vec![wrapper_value, point],
                            span,
                        );
                        self.lower_field_local_context_scene(
                            inner,
                            local_point,
                            chained_instance,
                            repeat_id,
                            span,
                        )
                    }
                }
            }
            _ => (point, instance_id, repeat_id),
        }
    }

    pub(crate) fn lower_scene_profile_distance_expr(
        &mut self,
        profile: &scene_ir::SceneProfileExpr,
        point: Value,
        span: TextRange,
    ) -> Value {
        match profile {
            scene_ir::SceneProfileExpr::Primitive { primitive, args } => match primitive {
                hir::ProfilePrimitive::Circle2 => {
                    let Some(radius) = self.lower_scene_named_arg_value(args, "radius", span)
                    else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("circle2"),
                        vec![point, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Rect2 => {
                    let Some(half) = self.lower_scene_named_arg_value(args, "half", span) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("rect2"),
                        vec![point, half],
                        span,
                    )
                }
                hir::ProfilePrimitive::RoundedRect2 => {
                    let (Some(half), Some(radius)) = (
                        self.lower_scene_named_arg_value(args, "half", span),
                        self.lower_scene_named_arg_value(args, "radius", span),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("rounded_rect2"),
                        vec![point, half, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Capsule2 => {
                    let (Some(a), Some(b), Some(radius)) = (
                        self.lower_scene_named_arg_value(args, "a", span),
                        self.lower_scene_named_arg_value(args, "b", span),
                        self.lower_scene_named_arg_value(args, "radius", span),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("capsule2"),
                        vec![point, a, b, radius],
                        span,
                    )
                }
                hir::ProfilePrimitive::Segment2 => {
                    let (Some(a), Some(b)) = (
                        self.lower_scene_named_arg_value(args, "a", span),
                        self.lower_scene_named_arg_value(args, "b", span),
                    ) else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("segment2"),
                        vec![point, a, b],
                        span,
                    )
                }
                hir::ProfilePrimitive::Polygon2 | hir::ProfilePrimitive::Polyline2 => {
                    let Some(vertices) = self.lower_scene_named_arg_value(args, "vertices", span)
                    else {
                        return Value::Const(Literal::Float(1_000_000.0));
                    };
                    let callee = match primitive {
                        hir::ProfilePrimitive::Polygon2 => "polygon2",
                        hir::ProfilePrimitive::Polyline2 => "polyline2",
                        _ => unreachable!(),
                    };
                    self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new(callee),
                        vec![point, vertices],
                        span,
                    )
                }
            },
        }
    }

    pub(crate) fn lower_field_local_distance_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let Some(scene) = self.field_scene(field).cloned() else {
            return self.lower_field_distance_call(field, point, span);
        };
        self.lower_field_local_distance_scene(field, &scene.root, point, span)
    }

    pub(crate) fn lower_field_local_distance_scene(
        &mut self,
        field: &SmolStr,
        node: &scene_ir::FieldNode,
        point: Value,
        span: TextRange,
    ) -> Value {
        match node {
            scene_ir::FieldNode::Use { target } => {
                self.lower_field_local_distance_call(target, point, span)
            }
            scene_ir::FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return self.lower_field_local_distance_scene(field, inner, point, span);
                };
                match kind {
                    scene_ir::TransformKind::UniformScale => {
                        let wrapper_value = self.lower_scene_value_expr(param, span);
                        let local_point = self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("uniform_scale"),
                            vec![wrapper_value.clone(), point],
                            span,
                        );
                        let scaled =
                            self.lower_field_local_distance_scene(field, inner, local_point, span);
                        let abs_scale = self.lower_call_temp(
                            MirType::Float,
                            SmolStr::new("abs"),
                            vec![wrapper_value],
                            span,
                        );
                        self.lower_binary_temp(
                            MirType::Float,
                            BinaryOp::Mul,
                            scaled,
                            abs_scale,
                            span,
                        )
                    }
                    _ => {
                        let local_point = match kind {
                            scene_ir::TransformKind::Translate => {
                                self.lower_scene_wrapped_support_point("translate", param, point, span)
                            }
                            scene_ir::TransformKind::Rotate => self
                                .lower_scene_wrapped_support_point("field_rotate_point", param, point, span),
                            scene_ir::TransformKind::AffineTransform => self
                                .lower_scene_wrapped_support_point("affine_transform", param, point, span),
                            scene_ir::TransformKind::Warp => {
                                self.lower_scene_wrapped_support_point("warp", param, point, span)
                            }
                            scene_ir::TransformKind::Bend => {
                                self.lower_scene_wrapped_support_point("bend", param, point, span)
                            }
                            scene_ir::TransformKind::Twist => {
                                self.lower_scene_wrapped_support_point("twist", param, point, span)
                            }
                            scene_ir::TransformKind::Taper => {
                                self.lower_scene_wrapped_support_point("taper", param, point, span)
                            }
                            scene_ir::TransformKind::Displace => self
                                .lower_scene_wrapped_support_point("displace", param, point, span),
                            scene_ir::TransformKind::UniformScale => unreachable!(),
                        };
                        self.lower_field_local_distance_scene(field, inner, local_point, span)
                    }
                }
            }
            scene_ir::FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return self.lower_field_local_distance_scene(field, inner, point, span);
                };
                let wrapper_value = self.lower_scene_value_expr(param, span);
                let local_point = match kind {
                    scene_ir::RepeatKind::RepeatLinear => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("repeat_linear"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::RepeatGrid => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("repeat_grid"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::RadialRepeat => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("radial_repeat"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::MirrorArray => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("mirror_array"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::InstanceArray => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("instance_array"),
                        vec![wrapper_value, point],
                        span,
                    ),
                };
                self.lower_field_local_distance_scene(field, inner, local_point, span)
            }
            scene_ir::FieldNode::Primitive { primitive, args } => {
                let Some(args) = args.as_ref() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                self.lower_field_primitive_distance_scene(*primitive, args, point, span)
            }
            scene_ir::FieldNode::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let mut current =
                    self.lower_field_local_distance_scene(field, first, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_local_distance_scene(field, item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_union"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::FieldNode::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let mut current =
                    self.lower_field_local_distance_scene(field, first, point.clone(), span);
                for item in iter {
                    let rhs =
                        self.lower_field_local_distance_scene(field, item, point.clone(), span);
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new("field_intersection"),
                        vec![current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::FieldNode::Subtract { left, right } => {
                let left =
                    self.lower_field_local_distance_scene(field, left, point.clone(), span);
                let right = self.lower_field_local_distance_scene(field, right, point, span);
                self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("field_subtract"),
                    vec![left, right],
                    span,
                )
            }
            scene_ir::FieldNode::Smooth {
                kind,
                smoothing,
                items,
            } => {
                let Some(first) = items.first() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let Some(smoothing) = smoothing.as_ref() else {
                    return self.lower_field_local_distance_scene(field, first, point, span);
                };
                let smoothing_value = self.lower_scene_value_expr(smoothing, span);
                let mut current =
                    self.lower_field_local_distance_scene(field, first, point.clone(), span);
                for item in items.iter().skip(1) {
                    let rhs =
                        self.lower_field_local_distance_scene(field, item, point.clone(), span);
                    let callee = match kind {
                        scene_ir::SmoothKind::Union => "field_smooth_union",
                        scene_ir::SmoothKind::Intersection => "field_smooth_intersection",
                        scene_ir::SmoothKind::Subtract => "field_smooth_subtract",
                    };
                    current = self.lower_call_temp(
                        MirType::Float,
                        SmolStr::new(callee),
                        vec![smoothing_value.clone(), current, rhs],
                        span,
                    );
                }
                current
            }
            scene_ir::FieldNode::Extrude { height, profile } => {
                let (Some(height), Some(profile)) = (height.as_ref(), profile.as_ref()) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let height_value = self.lower_scene_value_expr(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let profile_x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let profile_z = self.lower_get_component(point.clone(), MirType::Vec3, "z", span);
                let profile_point = self.lower_vec2_value(profile_x, profile_z, span);
                let profile_distance =
                    self.lower_scene_profile_distance_expr(profile, profile_point, span);
                let point_y = self.lower_get_component(point, MirType::Vec3, "y", span);
                let abs_y = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![point_y],
                    span,
                );
                let axial = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    abs_y,
                    half_height,
                    span,
                );
                self.lower_profile_cap_distance_value(profile_distance, axial, span)
            }
            scene_ir::FieldNode::Revolve { profile } => {
                let Some(profile) = profile.as_ref() else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let y = self.lower_get_component(point.clone(), MirType::Vec3, "y", span);
                let z = self.lower_get_component(point, MirType::Vec3, "z", span);
                let radial_input = self.lower_vec2_value(x, z, span);
                let radial = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("length"),
                    vec![radial_input],
                    span,
                );
                let profile_point = self.lower_vec2_value(radial, y, span);
                self.lower_scene_profile_distance_expr(profile, profile_point, span)
            }
            scene_ir::FieldNode::Sweep { path, profile } => {
                let (Some(path), Some(profile)) = (path.as_ref(), profile.as_ref()) else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let path_value = self.lower_scene_value_expr(path, span);
                let coords = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("field_sweep_coords"),
                    vec![path_value.clone(), point],
                    span,
                );
                let coords_x = self.lower_get_component(coords.clone(), MirType::Vec3, "x", span);
                let coords_y = self.lower_get_component(coords.clone(), MirType::Vec3, "y", span);
                let profile_point = self.lower_vec2_value(coords_x, coords_y, span);
                let profile_distance =
                    self.lower_scene_profile_distance_expr(profile, profile_point, span);
                let coords_z = self.lower_get_component(coords, MirType::Vec3, "z", span);
                let path_length = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("length"),
                    vec![path_value],
                    span,
                );
                let half_length = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    path_length,
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let abs_z = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![coords_z],
                    span,
                );
                let axial = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    abs_z,
                    half_length,
                    span,
                );
                self.lower_profile_cap_distance_value(profile_distance, axial, span)
            }
            scene_ir::FieldNode::Loft { height, from, to } => {
                let (Some(height), Some(from), Some(to)) =
                    (height.as_ref(), from.as_ref(), to.as_ref())
                else {
                    return Value::Const(Literal::Float(1_000_000.0));
                };
                let height_value = self.lower_scene_value_expr(height, span);
                let abs_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![height_value.clone()],
                    span,
                );
                let half_height = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Mul,
                    abs_height.clone(),
                    Value::Const(Literal::Float(0.5)),
                    span,
                );
                let safe_height = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("max"),
                    vec![abs_height, Value::Const(Literal::Float(0.0001))],
                    span,
                );
                let y = self.lower_get_component(point.clone(), MirType::Vec3, "y", span);
                let y_plus_half = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Add,
                    y.clone(),
                    half_height.clone(),
                    span,
                );
                let unclamped_t = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Div,
                    y_plus_half,
                    safe_height,
                    span,
                );
                let clamped = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("clamp"),
                    vec![
                        unclamped_t,
                        Value::Const(Literal::Float(0.0)),
                        Value::Const(Literal::Float(1.0)),
                    ],
                    span,
                );
                let point_x = self.lower_get_component(point.clone(), MirType::Vec3, "x", span);
                let point_z = self.lower_get_component(point, MirType::Vec3, "z", span);
                let profile_point = self.lower_vec2_value(point_x, point_z, span);
                let from_distance =
                    self.lower_scene_profile_distance_expr(from, profile_point.clone(), span);
                let to_distance =
                    self.lower_scene_profile_distance_expr(to, profile_point, span);
                let mixed = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("mix"),
                    vec![from_distance, to_distance, clamped],
                    span,
                );
                let abs_y = self.lower_call_temp(
                    MirType::Float,
                    SmolStr::new("abs"),
                    vec![y],
                    span,
                );
                let cap = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    abs_y,
                    half_height,
                    span,
                );
                self.lower_profile_cap_distance_value(mixed, cap, span)
            }
            scene_ir::FieldNode::OpaqueLeaf => self.lower_field_distance_call(field, point, span),
        }
    }

    pub(crate) fn lower_field_local_normal_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let Some(scene) = self.field_scene(field).cloned() else {
            return self.lower_field_normal_call(field, point, span);
        };
        self.lower_field_local_normal_scene(field, &scene.root, point, span)
    }

    pub(crate) fn lower_field_local_normal_expr(
        &mut self,
        field: &SmolStr,
        expr: &hir::FieldExpr,
        body: &hir::Body,
        point: Value,
        span: TextRange,
    ) -> Value {
        match expr {
            hir::FieldExpr::Use { target } => {
                self.lower_field_local_normal_call(target, point, span)
            }
            hir::FieldExpr::Translate {
                translate,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("translate", "offset", translate, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Rotate {
                rotate,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "field_rotate_point",
                    "rotation",
                    rotate,
                    point,
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::UniformScale { scale, body: inner } => {
                let wrapper_value = self.lower_wrapped_body_value(scale, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("uniform_scale"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::AffineTransform {
                transform,
                body: inner,
            } => {
                let local_point = self.lower_wrapped_support_point(
                    "affine_transform",
                    "transform",
                    transform,
                    point,
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Warp { warp, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("warp", "warp", warp, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatLinear {
                repeat,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(repeat, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("repeat_linear"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::RepeatGrid {
                repeat,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(repeat, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("repeat_grid"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::RadialRepeat {
                radial,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(radial, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("radial_repeat"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::MirrorArray {
                mirror,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(mirror, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("mirror_array"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::InstanceArray {
                instance,
                body: inner,
            } => {
                let wrapper_value = self.lower_wrapped_body_value(instance, span);
                let local_point = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("instance_array"),
                    vec![wrapper_value, point],
                    span,
                );
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Bend { bend, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("bend", "bend", bend, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Twist { twist, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("twist", "twist", twist, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Taper { taper, body: inner } => {
                let local_point =
                    self.lower_wrapped_support_point("taper", "taper", taper, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Displace {
                displace,
                body: inner,
            } => {
                let local_point =
                    self.lower_wrapped_support_point("displace", "displace", displace, point, span);
                self.lower_field_local_normal_expr(field, inner, body, local_point, span)
            }
            hir::FieldExpr::Primitive { primitive, args } => {
                let dx = self.lower_field_primitive_normal_axis_difference(
                    *primitive,
                    args,
                    body,
                    point.clone(),
                    [0.001, 0.0, 0.0],
                    span,
                );
                let dy = self.lower_field_primitive_normal_axis_difference(
                    *primitive,
                    args,
                    body,
                    point.clone(),
                    [0.0, 0.001, 0.0],
                    span,
                );
                let dz = self.lower_field_primitive_normal_axis_difference(
                    *primitive,
                    args,
                    body,
                    point,
                    [0.0, 0.0, 0.001],
                    span,
                );
                let gradient = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![dx, dy, dz],
                    span,
                );
                self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("normalize"),
                    vec![gradient],
                    span,
                )
            }
            _ => self.lower_field_normal_call(field, point, span),
        }
    }

    pub(crate) fn lower_field_local_normal_scene(
        &mut self,
        field: &SmolStr,
        node: &scene_ir::FieldNode,
        point: Value,
        span: TextRange,
    ) -> Value {
        match node {
            scene_ir::FieldNode::Use { target } => {
                self.lower_field_local_normal_call(target, point, span)
            }
            scene_ir::FieldNode::Transform { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return self.lower_field_local_normal_scene(field, inner, point, span);
                };
                let local_point = match kind {
                    scene_ir::TransformKind::Translate => {
                        self.lower_scene_wrapped_support_point("translate", param, point, span)
                    }
                    scene_ir::TransformKind::Rotate => self.lower_scene_wrapped_support_point(
                        "field_rotate_point",
                        param,
                        point,
                        span,
                    ),
                    scene_ir::TransformKind::UniformScale => {
                        let wrapper_value = self.lower_scene_value_expr(param, span);
                        self.lower_call_temp(
                            MirType::Vec3,
                            SmolStr::new("uniform_scale"),
                            vec![wrapper_value, point],
                            span,
                        )
                    }
                    scene_ir::TransformKind::AffineTransform => self
                        .lower_scene_wrapped_support_point("affine_transform", param, point, span),
                    scene_ir::TransformKind::Warp => {
                        self.lower_scene_wrapped_support_point("warp", param, point, span)
                    }
                    scene_ir::TransformKind::Bend => {
                        self.lower_scene_wrapped_support_point("bend", param, point, span)
                    }
                    scene_ir::TransformKind::Twist => {
                        self.lower_scene_wrapped_support_point("twist", param, point, span)
                    }
                    scene_ir::TransformKind::Taper => {
                        self.lower_scene_wrapped_support_point("taper", param, point, span)
                    }
                    scene_ir::TransformKind::Displace => {
                        self.lower_scene_wrapped_support_point("displace", param, point, span)
                    }
                };
                self.lower_field_local_normal_scene(field, inner, local_point, span)
            }
            scene_ir::FieldNode::Repeat { kind, param, inner } => {
                let Some(param) = param.as_ref() else {
                    return self.lower_field_local_normal_scene(field, inner, point, span);
                };
                let wrapper_value = self.lower_scene_value_expr(param, span);
                let local_point = match kind {
                    scene_ir::RepeatKind::RepeatLinear => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("repeat_linear"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::RepeatGrid => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("repeat_grid"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::RadialRepeat => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("radial_repeat"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::MirrorArray => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("mirror_array"),
                        vec![wrapper_value, point],
                        span,
                    ),
                    scene_ir::RepeatKind::InstanceArray => self.lower_call_temp(
                        MirType::Vec3,
                        SmolStr::new("instance_array"),
                        vec![wrapper_value, point],
                        span,
                    ),
                };
                self.lower_field_local_normal_scene(field, inner, local_point, span)
            }
            _ => {
                let dx_plus_point = self.lower_offset_point(point.clone(), [0.001, 0.0, 0.0], span);
                let dx_plus =
                    self.lower_field_local_distance_scene(field, node, dx_plus_point, span);
                let dx_minus_point =
                    self.lower_offset_point(point.clone(), [-0.001, 0.0, 0.0], span);
                let dx_minus =
                    self.lower_field_local_distance_scene(field, node, dx_minus_point, span);
                let dx = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    dx_plus,
                    dx_minus,
                    span,
                );
                let dy_plus_point = self.lower_offset_point(point.clone(), [0.0, 0.001, 0.0], span);
                let dy_plus =
                    self.lower_field_local_distance_scene(field, node, dy_plus_point, span);
                let dy_minus_point =
                    self.lower_offset_point(point.clone(), [0.0, -0.001, 0.0], span);
                let dy_minus =
                    self.lower_field_local_distance_scene(field, node, dy_minus_point, span);
                let dy = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    dy_plus,
                    dy_minus,
                    span,
                );
                let dz_plus_point = self.lower_offset_point(point.clone(), [0.0, 0.0, 0.001], span);
                let dz_plus =
                    self.lower_field_local_distance_scene(field, node, dz_plus_point, span);
                let dz_minus_point =
                    self.lower_offset_point(point, [0.0, 0.0, -0.001], span);
                let dz_minus =
                    self.lower_field_local_distance_scene(field, node, dz_minus_point, span);
                let dz = self.lower_binary_temp(
                    MirType::Float,
                    BinaryOp::Sub,
                    dz_plus,
                    dz_minus,
                    span,
                );
                let gradient = self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("vec3"),
                    vec![dx, dy, dz],
                    span,
                );
                self.lower_call_temp(
                    MirType::Vec3,
                    SmolStr::new("normalize"),
                    vec![gradient],
                    span,
                )
            }
        }
    }

    pub(crate) fn lower_field_primitive_normal_axis_difference(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[hir::Arg],
        body: &hir::Body,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_field_primitive_distance(primitive, args, body, plus_point, span);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_field_primitive_distance(primitive, args, body, minus_point, span);
        self.lower_binary_temp(MirType::Float, BinaryOp::Sub, plus, minus, span)
    }

    pub(crate) fn lower_field_primitive_normal_axis_difference_scene(
        &mut self,
        primitive: hir::FieldPrimitive,
        args: &[scene_ir::SceneArgExpr],
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_field_primitive_distance_scene(primitive, args, plus_point, span);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_field_primitive_distance_scene(primitive, args, minus_point, span);
        self.lower_binary_temp(MirType::Float, BinaryOp::Sub, plus, minus, span)
    }

    pub(crate) fn lower_radiance_call(
        &mut self,
        radiance: &SmolStr,
        point: Value,
        direction: Value,
        feature_id: Value,
        span: TextRange,
    ) -> Value {
        match self
            .radiance_param_counts
            .get(radiance)
            .copied()
            .unwrap_or(1)
        {
            1 => self.lower_call_temp(MirType::Vec3, radiance.clone(), vec![point], span),
            2 => self.lower_call_temp(
                MirType::Vec3,
                radiance.clone(),
                vec![point, direction],
                span,
            ),
            _ => self.lower_call_temp(
                MirType::Vec3,
                radiance.clone(),
                vec![point, direction, feature_id],
                span,
            ),
        }
    }

    pub(crate) fn lower_volume_call(
        &mut self,
        volume: &SmolStr,
        point: Value,
        surface_distance: Value,
        span: TextRange,
    ) -> Value {
        match self.volume_param_counts.get(volume).copied().unwrap_or(1) {
            1 => self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                volume.clone(),
                vec![point],
                span,
            ),
            _ => self.lower_call_temp(
                MirType::Named(SmolStr::new("Medium")),
                volume.clone(),
                vec![point, surface_distance],
                span,
            ),
        }
    }

    pub(crate) fn lower_get_named_field(
        &mut self,
        base: Value,
        type_name: &str,
        field: &str,
        ty: MirType,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::GetField {
                base,
                field: SmolStr::new(field),
                slot: self.field_slot(type_name, field),
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_field_distance_call(
        &mut self,
        field: &SmolStr,
        point: Value,
        span: TextRange,
    ) -> Value {
        let _ = self.lower_call_temp(
            MirType::Nil,
            SmolStr::new("__wr_metrics_field_sample"),
            vec![],
            span,
        );
        let temp = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(field.clone()),
                args: vec![point],
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_field_normal_call(&mut self, field: &SmolStr, point: Value, span: TextRange) -> Value {
        let dx = self.lower_field_axis_difference(field, point.clone(), [0.001, 0.0, 0.0], span);
        let dy = self.lower_field_axis_difference(field, point.clone(), [0.0, 0.001, 0.0], span);
        let dz = self.lower_field_axis_difference(field, point, [0.0, 0.0, 0.001], span);

        let gradient = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(gradient),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("vec3")),
                args: vec![dx, dy, dz],
            },
            span,
        });

        let normal = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(normal),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("normalize")),
                args: vec![Value::Temp(gradient)],
            },
            span,
        });
        Value::Temp(normal)
    }

    pub(crate) fn lower_field_axis_difference(
        &mut self,
        field: &SmolStr,
        point: Value,
        offset: [f64; 3],
        span: TextRange,
    ) -> Value {
        let plus_point = self.lower_offset_point(point.clone(), offset, span);
        let plus = self.lower_field_distance_call(field, plus_point, span);
        let minus_point =
            self.lower_offset_point(point, [-offset[0], -offset[1], -offset[2]], span);
        let minus = self.lower_field_distance_call(field, minus_point, span);
        let diff = self.new_temp(MirType::Float);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(diff),
            value: Rvalue::Binary {
                op: BinaryOp::Sub,
                lhs: plus,
                rhs: minus,
            },
            span,
        });
        Value::Temp(diff)
    }

    pub(crate) fn lower_offset_point(&mut self, point: Value, offset: [f64; 3], span: TextRange) -> Value {
        let offset_vec = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(offset_vec),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("vec3")),
                args: vec![
                    Value::Const(Literal::Float(offset[0])),
                    Value::Const(Literal::Float(offset[1])),
                    Value::Const(Literal::Float(offset[2])),
                ],
            },
            span,
        });
        let shifted = self.new_temp(MirType::Vec3);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(shifted),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: point,
                rhs: Value::Temp(offset_vec),
            },
            span,
        });
        Value::Temp(shifted)
    }
}
