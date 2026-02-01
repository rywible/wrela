use super::*;
use crate::string;
use crate::value::value_eq;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpListener;
use tokio::time::Duration;

fn await_ok(pending: Value) -> Value {
    let result = wr_pending_await(pending);
    let ok = wr_result_is_ok(result);
    assert!(ok.is_bool());
    assert!(ok.as_bool());
    let val = wr_result_unwrap(result);
    unsafe {
        wr_rc_dec(result);
        wr_rc_dec(ok);
    }
    val
}

fn await_result_ok(pending: Value) -> Value {
    let result = await_ok(pending);
    let ok = wr_result_is_ok(result);
    assert!(ok.is_bool());
    assert!(ok.as_bool());
    let val = wr_result_unwrap(result);
    unsafe {
        wr_rc_dec(result);
        wr_rc_dec(ok);
    }
    val
}

fn value_to_string_test(val: Value) -> String {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

fn storage_get_string_test(key: &str) -> Option<String> {
    let key_val = wr_str_from_utf8(key.as_ptr(), key.len());
    let pending = wr_storage_get(key_val);
    let result = await_result_ok(pending);
    unsafe {
        wr_rc_dec(key_val);
        wr_rc_dec(pending);
    }
    if result.is_nil() {
        return None;
    }
    let out = value_to_string_test(result);
    unsafe { wr_rc_dec(result) };
    Some(out)
}

fn storage_get_json_test(key: &str) -> Option<serde_json::Value> {
    storage_get_string_test(key).and_then(|raw| serde_json::from_str(&raw).ok())
}

fn storage_set_string_test(key: &str, value: &str) {
    let key_val = wr_str_from_utf8(key.as_ptr(), key.len());
    let value_val = wr_str_from_utf8(value.as_ptr(), value.len());
    let pending = wr_storage_set(key_val, value_val);
    let result = await_result_ok(pending);
    unsafe {
        wr_rc_dec(key_val);
        wr_rc_dec(value_val);
        wr_rc_dec(pending);
        wr_rc_dec(result);
    }
}

fn net_available() -> bool {
    use std::io::ErrorKind;

    static AVAIL: OnceLock<bool> = OnceLock::new();
    *AVAIL.get_or_init(|| {
        match crate::actor::runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await }) {
            Ok(listener) => {
                drop(listener);
                true
            }
            Err(err) if err.kind() == ErrorKind::PermissionDenied => false,
            Err(err) => panic!("bind: {err}"),
        }
    })
}

fn test_storage() -> Value {
    static STORAGE: OnceLock<Value> = OnceLock::new();
    *STORAGE.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wrela.db");
        std::mem::forget(dir);
        unsafe {
            std::env::set_var("WRELA_STORE_PATH", path.to_string_lossy().to_string());
            std::env::set_var("WRELA_RAFT_HTTP_ENABLED", "0");
        }
        let fields = [b"file_path".as_ptr()];
        let lens = [9usize];
        let cfg = wr_class_new(300, fields.as_ptr(), lens.as_ptr(), 1);
        let path_str = path.to_str().unwrap();
        let path_val = wr_str_from_utf8(path_str.as_ptr(), path_str.len());
        wr_class_set(cfg, b"file_path".as_ptr(), 9, path_val);
        let res = wr_storage_configure(cfg);
        let ok = wr_result_is_ok(res);
        assert!(ok.as_bool());
        let _ = wr_result_unwrap(res);
        unsafe {
            wr_rc_dec(path_val);
            wr_rc_dec(cfg);
            wr_rc_dec(res);
            wr_rc_dec(ok);
        }
        wr_class_new(301, std::ptr::null(), std::ptr::null(), 0)
    })
}

#[cfg(any(test, feature = "test-utils"))]
struct HaNode {
    config: crate::storage::config::StorageConfig,
    service: Arc<crate::storage::service::StorageService>,
    scheduler: crate::schedule::SchedulerStateHandle,
    jobs: crate::jobs::JobsStateHandle,
    realtime: crate::realtime::RealtimeStateHandle,
}

#[cfg(any(test, feature = "test-utils"))]
async fn with_node<F, R>(node: &HaNode, fut: F) -> R
where
    F: Future<Output = R>,
{
    crate::storage::config::with_storage_config_override(
        node.config.clone(),
        crate::storage::service::StorageService::with_storage_override(
            node.service.clone(),
            crate::schedule::with_scheduler_state_override(
                &node.scheduler,
                crate::jobs::with_jobs_state_override(
                    &node.jobs,
                    crate::realtime::with_realtime_state_override(&node.realtime, fut),
                ),
            ),
        ),
    )
    .await
}

#[cfg(any(test, feature = "test-utils"))]
async fn try_bind(addr: &str) -> Option<(TcpListener, String)> {
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            let addr = listener.local_addr().ok()?.to_string();
            Some((listener, addr))
        }
        Err(err) => {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                None
            } else {
                panic!("bind: {err}");
            }
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn config_for_node(
    path: String,
    node_id: u64,
    bind_addr: String,
    peers: HashMap<u64, String>,
    bootstrap: bool,
) -> crate::storage::config::StorageConfig {
    let blob_path = format!("{path}.blobs");
    crate::storage::config::StorageConfig {
        enabled: true,
        path,
        node_id,
        bind_addr,
        http_enabled: true,
        peer_token: None,
        peers,
        bootstrap,
        snapshot_interval: 200,
        batch_max_ops: 4,
        batch_max_ms: 2,
        queue_cap: 64,
        blob: crate::storage::config::BlobConfig {
            threshold_bytes: 256 * 1024,
            file_path: blob_path,
            s3: None,
        },
        backup: crate::storage::config::BackupConfig {
            enabled: false,
            max_age_secs: 3600,
            max_logs: 100_000,
            retention_days: 7,
            max_keep: 0,
            prefix: "backups".to_string(),
            only_leader: true,
            restore_mode: crate::storage::config::RestoreMode::Single,
            restore_id: None,
        },
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn peers_for(node_id: u64, all: &HashMap<u64, String>) -> HashMap<u64, String> {
    let mut peers = all.clone();
    peers.remove(&node_id);
    peers
}

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
fn storage_scan_prefix_cas_batch() {
    let _storage = test_storage();
    storage_set_string_test("scan:a", "1");
    storage_set_string_test("scan:b", "2");
    storage_set_string_test("scan:c", "3");
    assert_eq!(storage_get_string_test("scan:a").as_deref(), Some("1"));

    let key_a = wr_str_from_utf8(b"scan:a".as_ptr(), 6);
    let pending = wr_storage_get_with_version(key_a);
    let map_val = await_result_ok(pending);
    assert!(!map_val.is_nil());
    let key_version = wr_str_from_utf8(b"version".as_ptr(), 7);
    let version_val = wr_map_get(map_val, key_version);
    let version = version_val.as_int() as i64;
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(key_version);
        wr_rc_dec(version_val);
        wr_rc_dec(map_val);
    }

    let updated_value = wr_str_from_utf8(b"one".as_ptr(), 3);
    let ver_val = Value::from_int(version);
    let cas_pending = wr_storage_set_if_version(key_a, updated_value, ver_val);
    let cas_result = await_result_ok(cas_pending);
    assert!(cas_result.is_bool());
    assert!(cas_result.as_bool());
    unsafe {
        wr_rc_dec(updated_value);
        wr_rc_dec(cas_pending);
        wr_rc_dec(cas_result);
    }

    let bad_version = Value::from_int(version);
    let bad_value = wr_str_from_utf8(b"nope".as_ptr(), 4);
    let bad_pending = wr_storage_set_if_version(key_a, bad_value, bad_version);
    let bad_result = await_result_ok(bad_pending);
    assert!(bad_result.is_bool());
    assert!(!bad_result.as_bool());
    unsafe {
        wr_rc_dec(bad_value);
        wr_rc_dec(bad_pending);
        wr_rc_dec(bad_result);
    }

    let pending = wr_storage_get_with_version(key_a);
    let map_val = await_result_ok(pending);
    let key_version = wr_str_from_utf8(b"version".as_ptr(), 7);
    let version_val = wr_map_get(map_val, key_version);
    let new_version = version_val.as_int();
    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(key_version);
        wr_rc_dec(version_val);
        wr_rc_dec(map_val);
    }

    let delete_bad = wr_storage_delete_if_version(key_a, Value::from_int(new_version - 1));
    let delete_bad_res = await_result_ok(delete_bad);
    assert!(delete_bad_res.is_bool());
    assert!(!delete_bad_res.as_bool());
    unsafe {
        wr_rc_dec(delete_bad);
        wr_rc_dec(delete_bad_res);
    }

    let delete_ok = wr_storage_delete_if_version(key_a, Value::from_int(new_version));
    let delete_ok_res = await_result_ok(delete_ok);
    assert!(delete_ok_res.is_bool());
    assert!(delete_ok_res.as_bool());
    unsafe {
        wr_rc_dec(delete_ok);
        wr_rc_dec(delete_ok_res);
    }

    let list = wr_list_new(0);
    let entry1 = wr_map_new();
    let entry2 = wr_map_new();
    let key_key = wr_str_from_utf8(b"key".as_ptr(), 3);
    let value_key = wr_str_from_utf8(b"value".as_ptr(), 5);
    let k1 = wr_str_from_utf8(b"batch:1".as_ptr(), 7);
    let v1 = wr_str_from_utf8(b"ok1".as_ptr(), 3);
    let k2 = wr_str_from_utf8(b"batch:2".as_ptr(), 7);
    let v2 = wr_str_from_utf8(b"ok2".as_ptr(), 3);
    wr_map_set(entry1, key_key, k1);
    wr_map_set(entry1, value_key, v1);
    wr_map_set(entry2, key_key, k2);
    wr_map_set(entry2, value_key, v2);
    wr_list_push(list, entry1);
    wr_list_push(list, entry2);
    let batch_pending = wr_storage_batch_set(list);
    let batch_result = await_result_ok(batch_pending);
    assert!(batch_result.is_bool());
    assert!(batch_result.as_bool());
    unsafe {
        wr_rc_dec(key_key);
        wr_rc_dec(value_key);
        wr_rc_dec(k1);
        wr_rc_dec(v1);
        wr_rc_dec(k2);
        wr_rc_dec(v2);
        wr_rc_dec(entry1);
        wr_rc_dec(entry2);
        wr_rc_dec(list);
        wr_rc_dec(batch_pending);
        wr_rc_dec(batch_result);
    }

    assert_eq!(storage_get_string_test("batch:1").as_deref(), Some("ok1"));
    assert_eq!(storage_get_string_test("batch:2").as_deref(), Some("ok2"));

    let prefix = wr_str_from_utf8(b"scan:".as_ptr(), 5);
    let limit = Value::from_int(10);
    let mut prefix_list = Value::nil();
    let mut len = 0;
    for _ in 0..20 {
        if !prefix_list.is_nil() {
            unsafe { wr_rc_dec(prefix_list) };
        }
        let prefix_pending = wr_storage_list_prefix(prefix, limit);
        prefix_list = await_result_ok(prefix_pending);
        let len_val = wr_list_len(prefix_list);
        len = len_val.as_int();
        unsafe {
            wr_rc_dec(prefix_pending);
            wr_rc_dec(len_val);
        }
        if len >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(len >= 2);
    unsafe {
        wr_rc_dec(prefix);
    }
    let first = wr_list_get(prefix_list, 0);
    let first_key = value_to_string_test(first);
    assert!(first_key.starts_with("scan:"));
    unsafe {
        wr_rc_dec(first);
        wr_rc_dec(prefix_list);
    }

    let start = wr_str_from_utf8(b"scan:a".as_ptr(), 6);
    let end = wr_str_from_utf8(b"scan:z".as_ptr(), 6);
    let mut scan_list = Value::nil();
    let mut scan_len = 0usize;
    for _ in 0..20 {
        if !scan_list.is_nil() {
            unsafe { wr_rc_dec(scan_list) };
        }
        let scan_pending = wr_storage_scan(start, end, Value::from_int(10));
        scan_list = await_result_ok(scan_pending);
        let scan_len_val = wr_list_len(scan_list);
        scan_len = scan_len_val.as_int() as usize;
        unsafe {
            wr_rc_dec(scan_pending);
            wr_rc_dec(scan_len_val);
        }
        if scan_len >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(scan_len >= 2);
    let entry = wr_list_get(scan_list, 0);
    let key_field = wr_str_from_utf8(b"key".as_ptr(), 3);
    let value_field = wr_str_from_utf8(b"value".as_ptr(), 5);
    let got_key = wr_map_get(entry, key_field);
    let got_value = wr_map_get(entry, value_field);
    let got_key_str = value_to_string_test(got_key);
    let got_value_str = value_to_string_test(got_value);
    assert!(got_key_str.starts_with("scan:"));
    assert!(!got_value_str.is_empty());
    unsafe {
        wr_rc_dec(start);
        wr_rc_dec(end);
        wr_rc_dec(entry);
        wr_rc_dec(key_field);
        wr_rc_dec(value_field);
        wr_rc_dec(got_key);
        wr_rc_dec(got_value);
        wr_rc_dec(scan_list);
        wr_rc_dec(key_a);
    }
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
fn env_load_and_get_precedence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "PORT=1234\nDEBUG=true\nTIMEOUT=10\n").expect("write .env");

    let path_str = env_path.to_str().unwrap();
    let path_val = wr_str_from_utf8(path_str.as_ptr(), path_str.len());
    let ok = wr_env_load(path_val);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    let key = wr_str_from_utf8(b"PORT".as_ptr(), 4);
    let val = wr_env_get(key);
    let got = string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap();
    assert_eq!(got, "1234");

    unsafe {
        std::env::set_var("PORT", "9999");
    }
    let val2 = wr_env_get(key);
    let got2 = string::with_string_bytes(val2, |bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap();
    assert_eq!(got2, "9999");

    unsafe {
        std::env::remove_var("PORT");
    }
    unsafe {
        wr_rc_dec(path_val);
        wr_rc_dec(key);
        wr_rc_dec(val);
        wr_rc_dec(val2);
    }
}

#[test]
fn env_parse_bool_and_int() {
    let key_set = wr_str_from_utf8(b"WRELA_SET".as_ptr(), b"WRELA_SET".len());
    let val_set = wr_str_from_utf8(b"set-ok".as_ptr(), 6);
    let did_set = wr_env_set(key_set, val_set);
    assert!(did_set.is_bool());
    assert!(did_set.as_bool());
    let got_set = wr_env_get(key_set);
    let got_set_str =
        string::with_string_bytes(got_set, |bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap();
    assert_eq!(got_set_str, "set-ok");

    unsafe {
        std::env::set_var("WRELA_BOOL", "on");
        std::env::set_var("WRELA_INT", "42");
        std::env::set_var("WRELA_BAD_BOOL", "maybe");
        std::env::set_var("WRELA_BAD_INT", "oops");
    }

    let key_bool = wr_str_from_utf8(b"WRELA_BOOL".as_ptr(), b"WRELA_BOOL".len());
    let key_int = wr_str_from_utf8(b"WRELA_INT".as_ptr(), b"WRELA_INT".len());
    let key_bad_bool = wr_str_from_utf8(b"WRELA_BAD_BOOL".as_ptr(), b"WRELA_BAD_BOOL".len());
    let key_bad_int = wr_str_from_utf8(b"WRELA_BAD_INT".as_ptr(), b"WRELA_BAD_INT".len());

    let val_bool = wr_env_get_as_bool(key_bool);
    assert!(val_bool.is_bool());
    assert!(val_bool.as_bool());

    let val_int = wr_env_get_as_int(key_int);
    assert!(val_int.is_int());
    assert_eq!(val_int.as_int(), 42);

    let val_bad_bool = wr_env_get_as_bool(key_bad_bool);
    assert!(val_bad_bool.is_nil());

    let val_bad_int = wr_env_get_as_int(key_bad_int);
    assert!(val_bad_int.is_nil());

    unsafe {
        std::env::remove_var("WRELA_BOOL");
        std::env::remove_var("WRELA_INT");
        std::env::remove_var("WRELA_BAD_BOOL");
        std::env::remove_var("WRELA_BAD_INT");
        std::env::remove_var("WRELA_SET");
    }

    unsafe {
        wr_rc_dec(key_set);
        wr_rc_dec(val_set);
        wr_rc_dec(got_set);
        wr_rc_dec(key_bool);
        wr_rc_dec(key_int);
        wr_rc_dec(key_bad_bool);
        wr_rc_dec(key_bad_int);
    }
}

#[test]
fn auth_create_verify_and_tokens() {
    let storage = test_storage();
    let email = wr_str_from_utf8(b"user@example.com".as_ptr(), b"user@example.com".len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    assert!(user_map.is_ptr());

    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);
    assert!(!id_val.is_nil());

    let verify_pending = wr_auth_verify_password(storage, id_val, password);
    let ok = await_ok(verify_pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    let token_pending = wr_auth_issue_email_token(storage, id_val, Value::from_int(3600));
    let token = await_ok(token_pending);
    assert!(!token.is_nil());

    let verify_email_pending = wr_auth_verify_email_token(storage, token);
    let verified_id = await_ok(verify_email_pending);
    assert!(!verified_id.is_nil());

    let claims = wr_map_new();
    let claim_key = wr_str_from_utf8(b"role".as_ptr(), 4);
    let claim_val = wr_str_from_utf8(b"admin".as_ptr(), 5);
    wr_map_set(claims, claim_key, claim_val);
    let jwt_pending = wr_auth_issue_jwt(storage, id_val, claims, Value::from_int(3600));
    let jwt = await_ok(jwt_pending);
    assert!(!jwt.is_nil());

    let verify_jwt_pending = wr_auth_verify_jwt(storage, jwt);
    let claims_map = await_ok(verify_jwt_pending);
    assert!(!claims_map.is_nil());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(verify_pending);
        wr_rc_dec(ok);
        wr_rc_dec(token_pending);
        wr_rc_dec(token);
        wr_rc_dec(verify_email_pending);
        wr_rc_dec(verified_id);
        wr_rc_dec(claims);
        wr_rc_dec(claim_key);
        wr_rc_dec(claim_val);
        wr_rc_dec(jwt_pending);
        wr_rc_dec(jwt);
        wr_rc_dec(verify_jwt_pending);
        wr_rc_dec(claims_map);
    }
}

#[test]
fn auth_persists_and_rejects_duplicates() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("user+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);
    let user_id = value_to_string_test(id_val);

    let email_key = format!("auth:email:{email_str}");
    let stored_id = storage_get_string_test(&email_key).expect("email index");
    assert_eq!(stored_id, user_id);

    let user_key = format!("auth:user:{user_id}");
    let user_json = storage_get_string_test(&user_key).expect("user record");
    let parsed: serde_json::Value = serde_json::from_str(&user_json).expect("json");
    assert_eq!(
        parsed.get("email").and_then(|v| v.as_str()).unwrap(),
        email_str
    );

    let dup_pending = wr_auth_create_user(storage, email, username, password);
    let dup = await_ok(dup_pending);
    assert!(dup.is_nil());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(dup_pending);
        wr_rc_dec(dup);
    }
}

#[test]
fn auth_email_token_expires() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("token+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);

    let token_pending = wr_auth_issue_email_token(storage, id_val, Value::from_int(1));
    let token = await_ok(token_pending);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let verify_pending = wr_auth_verify_email_token(storage, token);
    let verified = await_ok(verify_pending);
    assert!(verified.is_nil());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(token_pending);
        wr_rc_dec(token);
        wr_rc_dec(verify_pending);
        wr_rc_dec(verified);
    }
}

#[test]
fn auth_email_token_single_use() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("singleuse+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);

    let token_pending = wr_auth_issue_email_token(storage, id_val, Value::from_int(3600));
    let token = await_ok(token_pending);
    let verify_pending = wr_auth_verify_email_token(storage, token);
    let verified = await_ok(verify_pending);
    assert!(!verified.is_nil());

    let verify_pending2 = wr_auth_verify_email_token(storage, token);
    let verified2 = await_ok(verify_pending2);
    assert!(verified2.is_nil());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(token_pending);
        wr_rc_dec(token);
        wr_rc_dec(verify_pending);
        wr_rc_dec(verified);
        wr_rc_dec(verify_pending2);
        wr_rc_dec(verified2);
    }
}

#[test]
fn auth_jwt_includes_custom_claims() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("claims+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);

    let claims = wr_map_new();
    let claim_key = wr_str_from_utf8(b"tier".as_ptr(), 4);
    let claim_val = wr_str_from_utf8(b"pro".as_ptr(), 3);
    wr_map_set(claims, claim_key, claim_val);
    let jwt_pending = wr_auth_issue_jwt(storage, id_val, claims, Value::from_int(3600));
    let jwt = await_ok(jwt_pending);
    let verify_pending = wr_auth_verify_jwt(storage, jwt);
    let claims_map = await_ok(verify_pending);
    let tier_val = wr_map_get(claims_map, claim_key);
    assert_eq!(value_to_string_test(tier_val), "pro");

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(claims);
        wr_rc_dec(claim_key);
        wr_rc_dec(claim_val);
        wr_rc_dec(jwt_pending);
        wr_rc_dec(jwt);
        wr_rc_dec(verify_pending);
        wr_rc_dec(claims_map);
        wr_rc_dec(tier_val);
    }
}

#[test]
fn auth_rejects_bad_password() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("badpass+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);

    let bad_pw = wr_str_from_utf8(b"wrong".as_ptr(), 5);
    let verify_pending = wr_auth_verify_password(storage, id_val, bad_pw);
    let ok = await_ok(verify_pending);
    assert!(ok.is_bool());
    assert!(!ok.as_bool());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(bad_pw);
        wr_rc_dec(verify_pending);
        wr_rc_dec(ok);
    }
}

#[test]
fn auth_bulk_persistence() {
    let storage = test_storage();
    let mut ids = Vec::new();
    for i in 0..10 {
        let email_str = format!("bulk{}-{}@example.com", i, uuid::Uuid::new_v4());
        let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
        let username = wr_str_from_utf8(b"user".as_ptr(), 4);
        let password = wr_str_from_utf8(b"pw".as_ptr(), 2);
        let pending = wr_auth_create_user(storage, email, username, password);
        let user_map = await_ok(pending);
        let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
        let id_val = wr_map_get(user_map, id_key);
        let user_id = value_to_string_test(id_val);
        ids.push((email_str.clone(), user_id.clone()));

        let email_key = format!("auth:email:{email_str}");
        assert_eq!(storage_get_string_test(&email_key).unwrap(), user_id);
        let user_key = format!("auth:user:{user_id}");
        assert!(storage_get_json_test(&user_key).is_some());

        unsafe {
            wr_rc_dec(email);
            wr_rc_dec(username);
            wr_rc_dec(password);
            wr_rc_dec(pending);
            wr_rc_dec(user_map);
            wr_rc_dec(id_key);
            wr_rc_dec(id_val);
        }
    }
    assert_eq!(ids.len(), 10);
}

#[test]
fn auth_email_verify_sets_flag() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("verify+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);
    let user_id = value_to_string_test(id_val);

    let token = await_ok(wr_auth_issue_email_token(
        storage,
        id_val,
        Value::from_int(3600),
    ));
    let _ = await_ok(wr_auth_verify_email_token(storage, token));

    let user_key = format!("auth:user:{user_id}");
    let user_json = storage_get_json_test(&user_key).expect("user json");
    assert_eq!(
        user_json.get("verified").and_then(|v| v.as_bool()),
        Some(true)
    );

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(token);
    }
}

#[test]
fn auth_jwt_expiry_enforced() {
    let storage = test_storage();
    let suffix = uuid::Uuid::new_v4().to_string();
    let email_str = format!("jwt-exp+{suffix}@example.com");
    let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
    let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
    let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);

    let pending = wr_auth_create_user(storage, email, username, password);
    let user_map = await_ok(pending);
    let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
    let id_val = wr_map_get(user_map, id_key);

    let jwt = await_ok(wr_auth_issue_jwt(
        storage,
        id_val,
        wr_map_new(),
        Value::from_int(1),
    ));
    let mut verified = Value::nil();
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        verified = await_ok(wr_auth_verify_jwt(storage, jwt));
        if verified.is_nil() {
            break;
        }
    }
    assert!(verified.is_nil());

    unsafe {
        wr_rc_dec(email);
        wr_rc_dec(username);
        wr_rc_dec(password);
        wr_rc_dec(pending);
        wr_rc_dec(user_map);
        wr_rc_dec(id_key);
        wr_rc_dec(id_val);
        wr_rc_dec(jwt);
        wr_rc_dec(verified);
    }
}

#[test]
fn auth_bulk_password_verification() {
    let storage = test_storage();
    let mut ids = Vec::new();
    for i in 0..5 {
        let email_str = format!("bulk-verify{}-{}@example.com", i, uuid::Uuid::new_v4());
        let email = wr_str_from_utf8(email_str.as_ptr(), email_str.len());
        let username = wr_str_from_utf8(b"tester".as_ptr(), 6);
        let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);
        let pending = wr_auth_create_user(storage, email, username, password);
        let user_map = await_ok(pending);
        let id_key = wr_str_from_utf8(b"id".as_ptr(), 2);
        let id_val = wr_map_get(user_map, id_key);
        ids.push(id_val);
        unsafe {
            wr_rc_dec(email);
            wr_rc_dec(username);
            wr_rc_dec(password);
            wr_rc_dec(pending);
            wr_rc_dec(user_map);
            wr_rc_dec(id_key);
        }
    }
    for id_val in ids.iter() {
        let password = wr_str_from_utf8(b"secret-pass".as_ptr(), 11);
        let ok = await_ok(wr_auth_verify_password(storage, *id_val, password));
        assert!(ok.as_bool());
        unsafe {
            wr_rc_dec(password);
            wr_rc_dec(ok);
        }
    }
    for id_val in ids {
        unsafe { wr_rc_dec(id_val) };
    }
}
#[test]
fn rbac_assign_and_check() {
    let storage = test_storage();
    let scope = wr_str_from_utf8(b"org".as_ptr(), 3);
    let name = wr_str_from_utf8(b"admin".as_ptr(), 5);
    let perms = wr_list_new(0);
    let perm = wr_str_from_utf8(b"repo:write".as_ptr(), 10);
    wr_list_push(perms, perm);

    let create_pending = wr_rbac_create_role(storage, scope, name, perms);
    let role_id = await_ok(create_pending);
    assert!(!role_id.is_nil());

    let user_id = wr_str_from_utf8(b"user-1".as_ptr(), 6);
    let scope_id = wr_str_from_utf8(b"org-1".as_ptr(), 5);
    let assign_pending = wr_rbac_assign_role(storage, user_id, role_id, scope_id);
    let assigned = await_ok(assign_pending);
    assert!(assigned.is_bool());
    assert!(assigned.as_bool());

    let check_pending = wr_rbac_check(storage, user_id, perm, scope_id);
    let allowed = await_ok(check_pending);
    assert!(allowed.is_bool());
    assert!(allowed.as_bool());

    let perms_pending = wr_rbac_permissions_for(storage, user_id, scope_id);
    let perms_list = await_ok(perms_pending);
    let len = wr_list_len(perms_list);
    assert_eq!(len.as_int(), 1);

    unsafe {
        wr_rc_dec(scope);
        wr_rc_dec(name);
        wr_rc_dec(perms);
        wr_rc_dec(perm);
        wr_rc_dec(create_pending);
        wr_rc_dec(role_id);
        wr_rc_dec(user_id);
        wr_rc_dec(scope_id);
        wr_rc_dec(assign_pending);
        wr_rc_dec(assigned);
        wr_rc_dec(check_pending);
        wr_rc_dec(allowed);
        wr_rc_dec(perms_pending);
        wr_rc_dec(perms_list);
        wr_rc_dec(len);
    }
}

#[test]
fn rbac_assignment_is_idempotent() {
    let storage = test_storage();
    let scope = wr_str_from_utf8(b"org".as_ptr(), 3);
    let name = wr_str_from_utf8(b"admin".as_ptr(), 5);
    let perms = wr_list_new(0);
    let perm = wr_str_from_utf8(b"repo:write".as_ptr(), 10);
    wr_list_push(perms, perm);

    let create_pending = wr_rbac_create_role(storage, scope, name, perms);
    let role_id = await_ok(create_pending);

    let user_id = wr_str_from_utf8(b"user-2".as_ptr(), 6);
    let scope_id = wr_str_from_utf8(b"org-2".as_ptr(), 5);
    let assign_pending = wr_rbac_assign_role(storage, user_id, role_id, scope_id);
    let _ = await_ok(assign_pending);
    let assign_pending2 = wr_rbac_assign_role(storage, user_id, role_id, scope_id);
    let _ = await_ok(assign_pending2);

    let perms_pending = wr_rbac_permissions_for(storage, user_id, scope_id);
    let perms_list = await_ok(perms_pending);
    let len = wr_list_len(perms_list);
    assert_eq!(len.as_int(), 1);

    unsafe {
        wr_rc_dec(scope);
        wr_rc_dec(name);
        wr_rc_dec(perms);
        wr_rc_dec(perm);
        wr_rc_dec(create_pending);
        wr_rc_dec(role_id);
        wr_rc_dec(user_id);
        wr_rc_dec(scope_id);
        wr_rc_dec(assign_pending);
        wr_rc_dec(assign_pending2);
        wr_rc_dec(perms_pending);
        wr_rc_dec(perms_list);
        wr_rc_dec(len);
    }
}

#[test]
fn rbac_scope_isolated() {
    let storage = test_storage();
    let scope = wr_str_from_utf8(b"org".as_ptr(), 3);
    let name = wr_str_from_utf8(b"reader".as_ptr(), 6);
    let perms = wr_list_new(0);
    let perm = wr_str_from_utf8(b"repo:read".as_ptr(), 9);
    wr_list_push(perms, perm);

    let create_pending = wr_rbac_create_role(storage, scope, name, perms);
    let role_id = await_ok(create_pending);

    let user_id = wr_str_from_utf8(b"user-3".as_ptr(), 6);
    let scope_a = wr_str_from_utf8(b"org-a".as_ptr(), 5);
    let scope_b = wr_str_from_utf8(b"org-b".as_ptr(), 5);
    let assign_pending = wr_rbac_assign_role(storage, user_id, role_id, scope_a);
    let _ = await_ok(assign_pending);

    let check_a = await_ok(wr_rbac_check(storage, user_id, perm, scope_a));
    let check_b = await_ok(wr_rbac_check(storage, user_id, perm, scope_b));
    assert!(check_a.as_bool());
    assert!(!check_b.as_bool());

    unsafe {
        wr_rc_dec(scope);
        wr_rc_dec(name);
        wr_rc_dec(perms);
        wr_rc_dec(perm);
        wr_rc_dec(create_pending);
        wr_rc_dec(role_id);
        wr_rc_dec(user_id);
        wr_rc_dec(scope_a);
        wr_rc_dec(scope_b);
        wr_rc_dec(assign_pending);
        wr_rc_dec(check_a);
        wr_rc_dec(check_b);
    }
}

#[test]
fn rbac_empty_permissions() {
    let storage = test_storage();
    let user_id = wr_str_from_utf8(b"user-empty".as_ptr(), 10);
    let scope_id = wr_str_from_utf8(b"scope-empty".as_ptr(), 11);
    let perms_pending = wr_rbac_permissions_for(storage, user_id, scope_id);
    let perms_list = await_ok(perms_pending);
    let len = wr_list_len(perms_list);
    assert_eq!(len.as_int(), 0);

    unsafe {
        wr_rc_dec(user_id);
        wr_rc_dec(scope_id);
        wr_rc_dec(perms_pending);
        wr_rc_dec(perms_list);
        wr_rc_dec(len);
    }
}

#[test]
fn rbac_permissions_union() {
    let storage = test_storage();
    let scope = wr_str_from_utf8(b"org".as_ptr(), 3);
    let role1 = wr_str_from_utf8(b"r1".as_ptr(), 2);
    let role2 = wr_str_from_utf8(b"r2".as_ptr(), 2);
    let perms1 = wr_list_new(0);
    let perms2 = wr_list_new(0);
    let p1 = wr_str_from_utf8(b"repo:read".as_ptr(), 9);
    let p2 = wr_str_from_utf8(b"repo:write".as_ptr(), 10);
    wr_list_push(perms1, p1);
    wr_list_push(perms2, p2);
    let id1 = await_ok(wr_rbac_create_role(storage, scope, role1, perms1));
    let id2 = await_ok(wr_rbac_create_role(storage, scope, role2, perms2));
    let user_id = wr_str_from_utf8(b"user-union".as_ptr(), 10);
    let scope_id = wr_str_from_utf8(b"org-u".as_ptr(), 5);
    let _ = await_ok(wr_rbac_assign_role(storage, user_id, id1, scope_id));
    let _ = await_ok(wr_rbac_assign_role(storage, user_id, id2, scope_id));
    let perms_list = await_ok(wr_rbac_permissions_for(storage, user_id, scope_id));
    let len = wr_list_len(perms_list);
    assert_eq!(len.as_int(), 2);

    unsafe {
        wr_rc_dec(scope);
        wr_rc_dec(role1);
        wr_rc_dec(role2);
        wr_rc_dec(perms1);
        wr_rc_dec(perms2);
        wr_rc_dec(p1);
        wr_rc_dec(p2);
        wr_rc_dec(id1);
        wr_rc_dec(id2);
        wr_rc_dec(user_id);
        wr_rc_dec(scope_id);
        wr_rc_dec(perms_list);
        wr_rc_dec(len);
    }
}

#[test]
fn files_upload_metadata_and_acl() {
    let storage = test_storage();
    let hello_str = wr_str_from_utf8(b"hello".as_ptr(), 5);
    let bytes = wr_bytes_from_string(hello_str);
    let opts = wr_map_new();
    let key_acl = wr_str_from_utf8(b"acl".as_ptr(), 3);
    let val_acl = wr_str_from_utf8(b"public".as_ptr(), 6);
    wr_map_set(opts, key_acl, val_acl);
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-1".as_ptr(), 6);
    wr_map_set(opts, key_owner, val_owner);

    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);
    assert!(!file_id.is_nil());

    let meta_pending = wr_files_metadata(storage, file_id);
    let meta = await_ok(meta_pending);
    assert!(!meta.is_nil());

    let key_acl_meta = wr_str_from_utf8(b"acl".as_ptr(), 3);
    let acl_val = wr_map_get(meta, key_acl_meta);
    let acl_str =
        string::with_string_bytes(acl_val, |bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap();
    assert_eq!(acl_str, "public");

    let private_acl = wr_str_from_utf8(b"private".as_ptr(), 7);
    let file_ref = wr_map_new();
    let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(file_ref, key_id, file_id);
    wr_map_set(file_ref, key_requester, val_owner);
    let set_acl_pending = wr_files_set_acl(storage, file_ref, private_acl);
    let set_acl_ok = await_ok(set_acl_pending);
    assert!(set_acl_ok.is_bool());
    assert!(set_acl_ok.as_bool());

    let signed_opts = wr_map_new();
    let key_requester_signed = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(signed_opts, key_requester_signed, val_owner);
    let url_pending = wr_files_signed_url(storage, file_id, signed_opts);
    let url = await_ok(url_pending);
    assert!(!url.is_nil());

    let delete_pending = wr_files_delete(storage, file_ref);
    let deleted = await_ok(delete_pending);
    assert!(deleted.is_bool());
    assert!(deleted.as_bool());

    unsafe {
        wr_rc_dec(hello_str);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_acl);
        wr_rc_dec(val_acl);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(meta_pending);
        wr_rc_dec(meta);
        wr_rc_dec(key_acl_meta);
        wr_rc_dec(acl_val);
        wr_rc_dec(set_acl_pending);
        wr_rc_dec(set_acl_ok);
        wr_rc_dec(private_acl);
        wr_rc_dec(file_ref);
        wr_rc_dec(key_id);
        wr_rc_dec(key_requester);
        wr_rc_dec(signed_opts);
        wr_rc_dec(key_requester_signed);
        wr_rc_dec(url_pending);
        wr_rc_dec(url);
        wr_rc_dec(delete_pending);
        wr_rc_dec(deleted);
    }
}

#[test]
fn files_delete_clears_metadata() {
    let storage = test_storage();
    let content = wr_str_from_utf8(b"data".as_ptr(), 4);
    let bytes = wr_bytes_from_string(content);
    let opts = wr_map_new();
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-del".as_ptr(), 8);
    wr_map_set(opts, key_owner, val_owner);
    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);

    let file_ref = wr_map_new();
    let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(file_ref, key_id, file_id);
    wr_map_set(file_ref, key_requester, val_owner);

    let delete_pending = wr_files_delete(storage, file_ref);
    let _ = await_ok(delete_pending);

    let meta_pending = wr_files_metadata(storage, file_ref);
    let meta = await_ok(meta_pending);
    assert!(meta.is_nil());

    unsafe {
        wr_rc_dec(content);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(file_ref);
        wr_rc_dec(key_id);
        wr_rc_dec(key_requester);
        wr_rc_dec(delete_pending);
        wr_rc_dec(meta_pending);
        wr_rc_dec(meta);
    }
}

#[test]
fn files_signed_url_contains_params() {
    let storage = test_storage();
    let content = wr_str_from_utf8(b"blob".as_ptr(), 4);
    let bytes = wr_bytes_from_string(content);
    let opts = wr_map_new();
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-2".as_ptr(), 6);
    wr_map_set(opts, key_owner, val_owner);
    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);

    let signed_opts = wr_map_new();
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(signed_opts, key_requester, val_owner);
    let key_ttl = wr_str_from_utf8(b"ttl".as_ptr(), 3);
    wr_map_set(signed_opts, key_ttl, Value::from_int(60));
    let key_method = wr_str_from_utf8(b"method".as_ptr(), 6);
    let val_method = wr_str_from_utf8(b"GET".as_ptr(), 3);
    wr_map_set(signed_opts, key_method, val_method);

    let url_pending = wr_files_signed_url(storage, file_id, signed_opts);
    let url = await_ok(url_pending);
    let url_str = value_to_string_test(url);
    assert!(url_str.contains("token="));
    assert!(url_str.contains("exp="));
    assert!(url_str.contains("method=GET"));

    unsafe {
        wr_rc_dec(content);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(signed_opts);
        wr_rc_dec(key_requester);
        wr_rc_dec(key_ttl);
        wr_rc_dec(key_method);
        wr_rc_dec(val_method);
        wr_rc_dec(url_pending);
        wr_rc_dec(url);
    }
}

#[test]
fn files_private_metadata_requires_requester() {
    let storage = test_storage();
    let content = wr_str_from_utf8(b"blob".as_ptr(), 4);
    let bytes = wr_bytes_from_string(content);
    let opts = wr_map_new();
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-meta".as_ptr(), 9);
    wr_map_set(opts, key_owner, val_owner);
    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);

    let meta_pending = wr_files_metadata(storage, file_id);
    let meta = await_ok(meta_pending);
    assert!(meta.is_nil());

    let file_ref = wr_map_new();
    let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(file_ref, key_id, file_id);
    wr_map_set(file_ref, key_requester, val_owner);

    let meta_allowed = await_ok(wr_files_metadata(storage, file_ref));
    assert!(!meta_allowed.is_nil());

    unsafe {
        wr_rc_dec(content);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(meta_pending);
        wr_rc_dec(meta);
        wr_rc_dec(file_ref);
        wr_rc_dec(key_id);
        wr_rc_dec(key_requester);
        wr_rc_dec(meta_allowed);
    }
}

#[test]
fn files_delete_requires_owner() {
    let storage = test_storage();
    let content = wr_str_from_utf8(b"data".as_ptr(), 4);
    let bytes = wr_bytes_from_string(content);
    let opts = wr_map_new();
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-del2".as_ptr(), 9);
    wr_map_set(opts, key_owner, val_owner);
    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);

    let delete_pending = wr_files_delete(storage, file_id);
    let deleted = await_ok(delete_pending);
    assert!(deleted.is_bool());
    assert!(!deleted.as_bool());

    let file_ref = wr_map_new();
    let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(file_ref, key_id, file_id);
    wr_map_set(file_ref, key_requester, val_owner);

    let delete_ok_pending = wr_files_delete(storage, file_ref);
    let delete_ok = await_ok(delete_ok_pending);
    assert!(delete_ok.is_bool());
    assert!(delete_ok.as_bool());

    unsafe {
        wr_rc_dec(content);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(delete_pending);
        wr_rc_dec(deleted);
        wr_rc_dec(file_ref);
        wr_rc_dec(key_id);
        wr_rc_dec(key_requester);
        wr_rc_dec(delete_ok_pending);
        wr_rc_dec(delete_ok);
    }
}

#[test]
fn files_acl_persists() {
    let storage = test_storage();
    let content = wr_str_from_utf8(b"data".as_ptr(), 4);
    let bytes = wr_bytes_from_string(content);
    let opts = wr_map_new();
    let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
    let val_owner = wr_str_from_utf8(b"user-acl".as_ptr(), 8);
    wr_map_set(opts, key_owner, val_owner);
    let pending = wr_files_upload_stream(storage, bytes, opts);
    let file_id = await_ok(pending);

    let file_ref = wr_map_new();
    let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
    let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
    wr_map_set(file_ref, key_id, file_id);
    wr_map_set(file_ref, key_requester, val_owner);

    let private_acl = wr_str_from_utf8(b"private".as_ptr(), 7);
    let set_acl_pending = wr_files_set_acl(storage, file_ref, private_acl);
    let _ = await_ok(set_acl_pending);

    let meta_pending = wr_files_metadata(storage, file_ref);
    let meta = await_ok(meta_pending);
    let key_acl = wr_str_from_utf8(b"acl".as_ptr(), 3);
    let acl_val = wr_map_get(meta, key_acl);
    assert_eq!(value_to_string_test(acl_val), "private");

    unsafe {
        wr_rc_dec(content);
        wr_rc_dec(bytes);
        wr_rc_dec(opts);
        wr_rc_dec(key_owner);
        wr_rc_dec(val_owner);
        wr_rc_dec(pending);
        wr_rc_dec(file_id);
        wr_rc_dec(file_ref);
        wr_rc_dec(key_id);
        wr_rc_dec(key_requester);
        wr_rc_dec(private_acl);
        wr_rc_dec(set_acl_pending);
        wr_rc_dec(meta_pending);
        wr_rc_dec(meta);
        wr_rc_dec(key_acl);
        wr_rc_dec(acl_val);
    }
}

#[test]
fn files_bulk_metadata() {
    let storage = test_storage();
    for _ in 0..5 {
        let content = wr_str_from_utf8(b"blob".as_ptr(), 4);
        let bytes = wr_bytes_from_string(content);
        let opts = wr_map_new();
        let key_owner = wr_str_from_utf8(b"owner_id".as_ptr(), 8);
        let val_owner = wr_str_from_utf8(b"user-bulk".as_ptr(), 9);
        wr_map_set(opts, key_owner, val_owner);
        let pending = wr_files_upload_stream(storage, bytes, opts);
        let file_id = await_ok(pending);
        let file_ref = wr_map_new();
        let key_id = wr_str_from_utf8(b"id".as_ptr(), 2);
        let key_requester = wr_str_from_utf8(b"requester_id".as_ptr(), 12);
        wr_map_set(file_ref, key_id, file_id);
        wr_map_set(file_ref, key_requester, val_owner);
        let meta = await_ok(wr_files_metadata(storage, file_ref));
        let key_size = wr_str_from_utf8(b"size".as_ptr(), 4);
        let size_val = wr_map_get(meta, key_size);
        assert_eq!(size_val.as_int(), 4);
        unsafe {
            wr_rc_dec(content);
            wr_rc_dec(bytes);
            wr_rc_dec(opts);
            wr_rc_dec(key_owner);
            wr_rc_dec(val_owner);
            wr_rc_dec(pending);
            wr_rc_dec(file_id);
            wr_rc_dec(file_ref);
            wr_rc_dec(key_id);
            wr_rc_dec(key_requester);
            wr_rc_dec(meta);
            wr_rc_dec(key_size);
            wr_rc_dec(size_val);
        }
    }
}

#[test]
fn jobs_process_and_dlq() {
    let storage = test_storage();
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn handle_ok(argc: usize, argv: *const Value) -> Value {
        let _ = (argc, argv);
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }

    extern "C" fn handle_fail(argc: usize, argv: *const Value) -> Value {
        let _ = (argc, argv);
        Value::nil()
    }

    const CLASS_OK: u32 = 20;
    const CLASS_FAIL: u32 = 21;

    wr_register_method(CLASS_OK, 0, handle_ok);
    wr_register_method(CLASS_FAIL, 0, handle_fail);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_OK, 0);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_FAIL, 0);

    let handler_ok = wr_actor_spawn(CLASS_OK as u64, Value::nil(), 1, 3, -1, -1, -1);
    let handler_fail = wr_actor_spawn(CLASS_FAIL as u64, Value::nil(), 1, 3, -1, -1, -1);

    let queue = wr_str_from_utf8(b"q".as_ptr(), 1);
    let process_pending = wr_jobs_process(storage, queue, handler_ok);
    let process_ok = await_ok(process_pending);
    assert!(process_ok.is_bool());
    assert!(process_ok.as_bool());

    let payload = wr_map_new();
    let enqueue_opts = wr_map_new();
    let enqueue_pending = wr_jobs_enqueue(storage, queue, payload, enqueue_opts);
    let _job_id = await_ok(enqueue_pending);

    for _ in 0..10 {
        if COUNT.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(COUNT.load(Ordering::SeqCst) > 0);

    let queue_fail = wr_str_from_utf8(b"q_fail".as_ptr(), 6);
    let process_fail_pending = wr_jobs_process(storage, queue_fail, handler_fail);
    let process_fail_ok = await_ok(process_fail_pending);
    assert!(process_fail_ok.is_bool());
    assert!(process_fail_ok.as_bool());

    let payload_fail = wr_map_new();
    let opts_fail = wr_map_new();
    let key_max = wr_str_from_utf8(b"max_retries".as_ptr(), 11);
    let val_zero = Value::from_int(0);
    wr_map_set(opts_fail, key_max, val_zero);
    let enqueue_fail_pending = wr_jobs_enqueue(storage, queue_fail, payload_fail, opts_fail);
    let _ = await_ok(enqueue_fail_pending);

    let mut dlq_len = Value::from_int(0);
    let mut dlq_list = Value::nil();
    for _ in 0..20 {
        if !dlq_list.is_nil() {
            unsafe { wr_rc_dec(dlq_list) };
        }
        let dlq_pending = wr_jobs_dead_letter(storage, queue_fail);
        dlq_list = await_ok(dlq_pending);
        dlq_len = wr_list_len(dlq_list);
        if dlq_len.as_int() == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(dlq_len.as_int(), 1);

    unsafe {
        wr_rc_dec(handler_ok);
        wr_rc_dec(handler_fail);
        wr_rc_dec(queue);
        wr_rc_dec(process_pending);
        wr_rc_dec(process_ok);
        wr_rc_dec(payload);
        wr_rc_dec(enqueue_opts);
        wr_rc_dec(enqueue_pending);
        wr_rc_dec(queue_fail);
        wr_rc_dec(process_fail_pending);
        wr_rc_dec(process_fail_ok);
        wr_rc_dec(payload_fail);
        wr_rc_dec(opts_fail);
        wr_rc_dec(key_max);
        wr_rc_dec(enqueue_fail_pending);
        wr_rc_dec(dlq_list);
        wr_rc_dec(dlq_len);
    }
}

#[test]
fn jobs_persists_queue_and_dlq() {
    let storage = test_storage();
    let queue_name = format!("q_{}", uuid::Uuid::new_v4());
    let queue = wr_str_from_utf8(queue_name.as_ptr(), queue_name.len());
    let payload = wr_map_new();
    let opts = wr_map_new();
    let pending = wr_jobs_enqueue(storage, queue, payload, opts);
    let job_id_val = await_ok(pending);
    let job_id = value_to_string_test(job_id_val);

    let queue_key = format!("jobs:queue:{queue_name}:{job_id}");
    let queue_json = storage_get_string_test(&queue_key).expect("queue stored");
    let parsed: serde_json::Value = serde_json::from_str(&queue_json).expect("queue json");
    assert!(parsed.is_object());

    unsafe {
        wr_rc_dec(queue);
        wr_rc_dec(payload);
        wr_rc_dec(opts);
        wr_rc_dec(pending);
        wr_rc_dec(job_id_val);
    }
}

#[test]
fn jobs_retry_increments_attempts() {
    let storage = test_storage();
    extern "C" fn handle_fail(_argc: usize, _argv: *const Value) -> Value {
        Value::nil()
    }
    const CLASS_FAIL: u32 = 41;
    wr_register_method(CLASS_FAIL, 0, handle_fail);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_FAIL, 0);
    let handler_fail = wr_actor_spawn(CLASS_FAIL as u64, Value::nil(), 1, 3, -1, -1, -1);

    let queue_name = format!("q_retry_{}", uuid::Uuid::new_v4());
    let queue = wr_str_from_utf8(queue_name.as_ptr(), queue_name.len());
    let _ = await_ok(wr_jobs_process(storage, queue, handler_fail));

    let payload = wr_map_new();
    let opts = wr_map_new();
    let key_max = wr_str_from_utf8(b"max_retries".as_ptr(), 11);
    let key_backoff = wr_str_from_utf8(b"backoff".as_ptr(), 7);
    wr_map_set(opts, key_max, Value::from_int(0));
    wr_map_set(opts, key_backoff, Value::from_int(0));
    let _ = await_ok(wr_jobs_enqueue(storage, queue, payload, opts));

    let mut dlq_list = Value::nil();
    let mut len = Value::from_int(0);
    for _ in 0..40 {
        if !dlq_list.is_nil() {
            unsafe { wr_rc_dec(dlq_list) };
        }
        dlq_list = await_ok(wr_jobs_dead_letter(storage, queue));
        len = wr_list_len(dlq_list);
        if len.as_int() == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(len.as_int(), 1);
    let entry = wr_list_get(dlq_list, 0);
    let attempts_key = wr_str_from_utf8(b"attempts".as_ptr(), 8);
    let attempts_val = wr_map_get(entry, attempts_key);
    assert!(attempts_val.is_int());
    assert!(attempts_val.as_int() >= 1);

    unsafe {
        wr_rc_dec(handler_fail);
        wr_rc_dec(queue);
        wr_rc_dec(payload);
        wr_rc_dec(opts);
        wr_rc_dec(key_max);
        wr_rc_dec(key_backoff);
        wr_rc_dec(dlq_list);
        wr_rc_dec(len);
        wr_rc_dec(entry);
        wr_rc_dec(attempts_key);
        wr_rc_dec(attempts_val);
    }
}

#[test]
fn jobs_process_picks_up_persisted_queue() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_ok(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_OK: u32 = 50;
    wr_register_method(CLASS_OK, 0, handle_ok);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_OK, 0);
    let handler = wr_actor_spawn(CLASS_OK as u64, Value::nil(), 1, 3, -1, -1, -1);

    let storage = test_storage();
    let queue_name = format!("q_persist_{}", uuid::Uuid::new_v4());
    let queue = wr_str_from_utf8(queue_name.as_ptr(), queue_name.len());
    let payload = wr_map_new();
    let opts = wr_map_new();
    let _ = await_ok(wr_jobs_enqueue(storage, queue, payload, opts));

    let _ = await_ok(wr_jobs_process(storage, queue, handler));
    for _ in 0..20 {
        if COUNT.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(COUNT.load(Ordering::SeqCst) > 0);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(queue);
        wr_rc_dec(payload);
        wr_rc_dec(opts);
    }
}

#[test]
fn jobs_dlq_bulk_count() {
    let storage = test_storage();
    extern "C" fn handle_fail(_argc: usize, _argv: *const Value) -> Value {
        Value::nil()
    }
    const CLASS_FAIL: u32 = 70;
    wr_register_method(CLASS_FAIL, 0, handle_fail);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_FAIL, 0);
    let handler = wr_actor_spawn(CLASS_FAIL as u64, Value::nil(), 1, 3, -1, -1, -1);

    let queue_name = format!("q_dlq_{}", uuid::Uuid::new_v4());
    let queue = wr_str_from_utf8(queue_name.as_ptr(), queue_name.len());
    let _ = await_ok(wr_jobs_process(storage, queue, handler));

    let opts = wr_map_new();
    let key_max = wr_str_from_utf8(b"max_retries".as_ptr(), 11);
    let key_backoff = wr_str_from_utf8(b"backoff".as_ptr(), 7);
    wr_map_set(opts, key_max, Value::from_int(0));
    wr_map_set(opts, key_backoff, Value::from_int(0));
    for _ in 0..3 {
        let payload = wr_map_new();
        let _ = await_ok(wr_jobs_enqueue(storage, queue, payload, opts));
        unsafe { wr_rc_dec(payload) };
    }
    let mut dlq_len = Value::from_int(0);
    for _ in 0..30 {
        let dlq = await_ok(wr_jobs_dead_letter(storage, queue));
        dlq_len = wr_list_len(dlq);
        let len = dlq_len.as_int();
        unsafe { wr_rc_dec(dlq) };
        if len == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(dlq_len.as_int(), 3);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(queue);
        wr_rc_dec(opts);
        wr_rc_dec(key_max);
        wr_rc_dec(key_backoff);
        wr_rc_dec(dlq_len);
    }
}

#[test]
fn schedule_every_runs() {
    let storage = test_storage();
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn handle_tick(argc: usize, argv: *const Value) -> Value {
        let _ = (argc, argv);
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }

    const CLASS_TICK: u32 = 30;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);

    let handler = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);
    let pending = wr_schedule_every(storage, Value::from_int(1), handler);
    let ok = await_ok(pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    for _ in 0..10 {
        if COUNT.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(COUNT.load(Ordering::SeqCst) > 0);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(pending);
        wr_rc_dec(ok);
    }
}

#[test]
fn schedule_persists_entries() {
    let storage = test_storage();
    extern "C" fn schedule_handle(_argc: usize, _argv: *const Value) -> Value {
        Value::from_bool(true)
    }
    wr_register_method(31, 0, schedule_handle);
    wr_register_method_name(b"handle".as_ptr(), 6, 31, 0);
    let handler = wr_actor_spawn(31, Value::nil(), 1, 3, -1, -1, -1);

    let pending = wr_schedule_every(storage, Value::from_int(5), handler);
    let _ = await_ok(pending);
    let mut entries = None;
    for _ in 0..20 {
        entries = storage_get_string_test("schedule:entries");
        if entries.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let entries = entries.expect("entries");
    let parsed: serde_json::Value = serde_json::from_str(&entries).expect("json");
    assert!(
        parsed
            .as_array()
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    );

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(pending);
    }
}

#[test]
fn schedule_at_immediate_runs() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 60;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    let handler = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);

    let storage = test_storage();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let pending = wr_schedule_at(storage, Value::from_int(now), handler);
    let _ = await_ok(pending);
    for _ in 0..20 {
        if COUNT.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(COUNT.load(Ordering::SeqCst) > 0);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(pending);
    }
}

#[test]
fn schedule_entries_count() {
    let storage = test_storage();
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 80;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    let handler = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);
    let _ = await_ok(wr_schedule_every(storage, Value::from_int(5), handler));
    let _ = await_ok(wr_schedule_cron(
        storage,
        wr_str_from_utf8(b"* * * * *".as_ptr(), 9),
        handler,
    ));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let _ = await_ok(wr_schedule_at(storage, Value::from_int(now), handler));

    let mut count = 0;
    for _ in 0..40 {
        let entries = storage_get_json_test("schedule:entries");
        count = entries
            .as_ref()
            .and_then(|val| val.as_array().map(|arr| arr.len()))
            .unwrap_or(0);
        if count >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(count >= 3);

    unsafe {
        wr_rc_dec(handler);
    }
}

#[test]
fn schedule_run_dedupes() {
    let _storage = test_storage();
    let run_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ok_first = crate::actor::runtime_block_on(crate::schedule::claim_run_for_test(
        "dedupe",
        run_at,
    ));
    let ok_second = crate::actor::runtime_block_on(crate::schedule::claim_run_for_test(
        "dedupe",
        run_at,
    ));
    assert!(ok_first);
    assert!(!ok_second);
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_schedule_runs_once_across_nodes() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);
    all.insert(3, addr3);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );
    let cfg3 = config_for_node(
        format!("{base}/db3"),
        3,
        "127.0.0.1:0".to_string(),
        peers_for(3, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();
    let realtime3 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );
    let service3 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg3.clone(),
            listener3,
            realtime3.as_arc(),
        )
        .await
        .expect("start service3"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };
    let node3 = HaNode {
        config: cfg3,
        service: service3,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime3,
    };

    with_node(&node1, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;
    with_node(&node2, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;
    with_node(&node3, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 201;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    let handler = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let pending = with_node(&node1, async {
        wr_schedule_at(Value::from_bool(true), Value::from_int(now), handler)
    })
    .await;
    let _ = await_ok(pending);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if COUNT.load(Ordering::SeqCst) >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(pending);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node3.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_scheduler_failover_runs_after_leader_stops() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let scheduler1 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);
    let scheduler2 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: scheduler1,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: scheduler2,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    with_node(&node1, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;
    with_node(&node2, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;

    with_node(&node1, async {
        crate::schedule::stop_scheduler_for_test(&node1.scheduler).await;
    })
    .await;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 204;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    let handler = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let pending = with_node(&node1, async {
        wr_schedule_at(Value::from_bool(true), Value::from_int(now), handler)
    })
    .await;
    let _ = await_ok(pending);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if COUNT.load(Ordering::SeqCst) >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);

    unsafe {
        wr_rc_dec(handler);
        wr_rc_dec(pending);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_jobs_run_once_across_nodes() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_job(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_JOB: u32 = 202;
    wr_register_method(CLASS_JOB, 0, handle_job);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_JOB, 0);

    let handler1 = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let handler2 = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let queue = wr_str_from_utf8(b"ha_jobs".as_ptr(), 7);

    let process1 = with_node(&node1, async {
        wr_jobs_process(Value::from_bool(true), queue, handler1)
    })
    .await;
    let process2 = with_node(&node2, async {
        wr_jobs_process(Value::from_bool(true), queue, handler2)
    })
    .await;
    let _ = await_ok(process1);
    let _ = await_ok(process2);

    let payload = wr_map_new();
    let opts = wr_map_new();
    let enqueue = with_node(&node1, async {
        wr_jobs_enqueue(Value::from_bool(true), queue, payload, opts)
    })
    .await;
    let job_id = await_ok(enqueue);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if COUNT.load(Ordering::SeqCst) >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);

    unsafe {
        wr_rc_dec(handler1);
        wr_rc_dec(handler2);
        wr_rc_dec(queue);
        wr_rc_dec(process1);
        wr_rc_dec(process2);
        wr_rc_dec(payload);
        wr_rc_dec(opts);
        wr_rc_dec(enqueue);
        if job_id.is_ptr() {
            wr_rc_dec(job_id);
        }
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_jobs_recover_after_lease_expiry() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_job(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_JOB: u32 = 203;
    wr_register_method(CLASS_JOB, 0, handle_job);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_JOB, 0);

    let handler2 = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let queue = wr_str_from_utf8(b"ha_jobs_expire".as_ptr(), 14);

    let process2 = with_node(&node2, async {
        wr_jobs_process(Value::from_bool(true), queue, handler2)
    })
    .await;
    let _ = await_ok(process2);

    let payload = wr_map_new();
    let opts = wr_map_new();
    let enqueue = with_node(&node1, async {
        wr_jobs_enqueue(Value::from_bool(true), queue, payload, opts)
    })
    .await;
    let job_id_val = await_ok(enqueue);
    let job_id = value_to_string_test(job_id_val);

    let lease_key = format!("jobs:lease:ha_jobs_expire:{job_id}");
    let owner = with_node(&node1, async { crate::lease::owner_id() }).await;
    let _ = with_node(&node1, async {
        crate::lease::try_acquire_lease(&lease_key, &owner, 1).await
    })
    .await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if COUNT.load(Ordering::SeqCst) >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);

    unsafe {
        wr_rc_dec(handler2);
        wr_rc_dec(queue);
        wr_rc_dec(process2);
        wr_rc_dec(payload);
        wr_rc_dec(opts);
        wr_rc_dec(enqueue);
        if job_id_val.is_ptr() {
            wr_rc_dec(job_id_val);
        }
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_jobs_burst_no_duplicates() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_job(_argc: usize, _argv: *const Value) -> Value {
        COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_JOB: u32 = 205;
    wr_register_method(CLASS_JOB, 0, handle_job);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_JOB, 0);

    let handler1 = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let handler2 = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let queue = wr_str_from_utf8(b"ha_jobs_burst".as_ptr(), 13);

    let _ = await_ok(with_node(&node1, async {
        wr_jobs_process(Value::from_bool(true), queue, handler1)
    })
    .await);
    let _ = await_ok(with_node(&node2, async {
        wr_jobs_process(Value::from_bool(true), queue, handler2)
    })
    .await);

    let total = 25;
    for _ in 0..total {
        let payload = wr_map_new();
        let opts = wr_map_new();
        let enqueue = with_node(&node1, async {
            wr_jobs_enqueue(Value::from_bool(true), queue, payload, opts)
        })
        .await;
        let job_id = await_ok(enqueue);
        unsafe {
            wr_rc_dec(payload);
            wr_rc_dec(opts);
            wr_rc_dec(enqueue);
            if job_id.is_ptr() {
                wr_rc_dec(job_id);
            }
        }
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if COUNT.load(Ordering::SeqCst) >= total {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), total);

    unsafe {
        wr_rc_dec(handler1);
        wr_rc_dec(handler2);
        wr_rc_dec(queue);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_chaos_scheduler_jobs_loop() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let scheduler1 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);
    let scheduler2 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: scheduler1,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: scheduler2,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    with_node(&node1, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;
    with_node(&node2, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static SCHED_COUNT: AtomicUsize = AtomicUsize::new(0);
    static JOB_COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        SCHED_COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    extern "C" fn handle_job(_argc: usize, _argv: *const Value) -> Value {
        JOB_COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 206;
    const CLASS_JOB: u32 = 207;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    wr_register_method(CLASS_JOB, 0, handle_job);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_JOB, 0);

    let handler_tick = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);
    let handler_job = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let queue = wr_str_from_utf8(b"ha_chaos_jobs".as_ptr(), 12);

    let _ = await_ok(with_node(&node1, async {
        wr_jobs_process(Value::from_bool(true), queue, handler_job)
    })
    .await);
    let _ = await_ok(with_node(&node2, async {
        wr_jobs_process(Value::from_bool(true), queue, handler_job)
    })
    .await);

    for _ in 0..3 {
        with_node(&node1, async {
            crate::schedule::stop_scheduler_for_test(&node1.scheduler).await;
        })
        .await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let pending = with_node(&node1, async {
            wr_schedule_at(Value::from_bool(true), Value::from_int(now), handler_tick)
        })
        .await;
        let _ = await_ok(pending);

        let payload = wr_map_new();
        let opts = wr_map_new();
        let enqueue = with_node(&node1, async {
            wr_jobs_enqueue(Value::from_bool(true), queue, payload, opts)
        })
        .await;
        let job_id = await_ok(enqueue);
        unsafe {
            wr_rc_dec(payload);
            wr_rc_dec(opts);
            wr_rc_dec(enqueue);
            if job_id.is_ptr() {
                wr_rc_dec(job_id);
            }
            wr_rc_dec(pending);
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if SCHED_COUNT.load(Ordering::SeqCst) >= 1 && JOB_COUNT.load(Ordering::SeqCst) >= 3 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(SCHED_COUNT.load(Ordering::SeqCst) >= 1);
    assert!(JOB_COUNT.load(Ordering::SeqCst) >= 3);

    unsafe {
        wr_rc_dec(handler_tick);
        wr_rc_dec(handler_job);
        wr_rc_dec(queue);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn soak_ha_jobs_scheduler_failover() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let soak_secs: u64 = std::env::var("WRELA_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .min(600);

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let scheduler1 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);
    let scheduler2 = crate::schedule::new_scheduler_state_for_test_with_ttl(1);

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: scheduler1,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: scheduler2,
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    with_node(&node1, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;
    with_node(&node2, async {
        crate::schedule::ensure_scheduler_started();
    })
    .await;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static SCHED_COUNT: AtomicUsize = AtomicUsize::new(0);
    static JOB_COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn handle_tick(_argc: usize, _argv: *const Value) -> Value {
        SCHED_COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    extern "C" fn handle_job(_argc: usize, _argv: *const Value) -> Value {
        JOB_COUNT.fetch_add(1, Ordering::SeqCst);
        Value::from_bool(true)
    }
    const CLASS_TICK: u32 = 208;
    const CLASS_JOB: u32 = 209;
    wr_register_method(CLASS_TICK, 0, handle_tick);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_TICK, 0);
    wr_register_method(CLASS_JOB, 0, handle_job);
    wr_register_method_name(b"handle".as_ptr(), 6, CLASS_JOB, 0);

    let handler_tick = wr_actor_spawn(CLASS_TICK as u64, Value::nil(), 1, 3, -1, -1, -1);
    let handler_job = wr_actor_spawn(CLASS_JOB as u64, Value::nil(), 1, 3, -1, -1, -1);
    let queue = wr_str_from_utf8(b"ha_soak_jobs".as_ptr(), 12);

    let _ = await_ok(with_node(&node1, async {
        wr_jobs_process(Value::from_bool(true), queue, handler_job)
    })
    .await);
    let _ = await_ok(with_node(&node2, async {
        wr_jobs_process(Value::from_bool(true), queue, handler_job)
    })
    .await);

    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(soak_secs);
    let mut iter = 0usize;
    while std::time::Instant::now() < deadline {
        iter += 1;
        if iter % 2 == 0 {
            with_node(&node1, async {
                crate::schedule::stop_scheduler_for_test(&node1.scheduler).await;
            })
            .await;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let pending = with_node(&node1, async {
            wr_schedule_at(Value::from_bool(true), Value::from_int(now), handler_tick)
        })
        .await;
        let _ = await_ok(pending);

        let payload = wr_map_new();
        let opts = wr_map_new();
        let enqueue = with_node(&node1, async {
            wr_jobs_enqueue(Value::from_bool(true), queue, payload, opts)
        })
        .await;
        let job_id = await_ok(enqueue);
        unsafe {
            wr_rc_dec(payload);
            wr_rc_dec(opts);
            wr_rc_dec(enqueue);
            if job_id.is_ptr() {
                wr_rc_dec(job_id);
            }
            wr_rc_dec(pending);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(SCHED_COUNT.load(Ordering::SeqCst) > 0);
    assert!(JOB_COUNT.load(Ordering::SeqCst) > 0);
    assert!(start.elapsed().as_secs() >= soak_secs.saturating_sub(1));

    unsafe {
        wr_rc_dec(handler_tick);
        wr_rc_dec(handler_job);
        wr_rc_dec(queue);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_realtime_fanout_delivers_across_nodes() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    let socket_id = wr_str_from_utf8(b"socket-ha".as_ptr(), 9);
    let room = wr_str_from_utf8(b"room-ha".as_ptr(), 7);

    let join_pending = with_node(&node2, async { wr_realtime_join(socket_id, room) }).await;
    let _ = await_ok(join_pending);

    let message = wr_str_from_utf8(b"hello-ha".as_ptr(), 8);
    let broadcast_pending =
        with_node(&node1, async { wr_realtime_broadcast(room, message) }).await;
    let _ = await_ok(broadcast_pending);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut inbox: Vec<Value> = Vec::new();
    let _ = inbox.len();
    loop {
        let received =
            with_node(&node2, async { crate::realtime::take_inbox_for_test("socket-ha").await })
                .await;
        if !received.is_empty() || tokio::time::Instant::now() > deadline {
            inbox = received;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(inbox.len(), 1);
    if let Some(val) = inbox.pop() {
        let got = value_to_string_test(val);
        assert_eq!(got, "hello-ha");
        unsafe {
            if val.is_ptr() {
                wr_rc_dec(val);
            }
        }
    }

    unsafe {
        wr_rc_dec(socket_id);
        wr_rc_dec(room);
        wr_rc_dec(join_pending);
        wr_rc_dec(message);
        wr_rc_dec(broadcast_pending);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_realtime_burst_fanout() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    let room = wr_str_from_utf8(b"room-burst".as_ptr(), 10);
    let socket1 = wr_str_from_utf8(b"socket-b1".as_ptr(), 9);
    let socket2 = wr_str_from_utf8(b"socket-b2".as_ptr(), 9);
    let _ = await_ok(with_node(&node2, async { wr_realtime_join(socket1, room) }).await);
    let _ = await_ok(with_node(&node2, async { wr_realtime_join(socket2, room) }).await);

    let total = 5;
    for idx in 0..total {
        let msg = format!("hello-{idx}");
        let msg_val = wr_str_from_utf8(msg.as_ptr(), msg.len());
        let pending = with_node(&node1, async { wr_realtime_broadcast(room, msg_val) }).await;
        let _ = await_ok(pending);
        unsafe {
            wr_rc_dec(msg_val);
            wr_rc_dec(pending);
        }
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut inbox1 = Vec::new();
    let mut inbox2 = Vec::new();
    let _ = (inbox1.len(), inbox2.len());
    loop {
        inbox1 =
            with_node(&node2, async { crate::realtime::take_inbox_for_test("socket-b1").await })
                .await;
        inbox2 =
            with_node(&node2, async { crate::realtime::take_inbox_for_test("socket-b2").await })
                .await;
        if inbox1.len() == total && inbox2.len() == total {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(inbox1.len(), total);
    assert_eq!(inbox2.len(), total);
    for val in inbox1.into_iter().chain(inbox2.into_iter()) {
        unsafe {
            if val.is_ptr() {
                wr_rc_dec(val);
            }
        }
    }

    unsafe {
        wr_rc_dec(room);
        wr_rc_dec(socket1);
        wr_rc_dec(socket2);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ha_pubsub_fanout_delivers_across_nodes() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    with_node(&node2, async {
        crate::pubsub::subscribe("ha:pubsub", |_| async {
            COUNT.fetch_add(1, Ordering::SeqCst);
        })
        .await;
    })
    .await;

    with_node(&node1, async {
        crate::pubsub::publish("ha:pubsub", serde_json::Value::String("ok".to_string())).await;
    })
    .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if COUNT.load(Ordering::SeqCst) > 0 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(COUNT.load(Ordering::SeqCst) > 0);

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_peer_token_enforced() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener, addr)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut cfg = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        HashMap::new(),
        true,
    );
    cfg.peer_token = Some("peer-token".to_string());

    let realtime = crate::realtime::new_realtime_state_for_test();
    let service = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg,
            listener,
            realtime.as_arc(),
        )
        .await
        .expect("start service"),
    );

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/storage/read");
    #[derive(serde::Serialize)]
    struct ReadReq {
        key: Vec<u8>,
    }

    let resp = client
        .post(&url)
        .json(&ReadReq { key: b"nope".to_vec() })
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    let resp = client
        .post(&url)
        .header("x-wrela-peer-token", "peer-token")
        .json(&ReadReq { key: b"nope".to_vec() })
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let env: serde_json::Value = resp.json().await.expect("json");
    assert!(env.get("ok").and_then(|v| v.as_bool()).unwrap_or(false));

    if let Ok(service) = Arc::try_unwrap(service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn soak_pubsub_with_peer_token() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let soak_secs: u64 = std::env::var("WRELA_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30)
        .min(300);

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let mut cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let mut cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );
    cfg1.peer_token = Some("soak-token".to_string());
    cfg2.peer_token = Some("soak-token".to_string());

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    with_node(&node2, async {
        crate::pubsub::subscribe("ha:pubsub:soak", |_| async {
            COUNT.fetch_add(1, Ordering::SeqCst);
        })
        .await;
    })
    .await;

    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(soak_secs);
    while std::time::Instant::now() < deadline {
        with_node(&node1, async {
            crate::pubsub::publish(
                "ha:pubsub:soak",
                serde_json::Value::String("ping".to_string()),
            )
            .await;
        })
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(COUNT.load(Ordering::SeqCst) > 0);
    assert!(start.elapsed().as_secs() >= soak_secs.saturating_sub(1));

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn soak_realtime_fanout_burst() {
    if !net_available() {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    }

    let soak_secs: u64 = std::env::var("WRELA_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .min(600);

    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all = HashMap::new();
    all.insert(1, addr1);
    all.insert(2, addr2);

    let cfg1 = config_for_node(
        format!("{base}/db1"),
        1,
        "127.0.0.1:0".to_string(),
        peers_for(1, &all),
        true,
    );
    let cfg2 = config_for_node(
        format!("{base}/db2"),
        2,
        "127.0.0.1:0".to_string(),
        peers_for(2, &all),
        false,
    );

    let realtime1 = crate::realtime::new_realtime_state_for_test();
    let realtime2 = crate::realtime::new_realtime_state_for_test();

    let service1 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg1.clone(),
            listener1,
            realtime1.as_arc(),
        )
        .await
        .expect("start service1"),
    );
    let service2 = Arc::new(
        crate::storage::service::StorageService::start_for_test_with_listener_and_state(
            cfg2.clone(),
            listener2,
            realtime2.as_arc(),
        )
        .await
        .expect("start service2"),
    );

    let node1 = HaNode {
        config: cfg1,
        service: service1,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime1,
    };
    let node2 = HaNode {
        config: cfg2,
        service: service2,
        scheduler: crate::schedule::new_scheduler_state_for_test(),
        jobs: crate::jobs::new_jobs_state_for_test(),
        realtime: realtime2,
    };

    let room = wr_str_from_utf8(b"room-soak".as_ptr(), 9);
    let socket1 = wr_str_from_utf8(b"socket-s1".as_ptr(), 9);
    let socket2 = wr_str_from_utf8(b"socket-s2".as_ptr(), 9);
    let _ = await_ok(with_node(&node2, async { wr_realtime_join(socket1, room) }).await);
    let _ = await_ok(with_node(&node2, async { wr_realtime_join(socket2, room) }).await);

    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(soak_secs);
    let mut idx = 0u64;
    while std::time::Instant::now() < deadline {
        let msg = format!("soak-{idx}");
        idx = idx.wrapping_add(1);
        let msg_val = wr_str_from_utf8(msg.as_ptr(), msg.len());
        let pending = with_node(&node1, async { wr_realtime_broadcast(room, msg_val) }).await;
        let _ = await_ok(pending);
        unsafe {
            wr_rc_dec(msg_val);
            wr_rc_dec(pending);
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    unsafe {
        wr_rc_dec(room);
        wr_rc_dec(socket1);
        wr_rc_dec(socket2);
    }

    if let Ok(service) = Arc::try_unwrap(node1.service) {
        service.shutdown().await;
    }
    assert!(start.elapsed().as_secs() >= soak_secs.saturating_sub(1));
    if let Ok(service) = Arc::try_unwrap(node2.service) {
        service.shutdown().await;
    }
}

#[test]
fn realtime_membership_persists() {
    let _storage = test_storage();
    let socket_id = wr_str_from_utf8(b"socket-1".as_ptr(), 8);
    let room = wr_str_from_utf8(b"room-1".as_ptr(), 6);
    let pending = wr_realtime_join(socket_id, room);
    let ok = await_ok(pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    let mut members = None;
    for _ in 0..40 {
        members = storage_get_json_test("realtime:room:room-1");
        if members.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let members = members.expect("room members");
    assert!(members
        .as_array()
        .map(|arr| arr.iter().any(|val| val.as_str() == Some("socket-1")))
        .unwrap_or(false));

    let mut socket_record = None;
    for _ in 0..40 {
        socket_record = storage_get_json_test("realtime:socket:socket-1");
        if socket_record.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let socket_record = socket_record.expect("socket record");
    assert!(socket_record
        .get("node_id")
        .and_then(|v| v.as_u64())
        .is_some());

    unsafe {
        wr_rc_dec(socket_id);
        wr_rc_dec(room);
        wr_rc_dec(pending);
        wr_rc_dec(ok);
    }
}

#[test]
fn realtime_membership_removes_on_leave() {
    let _storage = test_storage();
    let socket_id = wr_str_from_utf8(b"socket-2".as_ptr(), 8);
    let room = wr_str_from_utf8(b"room-2".as_ptr(), 6);
    let pending = wr_realtime_join(socket_id, room);
    let ok = await_ok(pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    let pending_leave = wr_realtime_leave(socket_id, room);
    let ok_leave = await_ok(pending_leave);
    assert!(ok_leave.is_bool());
    assert!(ok_leave.as_bool());

    let mut members = None;
    for _ in 0..40 {
        members = storage_get_json_test("realtime:room:room-2");
        if members.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    if let Some(members) = members {
        if let Some(arr) = members.as_array() {
            assert!(!arr.iter().any(|val| val.as_str() == Some("socket-2")));
        }
    }

    unsafe {
        wr_rc_dec(socket_id);
        wr_rc_dec(room);
        wr_rc_dec(pending);
        wr_rc_dec(ok);
        wr_rc_dec(pending_leave);
        wr_rc_dec(ok_leave);
    }
}

#[test]
fn realtime_stale_socket_cleanup() {
    let old = std::env::var("WRELA_REALTIME_SOCKET_TTL_SECS").ok();
    unsafe {
        std::env::set_var("WRELA_REALTIME_SOCKET_TTL_SECS", "1");
    }
    let _storage = test_storage();
    let socket_id = wr_str_from_utf8(b"socket-ttl".as_ptr(), 10);
    let room = wr_str_from_utf8(b"room-ttl".as_ptr(), 8);
    let pending = wr_realtime_join(socket_id, room);
    let ok = await_ok(pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    std::thread::sleep(std::time::Duration::from_secs(2));

    let mut members = None;
    for _ in 0..40 {
        members = storage_get_json_test("realtime:room:room-ttl");
        if members
            .as_ref()
            .and_then(|val| val.as_array())
            .map(|arr| arr.is_empty())
            .unwrap_or(true)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if let Some(members) = members {
        assert!(
            members
                .as_array()
                .map(|arr| !arr.iter().any(|val| val.as_str() == Some("socket-ttl")))
                .unwrap_or(true)
        );
    }

    let socket_record = storage_get_json_test("realtime:socket:socket-ttl");
    assert!(socket_record.is_none());

    unsafe {
        wr_rc_dec(socket_id);
        wr_rc_dec(room);
        wr_rc_dec(pending);
        wr_rc_dec(ok);
    }

    unsafe {
        if let Some(old) = old {
            std::env::set_var("WRELA_REALTIME_SOCKET_TTL_SECS", old);
        } else {
            std::env::remove_var("WRELA_REALTIME_SOCKET_TTL_SECS");
        }
    }
}

#[test]
fn lease_is_exclusive_and_expires() {
    let _storage = test_storage();
    let ok_first = crate::actor::runtime_block_on(crate::lease::try_acquire_lease(
        "lease:test",
        "owner-a",
        1,
    ));
    let ok_second = crate::actor::runtime_block_on(crate::lease::try_acquire_lease(
        "lease:test",
        "owner-b",
        1,
    ));
    assert!(ok_first);
    assert!(!ok_second);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let ok_third = crate::actor::runtime_block_on(crate::lease::try_acquire_lease(
        "lease:test",
        "owner-b",
        1,
    ));
    assert!(ok_third);
}

#[test]
fn search_index_query() {
    let storage = test_storage();
    let collection = wr_str_from_utf8(b"projects".as_ptr(), 8);
    let id = wr_str_from_utf8(b"p1".as_ptr(), 2);
    let text = wr_str_from_utf8(b"wrela runtime search".as_ptr(), 20);
    let fields = wr_map_new();
    let key_tag = wr_str_from_utf8(b"tag".as_ptr(), 3);
    let val_tag = wr_str_from_utf8(b"runtime".as_ptr(), 7);
    wr_map_set(fields, key_tag, val_tag);

    let index_pending = wr_search_index(storage, collection, id, text, fields);
    let indexed = await_ok(index_pending);
    assert!(indexed.is_bool());
    assert!(indexed.as_bool());

    let query = wr_str_from_utf8(b"runtime".as_ptr(), 7);
    let opts = wr_map_new();
    let query_pending = wr_search_query(storage, collection, query, opts);
    let results = await_ok(query_pending);
    let len = wr_list_len(results);
    assert_eq!(len.as_int(), 1);

    let remove_pending = wr_search_remove(storage, collection, id);
    let removed = await_ok(remove_pending);
    assert!(removed.is_bool());
    assert!(removed.as_bool());

    unsafe {
        wr_rc_dec(collection);
        wr_rc_dec(id);
        wr_rc_dec(text);
        wr_rc_dec(fields);
        wr_rc_dec(key_tag);
        wr_rc_dec(val_tag);
        wr_rc_dec(index_pending);
        wr_rc_dec(indexed);
        wr_rc_dec(query);
        wr_rc_dec(opts);
        wr_rc_dec(query_pending);
        wr_rc_dec(results);
        wr_rc_dec(len);
        wr_rc_dec(remove_pending);
        wr_rc_dec(removed);
    }
}

#[test]
fn search_persists_documents() {
    let storage = test_storage();
    let collection_name = format!("c_{}", uuid::Uuid::new_v4());
    let id_name = format!("id_{}", uuid::Uuid::new_v4());
    let collection = wr_str_from_utf8(collection_name.as_ptr(), collection_name.len());
    let id = wr_str_from_utf8(id_name.as_ptr(), id_name.len());
    let text = wr_str_from_utf8(b"persistent search".as_ptr(), 17);
    let fields = wr_map_new();
    let pending = wr_search_index(storage, collection, id, text, fields);
    let _ = await_ok(pending);

    let doc_key = format!("search:doc:{collection_name}:{id_name}");
    assert!(storage_get_string_test(&doc_key).is_some());
    let list_key = format!("search:collection:{collection_name}");
    let list_json = storage_get_string_test(&list_key).expect("collection list");
    let parsed: serde_json::Value = serde_json::from_str(&list_json).expect("json");
    assert!(
        parsed
            .as_array()
            .map(|arr| arr.iter().any(|v| v.as_str() == Some(&id_name)))
            .unwrap_or(false)
    );

    unsafe {
        wr_rc_dec(collection);
        wr_rc_dec(id);
        wr_rc_dec(text);
        wr_rc_dec(fields);
        wr_rc_dec(pending);
    }
}

#[test]
fn search_filters_and_pagination() {
    let storage = test_storage();
    let collection = wr_str_from_utf8(b"filter".as_ptr(), 6);
    let fields1 = wr_map_new();
    let k = wr_str_from_utf8(b"tag".as_ptr(), 3);
    let v1 = wr_str_from_utf8(b"a".as_ptr(), 1);
    let v2 = wr_str_from_utf8(b"b".as_ptr(), 1);
    wr_map_set(fields1, k, v1);
    let fields2 = wr_map_new();
    wr_map_set(fields2, k, v2);

    let id1 = wr_str_from_utf8(b"id1".as_ptr(), 3);
    let id2 = wr_str_from_utf8(b"id2".as_ptr(), 3);
    let text = wr_str_from_utf8(b"hello world".as_ptr(), 11);
    let _ = await_ok(wr_search_index(storage, collection, id1, text, fields1));
    let _ = await_ok(wr_search_index(storage, collection, id2, text, fields2));

    let opts = wr_map_new();
    let filters = wr_map_new();
    wr_map_set(filters, k, v1);
    let key_filters = wr_str_from_utf8(b"filters".as_ptr(), 7);
    wr_map_set(opts, key_filters, filters);
    let results = await_ok(wr_search_query(
        storage,
        collection,
        wr_str_from_utf8(b"hello".as_ptr(), 5),
        opts,
    ));
    let len = wr_list_len(results);
    assert_eq!(len.as_int(), 1);

    unsafe {
        wr_rc_dec(collection);
        wr_rc_dec(fields1);
        wr_rc_dec(fields2);
        wr_rc_dec(k);
        wr_rc_dec(v1);
        wr_rc_dec(v2);
        wr_rc_dec(id1);
        wr_rc_dec(id2);
        wr_rc_dec(text);
        wr_rc_dec(opts);
        wr_rc_dec(filters);
        wr_rc_dec(key_filters);
        wr_rc_dec(results);
        wr_rc_dec(len);
    }
}

#[test]
fn search_limit_offset() {
    let storage = test_storage();
    let collection = wr_str_from_utf8(b"limit".as_ptr(), 5);
    let text = wr_str_from_utf8(b"alpha beta".as_ptr(), 10);
    let fields = wr_map_new();
    let id1 = wr_str_from_utf8(b"a1".as_ptr(), 2);
    let id2 = wr_str_from_utf8(b"a2".as_ptr(), 2);
    let id3 = wr_str_from_utf8(b"a3".as_ptr(), 2);
    let _ = await_ok(wr_search_index(storage, collection, id1, text, fields));
    let _ = await_ok(wr_search_index(storage, collection, id2, text, fields));
    let _ = await_ok(wr_search_index(storage, collection, id3, text, fields));

    let opts = wr_map_new();
    let key_limit = wr_str_from_utf8(b"limit".as_ptr(), 5);
    let key_offset = wr_str_from_utf8(b"offset".as_ptr(), 6);
    wr_map_set(opts, key_limit, Value::from_int(1));
    wr_map_set(opts, key_offset, Value::from_int(1));
    let results = await_ok(wr_search_query(
        storage,
        collection,
        wr_str_from_utf8(b"alpha".as_ptr(), 5),
        opts,
    ));
    let len = wr_list_len(results);
    assert_eq!(len.as_int(), 1);

    unsafe {
        wr_rc_dec(collection);
        wr_rc_dec(text);
        wr_rc_dec(fields);
        wr_rc_dec(id1);
        wr_rc_dec(id2);
        wr_rc_dec(id3);
        wr_rc_dec(opts);
        wr_rc_dec(key_limit);
        wr_rc_dec(key_offset);
        wr_rc_dec(results);
        wr_rc_dec(len);
    }
}

#[test]
fn search_query_case_insensitive() {
    let storage = test_storage();
    let collection = wr_str_from_utf8(b"case".as_ptr(), 4);
    let id = wr_str_from_utf8(b"c1".as_ptr(), 2);
    let text = wr_str_from_utf8(b"Hello World".as_ptr(), 11);
    let fields = wr_map_new();
    let _ = await_ok(wr_search_index(storage, collection, id, text, fields));
    let results = await_ok(wr_search_query(
        storage,
        collection,
        wr_str_from_utf8(b"hello".as_ptr(), 5),
        wr_map_new(),
    ));
    let len = wr_list_len(results);
    assert_eq!(len.as_int(), 1);

    unsafe {
        wr_rc_dec(collection);
        wr_rc_dec(id);
        wr_rc_dec(text);
        wr_rc_dec(fields);
        wr_rc_dec(results);
        wr_rc_dec(len);
    }
}

#[test]
fn search_index_survives_restart() {
    crate::actor::runtime_block_on(async {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("wrela.db");
        let path_str = path.to_string_lossy().to_string();
        let blob_path = format!("{path_str}.blobs");

        let config = crate::storage::config::StorageConfig {
            enabled: true,
            path: path_str.clone(),
            node_id: 1,
            bind_addr: "127.0.0.1:0".to_string(),
            http_enabled: false,
            peer_token: None,
            peers: HashMap::new(),
            bootstrap: true,
            snapshot_interval: 50,
            batch_max_ops: 2,
            batch_max_ms: 1,
            queue_cap: 32,
            blob: crate::storage::config::BlobConfig {
                threshold_bytes: 256 * 1024,
                file_path: blob_path.clone(),
                s3: None,
            },
            backup: crate::storage::config::BackupConfig {
                enabled: false,
                max_age_secs: 3600,
                max_logs: 100_000,
                retention_days: 7,
                max_keep: 0,
                prefix: "backups".to_string(),
                only_leader: true,
                restore_mode: crate::storage::config::RestoreMode::Single,
                restore_id: None,
            },
        };

        let service = crate::storage::service::StorageService::start_for_test(config)
            .await
            .expect("start storage");
        let service = Arc::new(service);

        crate::storage::service::StorageService::with_storage_override(
            Arc::clone(&service),
            async {
                let storage = Value::from_int(1);
                let collection = wr_str_from_utf8(b"persist".as_ptr(), 7);
                let id = wr_str_from_utf8(b"doc1".as_ptr(), 4);
                let text = wr_str_from_utf8(b"search survives restart".as_ptr(), 23);
                let fields = wr_map_new();

                let pending = wr_search_index(storage, collection, id, text, fields);
                let ok = await_ok(pending);
                assert!(ok.is_bool());
                assert!(ok.as_bool());

                let query = wr_str_from_utf8(b"restart".as_ptr(), 7);
                let opts = wr_map_new();
                let results = await_ok(wr_search_query(storage, collection, query, opts));
                let len = wr_list_len(results);
                assert_eq!(len.as_int(), 1);

                unsafe {
                    wr_rc_dec(collection);
                    wr_rc_dec(id);
                    wr_rc_dec(text);
                    wr_rc_dec(fields);
                    wr_rc_dec(pending);
                    wr_rc_dec(ok);
                    wr_rc_dec(query);
                    wr_rc_dec(opts);
                    wr_rc_dec(results);
                    wr_rc_dec(len);
                }
            },
        )
        .await;

        let service = match Arc::try_unwrap(service) {
            Ok(service) => service,
            Err(_) => panic!("storage service refs"),
        };
        service.shutdown().await;

        let config = crate::storage::config::StorageConfig {
            enabled: true,
            path: path_str,
            node_id: 1,
            bind_addr: "127.0.0.1:0".to_string(),
            http_enabled: false,
            peer_token: None,
            peers: HashMap::new(),
            bootstrap: false,
            snapshot_interval: 50,
            batch_max_ops: 2,
            batch_max_ms: 1,
            queue_cap: 32,
            blob: crate::storage::config::BlobConfig {
                threshold_bytes: 256 * 1024,
                file_path: blob_path,
                s3: None,
            },
            backup: crate::storage::config::BackupConfig {
                enabled: false,
                max_age_secs: 3600,
                max_logs: 100_000,
                retention_days: 7,
                max_keep: 0,
                prefix: "backups".to_string(),
                only_leader: true,
                restore_mode: crate::storage::config::RestoreMode::Single,
                restore_id: None,
            },
        };

        let service = crate::storage::service::StorageService::start_for_test(config)
            .await
            .expect("restart storage");
        let service = Arc::new(service);

        crate::storage::service::StorageService::with_storage_override(
            Arc::clone(&service),
            async {
                let storage = Value::from_int(1);
                let collection = wr_str_from_utf8(b"persist".as_ptr(), 7);
                let query = wr_str_from_utf8(b"restart".as_ptr(), 7);
                let opts = wr_map_new();
                let results = await_ok(wr_search_query(storage, collection, query, opts));
                let len = wr_list_len(results);
                assert_eq!(len.as_int(), 1);

                unsafe {
                    wr_rc_dec(collection);
                    wr_rc_dec(query);
                    wr_rc_dec(opts);
                    wr_rc_dec(results);
                    wr_rc_dec(len);
                }
            },
        )
        .await;

        let service = match Arc::try_unwrap(service) {
            Ok(service) => service,
            Err(_) => panic!("storage service refs"),
        };
        service.shutdown().await;
    });
}

#[test]
fn realtime_join_leave_broadcast() {
    let rt = wr_realtime_on_connect(Value::nil());
    let ok = await_ok(rt);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    let socket_id = wr_str_from_utf8(b"s1".as_ptr(), 2);
    let room = wr_str_from_utf8(b"room".as_ptr(), 4);
    let join_pending = wr_realtime_join(socket_id, room);
    let joined = await_ok(join_pending);
    assert!(joined.is_bool());
    assert!(joined.as_bool());

    let msg = wr_str_from_utf8(b"hello".as_ptr(), 5);
    let broadcast_pending = wr_realtime_broadcast(room, msg);
    let broadcast_ok = await_ok(broadcast_pending);
    assert!(broadcast_ok.is_bool());
    assert!(broadcast_ok.as_bool());

    let send_pending = wr_realtime_send(socket_id, msg);
    let send_ok = await_ok(send_pending);
    assert!(send_ok.is_bool());
    assert!(send_ok.as_bool());

    let leave_pending = wr_realtime_leave(socket_id, room);
    let left = await_ok(leave_pending);
    assert!(left.is_bool());
    assert!(left.as_bool());

    unsafe {
        wr_rc_dec(rt);
        wr_rc_dec(ok);
        wr_rc_dec(socket_id);
        wr_rc_dec(room);
        wr_rc_dec(join_pending);
        wr_rc_dec(joined);
        wr_rc_dec(msg);
        wr_rc_dec(broadcast_pending);
        wr_rc_dec(broadcast_ok);
        wr_rc_dec(send_pending);
        wr_rc_dec(send_ok);
        wr_rc_dec(leave_pending);
        wr_rc_dec(left);
    }
}

#[test]
fn rate_limit_check_and_ip() {
    let storage = test_storage();
    let key = wr_str_from_utf8(b"client-1".as_ptr(), 8);
    let opts = wr_map_new();
    let key_burst = wr_str_from_utf8(b"burst".as_ptr(), 5);
    let key_per = wr_str_from_utf8(b"per_secs".as_ptr(), 8);
    wr_map_set(opts, key_burst, Value::from_int(1));
    wr_map_set(opts, key_per, Value::from_int(60));

    let pending1 = wr_rate_check(storage, key, opts);
    let ok1 = await_ok(pending1);
    assert!(ok1.is_bool());
    assert!(ok1.as_bool());

    let pending2 = wr_rate_check(storage, key, opts);
    let ok2 = await_ok(pending2);
    assert!(ok2.is_bool());
    assert!(!ok2.as_bool());

    let headers = wr_map_new();
    let hkey = wr_str_from_utf8(b"x-forwarded-for".as_ptr(), 15);
    let hval = wr_str_from_utf8(b"10.0.0.1".as_ptr(), 8);
    wr_map_set(headers, hkey, hval);

    let fields = [b"headers".as_ptr()];
    let lens = [7usize];
    let req = wr_class_new(200, fields.as_ptr(), lens.as_ptr(), 1);
    wr_class_set(req, b"headers".as_ptr(), 7, headers);

    let ip = wr_rate_ip(req);
    let ip_str =
        string::with_string_bytes(ip, |bytes| String::from_utf8_lossy(bytes).into_owned()).unwrap();
    assert_eq!(ip_str, "10.0.0.1");

    unsafe {
        wr_rc_dec(key);
        wr_rc_dec(opts);
        wr_rc_dec(key_burst);
        wr_rc_dec(key_per);
        wr_rc_dec(pending1);
        wr_rc_dec(ok1);
        wr_rc_dec(pending2);
        wr_rc_dec(ok2);
        wr_rc_dec(headers);
        wr_rc_dec(hkey);
        wr_rc_dec(hval);
        wr_rc_dec(req);
        wr_rc_dec(ip);
    }
}

#[test]
fn rate_limit_persists_bucket() {
    let storage = test_storage();
    let key_name = format!("rate-{}", uuid::Uuid::new_v4());
    let key = wr_str_from_utf8(key_name.as_ptr(), key_name.len());
    let opts = wr_map_new();
    let burst_key = wr_str_from_utf8(b"burst".as_ptr(), 5);
    wr_map_set(opts, burst_key, Value::from_int(2));
    let pending = wr_rate_check(storage, key, opts);
    let _ = await_ok(pending);

    let bucket_key = format!("rate:{key_name}");
    let bucket_json = storage_get_string_test(&bucket_key).expect("bucket");
    let parsed: serde_json::Value = serde_json::from_str(&bucket_json).expect("json");
    assert!(parsed.get("tokens").is_some());

    unsafe {
        wr_rc_dec(key);
        wr_rc_dec(opts);
        wr_rc_dec(burst_key);
        wr_rc_dec(pending);
    }
}

#[test]
fn rate_limit_refills_over_time() {
    let storage = test_storage();
    let key_name = format!("rate_refill_{}", uuid::Uuid::new_v4());
    let key = wr_str_from_utf8(key_name.as_ptr(), key_name.len());
    let opts = wr_map_new();
    let key_burst = wr_str_from_utf8(b"burst".as_ptr(), 5);
    let key_per = wr_str_from_utf8(b"per_secs".as_ptr(), 8);
    wr_map_set(opts, key_burst, Value::from_int(1));
    wr_map_set(opts, key_per, Value::from_int(2));

    let ok1 = await_ok(wr_rate_check(storage, key, opts));
    assert!(ok1.as_bool());
    let ok2 = await_ok(wr_rate_check(storage, key, opts));
    assert!(!ok2.as_bool());
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let ok3 = await_ok(wr_rate_check(storage, key, opts));
    assert!(ok3.as_bool());

    unsafe {
        wr_rc_dec(key);
        wr_rc_dec(opts);
        wr_rc_dec(key_burst);
        wr_rc_dec(key_per);
        wr_rc_dec(ok1);
        wr_rc_dec(ok2);
        wr_rc_dec(ok3);
    }
}

#[test]
fn rate_limit_key_isolation() {
    let storage = test_storage();
    let key1 = wr_str_from_utf8(b"key1".as_ptr(), 4);
    let key2 = wr_str_from_utf8(b"key2".as_ptr(), 4);
    let opts = wr_map_new();
    let key_burst = wr_str_from_utf8(b"burst".as_ptr(), 5);
    wr_map_set(opts, key_burst, Value::from_int(1));

    let ok1 = await_ok(wr_rate_check(storage, key1, opts));
    let ok2 = await_ok(wr_rate_check(storage, key2, opts));
    assert!(ok1.as_bool());
    assert!(ok2.as_bool());

    unsafe {
        wr_rc_dec(key1);
        wr_rc_dec(key2);
        wr_rc_dec(opts);
        wr_rc_dec(key_burst);
        wr_rc_dec(ok1);
        wr_rc_dec(ok2);
    }
}

#[test]
fn rate_limit_burst_respected() {
    let storage = test_storage();
    let key = wr_str_from_utf8(b"burst-key".as_ptr(), 9);
    let opts = wr_map_new();
    let key_burst = wr_str_from_utf8(b"burst".as_ptr(), 5);
    let key_per = wr_str_from_utf8(b"per_secs".as_ptr(), 8);
    wr_map_set(opts, key_burst, Value::from_int(3));
    wr_map_set(opts, key_per, Value::from_int(60));
    let ok1 = await_ok(wr_rate_check(storage, key, opts));
    let ok2 = await_ok(wr_rate_check(storage, key, opts));
    let ok3 = await_ok(wr_rate_check(storage, key, opts));
    let ok4 = await_ok(wr_rate_check(storage, key, opts));
    assert!(ok1.as_bool());
    assert!(ok2.as_bool());
    assert!(ok3.as_bool());
    assert!(!ok4.as_bool());

    unsafe {
        wr_rc_dec(key);
        wr_rc_dec(opts);
        wr_rc_dec(key_burst);
        wr_rc_dec(key_per);
        wr_rc_dec(ok1);
        wr_rc_dec(ok2);
        wr_rc_dec(ok3);
        wr_rc_dec(ok4);
    }
}

#[test]
fn admin_enable_starts() {
    if !net_available() {
        return;
    }
    let opts = wr_map_new();
    let key = wr_str_from_utf8(b"bind_addr".as_ptr(), 9);
    let val = wr_str_from_utf8(b"127.0.0.1:0".as_ptr(), 13);
    wr_map_set(opts, key, val);

    let pending = wr_admin_enable(opts);
    let ok = await_ok(pending);
    assert!(ok.is_bool());
    assert!(ok.as_bool());

    unsafe {
        wr_rc_dec(opts);
        wr_rc_dec(key);
        wr_rc_dec(val);
        wr_rc_dec(pending);
        wr_rc_dec(ok);
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
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};
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
    let class = wr_class_new(
        TypeId::UserBase as u32,
        name_ptrs.as_ptr(),
        name_lens.as_ptr(),
        1,
    );
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

    let rc_inc = wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_INC as i64)).as_int();
    let rc_dec = wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_DEC as i64)).as_int();
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
    let class = wr_class_new(
        TypeId::UserBase as u32,
        name_ptrs.as_ptr(),
        name_lens.as_ptr(),
        1,
    );

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

    let rc_inc = wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_INC as i64)).as_int();
    let rc_dec = wr_metrics_get(Value::from_int(crate::metrics::METRIC_RC_DEC as i64)).as_int();
    let released = rc_dec.saturating_sub(rc_inc);
    assert!(rc_dec >= rc_inc);
    assert!(released >= 9);
}
