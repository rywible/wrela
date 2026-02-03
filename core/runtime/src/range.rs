use crate::list;
use crate::value::{Value, int_value};

pub fn range_new(start: Value, end: Value) -> Value {
    match (num_kind(start), num_kind(end)) {
        (Some(NumKind::Integer(a)), Some(NumKind::Integer(b))) => range_int(a, b),
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
        list::list_push(list_val, Value::from_float(current));
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
        NumKind::Integer(x) => x as f64,
        NumKind::Float(x) => x,
    }
}

fn num_kind(val: Value) -> Option<NumKind> {
    if let Some(i) = int_value(val) {
        return Some(NumKind::Integer(i));
    }
    if is_float(val) {
        return Some(NumKind::Float(val.as_float()));
    }
    None
}

fn is_float(val: Value) -> bool {
    val.is_float()
}

enum NumKind {
    Integer(i64),
    Float(f64),
}
