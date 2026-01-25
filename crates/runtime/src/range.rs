use crate::float_box;
use crate::list;
use crate::value::{TypeId, Value};
use crate::{wr_rc_dec};

pub fn range_new(start: Value, end: Value) -> Value {
    match (num_kind(start), num_kind(end)) {
        (Some(NumKind::Int(a)), Some(NumKind::Int(b))) => range_int(a, b),
        (Some(a), Some(b)) => range_float(num_to_f64(a), num_to_f64(b)),
        _ => list::list_new(0),
    }
}

fn range_int(start: i64, end: i64) -> Value {
    let list_val = list::list_new(0);
    let step = if start <= end { 1 } else { -1 };
    let mut current = start;
    loop {
        list::list_push(list_val, Value::from_int(current));
        if current == end {
            break;
        }
        current = current.saturating_add(step);
    }
    list_val
}

fn range_float(start: f64, end: f64) -> Value {
    if !start.is_finite() || !end.is_finite() {
        return list::list_new(0);
    }
    let list_val = list::list_new(0);
    let step = if start <= end { 1.0 } else { -1.0 };
    let mut current = start;
    loop {
        let boxed = float_box::box_float(current);
        list::list_push(list_val, boxed);
        unsafe { wr_rc_dec(boxed) };
        if (step > 0.0 && current >= end) || (step < 0.0 && current <= end) {
            break;
        }
        current += step;
        if !current.is_finite() {
            break;
        }
    }
    list_val
}

fn num_to_f64(kind: NumKind) -> f64 {
    match kind {
        NumKind::Int(x) => x as f64,
        NumKind::Float(x) => x,
    }
}

fn num_kind(val: Value) -> Option<NumKind> {
    if val.is_int() {
        return Some(NumKind::Int(val.as_int()));
    }
    if is_float(val) {
        return Some(NumKind::Float(float_box::unbox_float(val)));
    }
    None
}

fn is_float(val: Value) -> bool {
    if !val.is_ptr() {
        return false;
    }
    unsafe { (*val.as_ptr()).type_id == TypeId::Float as u32 }
}

enum NumKind {
    Int(i64),
    Float(f64),
}
