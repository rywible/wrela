//! Owns MIR lowering for scene-medium capture helpers plus shared bridge/domain
//! helpers reused by scene and world capture lowering.
//! Does not own shape helper lowering or high-level query planning.
//!
//! Key invariants:
//! - bridge helper lowering must preserve the backend/region/domain evidence
//!   needed by runtime fallback and observability.
//! - world-auto backend normalization happens here so sibling lowering modules
//!   agree on the same execution choice.
//! - capture-index and domain-flag plumbing must stay ABI-compatible with the
//!   shared WGSL/native bridge records.
//!
//! Primary entrypoints:
//! - `lower_scene_medium_capture_helper`
//! - `lower_world_domain_validation`
//! - `lower_wgsl_bridge_failure`
//!
//! Failure modes / common pitfalls:
//! - duplicating bridge failure logic in sibling modules makes backend behavior
//!   diverge subtly.
//! - treating domain flags as optional here can allow invalid world dispatches
//!   to escape the lowering layer.

use super::{
    BinaryOp, DispatchBackend, FunctionLowerer, HashMap, HashSet, Literal, MirFunction, MirStmt,
    MirType, NativeWgslBridgeConfig, PortableAbiType, SmolStr, TextRange, TypeTagId, UnaryOp,
    Value, WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldRadianceBackend,
    WorldSurfaceBackend, WorldTraceBackend, portable_abi_from_type_ref, query_contract,
    stable_shape_capture_id, world_domain_mismatch_message,
};
use crate::hir;
use crate::mir::ir::*;
use crate::query_exec::region::build_region_exec_cases;

pub(crate) fn lower_scene_medium_capture_helper(
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
) -> MirFunction {
    let helper_name = SmolStr::new("__wr_scene_medium_capture");
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        None,
    );

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "medium_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(
        SmolStr::new("$scene_medium_result"),
        true,
        MirType::Named(SmolStr::new("Medium")),
    );
    let default_medium = lowerer.build_default_medium(span);
    lowerer.assign_use(Place::Local(result), default_medium, span);
    let return_block = lowerer.new_block();

    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let shape_scene = lowerer
            .shape_scene(&shape_name)
            .cloned()
            .expect("shape scene");
        let medium = lowerer.lower_shape_medium_participation_scene(
            &shape_scene.root,
            Value::Local(point),
            span,
        );
        lowerer.assign_use(Place::Local(result), medium, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "medium_at requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Medium"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(super) fn lower_world_domain_validation(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    domain: LocalId,
    query_name: &str,
    span: TextRange,
) -> (Value, Value) {
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                world_domain_mismatch_message(query_name),
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let spatial = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "spatial",
        MirType::Named(SmolStr::new("SpatialDomainContract")),
        span,
    );
    let detail = lowerer.lower_get_named_field(
        spatial,
        "SpatialDomainContract",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    (capture_scene_id, detail)
}

pub(super) fn lower_world_domain_flag_guard(
    lowerer: &mut FunctionLowerer,
    domain: LocalId,
    flag: &str,
    disabled_return: Value,
    span: TextRange,
) {
    let (contract_name, contract_field) = match flag {
        "material" => ("SurfaceDomainContract", "surface"),
        "radiance" | "media" => ("ParticipantDomainContract", "participants"),
        other => panic!("unknown SceneDomain flag '{other}'"),
    };
    let contract = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        contract_field,
        MirType::Named(SmolStr::new(contract_name)),
        span,
    );
    let enabled =
        lowerer.lower_get_named_field(contract, contract_name, flag, MirType::Boolean, span);
    let enabled_block = lowerer.new_block();
    let disabled_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: enabled,
        then_target: enabled_block,
        else_target: disabled_block,
        span,
    });

    lowerer.current_block = disabled_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(disabled_return),
        span,
    });

    lowerer.current_block = enabled_block;
}

pub(super) fn lower_world_region_dispatch<F>(
    lowerer: &mut FunctionLowerer,
    module: &hir::Module,
    capture_scene_id: Value,
    detail: Value,
    return_block: BlockId,
    invalid_message: &str,
    span: TextRange,
    mut emit_shapes: F,
) where
    F: FnMut(&mut FunctionLowerer, &[SmolStr], TextRange),
{
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for region_case in build_region_exec_cases(module) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(i64::from(region_case.scene_id))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });

        lowerer.current_block = match_block;
        let (coarse_shapes, fine_shapes) = match &region_case.shapes {
            Ok(shapes) => (&shapes.coarse, &shapes.fine),
            Err(message) => {
                let crash_temp = lowerer.new_temp(MirType::Unknown);
                lowerer.push_stmt(MirStmt::Assign {
                    place: Place::Temp(crash_temp),
                    value: Rvalue::Crash {
                        value: Value::Const(Literal::String(message.clone())),
                    },
                    span,
                });
                lowerer.set_terminator(Terminator::Return {
                    value: Some(Value::Temp(crash_temp)),
                    span,
                });
                dispatch_block = next_block;
                continue;
            }
        };

        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });

        for (shapes, block) in [(coarse_shapes, coarse_block), (fine_shapes, fine_block)] {
            lowerer.current_block = block;
            emit_shapes(lowerer, shapes, span);
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }

        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(invalid_message))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });
}

pub(super) struct MirWorldDistanceBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) point: Value,
    pub(super) result: LocalId,
    pub(super) span: TextRange,
}

impl WorldDistanceBackend for MirWorldDistanceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_distance(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let distance = self
            .lowerer
            .lower_shape_distance_call(shape, self.point.clone(), self.span);
        let next = self.lowerer.lower_call_temp(
            MirType::Float,
            SmolStr::new("min"),
            vec![Value::Local(self.result), distance],
            self.span,
        );
        self.lowerer
            .assign_use(Place::Local(self.result), next, self.span);
        Ok(())
    }
}

pub(super) struct MirWorldNormalBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) capture: LocalId,
    pub(super) domain: LocalId,
    pub(super) point: LocalId,
    pub(super) backend: LocalId,
    pub(super) span: TextRange,
}

impl WorldNormalBackend for MirWorldNormalBackend<'_> {
    type Error = std::convert::Infallible;
    type Point = Value;
    type Distance = Value;
    type Normal = Value;

    fn base_point(&mut self) -> Result<Self::Point, Self::Error> {
        Ok(Value::Local(self.point))
    }

    fn offset_point(
        &mut self,
        point: &Self::Point,
        axis: usize,
        delta: f32,
    ) -> Result<Self::Point, Self::Error> {
        let offset = match axis {
            0 => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(f64::from(delta))),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                ],
                self.span,
            ),
            1 => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(f64::from(delta))),
                    Value::Const(Literal::Float(0.0)),
                ],
                self.span,
            ),
            _ => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(f64::from(delta))),
                ],
                self.span,
            ),
        };
        Ok(self.lowerer.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Add,
            point.clone(),
            offset,
            self.span,
        ))
    }

    fn sample_world_distance(&mut self, point: Self::Point) -> Result<Self::Distance, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_world_distance_capture"),
            vec![
                Value::Local(self.capture),
                Value::Local(self.domain),
                point,
                Value::Local(self.backend),
            ],
            self.span,
        ))
    }

    fn subtract_distance(
        &mut self,
        positive: Self::Distance,
        negative: Self::Distance,
    ) -> Result<Self::Distance, Self::Error> {
        Ok(self.lowerer.lower_binary_temp(
            MirType::Float,
            BinaryOp::Sub,
            positive,
            negative,
            self.span,
        ))
    }

    fn compose_normal(
        &mut self,
        x: Self::Distance,
        y: Self::Distance,
        z: Self::Distance,
    ) -> Result<Self::Normal, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![x, y, z],
            self.span,
        ))
    }

    fn normalize_normal(&mut self, normal: Self::Normal) -> Result<Self::Normal, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![normal],
            self.span,
        ))
    }
}

pub(super) struct MirWorldTraceBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) origin: Value,
    pub(super) direction: Value,
    pub(super) max_distance: Value,
    pub(super) min_step: Value,
    pub(super) hit_epsilon: Value,
    pub(super) max_steps: Value,
    pub(super) result: LocalId,
    pub(super) span: TextRange,
}

impl WorldTraceBackend for MirWorldTraceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_trace(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let candidate = self.lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new(format!("__wr_shape_trace_{}", shape)),
            vec![
                self.origin.clone(),
                self.direction.clone(),
                self.max_distance.clone(),
                self.min_step.clone(),
                self.hit_epsilon.clone(),
                self.max_steps.clone(),
            ],
            self.span,
        );
        let candidate_hit = self.lowerer.lower_get_named_field(
            candidate.clone(),
            "Hit3",
            "hit",
            MirType::Boolean,
            self.span,
        );
        let current_hit = self.lowerer.lower_get_named_field(
            Value::Local(self.result),
            "Hit3",
            "hit",
            MirType::Boolean,
            self.span,
        );
        let candidate_distance = self.lowerer.lower_get_named_field(
            candidate.clone(),
            "Hit3",
            "distance",
            MirType::Float,
            self.span,
        );
        let current_distance = self.lowerer.lower_get_named_field(
            Value::Local(self.result),
            "Hit3",
            "distance",
            MirType::Float,
            self.span,
        );
        let current_miss =
            self.lowerer
                .lower_unary_temp(MirType::Boolean, UnaryOp::Not, current_hit, self.span);
        let candidate_nearer = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            candidate_distance,
            current_distance,
            self.span,
        );
        let replace = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Or,
            current_miss,
            candidate_nearer,
            self.span,
        );
        let should_take = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::And,
            candidate_hit,
            replace,
            self.span,
        );
        let take_block = self.lowerer.new_block();
        let skip_block = self.lowerer.new_block();
        let merge_block = self.lowerer.new_block();
        self.lowerer.set_terminator(Terminator::Branch {
            cond: should_take,
            then_target: take_block,
            else_target: skip_block,
            span: self.span,
        });
        self.lowerer.current_block = take_block;
        self.lowerer
            .assign_use(Place::Local(self.result), candidate, self.span);
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = skip_block;
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = merge_block;
        Ok(())
    }
}

pub(super) struct MirWorldSurfaceBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) hit: LocalId,
    pub(super) root_shape_id: Value,
    pub(super) result: LocalId,
    pub(super) span: TextRange,
}

impl WorldSurfaceBackend for MirWorldSurfaceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_surface(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let match_block = self.lowerer.new_block();
        let next_block = self.lowerer.new_block();
        let matched = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            self.root_shape_id.clone(),
            Value::Const(Literal::Integer(stable_shape_capture_id(shape))),
            self.span,
        );
        self.lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span: self.span,
        });
        self.lowerer.current_block = match_block;
        let surface = self.lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{}", shape)),
            vec![Value::Local(self.hit)],
            self.span,
        );
        self.lowerer
            .assign_use(Place::Local(self.result), surface, self.span);
        let merge_block = self.lowerer.new_block();
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = next_block;
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = merge_block;
        Ok(())
    }
}

pub(super) struct MirWorldRadianceBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) point: Value,
    pub(super) direction: Value,
    pub(super) result: LocalId,
    pub(super) span: TextRange,
}

impl WorldRadianceBackend for MirWorldRadianceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(scene) = self.lowerer.shape_scene(shape).cloned() {
            let radiance = self.lowerer.lower_shape_radiance_participation_scene(
                &scene.root,
                self.point.clone(),
                self.direction.clone(),
                self.span,
            );
            let sum = self.lowerer.lower_binary_temp(
                MirType::Vec3,
                BinaryOp::Add,
                Value::Local(self.result),
                radiance,
                self.span,
            );
            self.lowerer
                .assign_use(Place::Local(self.result), sum, self.span);
        }
        Ok(())
    }
}

pub(super) struct MirWorldMediumBackend<'a> {
    pub(super) lowerer: &'a mut FunctionLowerer,
    pub(super) point: LocalId,
    pub(super) result: LocalId,
    pub(super) span: TextRange,
}

impl WorldMediumBackend for MirWorldMediumBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_medium(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(scene) = self.lowerer.shape_scene(shape).cloned() {
            let medium = self.lowerer.lower_shape_medium_participation_scene(
                &scene.root,
                Value::Local(self.point),
                self.span,
            );
            let merged = self.lowerer.lower_additive_medium_combine(
                Value::Local(self.result),
                medium,
                self.span,
            );
            self.lowerer
                .assign_use(Place::Local(self.result), merged, self.span);
        }
        Ok(())
    }
}

pub(super) fn lower_wgsl_bridge_failure(
    lowerer: &mut FunctionLowerer,
    message: SmolStr,
    span: TextRange,
) {
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(message)),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });
}

fn world_auto_backend(default_backend: DispatchBackend) -> DispatchBackend {
    match default_backend {
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Cpu | DispatchBackend::VirtualGpu | DispatchBackend::Auto => {
            DispatchBackend::Cpu
        }
    }
}

pub(super) fn batch_auto_backend(default_backend: DispatchBackend) -> DispatchBackend {
    match default_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Auto => DispatchBackend::Cpu,
    }
}

fn lower_runtime_u32_list(
    lowerer: &mut FunctionLowerer,
    debug_name: &str,
    values: &[u32],
    span: TextRange,
) -> Value {
    let local = lowerer.new_local(
        SmolStr::new(format!("{debug_name}{}", lowerer.locals.len())),
        true,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(local),
        value: Rvalue::BuildList {
            items: values
                .iter()
                .map(|value| Value::Const(Literal::Integer(i64::from(*value))))
                .collect(),
            alloc: AllocKind::Escaping,
        },
        span,
    });
    Value::Local(local)
}

fn lower_world_shape_index_list(
    lowerer: &mut FunctionLowerer,
    shapes: &[SmolStr],
    shape_indices: &HashMap<SmolStr, u32>,
    span: TextRange,
) -> Value {
    let values = shapes
        .iter()
        .map(|shape| {
            *shape_indices
                .get(shape)
                .unwrap_or_else(|| panic!("missing WGSL shape index for world shape '{}'", shape))
        })
        .collect::<Vec<_>>();
    lower_runtime_u32_list(lowerer, "$world_shape_indices", &values, span)
}

fn lower_capture_index_lookup(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    capture_type: &str,
    capture_field: &str,
    capture_indices: &HashMap<SmolStr, u32>,
    stable_capture_id: fn(&SmolStr) -> i64,
    invalid_message: &str,
    span: TextRange,
) -> Value {
    let capture_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        capture_type,
        capture_field,
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new(format!("$capture_index{}", lowerer.locals.len())),
        true,
        MirType::Integer,
    );
    let join_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    let mut cases = capture_indices
        .iter()
        .map(|(name, index)| (stable_capture_id(name), *index))
        .collect::<Vec<_>>();
    cases.sort_by_key(|(stable_id, _)| *stable_id);
    for (stable_id, index) in cases {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_id.clone(),
            Value::Const(Literal::Integer(stable_id)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });

        lowerer.current_block = match_block;
        lowerer.assign_use(
            Place::Local(result),
            Value::Const(Literal::Integer(i64::from(index))),
            span,
        );
        lowerer.set_terminator(Terminator::Jump {
            target: join_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    lower_wgsl_bridge_failure(lowerer, SmolStr::new(invalid_message), span);
    lowerer.current_block = join_block;
    Value::Local(result)
}

pub(super) fn lower_world_wgsl_bridge_call(
    lowerer: &mut FunctionLowerer,
    result_type: MirType,
    config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    bridge_symbol: &str,
    shapes: &[SmolStr],
    shape_indices: &HashMap<SmolStr, u32>,
    args: Vec<Value>,
    span: TextRange,
) -> Option<Value> {
    let config = match config {
        Some(Ok(config)) => config,
        Some(Err(err)) => {
            lower_wgsl_bridge_failure(lowerer, err.clone(), span);
            return None;
        }
        None => {
            lower_wgsl_bridge_failure(
                lowerer,
                SmolStr::new(format!("missing WGSL bridge config for {bridge_symbol}")),
                span,
            );
            return None;
        }
    };
    let world_shape_indices = lower_world_shape_index_list(lowerer, shapes, shape_indices, span);
    let mut call_args = vec![
        Value::Const(Literal::String(config.source.clone())),
        Value::Const(Literal::Integer(config.workgroup_size)),
        world_shape_indices,
    ];
    call_args.extend(args);
    Some(lowerer.lower_call_temp(result_type, SmolStr::new(bridge_symbol), call_args, span))
}

fn lower_world_domain_flag_value(
    lowerer: &mut FunctionLowerer,
    domain: LocalId,
    flag: &str,
    span: TextRange,
) -> Value {
    let (contract_name, contract_field) = match flag {
        "material" => ("SurfaceDomainContract", "surface"),
        "radiance" | "media" => ("ParticipantDomainContract", "participants"),
        other => panic!("unknown SceneDomain flag '{other}'"),
    };
    let contract = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        contract_field,
        MirType::Named(SmolStr::new(contract_name)),
        span,
    );
    lowerer.lower_get_named_field(contract, contract_name, flag, MirType::Boolean, span)
}

pub(super) fn lower_world_batch_wgsl_bridge_call(
    lowerer: &mut FunctionLowerer,
    config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    contract_id: query_contract::QueryContractId,
    shapes: &[SmolStr],
    shape_indices: &HashMap<SmolStr, u32>,
    domain: LocalId,
    items: LocalId,
    span: TextRange,
) -> Option<Value> {
    let config = match config {
        Some(Ok(config)) => config,
        Some(Err(err)) => {
            lower_wgsl_bridge_failure(lowerer, err.clone(), span);
            return None;
        }
        None => {
            lower_wgsl_bridge_failure(
                lowerer,
                SmolStr::new(format!(
                    "missing WGSL bridge config for {}",
                    contract_id.as_str()
                )),
                span,
            );
            return None;
        }
    };
    let world_shape_indices = lower_world_shape_index_list(lowerer, shapes, shape_indices, span);
    let material = lower_world_domain_flag_value(lowerer, domain, "material", span);
    let radiance = lower_world_domain_flag_value(lowerer, domain, "radiance", span);
    let media = lower_world_domain_flag_value(lowerer, domain, "media", span);
    Some(lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new("__wr_wgsl_world_batch_queries"),
        vec![
            Value::Const(Literal::String(config.source.clone())),
            Value::Const(Literal::Integer(config.workgroup_size)),
            Value::Const(Literal::String(SmolStr::new(contract_id.as_str()))),
            world_shape_indices,
            material,
            radiance,
            media,
            Value::Local(items),
        ],
        span,
    ))
}

pub(super) fn lower_batch_wgsl_bridge_call(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    items: LocalId,
    config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    bridge_symbol: &str,
    capture_type: &str,
    capture_field: &str,
    capture_indices: &HashMap<SmolStr, u32>,
    stable_capture_id: fn(&SmolStr) -> i64,
    invalid_message: &str,
    span: TextRange,
) -> Option<Value> {
    let config = match config {
        Some(Ok(config)) => config,
        Some(Err(err)) => {
            lower_wgsl_bridge_failure(lowerer, err.clone(), span);
            return None;
        }
        None => {
            lower_wgsl_bridge_failure(
                lowerer,
                SmolStr::new(format!("missing WGSL bridge config for {bridge_symbol}")),
                span,
            );
            return None;
        }
    };
    let capture_index = lower_capture_index_lookup(
        lowerer,
        capture,
        capture_type,
        capture_field,
        capture_indices,
        stable_capture_id,
        invalid_message,
        span,
    );
    Some(lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new(bridge_symbol),
        vec![
            Value::Const(Literal::String(config.source.clone())),
            Value::Const(Literal::Integer(config.workgroup_size)),
            capture_index,
            Value::Local(items),
        ],
        span,
    ))
}

pub(super) fn lower_native_world_backend_guard(
    lowerer: &mut FunctionLowerer,
    backend: LocalId,
    auto_backend: DispatchBackend,
    cpu_block: BlockId,
    wgsl_block: BlockId,
    unsupported_block: BlockId,
    span: TextRange,
) {
    let is_cpu = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(0)),
        span,
    );
    let is_auto = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(3)),
        span,
    );
    let is_wgsl = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(2)),
        span,
    );
    let auto_target = world_auto_backend(auto_backend);
    let auto_block = lowerer.new_block();
    let backend_check_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: is_auto,
        then_target: auto_block,
        else_target: backend_check_block,
        span,
    });

    lowerer.current_block = auto_block;
    lowerer.set_terminator(Terminator::Jump {
        target: match auto_target {
            DispatchBackend::Wgsl => wgsl_block,
            DispatchBackend::Cpu | DispatchBackend::VirtualGpu | DispatchBackend::Auto => cpu_block,
        },
        span,
    });

    lowerer.current_block = backend_check_block;
    let cpu_or_wgsl = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Or,
        is_cpu.clone(),
        is_wgsl,
        span,
    );
    let direct_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: cpu_or_wgsl,
        then_target: direct_block,
        else_target: unsupported_block,
        span,
    });

    lowerer.current_block = direct_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_cpu,
        then_target: cpu_block,
        else_target: wgsl_block,
        span,
    });

    lowerer.current_block = unsupported_block;
    lower_wgsl_bridge_failure(
        lowerer,
        SmolStr::new("native MIR world queries currently support only cpu, wgsl, or auto backends"),
        span,
    );
}
