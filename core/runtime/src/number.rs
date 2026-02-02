use crate::string;
use crate::value::{TypeId, Value, int_value};

pub fn num_add(a: Value, b: Value) -> Value {
    if is_string(a) && is_string(b) {
        let parts = [a, b];
        return string::str_concat(parts.as_ptr(), parts.len());
    }
    numeric_binary(a, b, |x, y| x + y, |x, y| x + y)
}

pub fn num_sub(a: Value, b: Value) -> Value {
    numeric_binary(a, b, |x, y| x - y, |x, y| x - y)
}

pub fn num_mul(a: Value, b: Value) -> Value {
    numeric_binary(a, b, |x, y| x * y, |x, y| x * y)
}

pub fn num_div(a: Value, b: Value) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Int(x)), Some(NumKind::Int(y))) => {
            if y == 0 {
                std::process::abort();
            }
            Value::from_int(x / y)
        }
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(xf / yf)
        }
        _ => Value::nil(),
    }
}

pub fn num_mod(a: Value, b: Value) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Int(x)), Some(NumKind::Int(y))) => {
            if y == 0 {
                std::process::abort();
            }
            Value::from_int(x % y)
        }
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(xf % yf)
        }
        _ => Value::nil(),
    }
}

pub fn num_neg(a: Value) -> Value {
    match num_kind(a) {
        Some(NumKind::Int(x)) => Value::from_int(-x),
        Some(NumKind::Float(x)) => Value::from_float(-x),
        None => Value::nil(),
    }
}

pub fn num_lt(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x < y, |x, y| x < y))
}

pub fn num_gt(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x > y, |x, y| x > y))
}

pub fn num_le(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x <= y, |x, y| x <= y))
}

pub fn num_ge(a: Value, b: Value) -> Value {
    Value::from_bool(numeric_cmp(a, b, |x, y| x >= y, |x, y| x >= y))
}

fn numeric_binary(
    a: Value,
    b: Value,
    int_op: impl FnOnce(i64, i64) -> i64,
    float_op: impl FnOnce(f64, f64) -> f64,
) -> Value {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Int(x)), Some(NumKind::Int(y))) => Value::from_int(int_op(x, y)),
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            Value::from_float(float_op(xf, yf))
        }
        _ => Value::nil(),
    }
}

fn numeric_cmp(
    a: Value,
    b: Value,
    int_op: impl FnOnce(i64, i64) -> bool,
    float_op: impl FnOnce(f64, f64) -> bool,
) -> bool {
    match (num_kind(a), num_kind(b)) {
        (Some(NumKind::Int(x)), Some(NumKind::Int(y))) => int_op(x, y),
        (Some(x), Some(y)) => {
            let xf = num_to_f64(x);
            let yf = num_to_f64(y);
            float_op(xf, yf)
        }
        _ => false,
    }
}

fn num_to_f64(kind: NumKind) -> f64 {
    match kind {
        NumKind::Int(x) => x as f64,
        NumKind::Float(x) => x,
    }
}

fn num_kind(val: Value) -> Option<NumKind> {
    if let Some(i) = int_value(val) {
        return Some(NumKind::Int(i));
    }
    if is_float(val) {
        return Some(NumKind::Float(val.as_float()));
    }
    None
}

fn is_float(val: Value) -> bool {
    val.is_float()
}

fn is_string(val: Value) -> bool {
    if !val.is_ptr() {
        return false;
    }
    unsafe { (*val.as_ptr()).type_id == TypeId::String as u32 }
}

enum NumKind {
    Int(i64),
    Float(f64),
}
