use super::ir::{
    PirBlock, PirCallTarget, PirExpr, PirFunction, PirIntrinsic, PirModule, PirStmt, PirType,
    PirValue,
};
use crate::hir::{BinaryOp, UnaryOp};
use smol_str::SmolStr;
use std::collections::HashMap;
use thiserror::Error;
use wrela_runtime::{
    Value as RuntimeValue, wr_mat3_add, wr_mat3_component, wr_mat3_div_scalar,
    wr_mat3_from_columns, wr_mat3_identity, wr_mat3_mul_mat3, wr_mat3_mul_scalar,
    wr_mat3_mul_vec3, wr_mat3_sub, wr_mat4_add, wr_mat4_component, wr_mat4_div_scalar,
    wr_mat4_from_columns, wr_mat4_identity, wr_mat4_mul_mat4, wr_mat4_mul_scalar,
    wr_mat4_mul_vec4, wr_mat4_sub, wr_quat_new, wr_vec2_new, wr_vec3_new, wr_vec4_new,
    wr_vec_abs, wr_vec_add, wr_vec_ceil, wr_vec_clamp, wr_vec_component, wr_vec_cos, wr_vec_cross,
    wr_vec_distance, wr_vec_div, wr_vec_dot, wr_vec_floor, wr_vec_fract, wr_vec_length,
    wr_vec_max, wr_vec_min, wr_vec_mix, wr_vec_mul, wr_vec_normalize, wr_vec_pow, wr_vec_reflect,
    wr_vec_sign, wr_vec_sin, wr_vec_sqrt, wr_vec_sub,
};

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PirExecError {
    #[error("entry '{name}' was not found in the portable module")]
    MissingEntry { name: SmolStr },
    #[error("wrong arity for '{name}': expected {expected}, found {found}")]
    ArityMismatch {
        name: SmolStr,
        expected: usize,
        found: usize,
    },
    #[error("cannot assign to immutable local '{name}'")]
    ImmutableAssign { name: SmolStr },
    #[error("unknown local '{name}'")]
    UnknownLocal { name: SmolStr },
    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        expected: String,
        found: String,
    },
    #[error("unsupported operation: {message}")]
    UnsupportedOperation { message: String },
    #[error("index out of bounds")]
    IndexOutOfBounds,
}

pub fn execute_entry(module: &PirModule, args: Vec<PirValue>) -> Result<PirValue, PirExecError> {
    execute_function(module, module.entry.as_str(), args)
}

pub fn execute_function(
    module: &PirModule,
    name: &str,
    args: Vec<PirValue>,
) -> Result<PirValue, PirExecError> {
    let Some(function) = module.function(name) else {
        return Err(PirExecError::MissingEntry {
            name: SmolStr::new(name),
        });
    };
    execute_function_inner(module, function, args)
}

fn execute_function_inner(
    module: &PirModule,
    function: &PirFunction,
    args: Vec<PirValue>,
) -> Result<PirValue, PirExecError> {
    if args.len() != function.params.len() {
        return Err(PirExecError::ArityMismatch {
            name: function.name.clone(),
            expected: function.params.len(),
            found: args.len(),
        });
    }

    let mut scopes = vec![HashMap::new()];
    for (param, value) in function.params.iter().zip(args) {
        ensure_matches_type(&value, &param.ty)?;
        scopes
            .last_mut()
            .expect("scope")
            .insert(param.name.clone(), Variable { value, mutable: false });
    }

    match execute_block(module, &function.body, &mut scopes)? {
        Some(value) => {
            ensure_matches_type(&value, &function.ret)?;
            Ok(value)
        }
        None => Ok(PirValue::Nothing),
    }
}

fn execute_block(
    module: &PirModule,
    block: &PirBlock,
    scopes: &mut Vec<HashMap<SmolStr, Variable>>,
) -> Result<Option<PirValue>, PirExecError> {
    scopes.push(HashMap::new());
    for stmt in block {
        match stmt {
            PirStmt::Let {
                name,
                mutable,
                ty,
                value,
                ..
            } => {
                let value = execute_expr(module, value, scopes)?;
                ensure_matches_type(&value, ty)?;
                scopes
                    .last_mut()
                    .expect("scope")
                    .insert(name.clone(), Variable { value, mutable: *mutable });
            }
            PirStmt::Assign { name, value, .. } => {
                let value = execute_expr(module, value, scopes)?;
                assign_local(scopes, name, value)?;
            }
            PirStmt::Expr { value, .. } => {
                let _ = execute_expr(module, value, scopes)?;
            }
            PirStmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let condition = execute_expr(module, condition, scopes)?;
                match condition {
                    PirValue::Bool(true) => {
                        if let Some(returned) = execute_block(module, then_block, scopes)? {
                            scopes.pop();
                            return Ok(Some(returned));
                        }
                    }
                    PirValue::Bool(false) => {
                        if let Some(returned) = execute_block(module, else_block, scopes)? {
                            scopes.pop();
                            return Ok(Some(returned));
                        }
                    }
                    other => {
                        scopes.pop();
                        return Err(PirExecError::TypeMismatch {
                            expected: "Bool".to_string(),
                            found: value_label(&other),
                        });
                    }
                }
            }
            PirStmt::Return { value, .. } => {
                let returned = if let Some(value) = value {
                    execute_expr(module, value, scopes)?
                } else {
                    PirValue::Nothing
                };
                scopes.pop();
                return Ok(Some(returned));
            }
        }
    }
    scopes.pop();
    Ok(None)
}

fn execute_expr(
    module: &PirModule,
    expr: &PirExpr,
    scopes: &mut Vec<HashMap<SmolStr, Variable>>,
) -> Result<PirValue, PirExecError> {
    match expr {
        PirExpr::Literal(value) => Ok(value.clone()),
        PirExpr::Var { name, .. } => lookup_local(scopes, name).map(|var| var.value.clone()),
        PirExpr::Unary { op, expr, .. } => {
            let value = execute_expr(module, expr, scopes)?;
            eval_unary(*op, value)
        }
        PirExpr::Binary { op, lhs, rhs, .. } => {
            let lhs = execute_expr(module, lhs, scopes)?;
            let rhs = execute_expr(module, rhs, scopes)?;
            eval_binary(*op, lhs, rhs)
        }
        PirExpr::Call { target, args, ty } => {
            let args = args
                .iter()
                .map(|arg| execute_expr(module, arg, scopes))
                .collect::<Result<Vec<_>, _>>()?;
            let value = match target {
                PirCallTarget::Function(name) => execute_function(module, name, args)?,
                PirCallTarget::Intrinsic(intrinsic) => eval_intrinsic(*intrinsic, args, ty)?,
            };
            ensure_matches_type(&value, ty)?;
            Ok(value)
        }
        PirExpr::Member { base, member, .. } => {
            let base = execute_expr(module, base, scopes)?;
            eval_member(base, member)
        }
        PirExpr::Index { base, index, .. } => {
            let base = execute_expr(module, base, scopes)?;
            let index = execute_expr(module, index, scopes)?;
            eval_index(base, index)
        }
        PirExpr::ArrayLiteral { items, .. } => Ok(PirValue::Array(
            items
                .iter()
                .map(|item| execute_expr(module, item, scopes))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        PirExpr::StructLiteral { fields, ty, .. } => Ok(PirValue::Struct(super::ir::PirStructValue {
            ty: ty.clone(),
            fields: fields
                .iter()
                .map(|(name, expr)| Ok((name.clone(), execute_expr(module, expr, scopes)?)))
                .collect::<Result<Vec<_>, _>>()?,
        })),
    }
}

#[derive(Debug, Clone)]
struct Variable {
    value: PirValue,
    mutable: bool,
}

fn lookup_local<'a>(
    scopes: &'a [HashMap<SmolStr, Variable>],
    name: &SmolStr,
) -> Result<&'a Variable, PirExecError> {
    scopes
        .iter()
        .rev()
        .find_map(|scope| scope.get(name))
        .ok_or_else(|| PirExecError::UnknownLocal { name: name.clone() })
}

fn assign_local(
    scopes: &mut [HashMap<SmolStr, Variable>],
    name: &SmolStr,
    value: PirValue,
) -> Result<(), PirExecError> {
    for scope in scopes.iter_mut().rev() {
        if let Some(variable) = scope.get_mut(name) {
            if !variable.mutable {
                return Err(PirExecError::ImmutableAssign { name: name.clone() });
            }
            variable.value = value;
            return Ok(());
        }
    }
    Err(PirExecError::UnknownLocal { name: name.clone() })
}

fn eval_unary(op: UnaryOp, value: PirValue) -> Result<PirValue, PirExecError> {
    match op {
        UnaryOp::Neg => match value {
            PirValue::I32(value) => Ok(PirValue::I32(-value)),
            PirValue::I64(value) => Ok(PirValue::I64(-value)),
            PirValue::F32(value) => Ok(PirValue::F32(-value)),
            _ => Err(PirExecError::UnsupportedOperation {
                message: "negation only supports scalar numerics".to_string(),
            }),
        },
        UnaryOp::Not => match value {
            PirValue::Bool(value) => Ok(PirValue::Bool(!value)),
            _ => Err(PirExecError::TypeMismatch {
                expected: "Bool".to_string(),
                found: value_label(&value),
            }),
        },
        _ => Err(PirExecError::UnsupportedOperation {
            message: format!("unary op {op:?} is not implemented in PIR execution"),
        }),
    }
}

fn eval_binary(op: BinaryOp, lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match op {
        BinaryOp::Add => eval_add(lhs, rhs),
        BinaryOp::Sub => eval_sub(lhs, rhs),
        BinaryOp::Mul => eval_mul(lhs, rhs),
        BinaryOp::Div => eval_div(lhs, rhs),
        BinaryOp::Mod => eval_mod(lhs, rhs),
        BinaryOp::Eq => Ok(PirValue::Bool(lhs == rhs)),
        BinaryOp::Ne => Ok(PirValue::Bool(lhs != rhs)),
        BinaryOp::Lt => eval_cmp(lhs, rhs, |left, right| left < right),
        BinaryOp::Gt => eval_cmp(lhs, rhs, |left, right| left > right),
        BinaryOp::Le => eval_cmp(lhs, rhs, |left, right| left <= right),
        BinaryOp::Ge => eval_cmp(lhs, rhs, |left, right| left >= right),
        BinaryOp::And => match (lhs, rhs) {
            (PirValue::Bool(left), PirValue::Bool(right)) => Ok(PirValue::Bool(left && right)),
            (left, right) => Err(PirExecError::TypeMismatch {
                expected: "Bool".to_string(),
                found: format!("{}, {}", value_label(&left), value_label(&right)),
            }),
        },
        BinaryOp::Or => match (lhs, rhs) {
            (PirValue::Bool(left), PirValue::Bool(right)) => Ok(PirValue::Bool(left || right)),
            (left, right) => Err(PirExecError::TypeMismatch {
                expected: "Bool".to_string(),
                found: format!("{}, {}", value_label(&left), value_label(&right)),
            }),
        },
        _ => Err(PirExecError::UnsupportedOperation {
            message: format!("binary op {op:?} is not implemented in PIR execution"),
        }),
    }
}

fn eval_add(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match (&lhs, &rhs) {
        (PirValue::I32(_), PirValue::I32(_))
        | (PirValue::I64(_), PirValue::I64(_))
        | (PirValue::U32(_), PirValue::U32(_))
        | (PirValue::U64(_), PirValue::U64(_))
        | (PirValue::F32(_), PirValue::F32(_)) => eval_scalar_binary(lhs, rhs, BinaryOp::Add),
        (PirValue::Vec2(_), PirValue::Vec2(_))
        | (PirValue::Vec3(_), PirValue::Vec3(_))
        | (PirValue::Vec4(_), PirValue::Vec4(_))
        | (PirValue::Quat(_), PirValue::Quat(_)) => eval_runtime_vec_binary(lhs, rhs, wr_vec_add),
        (PirValue::Mat3(_), PirValue::Mat3(_)) => eval_runtime_mat3_binary(lhs, rhs, wr_mat3_add),
        (PirValue::Mat4(_), PirValue::Mat4(_)) => eval_runtime_mat4_binary(lhs, rhs, wr_mat4_add),
        _ => Err(PirExecError::UnsupportedOperation {
            message: "add is not implemented for these operands".to_string(),
        }),
    }
}

fn eval_sub(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match (&lhs, &rhs) {
        (PirValue::I32(_), PirValue::I32(_))
        | (PirValue::I64(_), PirValue::I64(_))
        | (PirValue::U32(_), PirValue::U32(_))
        | (PirValue::U64(_), PirValue::U64(_))
        | (PirValue::F32(_), PirValue::F32(_)) => eval_scalar_binary(lhs, rhs, BinaryOp::Sub),
        (PirValue::Vec2(_), PirValue::Vec2(_))
        | (PirValue::Vec3(_), PirValue::Vec3(_))
        | (PirValue::Vec4(_), PirValue::Vec4(_))
        | (PirValue::Quat(_), PirValue::Quat(_)) => eval_runtime_vec_binary(lhs, rhs, wr_vec_sub),
        (PirValue::Mat3(_), PirValue::Mat3(_)) => eval_runtime_mat3_binary(lhs, rhs, wr_mat3_sub),
        (PirValue::Mat4(_), PirValue::Mat4(_)) => eval_runtime_mat4_binary(lhs, rhs, wr_mat4_sub),
        _ => Err(PirExecError::UnsupportedOperation {
            message: "sub is not implemented for these operands".to_string(),
        }),
    }
}

fn eval_mul(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match (&lhs, &rhs) {
        (PirValue::I32(_), PirValue::I32(_))
        | (PirValue::I64(_), PirValue::I64(_))
        | (PirValue::U32(_), PirValue::U32(_))
        | (PirValue::U64(_), PirValue::U64(_))
        | (PirValue::F32(_), PirValue::F32(_)) => eval_scalar_binary(lhs, rhs, BinaryOp::Mul),
        (PirValue::Vec2(_), PirValue::Vec2(_))
        | (PirValue::Vec3(_), PirValue::Vec3(_))
        | (PirValue::Vec4(_), PirValue::Vec4(_))
        | (PirValue::Quat(_), PirValue::Quat(_))
        | (PirValue::Vec2(_), PirValue::F32(_))
        | (PirValue::Vec3(_), PirValue::F32(_))
        | (PirValue::Vec4(_), PirValue::F32(_))
        | (PirValue::Quat(_), PirValue::F32(_)) => eval_runtime_vec_binary(lhs, rhs, wr_vec_mul),
        (PirValue::Mat3(_), PirValue::Vec3(_)) => eval_runtime_mat3_vec(lhs, rhs),
        (PirValue::Mat3(_), PirValue::Mat3(_)) => eval_runtime_mat3_binary(lhs, rhs, wr_mat3_mul_mat3),
        (PirValue::Mat3(_), PirValue::F32(_)) => eval_runtime_mat3_scalar(lhs, rhs, wr_mat3_mul_scalar),
        (PirValue::Mat4(_), PirValue::Vec4(_)) => eval_runtime_mat4_vec(lhs, rhs),
        (PirValue::Mat4(_), PirValue::Mat4(_)) => eval_runtime_mat4_binary(lhs, rhs, wr_mat4_mul_mat4),
        (PirValue::Mat4(_), PirValue::F32(_)) => eval_runtime_mat4_scalar(lhs, rhs, wr_mat4_mul_scalar),
        _ => Err(PirExecError::UnsupportedOperation {
            message: "mul is not implemented for these operands".to_string(),
        }),
    }
}

fn eval_div(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match (&lhs, &rhs) {
        (PirValue::I32(_), PirValue::I32(_))
        | (PirValue::I64(_), PirValue::I64(_))
        | (PirValue::U32(_), PirValue::U32(_))
        | (PirValue::U64(_), PirValue::U64(_))
        | (PirValue::F32(_), PirValue::F32(_)) => eval_scalar_binary(lhs, rhs, BinaryOp::Div),
        (PirValue::Vec2(_), PirValue::Vec2(_))
        | (PirValue::Vec3(_), PirValue::Vec3(_))
        | (PirValue::Vec4(_), PirValue::Vec4(_))
        | (PirValue::Quat(_), PirValue::Quat(_))
        | (PirValue::Vec2(_), PirValue::F32(_))
        | (PirValue::Vec3(_), PirValue::F32(_))
        | (PirValue::Vec4(_), PirValue::F32(_))
        | (PirValue::Quat(_), PirValue::F32(_)) => eval_runtime_vec_binary(lhs, rhs, wr_vec_div),
        (PirValue::Mat3(_), PirValue::F32(_)) => eval_runtime_mat3_scalar(lhs, rhs, wr_mat3_div_scalar),
        (PirValue::Mat4(_), PirValue::F32(_)) => eval_runtime_mat4_scalar(lhs, rhs, wr_mat4_div_scalar),
        _ => Err(PirExecError::UnsupportedOperation {
            message: "div is not implemented for these operands".to_string(),
        }),
    }
}

fn eval_mod(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    match (lhs, rhs) {
        (PirValue::I32(left), PirValue::I32(right)) => Ok(PirValue::I32(left % right)),
        (PirValue::I64(left), PirValue::I64(right)) => Ok(PirValue::I64(left % right)),
        (PirValue::U32(left), PirValue::U32(right)) => Ok(PirValue::U32(left % right)),
        (PirValue::U64(left), PirValue::U64(right)) => Ok(PirValue::U64(left % right)),
        (left, right) => Err(PirExecError::UnsupportedOperation {
            message: format!(
                "mod is not implemented for {}, {}",
                value_label(&left),
                value_label(&right)
            ),
        }),
    }
}

fn eval_cmp(
    lhs: PirValue,
    rhs: PirValue,
    cmp: impl Fn(f64, f64) -> bool,
) -> Result<PirValue, PirExecError> {
    let Some(left) = numeric_f64(&lhs) else {
        return Err(PirExecError::TypeMismatch {
            expected: "numeric".to_string(),
            found: value_label(&lhs),
        });
    };
    let Some(right) = numeric_f64(&rhs) else {
        return Err(PirExecError::TypeMismatch {
            expected: "numeric".to_string(),
            found: value_label(&rhs),
        });
    };
    Ok(PirValue::Bool(cmp(left, right)))
}

fn eval_scalar_binary(
    lhs: PirValue,
    rhs: PirValue,
    op: BinaryOp,
) -> Result<PirValue, PirExecError> {
    match (lhs, rhs) {
        (PirValue::I32(left), PirValue::I32(right)) => Ok(match op {
            BinaryOp::Add => PirValue::I32(left + right),
            BinaryOp::Sub => PirValue::I32(left - right),
            BinaryOp::Mul => PirValue::I32(left * right),
            BinaryOp::Div => PirValue::I32(left / right),
            _ => unreachable!(),
        }),
        (PirValue::I64(left), PirValue::I64(right)) => Ok(match op {
            BinaryOp::Add => PirValue::I64(left + right),
            BinaryOp::Sub => PirValue::I64(left - right),
            BinaryOp::Mul => PirValue::I64(left * right),
            BinaryOp::Div => PirValue::I64(left / right),
            _ => unreachable!(),
        }),
        (PirValue::U32(left), PirValue::U32(right)) => Ok(match op {
            BinaryOp::Add => PirValue::U32(left + right),
            BinaryOp::Sub => PirValue::U32(left - right),
            BinaryOp::Mul => PirValue::U32(left * right),
            BinaryOp::Div => PirValue::U32(left / right),
            _ => unreachable!(),
        }),
        (PirValue::U64(left), PirValue::U64(right)) => Ok(match op {
            BinaryOp::Add => PirValue::U64(left + right),
            BinaryOp::Sub => PirValue::U64(left - right),
            BinaryOp::Mul => PirValue::U64(left * right),
            BinaryOp::Div => PirValue::U64(left / right),
            _ => unreachable!(),
        }),
        (PirValue::F32(left), PirValue::F32(right)) => Ok(match op {
            BinaryOp::Add => PirValue::F32(left + right),
            BinaryOp::Sub => PirValue::F32(left - right),
            BinaryOp::Mul => PirValue::F32(left * right),
            BinaryOp::Div => PirValue::F32(left / right),
            _ => unreachable!(),
        }),
        (left, right) => Err(PirExecError::TypeMismatch {
            expected: "matching scalar numerics".to_string(),
            found: format!("{}, {}", value_label(&left), value_label(&right)),
        }),
    }
}

fn eval_member(value: PirValue, member: &SmolStr) -> Result<PirValue, PirExecError> {
    match value {
        PirValue::Vec2(value) => vec_member(&value, member),
        PirValue::Vec3(value) => vec_member(&value, member),
        PirValue::Vec4(value) | PirValue::Quat(value) => vec_member(&value, member),
        PirValue::Struct(value) => value
            .field(member)
            .cloned()
            .ok_or_else(|| PirExecError::UnsupportedOperation {
                message: format!("struct field '{}' was not found", member),
            }),
        other => Err(PirExecError::UnsupportedOperation {
            message: format!("member access is not implemented for {}", value_label(&other)),
        }),
    }
}

fn vec_member<const N: usize>(
    value: &[f32; N],
    member: &SmolStr,
) -> Result<PirValue, PirExecError> {
    let index = match member.as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        "w" => 3,
        _ => {
            return Err(PirExecError::UnsupportedOperation {
                message: format!("unsupported vector member '{member}'"),
            });
        }
    };
    value
        .get(index)
        .copied()
        .map(PirValue::F32)
        .ok_or_else(|| PirExecError::UnsupportedOperation {
            message: format!("vector member '{member}' is out of range"),
        })
}

fn eval_index(base: PirValue, index: PirValue) -> Result<PirValue, PirExecError> {
    let index = value_to_index(&index)?;
    match base {
        PirValue::Array(items) => items.get(index).cloned().ok_or(PirExecError::IndexOutOfBounds),
        other => Err(PirExecError::UnsupportedOperation {
            message: format!("indexing is not implemented for {}", value_label(&other)),
        }),
    }
}

fn eval_intrinsic(
    intrinsic: PirIntrinsic,
    args: Vec<PirValue>,
    ty: &PirType,
) -> Result<PirValue, PirExecError> {
    match intrinsic {
        PirIntrinsic::CastI32 => cast_numeric(args, |value| PirValue::I32(value as i32)),
        PirIntrinsic::CastU32 => cast_numeric(args, |value| PirValue::U32(value.max(0.0) as u32)),
        PirIntrinsic::CastI64 => cast_numeric(args, |value| PirValue::I64(value as i64)),
        PirIntrinsic::CastU64 => cast_numeric(args, |value| PirValue::U64(value.max(0.0) as u64)),
        PirIntrinsic::CastF32 => cast_numeric(args, |value| PirValue::F32(value as f32)),
        PirIntrinsic::Vec2 => match args.as_slice() {
            [PirValue::F32(x), PirValue::F32(y)] => Ok(PirValue::Vec2([*x, *y])),
            values => Err(PirExecError::TypeMismatch {
                expected: "Vec2 args".to_string(),
                found: values
                    .iter()
                    .map(value_label)
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        },
        PirIntrinsic::Vec3 => match args.as_slice() {
            [PirValue::F32(x), PirValue::F32(y), PirValue::F32(z)] => {
                Ok(PirValue::Vec3([*x, *y, *z]))
            }
            values => Err(PirExecError::TypeMismatch {
                expected: "Vec3 args".to_string(),
                found: values
                    .iter()
                    .map(value_label)
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        },
        PirIntrinsic::Vec4 => match args.as_slice() {
            [PirValue::F32(x), PirValue::F32(y), PirValue::F32(z), PirValue::F32(w)] => {
                Ok(PirValue::Vec4([*x, *y, *z, *w]))
            }
            values => Err(PirExecError::TypeMismatch {
                expected: "Vec4 args".to_string(),
                found: values
                    .iter()
                    .map(value_label)
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        },
        PirIntrinsic::Quat => match args.as_slice() {
            [PirValue::F32(x), PirValue::F32(y), PirValue::F32(z), PirValue::F32(w)] => {
                Ok(PirValue::Quat([*x, *y, *z, *w]))
            }
            values => Err(PirExecError::TypeMismatch {
                expected: "Quat args".to_string(),
                found: values
                    .iter()
                    .map(value_label)
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        },
        PirIntrinsic::Mat3Identity => Ok(runtime_value_to_pir(wr_mat3_identity(), ty)?),
        PirIntrinsic::Mat3Cols => Ok(runtime_value_to_pir(
            wr_mat3_from_columns(
                runtime_value_from_pir(&args[0])?,
                runtime_value_from_pir(&args[1])?,
                runtime_value_from_pir(&args[2])?,
            ),
            ty,
        )?),
        PirIntrinsic::Mat4Identity => Ok(runtime_value_to_pir(wr_mat4_identity(), ty)?),
        PirIntrinsic::Mat4Cols => Ok(runtime_value_to_pir(
            wr_mat4_from_columns(
                runtime_value_from_pir(&args[0])?,
                runtime_value_from_pir(&args[1])?,
                runtime_value_from_pir(&args[2])?,
                runtime_value_from_pir(&args[3])?,
            ),
            ty,
        )?),
        PirIntrinsic::Dot => runtime_binary_intrinsic(args, ty, wr_vec_dot),
        PirIntrinsic::Length => runtime_unary_intrinsic(args, ty, wr_vec_length),
        PirIntrinsic::Normalize => runtime_unary_intrinsic(args, ty, wr_vec_normalize),
        PirIntrinsic::Cross => runtime_binary_intrinsic(args, ty, wr_vec_cross),
        PirIntrinsic::Min => runtime_binary_intrinsic(args, ty, wr_vec_min),
        PirIntrinsic::Max => runtime_binary_intrinsic(args, ty, wr_vec_max),
        PirIntrinsic::Clamp => runtime_ternary_intrinsic(args, ty, wr_vec_clamp),
        PirIntrinsic::Mix => runtime_ternary_intrinsic(args, ty, wr_vec_mix),
        PirIntrinsic::Abs => runtime_unary_intrinsic(args, ty, wr_vec_abs),
        PirIntrinsic::Sign => runtime_unary_intrinsic(args, ty, wr_vec_sign),
        PirIntrinsic::Floor => runtime_unary_intrinsic(args, ty, wr_vec_floor),
        PirIntrinsic::Ceil => runtime_unary_intrinsic(args, ty, wr_vec_ceil),
        PirIntrinsic::Fract => runtime_unary_intrinsic(args, ty, wr_vec_fract),
        PirIntrinsic::Sin => runtime_unary_intrinsic(args, ty, wr_vec_sin),
        PirIntrinsic::Cos => runtime_unary_intrinsic(args, ty, wr_vec_cos),
        PirIntrinsic::Sqrt => runtime_unary_intrinsic(args, ty, wr_vec_sqrt),
        PirIntrinsic::Pow => runtime_binary_intrinsic(args, ty, wr_vec_pow),
        PirIntrinsic::Distance => runtime_binary_intrinsic(args, ty, wr_vec_distance),
        PirIntrinsic::Reflect => runtime_binary_intrinsic(args, ty, wr_vec_reflect),
    }
}

fn cast_numeric(
    args: Vec<PirValue>,
    cast: impl Fn(f64) -> PirValue,
) -> Result<PirValue, PirExecError> {
    match args.as_slice() {
        [value] => numeric_f64(value)
            .map(cast)
            .ok_or_else(|| PirExecError::TypeMismatch {
                expected: "numeric".to_string(),
                found: value_label(value),
            }),
        values => Err(PirExecError::ArityMismatch {
            name: SmolStr::new("cast"),
            expected: 1,
            found: values.len(),
        }),
    }
}

fn runtime_unary_intrinsic(
    args: Vec<PirValue>,
    ty: &PirType,
    f: extern "C" fn(RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    match args.as_slice() {
        [value] => runtime_value_to_pir(f(runtime_value_from_pir(value)?), ty),
        values => Err(PirExecError::ArityMismatch {
            name: SmolStr::new("intrinsic"),
            expected: 1,
            found: values.len(),
        }),
    }
}

fn runtime_binary_intrinsic(
    args: Vec<PirValue>,
    ty: &PirType,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    match args.as_slice() {
        [left, right] => runtime_value_to_pir(
            f(runtime_value_from_pir(left)?, runtime_value_from_pir(right)?),
            ty,
        ),
        values => Err(PirExecError::ArityMismatch {
            name: SmolStr::new("intrinsic"),
            expected: 2,
            found: values.len(),
        }),
    }
}

fn runtime_ternary_intrinsic(
    args: Vec<PirValue>,
    ty: &PirType,
    f: extern "C" fn(RuntimeValue, RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    match args.as_slice() {
        [a, b, c] => runtime_value_to_pir(
            f(
                runtime_value_from_pir(a)?,
                runtime_value_from_pir(b)?,
                runtime_value_from_pir(c)?,
            ),
            ty,
        ),
        values => Err(PirExecError::ArityMismatch {
            name: SmolStr::new("intrinsic"),
            expected: 3,
            found: values.len(),
        }),
    }
}

fn eval_runtime_vec_binary(
    lhs: PirValue,
    rhs: PirValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    let ty = lhs.ty().clone();
    runtime_value_to_pir(f(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?), &ty)
}

fn eval_runtime_mat3_binary(
    lhs: PirValue,
    rhs: PirValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(f(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?), &PirType::Mat3)
}

fn eval_runtime_mat4_binary(
    lhs: PirValue,
    rhs: PirValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(f(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?), &PirType::Mat4)
}

fn eval_runtime_mat3_vec(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(
        wr_mat3_mul_vec3(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?),
        &PirType::Vec3,
    )
}

fn eval_runtime_mat4_vec(lhs: PirValue, rhs: PirValue) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(
        wr_mat4_mul_vec4(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?),
        &PirType::Vec4,
    )
}

fn eval_runtime_mat3_scalar(
    lhs: PirValue,
    rhs: PirValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(f(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?), &PirType::Mat3)
}

fn eval_runtime_mat4_scalar(
    lhs: PirValue,
    rhs: PirValue,
    f: extern "C" fn(RuntimeValue, RuntimeValue) -> RuntimeValue,
) -> Result<PirValue, PirExecError> {
    runtime_value_to_pir(f(runtime_value_from_pir(&lhs)?, runtime_value_from_pir(&rhs)?), &PirType::Mat4)
}

fn runtime_value_from_pir(value: &PirValue) -> Result<RuntimeValue, PirExecError> {
    match value {
        PirValue::Nothing => Ok(RuntimeValue::nil()),
        PirValue::Bool(value) => Ok(RuntimeValue::from_bool(*value)),
        PirValue::I32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        PirValue::U32(value) => Ok(RuntimeValue::from_int(*value as i64)),
        PirValue::I64(value) => Ok(RuntimeValue::from_int(*value)),
        PirValue::U64(value) => i64::try_from(*value)
            .map(RuntimeValue::from_int)
            .map_err(|_| PirExecError::UnsupportedOperation {
                message: "U64 is out of runtime conversion range".to_string(),
            }),
        PirValue::F32(value) => Ok(RuntimeValue::from_float(*value as f64)),
        PirValue::Vec2([x, y]) => Ok(wr_vec2_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
        )),
        PirValue::Vec3([x, y, z]) => Ok(wr_vec3_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
        )),
        PirValue::Vec4([x, y, z, w]) => Ok(wr_vec4_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        PirValue::Quat([x, y, z, w]) => Ok(wr_quat_new(
            RuntimeValue::from_float(*x as f64),
            RuntimeValue::from_float(*y as f64),
            RuntimeValue::from_float(*z as f64),
            RuntimeValue::from_float(*w as f64),
        )),
        PirValue::Mat3(values) => Ok(wr_mat3_from_columns(
            runtime_value_from_pir(&PirValue::Vec3([values[0], values[1], values[2]]))?,
            runtime_value_from_pir(&PirValue::Vec3([values[3], values[4], values[5]]))?,
            runtime_value_from_pir(&PirValue::Vec3([values[6], values[7], values[8]]))?,
        )),
        PirValue::Mat4(values) => Ok(wr_mat4_from_columns(
            runtime_value_from_pir(&PirValue::Vec4([
                values[0], values[1], values[2], values[3],
            ]))?,
            runtime_value_from_pir(&PirValue::Vec4([
                values[4], values[5], values[6], values[7],
            ]))?,
            runtime_value_from_pir(&PirValue::Vec4([
                values[8], values[9], values[10], values[11],
            ]))?,
            runtime_value_from_pir(&PirValue::Vec4([
                values[12], values[13], values[14], values[15],
            ]))?,
        )),
        PirValue::Array(_) | PirValue::Struct(_) => Err(PirExecError::UnsupportedOperation {
            message: "runtime math conversion does not support arrays or structs".to_string(),
        }),
    }
}

fn runtime_value_to_pir(value: RuntimeValue, ty: &PirType) -> Result<PirValue, PirExecError> {
    match ty {
        PirType::Nothing => Ok(PirValue::Nothing),
        PirType::Bool => Ok(PirValue::Bool(value.as_bool())),
        PirType::I32 => Ok(PirValue::I32(value.as_int() as i32)),
        PirType::U32 => Ok(PirValue::U32(value.as_int() as u32)),
        PirType::I64 => Ok(PirValue::I64(value.as_int())),
        PirType::U64 => Ok(PirValue::U64(value.as_int() as u64)),
        PirType::F32 => Ok(PirValue::F32(if value.is_float() {
            value.as_float() as f32
        } else {
            value.as_int() as f32
        })),
        PirType::Vec2 => Ok(PirValue::Vec2([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
        ])),
        PirType::Vec3 => Ok(PirValue::Vec3([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
        ])),
        PirType::Vec4 => Ok(PirValue::Vec4([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        PirType::Quat => Ok(PirValue::Quat([
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_vec_component(value, RuntimeValue::from_int(3)))?,
        ])),
        PirType::Mat3 => Ok(PirValue::Mat3([
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(3)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(4)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(5)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(6)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(7)))?,
            component_as_f32(wr_mat3_component(value, RuntimeValue::from_int(8)))?,
        ])),
        PirType::Mat4 => Ok(PirValue::Mat4([
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(0)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(1)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(2)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(3)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(4)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(5)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(6)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(7)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(8)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(9)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(10)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(11)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(12)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(13)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(14)))?,
            component_as_f32(wr_mat4_component(value, RuntimeValue::from_int(15)))?,
        ])),
        PirType::Array(_, _) | PirType::Struct(_) => Err(PirExecError::UnsupportedOperation {
            message: "runtime math conversion does not support array or struct targets".to_string(),
        }),
    }
}

fn component_as_f32(value: RuntimeValue) -> Result<f32, PirExecError> {
    if value.is_float() {
        Ok(value.as_float() as f32)
    } else {
        Ok(value.as_int() as f32)
    }
}

fn ensure_matches_type(value: &PirValue, ty: &PirType) -> Result<(), PirExecError> {
    match (value, ty) {
        (PirValue::Nothing, PirType::Nothing)
        | (PirValue::Bool(_), PirType::Bool)
        | (PirValue::I32(_), PirType::I32)
        | (PirValue::U32(_), PirType::U32)
        | (PirValue::I64(_), PirType::I64)
        | (PirValue::U64(_), PirType::U64)
        | (PirValue::F32(_), PirType::F32)
        | (PirValue::Vec2(_), PirType::Vec2)
        | (PirValue::Vec3(_), PirType::Vec3)
        | (PirValue::Vec4(_), PirType::Vec4)
        | (PirValue::Quat(_), PirType::Quat)
        | (PirValue::Mat3(_), PirType::Mat3)
        | (PirValue::Mat4(_), PirType::Mat4) => Ok(()),
        (PirValue::Array(items), PirType::Array(item_ty, len)) => {
            if items.len() != *len {
                return Err(PirExecError::TypeMismatch {
                    expected: format!("Array[len={}]", len),
                    found: format!("Array[len={}]", items.len()),
                });
            }
            for item in items {
                ensure_matches_type(item, item_ty)?;
            }
            Ok(())
        }
        (PirValue::Struct(value), PirType::Struct(layout)) => {
            if !matches!(&value.ty, PirType::Struct(found) if found.name == layout.name) {
                return Err(PirExecError::TypeMismatch {
                    expected: layout.name.to_string(),
                    found: match &value.ty {
                        PirType::Struct(found) => found.name.to_string(),
                        _ => "Struct".to_string(),
                    },
                });
            }
            for field in &layout.fields {
                let Some((_, field_value)) = value
                    .fields
                    .iter()
                    .find(|(field_name, _)| field_name == &field.name)
                else {
                    return Err(PirExecError::TypeMismatch {
                        expected: format!("field {}", field.name),
                        found: "missing".to_string(),
                    });
                };
                ensure_matches_type(field_value, &field.ty)?;
            }
            Ok(())
        }
        (value, ty) => Err(PirExecError::TypeMismatch {
            expected: format!("{ty:?}"),
            found: value_label(value),
        }),
    }
}

fn value_to_index(value: &PirValue) -> Result<usize, PirExecError> {
    match value {
        PirValue::I32(value) if *value >= 0 => Ok(*value as usize),
        PirValue::U32(value) => Ok(*value as usize),
        PirValue::I64(value) if *value >= 0 => Ok(*value as usize),
        PirValue::U64(value) => Ok(*value as usize),
        _ => Err(PirExecError::TypeMismatch {
            expected: "non-negative integer index".to_string(),
            found: value_label(value),
        }),
    }
}

fn numeric_f64(value: &PirValue) -> Option<f64> {
    match value {
        PirValue::I32(value) => Some(*value as f64),
        PirValue::U32(value) => Some(*value as f64),
        PirValue::I64(value) => Some(*value as f64),
        PirValue::U64(value) => Some(*value as f64),
        PirValue::F32(value) => Some(*value as f64),
        _ => None,
    }
}

fn value_label(value: &PirValue) -> String {
    match value {
        PirValue::Nothing => "Nothing".to_string(),
        PirValue::Bool(_) => "Bool".to_string(),
        PirValue::I32(_) => "I32".to_string(),
        PirValue::U32(_) => "U32".to_string(),
        PirValue::I64(_) => "I64".to_string(),
        PirValue::U64(_) => "U64".to_string(),
        PirValue::F32(_) => "F32".to_string(),
        PirValue::Vec2(_) => "Vec2".to_string(),
        PirValue::Vec3(_) => "Vec3".to_string(),
        PirValue::Vec4(_) => "Vec4".to_string(),
        PirValue::Mat3(_) => "Mat3".to_string(),
        PirValue::Mat4(_) => "Mat4".to_string(),
        PirValue::Quat(_) => "Quat".to_string(),
        PirValue::Array(_) => "Array".to_string(),
        PirValue::Struct(value) => match &value.ty {
            PirType::Struct(layout) => layout.name.to_string(),
            _ => "Struct".to_string(),
        },
    }
}
