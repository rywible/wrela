use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::Router;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::actor::{actor_class_id, runtime_block_on};
use crate::class::{class_get, class_new, class_set};
use crate::bytes;
use crate::map::{as_map_ref, map_new, map_set};
use crate::result;
use crate::string;
use crate::value::{int_value, Value};
use crate::{wr_rc_dec, wr_rc_inc};

struct Route {
    method: Method,
    handler: Value,
    method_id: u32,
}

impl Route {
    fn new(method: Method, handler: Value, method_id: u32) -> Self {
        unsafe { wr_rc_inc(handler) };
        Self {
            method,
            handler,
            method_id,
        }
    }
}

impl Clone for Route {
    fn clone(&self) -> Self {
        unsafe { wr_rc_inc(self.handler) };
        Self {
            method: self.method.clone(),
            handler: self.handler,
            method_id: self.method_id,
        }
    }
}

impl Drop for Route {
    fn drop(&mut self) {
        unsafe { wr_rc_dec(self.handler) };
    }
}

struct HttpRegistry {
    routes: HashMap<String, Vec<Route>>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Default for HttpRegistry {
    fn default() -> Self {
        Self {
            routes: HashMap::new(),
            shutdown: None,
        }
    }
}

static REGISTRY: OnceLock<Mutex<HttpRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<HttpRegistry> {
    REGISTRY.get_or_init(|| Mutex::new(HttpRegistry::default()))
}

struct ClassRegistry {
    by_name: HashMap<String, u32>,
    methods: HashMap<(u32, String), u32>,
}

impl Default for ClassRegistry {
    fn default() -> Self {
        Self {
            by_name: HashMap::new(),
            methods: HashMap::new(),
        }
    }
}

static CLASS_REGISTRY: OnceLock<Mutex<ClassRegistry>> = OnceLock::new();

fn class_registry() -> &'static Mutex<ClassRegistry> {
    CLASS_REGISTRY.get_or_init(|| Mutex::new(ClassRegistry::default()))
}

pub fn register_class(name_ptr: *const u8, len: usize, class_id: u32) {
    if name_ptr.is_null() && len != 0 {
        return;
    }
    let name = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    let Ok(name) = std::str::from_utf8(name) else {
        return;
    };
    let mut registry = class_registry().lock().expect("class registry lock");
    registry.by_name.insert(name.to_string(), class_id);
}

pub fn register_method_name(name_ptr: *const u8, len: usize, class_id: u32, method_id: u32) {
    if name_ptr.is_null() && len != 0 {
        return;
    }
    let name = unsafe { std::slice::from_raw_parts(name_ptr, len) };
    let Ok(name) = std::str::from_utf8(name) else {
        return;
    };
    let mut registry = class_registry().lock().expect("class registry lock");
    registry
        .methods
        .insert((class_id, name.to_string()), method_id);
}

fn class_id_for(name: &str) -> Option<u32> {
    let registry = class_registry().lock().expect("class registry lock");
    registry.by_name.get(name).copied()
}

pub(crate) fn method_id_for(class_id: u32, name: &str) -> Option<u32> {
    let registry = class_registry().lock().expect("class registry lock");
    registry
        .methods
        .get(&(class_id, name.to_string()))
        .copied()
}

pub fn serve_get_requests(path: Value, handler: Value) -> Value {
    add_route(Method::GET, path, handler)
}

pub fn serve_post_requests(path: Value, handler: Value) -> Value {
    add_route(Method::POST, path, handler)
}

pub fn serve_requests(method: Value, path: Value, handler: Value) -> Value {
    let Some(method_name) = string_value(method) else {
        return Value::nil();
    };
    let method_upper = method_name.to_ascii_uppercase();
    let Ok(method_parsed) = Method::from_bytes(method_upper.as_bytes()) else {
        return Value::nil();
    };
    add_route(method_parsed, path, handler)
}

fn add_route(method: Method, path: Value, handler: Value) -> Value {
    let Some(path_str) = string_value(path) else {
        return Value::nil();
    };
    let Some(class_id) = actor_class_id(handler) else {
        return Value::nil();
    };
    let Some(method_id) = method_id_for(class_id, "handle") else {
        return Value::nil();
    };
    let normalized = normalize_path(&path_str);

    let mut registry = registry().lock().expect("http registry lock");
    registry
        .routes
        .entry(normalized)
        .or_default()
        .push(Route::new(method, handler, method_id));
    Value::nil()
}

pub fn serve_on(addr: Value) -> Value {
    let Some(addr_str) = string_value(addr) else {
        return Value::nil();
    };
    let listener = match runtime_block_on(async { TcpListener::bind(&addr_str).await }) {
        Ok(listener) => listener,
        Err(_) => return Value::nil(),
    };
    let (server_listener, app, shutdown_rx) = setup_server(listener);
    runtime_block_on(async move {
        let _ = axum::serve(server_listener.0, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    Value::nil()
}

pub fn stop() -> Value {
    let mut registry = registry().lock().expect("http registry lock");
    if let Some(tx) = registry.shutdown.take() {
        let _ = tx.send(());
    }
    Value::nil()
}

#[derive(Clone)]
struct HandlerState {
    routes: Arc<HashMap<String, Vec<Route>>>,
}

fn build_router(routes: HashMap<String, Vec<Route>>) -> Router {
    let state = HandlerState {
        routes: Arc::new(routes),
    };
    let mut router = Router::new();
    for pattern in state.routes.keys() {
        let handler = any(handle_request);
        router = router.route(pattern, handler);
    }
    router.with_state(state)
}

fn setup_server(listener: TcpListener) -> (ServerAddr, Router, oneshot::Receiver<()>) {
    let mut registry = registry().lock().expect("http registry lock");
    let routes = std::mem::take(&mut registry.routes);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    registry.shutdown = Some(shutdown_tx);
    drop(registry);
    let app = build_router(routes);
    (ServerAddr(listener), app, shutdown_rx)
}

struct ServerAddr(TcpListener);

#[cfg(test)]
pub fn serve_on_listener(listener: TcpListener) -> std::net::SocketAddr {
    let (listener, app, shutdown_rx) = setup_server(listener);
    let addr = listener.0.local_addr().expect("local addr");
    crate::actor::runtime_spawn(async move {
        let _ = axum::serve(listener.0, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    addr
}

async fn handle_request(
    axum::extract::State(state): axum::extract::State<HandlerState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some((routes, params_map)) = match_route(uri.path(), &state.routes) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(route) = routes.iter().find(|route| route.method == method).cloned() else {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    };

    let query_map = build_query_map(&uri);
    let headers_map = build_headers_map(&headers);
    let body_val = bytes::bytes_from_slice(&body);

    let req_val = match build_request_value(&method, &uri, headers_map, query_map, params_map, body_val) {
        Some(val) => val,
        None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let args = [req_val];
    let pending = crate::actor::actor_send(route.handler, route.method_id, 1, args.as_ptr());
    let result = crate::actor::pending_await_async(pending).await;
    let response_val = result::result_unwrap(result);
    let response = build_response(response_val);

    unsafe {
        wr_rc_dec(pending);
        wr_rc_dec(result);
        wr_rc_dec(req_val);
        wr_rc_dec(response_val);
    }
    response
}

fn build_request_value(
    method: &Method,
    uri: &Uri,
    headers: Value,
    query: Value,
    params: Value,
    body: Value,
) -> Option<Value> {
    let class_id = class_id_for("HttpRequest")?;
    let req_val = class_new(class_id, std::ptr::null(), std::ptr::null(), 0);

    let method_val = string::str_from_bytes(method.as_str().as_bytes());
    let path_val = uri.path().as_bytes();
    let path_val = string::str_from_bytes(path_val);

    class_set(req_val, b"method".as_ptr(), 6, method_val);
    class_set(req_val, b"path".as_ptr(), 4, path_val);
    class_set(req_val, b"headers".as_ptr(), 7, headers);
    class_set(req_val, b"query".as_ptr(), 5, query);
    class_set(req_val, b"params".as_ptr(), 6, params);
    class_set(req_val, b"body".as_ptr(), 4, body);

    unsafe {
        wr_rc_dec(method_val);
        wr_rc_dec(path_val);
        wr_rc_dec(headers);
        wr_rc_dec(query);
        wr_rc_dec(params);
        wr_rc_dec(body);
    }

    Some(req_val)
}

fn build_response(resp_val: Value) -> axum::response::Response {
    let status = class_get(resp_val, b"status".as_ptr(), 6);
    let headers_val = class_get(resp_val, b"headers".as_ptr(), 7);
    let body_val = class_get(resp_val, b"body".as_ptr(), 4);

    let status_code = int_value(status)
        .and_then(|val| StatusCode::from_u16(val as u16).ok())
        .unwrap_or(StatusCode::OK);

    let (mut headers, has_content_type, has_server) = build_response_headers(headers_val);
    if !has_server {
        headers.insert("server", HeaderValue::from_static("Wrela"));
    }
    let (body_bytes, body_kind) = body_bytes(body_val);
    if !has_content_type {
        match body_kind {
            BodyKind::Text if !body_bytes.is_empty() => {
                headers.insert(
                    "content-type",
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
            }
            BodyKind::Binary if !body_bytes.is_empty() => {
                headers.insert(
                    "content-type",
                    HeaderValue::from_static("application/octet-stream"),
                );
            }
            _ => {}
        }
    }

    let mut response = axum::response::Response::new(axum::body::Body::from(body_bytes));
    *response.status_mut() = status_code;
    *response.headers_mut() = headers;

    unsafe {
        wr_rc_dec(status);
        wr_rc_dec(headers_val);
        wr_rc_dec(body_val);
    }
    response
}

fn build_response_headers(headers_val: Value) -> (HeaderMap, bool, bool) {
    let mut headers = HeaderMap::new();
    let mut has_content_type = false;
    let mut has_server = false;

    let Some(map) = as_map_ref(headers_val) else {
        return (headers, has_content_type, has_server);
    };
    unsafe {
        for (key, val) in (*map).entries.iter() {
            let Some(name) = string_value(key.0) else {
                continue;
            };
            let Some(value) = string_value(*val) else {
                continue;
            };
            let name_lower = name.to_ascii_lowercase();
            let Ok(header_name) = HeaderName::from_bytes(name_lower.as_bytes()) else {
                continue;
            };
            let Ok(header_value) = HeaderValue::from_str(&value) else {
                continue;
            };
            if header_name == HeaderName::from_static("content-type") {
                has_content_type = true;
            }
            if header_name == HeaderName::from_static("server") {
                has_server = true;
            }
            headers.insert(header_name, header_value);
        }
    }

    (headers, has_content_type, has_server)
}

fn build_headers_map(headers: &HeaderMap) -> Value {
    let map = map_new();
    for (name, value) in headers.iter() {
        let name_val = string::str_from_bytes(name.as_str().as_bytes());
        let value_val = match value.to_str() {
            Ok(val) => string::str_from_bytes(val.as_bytes()),
            Err(_) => continue,
        };
        map_set(map, name_val, value_val);
        unsafe {
            wr_rc_dec(name_val);
            wr_rc_dec(value_val);
        }
    }
    map
}

enum BodyKind {
    Empty,
    Text,
    Binary,
}

fn body_bytes(val: Value) -> (Bytes, BodyKind) {
    if let Some(bytes) = bytes::with_bytes(val, |data| Bytes::copy_from_slice(data)) {
        let kind = if bytes.is_empty() {
            BodyKind::Empty
        } else {
            BodyKind::Binary
        };
        return (bytes, kind);
    }
    if let Some(text) = string_value(val) {
        if text.is_empty() {
            return (Bytes::new(), BodyKind::Empty);
        }
        return (Bytes::from(text), BodyKind::Text);
    }
    (Bytes::new(), BodyKind::Empty)
}

fn build_query_map(uri: &Uri) -> Value {
    let query_map = map_new();
    if let Some(query) = uri.query() {
        for (key, val) in parse_query(query) {
            let key_val = string::str_from_bytes(key.as_bytes());
            let val_val = string::str_from_bytes(val.as_bytes());
            map_set(query_map, key_val, val_val);
            unsafe {
                wr_rc_dec(key_val);
                wr_rc_dec(val_val);
            }
        }
    }
    query_map
}

fn match_route<'a>(
    path: &str,
    routes: &'a HashMap<String, Vec<Route>>,
) -> Option<(&'a Vec<Route>, Value)> {
    let path_parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    for (pattern, routes) in routes {
        let pat_parts: Vec<&str> = pattern.trim_matches('/').split('/').collect();
        if path_parts.len() != pat_parts.len() {
            continue;
        }
        let params_map = map_new();
        let mut matched = true;
        for (pat, actual) in pat_parts.iter().zip(path_parts.iter()) {
            let param = if let Some(param) = pat.strip_prefix(':') {
                Some(param)
            } else {
                pat.strip_prefix('{').and_then(|p| p.strip_suffix('}'))
            };
            if let Some(param) = param {
                let key_val = string::str_from_bytes(param.as_bytes());
                let val_val = string::str_from_bytes(actual.as_bytes());
                map_set(params_map, key_val, val_val);
                unsafe {
                    wr_rc_dec(key_val);
                    wr_rc_dec(val_val);
                }
                continue;
            }
            if pat != actual {
                matched = false;
                break;
            }
        }
        if matched {
            return Some((routes, params_map));
        }
        unsafe { wr_rc_dec(params_map) };
    }
    None
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut iter = part.splitn(2, '=');
        let key = iter.next().unwrap_or("");
        let val = iter.next().unwrap_or("");
        out.push((percent_decode(key), percent_decode(val)));
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = from_hex(bytes[i + 1]);
                let lo = from_hex(bytes[i + 2]);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn normalize_path(path: &str) -> String {
    let mut out = String::new();
    if !path.starts_with('/') {
        out.push('/');
    }
    out.push_str(path);
    if out.is_empty() {
        out.push('/');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class;
    use crate::map;
    use crate::string;
    use crate::value;
    use crate::wr_actor_spawn;
    use crate::wr_rc_dec;
    use crate::wr_register_method;
    use reqwest::Client;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    const CLASS_HTTP_REQUEST: u32 = 200;
    const CLASS_HTTP_RESPONSE: u32 = 201;
    const CLASS_HANDLER: u32 = 202;
    const CLASS_HANDLER_TEXT: u32 = 203;
    const CLASS_HANDLER_PARAMS: u32 = 204;
    const CLASS_HANDLER_ECHO: u32 = 205;
    const CLASS_HANDLER_QUERY: u32 = 206;
    const CLASS_HANDLER_EMPTY: u32 = 207;
    const CLASS_HANDLER_POOL: u32 = 208;
    const CLASS_HANDLER_HEADERS: u32 = 209;
    const METHOD_HANDLE: u32 = 0;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock")
    }

    fn reset_registry() {
        let mut reg = registry().lock().expect("http registry lock");
        reg.routes.clear();
        reg.shutdown = None;
        drop(reg);
    }

    fn register_http_classes() {
        register_class(b"HttpRequest".as_ptr(), 11, CLASS_HTTP_REQUEST);
        register_class(b"HttpResponse".as_ptr(), 12, CLASS_HTTP_RESPONSE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_TEXT, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_PARAMS, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_ECHO, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_QUERY, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_EMPTY, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_POOL, METHOD_HANDLE);
        register_method_name(b"handle".as_ptr(), 6, CLASS_HANDLER_HEADERS, METHOD_HANDLE);
    }

    extern "C" fn handle_request(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let req = args[1];

        let params = class::class_get(req, b"params".as_ptr(), 6);
        let query = class::class_get(req, b"query".as_ptr(), 5);
        let headers = class::class_get(req, b"headers".as_ptr(), 7);
        let body = class::class_get(req, b"body".as_ptr(), 4);

        let key_id = string::str_from_bytes(b"id");
        let key_foo = string::str_from_bytes(b"foo");
        let key_hdr = string::str_from_bytes(b"x-test");

        let id = map::map_get(params, key_id);
        let foo = map::map_get(query, key_foo);
        let hdr = map::map_get(headers, key_hdr);

        let expect_id = string::str_from_bytes(b"42");
        let expect_foo = string::str_from_bytes(b"bar");
        let expect_hdr = string::str_from_bytes(b"ok");

        let ok = value::value_eq(id, expect_id)
            && value::value_eq(foo, expect_foo)
            && value::value_eq(hdr, expect_hdr);

        let body_len = bytes::bytes_len(body);
        let body_ok = value::int_value(body_len).unwrap_or(0) == 5;

        let status_code = if ok && body_ok { 200 } else { 400 };
        let body_text: &[u8] = if status_code == 200 {
            b"ok"
        } else {
            b"bad"
        };

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let headers_map = map::map_new();
        let header_key = string::str_from_bytes(b"x-echo");
        let header_val = string::str_from_bytes(b"yes");
        map::map_set(headers_map, header_key, header_val);

        let body_bytes = bytes::bytes_from_slice(body_text);
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(status_code));
        class::class_set(resp, b"headers".as_ptr(), 7, headers_map);
        class::class_set(resp, b"body".as_ptr(), 4, body_bytes);

        unsafe {
            wr_rc_dec(params);
            wr_rc_dec(query);
            wr_rc_dec(headers);
            wr_rc_dec(body);
            wr_rc_dec(key_id);
            wr_rc_dec(key_foo);
            wr_rc_dec(key_hdr);
            wr_rc_dec(id);
            wr_rc_dec(foo);
            wr_rc_dec(hdr);
            wr_rc_dec(expect_id);
            wr_rc_dec(expect_foo);
            wr_rc_dec(expect_hdr);
            wr_rc_dec(body_len);
            wr_rc_dec(headers_map);
            wr_rc_dec(header_key);
            wr_rc_dec(header_val);
            wr_rc_dec(body_bytes);
        }

        resp
    }

    extern "C" fn handle_text(argc: usize, _argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let headers_map = map::map_new();
        let header_key = string::str_from_bytes(b"content-type");
        let header_val = string::str_from_bytes(b"text/custom");
        map::map_set(headers_map, header_key, header_val);

        let body_text = string::str_from_bytes(b"hello");
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(200));
        class::class_set(resp, b"headers".as_ptr(), 7, headers_map);
        class::class_set(resp, b"body".as_ptr(), 4, body_text);

        unsafe {
            wr_rc_dec(headers_map);
            wr_rc_dec(header_key);
            wr_rc_dec(header_val);
            wr_rc_dec(body_text);
        }

        resp
    }

    extern "C" fn handle_params(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let req = args[1];
        let params = class::class_get(req, b"params".as_ptr(), 6);

        let key_team = string::str_from_bytes(b"team");
        let key_id = string::str_from_bytes(b"id");
        let team = map::map_get(params, key_team);
        let id = map::map_get(params, key_id);

        let expect_team = string::str_from_bytes(b"alpha");
        let expect_id = string::str_from_bytes(b"7");

        let ok = value::value_eq(team, expect_team) && value::value_eq(id, expect_id);

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let body: &[u8] = if ok { b"ok" } else { b"bad" };
        let body_bytes = bytes::bytes_from_slice(body);
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(if ok { 200 } else { 400 }));
        class::class_set(resp, b"headers".as_ptr(), 7, map::map_new());
        class::class_set(resp, b"body".as_ptr(), 4, body_bytes);

        unsafe {
            wr_rc_dec(params);
            wr_rc_dec(key_team);
            wr_rc_dec(key_id);
            wr_rc_dec(team);
            wr_rc_dec(id);
            wr_rc_dec(expect_team);
            wr_rc_dec(expect_id);
            wr_rc_dec(body_bytes);
        }

        resp
    }

    extern "C" fn handle_echo(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let req = args[1];
        let body = class::class_get(req, b"body".as_ptr(), 4);

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(200));
        class::class_set(resp, b"headers".as_ptr(), 7, map::map_new());
        class::class_set(resp, b"body".as_ptr(), 4, body);

        unsafe {
            wr_rc_dec(body);
        }

        resp
    }

    extern "C" fn handle_query(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let req = args[1];
        let query = class::class_get(req, b"query".as_ptr(), 5);

        let key_x = string::str_from_bytes(b"x");
        let key_y = string::str_from_bytes(b"y");
        let x = map::map_get(query, key_x);
        let y = map::map_get(query, key_y);

        let expect_x = string::str_from_bytes(b"a+b");
        let expect_y = string::str_from_bytes(b"hello world");
        let ok = value::value_eq(x, expect_x) && value::value_eq(y, expect_y);

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let body: &[u8] = if ok { b"ok" } else { b"bad" };
        let body_bytes = bytes::bytes_from_slice(body);
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(if ok { 200 } else { 400 }));
        class::class_set(resp, b"headers".as_ptr(), 7, map::map_new());
        class::class_set(resp, b"body".as_ptr(), 4, body_bytes);

        unsafe {
            wr_rc_dec(query);
            wr_rc_dec(key_x);
            wr_rc_dec(key_y);
            wr_rc_dec(x);
            wr_rc_dec(y);
            wr_rc_dec(expect_x);
            wr_rc_dec(expect_y);
            wr_rc_dec(body_bytes);
        }

        resp
    }

    extern "C" fn handle_empty(argc: usize, _argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let body_bytes = bytes::bytes_from_slice(b"");
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(204));
        class::class_set(resp, b"headers".as_ptr(), 7, map::map_new());
        class::class_set(resp, b"body".as_ptr(), 4, body_bytes);
        unsafe {
            wr_rc_dec(body_bytes);
        }
        resp
    }

    extern "C" fn handle_pool(argc: usize, argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let instance = args[0];

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let body_bytes = bytes::bytes_from_string(instance);
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(200));
        class::class_set(resp, b"headers".as_ptr(), 7, map::map_new());
        class::class_set(resp, b"body".as_ptr(), 4, body_bytes);

        unsafe {
            wr_rc_dec(body_bytes);
        }

        resp
    }

    extern "C" fn handle_headers(argc: usize, _argv: *const Value) -> Value {
        if argc < 2 {
            return Value::nil();
        }
        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let headers_map = map::map_new();
        let header_key = string::str_from_bytes(b"Content-Type");
        let header_val = string::str_from_bytes(b"text/custom");
        let server_key = string::str_from_bytes(b"Server");
        let server_val = string::str_from_bytes(b"Custom");
        map::map_set(headers_map, header_key, header_val);
        map::map_set(headers_map, server_key, server_val);

        let body_text = string::str_from_bytes(b"hello");
        class::class_set(resp, b"status".as_ptr(), 6, Value::from_int(200));
        class::class_set(resp, b"headers".as_ptr(), 7, headers_map);
        class::class_set(resp, b"body".as_ptr(), 4, body_text);

        unsafe {
            wr_rc_dec(headers_map);
            wr_rc_dec(header_key);
            wr_rc_dec(header_val);
            wr_rc_dec(server_key);
            wr_rc_dec(server_val);
            wr_rc_dec(body_text);
        }

        resp
    }

    fn spawn_handler(class_id: u32, handler: extern "C" fn(usize, *const Value) -> Value) -> Value {
        wr_register_method(class_id, METHOD_HANDLE, handler);
        wr_actor_spawn(class_id.into(), Value::nil(), 1, 3, -1, -1, -1)
    }

    fn start_server(route: &str, method: Method, handler: Value) -> std::net::SocketAddr {
        let route_val = string::str_from_bytes(route.as_bytes());
        match method {
            Method::GET => {
                serve_get_requests(route_val, handler);
            }
            Method::POST => {
                serve_post_requests(route_val, handler);
            }
            _ => {
                let method_val = string::str_from_bytes(method.as_str().as_bytes());
                serve_requests(method_val, route_val, handler);
                unsafe { wr_rc_dec(method_val) };
            }
        };
        unsafe { wr_rc_dec(route_val) };

        let listener =
            runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        serve_on_listener(listener)
    }

    fn start_server_with_routes(routes: Vec<(Method, &'static str, Value)>) -> std::net::SocketAddr {
        for (method, path, handler) in routes {
            let route_val = string::str_from_bytes(path.as_bytes());
            match method {
                Method::GET => {
                    serve_get_requests(route_val, handler);
                }
                Method::POST => {
                    serve_post_requests(route_val, handler);
                }
                _ => {
                    let method_val = string::str_from_bytes(method.as_str().as_bytes());
                    serve_requests(method_val, route_val, handler);
                    unsafe { wr_rc_dec(method_val) };
                }
            }
            unsafe { wr_rc_dec(route_val) };
        }
        let listener =
            runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        serve_on_listener(listener)
    }

    fn send_with_retry<F>(mut make: F) -> reqwest::Response
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for _ in 0..50 {
            match runtime_block_on(async { make().send().await }) {
                Ok(resp) => return resp,
                Err(err) if err.is_connect() => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) => panic!("request: {err:?}"),
            }
        }
        panic!("request: timeout waiting for server");
    }

    fn send_expect_fail<F>(mut make: F)
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for _ in 0..50 {
            match runtime_block_on(async { make().send().await }) {
                Ok(_) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) if err.is_connect() => return,
                Err(err) => panic!("request: {err:?}"),
            }
        }
        panic!("request: expected connection failure");
    }

    #[test]
    fn http_server_bytes_and_params() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let handler = spawn_handler(CLASS_HANDLER, handle_request);
        let addr = start_server("/users/:id", Method::POST, handler);

        let client = Client::new();
        let url = format!("http://{addr}/users/42?foo=bar");
        let resp = send_with_retry(|| client.post(&url).header("x-test", "ok").body("hello"));

        assert_eq!(resp.status(), StatusCode::OK);
        let echo = resp
            .headers()
            .get("x-echo")
            .and_then(|v| v.to_str().ok());
        assert_eq!(echo, Some("yes"));
        let server = resp
            .headers()
            .get("server")
            .and_then(|v| v.to_str().ok());
        assert_eq!(server, Some("Wrela"));
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(content_type, Some("application/octet-stream"));

        let body = runtime_block_on(async { resp.text().await.expect("body") });
        assert_eq!(body, "ok");

        stop();
        std::thread::sleep(Duration::from_millis(50));

        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_respects_content_type_and_not_found() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
        let addr = start_server("/text", Method::GET, handler);

        let client = Client::new();
        let url = format!("http://{addr}/text");
        let resp = send_with_retry(|| client.get(&url));
        assert_eq!(resp.status(), StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(content_type, Some("text/custom"));

        let missing = send_with_retry(|| client.get(format!("http://{addr}/missing")));
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let wrong_method = send_with_retry(|| client.post(format!("http://{addr}/text")));
        assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_multiple_routes_and_params() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler_params = spawn_handler(CLASS_HANDLER_PARAMS, handle_params);
        let handler_text = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
        let addr = start_server_with_routes(vec![
            (Method::GET, "/teams/:team/users/:id", handler_params),
            (Method::POST, "/text", handler_text),
        ]);

        let client = Client::new();
        let url = format!("http://{addr}/teams/alpha/users/7");
        let resp = send_with_retry(|| client.get(&url));
        assert_eq!(resp.status(), StatusCode::OK);

        let missing = send_with_retry(|| client.get(format!("http://{addr}/teams/alpha/users")));
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let post_text = send_with_retry(|| client.post(format!("http://{addr}/text")));
        assert_eq!(post_text.status(), StatusCode::OK);

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler_params);
            wr_rc_dec(handler_text);
        }
    }

    #[test]
    fn http_server_binary_round_trip() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler = spawn_handler(CLASS_HANDLER_ECHO, handle_echo);
        let addr = start_server("/echo", Method::POST, handler);

        let client = Client::new();
        let body = vec![0, 1, 2, 3, 4, 255];
        let resp = send_with_retry(|| client.post(format!("http://{addr}/echo")).body(body.clone()));
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = runtime_block_on(async { resp.bytes().await.expect("body") });
        assert_eq!(bytes.as_ref(), body.as_slice());

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_query_decode_and_empty_body() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler_query = spawn_handler(CLASS_HANDLER_QUERY, handle_query);
        let handler_empty = spawn_handler(CLASS_HANDLER_EMPTY, handle_empty);
        let addr = start_server_with_routes(vec![
            (Method::GET, "/query", handler_query),
            (Method::GET, "/empty", handler_empty),
        ]);

        let client = Client::new();
        let url = format!("http://{addr}/query?x=a%2Bb&y=hello+world");
        let resp = send_with_retry(|| client.get(&url));
        assert_eq!(resp.status(), StatusCode::OK);

        let empty_resp = send_with_retry(|| client.get(format!("http://{addr}/empty")));
        assert_eq!(empty_resp.status(), StatusCode::NO_CONTENT);
        let content_type = empty_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert!(content_type.is_none());

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler_query);
            wr_rc_dec(handler_empty);
        }
    }

    #[test]
    fn http_server_stop_closes_port() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
        let addr = start_server("/text", Method::GET, handler);

        let client = Client::new();
        let resp = send_with_retry(|| client.get(format!("http://{addr}/text")));
        assert_eq!(resp.status(), StatusCode::OK);

        stop();
        std::thread::sleep(Duration::from_millis(50));
        send_expect_fail(|| client.get(format!("http://{addr}/text")));

        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_ignores_invalid_handler() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let route_val = string::str_from_bytes(b"/invalid");
        serve_get_requests(route_val, Value::nil());
        unsafe { wr_rc_dec(route_val) };

        let listener =
            runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        let addr = serve_on_listener(listener);

        let client = Client::new();
        let resp = send_with_retry(|| client.get(format!("http://{addr}/invalid")));
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        stop();
        std::thread::sleep(Duration::from_millis(50));
    }

    #[test]
    fn http_server_pool_fan_out() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        wr_register_method(CLASS_HANDLER_POOL, METHOD_HANDLE, handle_pool);
        let instance_a = string::str_from_bytes(b"a");
        let actor_a = wr_actor_spawn(
            CLASS_HANDLER_POOL.into(),
            instance_a,
            1,
            3,
            -1,
            -1,
            -1,
        );
        let instance_b = string::str_from_bytes(b"b");
        let actor_b = wr_actor_spawn(
            CLASS_HANDLER_POOL.into(),
            instance_b,
            1,
            3,
            -1,
            -1,
            -1,
        );
        unsafe {
            wr_rc_dec(instance_a);
            wr_rc_dec(instance_b);
        }

        let handles = crate::list::list_new(2);
        crate::list::list_set(handles, 0, actor_a);
        crate::list::list_set(handles, 1, actor_b);
        let pool = crate::actor::pool_new(handles, 3, 2, 2, 1, 64);

        let addr = start_server("/pool", Method::GET, pool);
        let client = Client::new();

        let mut saw_a = false;
        let mut saw_b = false;
        for _ in 0..20 {
            let resp = send_with_retry(|| client.get(format!("http://{addr}/pool")));
            let body = runtime_block_on(async { resp.text().await.expect("body") });
            if body == "a" {
                saw_a = true;
            }
            if body == "b" {
                saw_b = true;
            }
            if saw_a && saw_b {
                break;
            }
        }

        assert!(saw_a && saw_b);

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(pool);
            wr_rc_dec(handles);
            wr_rc_dec(actor_a);
            wr_rc_dec(actor_b);
        }
    }

    #[test]
    fn http_server_concurrent_requests() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
        let addr = start_server("/text", Method::GET, handler);

        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("client");
        send_with_retry(|| client.get(format!("http://{addr}/text")));
        let total = 25usize;
        let timeout = Duration::from_secs(2);
        runtime_block_on(async {
            let mut set = tokio::task::JoinSet::new();
            for _ in 0..total {
                let client = client.clone();
                let url = format!("http://{addr}/text");
                set.spawn(async move {
                    match tokio::time::timeout(timeout, client.get(url).send()).await {
                        Ok(Ok(resp)) => resp.status(),
                        Ok(Err(_)) => StatusCode::SERVICE_UNAVAILABLE,
                        Err(_) => StatusCode::REQUEST_TIMEOUT,
                    }
                });
            }
            let mut ok = 0;
            while let Some(res) = set.join_next().await {
                let status = res.expect("join");
                if status == StatusCode::OK {
                    ok += 1;
                }
            }
            assert_eq!(ok, total);
        });

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_header_override_and_case() {
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler = spawn_handler(CLASS_HANDLER_HEADERS, handle_headers);
        let addr = start_server("/headers", Method::GET, handler);

        let client = Client::new();
        let resp = send_with_retry(|| client.get(format!("http://{addr}/headers")));
        assert_eq!(resp.status(), StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert_eq!(content_type, Some("text/custom"));
        let server = resp
            .headers()
            .get("server")
            .and_then(|v| v.to_str().ok());
        assert_eq!(server, Some("Custom"));

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler);
        }
    }
}

fn string_value(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).to_string())
}
