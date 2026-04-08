use crate::kernel::{KernelStructValue, KernelValue};
use crate::kernel::interp::KernelBatchQueryTrace;
use crate::kernel::{
    KernelBatchQueryPlan, KernelCaptureQueryPlan, KernelWorldQueryPlan, lower_capture_query_plan,
};
use crate::query_exec::cpu::{
    DirectQueryOps, default_hit, default_surface, medium_value,
};
use crate::query_exec::capture::{self, CaptureQueryBackend};
use crate::query_exec::{QueryExecContext, QueryExecError};
use crate::query_exec::world::{
    WorldDistanceBackend, WorldMediumBackend, WorldNormalBackend, WorldQueryBackend,
    WorldRadianceBackend, WorldSurfaceBackend, WorldTraceBackend, execute_world_distance,
    execute_world_medium, execute_world_normal, execute_world_radiance, execute_world_surface,
    execute_world_trace,
};
use crate::query_plan::{BatchQueryKind, CaptureQueryKind, CaptureQueryPlan, WorldQueryKind};
use smol_str::SmolStr;

pub(crate) fn execute_capture_query(
    ctx: &QueryExecContext,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    capture::execute_capture_query(
        &VirtualGpuCaptureBackend {
            direct: DirectQueryOps::new(ctx),
        },
        plan,
        args,
    )
}

pub(crate) fn execute_world_query(
    ctx: &QueryExecContext,
    plan: &KernelWorldQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    VirtualGpuDirectQueryEvaluator::new(ctx).execute_world_query(plan, args)
}

struct VirtualGpuDirectQueryEvaluator<'a> {
    direct: DirectQueryOps<'a>,
}

struct VirtualGpuCaptureBackend<'a> {
    direct: DirectQueryOps<'a>,
}

impl<'a> VirtualGpuDirectQueryEvaluator<'a> {
    fn new(ctx: &'a QueryExecContext) -> Self {
        Self {
            direct: DirectQueryOps::new(ctx),
        }
    }

    fn execute_world_query(
        &self,
        plan: &KernelWorldQueryPlan,
        args: &[KernelValue],
    ) -> Result<KernelValue, QueryExecError> {
        let capture = self.direct.resolve_region_capture(args.first())?;
        let domain = expect_struct_ref_arg(args.get(1), "SceneDomain")?;
        let detail = self
            .direct
            .validate_world_domain(&capture, domain, crate::query_exec::world::world_query_semantics(plan.kind).query_name)?;
        match plan.kind {
            WorldQueryKind::Distance => {
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldDistanceBackend {
                    direct: &self.direct,
                    capture: &capture,
                    detail,
                    point,
                    result: 1_000_000.0,
                };
                execute_world_distance(&mut backend)?;
                Ok(KernelValue::F32(backend.result))
            }
            WorldQueryKind::Normal => {
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldNormalBackend {
                    direct: &self.direct,
                    capture: &capture,
                    detail,
                    point,
                };
                Ok(KernelValue::Vec3(execute_world_normal(&mut backend)?))
            }
            WorldQueryKind::Trace => {
                let origin = expect_vec3_arg(args.get(2), "origin")?;
                let direction = expect_vec3_arg(args.get(3), "direction")?;
                let max_distance = expect_f32_arg(args.get(4), "max_distance")?;
                let min_step = expect_f32_arg(args.get(5), "min_step")?;
                let hit_epsilon = expect_f32_arg(args.get(6), "hit_epsilon")?;
                let max_steps = expect_i32(args.get(7), "max_steps")?;
                let mut backend = VirtualGpuWorldTraceBackend {
                    direct: &self.direct,
                    capture: &capture,
                    detail,
                    origin,
                    direction,
                    max_distance,
                    min_step,
                    hit_epsilon,
                    max_steps,
                    result: default_hit(origin),
                    best_distance: f32::INFINITY,
                };
                execute_world_trace(&mut backend)?;
                Ok(backend.result)
            }
            WorldQueryKind::Surface => {
                let hit = expect_struct_ref_arg(args.get(2), "Hit3")?;
                let mut backend = VirtualGpuWorldSurfaceBackend {
                    direct: &self.direct,
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
                let point = expect_vec3_arg(args.get(2), "point")?;
                let direction = expect_vec3_arg(args.get(3), "direction")?;
                let mut backend = VirtualGpuWorldRadianceBackend {
                    direct: &self.direct,
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
                let point = expect_vec3_arg(args.get(2), "point")?;
                let mut backend = VirtualGpuWorldMediumBackend {
                    direct: &self.direct,
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
}

impl CaptureQueryBackend for VirtualGpuCaptureBackend<'_> {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.direct.resolve_field_or_shape_capture(capture)
    }

    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError> {
        self.direct.resolve_shape_capture(capture)
    }

    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<f32, QueryExecError> {
        self.direct.eval_capture_distance(capture, point, capture_kind)
    }

    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: crate::query_plan::CaptureKind,
    ) -> Result<[f32; 3], QueryExecError> {
        self.direct.eval_capture_normal(capture, point, capture_kind)
    }

    fn trace_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError> {
        self.direct.trace_shape(
            shape,
            origin,
            direction,
            max_distance,
            min_step,
            hit_epsilon,
            max_steps,
        )
    }

    fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError> {
        self.direct.surface_at(shape, hit)
    }

    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        self.direct.radiance_at(shape, point, direction)
    }

    fn medium_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
    ) -> Result<KernelValue, QueryExecError> {
        self.direct.medium_at(shape, point)
    }
}

fn vgpu_backend_with_world_shapes<B, F>(
    direct: &DirectQueryOps<'_>,
    capture: &SmolStr,
    detail: i32,
    backend: &mut B,
    mut emit_shapes: F,
) -> Result<(), QueryExecError>
where
    F: FnMut(&mut B, &[SmolStr]) -> Result<(), QueryExecError>,
{
    let shapes = direct.resolve_world_shapes(capture, detail)?;
    emit_shapes(backend, &shapes)
}

fn vgpu_backend_with_domain_flag<B, F>(
    direct: &DirectQueryOps<'_>,
    domain: &KernelStructValue,
    kind: WorldQueryKind,
    backend: &mut B,
    enabled: F,
) -> Result<(), QueryExecError>
where
    F: FnOnce(&mut B) -> Result<(), QueryExecError>,
{
    if direct.world_domain_flag_enabled(domain, kind)? {
        enabled(backend)?;
    }
    Ok(())
}

fn vgpu_world_distance(
    direct: &DirectQueryOps<'_>,
    capture: &SmolStr,
    detail: i32,
    point: [f32; 3],
) -> Result<f32, QueryExecError> {
    let mut backend = VirtualGpuWorldDistanceBackend {
        direct,
        capture,
        detail,
        point,
        result: 1_000_000.0,
    };
    execute_world_distance(&mut backend)?;
    Ok(backend.result)
}

struct VirtualGpuWorldDistanceBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
    result: f32,
}

impl WorldQueryBackend for VirtualGpuWorldDistanceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_world_shapes(self.direct, self.capture, self.detail, self, emit_shapes)
    }

    fn with_domain_flag<F>(&mut self, _kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        enabled(self)
    }
}

impl WorldDistanceBackend for VirtualGpuWorldDistanceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_distance(&mut self) -> Result<(), Self::Error> {
        self.result = 1_000_000.0;
        Ok(())
    }

    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        self.result = self.result.min(self.direct.eval_shape_distance(shape, self.point)?);
        Ok(())
    }
}

struct VirtualGpuWorldNormalBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    point: [f32; 3],
}

impl WorldNormalBackend for VirtualGpuWorldNormalBackend<'_, '_> {
    type Error = QueryExecError;
    type Point = [f32; 3];
    type Distance = f32;
    type Normal = [f32; 3];

    fn base_point(&mut self) -> Result<Self::Point, Self::Error> {
        Ok(self.point)
    }

    fn offset_point(
        &mut self,
        point: &Self::Point,
        axis: usize,
        delta: f32,
    ) -> Result<Self::Point, Self::Error> {
        let mut point = *point;
        point[axis] += delta;
        Ok(point)
    }

    fn sample_world_distance(&mut self, point: Self::Point) -> Result<Self::Distance, Self::Error> {
        vgpu_world_distance(self.direct, self.capture, self.detail, point)
    }

    fn subtract_distance(
        &mut self,
        positive: Self::Distance,
        negative: Self::Distance,
    ) -> Result<Self::Distance, Self::Error> {
        Ok(positive - negative)
    }

    fn compose_normal(
        &mut self,
        x: Self::Distance,
        y: Self::Distance,
        z: Self::Distance,
    ) -> Result<Self::Normal, Self::Error> {
        Ok([x, y, z])
    }

    fn normalize_normal(&mut self, normal: Self::Normal) -> Result<Self::Normal, Self::Error> {
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length == 0.0 {
            Ok([0.0, 0.0, 1.0])
        } else {
            Ok([normal[0] / length, normal[1] / length, normal[2] / length])
        }
    }
}

struct VirtualGpuWorldTraceBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    min_step: f32,
    hit_epsilon: f32,
    max_steps: i32,
    result: KernelValue,
    best_distance: f32,
}

impl WorldQueryBackend for VirtualGpuWorldTraceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_world_shapes(self.direct, self.capture, self.detail, self, emit_shapes)
    }

    fn with_domain_flag<F>(&mut self, _kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        enabled(self)
    }
}

impl WorldTraceBackend for VirtualGpuWorldTraceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_trace(&mut self) -> Result<(), Self::Error> {
        self.result = default_hit(self.origin);
        self.best_distance = f32::INFINITY;
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let hit = self.direct.trace_shape(
            shape,
            self.origin,
            self.direction,
            self.max_distance,
            self.min_step,
            self.hit_epsilon,
            self.max_steps,
        )?;
        let hit_ref = expect_struct(&hit, "Hit3")?;
        if expect_struct_bool(hit_ref, "hit")? {
            let distance = expect_struct_f32(hit_ref, "distance")?;
            if distance < self.best_distance {
                self.best_distance = distance;
                self.result = hit;
            }
        }
        Ok(())
    }
}

struct VirtualGpuWorldSurfaceBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    hit: KernelStructValue,
    root_shape_id: u32,
    result: KernelValue,
}

impl WorldQueryBackend for VirtualGpuWorldSurfaceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_world_shapes(self.direct, self.capture, self.detail, self, emit_shapes)
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_domain_flag(self.direct, self.domain, kind, self, enabled)
    }
}

impl WorldSurfaceBackend for VirtualGpuWorldSurfaceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_surface(&mut self) -> Result<(), Self::Error> {
        self.result = default_surface();
        Ok(())
    }

    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if crate::query_exec::stable_shape_capture_id(shape) == self.root_shape_id {
            self.result = self.direct.surface_at(shape, &self.hit)?;
        }
        Ok(())
    }
}

struct VirtualGpuWorldRadianceBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    direction: [f32; 3],
    result: [f32; 3],
}

impl WorldQueryBackend for VirtualGpuWorldRadianceBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_world_shapes(self.direct, self.capture, self.detail, self, emit_shapes)
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_domain_flag(self.direct, self.domain, kind, self, enabled)
    }
}

impl WorldRadianceBackend for VirtualGpuWorldRadianceBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error> {
        self.result = [0.0, 0.0, 0.0];
        Ok(())
    }

    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let KernelValue::Vec3(next) = self.direct.radiance_at(shape, self.point, self.direction)?
        else {
            return Ok(());
        };
        self.result = [
            self.result[0] + next[0],
            self.result[1] + next[1],
            self.result[2] + next[2],
        ];
        Ok(())
    }
}

struct VirtualGpuWorldMediumBackend<'a, 'ctx> {
    direct: &'a DirectQueryOps<'ctx>,
    capture: &'a SmolStr,
    detail: i32,
    domain: &'a KernelStructValue,
    point: [f32; 3],
    density: f32,
    emission: [f32; 3],
    anisotropy: f32,
}

impl WorldQueryBackend for VirtualGpuWorldMediumBackend<'_, '_> {
    type Error = QueryExecError;

    fn with_world_shapes<F>(
        &mut self,
        _kind: WorldQueryKind,
        _invalid_message: &'static str,
        emit_shapes: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(&mut Self, &[SmolStr]) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_world_shapes(self.direct, self.capture, self.detail, self, emit_shapes)
    }

    fn with_domain_flag<F>(&mut self, kind: WorldQueryKind, enabled: F) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<(), Self::Error>,
    {
        vgpu_backend_with_domain_flag(self.direct, self.domain, kind, self, enabled)
    }
}

impl WorldMediumBackend for VirtualGpuWorldMediumBackend<'_, '_> {
    type Error = QueryExecError;

    fn init_world_medium(&mut self) -> Result<(), Self::Error> {
        self.density = 0.0;
        self.emission = [0.0, 0.0, 0.0];
        self.anisotropy = 0.0;
        Ok(())
    }

    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let KernelValue::Struct(next) = self.direct.medium_at(shape, self.point)? else {
            return Ok(());
        };
        self.density += expect_struct_f32(&next, "density")?;
        let next_emission = expect_struct_vec3_from_struct(&next, "emission")?;
        self.anisotropy += expect_struct_f32(&next, "anisotropy")?;
        self.emission = [
            self.emission[0] + next_emission[0],
            self.emission[1] + next_emission[1],
            self.emission[2] + next_emission[2],
        ];
        Ok(())
    }
}

pub(crate) fn execute_batch_query(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    args: &[KernelValue],
    trace: &KernelBatchQueryTrace,
) -> Result<KernelValue, QueryExecError> {
    let items = match args.get(1) {
        Some(KernelValue::Array(items)) => items,
        Some(other) => {
            return Err(QueryExecError::TypeMismatch {
                expected: "Array".to_string(),
                found: format!("{other:?}"),
            });
        }
        None => {
            return Err(QueryExecError::MissingCaptureTarget {
                kind: "batch query items",
            });
        }
    };

    let mut out = vec![KernelValue::Nothing; items.len()];
    for iteration in &trace.iterations {
        let item_index = iteration.item_index as usize;
        let Some(item) = items.get(item_index) else {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "virtual GPU scheduled missing item index {}",
                    iteration.item_index
                ),
            });
        };
        out[item_index] = execute_batch_item(ctx, plan, args.first(), item)?;
    }
    Ok(KernelValue::Array(out))
}

fn execute_batch_item(
    ctx: &QueryExecContext,
    plan: &KernelBatchQueryPlan,
    capture: Option<&KernelValue>,
    item: &KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match plan.kind {
        BatchQueryKind::Distance => {
            let point = expect_struct_vec3(item, "PointQuery", "point")?;
            let result = execute_capture_query(
                ctx,
                &capture_plan(CaptureQueryKind::Distance, plan)?,
                &[required_capture(capture)?, KernelValue::Vec3(point)],
            )?;
            Ok(distance_result(expect_f32(&result, "distance")?))
        }
        BatchQueryKind::Normal => {
            let point = expect_struct_vec3(item, "PointQuery", "point")?;
            let result = execute_capture_query(
                ctx,
                &capture_plan(CaptureQueryKind::Normal, plan)?,
                &[required_capture(capture)?, KernelValue::Vec3(point)],
            )?;
            Ok(normal_result(expect_vec3(&result, "normal")?))
        }
        BatchQueryKind::Trace => {
            let ray = expect_struct(item, "RayQuery")?;
            execute_capture_query(
                ctx,
                &capture_plan(CaptureQueryKind::Trace, plan)?,
                &[
                    required_capture(capture)?,
                    field_value(ray, "origin")?.clone(),
                    field_value(ray, "direction")?.clone(),
                    field_value(ray, "max_distance")?.clone(),
                    field_value(ray, "min_step")?.clone(),
                    field_value(ray, "hit_epsilon")?.clone(),
                    field_value(ray, "max_steps")?.clone(),
                ],
            )
        }
        BatchQueryKind::Surface => execute_capture_query(
            ctx,
            &capture_plan(CaptureQueryKind::Surface, plan)?,
            &[required_capture(capture)?, item.clone()],
        ),
        BatchQueryKind::Occluded => {
            let ray = expect_struct(item, "RayQuery")?;
            let hit = execute_capture_query(
                ctx,
                &capture_plan(CaptureQueryKind::Trace, plan)?,
                &[
                    required_capture(capture)?,
                    field_value(ray, "origin")?.clone(),
                    field_value(ray, "direction")?.clone(),
                    field_value(ray, "max_distance")?.clone(),
                    field_value(ray, "min_step")?.clone(),
                    field_value(ray, "hit_epsilon")?.clone(),
                    field_value(ray, "max_steps")?.clone(),
                ],
            )?;
            let hit = expect_struct(&hit, "Hit3")?;
            Ok(occlusion_result(
                expect_struct_bool(hit, "hit")?,
                expect_struct_f32(hit, "distance")?,
                expect_struct_i32(hit, "steps")?,
            ))
        }
    }
}

fn capture_plan(
    kind: CaptureQueryKind,
    plan: &KernelBatchQueryPlan,
) -> Result<crate::kernel::KernelCaptureQueryPlan, QueryExecError> {
    let capture_plan =
        CaptureQueryPlan::for_query(kind, plan.capture_kind, None).map_err(|message| {
            QueryExecError::Unsupported {
                message: message.to_string(),
            }
        })?;
    Ok(lower_capture_query_plan(&capture_plan))
}

fn required_capture(capture: Option<&KernelValue>) -> Result<KernelValue, QueryExecError> {
    capture
        .cloned()
        .ok_or(QueryExecError::MissingCaptureTarget {
            kind: "batch query capture",
        })
}

fn expect_struct_ref_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: name })?;
    expect_struct(value, name)
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    expected: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: expected })?;
    expect_vec3(value, expected)
}

fn expect_f32_arg(
    value: Option<&KernelValue>,
    expected: &'static str,
) -> Result<f32, QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: expected })?;
    expect_f32(value, expected)
}

fn expect_struct<'a>(
    value: &'a KernelValue,
    name: &str,
) -> Result<&'a crate::kernel::KernelStructValue, QueryExecError> {
    match value {
        KernelValue::Struct(value) if value.name.as_str() == name => Ok(value),
        other => Err(QueryExecError::TypeMismatch {
            expected: name.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn field_value<'a>(
    value: &'a crate::kernel::KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, QueryExecError> {
    value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
        .map(|(_, value)| value)
        .ok_or_else(|| QueryExecError::Unsupported {
            message: format!("missing field '{name}' on {}", value.name),
        })
}

fn expect_struct_vec3(
    value: &KernelValue,
    struct_name: &str,
    field: &str,
) -> Result<[f32; 3], QueryExecError> {
    let value = expect_struct(value, struct_name)?;
    match field_value(value, field)? {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "Vec3".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_bool(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<bool, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "Bool".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_f32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<f32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::F32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "F32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_i32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<i32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::I32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "I32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_u32(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<u32, QueryExecError> {
    match field_value(value, field)? {
        KernelValue::U32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "U32".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_struct_vec3_from_struct(
    value: &crate::kernel::KernelStructValue,
    field: &str,
) -> Result<[f32; 3], QueryExecError> {
    match field_value(value, field)? {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: "Vec3".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_i32(value: Option<&KernelValue>, expected: &'static str) -> Result<i32, QueryExecError> {
    let value = value.ok_or(QueryExecError::MissingCaptureTarget { kind: expected })?;
    match value {
        KernelValue::I32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: expected.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_f32(value: &KernelValue, expected: &str) -> Result<f32, QueryExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: expected.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn expect_vec3(value: &KernelValue, expected: &str) -> Result<[f32; 3], QueryExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(QueryExecError::TypeMismatch {
            expected: expected.to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn distance_result(distance: f32) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("DistanceResult"),
        fields: vec![(SmolStr::new("distance"), KernelValue::F32(distance))],
    })
}

fn normal_result(normal: [f32; 3]) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("NormalResult"),
        fields: vec![(SmolStr::new("normal"), KernelValue::Vec3(normal))],
    })
}

fn occlusion_result(occluded: bool, distance: f32, steps: i32) -> KernelValue {
    KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("OcclusionResult"),
        fields: vec![
            (SmolStr::new("occluded"), KernelValue::Bool(occluded)),
            (SmolStr::new("distance"), KernelValue::F32(distance)),
            (SmolStr::new("steps"), KernelValue::I32(steps)),
        ],
    })
}
