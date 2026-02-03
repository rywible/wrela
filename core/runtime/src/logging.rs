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
        for item in (&(*list_ref).data).iter().take((*list_ref).len) {
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
    unsafe {
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
            .is_none() {
                continue;
            }
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
