use crate::*;

fn str_value(input: &str) -> Value {
    wr_str_from_utf8(input.as_ptr(), input.len())
}

fn value_to_string(input: Value) -> String {
    crate::string::with_string_bytes(input, |bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default()
}

fn dec(input: Value) {
    unsafe {
        wr_rc_dec(input);
    }
}

#[test]
fn boxing_round_trip() {
    let int = wr_box_int(42);
    assert_eq!(wr_unbox_int(int), 42);

    let float = wr_box_float(3.5);
    assert_eq!(wr_unbox_float(float), 3.5);
}

#[test]
fn string_and_bytes_round_trip() {
    let hello = str_value("hello");
    let world = str_value(" world");
    let parts = [hello, world];

    let joined = wr_str_concat(parts.as_ptr(), parts.len());
    assert_eq!(value_to_string(joined), "hello world");

    let bytes = wr_bytes_from_string(joined);
    let len = wr_bytes_len(bytes);
    assert_eq!(len.as_int(), 11);

    let decoded = wr_bytes_to_string(bytes);
    assert_eq!(value_to_string(decoded), "hello world");

    dec(hello);
    dec(world);
    dec(joined);
    dec(bytes);
    dec(decoded);
}

#[test]
fn list_and_map_ops() {
    let list = wr_list_new(0);
    let one = wr_box_int(1);
    let two = wr_box_int(2);

    wr_list_push(list, one);
    wr_list_push(list, two);

    assert_eq!(wr_list_len(list).as_int(), 2);
    assert_eq!(wr_list_get(list, 1).as_int(), 2);

    let map = wr_map_new();
    let key = str_value("k");
    let val = str_value("v");
    let _ = wr_map_set(map, key, val);
    let got = wr_map_get(map, key);

    assert_eq!(value_to_string(got), "v");

    dec(list);
    dec(one);
    dec(two);
    dec(map);
    dec(key);
    dec(val);
    dec(got);
}

#[test]
fn result_ops() {
    let ok = wr_result_ok(wr_box_int(7));
    assert!(wr_result_is_ok(ok).as_bool());
    assert_eq!(wr_result_unwrap(ok).as_int(), 7);

    let err_msg = str_value("bad");
    let err = wr_result_err(err_msg);
    assert!(!wr_result_is_ok(err).as_bool());
    assert_eq!(value_to_string(wr_result_err_unwrap(err)), "bad");

    dec(ok);
    dec(err_msg);
    dec(err);
}

#[test]
fn env_ops() {
    let key = str_value("WRELA_TEST_ENV");
    let val = str_value("ok");

    let _ = wr_env_set(key, val);
    let got = wr_env_get(key);
    assert_eq!(value_to_string(got), "ok");

    dec(key);
    dec(val);
    dec(got);
}

#[test]
fn runtime_configure_smoke() {
    let names = [b"actor_batch_limit".as_ptr()];
    let lens = [17usize];
    let cfg = wr_class_new(1001, names.as_ptr(), lens.as_ptr(), 1);
    wr_class_set(cfg, b"actor_batch_limit".as_ptr(), 17, Value::from_int(4));

    let result = wr_runtime_configure(cfg);

    dec(cfg);
    dec(result);
}

#[test]
#[should_panic(expected = "actor_mailbox_cap")]
fn runtime_configure_rejects_normalized_negative_capacity() {
    let names = [b"actor_mailbox_cap".as_ptr()];
    let lens = [17usize];
    let cfg = wr_class_new(1002, names.as_ptr(), lens.as_ptr(), 1);
    wr_class_set(cfg, b"actor_mailbox_cap".as_ptr(), 17, Value::from_int(-1));
    let _ = crate::config::runtime_configure(cfg);
}

#[test]
fn actor_spawn_rejects_legacy_objective_fallback() {
    let actor = crate::actor::actor_spawn(1, Value::nil(), 1, 7, 256, 10, 64);
    assert!(actor.is_nil());
}

#[test]
fn actor_spawn_legacy_default_sentinel_uses_runtime_config() {
    let actor = crate::actor::actor_spawn(1, Value::nil(), 1, 3, -1, 10, 64);
    assert!(!actor.is_nil());
    dec(actor);
}
