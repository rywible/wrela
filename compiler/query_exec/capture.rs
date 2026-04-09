use crate::kernel::ir::{KernelBatchItemContract, KernelCaptureQueryPlan};
use crate::kernel::{KernelStructValue, KernelValue};
use crate::query_exec::cpu::QueryExecError;
use crate::query_plan::CaptureKind;
use smol_str::SmolStr;

pub(crate) trait CaptureQueryBackend {
    fn resolve_field_or_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
    fn resolve_shape_capture(
        &self,
        capture: Option<&KernelValue>,
    ) -> Result<SmolStr, QueryExecError>;
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
            Ok(KernelValue::F32(backend.capture_distance(
                &capture,
                point,
                plan.capture_kind,
            )?))
        }
        crate::query_plan::CaptureQueryKind::Normal => {
            let capture = backend.resolve_field_or_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            Ok(KernelValue::Vec3(backend.capture_normal(
                &capture,
                point,
                plan.capture_kind,
            )?))
        }
        crate::query_plan::CaptureQueryKind::Trace => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let ray = expect_struct_ref_arg(args.get(1), "RayQuery")?;
            let origin = expect_struct_vec3(ray, "origin")?;
            let direction = expect_struct_vec3(ray, "direction")?;
            let max_distance = expect_struct_f32(ray, "max_distance")?;
            let min_step = expect_struct_f32(ray, "min_step")?;
            let hit_epsilon = expect_struct_f32(ray, "hit_epsilon")?;
            let max_steps = expect_struct_i32(ray, "max_steps")?;
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
            let sample = expect_struct_ref_arg(args.get(1), "PointDirectionQuery")?;
            let point = expect_struct_vec3(sample, "point")?;
            let direction = expect_struct_vec3(sample, "direction")?;
            backend.radiance_at(&capture, point, direction)
        }
        crate::query_plan::CaptureQueryKind::Medium => {
            let capture = backend.resolve_shape_capture(args.first())?;
            let point = expect_vec3_arg(args.get(1), "point")?;
            backend.medium_at(&capture, point)
        }
    }
}

pub(crate) fn execute_batch_item_contract<B: CaptureQueryBackend>(
    backend: &B,
    contract: &KernelBatchItemContract,
    capture: Option<&KernelValue>,
    item: &KernelValue,
) -> Result<KernelValue, QueryExecError> {
    match contract {
        KernelBatchItemContract::CaptureQuery { plan } => {
            let args = build_batch_capture_args(plan, capture, item)?;
            let value = execute_capture_query(backend, plan, &args)?;
            match plan.kind {
                crate::query_plan::CaptureQueryKind::Distance => {
                    Ok(distance_result(expect_f32_arg(Some(&value), "distance")?))
                }
                crate::query_plan::CaptureQueryKind::Normal => {
                    Ok(normal_result(expect_vec3_arg(Some(&value), "normal")?))
                }
                _ => Ok(value),
            }
        }
        KernelBatchItemContract::TraceThenOcclusion { trace_plan } => {
            let args = build_batch_capture_args(trace_plan, capture, item)?;
            let hit = execute_capture_query(backend, trace_plan, &args)?;
            let hit = expect_struct_ref_arg(Some(&hit), "Hit3")?;
            Ok(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("OcclusionResult"),
                fields: vec![
                    (
                        SmolStr::new("occluded"),
                        KernelValue::Bool(expect_struct_bool(hit, "hit")?),
                    ),
                    (
                        SmolStr::new("distance"),
                        KernelValue::F32(expect_struct_f32(hit, "distance")?),
                    ),
                    (
                        SmolStr::new("steps"),
                        KernelValue::I32(expect_struct_i32(hit, "steps")?),
                    ),
                ],
            }))
        }
    }
}

fn distance_result(distance: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("DistanceResult"),
        fields: vec![(SmolStr::new("distance"), KernelValue::F32(distance))],
    })
}

fn normal_result(normal: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("NormalResult"),
        fields: vec![(SmolStr::new("normal"), KernelValue::Vec3(normal))],
    })
}

fn build_batch_capture_args(
    plan: &KernelCaptureQueryPlan,
    capture: Option<&KernelValue>,
    item: &KernelValue,
) -> Result<Vec<KernelValue>, QueryExecError> {
    let mut args = Vec::new();
    if let Some(capture) = capture {
        args.push(capture.clone());
    }
    match plan.kind {
        crate::query_plan::CaptureQueryKind::Distance
        | crate::query_plan::CaptureQueryKind::Normal => {
            let point = expect_struct_ref_arg(Some(item), "PointQuery")?;
            args.push(KernelValue::Vec3(expect_struct_vec3(point, "point")?));
        }
        crate::query_plan::CaptureQueryKind::Trace => {
            expect_struct_ref_arg(Some(item), "RayQuery")?;
            args.push(item.clone());
        }
        crate::query_plan::CaptureQueryKind::Surface => {
            expect_struct_ref_arg(Some(item), "Hit3")?;
            args.push(item.clone());
        }
        crate::query_plan::CaptureQueryKind::Radiance => {
            expect_struct_ref_arg(Some(item), "PointDirectionQuery")?;
            args.push(item.clone());
        }
        crate::query_plan::CaptureQueryKind::Medium => {
            let point = expect_struct_ref_arg(Some(item), "PointQuery")?;
            args.push(KernelValue::Vec3(expect_struct_vec3(point, "point")?));
        }
    }
    Ok(args)
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

fn expect_struct_bool(
    value: &KernelStructValue,
    name: &'static str,
) -> Result<bool, QueryExecError> {
    match value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
    {
        Some((_, KernelValue::Bool(value))) => Ok(*value),
        Some((_, other)) => Err(QueryExecError::TypeMismatch {
            expected: format!("Bool field {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_f32(value: &KernelStructValue, name: &'static str) -> Result<f32, QueryExecError> {
    match value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
    {
        Some((_, KernelValue::F32(value))) => Ok(*value),
        Some((_, other)) => Err(QueryExecError::TypeMismatch {
            expected: format!("F32 field {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_i32(value: &KernelStructValue, name: &'static str) -> Result<i32, QueryExecError> {
    match value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
    {
        Some((_, KernelValue::I32(value))) => Ok(*value),
        Some((_, other)) => Err(QueryExecError::TypeMismatch {
            expected: format!("I32 field {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}

fn expect_struct_vec3(
    value: &KernelStructValue,
    name: &'static str,
) -> Result<[f32; 3], QueryExecError> {
    match value
        .fields
        .iter()
        .find(|(field, _)| field.as_str() == name)
    {
        Some((_, KernelValue::Vec3(value))) => Ok(*value),
        Some((_, other)) => Err(QueryExecError::TypeMismatch {
            expected: format!("Vec3 field {name}"),
            found: format!("{other:?}"),
        }),
        None => Err(QueryExecError::MissingCaptureTarget { kind: name }),
    }
}
