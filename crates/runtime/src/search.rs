use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::list;
use crate::map;
use crate::storage_helpers::{storage_delete, storage_get_json, storage_get_json_vec, storage_set_json, value_to_string};
use crate::string;
use crate::value::{int_value, Value};
use crate::wr_rc_dec;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Serialize, Deserialize)]
struct Document {
    id: String,
    text: String,
    fields: HashMap<String, JsonValue>,
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn json_from_map(val: Value) -> HashMap<String, JsonValue> {
    let mut out = HashMap::new();
    let Some(map_ptr) = map::as_map_ref(val) else { return out };
    unsafe {
        for (key, value) in (&(*map_ptr).entries).iter() {
            let Some(key_str) = value_to_string(key.0) else { continue };
            let json_val = if value.is_nil() {
                JsonValue::Null
            } else if value.is_bool() {
                JsonValue::Bool(value.as_bool())
            } else if let Some(i) = int_value(*value) {
                JsonValue::Number(i.into())
            } else if value.is_float() {
                serde_json::Number::from_f64(value.as_float())
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            } else if let Some(s) = value_to_string(*value) {
                JsonValue::String(s)
            } else {
                JsonValue::Null
            };
            out.insert(key_str, json_val);
        }
    }
    out
}

fn map_from_fields(fields: &HashMap<String, JsonValue>) -> Value {
    let map_val = map::map_new();
    for (key, val) in fields {
        let key_val = string::str_from_bytes(key.as_bytes());
        let out_val = match val {
            JsonValue::Null => Value::nil(),
            JsonValue::Bool(b) => Value::from_bool(*b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::from_int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::from_float(f)
                } else {
                    Value::nil()
                }
            }
            JsonValue::String(s) => string::str_from_bytes(s.as_bytes()),
            JsonValue::Array(_) | JsonValue::Object(_) => Value::nil(),
        };
        map::map_set(map_val, key_val, out_val);
        unsafe {
            wr_rc_dec(key_val);
            if out_val.is_ptr() {
                wr_rc_dec(out_val);
            }
        }
    }
    map_val
}

fn map_get_map(map_val: Value, key: &str) -> Option<Value> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    if got.is_nil() {
        return None;
    }
    Some(got)
}

fn map_get_int(map_val: Value, key: &str) -> Option<i64> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    let out = int_value(got);
    unsafe { wr_rc_dec(got) };
    out
}

pub fn search_index(storage: Value, collection: Value, id: Value, text: Value, fields: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let collection = match value_to_string(collection) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let id = match value_to_string(id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let text = match value_to_string(text) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let fields_map = if fields.is_nil() {
        HashMap::new()
    } else {
        json_from_map(fields)
    };
    runtime_spawn(async move {
        let doc_key = format!("search:doc:{collection}:{id}");
        let doc = Document {
            id: id.clone(),
            text: text.clone(),
            fields: fields_map,
        };
        let mut ids = storage_get_json_vec::<String>(&format!("search:collection:{collection}")).await;
        if !ids.contains(&id) {
            ids.push(id.clone());
        }
        let ok = storage_set_json(&doc_key, &doc).await
            && storage_set_json(&format!("search:collection:{collection}"), &ids).await;
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}

pub fn search_remove(storage: Value, collection: Value, id: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let collection = match value_to_string(collection) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let id = match value_to_string(id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let doc_key = format!("search:doc:{collection}:{id}");
        let removed = storage_get_json::<Document>(&doc_key).await.is_some();
        if removed {
            let mut ids = storage_get_json_vec::<String>(&format!("search:collection:{collection}")).await;
            ids.retain(|val| val != &id);
            let _ = storage_set_json(&format!("search:collection:{collection}"), &ids).await;
            let _ = storage_delete(&doc_key).await;
        }
        resolve_pending(state, Value::from_bool(removed));
    });
    pending
}

pub fn search_query(storage: Value, collection: Value, query: Value, opts: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let collection = match value_to_string(collection) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let query = match value_to_string(query) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let filters = map_get_map(opts, "filters");
    let limit = map_get_int(opts, "limit").unwrap_or(50).max(0) as usize;
    let offset = map_get_int(opts, "offset").unwrap_or(0).max(0) as usize;
    runtime_spawn(async move {
        let tokens = tokenize(&query);
        if tokens.is_empty() {
            resolve_pending(state, list::list_new(0));
            return;
        }
        let ids = storage_get_json_vec::<String>(&format!("search:collection:{collection}")).await;
        let mut docs = Vec::new();
        for id in ids {
            if let Some(doc) = storage_get_json::<Document>(&format!("search:doc:{collection}:{id}")).await {
                docs.push(doc);
            }
        }
        let token_set: HashSet<String> = tokens.into_iter().collect();
        docs.retain(|doc| {
            let text_tokens: HashSet<String> = tokenize(&doc.text).into_iter().collect();
            token_set.is_subset(&text_tokens)
        });
        if let Some(filters_val) = filters {
            let filter_fields = json_from_map(filters_val);
            docs.retain(|doc| {
                for (key, val) in &filter_fields {
                    if doc.fields.get(key) != Some(val) {
                        return false;
                    }
                }
                true
            });
            unsafe { wr_rc_dec(filters_val) };
        }
        let list_val = list::list_new(0);
        for doc in docs.into_iter().skip(offset).take(limit) {
            let map_val = map::map_new();
            let key_id = string::str_from_bytes(b"id");
            let id_val = string::str_from_bytes(doc.id.as_bytes());
            map::map_set(map_val, key_id, id_val);
            unsafe {
                wr_rc_dec(key_id);
                wr_rc_dec(id_val);
            }
            let key_text = string::str_from_bytes(b"text");
            let text_val = string::str_from_bytes(doc.text.as_bytes());
            map::map_set(map_val, key_text, text_val);
            unsafe {
                wr_rc_dec(key_text);
                wr_rc_dec(text_val);
            }
            let key_fields = string::str_from_bytes(b"fields");
            let fields_val = map_from_fields(&doc.fields);
            map::map_set(map_val, key_fields, fields_val);
            unsafe {
                wr_rc_dec(key_fields);
                wr_rc_dec(fields_val);
            }
            list::list_push(list_val, map_val);
            unsafe { wr_rc_dec(map_val) };
        }
        resolve_pending(state, list_val);
    });
    pending
}
