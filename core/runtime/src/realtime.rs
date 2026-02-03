use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::bytes;
use crate::map;
use crate::pubsub;
use crate::storage::config::storage_config;
use crate::storage_helpers::{
    storage_delete, storage_get_string, storage_get_string_with_version, storage_set_string_if_version,
};
use crate::string;
use crate::value::Value;
use crate::{list, wr_rc_dec, wr_rc_inc};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
#[cfg(any(test, feature = "test-utils"))]
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Clone)]
pub(crate) struct RealtimeConfig {
    pub(crate) socket_ttl_secs: Option<u64>,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self { socket_ttl_secs: None }
    }
}

static REALTIME_CONFIG: OnceLock<Mutex<RealtimeConfig>> = OnceLock::new();

fn realtime_config() -> RealtimeConfig {
    REALTIME_CONFIG
        .get_or_init(|| Mutex::new(RealtimeConfig::default()))
        .lock()
        .expect("realtime config lock")
        .clone()
}

fn set_realtime_config(config: RealtimeConfig) {
    *REALTIME_CONFIG
        .get_or_init(|| Mutex::new(RealtimeConfig::default()))
        .lock()
        .expect("realtime config lock") = config;
}

#[derive(Default)]
pub(crate) struct RealtimeState {
    rooms: HashMap<String, HashSet<String>>,
    inbox: HashMap<String, Vec<Value>>,
    on_connect: Option<Value>,
}

pub(crate) type RealtimeStateHandleArc = Arc<RwLock<RealtimeState>>;

static STATE: OnceLock<RealtimeStateHandleArc> = OnceLock::new();
static CLEANUP_STARTED: OnceLock<()> = OnceLock::new();
static PUBSUB_REGISTERED: OnceLock<()> = OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    static REALTIME_STATE_OVERRIDE: Arc<RwLock<RealtimeState>>;
}

fn realtime_state() -> RealtimeStateHandleArc {
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(state) = REALTIME_STATE_OVERRIDE.try_with(Arc::clone) {
        return state;
    }
    STATE
        .get_or_init(|| Arc::new(RwLock::new(RealtimeState::default())))
        .clone()
}

pub(crate) fn realtime_state_shared() -> RealtimeStateHandleArc {
    STATE
        .get_or_init(|| Arc::new(RwLock::new(RealtimeState::default())))
        .clone()
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub struct RealtimeStateHandle(Arc<RwLock<RealtimeState>>);

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub fn new_realtime_state_for_test() -> RealtimeStateHandle {
    RealtimeStateHandle(Arc::new(RwLock::new(RealtimeState::default())))
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub async fn with_realtime_state_override<F, R>(state: &RealtimeStateHandle, fut: F) -> R
where
    F: Future<Output = R>,
{
    REALTIME_STATE_OVERRIDE.scope(state.0.clone(), fut).await
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(dead_code)]
pub async fn take_inbox_for_test(socket_id: &str) -> Vec<Value> {
    let state = realtime_state();
    let mut guard = state.write().await;
    guard.inbox.remove(socket_id).unwrap_or_default()
}

#[cfg(any(test, feature = "test-utils"))]
impl RealtimeStateHandle {
    #[allow(dead_code)]
    pub fn as_arc(&self) -> RealtimeStateHandleArc {
        self.0.clone()
    }
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn value_to_json(val: Value, depth: usize) -> JsonValue {
    if depth > 16 {
        return JsonValue::Null;
    }
    if val.is_nil() {
        return JsonValue::Null;
    }
    if val.is_bool() {
        return JsonValue::Bool(val.as_bool());
    }
    if let Some(i) = crate::value::int_value(val) {
        return JsonValue::Number(i.into());
    }
    if val.is_float() {
        return serde_json::Number::from_f64(val.as_float())
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null);
    }
    if let Some(s) = value_to_string(val) {
        return JsonValue::String(s);
    }
    if let Some(bytes) = bytes::with_bytes(val, |b| b.to_vec()) {
        let mut map = JsonMap::new();
        map.insert(
            "__bytes__".to_string(),
            JsonValue::String(STANDARD.encode(bytes)),
        );
        return JsonValue::Object(map);
    }
    if let Some(list_ptr) = list::as_list_ref(val) {
        let mut out = Vec::new();
        unsafe {
            for entry in (*list_ptr).data.iter().take((*list_ptr).len) {
                out.push(value_to_json(*entry, depth + 1));
            }
        }
        return JsonValue::Array(out);
    }
    if let Some(map_ptr) = map::as_map_ref(val) {
        let mut out = JsonMap::new();
        unsafe {
            for (key, value) in (&(*map_ptr).entries).iter() {
                let Some(key_str) = value_to_string(key.0) else {
                    continue;
                };
                out.insert(key_str, value_to_json(*value, depth + 1));
            }
        }
        return JsonValue::Object(out);
    }
    JsonValue::Null
}

fn json_to_value(val: &JsonValue, depth: usize) -> Value {
    if depth > 16 {
        return Value::nil();
    }
    match val {
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
        JsonValue::Array(items) => {
            let list_val = list::list_new(items.len());
            for (idx, item) in items.iter().enumerate() {
                let v = json_to_value(item, depth + 1);
                list::list_set(list_val, idx, v);
            }
            list_val
        }
        JsonValue::Object(map_obj) => {
            if let Some(JsonValue::String(encoded)) = map_obj.get("__bytes__") {
                if let Ok(bytes) = STANDARD.decode(encoded.as_bytes()) {
                    return bytes::bytes_from_slice(&bytes);
                }
            }
            let map_val = map::map_new();
            for (key, value) in map_obj {
                let key_val = string::str_from_bytes(key.as_bytes());
                let val = json_to_value(value, depth + 1);
                map::map_set(map_val, key_val, val);
                unsafe {
                    wr_rc_dec(key_val);
                    if val.is_ptr() {
                        wr_rc_dec(val);
                    }
                }
            }
            map_val
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn realtime_socket_ttl_secs() -> Option<u64> {
    realtime_config().socket_ttl_secs
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SocketRecord {
    node_id: u64,
    updated_at: u64,
}

async fn update_room_membership(room: &str, socket_id: &str, add: bool) {
    let key = format!("realtime:room:{room}");
    for _ in 0..6 {
        let (mut members, version) = match storage_get_string_with_version(&key).await {
            Some((raw, version)) => {
                let members = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
                (members, Some(version))
            }
            None => (Vec::new(), None),
        };
        let mut changed = false;
        if add {
            if !members.iter().any(|m| m == socket_id) {
                members.push(socket_id.to_string());
                changed = true;
            }
        } else {
            let before = members.len();
            members.retain(|m| m != socket_id);
            changed = before != members.len();
        }
        if !changed {
            return;
        }
        let Ok(raw) = serde_json::to_string(&members) else {
            return;
        };
        if storage_set_string_if_version(&key, &raw, version).await {
            return;
        }
        tokio::task::yield_now().await;
    }
}

pub fn realtime_configure(config: Value) -> Value {
    let new_config = realtime_config_from_value(config);
    set_realtime_config(new_config);
    Value::nil()
}

fn realtime_config_from_value(config: Value) -> RealtimeConfig {
    let mut out = RealtimeConfig::default();
    if let Some(val) = config_field_u64(config, "socket_ttl_secs") {
        if val == 0 {
            out.socket_ttl_secs = None;
        } else {
            out.socket_ttl_secs = Some(val);
        }
    }
    out
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    let val = crate::class::class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = crate::value::int_value(val).and_then(|num| if num >= 0 { Some(num as u64) } else { None });
    unsafe { wr_rc_dec(val) };
    out
}

#[cfg(test)]
pub fn set_realtime_config_for_test(config: RealtimeConfig) {
    set_realtime_config(config);
}

#[cfg(test)]
pub fn realtime_config_for_test() -> RealtimeConfig {
    realtime_config()
}

async fn update_socket_rooms(socket_id: &str, room: &str, add: bool) -> bool {
    let key = format!("realtime:socket_rooms:{socket_id}");
    for _ in 0..6 {
        let (mut rooms, version) = match storage_get_string_with_version(&key).await {
            Some((raw, version)) => {
                let rooms = serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default();
                (rooms, Some(version))
            }
            None => (Vec::new(), None),
        };
        let mut changed = false;
        if add {
            if !rooms.iter().any(|r| r == room) {
                rooms.push(room.to_string());
                changed = true;
            }
        } else {
            let before = rooms.len();
            rooms.retain(|r| r != room);
            changed = before != rooms.len();
        }
        if !changed {
            return !rooms.is_empty();
        }
        let Ok(raw) = serde_json::to_string(&rooms) else {
            return !rooms.is_empty();
        };
        if storage_set_string_if_version(&key, &raw, version).await {
            return !rooms.is_empty();
        }
        tokio::task::yield_now().await;
    }
    true
}

async fn store_socket_record(socket_id: &str) {
    let key = format!("realtime:socket:{socket_id}");
    let record = SocketRecord {
        node_id: storage_config().node_id,
        updated_at: now_secs(),
    };
    if let Ok(raw) = serde_json::to_string(&record) {
        let _ = storage_set_string_if_version(&key, &raw, None).await;
    }
}

async fn load_socket_rooms(socket_id: &str) -> Vec<String> {
    let key = format!("realtime:socket_rooms:{socket_id}");
    let Some(raw) = storage_get_string(&key).await else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

async fn delete_socket_records(socket_id: &str) {
    let key_rooms = format!("realtime:socket_rooms:{socket_id}");
    let key_socket = format!("realtime:socket:{socket_id}");
    let _ = storage_delete(&key_rooms).await;
    let _ = storage_delete(&key_socket).await;
}

fn ensure_cleanup_started() {
    if CLEANUP_STARTED.get().is_some() {
        return;
    }
    let Some(ttl_secs) = realtime_socket_ttl_secs() else {
        return;
    };
    if CLEANUP_STARTED.set(()).is_err() {
        return;
    }
    runtime_spawn(async move {
        let interval = Duration::from_secs((ttl_secs / 2).max(1));
        loop {
            tokio::time::sleep(interval).await;
            cleanup_stale_sockets(ttl_secs).await;
        }
    });
}

async fn cleanup_stale_sockets(ttl_secs: u64) {
    let keys = crate::storage_helpers::storage_list_prefix_keys("realtime:socket:", 1000).await;
    if keys.is_empty() {
        return;
    }
    let now = now_secs();
    for key in keys {
        let Some(raw) = storage_get_string(&key).await else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<SocketRecord>(&raw) else {
            continue;
        };
        if now.saturating_sub(record.updated_at) <= ttl_secs {
            continue;
        }
        let socket_id = key.trim_start_matches("realtime:socket:");
        let rooms = load_socket_rooms(socket_id).await;
        for room in &rooms {
            update_room_membership(room, socket_id, false).await;
        }
        {
            let state = realtime_state();
            let mut guard = state.write().await;
            for room in rooms {
                if let Some(room_set) = guard.rooms.get_mut(&room) {
                    room_set.remove(socket_id);
                    if room_set.is_empty() {
                        guard.rooms.remove(&room);
                    }
                }
            }
            guard.inbox.remove(socket_id);
        }
        delete_socket_records(socket_id).await;
    }
}

async fn deliver_local(room: &str, message: Value) {
    let state = realtime_state();
    deliver_local_with(&state, room, message).await;
}

async fn deliver_local_with(state: &RealtimeStateHandleArc, room: &str, message: Value) {
    let mut guard = state.write().await;
    let sockets: Vec<String> = guard
        .rooms
        .get(room)
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default();
    for socket in sockets {
        guard.inbox.entry(socket).or_default().push(message);
        unsafe { wr_rc_inc(message) };
    }
    unsafe { wr_rc_dec(message) };
}

async fn deliver_socket(socket_id: &str, message: Value) {
    let state = realtime_state();
    let mut guard = state.write().await;
    guard.inbox.entry(socket_id.to_string()).or_default().push(message);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FanoutRequest {
    pub room: String,
    pub message: JsonValue,
}

#[allow(dead_code)]
pub async fn deliver_fanout(req: FanoutRequest) {
    let msg = json_to_value(&req.message, 0);
    unsafe { wr_rc_inc(msg) };
    deliver_local(&req.room, msg).await;
}

pub async fn deliver_fanout_with(state: RealtimeStateHandleArc, req: FanoutRequest) {
    let msg = json_to_value(&req.message, 0);
    unsafe { wr_rc_inc(msg) };
    deliver_local_with(&state, &req.room, msg).await;
}

async fn ensure_pubsub_registered() {
    if PUBSUB_REGISTERED.set(()).is_err() {
        return;
    }
    pubsub::subscribe("realtime:fanout", |payload| async move {
        if let Ok(req) = serde_json::from_value::<FanoutRequest>(payload) {
            deliver_fanout(req).await;
        }
    })
    .await;
}

pub fn realtime_on_connect(handler: Value) -> Value {
    let (pending, state) = pending_new();
    runtime_spawn(async move {
        let realtime_state_ref = realtime_state();
        let mut guard = realtime_state_ref.write().await;
        if let Some(old) = guard.on_connect.take() {
            unsafe { wr_rc_dec(old) };
        }
        unsafe { wr_rc_inc(handler) };
        guard.on_connect = Some(handler);
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}

pub fn realtime_join(socket_id: Value, room: Value) -> Value {
    let (pending, state) = pending_new();
    let socket_id = match value_to_string(socket_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let room = match value_to_string(room) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        ensure_cleanup_started();
        let realtime_state_ref = realtime_state();
        let mut guard = realtime_state_ref.write().await;
        let room_key = room.clone();
        let socket_key = socket_id.clone();
        guard.rooms.entry(room).or_default().insert(socket_id);
        update_room_membership(&room_key, &socket_key, true).await;
        let has_rooms = update_socket_rooms(&socket_key, &room_key, true).await;
        store_socket_record(&socket_key).await;
        if !has_rooms {
            delete_socket_records(&socket_key).await;
        }
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}

pub fn realtime_leave(socket_id: Value, room: Value) -> Value {
    let (pending, state) = pending_new();
    let socket_id = match value_to_string(socket_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let room = match value_to_string(room) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        ensure_cleanup_started();
        let realtime_state_ref = realtime_state();
        let mut guard = realtime_state_ref.write().await;
        if let Some(room_set) = guard.rooms.get_mut(&room) {
            room_set.remove(&socket_id);
            if room_set.is_empty() {
                guard.rooms.remove(&room);
            }
        }
        update_room_membership(&room, &socket_id, false).await;
        let has_rooms = update_socket_rooms(&socket_id, &room, false).await;
        if !has_rooms {
            guard.inbox.remove(&socket_id);
            delete_socket_records(&socket_id).await;
        }
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}

pub fn realtime_broadcast(room: Value, message: Value) -> Value {
    let (pending, state) = pending_new();
    let room = match value_to_string(room) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    unsafe { wr_rc_inc(message) };
    runtime_spawn(async move {
        ensure_pubsub_registered().await;
        let req = FanoutRequest {
            room: room.clone(),
            message: value_to_json(message, 0),
        };
        unsafe { wr_rc_dec(message) };
        let payload = serde_json::to_value(req).unwrap_or(JsonValue::Null);
        pubsub::publish("realtime:fanout", payload).await;
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}

pub fn realtime_send(socket_id: Value, message: Value) -> Value {
    let (pending, state) = pending_new();
    let socket_id = match value_to_string(socket_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    unsafe { wr_rc_inc(message) };
    runtime_spawn(async move {
        deliver_socket(&socket_id, message).await;
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}
