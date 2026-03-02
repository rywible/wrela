pub(crate) mod logging {
    use crate::list;
    use crate::map;
    use crate::string;
    use crate::value::{Value, int_value};
    use std::cell::RefCell;
    use std::io::{self, Write};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    struct LogConfig {
        level: String,
    }

    impl Default for LogConfig {
        fn default() -> Self {
            Self {
                level: "info".to_string(),
            }
        }
    }

    static LOG_CONFIG: OnceLock<Mutex<LogConfig>> = OnceLock::new();
    thread_local! {
        static LOG_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(512));
    }

    fn log_config() -> LogConfig {
        LOG_CONFIG
            .get_or_init(|| Mutex::new(LogConfig::default()))
            .lock()
            .expect("log config lock")
            .clone()
    }

    fn set_log_config(config: LogConfig) {
        *LOG_CONFIG
            .get_or_init(|| Mutex::new(LogConfig::default()))
            .lock()
            .expect("log config lock") = config;
    }

    fn log_level_threshold() -> u8 {
        match log_config().level.to_ascii_lowercase().as_str() {
            "debug" => 10,
            "info" => 20,
            "warn" | "warning" => 30,
            "error" => 40,
            _ => 20,
        }
    }

    fn level_value(level: &str) -> u8 {
        match level {
            "debug" => 10,
            "info" => 20,
            "warn" | "warning" => 30,
            "error" => 40,
            _ => 20,
        }
    }

    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn value_to_string(val: Value) -> Option<String> {
        string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    fn write_json_string(buf: &mut Vec<u8>, text: &str) {
        buf.push(b'"');
        for b in text.bytes() {
            match b {
                b'"' => buf.extend_from_slice(br#"\""#),
                b'\\' => buf.extend_from_slice(br#"\\"#),
                b'\n' => buf.extend_from_slice(br#"\n"#),
                b'\r' => buf.extend_from_slice(br#"\r"#),
                b'\t' => buf.extend_from_slice(br#"\t"#),
                b'\x08' => buf.extend_from_slice(br#"\b"#),
                b'\x0c' => buf.extend_from_slice(br#"\f"#),
                0x00..=0x1f => {
                    let _ = write!(buf, "\\u{:04x}", b);
                }
                _ => buf.push(b),
            }
        }
        buf.push(b'"');
    }

    fn write_json_string_value(buf: &mut Vec<u8>, val: Value) -> bool {
        string::with_string_bytes(val, |bytes| {
            let text = String::from_utf8_lossy(bytes);
            write_json_string(buf, text.as_ref());
        })
        .is_some()
    }

    fn write_json_list(buf: &mut Vec<u8>, list_ref: *mut list::ListObj) {
        buf.push(b'[');
        let mut first = true;
        unsafe {
            for item in (*list_ref).data.iter().take((*list_ref).len) {
                if !first {
                    buf.push(b',');
                }
                first = false;
                write_json_value(buf, *item);
            }
        }
        buf.push(b']');
    }

    fn write_json_map(buf: &mut Vec<u8>, map_ref: *mut map::MapObj) {
        buf.push(b'{');
        let mut first = true;
        let mut iter = map::map_iter(map_ref);
        while let Some((key, value)) = iter.next() {
            if string::with_string_bytes(key.0, |bytes| {
                let text = String::from_utf8_lossy(bytes);
                if !first {
                    buf.push(b',');
                }
                first = false;
                write_json_string(buf, text.as_ref());
                buf.push(b':');
                write_json_value(buf, value);
            })
            .is_none()
            {
                continue;
            }
        }
        buf.push(b'}');
    }

    fn write_json_value(buf: &mut Vec<u8>, val: Value) {
        if val.is_nil() {
            buf.extend_from_slice(b"null");
            return;
        }
        if val.is_bool() {
            if val.as_bool() {
                buf.extend_from_slice(b"true");
            } else {
                buf.extend_from_slice(b"false");
            }
            return;
        }
        if let Some(i) = int_value(val) {
            let _ = write!(buf, "{}", i);
            return;
        }
        if val.is_float() {
            let f = val.as_float();
            if f.is_finite() {
                let _ = write!(buf, "{}", f);
            } else {
                buf.extend_from_slice(b"null");
            }
            return;
        }
        if write_json_string_value(buf, val) {
            return;
        }
        if let Some(list_ref) = list::as_list_ref(val) {
            write_json_list(buf, list_ref);
            return;
        }
        if let Some(map_ref) = map::as_map_ref(val) {
            write_json_map(buf, map_ref);
            return;
        }
        buf.extend_from_slice(b"\"<value>\"");
    }

    fn should_emit_fields(fields: Value) -> bool {
        if fields.is_nil() {
            return false;
        }
        if fields.is_float() && !fields.as_float().is_finite() {
            return false;
        }
        true
    }

    fn write_log_line_with_ts(buf: &mut Vec<u8>, ts: u64, level: &str, msg: &str, fields: Value) {
        buf.extend_from_slice(br#"{"ts":"#);
        let _ = write!(buf, "{}", ts);
        buf.extend_from_slice(br#","level":"#);
        write_json_string(buf, level);
        buf.extend_from_slice(br#","msg":"#);
        write_json_string(buf, msg);
        if should_emit_fields(fields) {
            buf.extend_from_slice(br#","fields":"#);
            write_json_value(buf, fields);
        }
        buf.push(b'}');
    }

    pub fn log(level: Value, msg: Value, fields: Value) -> Value {
        let level = value_to_string(level).unwrap_or_else(|| "info".to_string());
        let level = level.to_ascii_lowercase();
        if level_value(&level) < log_level_threshold() {
            return Value::from_bool(false);
        }
        let message = value_to_string(msg).unwrap_or_else(|| "<value>".to_string());
        let ts = now_millis();
        LOG_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            buffer.clear();
            write_log_line_with_ts(&mut buffer, ts, &level, &message, fields);
            buffer.push(b'\n');
            if level_value(&level) >= 30 {
                let _ = io::stderr().write_all(&buffer);
            } else {
                let _ = io::stdout().write_all(&buffer);
            }
        });
        Value::from_bool(true)
    }

    pub fn log_configure(config: Value) -> Value {
        let new_config = log_config_from_value(config);
        set_log_config(new_config);
        Value::nil()
    }

    fn log_config_from_value(config: Value) -> LogConfig {
        let mut out = LogConfig::default();
        if let Some(level) = config_field_string(config, "level") {
            out.level = level;
        }
        out
    }

    fn config_field_string(config: Value, field: &str) -> Option<String> {
        let val = crate::class::class_get(config, field.as_ptr(), field.len());
        if val.is_nil() {
            unsafe { crate::wr_rc_dec(val) };
            return None;
        }
        let out = value_to_string(val).and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        unsafe { crate::wr_rc_dec(val) };
        out
    }

    #[cfg(test)]
    fn test_log_line(level: &str, msg: &str, fields: Value) -> String {
        let mut buf = Vec::new();
        write_log_line_with_ts(&mut buf, 123, level, msg, fields);
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::string;
        use serde_json::Value as JsonValue;

        #[test]
        fn log_line_encodes_fields() {
            let fields = map::map_new();
            let key = string::str_from_bytes(b"foo");
            map::map_set(fields, key, Value::from_int(7));
            unsafe { crate::wr_rc_dec(key) };

            let line = test_log_line("info", "hello", fields);
            let json: JsonValue = serde_json::from_str(&line).expect("json");
            assert_eq!(json["ts"], 123);
            assert_eq!(json["level"], "info");
            assert_eq!(json["msg"], "hello");
            assert_eq!(json["fields"]["foo"], 7);

            unsafe { crate::wr_rc_dec(fields) };
        }

        #[test]
        fn log_line_omits_nil_fields() {
            let line = test_log_line("info", "hello", Value::nil());
            let json: JsonValue = serde_json::from_str(&line).expect("json");
            assert!(json.get("fields").is_none());
        }

        #[test]
        fn log_line_omits_nan_fields() {
            let line = test_log_line("info", "hello", Value::from_float(f64::NAN));
            let json: JsonValue = serde_json::from_str(&line).expect("json");
            assert!(json.get("fields").is_none());
        }
    }
}

use crate::bytes;
use crate::kernel::config;
use crate::kernel::runtime;
use crate::map;
use crate::result;
use crate::string;
use crate::value::{Value, int_value};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicI8;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn print(val: Value) -> Value {
    if val.is_ptr() {
        unsafe {
            let header = &*val.as_ptr();
            if header.type_id == crate::TypeId::String as u32 {
                let _ = string::with_string_bytes(val, |bytes| {
                    println!("{}", String::from_utf8_lossy(bytes));
                });
                return Value::nil();
            }
        }
    }
    println!("<value>");
    Value::nil()
}

pub(crate) fn log(level: Value, msg: Value, fields: Value) -> Value {
    logging::log(level, msg, fields)
}

pub(crate) fn log_configure(config: Value) -> Value {
    logging::log_configure(config)
}

fn builtin_error(message: &str) -> Value {
    string::str_from_utf8(message.as_ptr(), message.len())
}

fn capability_denied_result(capability: &str, operation: &str) -> Value {
    result::result_err(builtin_error(&format!(
        "capability_denied:{capability}.{operation}"
    )))
}

fn capability_denied_log(capability: &str, operation: &str) {
    eprintln!("runtime error: capability_denied:{capability}.{operation}");
}

fn string_bytes(val: Value) -> Option<Vec<u8>> {
    string::with_string_bytes(val, |bytes| bytes.to_vec())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn resolve_for_policy(raw_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        return Ok(normalize_path(&path));
    }
    let cwd = std::env::current_dir().map_err(|err| format!("failed to read cwd: {err}"))?;
    Ok(normalize_path(&cwd.join(path)))
}

fn canonicalize_for_policy(path: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(path) {
        return normalize_path(&real);
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(real_parent) = std::fs::canonicalize(parent)
    {
        return normalize_path(&real_parent.join(name));
    }
    normalize_path(path)
}

fn enforce_spec_fs_write_scope(raw_path: &str) -> Result<(), String> {
    let Some(scope_raw) = std::env::var_os("WRELA_SPEC_FS_ROOT") else {
        return Ok(());
    };
    let scope = canonicalize_for_policy(Path::new(&scope_raw));
    let resolved = canonicalize_for_policy(&resolve_for_policy(raw_path)?);
    if resolved == scope || resolved.starts_with(&scope) {
        return Ok(());
    }
    Err(format!(
        "spec lane forbids writing outside isolated test directory: attempted '{}', allowed root '{}'",
        resolved.display(),
        scope.display()
    ))
}

pub(crate) fn fs_read_bytes(path: Value) -> Value {
    if !config::capability_fs_enabled() {
        return capability_denied_result("fs", "read");
    }
    let Some(bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("fs_read_bytes expects a String"));
    };
    let path_str = String::from_utf8_lossy(&bytes);
    match std::fs::read(path_str.as_ref()) {
        Ok(contents) => result::result_ok(bytes::bytes_from_slice(&contents)),
        Err(err) => result::result_err(builtin_error(&format!("fs_read_bytes: {err}"))),
    }
}

pub(crate) fn fs_write_bytes(path: Value, contents: Value) -> Value {
    if !config::capability_fs_enabled() {
        return capability_denied_result("fs", "write");
    }
    let Some(path_bytes) = string_bytes(path) else {
        return result::result_err(builtin_error("fs_write_bytes expects a String path"));
    };
    let Some(contents_bytes) = bytes::with_bytes(contents, |bytes| bytes.to_vec()) else {
        return result::result_err(builtin_error("fs_write_bytes expects Bytes contents"));
    };
    let path_str = String::from_utf8_lossy(&path_bytes);
    if let Err(message) = enforce_spec_fs_write_scope(path_str.as_ref()) {
        return result::result_err(builtin_error(&message));
    }
    match std::fs::write(path_str.as_ref(), contents_bytes) {
        Ok(()) => result::result_ok(Value::nil()),
        Err(err) => result::result_err(builtin_error(&format!("fs_write_bytes: {err}"))),
    }
}

const HTTP_CASSETTE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HttpMode {
    Replay,
    Record,
}

#[derive(Debug, Serialize, Deserialize)]
struct HttpCassetteV1 {
    version: u32,
    request: HttpCassetteRequestV1,
    response: HttpCassetteResponseV1,
}

#[derive(Debug, Serialize, Deserialize)]
struct HttpCassetteRequestV1 {
    service: String,
    endpoint: String,
    method: String,
    url: String,
    headers_redacted: BTreeMap<String, String>,
    body_base64: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HttpCassetteResponseV1 {
    status: u16,
    headers: BTreeMap<String, String>,
    body_base64: String,
}

#[derive(Debug)]
struct HttpRuntimeResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug)]
struct HttpCassetteKey {
    service: String,
    endpoint: String,
    method: String,
    url_hash: String,
    body_hash: String,
    headers_hash: String,
}

impl HttpCassetteKey {
    fn file_name(&self) -> String {
        format!(
            "{}__{}__{}__{}__{}__{}.json",
            sanitize_key_component(&self.service),
            sanitize_key_component(&self.endpoint),
            sanitize_key_component(&self.method),
            self.url_hash,
            self.body_hash,
            self.headers_hash
        )
    }
}

fn sanitize_key_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

fn http_mode_from_env() -> HttpMode {
    match std::env::var("WRELA_HTTP_MODE")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("record") => HttpMode::Record,
        _ => HttpMode::Replay,
    }
}

fn workspace_root_from_env() -> Option<PathBuf> {
    let raw = std::env::var("WRELA_WORKSPACE_ROOT").ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    let path = PathBuf::from(raw.trim());
    if path.is_absolute() {
        Some(path)
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    }
}

fn cassette_root() -> Result<PathBuf, String> {
    if let Ok(raw) = std::env::var("WRELA_CASSETTE_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            return Ok(if path.is_absolute() {
                path
            } else if let Some(workspace_root) = workspace_root_from_env() {
                workspace_root.join(path)
            } else {
                std::env::current_dir()
                    .map_err(|err| format!("failed to read current directory: {err}"))?
                    .join(path)
            });
        }
    }
    if let Some(workspace_root) = workspace_root_from_env() {
        return Ok(workspace_root.join("tests").join("cassettes"));
    }
    Ok(std::env::current_dir()
        .map_err(|err| format!("failed to read current directory: {err}"))?
        .join("tests")
        .join("cassettes"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn is_secret_header(key: &str) -> bool {
    matches!(
        key,
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "x-auth-token"
    )
}

fn is_redacted_json_key(key: &str) -> bool {
    matches!(key, "api_key" | "token" | "secret" | "password")
}

fn is_volatile_response_header(key: &str) -> bool {
    matches!(
        key,
        "date" | "server" | "x-request-id" | "x-amzn-requestid" | "cf-ray"
    )
}

fn redacted_response_headers_map(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (key, value) in headers {
        let redacted = if key == "set-cookie" {
            "<redacted>".to_string()
        } else {
            value.clone()
        };
        out.insert(key.clone(), redacted);
    }
    out
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(obj) => {
            for (key, val) in obj.iter_mut() {
                if is_redacted_json_key(&key.to_ascii_lowercase()) {
                    *val = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_json_value(val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

fn redact_json_body_bytes(bytes: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return bytes.to_vec();
    };
    redact_json_value(&mut value);
    match serde_json::to_vec_pretty(&value) {
        Ok(redacted) => redacted,
        Err(_) => bytes.to_vec(),
    }
}

fn lock_path_for(cassette_path: &Path) -> PathBuf {
    let file_name = cassette_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cassette".to_string());
    cassette_path.with_file_name(format!("{file_name}.lock"))
}

fn temp_path_for(cassette_path: &Path) -> PathBuf {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = cassette_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cassette".to_string());
    cassette_path.with_file_name(format!("{file_name}.tmp.{pid}.{counter}"))
}

struct CassetteLockGuard {
    lock_path: PathBuf,
}

impl Drop for CassetteLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

fn acquire_cassette_lock(
    cassette_path: &Path,
    timeout: Duration,
) -> Result<CassetteLockGuard, String> {
    let lock_path = lock_path_for(cassette_path);
    let start = Instant::now();
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                return Ok(CassetteLockGuard { lock_path });
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if start.elapsed() >= timeout {
                    return Err(format!(
                        "timed out waiting for cassette lock '{}'",
                        lock_path.display()
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                return Err(format!(
                    "failed to create cassette lock '{}': {err}",
                    lock_path.display()
                ));
            }
        }
    }
}

fn write_cassette_atomic(cassette_path: &Path, payload: &[u8]) -> Result<(), String> {
    if let Some(parent) = cassette_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create cassette directory '{}': {err}",
                parent.display()
            )
        })?;
    }

    let _guard = acquire_cassette_lock(cassette_path, Duration::from_secs(10))?;
    let temp_path = temp_path_for(cassette_path);
    let mut temp = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|err| {
            format!(
                "failed to create temporary cassette '{}': {err}",
                temp_path.display()
            )
        })?;
    if let Err(err) = temp.write_all(payload) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "failed to write temporary cassette '{}': {err}",
            temp_path.display()
        ));
    }
    if let Err(err) = temp.sync_all() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "failed to sync temporary cassette '{}': {err}",
            temp_path.display()
        ));
    }
    drop(temp);
    if let Err(err) = std::fs::rename(&temp_path, cassette_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "failed to atomically replace cassette '{}' from '{}': {err}",
            cassette_path.display(),
            temp_path.display()
        ));
    }
    Ok(())
}

fn collect_headers(headers: Value) -> Result<Vec<(String, String)>, String> {
    let Some(headers_ref) = map::as_map_ref(headers) else {
        return Err("http_call expects Map headers".to_string());
    };
    let mut pairs = Vec::new();
    let mut iter = map::map_iter(headers_ref);
    while let Some((key, value)) = iter.next() {
        let Some(key_text) = string::with_string_bytes(key.0, |bytes| {
            String::from_utf8_lossy(bytes).trim().to_ascii_lowercase()
        }) else {
            return Err("http_call expects String header keys".to_string());
        };
        let Some(value_text) = string::with_string_bytes(value, |bytes| {
            String::from_utf8_lossy(bytes).trim().to_string()
        }) else {
            return Err("http_call expects String header values".to_string());
        };
        if !key_text.is_empty() {
            pairs.push((key_text, value_text));
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    Ok(pairs)
}

fn stable_headers_materialized(headers: &[(String, String)]) -> String {
    let mut out = String::new();
    for (key, value) in headers {
        let stable_value = if is_secret_header(key) {
            "<redacted>"
        } else {
            value.as_str()
        };
        out.push_str(key);
        out.push(':');
        out.push_str(stable_value);
        out.push('\n');
    }
    out
}

fn redacted_headers_map(headers: &[(String, String)]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (key, value) in headers {
        let redacted = if is_secret_header(key) {
            "<redacted>".to_string()
        } else {
            value.clone()
        };
        out.insert(key.clone(), redacted);
    }
    out
}

fn cassette_key(
    service: &str,
    endpoint: &str,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
) -> HttpCassetteKey {
    HttpCassetteKey {
        service: service.to_string(),
        endpoint: endpoint.to_string(),
        method: method.to_ascii_lowercase(),
        url_hash: sha256_hex(url.as_bytes()),
        body_hash: sha256_hex(body.as_bytes()),
        headers_hash: sha256_hex(stable_headers_materialized(headers).as_bytes()),
    }
}

fn cassette_path(root: &Path, key: &HttpCassetteKey) -> PathBuf {
    root.join(key.file_name())
}

fn reqwest_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| reqwest::Client::builder().build().expect("reqwest client"))
        .clone()
}

async fn perform_http_call_record_async(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
    timeout_ms: i64,
) -> Result<HttpRuntimeResponse, String> {
    if timeout_ms <= 0 {
        return Err("http_call timeout_ms must be > 0".to_string());
    }
    if url.is_empty() {
        return Err("http_call requires a non-empty URL".to_string());
    }
    let client = reqwest_client();
    let timeout = Duration::from_millis(timeout_ms as u64);

    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        if key == "content-length" {
            continue;
        }
        let name = HeaderName::try_from(key.as_str())
            .map_err(|err| format!("invalid header name: {err}"))?;
        let val = HeaderValue::try_from(value.as_str())
            .map_err(|err| format!("invalid header value: {err}"))?;
        header_map.insert(name, val);
    }

    let method = method
        .parse::<reqwest::Method>()
        .map_err(|_| format!("unsupported HTTP method: {method}"))?;

    let response = client
        .request(method, url)
        .headers(header_map)
        .body(body.to_string())
        .timeout(timeout)
        .send()
        .await
        .map_err(|err| format!("http request failed: {err}"))?;

    let status = response.status().as_u16();
    let mut headers = BTreeMap::new();
    for (key, value) in response.headers() {
        let key_lower = key.as_str().to_ascii_lowercase();
        if is_volatile_response_header(&key_lower) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            headers.insert(key_lower, v.to_string());
        }
    }
    let body = response
        .bytes()
        .await
        .map_err(|err| format!("http response body read failed: {err}"))?
        .to_vec();

    Ok(HttpRuntimeResponse {
        status,
        headers,
        body,
    })
}

pub(crate) fn http_call(
    service: Value,
    endpoint: Value,
    method: Value,
    url: Value,
    headers: Value,
    body: Value,
    timeout_ms: Value,
) -> Value {
    if !config::capability_net_enabled() {
        return capability_denied_result("net", "http_call");
    }
    let Some(service) = value_to_string(service) else {
        return result::result_err(builtin_error("http_call expects String service"));
    };
    let Some(endpoint) = value_to_string(endpoint) else {
        return result::result_err(builtin_error("http_call expects String endpoint"));
    };
    let Some(method) = value_to_string(method) else {
        return result::result_err(builtin_error("http_call expects String method"));
    };
    let Some(url) = value_to_string(url) else {
        return result::result_err(builtin_error("http_call expects String url"));
    };
    let Some(body) = value_to_string(body) else {
        return result::result_err(builtin_error("http_call expects String body"));
    };
    let Some(timeout_ms) = int_value(timeout_ms) else {
        return result::result_err(builtin_error("http_call expects Integer timeout_ms"));
    };
    let headers = match collect_headers(headers) {
        Ok(headers) => headers,
        Err(message) => return result::result_err(builtin_error(&message)),
    };
    let key = cassette_key(&service, &endpoint, &method, &url, &headers, &body);
    let cassette_root = match cassette_root() {
        Ok(path) => path,
        Err(message) => return result::result_err(builtin_error(&message)),
    };
    let cassette_path = cassette_path(&cassette_root, &key);

    match http_mode_from_env() {
        HttpMode::Replay => {
            let payload = match std::fs::read_to_string(&cassette_path) {
                Ok(payload) => payload,
                Err(_) => {
                    let message = format!(
                        "cassette missing for replay mode: expected '{}' (run `wrela test --record` to create it)",
                        cassette_path.display()
                    );
                    return result::result_err(builtin_error(&message));
                }
            };
            let cassette: HttpCassetteV1 = match serde_json::from_str(&payload) {
                Ok(cassette) => cassette,
                Err(err) => {
                    let message = format!(
                        "cassette parse failed at '{}': {err}",
                        cassette_path.display()
                    );
                    return result::result_err(builtin_error(&message));
                }
            };
            if cassette.version != HTTP_CASSETTE_SCHEMA_VERSION {
                let message = format!(
                    "unsupported cassette version {} at '{}' (expected version {})",
                    cassette.version,
                    cassette_path.display(),
                    HTTP_CASSETTE_SCHEMA_VERSION
                );
                return result::result_err(builtin_error(&message));
            }
            let body_bytes = match BASE64.decode(cassette.response.body_base64.as_bytes()) {
                Ok(bytes) => bytes,
                Err(err) => {
                    let message = format!(
                        "cassette response body decode failed at '{}': {err}",
                        cassette_path.display()
                    );
                    return result::result_err(builtin_error(&message));
                }
            };
            let response_text = match String::from_utf8(body_bytes) {
                Ok(text) => text,
                Err(_) => {
                    let message = format!(
                        "cassette response body at '{}' is not valid UTF-8",
                        cassette_path.display()
                    );
                    return result::result_err(builtin_error(&message));
                }
            };
            result::result_ok(string::str_from_bytes(response_text.as_bytes()))
        }
        HttpMode::Record => {
            let response = match runtime::tokio_runtime().block_on(perform_http_call_record_async(
                &method, &url, &headers, &body, timeout_ms,
            )) {
                Ok(response) => response,
                Err(message) => {
                    return result::result_err(builtin_error(&format!(
                        "http record failed: {message}"
                    )));
                }
            };
            let request_headers_redacted = redacted_headers_map(&headers);
            let request_body_redacted = redact_json_body_bytes(body.as_bytes());
            let response_headers_redacted = redacted_response_headers_map(&response.headers);
            let response_body_redacted = redact_json_body_bytes(&response.body);
            let cassette = HttpCassetteV1 {
                version: HTTP_CASSETTE_SCHEMA_VERSION,
                request: HttpCassetteRequestV1 {
                    service,
                    endpoint,
                    method,
                    url,
                    headers_redacted: request_headers_redacted,
                    body_base64: BASE64.encode(&request_body_redacted),
                },
                response: HttpCassetteResponseV1 {
                    status: response.status,
                    headers: response_headers_redacted,
                    body_base64: BASE64.encode(&response_body_redacted),
                },
            };
            let payload = match serde_json::to_vec_pretty(&cassette) {
                Ok(payload) => payload,
                Err(err) => {
                    return result::result_err(builtin_error(&format!(
                        "failed to serialize cassette: {err}"
                    )));
                }
            };
            if let Err(err) = write_cassette_atomic(&cassette_path, &payload) {
                return result::result_err(builtin_error(&format!(
                    "failed to write cassette atomically '{}': {err}",
                    cassette_path.display()
                )));
            }
            let response_text = match String::from_utf8(response.body) {
                Ok(text) => text,
                Err(_) => {
                    return result::result_err(builtin_error(
                        "http response body is not valid UTF-8 for current String API",
                    ));
                }
            };
            result::result_ok(string::str_from_bytes(response_text.as_bytes()))
        }
    }
}

/// Async variant for Rust-native callers (e.g. Axum handlers). Performs the full cassette
/// flow: replay from disk or record via async HTTP, returning the response body as UTF-8.
#[allow(dead_code)]
pub async fn http_call_async(
    service: &str,
    endpoint: &str,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
    timeout_ms: i64,
) -> Result<String, String> {
    if !config::capability_net_enabled() {
        return Err("capability_denied:net.http_call".to_string());
    }
    let key = cassette_key(service, endpoint, method, url, headers, body);
    let cassette_root = cassette_root()?;
    let cassette_path = cassette_path(&cassette_root, &key);

    match http_mode_from_env() {
        HttpMode::Replay => {
            let path = cassette_path.clone();
            let payload = tokio::task::spawn_blocking(move || std::fs::read_to_string(path))
                .await
                .map_err(|e| format!("cassette read task failed: {e}"))?
                .map_err(|e| {
                    format!(
                        "cassette missing for replay mode: expected '{}' (run `wrela test --record` to create it): {e}",
                        cassette_path.display()
                    )
                })?;
            let cassette: HttpCassetteV1 = serde_json::from_str(&payload).map_err(|err| {
                format!(
                    "cassette parse failed at '{}': {err}",
                    cassette_path.display()
                )
            })?;
            if cassette.version != HTTP_CASSETTE_SCHEMA_VERSION {
                return Err(format!(
                    "unsupported cassette version {} at '{}' (expected version {})",
                    cassette.version,
                    cassette_path.display(),
                    HTTP_CASSETTE_SCHEMA_VERSION
                ));
            }
            let body_bytes = BASE64
                .decode(cassette.response.body_base64.as_bytes())
                .map_err(|err| {
                    format!(
                        "cassette response body decode failed at '{}': {err}",
                        cassette_path.display()
                    )
                })?;
            String::from_utf8(body_bytes).map_err(|_| {
                format!(
                    "cassette response body at '{}' is not valid UTF-8",
                    cassette_path.display()
                )
            })
        }
        HttpMode::Record => {
            let response =
                perform_http_call_record_async(method, url, headers, body, timeout_ms).await?;
            let request_headers_redacted = redacted_headers_map(headers);
            let request_body_redacted = redact_json_body_bytes(body.as_bytes());
            let response_headers_redacted = redacted_response_headers_map(&response.headers);
            let response_body_redacted = redact_json_body_bytes(&response.body);
            let cassette = HttpCassetteV1 {
                version: HTTP_CASSETTE_SCHEMA_VERSION,
                request: HttpCassetteRequestV1 {
                    service: service.to_string(),
                    endpoint: endpoint.to_string(),
                    method: method.to_string(),
                    url: url.to_string(),
                    headers_redacted: request_headers_redacted,
                    body_base64: BASE64.encode(&request_body_redacted),
                },
                response: HttpCassetteResponseV1 {
                    status: response.status,
                    headers: response_headers_redacted,
                    body_base64: BASE64.encode(&response_body_redacted),
                },
            };
            let payload = serde_json::to_vec_pretty(&cassette)
                .map_err(|err| format!("failed to serialize cassette: {err}"))?;
            let path = cassette_path.clone();
            tokio::task::spawn_blocking(move || write_cassette_atomic(&path, &payload))
                .await
                .map_err(|e| format!("cassette write task failed: {e}"))?
                .map_err(|err| {
                    format!(
                        "failed to write cassette atomically '{}': {err}",
                        cassette_path.display()
                    )
                })?;
            String::from_utf8(response.body).map_err(|_| {
                "http response body is not valid UTF-8 for current String API".to_string()
            })
        }
    }
}

pub(crate) fn external_call(
    service: Value,
    endpoint: Value,
    method: Value,
    url: Value,
    headers: Value,
    body: Value,
    timeout_ms: Value,
) -> Value {
    if !config::capability_net_enabled() {
        return capability_denied_result("net", "external_call");
    }
    let Some(service) = value_to_string(service) else {
        return result::result_err(builtin_error("external_call expects String service"));
    };
    let Some(endpoint) = value_to_string(endpoint) else {
        return result::result_err(builtin_error("external_call expects String endpoint"));
    };
    let Some(method) = value_to_string(method) else {
        return result::result_err(builtin_error("external_call expects String method"));
    };
    let Some(url) = value_to_string(url) else {
        return result::result_err(builtin_error("external_call expects String url"));
    };
    let Some(body) = value_to_string(body) else {
        return result::result_err(builtin_error("external_call expects String body"));
    };
    let Some(timeout_ms) = int_value(timeout_ms) else {
        return result::result_err(builtin_error("external_call expects Integer timeout_ms"));
    };
    let Some(headers_ref) = map::as_map_ref(headers) else {
        return result::result_err(builtin_error("external_call expects Map headers"));
    };
    let headers_len = map::map_len(headers_ref);
    let response = format!(
        "external.stub:service={service};endpoint={endpoint};method={method};url={url};headers={headers_len};body_len={};timeout_ms={timeout_ms}",
        body.len()
    );
    result::result_ok(string::str_from_bytes(response.as_bytes()))
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn env_overlay() -> &'static Mutex<HashMap<String, String>> {
    static OVERLAY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    OVERLAY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn env_get(key: Value) -> Value {
    if !config::capability_env_get_enabled() {
        capability_denied_log("env", "get");
        return Value::nil();
    }
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    if let Ok(overlay) = env_overlay().lock() {
        if let Some(val) = overlay.get(&key) {
            return string::str_from_bytes(val.as_bytes());
        }
    }
    match std::env::var(&key).ok() {
        Some(val) => string::str_from_bytes(val.as_bytes()),
        None => Value::nil(),
    }
}

pub(crate) fn env_set(key: Value, val: Value) -> Value {
    if !config::capability_env_set_enabled() {
        capability_denied_log("env", "set");
        return Value::from_bool(false);
    }
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::from_bool(false),
    };
    let val = match value_to_string(val) {
        Some(val) => val,
        None => return Value::from_bool(false),
    };
    match env_overlay().lock() {
        Ok(mut overlay) => {
            overlay.insert(key, val);
            Value::from_bool(true)
        }
        Err(_) => Value::from_bool(false),
    }
}

pub(crate) fn clock_ns() -> Value {
    if !config::capability_time_enabled() {
        capability_denied_log("time", "now");
        return Value::from_int(0);
    }
    if virtual_time_enabled() {
        return Value::from_int(virtual_clock_ns().load(Ordering::Relaxed));
    }
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    Value::from_int(start.elapsed().as_nanos() as i64)
}

pub(crate) fn sleep_ms(ms_val: Value) -> Value {
    if !config::capability_time_enabled() {
        capability_denied_log("time", "sleep");
        return crate::actor::sleep_ms(0);
    }
    let ms = int_value(ms_val).unwrap_or(0);
    if virtual_time_enabled() {
        if ms > 0 {
            let delta_ns = ms.saturating_mul(1_000_000);
            virtual_clock_ns().fetch_add(delta_ns, Ordering::Relaxed);
        }
        return crate::actor::sleep_ms(0);
    }
    crate::actor::sleep_ms(ms)
}

fn virtual_time_enabled() -> bool {
    #[cfg(test)]
    {
        match virtual_time_test_override().load(Ordering::Relaxed) {
            0 => return false,
            1 => return true,
            _ => {}
        }
    }
    let env_enabled = std::env::var("WRELA_TEST_VIRTUAL_TIME")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    env_enabled || config::deterministic_runtime_enabled()
}

fn virtual_clock_ns() -> &'static AtomicI64 {
    static CLOCK: OnceLock<AtomicI64> = OnceLock::new();
    CLOCK.get_or_init(|| {
        let start = std::env::var("WRELA_VIRTUAL_TIME_START_NS")
            .ok()
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .unwrap_or(0);
        AtomicI64::new(start)
    })
}

#[cfg(test)]
fn set_virtual_clock_ns_for_tests(value: i64) {
    virtual_clock_ns().store(value, Ordering::Relaxed);
}

#[cfg(test)]
fn virtual_time_test_override() -> &'static AtomicI8 {
    static OVERRIDE: OnceLock<AtomicI8> = OnceLock::new();
    OVERRIDE.get_or_init(|| AtomicI8::new(-1))
}

#[cfg(test)]
fn set_virtual_time_enabled_for_tests(value: bool) {
    virtual_time_test_override().store(if value { 1 } else { 0 }, Ordering::Relaxed);
}

#[cfg(test)]
fn clear_virtual_time_enabled_for_tests() {
    virtual_time_test_override().store(-1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RuntimeConfig, set_test_runtime_config_override};
    use crate::data::result;
    use crate::string;
    use std::sync::{Arc, Barrier, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct CapabilityOverrideGuard;

    impl CapabilityOverrideGuard {
        fn install(config: RuntimeConfig) -> Self {
            set_test_runtime_config_override(Some(config));
            Self
        }
    }

    impl Drop for CapabilityOverrideGuard {
        fn drop(&mut self) {
            set_test_runtime_config_override(None);
        }
    }

    fn virtual_time_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("virtual time test lock")
    }

    #[test]
    fn redact_json_body_bytes_redacts_secret_keys_recursively() {
        let body = br#"{
  "api_key":"abc",
  "nested":{"token":"xyz","safe":"ok"},
  "items":[{"password":"pw"},{"secret":"shh"}]
}"#;
        let redacted = redact_json_body_bytes(body);
        let json: serde_json::Value = serde_json::from_slice(&redacted).expect("json parse");
        assert_eq!(json["api_key"], "<redacted>");
        assert_eq!(json["nested"]["token"], "<redacted>");
        assert_eq!(json["nested"]["safe"], "ok");
        assert_eq!(json["items"][0]["password"], "<redacted>");
        assert_eq!(json["items"][1]["secret"], "<redacted>");
    }

    #[test]
    fn redact_json_body_bytes_keeps_non_json_body_unchanged() {
        let body = b"not-json";
        let redacted = redact_json_body_bytes(body);
        assert_eq!(redacted, body);
    }

    #[test]
    fn redacted_response_headers_map_redacts_set_cookie_only() {
        let mut headers = BTreeMap::new();
        headers.insert(
            "set-cookie".to_string(),
            "session=abc; HttpOnly".to_string(),
        );
        headers.insert("content-type".to_string(), "application/json".to_string());
        let redacted = redacted_response_headers_map(&headers);
        assert_eq!(
            redacted.get("set-cookie").map(String::as_str),
            Some("<redacted>")
        );
        assert_eq!(
            redacted.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn write_cassette_atomic_concurrent_writers_produce_valid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cassette_path = dir.path().join("shared.json");
        let workers = 16usize;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::with_capacity(workers);
        for i in 0..workers {
            let barrier = Arc::clone(&barrier);
            let cassette_path = cassette_path.clone();
            handles.push(std::thread::spawn(move || {
                let payload = format!("{{\"worker\":{},\"token\":\"s{}\",\"safe\":\"ok\"}}", i, i);
                barrier.wait();
                write_cassette_atomic(&cassette_path, payload.as_bytes())
            }));
        }
        for handle in handles {
            handle.join().expect("thread join").expect("atomic write");
        }

        let final_payload = std::fs::read(&cassette_path).expect("read cassette");
        let parsed: serde_json::Value = serde_json::from_slice(&final_payload).expect("valid json");
        assert!(parsed.get("worker").is_some());
        assert!(parsed.get("token").is_some());
        assert_eq!(parsed["safe"], "ok");

        let entries = std::fs::read_dir(dir.path()).expect("read dir");
        for entry in entries {
            let entry = entry.expect("dir entry");
            let name = entry.file_name();
            let name = name.to_string_lossy();
            assert!(!name.contains(".lock"), "lock file leaked: {name}");
            assert!(!name.contains(".tmp."), "temp file leaked: {name}");
        }
    }

    #[test]
    fn virtual_clock_ns_is_deterministic_when_enabled() {
        let _lock = virtual_time_test_guard();
        let mut cfg = RuntimeConfig::default();
        cfg.allow_time = true;
        let _guard = CapabilityOverrideGuard::install(cfg);
        set_virtual_time_enabled_for_tests(true);
        set_virtual_clock_ns_for_tests(123);
        let first = int_value(clock_ns()).expect("clock first");
        let second = int_value(clock_ns()).expect("clock second");
        assert_eq!(first, 123);
        assert_eq!(second, 123);
        clear_virtual_time_enabled_for_tests();
    }

    #[test]
    fn virtual_sleep_advances_clock_without_wall_delay() {
        let _lock = virtual_time_test_guard();
        let mut cfg = RuntimeConfig::default();
        cfg.allow_time = true;
        let _guard = CapabilityOverrideGuard::install(cfg);
        set_virtual_time_enabled_for_tests(true);
        set_virtual_clock_ns_for_tests(0);
        let started = Instant::now();
        let pending = sleep_ms(Value::from_int(1000));
        unsafe {
            crate::wr_rc_dec(pending);
        }
        let elapsed = started.elapsed();
        let now = int_value(clock_ns()).expect("clock");
        assert!(elapsed < Duration::from_millis(100));
        assert_eq!(now, 1_000_000_000);
        clear_virtual_time_enabled_for_tests();
    }

    #[test]
    fn deterministic_runtime_implicitly_uses_virtual_time() {
        let _lock = virtual_time_test_guard();
        let mut cfg = RuntimeConfig::default();
        cfg.allow_time = true;
        cfg.deterministic = true;
        let _guard = CapabilityOverrideGuard::install(cfg);
        clear_virtual_time_enabled_for_tests();
        set_virtual_clock_ns_for_tests(0);
        let started = Instant::now();
        let pending = sleep_ms(Value::from_int(25));
        unsafe {
            crate::wr_rc_dec(pending);
        }
        let elapsed = started.elapsed();
        let now = int_value(clock_ns()).expect("clock");
        assert!(elapsed < Duration::from_millis(100));
        assert_eq!(now, 25_000_000);
        clear_virtual_time_enabled_for_tests();
    }

    #[test]
    fn sandbox_denies_fs_and_net_builtins() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_fs = false;
        cfg.allow_net = false;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let path = string::str_from_bytes(b"./nonexistent");
        let read_result = fs_read_bytes(path);
        assert!(!result::result_is_ok(read_result).as_bool());
        let read_err = result::result_err_unwrap(read_result);
        let read_err_text = value_to_string(read_err).expect("error text");
        assert_eq!(read_err_text, "capability_denied:fs.read");

        let write_result = fs_write_bytes(path, Value::nil());
        assert!(!result::result_is_ok(write_result).as_bool());
        let write_err = result::result_err_unwrap(write_result);
        let write_err_text = value_to_string(write_err).expect("error text");
        assert_eq!(write_err_text, "capability_denied:fs.write");

        let service = string::str_from_bytes(b"billing");
        let endpoint = string::str_from_bytes(b"charge");
        let method = string::str_from_bytes(b"POST");
        let url = string::str_from_bytes(b"https://api.example.test/charge");
        let headers = map::map_new();
        let body = string::str_from_bytes(b"{}");
        let timeout_ms = Value::from_int(1000);
        let net_result = external_call(service, endpoint, method, url, headers, body, timeout_ms);
        assert!(!result::result_is_ok(net_result).as_bool());
        let net_err = result::result_err_unwrap(net_result);
        let net_err_text = value_to_string(net_err).expect("error text");
        assert_eq!(net_err_text, "capability_denied:net.external_call");

        unsafe {
            crate::wr_rc_dec(path);
            crate::wr_rc_dec(read_result);
            crate::wr_rc_dec(read_err);
            crate::wr_rc_dec(write_result);
            crate::wr_rc_dec(write_err);
            crate::wr_rc_dec(service);
            crate::wr_rc_dec(endpoint);
            crate::wr_rc_dec(method);
            crate::wr_rc_dec(url);
            crate::wr_rc_dec(headers);
            crate::wr_rc_dec(body);
            crate::wr_rc_dec(net_result);
            crate::wr_rc_dec(net_err);
        }
    }

    #[test]
    fn sandbox_denies_env_set_and_time_when_disabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_env_get = false;
        cfg.allow_env_set = false;
        cfg.allow_time = false;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let key = string::str_from_bytes(b"WRELA_CAPABILITY_TEST");
        let value = string::str_from_bytes(b"ok");
        let set_ok = env_set(key, value);
        assert!(!set_ok.as_bool());
        let got = env_get(key);
        assert!(got.is_nil());
        let started = Instant::now();
        let sleep_result = sleep_ms(Value::from_int(250));
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(
            result::result_is_ok(sleep_result).as_bool(),
            "time-denied sleep should resolve immediately"
        );
        let now = clock_ns();
        assert_eq!(int_value(now), Some(0));

        unsafe {
            crate::wr_rc_dec(key);
            crate::wr_rc_dec(value);
            crate::wr_rc_dec(got);
            crate::wr_rc_dec(sleep_result);
            crate::wr_rc_dec(now);
            crate::wr_rc_dec(set_ok);
        }
    }

    #[test]
    fn sandbox_denies_http_call_sync_and_async_when_net_disabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.allow_net = false;
        let _guard = CapabilityOverrideGuard::install(cfg);

        let service = string::str_from_bytes(b"billing");
        let endpoint = string::str_from_bytes(b"charge");
        let method = string::str_from_bytes(b"POST");
        let url = string::str_from_bytes(b"https://api.example.test/charge");
        let headers = map::map_new();
        let body = string::str_from_bytes(b"{}");
        let timeout_ms = Value::from_int(1000);
        let net_result = http_call(service, endpoint, method, url, headers, body, timeout_ms);
        assert!(!result::result_is_ok(net_result).as_bool());
        let net_err = result::result_err_unwrap(net_result);
        let net_err_text = value_to_string(net_err).expect("error text");
        assert_eq!(net_err_text, "capability_denied:net.http_call");

        let async_err = runtime::tokio_runtime().block_on(http_call_async(
            "billing",
            "charge",
            "POST",
            "https://api.example.test/charge",
            &[],
            "{}",
            1000,
        ));
        assert_eq!(
            async_err,
            Err("capability_denied:net.http_call".to_string())
        );

        unsafe {
            crate::wr_rc_dec(service);
            crate::wr_rc_dec(endpoint);
            crate::wr_rc_dec(method);
            crate::wr_rc_dec(url);
            crate::wr_rc_dec(headers);
            crate::wr_rc_dec(body);
            crate::wr_rc_dec(net_result);
            crate::wr_rc_dec(net_err);
        }
    }
}
