use openraft::BasicNode;
use openraft::Config;
use openraft::Raft;
use openraft::RaftTypeConfig;
use openraft::SnapshotPolicy;
use openraft::storage::Adaptor;
use std::collections::BTreeMap;
use std::fmt;
#[cfg(any(test, feature = "test-utils"))]
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::time::sleep;

use crate::diagnostics;
use crate::list;
use crate::map;
use crate::metrics;
use crate::string;
use crate::value::Value;

use super::backup::{BackupSink, BackupState, latest_backup_key, should_restore, verify_checksum};
use super::blob::BlobBackend;
use super::config::{StorageConfig, batch_max_delay, storage_config};
use super::store::{KvCommand, KvRequest, KvResponse, KvStore, NodeId, TypeConfig};
use super::transport::{
    HttpNetworkFactory, NullNetworkFactory, RpcEnvelope, StoragePrefixRequest,
    StoragePrefixResponse, StorageReadRequest, StorageReadResponse, StorageReadVersionResponse,
    StorageScanRequest, StorageScanResponse, start_http_server,
};
use super::value::{StoredRecord, StoredValue};

#[derive(Debug)]
pub enum StorageError {
    Disabled,
    InitFailed(&'static str),
    QueueFull,
    Closed,
    Internal(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Disabled => write!(f, "storage disabled"),
            StorageError::InitFailed(msg) => write!(f, "storage init failed: {msg}"),
            StorageError::QueueFull => write!(f, "storage queue full"),
            StorageError::Closed => write!(f, "storage closed"),
            StorageError::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

pub enum StorageRequest {
    Get {
        key: Vec<u8>,
    },
    GetBytes {
        key: Vec<u8>,
    },
    GetWithVersion {
        key: Vec<u8>,
    },
    Scan {
        start: Option<Vec<u8>>,
        end: Option<Vec<u8>>,
        limit: usize,
    },
    ListPrefix {
        prefix: Vec<u8>,
        limit: usize,
    },
    Put {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        key: Vec<u8>,
    },
    CompareAndSet {
        key: Vec<u8>,
        expected_version: Option<u64>,
        value: Option<Vec<u8>>,
    },
    BatchSet {
        items: Vec<(Vec<u8>, Vec<u8>)>,
    },
}

pub enum StorageResponse {
    Ok(Value),
    Bytes(Option<Vec<u8>>),
    Err(String),
}

struct Envelope {
    req: StorageRequest,
    resp: oneshot::Sender<StorageResponse>,
}

struct WriteItem {
    cmd: KvCommand,
    resp: oneshot::Sender<StorageResponse>,
}

pub struct StorageService {
    sender: mpsc::Sender<Envelope>,
    raft: Raft<TypeConfig>,
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    store: Arc<KvStore>,
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    blob: BlobBackend,
    http_handle: tokio::task::JoinHandle<()>,
    backup_handle: tokio::task::JoinHandle<()>,
    #[cfg_attr(not(any(test, feature = "test-utils")), allow(dead_code))]
    peers: std::collections::HashMap<NodeId, String>,
}

static STORAGE: std::sync::OnceLock<StorageService> = std::sync::OnceLock::new();
static STORAGE_INIT_LOCK: std::sync::OnceLock<AsyncMutex<()>> = std::sync::OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    static STORAGE_OVERRIDE: Arc<StorageService>;
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn capture_storage_override() -> Option<Arc<StorageService>> {
    STORAGE_OVERRIDE.try_with(Arc::clone).ok()
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) async fn with_storage_override_if_present<F, R>(
    service: Option<Arc<StorageService>>,
    fut: F,
) -> R
where
    F: Future<Output = R>,
{
    if let Some(service) = service {
        STORAGE_OVERRIDE.scope(service, fut).await
    } else {
        fut.await
    }
}

impl StorageService {
    pub async fn dispatch(req: StorageRequest) -> Result<StorageResponse, StorageError> {
        #[cfg(any(test, feature = "test-utils"))]
        if let Ok(service) = STORAGE_OVERRIDE.try_with(Arc::clone) {
            return service.dispatch_to(req).await;
        }
        let service = Self::get_or_init().await?;
        service.dispatch_to(req).await
    }

    pub async fn dispatch_to(&self, req: StorageRequest) -> Result<StorageResponse, StorageError> {
        let (tx, rx) = oneshot::channel();
        match self.sender.try_send(Envelope { req, resp: tx }) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => return Err(StorageError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(StorageError::Closed),
        }
        rx.await.map_err(|_| StorageError::Closed)
    }

    pub async fn shutdown(self) {
        let _ = self.http_handle.abort();
        let _ = self.backup_handle.abort();
        let _ = self.raft.shutdown().await;
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn with_storage_override<F, R>(service: Arc<StorageService>, fut: F) -> R
    where
        F: Future<Output = R>,
    {
        STORAGE_OVERRIDE.scope(service, fut).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn raft_ref(&self) -> &Raft<TypeConfig> {
        &self.raft
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn local_get(&self, key: &[u8]) -> Option<Vec<u8>> {
        read_local_value(&self.store, &self.blob, key)
            .await
            .ok()
            .flatten()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn log_len(&self) -> usize {
        self.store.log_len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn forward_read_for_test(&self, key: Vec<u8>) -> StorageResponse {
        let resp =
            read_linearizable_value(&self.raft, &self.store, &self.blob, key, Some(&self.peers))
                .await;
        match resp {
            Ok(Some(bytes)) => StorageResponse::Ok(string::str_from_bytes(&bytes)),
            Ok(None) => StorageResponse::Ok(Value::nil()),
            Err(err) => StorageResponse::Err(err.to_string()),
        }
    }

    async fn get_or_init() -> Result<&'static StorageService, StorageError> {
        if let Some(service) = STORAGE.get() {
            return Ok(service);
        }
        let lock = STORAGE_INIT_LOCK.get_or_init(|| AsyncMutex::new(()));
        let _guard = lock.lock().await;
        if let Some(service) = STORAGE.get() {
            return Ok(service);
        }
        let config = storage_config();
        if !config.enabled {
            return Err(StorageError::Disabled);
        }
        let service = Self::start(config, None, None).await?;
        let _ = STORAGE.set(service);
        STORAGE
            .get()
            .ok_or(StorageError::InitFailed("failed to store service"))
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn start_for_test(config: StorageConfig) -> Result<Self, StorageError> {
        Self::start(config, None, None).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn start_for_test_with_listener(
        config: StorageConfig,
        listener: TcpListener,
    ) -> Result<Self, StorageError> {
        Self::start(config, Some(listener), None).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[allow(dead_code)]
    pub(crate) async fn start_for_test_with_listener_and_state(
        config: StorageConfig,
        listener: TcpListener,
        realtime_state: crate::realtime::RealtimeStateHandleArc,
    ) -> Result<Self, StorageError> {
        Self::start(config, Some(listener), Some(realtime_state)).await
    }

    async fn start(
        mut config: StorageConfig,
        listener: Option<TcpListener>,
        realtime_state: Option<crate::realtime::RealtimeStateHandleArc>,
    ) -> Result<Self, StorageError> {
        let (listener, bind_addr_override) = if config.http_enabled {
            let listener = match listener {
                Some(listener) => listener,
                None => TcpListener::bind(&config.bind_addr).await.map_err(|err| {
                    StorageError::Internal(format!("failed to bind raft http: {err}"))
                })?,
            };
            let bind_addr = listener
                .local_addr()
                .map_err(|err| {
                    StorageError::Internal(format!("failed to read raft http addr: {err}"))
                })?
                .to_string();
            (Some(listener), Some(bind_addr))
        } else {
            (None, None)
        };

        if let Some(addr) = bind_addr_override {
            config.bind_addr = addr;
        }

        let blob = BlobBackend::from_config(&config.blob)
            .await
            .map_err(|err| StorageError::Internal(err.to_string()))?;
        let backup_state = Arc::new(BackupState::new());
        let backup_sink = if config.backup.enabled {
            Some(Arc::new(BackupSink::new(
                blob.clone(),
                config.backup.clone(),
                backup_state.clone(),
                config.node_id,
            )))
        } else {
            None
        };
        let store = KvStore::new(&config.path, blob.clone(), backup_sink).await;

        maybe_restore_from_backup(&store, &blob, &config)
            .await
            .map_err(|err| StorageError::Internal(err.to_string()))?;

        let mut peers = config.peers.clone();
        peers.remove(&config.node_id);
        let peers = Arc::new(peers);
        config.peers = (*peers).clone();

        let mut raft_config = Config {
            cluster_name: "wrela".to_string(),
            ..Default::default()
        };
        raft_config.snapshot_policy = SnapshotPolicy::LogsSinceLast(config.snapshot_interval);
        let raft_config = Arc::new(
            raft_config
                .validate()
                .map_err(|_| StorageError::InitFailed("invalid raft config"))?,
        );

        let (log_store, state_machine) = Adaptor::new(store.clone());
        let raft = if config.http_enabled {
            let network = HttpNetworkFactory::new((*peers).clone(), config.peer_token.clone());
            Raft::new(
                config.node_id,
                raft_config,
                network,
                log_store,
                state_machine,
            )
            .await
            .map_err(|_| StorageError::InitFailed("failed to create raft"))?
        } else {
            let network = NullNetworkFactory::default();
            Raft::new(
                config.node_id,
                raft_config,
                network,
                log_store,
                state_machine,
            )
            .await
            .map_err(|_| StorageError::InitFailed("failed to create raft"))?
        };

        let http_handle = if let Some(listener) = listener {
            let realtime = realtime_state.unwrap_or_else(crate::realtime::realtime_state_shared);
            let (_addr, handle) = start_http_server(
                listener,
                raft.clone(),
                store.clone(),
                blob.clone(),
                realtime,
                config.peer_token.clone(),
            )
            .await?;
            handle
        } else {
            tokio::spawn(async {})
        };

        let backup_handle =
            start_backup_task(raft.clone(), store.clone(), backup_state, config.clone());

        bootstrap_or_join(&raft, &config).await?;

        let (sender, receiver) = mpsc::channel(config.queue_cap);
        let service_peers = config.peers.clone();
        crate::actor::runtime_spawn(run_loop(
            raft.clone(),
            store.clone(),
            blob.clone(),
            receiver,
            config,
        ));
        diagnostics::log_event("storage: raft service started");
        Ok(StorageService {
            sender,
            raft,
            store,
            blob,
            http_handle,
            backup_handle,
            peers: service_peers,
        })
    }
}

async fn maybe_restore_from_backup(
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    config: &StorageConfig,
) -> Result<(), String> {
    if !should_restore(&config.backup) {
        return Ok(());
    }
    let has_state = store.has_state().map_err(|err| err.to_string())?;
    if has_state {
        diagnostics::log_event("storage: restore skipped; state exists");
        return Ok(());
    }
    let key = if let Some(id) = config.backup.restore_id.clone() {
        id
    } else {
        let Some(key) = latest_backup_key(blob, &config.backup, config.node_id).await else {
            diagnostics::log_event("storage: restore skipped; no backups found");
            return Ok(());
        };
        key
    };
    let bytes = match blob.get_named(&key).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if config.backup.restore_id.is_some() {
                crate::metrics::inc_storage_backup_restore_failure();
                return Err(err.to_string());
            }
            diagnostics::log_event("storage: restore skipped; failed to load backup");
            return Ok(());
        }
    };
    if let Err(err) = verify_checksum(blob, &key, &bytes, config.backup.restore_id.is_some()).await
    {
        if config.backup.restore_id.is_some() {
            crate::metrics::inc_storage_backup_restore_failure();
            return Err(err);
        }
        diagnostics::log_event("storage: restore skipped; checksum mismatch");
        return Ok(());
    }
    if let Err(err) = store.restore_from_backup(key, bytes).await {
        if config.backup.restore_id.is_some() {
            crate::metrics::inc_storage_backup_restore_failure();
            return Err(err.to_string());
        }
        diagnostics::log_event("storage: restore skipped; failed to apply backup");
        return Ok(());
    }
    diagnostics::log_event("storage: restored from backup");
    Ok(())
}

fn start_backup_task(
    raft: Raft<TypeConfig>,
    store: Arc<KvStore>,
    backup_state: Arc<BackupState>,
    config: StorageConfig,
) -> tokio::task::JoinHandle<()> {
    if !config.backup.enabled {
        return tokio::spawn(async {});
    }
    let node_id = config.node_id;
    let max_age_secs = config.backup.max_age_secs;
    let max_logs = config.backup.max_logs;
    let only_leader = config.backup.only_leader;
    tokio::spawn(async move {
        let tick = Duration::from_secs(max_age_secs.clamp(1, 30));
        loop {
            sleep(tick).await;
            if only_leader {
                let leader = raft.current_leader().await;
                backup_state.set_leader_id(leader);
                if leader != Some(node_id) {
                    continue;
                }
            } else {
                backup_state.set_leader_id(raft.current_leader().await);
            }
            if backup_state.snapshot_inflight() {
                continue;
            }
            let metrics = raft.data_metrics();
            let (snapshot_index, last_applied) = {
                let data = metrics.borrow();
                let snap = data.snapshot.map(|id| id.index).unwrap_or(0);
                let last = data.last_applied.map(|id| id.index).unwrap_or(0);
                (snap, last)
            };
            if backup_state.last_snapshot_index() == 0 && snapshot_index > 0 {
                backup_state.set_last_snapshot_index(snapshot_index);
            }
            if last_applied <= snapshot_index {
                continue;
            }
            let progressed = last_applied > backup_state.last_snapshot_index();
            if !progressed {
                continue;
            }
            let last = backup_state.last_backup_at();
            let age = now_secs().saturating_sub(last);
            let age_trigger = age >= max_age_secs;
            let log_trigger = store.log_len() >= max_logs;
            if age_trigger || log_trigger {
                backup_state.set_snapshot_inflight(true);
                if raft.trigger().snapshot().await.is_err() {
                    backup_state.set_snapshot_inflight(false);
                }
            }
        }
    })
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

async fn bootstrap_or_join(
    raft: &Raft<TypeConfig>,
    config: &StorageConfig,
) -> Result<(), StorageError> {
    let is_initialized = raft
        .is_initialized()
        .await
        .map_err(|_| StorageError::InitFailed("failed to read raft state"))?;
    if is_initialized {
        return Ok(());
    }

    if config.bootstrap {
        let mut members: BTreeMap<<TypeConfig as RaftTypeConfig>::NodeId, BasicNode> =
            BTreeMap::new();
        members.insert(
            config.node_id,
            BasicNode {
                addr: config.bind_addr.clone(),
            },
        );
        for (id, addr) in &config.peers {
            members.insert(*id, BasicNode { addr: addr.clone() });
        }
        raft.initialize(members)
            .await
            .map_err(|_| StorageError::InitFailed("failed to bootstrap raft"))?;
        diagnostics::log_event("storage: raft bootstrap complete");
        return Ok(());
    }

    diagnostics::log_event("storage: bootstrap disabled; waiting for external membership");
    Ok(())
}

async fn run_loop(
    raft: Raft<TypeConfig>,
    store: Arc<KvStore>,
    blob: BlobBackend,
    mut rx: mpsc::Receiver<Envelope>,
    config: StorageConfig,
) {
    let mut batch: Vec<WriteItem> = Vec::new();
    let mut batch_start = None::<Instant>;
    loop {
        if batch.is_empty() {
            match rx.recv().await {
                Some(msg) => {
                    handle_envelope(
                        msg,
                        &raft,
                        &store,
                        &blob,
                        &mut batch,
                        &mut batch_start,
                        &config,
                    )
                    .await
                }
                None => break,
            }
        } else {
            let elapsed = batch_start.map(|t| t.elapsed()).unwrap_or_default();
            let delay = batch_max_delay(&config);
            let remaining = delay.saturating_sub(elapsed);
            tokio::select! {
                maybe_msg = rx.recv() => {
                    match maybe_msg {
                        Some(msg) => {
                            handle_envelope(
                                msg,
                                &raft,
                                &store,
                                &blob,
                                &mut batch,
                                &mut batch_start,
                                &config,
                            )
                            .await
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(remaining) => {
                    flush_batch(&raft, &mut batch, &mut batch_start).await;
                }
            }
        }
        if batch.len() >= config.batch_max_ops {
            flush_batch(&raft, &mut batch, &mut batch_start).await;
        }
    }
    if !batch.is_empty() {
        flush_batch(&raft, &mut batch, &mut batch_start).await;
    }
}

async fn handle_envelope(
    env: Envelope,
    raft: &Raft<TypeConfig>,
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    batch: &mut Vec<WriteItem>,
    batch_start: &mut Option<Instant>,
    config: &StorageConfig,
) {
    match env.req {
        StorageRequest::Get { key } => {
            if !batch.is_empty() {
                flush_batch(raft, batch, batch_start).await;
            }
            let start = Instant::now();
            let resp = read_linearizable_value(raft, store, blob, key, Some(&config.peers)).await;
            let resp = match resp {
                Ok(Some(bytes)) => StorageResponse::Ok(string::str_from_bytes(&bytes)),
                Ok(None) => StorageResponse::Ok(Value::nil()),
                Err(err) => StorageResponse::Err(err.to_string()),
            };
            let _ = env.resp.send(resp);
            metrics::inc_storage_read();
            metrics::record_storage_read_latency(start.elapsed());
        }
        StorageRequest::GetBytes { key } => {
            if !batch.is_empty() {
                flush_batch(raft, batch, batch_start).await;
            }
            let start = Instant::now();
            let resp = read_linearizable_value(raft, store, blob, key, Some(&config.peers)).await;
            let resp = match resp {
                Ok(Some(bytes)) => StorageResponse::Bytes(Some(bytes)),
                Ok(None) => StorageResponse::Bytes(None),
                Err(err) => StorageResponse::Err(err.to_string()),
            };
            let _ = env.resp.send(resp);
            metrics::inc_storage_read();
            metrics::record_storage_read_latency(start.elapsed());
        }
        StorageRequest::GetWithVersion { key } => {
            if !batch.is_empty() {
                flush_batch(raft, batch, batch_start).await;
            }
            let start = Instant::now();
            let resp =
                read_linearizable_record_value(raft, store, blob, key, Some(&config.peers)).await;
            let resp = match resp {
                Ok(Some((bytes, version))) => StorageResponse::Ok(record_to_map(bytes, version)),
                Ok(None) => StorageResponse::Ok(Value::nil()),
                Err(err) => StorageResponse::Err(err.to_string()),
            };
            let _ = env.resp.send(resp);
            metrics::inc_storage_read();
            metrics::record_storage_read_latency(start.elapsed());
        }
        StorageRequest::Scan { start, end, limit } => {
            if !batch.is_empty() {
                flush_batch(raft, batch, batch_start).await;
            }
            let start_ts = Instant::now();
            let resp =
                read_linearizable_scan(raft, store, blob, start, end, limit, Some(&config.peers))
                    .await;
            let resp = match resp {
                Ok(list) => StorageResponse::Ok(list),
                Err(err) => StorageResponse::Err(err.to_string()),
            };
            let _ = env.resp.send(resp);
            metrics::inc_storage_read();
            metrics::record_storage_read_latency(start_ts.elapsed());
        }
        StorageRequest::ListPrefix { prefix, limit } => {
            if !batch.is_empty() {
                flush_batch(raft, batch, batch_start).await;
            }
            let start_ts = Instant::now();
            let resp =
                read_linearizable_prefix(raft, store, prefix, limit, Some(&config.peers)).await;
            let resp = match resp {
                Ok(list) => StorageResponse::Ok(list),
                Err(err) => StorageResponse::Err(err.to_string()),
            };
            let _ = env.resp.send(resp);
            metrics::inc_storage_read();
            metrics::record_storage_read_latency(start_ts.elapsed());
        }
        StorageRequest::Put { key, value } => {
            let stored = if value.len() > config.blob.threshold_bytes {
                match blob.put(&value).await {
                    Ok(blob_ref) => StoredValue::Blob(blob_ref),
                    Err(err) => {
                        let _ = env.resp.send(StorageResponse::Err(err.to_string()));
                        return;
                    }
                }
            } else {
                StoredValue::Inline(value)
            };
            batch.push(WriteItem {
                cmd: KvCommand::Put { key, value: stored },
                resp: env.resp,
            });
            if batch_start.is_none() {
                *batch_start = Some(Instant::now());
                metrics::inc_storage_batch_open();
            }
            if batch.len() >= config.batch_max_ops {
                flush_batch(raft, batch, batch_start).await;
            }
        }
        StorageRequest::Delete { key } => {
            batch.push(WriteItem {
                cmd: KvCommand::Delete { key },
                resp: env.resp,
            });
            if batch_start.is_none() {
                *batch_start = Some(Instant::now());
                metrics::inc_storage_batch_open();
            }
            if batch.len() >= config.batch_max_ops {
                flush_batch(raft, batch, batch_start).await;
            }
        }
        StorageRequest::CompareAndSet {
            key,
            expected_version,
            value,
        } => {
            let stored = match value {
                Some(raw) => {
                    if raw.len() > config.blob.threshold_bytes {
                        match blob.put(&raw).await {
                            Ok(blob_ref) => Some(StoredValue::Blob(blob_ref)),
                            Err(err) => {
                                let _ = env.resp.send(StorageResponse::Err(err.to_string()));
                                return;
                            }
                        }
                    } else {
                        Some(StoredValue::Inline(raw))
                    }
                }
                None => None,
            };
            let result = raft
                .client_write(KvRequest::CompareAndSet {
                    key,
                    expected_version,
                    value: stored.clone(),
                })
                .await;
            let resp = match result {
                Ok(resp) => match resp.data {
                    KvResponse::CompareAndSet(applied) => {
                        if !applied {
                            if let Some(StoredValue::Blob(blob_ref)) = stored {
                                let _ = blob.delete(&blob_ref).await;
                            }
                        }
                        StorageResponse::Ok(Value::from_bool(applied))
                    }
                    _ => StorageResponse::Err("compare-and-set failed".to_string()),
                },
                Err(err) => {
                    if let Some(StoredValue::Blob(blob_ref)) = stored {
                        let _ = blob.delete(&blob_ref).await;
                    }
                    StorageResponse::Err(err.to_string())
                }
            };
            let _ = env.resp.send(resp);
        }
        StorageRequest::BatchSet { items } => {
            let mut ops = Vec::with_capacity(items.len());
            let mut staged_blobs = Vec::new();
            for (key, value) in items {
                let stored = if value.len() > config.blob.threshold_bytes {
                    match blob.put(&value).await {
                        Ok(blob_ref) => {
                            staged_blobs.push(blob_ref.clone());
                            StoredValue::Blob(blob_ref)
                        }
                        Err(err) => {
                            let _ = env.resp.send(StorageResponse::Err(err.to_string()));
                            return;
                        }
                    }
                } else {
                    StoredValue::Inline(value)
                };
                ops.push(KvCommand::Put { key, value: stored });
            }
            let result = raft.client_write(KvRequest::Batch { ops }).await;
            let resp = match result {
                Ok(_) => StorageResponse::Ok(Value::from_bool(true)),
                Err(err) => {
                    for blob_ref in staged_blobs {
                        let _ = blob.delete(&blob_ref).await;
                    }
                    StorageResponse::Err(err.to_string())
                }
            };
            let _ = env.resp.send(resp);
        }
    }
}

async fn flush_batch(
    raft: &Raft<TypeConfig>,
    batch: &mut Vec<WriteItem>,
    batch_start: &mut Option<Instant>,
) {
    if batch.is_empty() {
        return;
    }
    let started = batch_start.take();
    let size = batch.len();
    let ops = std::mem::take(batch);
    metrics::record_storage_batch_size(size);
    if let Some(start) = started {
        metrics::record_storage_batch_latency(start.elapsed());
    }
    let start_commit = Instant::now();
    let commands: Vec<KvCommand> = ops.iter().map(|item| item.cmd.clone()).collect();
    let result = raft.client_write(KvRequest::Batch { ops: commands }).await;
    metrics::record_storage_commit_latency(start_commit.elapsed());
    for item in ops {
        let resp = match &result {
            Ok(_) => StorageResponse::Ok(Value::nil()),
            Err(err) => StorageResponse::Err(err.to_string()),
        };
        let _ = item.resp.send(resp);
    }
}

fn record_to_map(value_bytes: Vec<u8>, version: u64) -> Value {
    let map_val = map::map_new();
    let value_val = string::str_from_bytes(&value_bytes);
    let key_value = string::str_from_bytes(b"value");
    map::map_set(map_val, key_value, value_val);
    unsafe {
        crate::wr_rc_dec(key_value);
        if value_val.is_ptr() {
            crate::wr_rc_dec(value_val);
        }
    }
    let key_version = string::str_from_bytes(b"version");
    map::map_set(map_val, key_version, Value::from_int(version as i64));
    unsafe { crate::wr_rc_dec(key_version) };
    map_val
}

async fn record_bytes(blob: &BlobBackend, record: &StoredRecord) -> Result<Vec<u8>, StorageError> {
    match &record.value {
        StoredValue::Inline(bytes) => Ok(bytes.clone()),
        StoredValue::Blob(blob_ref) => blob
            .get(blob_ref)
            .await
            .map_err(|err| StorageError::Internal(err.to_string())),
    }
}

fn scan_entry_map(key: &[u8], value: &[u8], version: u64) -> Value {
    let map_val = map::map_new();
    let key_key = string::str_from_bytes(b"key");
    let key_val = string::str_from_bytes(key);
    map::map_set(map_val, key_key, key_val);
    unsafe {
        crate::wr_rc_dec(key_key);
        crate::wr_rc_dec(key_val);
    }
    let value_key = string::str_from_bytes(b"value");
    let value_val = string::str_from_bytes(value);
    map::map_set(map_val, value_key, value_val);
    unsafe {
        crate::wr_rc_dec(value_key);
        crate::wr_rc_dec(value_val);
    }
    let version_key = string::str_from_bytes(b"version");
    map::map_set(map_val, version_key, Value::from_int(version as i64));
    unsafe { crate::wr_rc_dec(version_key) };
    map_val
}

fn list_from_keys(keys: Vec<Vec<u8>>) -> Value {
    let list_val = list::list_new(0);
    for key in keys {
        let key_val = string::str_from_bytes(&key);
        list::list_push(list_val, key_val);
    }
    list_val
}

fn list_from_entries(entries: Vec<(Vec<u8>, Vec<u8>, u64)>) -> Value {
    let list_val = list::list_new(0);
    for (key, value, version) in entries {
        let map_val = scan_entry_map(&key, &value, version);
        list::list_push(list_val, map_val);
    }
    list_val
}

async fn read_local_record_value(
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    key: &[u8],
) -> Result<Option<(Vec<u8>, u64)>, StorageError> {
    let record = {
        let guard = store.state_machine.read().await;
        guard
            .get_value(key)
            .map_err(|err| StorageError::Internal(format!("state machine get: {err}")))?
    };
    let Some(record) = record else {
        return Ok(None);
    };
    let bytes = record_bytes(blob, &record).await?;
    Ok(Some((bytes, record.version)))
}

async fn read_local_value(
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    key: &[u8],
) -> Result<Option<Vec<u8>>, StorageError> {
    read_local_record_value(store, blob, key)
        .await
        .map(|opt| opt.map(|(bytes, _)| bytes))
}

async fn read_linearizable_value(
    raft: &Raft<TypeConfig>,
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    key: Vec<u8>,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Option<Vec<u8>>, StorageError> {
    match raft.ensure_linearizable().await {
        Ok(_) => read_local_value(store, blob, &key).await,
        Err(err) => {
            if is_single_node(peers) {
                read_local_value(store, blob, &key).await
            } else {
                forward_to_leader(
                    err,
                    peers,
                    |addr| async move { forward_read(addr, key).await },
                )
                .await
            }
        }
    }
}

async fn read_linearizable_record_value(
    raft: &Raft<TypeConfig>,
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    key: Vec<u8>,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Option<(Vec<u8>, u64)>, StorageError> {
    match raft.ensure_linearizable().await {
        Ok(_) => read_local_record_value(store, blob, &key).await,
        Err(err) => {
            if is_single_node(peers) {
                read_local_record_value(store, blob, &key).await
            } else {
                forward_to_leader(err, peers, |addr| async move {
                    forward_read_version(addr, key).await
                })
                .await
            }
        }
    }
}

async fn read_linearizable_scan(
    raft: &Raft<TypeConfig>,
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    limit: usize,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Value, StorageError> {
    match raft.ensure_linearizable().await {
        Ok(_) => {
            read_scan_with_retry(store, blob, start.as_deref(), end.as_deref(), limit, peers).await
        }
        Err(err) => {
            if is_single_node(peers) {
                read_scan_with_retry(store, blob, start.as_deref(), end.as_deref(), limit, peers)
                    .await
            } else {
                forward_to_leader(err, peers, |addr| async move {
                    forward_scan(addr, start, end, limit).await
                })
                .await
            }
        }
    }
}

async fn read_linearizable_prefix(
    raft: &Raft<TypeConfig>,
    store: &Arc<KvStore>,
    prefix: Vec<u8>,
    limit: usize,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Value, StorageError> {
    match raft.ensure_linearizable().await {
        Ok(_) => read_prefix_with_retry(store, &prefix, limit, peers).await,
        Err(err) => {
            if is_single_node(peers) {
                read_prefix_with_retry(store, &prefix, limit, peers).await
            } else {
                forward_to_leader(err, peers, |addr| async move {
                    forward_prefix(addr, prefix, limit).await
                })
                .await
            }
        }
    }
}

fn is_single_node(peers: Option<&std::collections::HashMap<NodeId, String>>) -> bool {
    peers.map(|p| p.is_empty()).unwrap_or(true)
}

async fn read_scan_with_retry(
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    start: Option<&[u8]>,
    end: Option<&[u8]>,
    limit: usize,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Value, StorageError> {
    let mut entries = read_scan_entries(store, blob, start, end, limit).await?;
    if entries.is_empty() && is_single_node(peers) && start.is_some() {
        tokio::time::sleep(Duration::from_millis(5)).await;
        entries = read_scan_entries(store, blob, start, end, limit).await?;
    }
    Ok(list_from_entries(entries))
}

async fn read_scan_entries(
    store: &Arc<KvStore>,
    blob: &BlobBackend,
    start: Option<&[u8]>,
    end: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, StorageError> {
    let records = {
        let guard = store.state_machine.read().await;
        guard.scan_range(start, end, limit)
    }
    .map_err(|err| StorageError::Internal(format!("state machine scan: {err}")))?;
    let mut entries = Vec::with_capacity(records.len());
    for (key, record) in records {
        let bytes = record_bytes(blob, &record).await?;
        entries.push((key, bytes, record.version));
    }
    Ok(entries)
}

async fn read_prefix_with_retry(
    store: &Arc<KvStore>,
    prefix: &[u8],
    limit: usize,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
) -> Result<Value, StorageError> {
    let mut keys = read_prefix_keys(store, prefix, limit).await?;
    if keys.is_empty() && is_single_node(peers) && !prefix.is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
        keys = read_prefix_keys(store, prefix, limit).await?;
    }
    if keys.is_empty() && !prefix.is_empty() {
        let end = next_prefix_bytes(prefix);
        if let Ok(scan_keys) = read_scan_keys(store, Some(prefix), end.as_deref(), limit).await {
            if !scan_keys.is_empty() {
                keys = scan_keys;
            }
        }
    }
    Ok(list_from_keys(keys))
}

async fn read_prefix_keys(
    store: &Arc<KvStore>,
    prefix: &[u8],
    limit: usize,
) -> Result<Vec<Vec<u8>>, StorageError> {
    let keys = {
        let guard = store.state_machine.read().await;
        guard.list_prefix_keys(prefix, limit)
    }
    .map_err(|err| StorageError::Internal(format!("state machine prefix: {err}")))?;
    Ok(keys)
}

async fn read_scan_keys(
    store: &Arc<KvStore>,
    start: Option<&[u8]>,
    end: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<Vec<u8>>, StorageError> {
    let records = {
        let guard = store.state_machine.read().await;
        guard.scan_range(start, end, limit)
    }
    .map_err(|err| StorageError::Internal(format!("state machine scan: {err}")))?;
    Ok(records.into_iter().map(|(key, _record)| key).collect())
}

fn next_prefix_bytes(prefix: &[u8]) -> Option<Vec<u8>> {
    if prefix.is_empty() {
        return None;
    }
    let mut out = prefix.to_vec();
    for idx in (0..out.len()).rev() {
        if out[idx] != 0xFF {
            out[idx] = out[idx].wrapping_add(1);
            out.truncate(idx + 1);
            return Some(out);
        }
    }
    None
}

async fn forward_to_leader<E, F, Fut, T>(
    err: openraft::error::RaftError<NodeId, E>,
    peers: Option<&std::collections::HashMap<NodeId, String>>,
    f: F,
) -> Result<T, StorageError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<T, StorageError>>,
    E: openraft::TryAsRef<openraft::error::ForwardToLeader<NodeId, BasicNode>>
        + std::error::Error
        + Clone
        + Send
        + Sync
        + 'static,
{
    if let Some(fwd) = err.forward_to_leader::<BasicNode>() {
        if let Some(node) = fwd.leader_node.as_ref() {
            if !node.addr.is_empty() {
                return f(node.addr.clone()).await;
            }
        }
        if let Some(id) = fwd.leader_id {
            if let Some(peers) = peers {
                if let Some(addr) = peers.get(&id) {
                    return f(addr.clone()).await;
                }
            }
        }
        Err(StorageError::Internal("leader address unknown".to_string()))
    } else {
        Err(StorageError::Internal(err.to_string()))
    }
}

async fn forward_read(addr: String, key: Vec<u8>) -> Result<Option<Vec<u8>>, StorageError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let url = format!("http://{}/storage/read", addr);
    let mut request = client.post(url).json(&StorageReadRequest { key });
    if let Some(token) = storage_config().peer_token {
        request = request.header("x-wrela-peer-token", token);
    }
    let resp = request
        .send()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let envelope: RpcEnvelope<StorageReadResponse> = resp
        .json()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    if envelope.ok {
        Ok(envelope.data.and_then(|d| d.value))
    } else {
        Err(StorageError::Internal(
            envelope.error.unwrap_or_else(|| "read failed".to_string()),
        ))
    }
}

async fn forward_read_version(
    addr: String,
    key: Vec<u8>,
) -> Result<Option<(Vec<u8>, u64)>, StorageError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let url = format!("http://{}/storage/read_version", addr);
    let mut request = client.post(url).json(&StorageReadRequest { key });
    if let Some(token) = storage_config().peer_token {
        request = request.header("x-wrela-peer-token", token);
    }
    let resp = request
        .send()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let envelope: RpcEnvelope<StorageReadVersionResponse> = resp
        .json()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    if !envelope.ok {
        return Err(StorageError::Internal(
            envelope.error.unwrap_or_else(|| "read failed".to_string()),
        ));
    }
    let Some(data) = envelope.data else {
        return Ok(None);
    };
    match (data.value, data.version) {
        (Some(bytes), Some(version)) => Ok(Some((bytes, version))),
        _ => Ok(None),
    }
}

async fn forward_scan(
    addr: String,
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    limit: usize,
) -> Result<Value, StorageError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let url = format!("http://{}/storage/scan", addr);
    let mut request = client
        .post(url)
        .json(&StorageScanRequest { start, end, limit });
    if let Some(token) = storage_config().peer_token {
        request = request.header("x-wrela-peer-token", token);
    }
    let resp = request
        .send()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let envelope: RpcEnvelope<StorageScanResponse> = resp
        .json()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    if !envelope.ok {
        return Err(StorageError::Internal(
            envelope.error.unwrap_or_else(|| "scan failed".to_string()),
        ));
    }
    let entries = envelope
        .data
        .map(|data| {
            data.entries
                .into_iter()
                .map(|entry| (entry.key, entry.value, entry.version))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(list_from_entries(entries))
}

async fn forward_prefix(
    addr: String,
    prefix: Vec<u8>,
    limit: usize,
) -> Result<Value, StorageError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let url = format!("http://{}/storage/prefix", addr);
    let mut request = client
        .post(url)
        .json(&StoragePrefixRequest { prefix, limit });
    if let Some(token) = storage_config().peer_token {
        request = request.header("x-wrela-peer-token", token);
    }
    let resp = request
        .send()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    let envelope: RpcEnvelope<StoragePrefixResponse> = resp
        .json()
        .await
        .map_err(|err| StorageError::Internal(err.to_string()))?;
    if !envelope.ok {
        return Err(StorageError::Internal(
            envelope
                .error
                .unwrap_or_else(|| "prefix failed".to_string()),
        ));
    }
    let keys = envelope.data.map(|data| data.keys).unwrap_or_default();
    Ok(list_from_keys(keys))
}
