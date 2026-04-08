use crate::kernel::ir::KernelCaptureQueryPlan;
use crate::kernel::{KernelStructValue, KernelValue};
use crate::query_exec::cpu::QueryExecError;
use crate::query_plan::CaptureKind;
use smol_str::SmolStr;

pub(crate) trait CaptureQueryBackend {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
    fn resolve_shape_capture(&self, capture: Option<&KernelValue>) -> Result<SmolStr, QueryExecError>;
    fn capture_distance(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: CaptureKind,
    ) -> Result<f32, QueryExecError>;
    fn capture_normal(
        &self,
        capture: &SmolStr,
        point: [f32; 3],
        capture_kind: CaptureKind,
    ) -> Result<[f32; 3], QueryExecError>;
    fn trace_shape(
        &self,
        shape: &SmolStr,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        min_step: f32,
        hit_epsilon: f32,
        max_steps: i32,
    ) -> Result<KernelValue, QueryExecError>;
    fn surface_at(
        &self,
        shape: &SmolStr,
        hit: &KernelStructValue,
    ) -> Result<KernelValue, QueryExecError>;
    fn radiance_at(
        &self,
        shape: &SmolStr,
        point: [f32; 3],
        direction: [f32; 3],
    ) -> Result<KernelValue, QueryExecError>;
    fn medium_at(&self, shape: &SmolStr, point: [f32; 3]) -> Result<KernelValue, QueryExecError>;
}

pub(crate) fn execute_capture_query<B: CaptureQueryBackend>(
    backend: &B,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<KernelValue, QueryExecError> {
    match plan.kind {
        crate::query_plan::CaptureQueryKind::Distance => {
            let capture = backend.resolve_field_or_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            Ok(KernelValue::F32(
                backend.capture_distance(&capture, point, plan.capture_kind)?,
            ))
        }
        crate::query_plan::CaptureQueryKind::Normal => {
            let capture = backend.resolve_field_or_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            Ok(KernelValue::Vec3(
                backend.capture_normal(&capture, point, plan.capture_kind)?,
            ))
        }
        crate::query_plan::CaptureQueryKind::Trace => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let origin = expect_vec3_arg(args.get(1), "origin")?;
            let direction = expect_vec3_arg(args.get(2), "direction")?;
            let max_distance = expect_f32_arg(args.get(3), "max_distance")?;
            let min_step = expect_f32_arg(args.get(4), "min_step")?;
            let hit_epsilon = expect_f32_arg(args.get(5), "hit_epsilon")?;
            let max_steps = expect_i32_arg(args.get(6), "max_steps")?;
            backend.trace_shape(
                &capture,
                origin,
                direction,
                max_distance,
                min_step,
                hit_epsilon,
                max_steps,
            )
        }
        crate::query_plan::CaptureQueryKind::Surface => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let hit = expect_struct_ref_arg(args.get(1), "Hit3")?;
            backend.surface_at(&capture, hit)
        }
        crate::query_plan::CaptureQueryKind::Radiance => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            let direction = expect_vec3_arg(args.get(2), "direction")?;
            backend.radiance_at(&capture, point, direction)
        }
        crate::query_plan::CaptureQueryKind::Medium => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            backend.medium_at(&capture, point)
        }
    }
}

fn expect_vec3_arg(
    value: Option<&KernelValue>,
    name: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    match value {
        Some(KernelValue::Vec3(vec)) => Ok(*vec),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("Vec3 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_f32_arg(value: Option<&KernelValue>, name: &'static str) -> Result<f32, QueryExecError> {
    match value {
        Some(KernelValue::F32(number)) => Ok(*number),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("F32 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_i32_arg(value: Option<&KernelValue>, name: &'static str) -> Result<i32, QueryExecError> {
    match value {
        Some(KernelValue::I32(number)) => Ok(*number),
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("I32 for {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_ref_arg<'a>(
    value: Option<&'a KernelValue>,
    name: &'static str,
) -> Result<&'a KernelStructValue, QueryExecError> {
    match value {
        Some(KernelValue::Struct(struct_value)) if struct_value.name.as_str() == name => {
            Ok(struct_value)
        }
        Some(other) => Err(QueryExecError::TypeMismatch {
            expected: format!("{name} struct"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}
