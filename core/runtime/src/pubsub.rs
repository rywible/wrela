use crate::actor::runtime_spawn;
use crate::metrics;
use crate::storage::config::storage_config;
use crate::storage_helpers::{storage_get_json_result, storage_set_json_result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::RwLock;

type Handler =
    Arc<dyn Fn(JsonValue) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PubSubMessage {
    pub topic: String,
    pub payload: JsonValue,
}

static SUBSCRIBERS: OnceLock<Arc<RwLock<HashMap<String, Vec<Handler>>>>> = OnceLock::new();
static CLIENT: OnceLock<Client> = OnceLock::new();
static DLQ_WORKER: OnceLock<()> = OnceLock::new();

fn subscribers() -> Arc<RwLock<HashMap<String, Vec<Handler>>>> {
    SUBSCRIBERS
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

fn client() -> Client {
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_millis(500))
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_else(|_| Client::new())
        })
        .clone()
}

fn peer_token() -> Option<String> {
    storage_config().peer_token
}

fn dlq_retry_ms() -> u64 {
    std::env::var("WRELA_PUBSUB_DLQ_RETRY_MS")
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(2000)
        .max(50)
}

fn dlq_max_len() -> usize {
    std::env::var("WRELA_PUBSUB_DLQ_MAX")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(1000)
        .max(1)
}

fn sanitize_peer_key(addr: &str) -> String {
    addr.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn dlq_key_for_peer(addr: &str) -> String {
    format!("pubsub:dlq:{}", sanitize_peer_key(addr))
}

async fn try_publish_to_peer(addr: &str, message: &PubSubMessage, token: Option<&str>) -> bool {
    let url = format!("http://{addr}/pubsub/publish");
    let client = client();
    for attempt in 0..3 {
        let mut request = client.post(&url).json(message);
        if let Some(token) = token {
            request = request.header("x-wrela-peer-token", token);
        }
        match request.send().await {
            Ok(resp) if resp.status().is_success() => return true,
            _ => {
                if attempt < 2 {
                    let backoff = 50 * (attempt + 1) as u64;
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
            }
        }
    }
    false
}

async fn dlq_enqueue(addr: &str, message: PubSubMessage) {
    let key = dlq_key_for_peer(addr);
    let mut queue = match storage_get_json_result::<Vec<PubSubMessage>>(&key).await {
        Ok(Some(items)) => items,
        Ok(None) => Vec::new(),
        Err(_) => return,
    };
    queue.push(message);
    let max_len = dlq_max_len();
    if queue.len() > max_len {
        let extra = queue.len() - max_len;
        queue.drain(0..extra);
    }
    let _ = storage_set_json_result(&key, &queue).await;
}

async fn dlq_drain_peer(addr: &str) {
    let key = dlq_key_for_peer(addr);
    let Some(queue) = storage_get_json_result::<Vec<PubSubMessage>>(&key)
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    if queue.is_empty() {
        return;
    }
    let token = peer_token();
    let mut remaining = Vec::new();
    for message in queue {
        if !try_publish_to_peer(addr, &message, token.as_deref()).await {
            remaining.push(message);
        }
    }
    let _ = storage_set_json_result(&key, &remaining).await;
}

fn ensure_dlq_worker_started() {
    DLQ_WORKER.get_or_init(|| {
        tokio::spawn(async {
            loop {
                let peers = storage_config().peers;
                if !peers.is_empty() {
                    for addr in peers.values() {
                        dlq_drain_peer(addr).await;
                    }
                }
                tokio::time::sleep(Duration::from_millis(dlq_retry_ms())).await;
            }
        });
    });
}

pub async fn subscribe<F, Fut>(topic: &str, handler: F)
where
    F: Fn(JsonValue) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let handler: Handler = Arc::new(move |payload| Box::pin(handler(payload)));
    let subs = subscribers();
    let mut guard = subs.write().await;
    guard.entry(topic.to_string()).or_default().push(handler);
}

async fn deliver_local(topic: &str, payload: JsonValue) {
    let handlers = {
        let subs = subscribers();
        let guard = subs.read().await;
        guard.get(topic).cloned()
    };
    if let Some(handlers) = handlers {
        for handler in handlers {
            let payload = payload.clone();
            runtime_spawn(async move {
                (handler)(payload).await;
            });
        }
    }
}

pub async fn publish(topic: &str, payload: JsonValue) {
    deliver_local(topic, payload.clone()).await;
    let peers = storage_config().peers;
    if peers.is_empty() {
        return;
    }
    ensure_dlq_worker_started();
    #[cfg(feature = "metrics")]
    metrics::inc_pubsub_publish();
    let message = PubSubMessage {
        topic: topic.to_string(),
        payload,
    };
    let token = peer_token();
    for addr in peers.values().cloned() {
        let message = message.clone();
        let token = token.clone();
        tokio::spawn(async move {
            let success = try_publish_to_peer(&addr, &message, token.as_deref()).await;
            if !success {
                dlq_enqueue(&addr, message).await;
                #[cfg(feature = "metrics")]
                metrics::inc_pubsub_publish_failure();
            }
        });
    }
}

pub async fn handle_publish(message: PubSubMessage) {
    deliver_local(&message.topic, message.payload).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::Json;
    use std::collections::HashMap;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use crate::storage::config::{BackupConfig, BlobConfig, RestoreMode, StorageConfig};
    use crate::storage::service::StorageService;
    use crate::storage_helpers::storage_get_json_result;

    fn net_available() -> bool {
        use std::io::ErrorKind;
        match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => {
                drop(listener);
                true
            }
            Err(err) if err.kind() == ErrorKind::PermissionDenied => false,
            Err(err) => panic!("bind failed: {err}"),
        }
    }

    async fn pick_free_addr() -> Option<String> {
        match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => {
                let addr = listener.local_addr().ok()?.to_string();
                drop(listener);
                Some(addr)
            }
            Err(_) => None,
        }
    }

    async fn start_peer_server(addr: &str) -> oneshot::Sender<()> {
        let listener = TcpListener::bind(addr).await.expect("bind peer server");
        let app = axum::Router::new().route(
            "/pubsub/publish",
            post(|Json(_msg): Json<PubSubMessage>| async { axum::http::StatusCode::OK }),
        );
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        tx
    }

    fn storage_config_for(path: String, peer_addr: String) -> StorageConfig {
        let blob_path = format!("{path}.blobs");
        let mut peers = HashMap::new();
        peers.insert(2, peer_addr);
        StorageConfig {
            enabled: true,
            path,
            node_id: 1,
            bind_addr: "127.0.0.1:0".to_string(),
            http_enabled: false,
            peer_token: None,
            peers,
            bootstrap: true,
            snapshot_interval: 50,
            batch_max_ops: 2,
            batch_max_ms: 1,
            queue_cap: 32,
            blob: BlobConfig {
                threshold_bytes: 256 * 1024,
                file_path: blob_path,
                s3: None,
            },
            backup: BackupConfig {
                enabled: false,
                max_age_secs: 3600,
                max_logs: 100_000,
                retention_days: 7,
                max_keep: 0,
                prefix: "backups".to_string(),
                only_leader: true,
                restore_mode: RestoreMode::Single,
                restore_id: None,
            },
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pubsub_dlq_retries_after_peer_returns() {
        if !net_available() {
            return;
        }
        let Some(peer_addr) = pick_free_addr().await else {
            eprintln!("skipping: unable to bind sockets in this environment");
            return;
        };
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("wrela_pubsub.db");
        let cfg = storage_config_for(path.to_string_lossy().to_string(), peer_addr.clone());

        let service = StorageService::start_for_test(cfg.clone())
            .await
            .expect("start storage");
        let service = Arc::new(service);

        unsafe {
            std::env::set_var("WRELA_PUBSUB_DLQ_RETRY_MS", "50");
        }

        crate::storage::service::StorageService::with_storage_override(
            Arc::clone(&service),
            crate::storage::config::with_storage_config_override(cfg, async move {
                publish("dlq:test", JsonValue::String("ping".to_string())).await;
                let key = dlq_key_for_peer(&peer_addr);
                let mut tries = 0u32;
                loop {
                    if let Ok(Some(queue)) =
                        storage_get_json_result::<Vec<PubSubMessage>>(&key).await
                    {
                        if !queue.is_empty() {
                            break;
                        }
                    }
                    tries += 1;
                    if tries > 40 {
                        panic!("timed out waiting for dlq enqueue");
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }

                let shutdown = start_peer_server(&peer_addr).await;
                let mut drained = false;
                for _ in 0..80 {
                    let queue = storage_get_json_result::<Vec<PubSubMessage>>(&key)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    if queue.is_empty() {
                        drained = true;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                let _ = shutdown.send(());
                assert!(drained, "dlq did not drain");
            }),
        )
        .await;

        unsafe {
            std::env::remove_var("WRELA_PUBSUB_DLQ_RETRY_MS");
        }

        let service = match Arc::try_unwrap(service) {
            Ok(service) => service,
            Err(_) => panic!("storage service refs"),
        };
        service.shutdown().await;
    }
}
