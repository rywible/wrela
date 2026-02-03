use axum::Router;
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::any;
use jsonwebtoken::{DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::actor::{actor_class_id, runtime_block_on};
use crate::bytes;
use crate::class::{class_get, class_new, class_set};
use crate::list::as_list_ref;
use crate::map::{as_map_ref, map_new, map_set};
use crate::result;
use crate::string;
use crate::value::{Value, int_value};
use crate::{wr_rc_dec, wr_rc_inc};
use crate::storage_helpers::{storage_get_json_result, storage_get_json_vec_result, storage_set_json_result};
use crate::storage::service::StorageError;

#[derive(Clone)]
struct HttpConfig {
    auth_token: Option<String>,
    auth_jwt_enabled: bool,
    jwt_secret: String,
    rbac_permission: Option<String>,
    rbac_scope: String,
    rbac_skip_paths: Vec<String>,
    rate_limit_enabled: bool,
    rate_limit_burst: u64,
    rate_limit_per_secs: u64,
    rate_limit_skip_paths: Vec<String>,
    hsts_enabled: bool,
    csp: Option<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            auth_token: None,
            auth_jwt_enabled: false,
            jwt_secret: "wrela-dev-secret".to_string(),
            rbac_permission: None,
            rbac_scope: "global".to_string(),
            rbac_skip_paths: Vec::new(),
            rate_limit_enabled: false,
            rate_limit_burst: 60,
            rate_limit_per_secs: 60,
            rate_limit_skip_paths: Vec::new(),
            hsts_enabled: false,
            csp: None,
        }
    }
}

static HTTP_CONFIG: OnceLock<Mutex<HttpConfig>> = OnceLock::new();

fn http_config() -> HttpConfig {
    HTTP_CONFIG
        .get_or_init(|| Mutex::new(HttpConfig::default()))
        .lock()
        .expect("http config lock")
        .clone()
}

fn set_http_config(config: HttpConfig) {
    *HTTP_CONFIG
        .get_or_init(|| Mutex::new(HttpConfig::default()))
        .lock()
        .expect("http config lock") = config;
}

fn http_auth_token() -> Option<String> {
    http_config().auth_token
}

fn http_auth_jwt_enabled() -> bool {
    http_config().auth_jwt_enabled
}

fn jwt_secret() -> String {
    http_config().jwt_secret
}

#[derive(Deserialize)]
struct JwtClaims {
    #[allow(dead_code)]
    exp: usize,
    sub: Option<String>,
}

fn decode_jwt(token: &str) -> Option<JwtClaims> {
    let key = DecodingKey::from_secret(jwt_secret().as_bytes());
    let mut validation = Validation::default();
    validation.validate_exp = true;
    jsonwebtoken::decode::<JwtClaims>(token, &key, &validation)
        .ok()
        .map(|data| data.claims)
}

fn authorized(headers: &HeaderMap) -> bool {
    let auth = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if let Some(token) = http_auth_token() {
        if auth == token || auth == format!("Bearer {token}") {
            return true;
        }
    }
    if http_auth_jwt_enabled() {
        let bearer = auth.strip_prefix("Bearer ").unwrap_or(auth);
        return !bearer.is_empty() && decode_jwt(bearer).is_some();
    }
    true
}

fn http_rbac_permission() -> Option<String> {
    http_config().rbac_permission
}

fn http_rbac_scope() -> String {
    http_config().rbac_scope
}

fn http_rbac_skip_paths() -> Vec<String> {
    http_config().rbac_skip_paths
}

fn http_rate_limit_enabled() -> bool {
    http_config().rate_limit_enabled
}

fn http_rate_limit_burst() -> u64 {
    http_config().rate_limit_burst.max(1)
}

fn http_rate_limit_per_secs() -> u64 {
    http_config().rate_limit_per_secs.max(1)
}

fn http_rate_limit_skip_paths() -> Vec<String> {
    http_config().rate_limit_skip_paths
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn rate_limit_key(headers: &HeaderMap) -> String {
    if let Some(ip) = header_value(headers, "x-forwarded-for") {
        return ip;
    }
    if let Some(ip) = header_value(headers, "x-real-ip") {
        return ip;
    }
    "unknown".to_string()
}

fn jwt_subject(headers: &HeaderMap) -> Option<String> {
    let auth = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if auth.is_empty() {
        return None;
    }
    let bearer = auth.strip_prefix("Bearer ").unwrap_or(auth);
    decode_jwt(bearer).and_then(|claims| claims.sub)
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredRole {
    id: String,
    scope: String,
    name: String,
    permissions: Vec<String>,
}

async fn rbac_allowed(
    user_id: &str,
    permission: &str,
    scope_id: &str,
) -> Result<bool, StorageError> {
    let assign_key = format!("rbac:assign:{scope_id}:{user_id}");
    let role_ids = storage_get_json_vec_result::<String>(&assign_key).await?;
    for role_id in role_ids {
        let role_key = format!("rbac:role:{role_id}");
        match storage_get_json_result::<StoredRole>(&role_key).await? {
            Some(role) => {
                if role.permissions.iter().any(|p| p == permission) {
                    return Ok(true);
                }
            }
            None => {}
        }
    }
    Ok(false)
}

#[derive(Clone, Serialize, Deserialize)]
struct RateBucket {
    tokens: f64,
    last: u64,
    burst: f64,
    per_secs: f64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn rate_limit_allowed(
    key: &str,
    burst: u64,
    per_secs: u64,
) -> Result<bool, StorageError> {
    let bucket_key = format!("rate:{key}");
    let burst = burst as f64;
    let per_secs = per_secs as f64;
    let now = now_secs();
    let mut bucket = storage_get_json_result::<RateBucket>(&bucket_key)
        .await?
        .unwrap_or(RateBucket {
            tokens: burst,
            last: now,
            burst,
            per_secs,
        });
    let elapsed = (now.saturating_sub(bucket.last)) as f64;
    let rate = bucket.burst / bucket.per_secs;
    bucket.tokens = (bucket.tokens + elapsed * rate).min(bucket.burst);
    bucket.last = now;
    let ok = if bucket.tokens >= 1.0 {
        bucket.tokens -= 1.0;
        true
    } else {
        false
    };
    storage_set_json_result(&bucket_key, &bucket).await?;
    Ok(ok)
}

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
    registry.methods.get(&(class_id, name.to_string())).copied()
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

pub fn http_server_configure(config: Value) -> Value {
    let new_config = http_config_from_value(config);
    set_http_config(new_config);
    Value::nil()
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
    patterns: Arc<Vec<RoutePattern>>,
}

struct RoutePattern {
    pattern: String,
    parts: Vec<PatternPart>,
    routes: Vec<Route>,
}

enum PatternPart {
    Static(String),
    Param(String),
}

fn build_router(routes: HashMap<String, Vec<Route>>) -> Router {
    let mut patterns = Vec::with_capacity(routes.len());
    for (pattern, routes) in routes {
        patterns.push(RoutePattern {
            parts: split_pattern(&pattern),
            pattern,
            routes,
        });
    }
    let state = HandlerState {
        patterns: Arc::new(patterns),
    };
    let mut router = Router::new();
    for entry in state.patterns.iter() {
        let handler = any(handle_request);
        router = router.route(&entry.pattern, handler);
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
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let path = uri.path();
    if let Some(permission) = http_rbac_permission() {
        let skip = http_rbac_skip_paths();
        if !path_matches_any(path, &skip) {
            let scope = http_rbac_scope();
            let Some(user_id) = jwt_subject(&headers) else {
                return StatusCode::FORBIDDEN.into_response();
            };
            match rbac_allowed(&user_id, &permission, &scope).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            }
        }
    }
    if http_rate_limit_enabled() {
        let skip = http_rate_limit_skip_paths();
        if !path_matches_any(path, &skip) {
            let key = rate_limit_key(&headers);
            let burst = http_rate_limit_burst();
            let per_secs = http_rate_limit_per_secs();
            match rate_limit_allowed(&key, burst, per_secs).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
                Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            }
        }
    }
    let Some((routes, params_map)) = match_route(path, &state.patterns) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(route) = routes.iter().find(|route| route.method == method).cloned() else {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    };

    let query_map = build_query_map(&uri);
    let headers_map = build_headers_map(&headers);
    let body_val = bytes::bytes_from_slice(&body);

    let req_val =
        match build_request_value(&method, &uri, headers_map, query_map, params_map, body_val) {
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

    apply_default_security_headers(&mut headers);
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
    let mut has_csp = false;
    let mut has_xcto = false;
    let mut has_xfo = false;
    let mut has_referrer = false;
    let mut has_permissions = false;

    let Some(map) = as_map_ref(headers_val) else {
        return (headers, has_content_type, has_server);
    };
    unsafe {
        let mut iter = crate::map::map_iter(map);
        while let Some((key, val)) = iter.next() {
            let Some(name) = string_value(key.0) else {
                continue;
            };
            let Some(value) = string_value(val) else {
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
            if header_name == HeaderName::from_static("content-security-policy") {
                has_csp = true;
            }
            if header_name == HeaderName::from_static("x-content-type-options") {
                has_xcto = true;
            }
            if header_name == HeaderName::from_static("x-frame-options") {
                has_xfo = true;
            }
            if header_name == HeaderName::from_static("referrer-policy") {
                has_referrer = true;
            }
            if header_name == HeaderName::from_static("permissions-policy") {
                has_permissions = true;
            }
            headers.insert(header_name, header_value);
        }
    }

    if !has_csp {
        if let Some(csp) = default_csp() {
            let value = HeaderValue::from_str(&csp).unwrap_or_else(|_| {
                HeaderValue::from_static(
                    "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'",
                )
            });
            headers.insert("content-security-policy", value);
        }
    }
    if !has_xcto {
        headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    }
    if !has_xfo {
        headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    }
    if !has_referrer {
        headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    }
    if !has_permissions {
        headers.insert(
            "permissions-policy",
            HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
        );
    }

    (headers, has_content_type, has_server)
}

fn apply_default_security_headers(headers: &mut HeaderMap) {
    if http_config().hsts_enabled {
        headers.entry("strict-transport-security").or_insert(
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
}

fn default_csp() -> Option<String> {
    if let Some(val) = http_config().csp {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    Some(
        "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'"
            .to_string(),
    )
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
        QUERY_SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            for part in query.split('&') {
                if part.is_empty() {
                    continue;
                }
                let mut iter = part.splitn(2, '=');
                let key = iter.next().unwrap_or("");
                let val = iter.next().unwrap_or("");
                let key_decoded = decode_cow(key, &mut scratch);
                let val_decoded = decode_cow(val, &mut scratch);
                let key_val = string::str_from_bytes(key_decoded.as_bytes());
                let val_val = string::str_from_bytes(val_decoded.as_bytes());
                map_set(query_map, key_val, val_val);
                unsafe {
                    wr_rc_dec(key_val);
                    wr_rc_dec(val_val);
                }
            }
        });
    }
    query_map
}

fn match_route<'a>(
    path: &str,
    patterns: &'a [RoutePattern],
) -> Option<(&'a Vec<Route>, Value)> {
    for entry in patterns {
        let mut params_map: Option<Value> = None;
        let mut matched = true;
        let mut segments = path.trim_matches('/').split('/');
        for part in entry.parts.iter() {
            let Some(actual) = segments.next() else {
                matched = false;
                break;
            };
            match part {
                PatternPart::Static(text) => {
                    if text != actual {
                        matched = false;
                        break;
                    }
                }
                PatternPart::Param(name) => {
                    let map = params_map.get_or_insert_with(map_new);
                    let key_val = string::str_from_bytes(name.as_bytes());
                    let val_val = string::str_from_bytes(actual.as_bytes());
                    map_set(*map, key_val, val_val);
                    unsafe {
                        wr_rc_dec(key_val);
                        wr_rc_dec(val_val);
                    }
                }
            }
        }
        if matched && segments.next().is_some() {
            matched = false;
        }
        if matched {
            let params_map = params_map.unwrap_or_else(map_new);
            return Some((&entry.routes, params_map));
        }
        if let Some(map) = params_map {
            unsafe { wr_rc_dec(map) };
        }
    }
    None
}

#[cfg(test)]
pub(crate) fn bench_build_query_map_once(uri: &Uri) -> Value {
    build_query_map(uri)
}

#[cfg(test)]
pub(crate) fn bench_match_route_once(path: &str) -> bool {
    let route = Route::new(Method::GET, Value::nil(), 0);
    let patterns = vec![RoutePattern {
        pattern: "/users/:id".to_string(),
        parts: split_pattern("/users/:id"),
        routes: vec![route],
    }];
    match_route(path, &patterns).is_some()
}

fn split_pattern(pattern: &str) -> Vec<PatternPart> {
    let mut parts = Vec::new();
    for part in pattern.trim_matches('/').split('/') {
        let param = if let Some(param) = part.strip_prefix(':') {
            Some(param)
        } else {
            part.strip_prefix('{').and_then(|p| p.strip_suffix('}'))
        };
        if let Some(name) = param {
            parts.push(PatternPart::Param(name.to_string()));
        } else {
            parts.push(PatternPart::Static(part.to_string()));
        }
    }
    parts
}

fn decode_cow<'a>(input: &'a str, scratch: &mut Vec<u8>) -> Cow<'a, str> {
    if !needs_decode(input) {
        return Cow::Borrowed(input);
    }
    Cow::Owned(percent_decode_into(input, scratch))
}

thread_local! {
    static QUERY_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

fn needs_decode(input: &str) -> bool {
    input.as_bytes().iter().any(|b| *b == b'%' || *b == b'+')
}

fn percent_decode_into(input: &str, scratch: &mut Vec<u8>) -> String {
    scratch.clear();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = from_hex(bytes[i + 1]);
                let lo = from_hex(bytes[i + 2]);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    scratch.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
                scratch.push(bytes[i]);
            }
            b'+' => scratch.push(b' '),
            b => scratch.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(scratch).to_string()
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

fn path_matches_any(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if path_matches(pattern, path) {
            return true;
        }
    }
    false
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    pattern == path
}

fn http_config_from_value(config: Value) -> HttpConfig {
    let mut out = HttpConfig::default();

    if let Some(token) = config_field_string(config, "auth_token") {
        out.auth_token = Some(token);
    }
    if let Some(enabled) = config_field_bool(config, "auth_jwt_enabled") {
        out.auth_jwt_enabled = enabled;
    }
    if let Some(secret) = config_field_string(config, "jwt_secret") {
        out.jwt_secret = secret;
    }
    if let Some(permission) = config_field_string(config, "rbac_permission") {
        out.rbac_permission = Some(permission);
    }
    if let Some(scope) = config_field_string(config, "rbac_scope") {
        out.rbac_scope = scope;
    }
    let rbac_skip = config_field_string_list(config, "rbac_skip_paths");
    if !rbac_skip.is_empty() {
        out.rbac_skip_paths = rbac_skip;
    }
    if let Some(enabled) = config_field_bool(config, "rate_limit_enabled") {
        out.rate_limit_enabled = enabled;
    }
    if let Some(burst) = config_field_u64(config, "rate_limit_burst") {
        out.rate_limit_burst = burst.max(1);
    }
    if let Some(per_secs) = config_field_u64(config, "rate_limit_per_secs") {
        out.rate_limit_per_secs = per_secs.max(1);
    }
    let rate_skip = config_field_string_list(config, "rate_limit_skip_paths");
    if !rate_skip.is_empty() {
        out.rate_limit_skip_paths = rate_skip;
    }
    if let Some(enabled) = config_field_bool(config, "hsts_enabled") {
        out.hsts_enabled = enabled;
    }
    if let Some(csp) = config_field_string(config, "csp") {
        out.csp = Some(csp);
    }

    out
}

fn config_field_string(config: Value, field: &str) -> Option<String> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = string_value(val).and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_bool(config: Value, field: &str) -> Option<bool> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = if val.is_bool() { Some(val.as_bool()) } else { None };
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = int_value(val).and_then(|num| if num >= 0 { Some(num as u64) } else { None });
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_string_list(config: Value, field: &str) -> Vec<String> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(list) = as_list_ref(val) {
        unsafe {
            for entry in (*list).data.iter() {
                if let Some(text) = string_value(*entry) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_string());
                    }
                }
            }
        }
        unsafe { wr_rc_dec(val) };
        return out;
    }
    if let Some(text) = string_value(val) {
        out.extend(
            text.split(',')
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty()),
        );
    }
    unsafe { wr_rc_dec(val) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class;
    use crate::list;
    use crate::map;
    use crate::string;
    use crate::value;
    use crate::wr_actor_spawn;
    use crate::wr_pending_await;
    use crate::wr_result_is_ok;
    use crate::wr_result_unwrap;
    use crate::storage::config::StorageUserConfig;
    use crate::storage::config::{BackupConfig, BlobConfig, RestoreMode, StorageConfig};
    use crate::wr_rc_dec;
    use crate::wr_register_method;
    use reqwest::Client;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    const CLASS_HTTP_REQUEST: u32 = 200;
    const CLASS_HTTP_RESPONSE: u32 = 201;
    const CLASS_HANDLER: u32 = 202;
    const CLASS_HANDLER_TEXT: u32 = 203;
    const CLASS_HANDLER_PARAMS: u32 = 204;
    const CLASS_HANDLER_ECHO: u32 = 205;
    const CLASS_HANDLER_QUERY: u32 = 206;

    fn with_http_config<F: FnOnce()>(config: HttpConfig, f: F) {
        let prev = http_config();
        set_http_config(config);
        f();
        set_http_config(prev);
    }
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

    fn ensure_storage_configured() {
        static STORAGE_ONCE: OnceLock<()> = OnceLock::new();
        STORAGE_ONCE.get_or_init(|| {
            let dir = tempfile::tempdir().expect("temp dir");
            let path = dir.path().join("wrela-http-tests.db");
            std::mem::forget(dir);
            let user = StorageUserConfig {
                file_path: Some(path.to_string_lossy().to_string()),
                http_enabled: Some(false),
                ..Default::default()
            };
            crate::storage::config::set_storage_user_config(user);
        });
    }

    fn disabled_storage_config() -> StorageConfig {
        StorageConfig {
            enabled: false,
            path: String::new(),
            node_id: 1,
            bind_addr: String::new(),
            http_enabled: false,
            peer_token: None,
            peers: HashMap::new(),
            bootstrap: true,
            snapshot_interval: 1,
            batch_max_ops: 1,
            batch_max_ms: 1,
            queue_cap: 1,
            blob: BlobConfig {
                threshold_bytes: 1,
                file_path: String::new(),
                s3: None,
            },
            backup: BackupConfig {
                enabled: false,
                max_age_secs: 60,
                max_logs: 1,
                retention_days: 1,
                max_keep: 0,
                prefix: "backups".to_string(),
                only_leader: true,
                restore_mode: RestoreMode::Single,
                restore_id: None,
            },
        }
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
        let body_text: &[u8] = if status_code == 200 { b"ok" } else { b"bad" };

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
        class::class_set(
            resp,
            b"status".as_ptr(),
            6,
            Value::from_int(if ok { 200 } else { 400 }),
        );
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
        let expect_x_plain = string::str_from_bytes(b"plain");
        let expect_y_plain = string::str_from_bytes(b"ok");
        let ok = (value::value_eq(x, expect_x) && value::value_eq(y, expect_y))
            || (value::value_eq(x, expect_x_plain) && value::value_eq(y, expect_y_plain));

        let resp = class::class_new(CLASS_HTTP_RESPONSE, std::ptr::null(), std::ptr::null(), 0);
        let body: &[u8] = if ok { b"ok" } else { b"bad" };
        let body_bytes = bytes::bytes_from_slice(body);
        class::class_set(
            resp,
            b"status".as_ptr(),
            6,
            Value::from_int(if ok { 200 } else { 400 }),
        );
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
            wr_rc_dec(expect_x_plain);
            wr_rc_dec(expect_y_plain);
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

        let listener = runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        serve_on_listener(listener)
    }

    fn start_server_with_routes(
        routes: Vec<(Method, &'static str, Value)>,
    ) -> std::net::SocketAddr {
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
        let listener = runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        serve_on_listener(listener)
    }

    fn net_available() -> bool {
        use std::io::ErrorKind;
        use std::sync::OnceLock;

        static AVAIL: OnceLock<bool> = OnceLock::new();
        *AVAIL.get_or_init(|| {
            match runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await }) {
                Ok(listener) => {
                    drop(listener);
                    true
                }
                Err(err) if err.kind() == ErrorKind::PermissionDenied => false,
                Err(err) => panic!("bind: {err}"),
            }
        })
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
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let handler = spawn_handler(CLASS_HANDLER, handle_request);
        let addr = start_server("/users/:id", Method::POST, handler);

        let client = Client::new();
        let url = format!("http://{addr}/users/42?foo=bar");
        let resp = send_with_retry(|| client.post(&url).header("x-test", "ok").body("hello"));

        assert_eq!(resp.status(), StatusCode::OK);
        let echo = resp.headers().get("x-echo").and_then(|v| v.to_str().ok());
        assert_eq!(echo, Some("yes"));
        let server = resp.headers().get("server").and_then(|v| v.to_str().ok());
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
        if !net_available() {
            return;
        }
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
        if !net_available() {
            return;
        }
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
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let handler = spawn_handler(CLASS_HANDLER_ECHO, handle_echo);
        let addr = start_server("/echo", Method::POST, handler);

        let client = Client::new();
        let body = vec![0, 1, 2, 3, 4, 255];
        let resp = send_with_retry(|| {
            client
                .post(format!("http://{addr}/echo"))
                .body(body.clone())
        });
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
        if !net_available() {
            return;
        }
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
        let url_plain = format!("http://{addr}/query?x=plain&y=ok");
        let resp_plain = send_with_retry(|| client.get(&url_plain));
        assert_eq!(resp_plain.status(), StatusCode::OK);

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
        if !net_available() {
            return;
        }
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
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        let route_val = string::str_from_bytes(b"/invalid");
        serve_get_requests(route_val, Value::nil());
        unsafe { wr_rc_dec(route_val) };

        let listener = runtime_block_on(async { TcpListener::bind("127.0.0.1:0").await.unwrap() });
        let addr = serve_on_listener(listener);

        let client = Client::new();
        let resp = send_with_retry(|| client.get(format!("http://{addr}/invalid")));
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        stop();
        std::thread::sleep(Duration::from_millis(50));
    }

    #[test]
    fn http_server_pool_fan_out() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();

        wr_register_method(CLASS_HANDLER_POOL, METHOD_HANDLE, handle_pool);
        let instance_a = string::str_from_bytes(b"a");
        let actor_a = wr_actor_spawn(CLASS_HANDLER_POOL.into(), instance_a, 1, 3, -1, -1, -1);
        let instance_b = string::str_from_bytes(b"b");
        let actor_b = wr_actor_spawn(CLASS_HANDLER_POOL.into(), instance_b, 1, 3, -1, -1, -1);
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
        if !net_available() {
            return;
        }
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
        if !net_available() {
            return;
        }
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
        let server = resp.headers().get("server").and_then(|v| v.to_str().ok());
        assert_eq!(server, Some("Custom"));
        let csp = resp
            .headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok());
        assert!(csp.is_some());
        let xfo = resp
            .headers()
            .get("x-frame-options")
            .and_then(|v| v.to_str().ok());
        assert_eq!(xfo, Some("DENY"));
        let xcto = resp
            .headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok());
        assert_eq!(xcto, Some("nosniff"));
        let referrer = resp
            .headers()
            .get("referrer-policy")
            .and_then(|v| v.to_str().ok());
        assert_eq!(referrer, Some("no-referrer"));
        let perms = resp
            .headers()
            .get("permissions-policy")
            .and_then(|v| v.to_str().ok());
        assert!(perms.is_some());

        stop();
        std::thread::sleep(Duration::from_millis(50));
        unsafe {
            wr_rc_dec(handler);
        }
    }

    #[test]
    fn http_server_auth_token() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let mut cfg = HttpConfig::default();
        cfg.auth_token = Some("token-123".to_string());
        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/secure", Method::GET, handler);

            let client = Client::new();
            let url = format!("http://{addr}/secure");
            let resp = send_with_retry(|| client.get(&url));
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

            let resp =
                send_with_retry(|| client.get(&url).header("authorization", "Bearer token-123"));
            assert_eq!(resp.status(), StatusCode::OK);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_server_auth_jwt() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let mut cfg = HttpConfig::default();
        cfg.auth_jwt_enabled = true;
        cfg.jwt_secret = "test-secret".to_string();
        #[derive(serde::Serialize)]
        struct Claims {
            exp: usize,
        }
        with_http_config(cfg, || {
            let header = jsonwebtoken::Header::default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let claims = Claims { exp: (now + 60) as usize };
            let key = jsonwebtoken::EncodingKey::from_secret(b"test-secret");
            let token = jsonwebtoken::encode(&header, &claims, &key).expect("token");

            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/secure-jwt", Method::GET, handler);

            let client = Client::new();
            let url = format!("http://{addr}/secure-jwt");
            let resp = send_with_retry(|| client.get(&url));
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

            let resp = send_with_retry(|| {
                client.get(&url).header("authorization", format!("Bearer {token}"))
            });
            assert_eq!(resp.status(), StatusCode::OK);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_server_csp_override_and_hsts() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        let mut cfg = HttpConfig::default();
        cfg.csp = Some("default-src 'none'".to_string());
        cfg.hsts_enabled = true;
        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/csp", Method::GET, handler);

            let client = Client::new();
            let resp = send_with_retry(|| client.get(format!("http://{addr}/csp")));
            assert_eq!(resp.status(), StatusCode::OK);
            let csp = resp
                .headers()
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok());
            assert_eq!(csp, Some("default-src 'none'"));
            let hsts = resp
                .headers()
                .get("strict-transport-security")
                .and_then(|v| v.to_str().ok());
            assert!(hsts.is_some());

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_server_rbac_permission() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        ensure_storage_configured();
        let mut cfg = HttpConfig::default();
        cfg.auth_jwt_enabled = true;
        cfg.jwt_secret = "rbac-secret".to_string();
        cfg.rbac_permission = Some("admin".to_string());
        cfg.rbac_scope = "scope-a".to_string();

        let storage = Value::from_int(1);
        let scope = string::str_from_bytes(b"scope-a");
        let role_name = string::str_from_bytes(b"admin-role");
        let permissions = list::list_new(0);
        let perm = string::str_from_bytes(b"admin");
        list::list_push(permissions, perm);
        let role_pending = crate::wr_rbac_create_role(storage, scope, role_name, permissions);
        let role_id = await_ok(role_pending);

        let user_id = string::str_from_bytes(b"user-1");
        let assign_pending = crate::wr_rbac_assign_role(storage, user_id, role_id, scope);
        let assigned = await_ok(assign_pending);
        assert!(assigned.is_bool());
        assert!(assigned.as_bool());

        #[derive(serde::Serialize)]
        struct Claims {
            exp: usize,
            sub: String,
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let header = jsonwebtoken::Header::default();
        let key = jsonwebtoken::EncodingKey::from_secret(b"rbac-secret");
        let token_user1 = jsonwebtoken::encode(
            &header,
            &Claims {
                exp: (now + 60) as usize,
                sub: "user-1".to_string(),
            },
            &key,
        )
        .expect("token user1");
        let token_user2 = jsonwebtoken::encode(
            &header,
            &Claims {
                exp: (now + 60) as usize,
                sub: "user-2".to_string(),
            },
            &key,
        )
        .expect("token user2");

        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/secure-rbac", Method::GET, handler);

            let client = Client::new();
            let url = format!("http://{addr}/secure-rbac");
            let resp = send_with_retry(|| client.get(&url));
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

            let resp = send_with_retry(|| {
                client
                    .get(&url)
                    .header("authorization", format!("Bearer {token_user2}"))
            });
            assert_eq!(resp.status(), StatusCode::FORBIDDEN);

            let resp = send_with_retry(|| {
                client
                    .get(&url)
                    .header("authorization", format!("Bearer {token_user1}"))
            });
            assert_eq!(resp.status(), StatusCode::OK);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });

        unsafe {
            wr_rc_dec(scope);
            wr_rc_dec(role_name);
            wr_rc_dec(permissions);
            wr_rc_dec(perm);
            wr_rc_dec(role_pending);
            wr_rc_dec(role_id);
            wr_rc_dec(user_id);
            wr_rc_dec(assign_pending);
            wr_rc_dec(assigned);
        }
    }

    #[test]
    fn http_server_rate_limit_blocks() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        ensure_storage_configured();
        let mut cfg = HttpConfig::default();
        cfg.rate_limit_enabled = true;
        cfg.rate_limit_burst = 1;
        cfg.rate_limit_per_secs = 60;
        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/limited", Method::GET, handler);
            let client = Client::new();
            let url = format!("http://{addr}/limited");
            let ip = format!("test-{}", uuid::Uuid::new_v4());
            let resp = send_with_retry(|| client.get(&url).header("x-real-ip", ip.clone()));
            assert_eq!(resp.status(), StatusCode::OK);

            let resp = send_with_retry(|| client.get(&url).header("x-real-ip", ip.clone()));
            assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_server_rbac_skip_paths() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        ensure_storage_configured();
        let mut cfg = HttpConfig::default();
        cfg.auth_jwt_enabled = true;
        cfg.jwt_secret = "skip-secret".to_string();
        cfg.rbac_permission = Some("admin".to_string());
        cfg.rbac_scope = "scope-skip".to_string();
        cfg.rbac_skip_paths = vec!["/public".to_string()];
        #[derive(serde::Serialize)]
        struct Claims {
            exp: usize,
            sub: String,
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let header = jsonwebtoken::Header::default();
        let key = jsonwebtoken::EncodingKey::from_secret(b"skip-secret");
        let token = jsonwebtoken::encode(
            &header,
            &Claims {
                exp: (now + 60) as usize,
                sub: "user-no-role".to_string(),
            },
            &key,
        )
        .expect("token");

        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/public", Method::GET, handler);

            let client = Client::new();
            let url = format!("http://{addr}/public");
            let resp = send_with_retry(|| {
                client
                    .get(&url)
                    .header("authorization", format!("Bearer {token}"))
            });
            assert_eq!(resp.status(), StatusCode::OK);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_server_rate_limit_skip_paths() {
        if !net_available() {
            return;
        }
        let _lock = test_lock();
        reset_registry();
        register_http_classes();
        ensure_storage_configured();
        let mut cfg = HttpConfig::default();
        cfg.rate_limit_enabled = true;
        cfg.rate_limit_burst = 1;
        cfg.rate_limit_per_secs = 60;
        cfg.rate_limit_skip_paths = vec!["/open".to_string()];
        with_http_config(cfg, || {
            let handler = spawn_handler(CLASS_HANDLER_TEXT, handle_text);
            let addr = start_server("/open", Method::GET, handler);
            let client = Client::new();
            let url = format!("http://{addr}/open");
            let ip = format!("skip-{}", uuid::Uuid::new_v4());

            let resp = send_with_retry(|| client.get(&url).header("x-real-ip", ip.clone()));
            assert_eq!(resp.status(), StatusCode::OK);
            let resp = send_with_retry(|| client.get(&url).header("x-real-ip", ip.clone()));
            assert_eq!(resp.status(), StatusCode::OK);

            stop();
            std::thread::sleep(Duration::from_millis(50));
            unsafe {
                wr_rc_dec(handler);
            }
        });
    }

    #[test]
    fn http_storage_outage_denies_rbac() {
        let cfg = disabled_storage_config();
        crate::actor::runtime_block_on(async move {
            crate::storage::config::with_storage_config_override(cfg, async {
                let res = rbac_allowed("user", "perm", "scope").await;
                assert!(res.is_err());
            })
            .await;
        });
    }

    #[test]
    fn http_storage_outage_denies_rate_limit() {
        let cfg = disabled_storage_config();
        crate::actor::runtime_block_on(async move {
            crate::storage::config::with_storage_config_override(cfg, async {
                let res = rate_limit_allowed("ip", 1, 60).await;
                assert!(res.is_err());
            })
            .await;
        });
    }
}

fn string_value(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).to_string())
}
