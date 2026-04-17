//! Owns synthesized MIR dispatch functions for interface calls.
//! Does not own interface/type checking or authored method bodies.
//!
//! Key invariants:
//! - generated dispatch functions must agree with the interface implementation
//!   map collected at module-lowering time.
//! - dispatch synthesis may normalize calling shape, but it must not change the
//!   semantics of which implementation is selected.
//!
//! Primary entrypoints:
//! - `build_interface_dispatch_functions`
//! - `builtin_function_names`
//!
//! Failure modes / common pitfalls:
//! - generating dispatch functions from stale impl maps creates runtime drift
//!   that looks like an expression-lowering bug elsewhere.

use super::*;

pub(super) fn build_interface_dispatch_functions(
    module: &hir::Module,
    interface_impls: &HashMap<SmolStr, Vec<SmolStr>>,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for (_idx, interface) in module.interfaces.iter() {
        let impls = interface_impls
            .get(&interface.name)
            .cloned()
            .unwrap_or_default();
        for method in &interface.methods {
            let params: Vec<SmolStr> = method.params.iter().map(|p| p.name.clone()).collect();
            functions.push(build_interface_dispatch_function(
                &interface.name,
                &method.name,
                &params,
                &impls,
                type_tags,
            ));
        }
    }
    functions
}

fn build_interface_dispatch_function(
    interface: &SmolStr,
    method: &SmolStr,
    params: &[SmolStr],
    impls: &[SmolStr],
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut locals = Vec::new();
    let mut params_ids = Vec::new();
    let mut temps = Vec::new();

    let receiver_id = LocalId(0);
    locals.push(Local {
        name: SmolStr::new("self"),
        mutable: false,
        ty: MirType::Unknown,
    });
    params_ids.push(receiver_id);

    for (idx, name) in params.iter().enumerate() {
        let local_id = LocalId(idx + 1);
        locals.push(Local {
            name: name.clone(),
            mutable: false,
            ty: MirType::Unknown,
        });
        params_ids.push(local_id);
    }

    let mut blocks = Vec::new();
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    let mut cases = Vec::new();
    let mut impls_with_tags = Vec::new();
    for class in impls {
        let Some(tag) = type_tags.get(class) else {
            continue;
        };
        let block_id = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable { span },
        });
        cases.push((SwitchCase::Type(*tag), block_id));
        impls_with_tags.push(class.clone());
    }

    let default_block = BlockId(blocks.len());
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    blocks[0].terminator = Terminator::Switch {
        scrutinee: Value::Local(receiver_id),
        cases,
        default: default_block,
        span,
    };

    let call_args: Vec<Value> = params_ids.iter().map(|id| Value::Local(*id)).collect();

    for (idx, class) in impls_with_tags.iter().enumerate() {
        let block_id = BlockId(idx + 1);
        if block_id.0 >= blocks.len() {
            continue;
        }
        let temp_id = TempId(temps.len());
        temps.push(Temp {
            ty: MirType::Unknown,
        });
        let func_name = SmolStr::new(format!("{}.{}", class, method));
        blocks[block_id.0].stmts.push(MirStmt::Assign {
            place: Place::Temp(temp_id),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(func_name),
                args: call_args.clone(),
            },
            span,
        });
        blocks[block_id.0].terminator = Terminator::Return {
            value: Some(Value::Temp(temp_id)),
            span,
        };
    }

    let crash_temp = TempId(temps.len());
    temps.push(Temp {
        ty: MirType::Unknown,
    });
    blocks[default_block.0].stmts.push(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new("interface dispatch failed"))),
        },
        span,
    });
    blocks[default_block.0].terminator = Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    };

    MirFunction {
        name: SmolStr::new(format!("{}.{}", interface, method)),
        params: params_ids,
        abi_params: vec![PortableAbiType::Value; params.len() + 1],
        abi_return: PortableAbiType::Value,
        locals,
        temps,
        blocks,
        entry: BlockId(0),
        suspendable: false,
    }
}

pub(super) fn builtin_function_names() -> Vec<SmolStr> {
    vec![
        SmolStr::new("__wr_assert_err"),
        SmolStr::new("__wr_print"),
        SmolStr::new("__wr_vec_component"),
        SmolStr::new("__wr_bytes_from_string"),
        SmolStr::new("__wr_bytes_from_list"),
        SmolStr::new("__wr_bytes_to_string"),
        SmolStr::new("__wr_bytes_to_list"),
        SmolStr::new("__wr_bytes_len"),
        SmolStr::new("__wr_fs_read_bytes"),
        SmolStr::new("__wr_fs_write_bytes"),
        SmolStr::new("__wr_external_call"),
        SmolStr::new("__wr_http_call"),
        SmolStr::new("vec2"),
        SmolStr::new("vec3"),
        SmolStr::new("vec4"),
        SmolStr::new("quat"),
        SmolStr::new("mat3_identity"),
        SmolStr::new("mat3_cols"),
        SmolStr::new("mat4_identity"),
        SmolStr::new("mat4_cols"),
        SmolStr::new("f32"),
        SmolStr::new("i32"),
        SmolStr::new("i64"),
        SmolStr::new("u32"),
        SmolStr::new("u64"),
        SmolStr::new("dot"),
        SmolStr::new("length"),
        SmolStr::new("normalize"),
        SmolStr::new("cross"),
        SmolStr::new("min"),
        SmolStr::new("max"),
        SmolStr::new("clamp"),
        SmolStr::new("mix"),
        SmolStr::new("abs"),
        SmolStr::new("sign"),
        SmolStr::new("floor"),
        SmolStr::new("ceil"),
        SmolStr::new("fract"),
        SmolStr::new("sin"),
        SmolStr::new("cos"),
        SmolStr::new("sqrt"),
        SmolStr::new("pow"),
        SmolStr::new("distance"),
        SmolStr::new("reflect"),
        SmolStr::new("bounds2_center"),
        SmolStr::new("bounds2_size"),
        SmolStr::new("bounds3_center"),
        SmolStr::new("bounds3_size"),
        SmolStr::new("transform3_identity"),
        SmolStr::new("transform_point"),
        SmolStr::new("transform_vector"),
        SmolStr::new("transform_normal"),
        SmolStr::new("compose_transform3"),
        SmolStr::new("inverse_transform3"),
        SmolStr::new("translate"),
        SmolStr::new("rotate"),
        SmolStr::new("uniform_scale"),
        SmolStr::new("affine_transform"),
        SmolStr::new("warp"),
        SmolStr::new("repeat_linear"),
        SmolStr::new("repeat_grid"),
        SmolStr::new("radial_repeat"),
        SmolStr::new("mirror_array"),
        SmolStr::new("instance_array"),
        SmolStr::new("field_translate_point"),
        SmolStr::new("field_rotate_point"),
        SmolStr::new("field_uniform_scale_point"),
        SmolStr::new("field_affine_transform_point"),
        SmolStr::new("field_warp_point"),
        SmolStr::new("field_repeat_linear_point"),
        SmolStr::new("field_repeat_grid_point"),
        SmolStr::new("field_radial_repeat_point"),
        SmolStr::new("field_mirror_array_point"),
        SmolStr::new("field_instance_array_point"),
        SmolStr::new("field_sweep_coords"),
        SmolStr::new("field_profile_vertices_bounds4"),
        SmolStr::new("field_smooth_union"),
        SmolStr::new("field_smooth_intersection"),
        SmolStr::new("field_smooth_subtract"),
        SmolStr::new("field_bend_point"),
        SmolStr::new("field_twist_point"),
        SmolStr::new("field_taper_point"),
        SmolStr::new("field_displace_point"),
        SmolStr::new("rounded_box"),
        SmolStr::new("ellipsoid"),
        SmolStr::new("cone"),
        SmolStr::new("capped_cone"),
        SmolStr::new("box_frame"),
        SmolStr::new("slab"),
        SmolStr::new("triangle_prism"),
        SmolStr::new("hex_prism"),
        SmolStr::new("sphere"),
        SmolStr::new("box"),
        SmolStr::new("capsule"),
        SmolStr::new("cylinder"),
        SmolStr::new("plane"),
        SmolStr::new("torus"),
        SmolStr::new("circle2"),
        SmolStr::new("rect2"),
        SmolStr::new("rounded_rect2"),
        SmolStr::new("capsule2"),
        SmolStr::new("segment2"),
        SmolStr::new("polygon2"),
        SmolStr::new("polyline2"),
        SmolStr::new("smooth_union"),
        SmolStr::new("smooth_intersection"),
        SmolStr::new("smooth_subtract"),
        SmolStr::new("__wr_primitive_sphere"),
        SmolStr::new("__wr_primitive_box"),
        SmolStr::new("__wr_primitive_capsule"),
        SmolStr::new("__wr_primitive_cylinder"),
        SmolStr::new("__wr_primitive_plane"),
        SmolStr::new("__wr_primitive_torus"),
        SmolStr::new("field_union"),
        SmolStr::new("field_intersection"),
        SmolStr::new("field_subtract"),
        SmolStr::new("bend"),
        SmolStr::new("twist"),
        SmolStr::new("taper"),
        SmolStr::new("displace"),
        SmolStr::new("__wr_field_distance_capture"),
        SmolStr::new("__wr_field_normal_capture"),
        SmolStr::new("__wr_shape_distance_capture"),
        SmolStr::new("__wr_shape_normal_capture"),
        SmolStr::new("__wr_scene_trace_capture"),
        SmolStr::new("__wr_scene_occluded_capture"),
        SmolStr::new("__wr_scene_surface_capture"),
        SmolStr::new("__wr_scene_radiance_capture"),
        SmolStr::new("__wr_scene_medium_capture"),
        SmolStr::new("__wr_world_occluded_capture"),
        SmolStr::new("__wr_field_distance_batch_queries"),
        SmolStr::new("__wr_shape_distance_batch_queries"),
        SmolStr::new("__wr_field_normal_batch_queries"),
        SmolStr::new("__wr_shape_normal_batch_queries"),
        SmolStr::new("__wr_scene_trace_batch_queries"),
        SmolStr::new("__wr_scene_surface_batch_queries"),
        SmolStr::new("__wr_scene_occluded_batch_queries"),
        SmolStr::new("__wr_scene_trace_queries"),
        SmolStr::new("__wr_scene_surface_queries"),
        SmolStr::new("gpu_buffer_new"),
        SmolStr::new("gpu_buffer_len"),
        SmolStr::new("gpu_buffer_get"),
        SmolStr::new("gpu_buffer_set"),
        SmolStr::new("gpu_atomic_i32_new"),
        SmolStr::new("gpu_atomic_i32_drop"),
        SmolStr::new("gpu_atomic_i32_load"),
        SmolStr::new("gpu_atomic_i32_store"),
        SmolStr::new("gpu_atomic_i32_fetch_add"),
        SmolStr::new("gpu_atomic_u32_new"),
        SmolStr::new("gpu_atomic_u32_drop"),
        SmolStr::new("gpu_atomic_u32_load"),
        SmolStr::new("gpu_atomic_u32_store"),
        SmolStr::new("gpu_atomic_u32_fetch_add"),
        SmolStr::new("global_invocation_id"),
        SmolStr::new("local_invocation_id"),
        SmolStr::new("workgroup_id"),
        SmolStr::new("num_workgroups"),
        SmolStr::new("workgroup_size"),
        SmolStr::new("gpu_schedule_deterministic"),
        SmolStr::new("gpu_schedule_reverse"),
        SmolStr::new("gpu_schedule_shuffle"),
        SmolStr::new("gpu_schedule_workgroup_reverse"),
        SmolStr::new("gpu_schedule_workgroup_shuffle"),
        SmolStr::new("gpu_schedule_round_robin_workgroups"),
        SmolStr::new("dispatch_compute"),
        SmolStr::new("__wr_gpu_dispatch_begin"),
        SmolStr::new("__wr_gpu_dispatch_select_invocation"),
        SmolStr::new("__wr_gpu_dispatch_end"),
        SmolStr::new("__wr_map_new"),
        SmolStr::new("__wr_list_push"),
        SmolStr::new("__wr_list_get"),
        SmolStr::new("__wr_list_set"),
        SmolStr::new("__wr_list_len"),
        SmolStr::new("__wr_map_get"),
        SmolStr::new("__wr_map_len"),
        SmolStr::new("__wr_map_set"),
        SmolStr::new("__wr_str_len"),
        SmolStr::new("__wr_log"),
        SmolStr::new("__wr_log_configure"),
        SmolStr::new("__wr_runtime_cpu_count"),
        SmolStr::new("__wr_reactor_new"),
        SmolStr::new("__wr_reactor_drop"),
        SmolStr::new("__wr_reactor_register"),
        SmolStr::new("__wr_reactor_deregister"),
        SmolStr::new("__wr_reactor_arm_timer"),
        SmolStr::new("__wr_task_signal_new"),
        SmolStr::new("__wr_task_signal_drop"),
        SmolStr::new("__wr_task_unpark_one"),
        SmolStr::new("__wr_task_unpark_all"),
        SmolStr::new("__wr_task_epoch"),
        SmolStr::new("__wr_atomic_i64_new"),
        SmolStr::new("__wr_atomic_i64_drop"),
        SmolStr::new("__wr_atomic_i64_load"),
        SmolStr::new("__wr_atomic_i64_store"),
        SmolStr::new("__wr_atomic_i64_fetch_add"),
        SmolStr::new("__wr_pool_size"),
        SmolStr::new("__wr_pool_rr"),
        SmolStr::new("__wr_pool_queue_len"),
        SmolStr::new("__wr_actor_mailbox_len"),
        SmolStr::new("__wr_actor_pause"),
        SmolStr::new("__wr_actor_resume"),
        SmolStr::new("__wr_actor_pause_wait"),
        SmolStr::new("__wr_actor_fire_burst_begin"),
        SmolStr::new("__wr_actor_fire_burst_end"),
        SmolStr::new("__wr_actor_fire_burst_abort"),
        SmolStr::new("__wr_metrics_get"),
        SmolStr::new("__wr_metrics_dropped_paused_id"),
        SmolStr::new("__wr_metrics_messages_dropped_id"),
        SmolStr::new("__wr_metrics_scene_trace_id"),
        SmolStr::new("__wr_metrics_field_sample_id"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path"),
        SmolStr::new("__wr_metrics_scene_trace_hit"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_count_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_steps_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_field_samples_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_1_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_4_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_8_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_16_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_gt_16_id"),
        SmolStr::new("__wr_metrics_scene_trace"),
        SmolStr::new("__wr_metrics_field_sample"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost_id"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost_id"),
        SmolStr::new("__wr_clock_ns"),
        SmolStr::new("__wr_sleep_ms"),
        SmolStr::new("__wr_env_get"),
        SmolStr::new("__wr_env_set"),
        SmolStr::new("__wr_runtime_configure"),
    ]
}
