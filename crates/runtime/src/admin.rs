use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::map;
use crate::metrics;
use crate::string;
use crate::value::Value;
use crate::wr_rc_dec;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::net::TcpListener;

#[derive(Clone)]
struct AdminConfig {
    auth_token: Option<String>,
}

struct AdminState {
    started: bool,
}

static STATE: OnceLock<Mutex<AdminState>> = OnceLock::new();

fn admin_state() -> &'static Mutex<AdminState> {
    STATE.get_or_init(|| Mutex::new(AdminState { started: false }))
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn map_get_string(map_val: Value, key: &str) -> Option<String> {
    let key_val = string::str_from_bytes(key.as_bytes());
    let got = map::map_get(map_val, key_val);
    unsafe { wr_rc_dec(key_val) };
    if got.is_nil() {
        return None;
    }
    let out = value_to_string(got);
    unsafe { wr_rc_dec(got) };
    out
}

async fn metrics_handler(
    State(config): State<Arc<AdminConfig>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = &config.auth_token {
        let auth_ok = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .map(|v| v == format!("Bearer {token}"))
            .unwrap_or(false);
        if !auth_ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    let body = json!({
        "messages_sent": metrics::metrics_get_raw(metrics::METRIC_MESSAGES_SENT),
        "messages_dropped": metrics::metrics_get_raw(metrics::METRIC_MESSAGES_DROPPED),
        "pending_resolved": metrics::metrics_get_raw(metrics::METRIC_PENDING_RESOLVED),
        "storage_backup_success": metrics::metrics_get_raw(metrics::METRIC_STORAGE_BACKUP_SUCCESS),
        "storage_backup_failure": metrics::metrics_get_raw(metrics::METRIC_STORAGE_BACKUP_FAILURE),
        "pubsub_publish": metrics::metrics_get_raw(metrics::METRIC_PUBSUB_PUBLISH),
        "pubsub_publish_failure": metrics::metrics_get_raw(metrics::METRIC_PUBSUB_PUBLISH_FAILURE),
        "sched_wakeups": metrics::metrics_get_raw(metrics::METRIC_SCHED_WAKEUPS),
        "jobs_wakeups": metrics::metrics_get_raw(metrics::METRIC_JOBS_WAKEUPS),
    });
    Json(body).into_response()
}

async fn health_handler() -> impl IntoResponse {
    StatusCode::OK
}

pub fn admin_enable(opts: Value) -> Value {
    let (pending, state) = pending_new();
    let bind_addr =
        map_get_string(opts, "bind_addr").unwrap_or_else(|| "127.0.0.1:9090".to_string());
    let auth = map_get_string(opts, "auth");
    runtime_spawn(async move {
        let should_start = {
            let mut guard = admin_state().lock().expect("admin state lock");
            if guard.started {
                false
            } else {
                guard.started = true;
                true
            }
        };
        if !should_start {
            resolve_pending(state, Value::from_bool(true));
            return;
        }
        let config = Arc::new(AdminConfig { auth_token: auth });
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .with_state(config);
        let listener = match TcpListener::bind(&bind_addr).await {
            Ok(listener) => listener,
            Err(_) => match TcpListener::bind("127.0.0.1:0").await {
                Ok(listener) => listener,
                Err(_) => {
                    resolve_pending(state, Value::from_bool(false));
                    return;
                }
            },
        };
        resolve_pending(state, Value::from_bool(true));
        let _ = axum::serve(listener, app).await;
    });
    pending
}
