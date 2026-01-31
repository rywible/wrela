use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::string;
use crate::value::Value;
use crate::{wr_rc_dec, wr_rc_inc};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

struct RealtimeState {
    rooms: HashMap<String, HashSet<String>>,
    inbox: HashMap<String, Vec<Value>>,
    on_connect: Option<Value>,
}

static STATE: OnceLock<Mutex<RealtimeState>> = OnceLock::new();

fn realtime_state() -> &'static Mutex<RealtimeState> {
    STATE.get_or_init(|| {
        Mutex::new(RealtimeState {
            rooms: HashMap::new(),
            inbox: HashMap::new(),
            on_connect: None,
        })
    })
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub fn realtime_on_connect(handler: Value) -> Value {
    let (pending, state) = pending_new();
    runtime_spawn(async move {
        let mut guard = realtime_state().lock().expect("realtime state lock");
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
        let mut guard = realtime_state().lock().expect("realtime state lock");
        guard.rooms.entry(room).or_default().insert(socket_id);
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
        let mut guard = realtime_state().lock().expect("realtime state lock");
        if let Some(room_set) = guard.rooms.get_mut(&room) {
            room_set.remove(&socket_id);
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
        let mut guard = realtime_state().lock().expect("realtime state lock");
        let sockets: Vec<String> = guard
            .rooms
            .get(&room)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        for socket in sockets {
            guard.inbox.entry(socket).or_default().push(message);
            unsafe { wr_rc_inc(message) };
        }
        unsafe { wr_rc_dec(message) };
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
        let mut guard = realtime_state().lock().expect("realtime state lock");
        guard.inbox.entry(socket_id).or_default().push(message);
        resolve_pending(state, Value::from_bool(true));
    });
    pending
}
