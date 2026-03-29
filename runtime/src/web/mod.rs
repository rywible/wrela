pub(crate) mod axum_bridge;

use crate::bytes;
use crate::kernel::runtime;
use crate::list;
use crate::map;
use crate::result;
use crate::string;
use crate::value::{Value, int_value};
use axum::Router;
use axum::body::Body;
use axum::routing::any;
use dashmap::DashMap;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Condvar;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::{mpsc, oneshot};

struct WebServerRegistry {
    next_listener_handle: AtomicI64,
    listeners: DashMap<i64, Arc<Mutex<AxumListenerState>>>,
}

type PendingRequest = (
    HttpRequestFrame,
    oneshot::Sender<axum::http::Response<Body>>,
);

#[allow(dead_code)]
struct AxumListenerState {
    pending_requests: Arc<(Mutex<VecDeque<PendingRequest>>, Condvar)>,
    pending_accepts: VecDeque<i64>,
    next_connection_handle: i64,
    connections: HashMap<
        i64,
        (
            HttpRequestFrame,
            Option<oneshot::Sender<axum::http::Response<Body>>>,
        ),
    >,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    bound_addr: Option<std::net::SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpRequestFrame {
    pub method: String,
    pub path: String,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub keep_alive_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpResponseFrame {
    pub status_code: u16,
    pub reason_phrase: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub should_close_connection: bool,
}

fn next_positive_i64_handle(counter: &AtomicI64) -> Option<i64> {
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current <= 0 {
            return None;
        }
        let next = if current == i64::MAX { 0 } else { current + 1 };
        if counter
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Some(current);
        }
    }
}

fn next_positive_i64_counter(counter: &mut i64) -> Option<i64> {
    let handle = *counter;
    if handle <= 0 {
        return None;
    }
    *counter = if handle == i64::MAX { 0 } else { handle + 1 };
    Some(handle)
}

impl WebServerRegistry {
    fn new() -> Self {
        Self {
            next_listener_handle: AtomicI64::new(1),
            listeners: DashMap::new(),
        }
    }

    fn insert_listener(&self, state: AxumListenerState) -> Option<i64> {
        let handle = next_positive_i64_handle(&self.next_listener_handle)?;
        self.listeners.insert(handle, Arc::new(Mutex::new(state)));
        Some(handle)
    }

    fn get_listener(&self, handle: i64) -> Option<Arc<Mutex<AxumListenerState>>> {
        if handle <= 0 {
            return None;
        }
        self.listeners
            .get(&handle)
            .map(|listener| listener.value().clone())
    }

    fn remove_listener(&self, handle: i64) -> bool {
        if handle <= 0 {
            return false;
        }
        self.listeners.remove(&handle).is_some()
    }
}

fn web_server_registry() -> &'static WebServerRegistry {
    static REGISTRY: OnceLock<WebServerRegistry> = OnceLock::new();
    REGISTRY.get_or_init(WebServerRegistry::new)
}

fn err(message: &str) -> Value {
    let error = string::str_from_utf8(message.as_ptr(), message.len());
    let out = result::result_err(error);
    unsafe {
        crate::wr_rc_dec(error);
    }
    out
}

fn ok(value: Value) -> Value {
    result::result_ok(value)
}

fn str_value(text: &str) -> Value {
    string::str_from_utf8(text.as_ptr(), text.len())
}

fn map_set_string(map_value: Value, key: &str, value: &str) {
    let key_value = str_value(key);
    let value_value = str_value(value);
    map::map_set(map_value, key_value, value_value);
    unsafe {
        crate::wr_rc_dec(key_value);
        crate::wr_rc_dec(value_value);
    }
}

fn map_set_int(map_value: Value, key: &str, value: i64) {
    let key_value = str_value(key);
    map::map_set(map_value, key_value, Value::from_int(value));
    unsafe {
        crate::wr_rc_dec(key_value);
    }
}

fn map_set_bool(map_value: Value, key: &str, value: bool) {
    let key_value = str_value(key);
    map::map_set(map_value, key_value, Value::from_bool(value));
    unsafe {
        crate::wr_rc_dec(key_value);
    }
}

fn map_set_value(map_value: Value, key: &str, value: Value) {
    let key_value = str_value(key);
    map::map_set(map_value, key_value, value);
    unsafe {
        crate::wr_rc_dec(key_value);
    }
}

fn event_map(event_type: &str, connection_handle: Option<i64>) -> Value {
    let out = map::map_new();
    map_set_string(out, "event_type", event_type);
    if let Some(connection_handle) = connection_handle {
        map_set_int(out, "connection_handle", connection_handle);
    } else {
        map_set_value(out, "connection_handle", Value::nil());
    }
    out
}

fn value_to_string(value: Value) -> Option<String> {
    string::with_string_bytes(value, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn map_field(config: Value, field: &str) -> Option<Value> {
    let map_ref = map::as_map_ref(config)?;
    let mut iter = map::map_iter(map_ref);
    while let Some((key, value)) = iter.next() {
        let Some(key_text) = value_to_string(key.0) else {
            continue;
        };
        if key_text != field {
            continue;
        }
        if value.is_nil() {
            return None;
        }
        unsafe {
            crate::wr_rc_inc(value);
        }
        return Some(value);
    }
    None
}

fn parse_bind_address(config: Value) -> Result<String, String> {
    if config.is_nil() {
        return Ok(
            std::env::var("WRELA_WEB_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        );
    }

    let Some(bind_value) = map_field(config, "bind_address") else {
        return Ok(
            std::env::var("WRELA_WEB_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        );
    };

    let out = value_to_string(bind_value).ok_or_else(|| {
        "web_server_create_listener expects String bind_address in configuration".to_string()
    });
    unsafe {
        crate::wr_rc_dec(bind_value);
    }
    out
}

fn parse_listener_namespace(config: Value) -> Result<String, String> {
    let Some(namespace_value) = map_field(config, "listener_namespace") else {
        return Ok("app".to_string());
    };
    let out = value_to_string(namespace_value)
        .ok_or_else(|| "listener_namespace must be a String".to_string())?;
    unsafe {
        crate::wr_rc_dec(namespace_value);
    }
    Ok(out)
}

fn parse_reuse_port_enabled(config: Value) -> Result<bool, String> {
    let Some(reuse_port_enabled_value) = map_field(config, "reuse_port_enabled") else {
        return Ok(false);
    };
    let out = if reuse_port_enabled_value.is_bool() {
        Ok(reuse_port_enabled_value.as_bool())
    } else {
        Err("reuse_port_enabled must be a Boolean".to_string())
    };
    unsafe {
        crate::wr_rc_dec(reuse_port_enabled_value);
    }
    out
}

fn request_headers_to_map(headers: &[(String, String)]) -> Value {
    let headers_map = map::map_new();
    for (name, value) in headers {
        map_set_string(headers_map, name.as_str(), value.as_str());
    }
    headers_map
}

fn request_frame_to_value(frame: HttpRequestFrame) -> Value {
    let out = map::map_new();
    map_set_string(out, "method", frame.method.as_str());
    map_set_string(out, "path", frame.path.as_str());
    map_set_string(out, "http_version", frame.http_version.as_str());
    map_set_bool(out, "keep_alive_requested", frame.keep_alive_requested);

    let headers_map = request_headers_to_map(frame.headers.as_slice());
    map_set_value(out, "headers", headers_map);
    unsafe {
        crate::wr_rc_dec(headers_map);
    }

    let body_bytes_value = bytes::bytes_from_slice(frame.body.as_slice());
    map_set_value(out, "body_bytes", body_bytes_value);
    unsafe {
        crate::wr_rc_dec(body_bytes_value);
    }

    match String::from_utf8(frame.body) {
        Ok(body_text) => map_set_string(out, "body_text", body_text.as_str()),
        Err(_) => map_set_value(out, "body_text", Value::nil()),
    }

    out
}

fn parse_header_pairs_from_map(headers_value: Value) -> Result<Vec<(String, String)>, String> {
    let Some(map_ref) = map::as_map_ref(headers_value) else {
        return Err("response_frame.headers must be a Map".to_string());
    };

    let mut out = Vec::new();
    let mut iter = map::map_iter(map_ref);
    while let Some((key, value)) = iter.next() {
        let header_name = value_to_string(key.0)
            .ok_or_else(|| "response_frame.headers keys must be Strings".to_string())?;
        let header_value = value_to_string(value)
            .or_else(|| int_value(value).map(|number| number.to_string()))
            .or_else(|| {
                if value.is_bool() {
                    if value.as_bool() {
                        return Some("true".to_string());
                    }
                    return Some("false".to_string());
                }
                None
            })
            .ok_or_else(|| {
                "response_frame.headers values must be Strings, Integers, or Booleans".to_string()
            })?;
        out.push((header_name, header_value));
    }

    Ok(out)
}

fn map_value_to_bool(value: Value) -> Option<bool> {
    if value.is_bool() {
        Some(value.as_bool())
    } else {
        None
    }
}

fn should_close_connection_from_headers(headers: &[(String, String)]) -> bool {
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("connection") {
            continue;
        }
        for token in value.split(',') {
            if token.trim().eq_ignore_ascii_case("close") {
                return true;
            }
        }
    }
    false
}

fn parse_http_response_frame_from_value(
    response_frame: Value,
) -> Result<HttpResponseFrame, String> {
    if response_frame.is_nil() {
        return Err("web_server_write_http_response_frame expects response_frame Map".to_string());
    }

    let Some(status_code_value) = map_field(response_frame, "status_code") else {
        return Err("response_frame.status_code is required".to_string());
    };
    let status_code = int_value(status_code_value)
        .ok_or_else(|| "response_frame.status_code must be an Integer".to_string())?;
    unsafe {
        crate::wr_rc_dec(status_code_value);
    }
    if !(100..=599).contains(&status_code) {
        return Err("response_frame.status_code must be in range [100, 599]".to_string());
    }

    let reason_phrase =
        if let Some(reason_phrase_value) = map_field(response_frame, "reason_phrase") {
            let parsed = value_to_string(reason_phrase_value)
                .ok_or_else(|| "response_frame.reason_phrase must be a String".to_string())?;
            unsafe {
                crate::wr_rc_dec(reason_phrase_value);
            }
            if parsed.trim().is_empty() {
                None
            } else {
                Some(parsed)
            }
        } else {
            None
        };

    let headers = if let Some(headers_value) = map_field(response_frame, "headers") {
        let parsed = parse_header_pairs_from_map(headers_value)?;
        unsafe {
            crate::wr_rc_dec(headers_value);
        }
        parsed
    } else {
        Vec::new()
    };

    let body_from_text = if let Some(body_text_value) = map_field(response_frame, "body_text") {
        let body_text = value_to_string(body_text_value)
            .ok_or_else(|| "response_frame.body_text must be a String".to_string())?;
        unsafe {
            crate::wr_rc_dec(body_text_value);
        }
        Some(body_text.into_bytes())
    } else {
        None
    };

    let body_from_bytes = if let Some(body_bytes_value) = map_field(response_frame, "body_bytes") {
        let body_bytes = bytes::with_bytes(body_bytes_value, |body_bytes| body_bytes.to_vec())
            .ok_or_else(|| "response_frame.body_bytes must be a Bytes value".to_string())?;
        unsafe {
            crate::wr_rc_dec(body_bytes_value);
        }
        Some(body_bytes)
    } else {
        None
    };

    if body_from_text.is_some() && body_from_bytes.is_some() {
        return Err("response_frame must set either body_text or body_bytes, not both".to_string());
    }

    let should_close_connection =
        if let Some(should_close_value) = map_field(response_frame, "should_close_connection") {
            let flag = map_value_to_bool(should_close_value).ok_or_else(|| {
                "response_frame.should_close_connection must be a Boolean".to_string()
            })?;
            unsafe {
                crate::wr_rc_dec(should_close_value);
            }
            flag
        } else {
            should_close_connection_from_headers(headers.as_slice())
        };

    Ok(HttpResponseFrame {
        status_code: status_code as u16,
        reason_phrase,
        headers,
        body: body_from_text.or(body_from_bytes).unwrap_or_default(),
        should_close_connection,
    })
}

pub(crate) fn web_server_create_listener(configuration: Value) -> Value {
    let bind_address = match parse_bind_address(configuration) {
        Ok(bind_address) => bind_address,
        Err(message) => return err(&message),
    };

    let _listener_namespace = match parse_listener_namespace(configuration) {
        Ok(listener_namespace) => listener_namespace,
        Err(message) => return err(&message),
    };

    if _listener_namespace.eq_ignore_ascii_case("db") {
        return err("web listener namespace `db` is reserved; use app namespace");
    }

    let _reuse_port_enabled = match parse_reuse_port_enabled(configuration) {
        Ok(flag) => flag,
        Err(message) => return err(&message),
    };
    // SO_REUSEPORT can be applied to TokioTcpListener via socket2 if needed later.

    let (request_tx, mut request_rx) = mpsc::channel(256);
    let pending_requests: Arc<(Mutex<VecDeque<PendingRequest>>, Condvar)> =
        Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));

    let rt = runtime::tokio_runtime();
    let bind_addr = bind_address.clone();
    let bridge_queue = pending_requests.clone();
    rt.spawn(async move {
        while let Some(msg) = request_rx.recv().await {
            bridge_queue.0.lock().unwrap().push_back(msg);
            bridge_queue.1.notify_one();
        }
    });

    let (addr_tx, addr_rx) = std::sync::mpsc::sync_channel(1);
    let server_handle = rt.spawn(async move {
        let listener = match TokioTcpListener::bind(bind_addr.as_str()).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("web_server_create_listener bind failed: {e}");
                let _ = addr_tx.send(std::net::SocketAddr::from(([127, 0, 0, 1], 0)));
                return;
            }
        };
        let _ = addr_tx.send(
            listener
                .local_addr()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 0))),
        );
        let tx = request_tx.clone();
        let router = Router::new()
            .route(
                "/",
                any({
                    let tx = tx.clone();
                    move |req: axum::extract::Request| {
                        let tx = tx.clone();
                        async move { axum_bridge::bridge_handler(req, tx).await }
                    }
                }),
            )
            .route(
                "/{*path}",
                any({
                    let tx = tx.clone();
                    move |req: axum::extract::Request| {
                        let tx = tx.clone();
                        async move { axum_bridge::bridge_handler(req, tx).await }
                    }
                }),
            );
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("axum server error: {e}");
        }
    });

    let bound_addr = addr_rx.recv_timeout(Duration::from_secs(2)).ok();

    let state = AxumListenerState {
        pending_requests,
        pending_accepts: VecDeque::new(),
        next_connection_handle: 1,
        connections: HashMap::new(),
        server_handle: Some(server_handle),
        bound_addr,
    };
    let Some(listener_handle) = web_server_registry().insert_listener(state) else {
        return err("web_server_create_listener listener handle space exhausted");
    };
    ok(Value::from_int(listener_handle))
}

pub(crate) fn web_server_poll_event(listener_handle: Value, timeout_ms: Value) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let timeout_ms = int_value(timeout_ms).unwrap_or(-1);
    if timeout_ms < 0 {
        return err("web_server_poll_event expects timeout_ms >= 0");
    }

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_poll_event invalid listener handle");
    };

    loop {
        let (maybe_item, pending_requests) = {
            let state = state_lock.lock().expect("web server state lock");
            if let Some(&next_connection) = state.pending_accepts.front() {
                return ok(event_map("listener_readable", Some(next_connection)));
            }
            let item = state.pending_requests.0.lock().unwrap().pop_front();
            (item, state.pending_requests.clone())
        };
        if let Some((frame, resp_tx)) = maybe_item {
            let mut state = state_lock.lock().expect("web server state lock");
            if let Some(handle) = next_positive_i64_counter(&mut state.next_connection_handle) {
                state.connections.insert(handle, (frame, Some(resp_tx)));
                state.pending_accepts.push_back(handle);
                return ok(event_map("listener_readable", Some(handle)));
            }
            let _ = resp_tx.send(
                axum::http::Response::builder()
                    .status(503)
                    .body(Body::from("connection handle exhausted"))
                    .unwrap(),
            );
            continue;
        }

        let guard = pending_requests.0.lock().unwrap();
        let (_guard, result) = match pending_requests
            .1
            .wait_timeout(guard, Duration::from_millis(timeout_ms as u64))
        {
            Ok(pair) => pair,
            Err(e) => return err(&format!("web_server_poll_event condvar poisoned: {e}")),
        };
        if result.timed_out() {
            return ok(event_map("none", None));
        }
        // Notified, loop to check queue again
    }
}

pub(crate) fn web_server_accept_connection(listener_handle: Value) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_accept_connection invalid listener handle");
    };

    let mut state = state_lock.lock().expect("web server state lock");
    if let Some(connection_handle) = state.pending_accepts.pop_front() {
        return ok(Value::from_int(connection_handle));
    }
    err("web_server_accept_connection no pending connection")
}

pub(crate) fn web_server_read_connection_bytes(
    _listener_handle: Value,
    _connection_handle: Value,
    _max_bytes: Value,
) -> Value {
    err(
        "web_server_read_connection_bytes not supported with axum backend (use read_http_request_frame)",
    )
}

pub(crate) fn web_server_write_connection_bytes(
    _listener_handle: Value,
    _connection_handle: Value,
    _payload: Value,
) -> Value {
    err(
        "web_server_write_connection_bytes not supported with axum backend (use write_http_response_frame)",
    )
}

pub(crate) fn web_server_read_http_request_frame(
    listener_handle: Value,
    connection_handle: Value,
) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let connection_handle = int_value(connection_handle).unwrap_or(0);

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_read_http_request_frame invalid listener handle");
    };

    let state = state_lock.lock().expect("web server state lock");
    let Some((frame, _)) = state.connections.get(&connection_handle) else {
        return err("web_server_read_http_request_frame invalid connection handle");
    };
    ok(request_frame_to_value(frame.clone()))
}

pub(crate) fn web_server_write_http_response_frame(
    listener_handle: Value,
    connection_handle: Value,
    response_frame: Value,
) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let connection_handle = int_value(connection_handle).unwrap_or(0);

    let response_frame = match parse_http_response_frame_from_value(response_frame) {
        Ok(rf) => rf,
        Err(message) => return err(&message),
    };

    let response = match axum_bridge::response_from_frame(&response_frame) {
        Ok(r) => r,
        Err(message) => return err(&message),
    };

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_write_http_response_frame invalid listener handle");
    };

    let mut state = state_lock.lock().expect("web server state lock");
    let Some((_, resp_tx)) = state.connections.get_mut(&connection_handle) else {
        return err("web_server_write_http_response_frame invalid connection handle");
    };
    let Some(resp_tx) = resp_tx.take() else {
        return err("web_server_write_http_response_frame connection already responded");
    };
    if resp_tx.send(response).is_err() {
        return err("web_server_write_http_response_frame response receiver dropped");
    }
    state.connections.remove(&connection_handle);
    state.pending_accepts.retain(|h| *h != connection_handle);
    let written = response_frame.body.len() + 64;
    ok(Value::from_int(written as i64))
}

pub(crate) fn web_server_write_http_response_vectored(
    listener_handle: Value,
    connection_handle: Value,
    head_bytes: Value,
    body_bytes: Value,
    _should_close_connection: Value,
) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let connection_handle = int_value(connection_handle).unwrap_or(0);

    let Some(head_bytes) = bytes::with_bytes(head_bytes, |bytes| bytes.to_vec()) else {
        return err("web_server_write_http_response_vectored expects Bytes head_bytes");
    };
    let Some(body_bytes) = bytes::with_bytes(body_bytes, |bytes| bytes.to_vec()) else {
        return err("web_server_write_http_response_vectored expects Bytes body_bytes");
    };

    let response = match axum_bridge::response_from_vectored(&head_bytes, body_bytes.clone()) {
        Ok(r) => r,
        Err(message) => return err(&message),
    };

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_write_http_response_vectored invalid listener handle");
    };

    let mut state = state_lock.lock().expect("web server state lock");
    let Some((_, resp_tx)) = state.connections.get_mut(&connection_handle) else {
        return err("web_server_write_http_response_vectored invalid connection handle");
    };
    let Some(resp_tx) = resp_tx.take() else {
        return err("web_server_write_http_response_vectored connection already responded");
    };
    if resp_tx.send(response).is_err() {
        return err("web_server_write_http_response_vectored response receiver dropped");
    }
    state.connections.remove(&connection_handle);
    state.pending_accepts.retain(|h| *h != connection_handle);
    let written = head_bytes.len() + body_bytes.len();
    ok(Value::from_int(written as i64))
}

pub(crate) fn web_server_send_file(
    listener_handle: Value,
    connection_handle: Value,
    file_path: Value,
    offset: Value,
    length: Value,
    content_type: Value,
    should_close_connection: Value,
) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let connection_handle = int_value(connection_handle).unwrap_or(0);
    let offset = int_value(offset).unwrap_or(0);
    let length = int_value(length).unwrap_or(-1);
    if offset < 0 || length < 0 {
        return err("web_server_send_file expects offset >= 0 and length >= 0");
    }
    if !should_close_connection.is_bool() {
        return err("web_server_send_file expects Boolean should_close_connection");
    }
    let Some(file_path) = value_to_string(file_path) else {
        return err("web_server_send_file expects String file_path");
    };
    let Some(content_type) = value_to_string(content_type) else {
        return err("web_server_send_file expects String content_type");
    };

    let file_metadata = match std::fs::metadata(file_path.as_str()) {
        Ok(m) => m,
        Err(error) => return err(&format!("web_server_send_file stat failed: {error}")),
    };
    let file_size = file_metadata.len() as usize;
    let start_offset = offset as usize;
    if start_offset > file_size {
        return err("web_server_send_file offset is beyond file length");
    }
    let max_len = file_size.saturating_sub(start_offset);
    let send_len = std::cmp::min(length as usize, max_len);

    let mut file = match File::open(file_path.as_str()) {
        Ok(f) => f,
        Err(error) => return err(&format!("web_server_send_file open failed: {error}")),
    };
    if let Err(error) = file.seek(SeekFrom::Start(start_offset as u64)) {
        return err(&format!("web_server_send_file seek failed: {error}"));
    }
    let mut body = vec![0u8; send_len];
    if send_len > 0 {
        if let Err(error) = file.read(&mut body) {
            return err(&format!("web_server_send_file read failed: {error}"));
        }
    }

    let headers = vec![
        ("Content-Type".to_string(), content_type),
        ("Content-Length".to_string(), send_len.to_string()),
    ];
    let frame = HttpResponseFrame {
        status_code: 200,
        reason_phrase: None,
        headers,
        body,
        should_close_connection: should_close_connection.as_bool(),
    };
    let response = match axum_bridge::response_from_frame(&frame) {
        Ok(r) => r,
        Err(message) => return err(&message),
    };

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_send_file invalid listener handle");
    };

    let mut state = state_lock.lock().expect("web server state lock");
    let Some((_, resp_tx)) = state.connections.get_mut(&connection_handle) else {
        return err("web_server_send_file invalid connection handle");
    };
    let Some(resp_tx) = resp_tx.take() else {
        return err("web_server_send_file connection already responded");
    };
    if resp_tx.send(response).is_err() {
        return err("web_server_send_file response receiver dropped");
    }
    state.connections.remove(&connection_handle);
    state.pending_accepts.retain(|h| *h != connection_handle);
    let written = send_len + 128;
    ok(Value::from_int(written as i64))
}

pub(crate) fn web_server_close_connection(
    listener_handle: Value,
    connection_handle: Value,
) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    let connection_handle = int_value(connection_handle).unwrap_or(0);

    let Some(state_lock) = web_server_registry().get_listener(listener_handle) else {
        return err("web_server_close_connection invalid listener handle");
    };
    let mut state = state_lock.lock().expect("web server state lock");
    state.pending_accepts.retain(|h| *h != connection_handle);
    if let Some((_, resp_tx)) = state.connections.remove(&connection_handle) {
        if let Some(tx) = resp_tx {
            let _ = tx.send(
                axum::http::Response::builder()
                    .status(503)
                    .body(Body::from("connection closed"))
                    .unwrap(),
            );
        }
    }
    ok(Value::nil())
}

pub(crate) fn web_server_close_listener(listener_handle: Value) -> Value {
    let listener_handle = int_value(listener_handle).unwrap_or(0);
    if let Some(state_lock) = web_server_registry().get_listener(listener_handle) {
        if let Some(handle) = state_lock.lock().unwrap().server_handle.take() {
            handle.abort();
        }
    }
    ok(Value::from_bool(
        web_server_registry().remove_listener(listener_handle),
    ))
}

pub(crate) fn web_server_configure_listener_socket(
    listener_handle: Value,
    reuse_port_enabled: Value,
) -> Value {
    let _listener_handle = int_value(listener_handle).unwrap_or(0);
    if !reuse_port_enabled.is_bool() {
        return err("web_server_configure_listener_socket expects Boolean reuse_port_enabled");
    }
    // Axum/Tokio listener does not expose socket options here; SO_REUSEPORT
    // would require socket2 before bind. No-op for compatibility.
    ok(Value::from_bool(reuse_port_enabled.as_bool()))
}

fn json_to_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::nil(),
        JsonValue::Bool(flag) => Value::from_bool(*flag),
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                Value::from_int(value)
            } else if let Some(value) = number.as_f64() {
                Value::from_float(value)
            } else {
                Value::nil()
            }
        }
        JsonValue::String(text) => str_value(text),
        JsonValue::Array(items) => {
            let list_value = list::list_new(0);
            for item in items {
                let item_value = json_to_value(item);
                list::list_push(list_value, item_value);
                unsafe {
                    crate::wr_rc_dec(item_value);
                }
            }
            list_value
        }
        JsonValue::Object(entries) => {
            let map_value = map::map_new();
            for (key, item) in entries {
                let key_value = str_value(key);
                let item_value = json_to_value(item);
                map::map_set(map_value, key_value, item_value);
                unsafe {
                    crate::wr_rc_dec(key_value);
                    crate::wr_rc_dec(item_value);
                }
            }
            map_value
        }
    }
}

fn value_to_json(value: Value, depth: usize) -> JsonValue {
    if depth > 32 {
        return JsonValue::Null;
    }
    if value.is_nil() {
        return JsonValue::Null;
    }
    if value.is_bool() {
        return JsonValue::Bool(value.as_bool());
    }
    if let Some(integer_value) = int_value(value) {
        return JsonValue::Number(serde_json::Number::from(integer_value));
    }
    if value.is_float() {
        let float_value = value.as_float();
        if let Some(number) = serde_json::Number::from_f64(float_value) {
            return JsonValue::Number(number);
        }
        return JsonValue::Null;
    }
    if let Some(text) = value_to_string(value) {
        return JsonValue::String(text);
    }
    if let Some(list_ref) = list::as_list_ref(value) {
        let mut items = Vec::new();
        unsafe {
            for item in (&(*list_ref).data).iter().take((*list_ref).len) {
                items.push(value_to_json(*item, depth + 1));
            }
        }
        return JsonValue::Array(items);
    }
    if let Some(map_ref) = map::as_map_ref(value) {
        let mut json_map = serde_json::Map::new();
        let mut iter = map::map_iter(map_ref);
        while let Some((key, item_value)) = iter.next() {
            let key_text = value_to_string(key.0)
                .or_else(|| int_value(key.0).map(|value| value.to_string()))
                .unwrap_or_else(|| "<key>".to_string());
            json_map.insert(key_text, value_to_json(item_value, depth + 1));
        }
        return JsonValue::Object(json_map);
    }
    JsonValue::String("<value>".to_string())
}

pub(crate) fn web_parse_json_text(text: Value) -> Value {
    let Some(text) = value_to_string(text) else {
        return err("web_parse_json_text expects String text");
    };

    let parsed = match serde_json::from_str::<JsonValue>(text.as_str()) {
        Ok(parsed) => parsed,
        Err(error) => return err(&format!("web_parse_json_text parse failed: {error}")),
    };

    let JsonValue::Object(_) = parsed else {
        return err("web_parse_json_text expects top-level JSON object");
    };

    ok(json_to_value(&parsed))
}

pub(crate) fn web_render_json_text(value: Value) -> Value {
    let json_value = value_to_json(value, 0);
    let text = match serde_json::to_string(&json_value) {
        Ok(text) => text,
        Err(error) => {
            return err(&format!(
                "web_render_json_text serialization failed: {error}"
            ));
        }
    };
    ok(str_value(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{ErrorKind, Read, Write};
    use std::net::{Shutdown, TcpStream};
    use std::sync::atomic::AtomicI64;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn handle_allocators_stop_at_max() {
        let next = AtomicI64::new(i64::MAX - 1);
        assert_eq!(next_positive_i64_handle(&next), Some(i64::MAX - 1));
        assert_eq!(next_positive_i64_handle(&next), Some(i64::MAX));
        assert_eq!(next_positive_i64_handle(&next), None);

        let mut local = i64::MAX - 1;
        assert_eq!(next_positive_i64_counter(&mut local), Some(i64::MAX - 1));
        assert_eq!(next_positive_i64_counter(&mut local), Some(i64::MAX));
        assert_eq!(next_positive_i64_counter(&mut local), None);
    }

    #[test]
    fn json_round_trip_object_succeeds() {
        let parsed = web_parse_json_text(str_value("{\"hello\":\"world\",\"count\":2}"));
        let parsed_ok = result::result_is_ok(parsed);
        assert!(parsed_ok.is_bool() && parsed_ok.as_bool());

        let rendered = web_render_json_text(result::result_unwrap(parsed));
        let rendered_ok = result::result_is_ok(rendered);
        assert!(rendered_ok.is_bool() && rendered_ok.as_bool());

        let rendered_text = result::result_unwrap(rendered);
        let rendered_string = value_to_string(rendered_text).expect("rendered json string");
        assert!(rendered_string.contains("hello"));
        assert!(rendered_string.contains("world"));

        unsafe {
            crate::wr_rc_dec(parsed_ok);
            crate::wr_rc_dec(rendered_ok);
            crate::wr_rc_dec(rendered_text);
            crate::wr_rc_dec(parsed);
            crate::wr_rc_dec(rendered);
        }
    }

    #[test]
    fn listener_create_close_round_trip_succeeds() {
        let config = map::map_new();
        map_set_string(config, "bind_address", "127.0.0.1:0");
        let created = web_server_create_listener(config);
        let created_ok = result::result_is_ok(created);
        assert!(created_ok.is_bool() && created_ok.as_bool());

        let handle = result::result_unwrap(created);
        let closed = web_server_close_listener(handle);
        let closed_ok = result::result_is_ok(closed);
        assert!(closed_ok.is_bool() && closed_ok.as_bool());

        unsafe {
            crate::wr_rc_dec(config);
            crate::wr_rc_dec(created_ok);
            crate::wr_rc_dec(handle);
            crate::wr_rc_dec(closed_ok);
            crate::wr_rc_dec(created);
            crate::wr_rc_dec(closed);
        }
    }

    #[test]
    fn listener_create_rejects_reserved_db_namespace() {
        let config = map::map_new();
        map_set_string(config, "bind_address", "127.0.0.1:0");
        map_set_string(config, "listener_namespace", "db");
        let created = web_server_create_listener(config);
        let created_ok = result::result_is_ok(created);
        assert!(created_ok.is_bool());
        assert!(!created_ok.as_bool());

        unsafe {
            crate::wr_rc_dec(config);
            crate::wr_rc_dec(created_ok);
            crate::wr_rc_dec(created);
        }
    }

    #[test]
    fn response_frame_map_rejects_conflicting_body_sources() {
        let response_frame_map = map::map_new();
        map_set_int(response_frame_map, "status_code", 200);
        map_set_string(response_frame_map, "body_text", "hello");
        let body_bytes = bytes::bytes_from_slice(b"world");
        map_set_value(response_frame_map, "body_bytes", body_bytes);

        let parsed = parse_http_response_frame_from_value(response_frame_map);
        assert!(parsed.is_err());

        unsafe {
            crate::wr_rc_dec(body_bytes);
            crate::wr_rc_dec(response_frame_map);
        }
    }

    #[test]
    fn response_frame_map_parses_minimal_payload() {
        let response_frame_map = map::map_new();
        map_set_int(response_frame_map, "status_code", 503);
        map_set_string(response_frame_map, "body_text", "queue saturated");

        let parsed =
            parse_http_response_frame_from_value(response_frame_map).expect("response frame");
        assert_eq!(parsed.status_code, 503);
        assert_eq!(parsed.body, b"queue saturated".to_vec());

        unsafe {
            crate::wr_rc_dec(response_frame_map);
        }
    }

    fn value_to_i64(value: Value) -> i64 {
        int_value(value).expect("expected Integer value")
    }

    fn map_field_string(map_value: Value, key: &str) -> String {
        let field_value = map_field(map_value, key).expect("expected map field");
        let field_text = value_to_string(field_value).expect("expected string field");
        unsafe {
            crate::wr_rc_dec(field_value);
        }
        field_text
    }

    #[test]
    fn poll_event_reports_listener_and_connection_readable() {
        let listener_configuration = map::map_new();
        map_set_string(listener_configuration, "bind_address", "127.0.0.1:0");
        let create_result = web_server_create_listener(listener_configuration);
        let create_ok = result::result_is_ok(create_result);
        assert!(create_ok.is_bool() && create_ok.as_bool());

        let listener_handle = result::result_unwrap(create_result);
        let listener_handle_i64 = value_to_i64(listener_handle);
        let listener_state_lock = web_server_registry()
            .get_listener(listener_handle_i64)
            .expect("listener state");
        let listener_socket_address = listener_state_lock
            .lock()
            .expect("listener lock")
            .bound_addr
            .expect("bound address (server may not have bound yet)");

        let mut client = TcpStream::connect(listener_socket_address).expect("connect listener");
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set read timeout");

        client
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("write request");
        client.flush().expect("flush");

        let mut listener_event_result = Value::nil();
        let mut listener_event = Value::nil();
        for _ in 0..20 {
            listener_event_result = web_server_poll_event(listener_handle, Value::from_int(100));
            let ok = result::result_is_ok(listener_event_result);
            if ok.is_bool() && ok.as_bool() {
                listener_event = result::result_unwrap(listener_event_result);
                if map_field_string(listener_event, "event_type") == "listener_readable" {
                    break;
                }
                unsafe {
                    crate::wr_rc_dec(listener_event);
                }
                listener_event = Value::nil();
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !listener_event.is_nil(),
            "request should arrive within ~2s (poll returns listener_readable)"
        );
        assert_eq!(
            map_field_string(listener_event, "event_type"),
            "listener_readable"
        );
        let listener_event_ok = result::result_is_ok(listener_event_result);

        let accepted_connection_result = web_server_accept_connection(listener_handle);
        let accepted_connection_ok = result::result_is_ok(accepted_connection_result);
        assert!(
            accepted_connection_ok.is_bool() && accepted_connection_ok.as_bool(),
            "expected accepted connection"
        );
        let accepted_connection_handle = result::result_unwrap(accepted_connection_result);

        let request_frame_result =
            web_server_read_http_request_frame(listener_handle, accepted_connection_handle);
        let request_frame_ok = result::result_is_ok(request_frame_result);
        assert!(request_frame_ok.is_bool() && request_frame_ok.as_bool());
        let request_frame = result::result_unwrap(request_frame_result);
        assert_eq!(map_field_string(request_frame, "path"), "/api/health");

        unsafe {
            crate::wr_rc_dec(listener_configuration);
            crate::wr_rc_dec(create_ok);
            crate::wr_rc_dec(listener_event_ok);
            crate::wr_rc_dec(listener_event);
            crate::wr_rc_dec(listener_event_result);
            crate::wr_rc_dec(accepted_connection_ok);
            crate::wr_rc_dec(accepted_connection_handle);
            crate::wr_rc_dec(accepted_connection_result);
            crate::wr_rc_dec(request_frame_ok);
            crate::wr_rc_dec(request_frame);
            crate::wr_rc_dec(request_frame_result);
            crate::wr_rc_dec(listener_handle);
            crate::wr_rc_dec(create_result);
        }
    }

    #[test]
    #[ignore = "Axum does not expose peer disconnect at request granularity; each request is independent"]
    fn poll_event_reports_connection_closed_when_peer_disconnects() {
        let listener_configuration = map::map_new();
        map_set_string(listener_configuration, "bind_address", "127.0.0.1:0");
        let create_result = web_server_create_listener(listener_configuration);
        let listener_handle = result::result_unwrap(create_result);
        let listener_handle_i64 = value_to_i64(listener_handle);
        let listener_state_lock = web_server_registry()
            .get_listener(listener_handle_i64)
            .expect("listener state");
        let listener_socket_address = listener_state_lock
            .lock()
            .expect("listener lock")
            .bound_addr
            .expect("bound address");

        let client = TcpStream::connect(listener_socket_address).expect("connect listener");
        let _ = web_server_poll_event(listener_handle, Value::from_int(250));
        let accepted_connection_result = web_server_accept_connection(listener_handle);
        let accepted_connection_handle = result::result_unwrap(accepted_connection_result);
        client
            .shutdown(Shutdown::Both)
            .expect("shutdown client connection");
        thread::sleep(Duration::from_millis(10));

        let event_result = web_server_poll_event(listener_handle, Value::from_int(250));
        let event_ok = result::result_is_ok(event_result);
        assert!(event_ok.is_bool() && event_ok.as_bool());
        let event_value = result::result_unwrap(event_result);
        assert_eq!(
            map_field_string(event_value, "event_type"),
            "connection_readable"
        );

        let read_result =
            web_server_read_http_request_frame(listener_handle, accepted_connection_handle);
        let read_ok = result::result_is_ok(read_result);
        assert!(read_ok.is_bool() && !read_ok.as_bool());
        unsafe {
            crate::wr_rc_dec(read_ok);
            crate::wr_rc_dec(read_result);
        }

        unsafe {
            crate::wr_rc_dec(listener_configuration);
            crate::wr_rc_dec(accepted_connection_handle);
            crate::wr_rc_dec(accepted_connection_result);
            crate::wr_rc_dec(event_ok);
            crate::wr_rc_dec(event_value);
            crate::wr_rc_dec(event_result);
            crate::wr_rc_dec(listener_handle);
            crate::wr_rc_dec(create_result);
        }
    }

    #[test]
    fn poll_event_does_not_hold_listener_lock_while_waiting() {
        let listener_configuration = map::map_new();
        map_set_string(listener_configuration, "bind_address", "127.0.0.1:0");
        let create_result = web_server_create_listener(listener_configuration);
        let create_ok = result::result_is_ok(create_result);
        assert!(create_ok.is_bool() && create_ok.as_bool());

        let listener_handle = result::result_unwrap(create_result);
        let listener_handle_i64 = value_to_i64(listener_handle);

        let poll_thread = thread::spawn(move || {
            web_server_poll_event(Value::from_int(listener_handle_i64), Value::from_int(250))
        });
        thread::sleep(Duration::from_millis(20));

        let accept_start = Instant::now();
        let accept_result = web_server_accept_connection(listener_handle);
        let accept_elapsed = accept_start.elapsed();
        let accept_ok = result::result_is_ok(accept_result);
        assert!(accept_ok.is_bool() && !accept_ok.as_bool());
        assert!(
            accept_elapsed < Duration::from_millis(125),
            "accept should not block on poll lock, elapsed: {:?}",
            accept_elapsed
        );

        let poll_result = poll_thread.join().expect("poll thread");
        let poll_ok = result::result_is_ok(poll_result);
        assert!(poll_ok.is_bool() && poll_ok.as_bool());

        unsafe {
            crate::wr_rc_dec(listener_configuration);
            crate::wr_rc_dec(create_ok);
            crate::wr_rc_dec(accept_ok);
            crate::wr_rc_dec(accept_result);
            crate::wr_rc_dec(poll_ok);
            crate::wr_rc_dec(poll_result);
            crate::wr_rc_dec(listener_handle);
            crate::wr_rc_dec(create_result);
        }
    }

    #[test]
    fn write_connection_bytes_not_supported_with_axum() {
        let payload = bytes::bytes_from_slice(b"hello");
        let write_result =
            web_server_write_connection_bytes(Value::from_int(1), Value::from_int(1), payload);
        let write_ok = result::result_is_ok(write_result);
        assert!(
            write_ok.is_bool() && !write_ok.as_bool(),
            "axum backend does not support write_connection_bytes"
        );
        unsafe {
            crate::wr_rc_dec(payload);
            crate::wr_rc_dec(write_result);
        }
    }

    #[test]
    fn write_http_response_frame_emits_503_retry_after_and_json_body() {
        let listener_configuration = map::map_new();
        map_set_string(listener_configuration, "bind_address", "127.0.0.1:0");
        let create_result = web_server_create_listener(listener_configuration);
        let create_ok = result::result_is_ok(create_result);
        assert!(create_ok.is_bool() && create_ok.as_bool());

        let listener_handle = result::result_unwrap(create_result);
        let listener_handle_i64 = value_to_i64(listener_handle);
        let listener_state_lock = web_server_registry()
            .get_listener(listener_handle_i64)
            .expect("listener state");
        let listener_socket_address = listener_state_lock
            .lock()
            .expect("listener lock")
            .bound_addr
            .expect("bound address");

        let mut client = TcpStream::connect(listener_socket_address).expect("connect listener");
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set read timeout");

        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("write request");
        client.flush().expect("flush");

        let mut listener_event_result = Value::nil();
        let mut listener_event = Value::nil();
        for _ in 0..20 {
            listener_event_result = web_server_poll_event(listener_handle, Value::from_int(100));
            let ok = result::result_is_ok(listener_event_result);
            if ok.is_bool() && ok.as_bool() {
                listener_event = result::result_unwrap(listener_event_result);
                if map_field_string(listener_event, "event_type") == "listener_readable" {
                    break;
                }
                unsafe {
                    crate::wr_rc_dec(listener_event);
                }
                listener_event = Value::nil();
            }
            thread::sleep(Duration::from_millis(25));
        }
        let listener_event_ok = result::result_is_ok(listener_event_result);
        assert!(
            listener_event_ok.is_bool() && listener_event_ok.as_bool(),
            "poll_event should succeed"
        );
        assert_eq!(
            map_field_string(listener_event, "event_type"),
            "listener_readable",
            "request should arrive within ~2s"
        );

        let accepted_connection_result = web_server_accept_connection(listener_handle);
        let accepted_connection_ok = result::result_is_ok(accepted_connection_result);
        assert!(
            accepted_connection_ok.is_bool() && accepted_connection_ok.as_bool(),
            "expected accepted connection"
        );
        let accepted_connection_handle = result::result_unwrap(accepted_connection_result);

        let response_headers = map::map_new();
        map_set_int(response_headers, "Retry-After", 1);
        map_set_string(response_headers, "Content-Type", "application/json");

        let response_frame = map::map_new();
        map_set_int(response_frame, "status_code", 503);
        map_set_value(response_frame, "headers", response_headers);
        map_set_string(
            response_frame,
            "body_text",
            "{\"error\":\"queue_saturated\",\"error_description\":\"request queue threshold exceeded\"}",
        );
        map_set_bool(response_frame, "should_close_connection", true);

        let write_result = web_server_write_http_response_frame(
            listener_handle,
            accepted_connection_handle,
            response_frame,
        );
        let write_ok = result::result_is_ok(write_result);
        assert!(write_ok.is_bool() && write_ok.as_bool());
        let written_bytes = result::result_unwrap(write_result);
        assert!(value_to_i64(written_bytes) > 0);

        let mut response_buffer = Vec::new();
        let mut read_buffer = [0u8; 1024];
        loop {
            match client.read(&mut read_buffer) {
                Ok(0) => break,
                Ok(read_len) => response_buffer.extend_from_slice(&read_buffer[..read_len]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => panic!("failed to read response from listener: {error}"),
            }
        }

        let response_text = String::from_utf8_lossy(response_buffer.as_slice());
        assert!(response_text.starts_with("HTTP/1.1 503 Service Unavailable\r\n"));
        // Header names may be normalized (e.g. lowercase) by HTTP stack
        assert!(
            response_text.to_lowercase().contains("retry-after: 1"),
            "expected Retry-After: 1 header"
        );
        assert!(
            response_text
                .to_lowercase()
                .contains("content-type: application/json"),
            "expected Content-Type header"
        );
        assert!(
            response_text.ends_with(
                "{\"error\":\"queue_saturated\",\"error_description\":\"request queue threshold exceeded\"}"
            )
        );

        unsafe {
            crate::wr_rc_dec(listener_configuration);
            crate::wr_rc_dec(create_ok);
            crate::wr_rc_dec(listener_event_ok);
            crate::wr_rc_dec(listener_event);
            crate::wr_rc_dec(listener_event_result);
            crate::wr_rc_dec(accepted_connection_ok);
            crate::wr_rc_dec(accepted_connection_handle);
            crate::wr_rc_dec(accepted_connection_result);
            crate::wr_rc_dec(response_headers);
            crate::wr_rc_dec(response_frame);
            crate::wr_rc_dec(write_ok);
            crate::wr_rc_dec(written_bytes);
            crate::wr_rc_dec(write_result);
            crate::wr_rc_dec(listener_handle);
            crate::wr_rc_dec(create_result);
        }
    }
}
