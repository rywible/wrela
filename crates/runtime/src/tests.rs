use super::*;
use crate::value::value_eq;

#[test]
fn value_tags_roundtrip() {
    let i = Value::from_int(-42);
    assert!(i.is_int());
    assert_eq!(i.as_int(), -42);
    assert_eq!(wr_type_id(i), TypeId::Int as u32);

    let b = Value::from_bool(true);
    assert!(b.is_bool());
    assert_eq!(b.as_bool(), true);
    assert_eq!(wr_type_id(b), TypeId::Bool as u32);

    let n = Value::nil();
    assert!(n.is_nil());
    assert_eq!(wr_type_id(n), TypeId::Nil as u32);
}

#[test]
fn nanbox_float_roundtrip() {
    let v = wr_box_float(3.5);
    assert!(v.is_float());
    let f = wr_unbox_float(v);
    assert!((f - 3.5).abs() < f64::EPSILON);
    assert_eq!(wr_type_id(v), TypeId::Float as u32);
}

#[test]
fn nanbox_float_nan_behavior() {
    let v = wr_box_float(f64::NAN);
    assert!(v.is_float());
    let f = wr_unbox_float(v);
    assert!(f.is_nan());
    assert!(!value_eq(v, v));
}

#[test]
fn nanbox_int_float_eq() {
    let i = Value::from_int(42);
    let f = Value::from_float(42.0);
    assert!(value_eq(i, f));
    assert!(value_eq(f, i));
}

#[test]
fn boxed_int_roundtrip() {
    let big = (1i64 << 48) + 5;
    let v = Value::from_int(big);
    assert!(!v.is_int());
    assert!(v.is_ptr());
    assert_eq!(crate::value::int_value(v), Some(big));
    assert_eq!(wr_type_id(v), TypeId::Int as u32);
    unsafe { wr_rc_dec(v) };
}

#[test]
fn string_concat_and_intern() {
    let hello = wr_str_from_utf8(b"hello".as_ptr(), 5);
    let space = wr_str_from_utf8(b" ".as_ptr(), 1);
    let world = wr_str_from_utf8(b"world".as_ptr(), 5);
    let parts = [hello, space, world];
    let joined = wr_str_concat(parts.as_ptr(), parts.len());
    assert!(joined.is_ptr());

    let a = wr_str_from_utf8(b"same".as_ptr(), 4);
    let b = wr_str_from_utf8(b"same".as_ptr(), 4);
    let ia = wr_str_intern(a);
    let ib = wr_str_intern(b);
    assert_eq!(ia.0, ib.0);
    unsafe { wr_rc_inc(ib) };

    unsafe {
        wr_rc_dec(hello);
        wr_rc_dec(space);
        wr_rc_dec(world);
        wr_rc_dec(joined);
        wr_rc_dec(ia);
        wr_rc_dec(ib);
    }
}

#[test]
fn list_set_get() {
    let list = wr_list_new(2);
    wr_list_set(list, 0, Value::from_int(10));
    wr_list_set(list, 1, Value::from_int(20));
    let v0 = wr_list_get(list, 0);
    let v1 = wr_list_get(list, 1);
    assert_eq!(v0.as_int(), 10);
    assert_eq!(v1.as_int(), 20);
    unsafe { wr_rc_dec(list) };
}

#[test]
fn map_set_get() {
    let map = wr_map_new();
    let key = Value::from_int(7);
    let val = Value::from_int(9);
    wr_map_set(map, key, val);
    let got = wr_map_get(map, key);
    assert_eq!(got.as_int(), 9);
    unsafe { wr_rc_dec(map) };
}

#[test]
fn result_ok_err_unwrap() {
    let ok = wr_result_ok(Value::from_int(42));
    let ok_flag = wr_result_is_ok(ok);
    assert!(ok_flag.is_bool());
    assert!(ok_flag.as_bool());
    let unwrapped = wr_result_unwrap(ok);
    assert!(unwrapped.is_int());
    assert_eq!(unwrapped.as_int(), 42);

    let err = wr_result_err(Value::from_int(7));
    let err_flag = wr_result_is_ok(err);
    assert!(err_flag.is_bool());
    assert!(!err_flag.as_bool());

    unsafe {
        wr_rc_dec(ok);
        wr_rc_dec(unwrapped);
        wr_rc_dec(err);
    }
}

#[test]
fn iter_list() {
    let list = wr_list_new(0);
    wr_list_push(list, Value::from_int(1));
    wr_list_push(list, Value::from_int(2));
    let iter = wr_iter_init(list);
    let mut out = Value::nil();
    let mut done = Value::from_bool(false);
    wr_iter_next(iter, &mut out, &mut done);
    assert_eq!(out.as_int(), 1);
    assert!(!done.as_bool());
    wr_iter_next(iter, &mut out, &mut done);
    assert_eq!(out.as_int(), 2);
    assert!(!done.as_bool());
    wr_iter_next(iter, &mut out, &mut done);
    assert!(done.as_bool());
    unsafe {
        wr_rc_dec(iter);
        wr_rc_dec(list);
    }
}

#[test]
fn iter_map_keys() {
    let map = wr_map_new();
    wr_map_set(map, Value::from_int(1), Value::from_int(10));
    wr_map_set(map, Value::from_int(2), Value::from_int(20));
    let iter = wr_iter_init(map);
    let mut out = Value::nil();
    let mut done = Value::from_bool(false);
    wr_iter_next(iter, &mut out, &mut done);
    assert!(!done.as_bool());
    assert!(out.is_int());
    unsafe {
        wr_rc_dec(iter);
        wr_rc_dec(map);
    }
}

#[test]
fn actor_stub_pending() {
    extern "C" fn add_one(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let v = args[1];
        if v.is_int() {
            Value::from_int(v.as_int() + 1)
        } else {
            Value::nil()
        }
    }

    wr_register_method(1, 0, add_one);
    let actor = wr_actor_spawn(1, Value::nil(), 1, 3, -1, -1, -1);
    let arg = Value::from_int(41);
    let pending = wr_actor_send(actor, 0, 1, &arg as *const Value);
    let val = wr_pending_await(pending);
    let ok = wr_result_is_ok(val);
    assert!(ok.as_bool());
    let unwrapped = wr_result_unwrap(val);
    assert!(unwrapped.is_int());
    assert_eq!(unwrapped.as_int(), 42);
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(actor);
        wr_rc_dec(val);
        wr_rc_dec(unwrapped);
    }
}

#[test]
fn pending_await_multiple_times() {
    extern "C" fn add_two(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let v = args[1];
        if v.is_int() {
            Value::from_int(v.as_int() + 2)
        } else {
            Value::nil()
        }
    }

    wr_register_method(3, 0, add_two);
    let actor = wr_actor_spawn(3, Value::nil(), 1, 3, -1, -1, -1);
    let arg = Value::from_int(40);
    let pending = wr_actor_send(actor, 0, 1, &arg as *const Value);
    let val1 = wr_pending_await(pending);
    let val2 = wr_pending_await(pending);
    let ok1 = wr_result_is_ok(val1);
    let ok2 = wr_result_is_ok(val2);
    assert!(ok1.as_bool());
    assert!(ok2.as_bool());
    let unwrapped1 = wr_result_unwrap(val1);
    let unwrapped2 = wr_result_unwrap(val2);
    assert!(unwrapped1.is_int());
    assert_eq!(unwrapped1.as_int(), 42);
    assert!(unwrapped2.is_int());
    assert_eq!(unwrapped2.as_int(), 42);
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(actor);
        wr_rc_dec(val1);
        wr_rc_dec(val2);
        wr_rc_dec(unwrapped1);
        wr_rc_dec(unwrapped2);
    }
}

#[test]
fn actor_missing_method_returns_nil() {
    let actor = wr_actor_spawn(10, Value::nil(), 1, 3, -1, -1, -1);
    let pending = wr_actor_send(actor, 99, 0, std::ptr::null());
    let val = wr_pending_await(pending);
    let ok = wr_result_is_ok(val);
    assert!(ok.as_bool());
    let unwrapped = wr_result_unwrap(val);
    assert!(unwrapped.is_nil());
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(actor);
        wr_rc_dec(val);
        wr_rc_dec(unwrapped);
    }
}

#[test]
fn actor_invalid_args_returns_nil() {
    extern "C" fn add_one(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let v = args[1];
        if v.is_int() {
            Value::from_int(v.as_int() + 1)
        } else {
            Value::nil()
        }
    }

    wr_register_method(11, 0, add_one);
    let actor = wr_actor_spawn(11, Value::nil(), 1, 3, -1, -1, -1);
    let pending = wr_actor_send(actor, 0, 0, std::ptr::null());
    let val = wr_pending_await(pending);
    let ok = wr_result_is_ok(val);
    assert!(ok.as_bool());
    let unwrapped = wr_result_unwrap(val);
    assert!(unwrapped.is_nil());
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(actor);
        wr_rc_dec(val);
        wr_rc_dec(unwrapped);
    }
}

#[test]
fn actor_fire_executes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::time::Duration;

    static COUNTER_PTR: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

    extern "C" fn bump(_argc: usize, _argv: *const Value) -> Value {
        if let Some(counter) = COUNTER_PTR.get() {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        Value::nil()
    }

    let counter = Arc::new(AtomicUsize::new(0));
    let _ = COUNTER_PTR.set(counter.clone());

    wr_register_method(2, 0, bump);
    let actor = wr_actor_spawn(2, Value::nil(), 1, 3, -1, -1, -1);
    wr_actor_fire(actor, 0, 0, std::ptr::null());

    // Wait briefly for the actor thread to process the message.
    for _ in 0..100 {
        if counter.load(Ordering::SeqCst) == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(counter.load(Ordering::SeqCst), 1);
    unsafe {
        wr_rc_dec(actor);
    }
}

#[test]
fn class_get_set() {
    let name_ptrs = [b"x".as_ptr()];
    let name_lens = [1usize];
    let class = wr_class_new(TypeId::UserBase as u32, name_ptrs.as_ptr(), name_lens.as_ptr(), 1);
    assert_eq!(wr_type_id(class), TypeId::UserBase as u32);
    let got = wr_class_get(class, b"x".as_ptr(), 1);
    assert!(got.is_nil());
    let val = Value::from_int(99);
    wr_class_set(class, b"x".as_ptr(), 1, val);
    let got2 = wr_class_get(class, b"x".as_ptr(), 1);
    assert!(got2.is_int());
    assert_eq!(got2.as_int(), 99);
    unsafe { wr_rc_dec(class) };
}

#[test]
fn metrics_counts_basic() {
    wr_metrics_reset();
    let list = wr_list_new(0);
    let s = wr_str_from_utf8(b"x".as_ptr(), 1);
    wr_list_push(list, s);
    unsafe { wr_rc_dec(list) };

    let rc_inc =
        wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_INC as i64)).as_int();
    let rc_dec =
        wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_DEC as i64)).as_int();
    assert!(rc_inc >= 1);
    assert!(rc_dec >= 1);
}

#[test]
fn refcount_invariants_common_flows() {
    extern "C" fn noop(_argc: usize, _argv: *const Value) -> Value {
        Value::nil()
    }

    wr_metrics_reset();
    let list = wr_list_new(0);
    let map = wr_map_new();
    let name_ptrs = [b"x".as_ptr()];
    let name_lens = [1usize];
    let class = wr_class_new(TypeId::UserBase as u32, name_ptrs.as_ptr(), name_lens.as_ptr(), 1);

    let key = wr_str_from_utf8(b"k".as_ptr(), 1);
    let val = wr_str_from_utf8(b"v".as_ptr(), 1);

    wr_list_push(list, key);
    wr_map_set(map, key, val);
    wr_class_set(class, b"x".as_ptr(), 1, val);

    wr_register_method(12, 0, noop);
    let actor = wr_actor_spawn(12, Value::nil(), 1, 3, -1, -1, -1);
    let arg = wr_str_from_utf8(b"a".as_ptr(), 1);
    let pending = wr_actor_send(actor, 0, 1, &arg as *const Value);
    let result = wr_pending_await(pending);

    unsafe {
        wr_rc_dec(key);
        wr_rc_dec(val);
        wr_rc_dec(list);
        wr_rc_dec(map);
        wr_rc_dec(class);
        wr_rc_dec(arg);
        wr_rc_dec(actor);
        wr_rc_dec(pending);
        wr_rc_dec(result);
    }

    let rc_inc =
        wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_INC as i64)).as_int();
    let rc_dec =
        wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_DEC as i64)).as_int();
    let released = rc_dec.saturating_sub(rc_inc);
    assert!(rc_dec >= rc_inc);
    assert!(released >= 9);
}
