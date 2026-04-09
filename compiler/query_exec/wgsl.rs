pub(crate) mod codegen;

use self::codegen::{ShaderPlan, generate_shader};
use crate::kernel::KernelBatchQueryTrace;
use crate::kernel::ir::{KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan};
use crate::kernel::{
    KernelStructValue, KernelValidationError, KernelValue, validate_batch_query_plan,
    validate_capture_query_plan, validate_world_query_plan,
};
use crate::portable::{
    PortableAbiType, portable_abi_array_stride, portable_abi_decode_slice,
    portable_abi_encode_slice, portable_abi_encode_value, portable_abi_layout,
};
use crate::query_exec::QueryExecutionObservability;
use crate::query_exec::cpu::{DirectQueryOps, QueryExecError};
use crate::query_exec::world::world_query_semantics;
use crate::query_plan::{BatchQueryKind, CaptureKind, CaptureQueryKind, WorldQueryKind};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, mpsc};
use wgpu::util::{DeviceExt, initialize_adapter_from_env_or_default};

#[derive(Clone)]
struct NativeWgpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

#[derive(Debug)]
pub(crate) struct GpuDispatchRequest {
    pub(crate) dispatch: KernelValue,
    pub(crate) items: Vec<KernelValue>,
    pub(crate) world_shape_indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedShaderModule {
    pub(crate) source: String,
    pub(crate) workgroup_size: u32,
    pub(crate) dispatch_abi: PortableAbiType,
    pub(crate) item_abi: PortableAbiType,
    pub(crate) result_abi: PortableAbiType,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeWgslBridgeConfig {
    pub(crate) source: SmolStr,
    pub(crate) workgroup_size: i64,
}

#[derive(Clone)]
struct CachedPipeline {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

pub(crate) fn execute_capture_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    ops.note_dispatch();
    if let Err(errors) = validate_capture_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("capture query", errors));
    }
    let request = build_capture_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::Capture(plan))?;
    let mut values = dispatch_compiled_shader(&generated, request)?;
    note_capture_observability(&ops, plan.kind);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no capture result".to_string(),
    })?;
    Ok((value, ops.snapshot_observability()))
}

pub(crate) fn execute_world_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    ops.note_dispatch();
    if let Err(errors) = validate_world_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("world query", errors));
    }
    let request = build_world_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::World(plan))?;
    let mut values = dispatch_compiled_shader(&generated, request)?;
    note_world_observability(&ops, plan.kind);
    let value = values.pop().ok_or_else(|| QueryExecError::Unsupported {
        message: "native WGSL backend produced no world result".to_string(),
    })?;
    Ok((value, ops.snapshot_observability()))
}

pub(crate) fn execute_batch_query_with_observability(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    _trace: &KernelBatchQueryTrace,
) -> Result<(KernelValue, QueryExecutionObservability), QueryExecError> {
    let ops = DirectQueryOps::new(ctx);
    ops.note_dispatch();
    if let Err(errors) = validate_batch_query_plan(plan) {
        ops.note_contract_validation_failure();
        return Err(validation_error("batch query", errors));
    }
    let request = build_batch_request(&ops, plan, args)?;
    let generated = generate_compiled_shader(ctx, ShaderPlan::Batch(plan))?;
    let values = dispatch_compiled_shader(&generated, request)?;
    note_batch_observability(&ops, plan.kind);
    Ok((KernelValue::Array(values), ops.snapshot_observability()))
}

pub(crate) fn compile_world_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelWorldQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::World(plan))
}

pub(crate) fn compile_batch_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: &KernelBatchQueryPlan,
) -> Result<GeneratedShaderModule, QueryExecError> {
    generate_compiled_shader(ctx, ShaderPlan::Batch(plan))
}

pub(crate) fn bridge_config(shader: &GeneratedShaderModule) -> NativeWgslBridgeConfig {
    NativeWgslBridgeConfig {
        source: SmolStr::new(shader.source.as_str()),
        workgroup_size: i64::from(shader.workgroup_size),
    }
}

fn build_capture_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let (capture_kind, capture_index) = match plan.kind {
        CaptureQueryKind::Distance | CaptureQueryKind::Normal => match plan.capture_kind {
            CaptureKind::Field => {
                let capture = ops.resolve_field_or_shape_capture(args.first())?;
                (0u32, field_index(ops.context(), &capture)?)
            }
            CaptureKind::Shape => {
                let capture = ops.resolve_field_or_shape_capture(args.first())?;
                (1u32, shape_index(ops.context(), &capture)?)
            }
            CaptureKind::Region => {
                return Err(QueryExecError::Unsupported {
                    message: "region captures are only valid for world queries".to_string(),
                });
            }
        },
        CaptureQueryKind::Trace
        | CaptureQueryKind::Surface
        | CaptureQueryKind::Radiance
        | CaptureQueryKind::Medium => {
            let capture = ops.resolve_shape_capture(args.first())?;
            (1u32, shape_index(ops.context(), &capture)?)
        }
    };

    let item = match plan.kind {
        CaptureQueryKind::Distance | CaptureQueryKind::Normal | CaptureQueryKind::Medium => {
            point_query(expect_vec3_arg(args.get(1), "point")?)
        }
        CaptureQueryKind::Trace => {
            expect_struct_arg(args.get(1), "RayQuery")?;
            args.get(1)
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "ray" })?
        }
        CaptureQueryKind::Surface => args
            .get(1)
            .cloned()
            .ok_or(QueryExecError::MissingCaptureTarget { kind: "hit" })?,
        CaptureQueryKind::Radiance => {
            expect_struct_arg(args.get(1), "PointDirectionQuery")?;
            args.get(1)
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "sample" })?
        }
    };

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(capture_kind, capture_index, 1, 0, true, true, true),
        items: vec![item],
        world_shape_indices: Vec::new(),
    })
}

fn note_capture_observability(ops: &DirectQueryOps<'_>, kind: CaptureQueryKind) {
    ops.note_artifact_load();
    if matches!(kind, CaptureQueryKind::Trace) {
        ops.note_trace_step();
    }
}

fn note_world_observability(ops: &DirectQueryOps<'_>, kind: WorldQueryKind) {
    ops.note_artifact_load();
    if matches!(kind, WorldQueryKind::Trace) {
        ops.note_trace_step();
    }
}

fn note_batch_observability(ops: &DirectQueryOps<'_>, kind: BatchQueryKind) {
    ops.note_artifact_load();
    if matches!(kind, BatchQueryKind::Trace | BatchQueryKind::Occluded) {
        ops.note_trace_step();
    }
}

fn build_world_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let capture = ops.resolve_region_capture(args.first())?;
    let domain = expect_struct_arg(args.get(1), "SceneDomain")?;
    let detail = ops.validate_world_domain(
        &capture,
        domain,
        world_query_semantics(plan.kind).query_name,
    )?;
    let surface_root_shape_id = if matches!(plan.kind, WorldQueryKind::Surface) {
        let hit = expect_struct_arg(args.get(2), "Hit3")?;
        Some(expect_struct_u32(hit, "root_shape_id")?)
    } else {
        None
    };
    let world_shapes = ops.resolve_world_shapes(&capture, detail, surface_root_shape_id)?;
    ops.note_candidate_count(world_shapes.len() as u32);
    let world_shape_indices = world_shapes
        .iter()
        .map(|shape| shape_index(ops.context(), shape))
        .collect::<Result<Vec<_>, _>>()?;
    let item = match plan.kind {
        WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Medium => {
            point_query(expect_vec3_arg(args.get(2), "point")?)
        }
        WorldQueryKind::Trace => {
            expect_struct_arg(args.get(2), "RayQuery")?;
            args.get(2)
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "ray" })?
        }
        WorldQueryKind::Surface => args
            .get(2)
            .cloned()
            .ok_or(QueryExecError::MissingCaptureTarget { kind: "hit" })?,
        WorldQueryKind::Radiance => {
            expect_struct_arg(args.get(2), "PointDirectionQuery")?;
            args.get(2)
                .cloned()
                .ok_or(QueryExecError::MissingCaptureTarget { kind: "sample" })?
        }
    };

    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            2,
            0,
            1,
            world_shape_indices.len() as u32,
            ops.world_domain_flag_enabled(domain, WorldQueryKind::Surface)?,
            ops.world_domain_flag_enabled(domain, WorldQueryKind::Radiance)?,
            ops.world_domain_flag_enabled(domain, WorldQueryKind::Medium)?,
        ),
        items: vec![item],
        world_shape_indices,
    })
}

fn build_batch_request(
    ops: &DirectQueryOps<'_>,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
) -> Result<GpuDispatchRequest, QueryExecError> {
    let capture = match plan.kind {
        BatchQueryKind::Distance | BatchQueryKind::Normal => {
            ops.resolve_field_or_shape_capture(args.first())?
        }
        BatchQueryKind::Trace | BatchQueryKind::Surface | BatchQueryKind::Occluded => {
            ops.resolve_shape_capture(args.first())?
        }
    };
    let items = expect_array_arg(
        args.get(1),
        match plan.kind {
            BatchQueryKind::Distance | BatchQueryKind::Normal => "points",
            BatchQueryKind::Surface => "hits",
            BatchQueryKind::Trace | BatchQueryKind::Occluded => "rays",
        },
    )?;
    ops.note_candidate_count(items.len() as u32);
    Ok(GpuDispatchRequest {
        dispatch: dispatch_config(
            match plan.capture_kind {
                CaptureKind::Field => 0,
                CaptureKind::Shape => 1,
                CaptureKind::Region => 2,
            },
            match plan.capture_kind {
                CaptureKind::Field => field_index(ops.context(), &capture)?,
                CaptureKind::Shape => shape_index(ops.context(), &capture)?,
                CaptureKind::Region => 0,
            },
            items.len() as u32,
            0,
            true,
            true,
            true,
        ),
        items: items.to_vec(),
        world_shape_indices: Vec::new(),
    })
}

fn generate_compiled_shader(
    ctx: &crate::query_exec::context::QueryExecContext,
    plan: ShaderPlan<'_>,
) -> Result<GeneratedShaderModule, QueryExecError> {
    let generated = generate_shader(ctx, plan)?;
    validate_generated_shader(&generated.source)?;
    Ok(GeneratedShaderModule {
        source: generated.source,
        workgroup_size: generated.workgroup_size,
        dispatch_abi: generated.dispatch_abi,
        item_abi: generated.item_abi,
        result_abi: generated.result_abi,
    })
}

pub(crate) fn dispatch_compiled_shader(
    generated: &GeneratedShaderModule,
    request: GpuDispatchRequest,
) -> Result<Vec<KernelValue>, QueryExecError> {
    if request.items.is_empty() {
        return Ok(Vec::new());
    }

    let native = native_wgpu_context()?;
    let dispatch_bytes = encode_value(&generated.dispatch_abi, &request.dispatch)?;
    let input_bytes = encode_slice(&generated.item_abi, &request.items)?;
    let shape_bytes = encode_shape_indices(&request.world_shape_indices)?;
    let result_stride = portable_abi_array_stride(&generated.result_abi) as usize;
    let result_buffer_size = (result_stride * request.items.len()).max(result_stride.max(4));

    let dispatch_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.dispatch"),
            contents: &dispatch_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let input_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.input"),
            contents: &input_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.wgsl.output"),
        size: result_buffer_size as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let world_shapes_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.wgsl.world_shapes"),
            contents: &shape_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
    let readback_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wrela.wgsl.readback"),
        size: result_buffer_size as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let dispatch_min_size =
        wgpu::BufferSize::new(portable_abi_layout(&generated.dispatch_abi).size as u64);
    let cached = compiled_pipeline(
        &native,
        &generated.source,
        generated.workgroup_size,
        dispatch_min_size,
    )?;
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.wgsl.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: world_shapes_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = native
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.wgsl.encoder"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("wrela.wgsl.compute_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (request.items.len() as u32).div_ceil(generated.workgroup_size),
            1,
            1,
        );
    }
    encoder.copy_buffer_to_buffer(
        &output_buffer,
        0,
        &readback_buffer,
        0,
        result_buffer_size as u64,
    );
    native.queue.submit(Some(encoder.finish()));

    let slice = readback_buffer.slice(..result_buffer_size as u64);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(wgpu_poll_error)?;
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(QueryExecError::Unsupported {
                message: format!("native WGSL readback failed: {err}"),
            });
        }
        Err(err) => {
            return Err(QueryExecError::Unsupported {
                message: format!("native WGSL readback channel failed: {err}"),
            });
        }
    }
    let bytes = slice.get_mapped_range().to_vec();
    let _ = slice;
    readback_buffer.unmap();

    decode_slice(&generated.result_abi, &bytes, request.items.len())
}

fn compiled_pipeline(
    native: &NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    dispatch_min_size: Option<wgpu::BufferSize>,
) -> Result<CachedPipeline, QueryExecError> {
    static PIPELINES: OnceLock<Mutex<HashMap<(String, u32, u64), CachedPipeline>>> =
        OnceLock::new();
    let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (
        source.to_string(),
        workgroup_size,
        dispatch_min_size.map(wgpu::BufferSize::get).unwrap_or(0),
    );

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            return Ok(cached.clone());
        }
    }

    let bind_group_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela.wgsl.bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: dispatch_min_size,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela.wgsl.pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
    let shader_module = native
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wrela.wgsl.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
    let error_scope = native
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = native
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("wrela.wgsl.pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("WG_SIZE", workgroup_size as f64)],
                zero_initialize_workgroup_memory: true,
            },
            cache: None,
        });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(wgpu_poll_error)?;
    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err}"),
        });
    }

    let cached = CachedPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn validate_generated_shader(source: &str) -> Result<(), QueryExecError> {
    let module =
        naga::front::wgsl::parse_str(source).map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL parse failed: {err}"),
        })?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|err| QueryExecError::Unsupported {
            message: format!("native WGSL validation failed: {err}"),
        })?;
    Ok(())
}

fn native_wgpu_context() -> Result<&'static NativeWgpuContext, QueryExecError> {
    static CONTEXT: OnceLock<Result<NativeWgpuContext, String>> = OnceLock::new();
    match CONTEXT.get_or_init(init_native_wgpu_context) {
        Ok(context) => Ok(context),
        Err(message) => Err(QueryExecError::Unsupported {
            message: format!("native WGSL backend initialization failed: {message}"),
        }),
    }
}

fn init_native_wgpu_context() -> Result<NativeWgpuContext, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(initialize_adapter_from_env_or_default(&instance, None))
        .map_err(|err| format!("request adapter failed: {err}"))?;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("wrela.wgsl.device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))
        .map_err(|err| format!("request device failed: {err}"))?;
    Ok(NativeWgpuContext { device, queue })
}

fn validation_error(label: &str, errors: Vec<KernelValidationError>) -> QueryExecError {
    let messages = errors
        .into_iter()
        .map(|error| error.message)
        .collect::<Vec<_>>()
        .join("; ");
    QueryExecError::Unsupported {
        message: format!("native WGSL contract validation failed for {label}: {messages}"),
    }
}

fn field_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .fields
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingField { name: name.clone() })
}

fn shape_index(
    ctx: &crate::query_exec::context::QueryExecContext,
    name: &SmolStr,
) -> Result<u32, QueryExecError> {
    ctx.scene
        .shapes
        .keys()
        .enumerate()
        .find_map(|(index, candidate)| (candidate == name).then_some(index as u32))
        .ok_or_else(|| QueryExecError::MissingShape { name: name.clone() })
}

pub(crate) fn dispatch_config(
    capture_kind: u32,
    capture_index: u32,
    item_count: u32,
    shape_count: u32,
    material_enabled: bool,
    radiance_enabled: bool,
    media_enabled: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(capture_kind)),
            (
                SmolStr::new("capture_index"),
                KernelValue::U32(capture_index),
            ),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(shape_count)),
            (
                SmolStr::new("material_enabled"),
                KernelValue::Bool(material_enabled),
            ),
            (
                SmolStr::new("radiance_enabled"),
                KernelValue::Bool(radiance_enabled),
            ),
            (
                SmolStr::new("media_enabled"),
                KernelValue::Bool(media_enabled),
            ),
        ],
    })
}

fn point_query(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn encode_shape_indices(indices: &[u32]) -> Result<Vec<u8>, QueryExecError> {
    let values = if indices.is_empty() {
        vec![KernelValue::U32(0)]
    } else {
        indices.iter().copied().map(KernelValue::U32).collect()
    };
    encode_slice(&PortableAbiType::U32, &values)
}

fn encode_value(abi: &PortableAbiType, value: &KernelValue) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_value(abi, value).map_err(portable_abi_error)
}

fn encode_slice(abi: &PortableAbiType, values: &[KernelValue]) -> Result<Vec<u8>, QueryExecError> {
    portable_abi_encode_slice(abi, values).map_err(portable_abi_error)
}

fn decode_slice(
    abi: &PortableAbiType,
    bytes: &[u8],
    len: usize,
) -> Result<Vec<KernelValue>, QueryExecError> {
    portable_abi_decode_slice(abi, bytes, len).map_err(portable_abi_error)
}

fn portable_abi_error(err: crate::portable::PortableAbiError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL ABI conversion failed: {err}"),
    }
}

fn wgpu_poll_error(err: wgpu::PollError) -> QueryExecError {
    QueryExecError::Unsupported {
        message: format!("native WGSL device poll failed: {err}"),
    }
}

fn expect_array_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a [KernelValue], QueryExecError> {
    match value {
        Some(KernelValue::Array(values)) => Ok(values),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Array for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(value)) if value.name.as_str() == name => Ok(value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_u32(value: &KernelStructValue, field: &str) -> Result<u32, QueryExecError> {
    let Some((_, value)) = value.fields.iter().find(|(name, _)| name.as_str() == field) else {
        return Err(QueryExecError::MissingCaptureTarget {
            kind: "struct field",
        });
    };
    match value {
        KernelValue::U32(value) => Ok(*value),
        KernelValue::I32(value) if *value >= 0 => Ok(*value as u32),
        other => Err(QueryExecError::TypeMismatch {
            expected: format!("U32 for field {field}"),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    name: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(value)) => Ok(*value),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Vec3 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}
