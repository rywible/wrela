use crate::list;
use crate::map;
use crate::string;
use crate::value::{Value, int_value};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::time::{SystemTime, UNIX_EPOCH};

fn log_level_threshold() -> u8 {
    let raw = std::env::var("WRELA_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    match raw.to_ascii_lowercase().as_str() {
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

fn map_value_to_json(val: Value) -> JsonValue {
    let Some(map_ref) = map::as_map_ref(val) else {
        return JsonValue::Null;
    };
    let mut obj = JsonMap::new();
    unsafe {
        for (key, value) in (&(*map_ref).entries).iter() {
            let Some(key_str) = value_to_string(key.0) else {
                continue;
            };
            obj.insert(key_str, value_to_json(*value));
        }
    }
    JsonValue::Object(obj)
}

fn list_value_to_json(val: Value) -> JsonValue {
    let Some(list_ref) = list::as_list_ref(val) else {
        return JsonValue::Null;
    };
    let mut out = Vec::new();
    unsafe {
        for item in (&(*list_ref).data).iter().take((*list_ref).len) {
            out.push(value_to_json(*item));
        }
    }
    JsonValue::Array(out)
}

fn value_to_json(val: Value) -> JsonValue {
    if val.is_nil() {
        return JsonValue::Null;
    }
    if val.is_bool() {
        return JsonValue::Bool(val.as_bool());
    }
    if let Some(i) = int_value(val) {
        return JsonValue::Number(i.into());
    }
    if val.is_float() {
        if let Some(num) = serde_json::Number::from_f64(val.as_float()) {
            return JsonValue::Number(num);
        }
        return JsonValue::Null;
    }
    if let Some(s) = value_to_string(val) {
        return JsonValue::String(s);
    }
    if list::as_list_ref(val).is_some() {
        return list_value_to_json(val);
    }
    if map::as_map_ref(val).is_some() {
        return map_value_to_json(val);
    }
    JsonValue::String("<value>".to_string())
}

pub fn log(level: Value, msg: Value, fields: Value) -> Value {
    let level = value_to_string(level).unwrap_or_else(|| "info".to_string());
    let level = level.to_ascii_lowercase();
    if level_value(&level) < log_level_threshold() {
        return Value::from_bool(false);
    }
    let message = value_to_string(msg).unwrap_or_else(|| "<value>".to_string());
    let fields_json = if fields.is_nil() {
        JsonValue::Null
    } else {
        value_to_json(fields)
    };
    let mut obj = JsonMap::new();
    obj.insert("ts".to_string(), JsonValue::Number(now_millis().into()));
    obj.insert("level".to_string(), JsonValue::String(level.clone()));
    obj.insert("msg".to_string(), JsonValue::String(message));
    if !fields_json.is_null() {
        obj.insert("fields".to_string(), fields_json);
    }
    let line = JsonValue::Object(obj).to_string();
    if level_value(&level) >= 30 {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
    Value::from_bool(true)
}
