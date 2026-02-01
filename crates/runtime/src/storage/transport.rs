use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpListener;

use openraft::RaftTypeConfig;
use openraft::error::{InstallSnapshotError, NetworkError, RPCError, RaftError};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};

use crate::storage::blob::BlobBackend;
use crate::storage::service::StorageError;
use crate::storage::store::TypeConfig;
use crate::storage::value::{StoredRecord, StoredValue};
use crate::pubsub::{self, PubSubMessage};
use crate::realtime::{self, FanoutRequest};

pub type NodeId = <TypeConfig as RaftTypeConfig>::NodeId;
pub type Node = <TypeConfig as RaftTypeConfig>::Node;

#[derive(Clone)]
pub struct HttpNetworkFactory {
    peers: Arc<HashMap<NodeId, String>>,
    client: reqwest::Client,
    peer_token: Option<String>,
}

impl HttpNetworkFactory {
    pub fn new(peers: HashMap<NodeId, String>, peer_token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("reqwest client");
        Self {
            peers: Arc::new(peers),
            client,
            peer_token,
        }
    }
}

pub struct HttpNetwork {
    addr: String,
    client: reqwest::Client,
    peer_token: Option<String>,
}

#[derive(Clone, Default)]
pub struct NullNetworkFactory;

pub struct NullNetwork;

impl RaftNetworkFactory<TypeConfig> for NullNetworkFactory {
    type Network = NullNetwork;

    async fn new_client(&mut self, _target: NodeId, _node: &Node) -> Self::Network {
        NullNetwork
    }
}

impl RaftNetwork<TypeConfig> for NullNetwork {
    async fn append_entries(
        &mut self,
        _req: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, Node, RaftError<NodeId>>> {
        Err(unreachable_rpc::<RaftError<NodeId>>())
    }

    async fn install_snapshot(
        &mut self,
        _req: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, Node, RaftError<NodeId, InstallSnapshotError>>,
    > {
        Err(unreachable_rpc::<RaftError<NodeId, InstallSnapshotError>>())
    }

    async fn vote(
        &mut self,
        _req: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, Node, RaftError<NodeId>>> {
        Err(unreachable_rpc::<RaftError<NodeId>>())
    }
}

fn unreachable_rpc<E>() -> RPCError<NodeId, Node, E>
where
    E: std::error::Error + Clone + Send + Sync + 'static,
{
    let err = std::io::Error::new(std::io::ErrorKind::Other, "network disabled");
    RPCError::Network(NetworkError::new(&err))
}

#[derive(Serialize, Deserialize)]
pub struct RpcEnvelope<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> RpcEnvelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct StorageReadRequest {
    pub key: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct StorageReadResponse {
    pub value: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
pub struct StorageReadVersionResponse {
    pub value: Option<Vec<u8>>,
    pub version: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct StorageScanRequest {
    pub start: Option<Vec<u8>>,
    pub end: Option<Vec<u8>>,
    pub limit: usize,
}

#[derive(Serialize, Deserialize)]
pub struct StorageScanEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub version: u64,
}

#[derive(Serialize, Deserialize)]
pub struct StorageScanResponse {
    pub entries: Vec<StorageScanEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct StoragePrefixRequest {
    pub prefix: Vec<u8>,
    pub limit: usize,
}

#[derive(Serialize, Deserialize)]
pub struct StoragePrefixResponse {
    pub keys: Vec<Vec<u8>>,
}

#[cfg(any(test, feature = "test-utils"))]
static DROP_REPLICATION: AtomicBool = AtomicBool::new(false);

#[cfg(any(test, feature = "test-utils"))]
pub fn set_drop_replication(enabled: bool) {
    DROP_REPLICATION.store(enabled, Ordering::SeqCst);
}

#[cfg(any(test, feature = "test-utils"))]
fn replication_dropped() -> bool {
    DROP_REPLICATION.load(Ordering::SeqCst)
}

impl RaftNetworkFactory<TypeConfig> for HttpNetworkFactory {
    type Network = HttpNetwork;

    async fn new_client(&mut self, target: NodeId, node: &Node) -> Self::Network {
        let addr = self
            .peers
            .get(&target)
            .cloned()
            .unwrap_or_else(|| node.addr.clone());
        HttpNetwork {
            addr,
            client: self.client.clone(),
            peer_token: self.peer_token.clone(),
        }
    }
}

impl RaftNetwork<TypeConfig> for HttpNetwork {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, Node, RaftError<NodeId>>> {
        #[cfg(any(test, feature = "test-utils"))]
        if replication_dropped() {
            let err = std::io::Error::new(std::io::ErrorKind::Other, "replication dropped");
            return Err(RPCError::Network(NetworkError::new(&err)));
        }
        self.post_json("/raft/append", req).await
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, Node, RaftError<NodeId, InstallSnapshotError>>,
    > {
        #[cfg(any(test, feature = "test-utils"))]
        if replication_dropped() {
            let err = std::io::Error::new(std::io::ErrorKind::Other, "replication dropped");
            return Err(RPCError::Network(NetworkError::new(&err)));
        }
        self.post_json("/raft/install_snapshot", req).await
    }

    async fn vote(
        &mut self,
        req: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, Node, RaftError<NodeId>>> {
        self.post_json("/raft/vote", req).await
    }
}

impl HttpNetwork {
    async fn post_json<T, R, E>(&self, path: &str, req: T) -> Result<R, RPCError<NodeId, Node, E>>
    where
        T: Serialize + Send + Sync,
        R: for<'de> Deserialize<'de> + Send,
        E: std::error::Error + Clone + Send + Sync + 'static,
    {
        let url = format!("http://{}{}", self.addr, path);
        let mut request = self.client.post(url).json(&req);
        if let Some(token) = self.peer_token.as_ref() {
            request = request.header("x-wrela-peer-token", token);
        }
        let resp = request.send().await
            .map_err(|err| RPCError::Network(NetworkError::new(&err)))?;
        let status = resp.status();
        if !status.is_success() {
            let err =
                std::io::Error::new(std::io::ErrorKind::Other, format!("http status {}", status));
            return Err(RPCError::Network(NetworkError::new(&err)));
        }
        let envelope: RpcEnvelope<R> = resp
            .json()
            .await
            .map_err(|err| RPCError::Network(NetworkError::new(&err)))?;
        if envelope.ok {
            envelope.data.ok_or_else(|| {
                RPCError::Network(NetworkError::new(&std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing response data",
                )))
            })
        } else {
            Err(RPCError::Network(NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                envelope
                    .error
                    .unwrap_or_else(|| "unknown rpc error".to_string()),
            ))))
        }
    }
}

#[derive(Clone)]
pub struct HttpServer {
    raft: openraft::Raft<TypeConfig>,
    store: Arc<crate::storage::store::KvStore>,
    blob: BlobBackend,
    realtime: crate::realtime::RealtimeStateHandleArc,
    peer_token: Option<String>,
}

pub async fn start_http_server(
    listener: TcpListener,
    raft: openraft::Raft<TypeConfig>,
    store: Arc<crate::storage::store::KvStore>,
    blob: BlobBackend,
    realtime: crate::realtime::RealtimeStateHandleArc,
    peer_token: Option<String>,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), StorageError> {
    let addr = listener
        .local_addr()
        .map_err(|_| StorageError::InitFailed("failed to read raft http addr"))?;

    let app = Router::new()
        .route("/raft/append", post(append_entries))
        .route("/raft/install_snapshot", post(install_snapshot))
        .route("/raft/vote", post(vote))
        .route("/storage/read", post(storage_read))
        .route("/storage/read_version", post(storage_read_version))
        .route("/storage/scan", post(storage_scan))
        .route("/storage/prefix", post(storage_prefix))
        .route("/realtime/fanout", post(realtime_fanout))
        .route("/pubsub/publish", post(pubsub_publish));

    #[cfg(any(test, feature = "test-utils"))]
    let app = app.route("/storage/leader", post(storage_leader));

    let app = app.with_state(HttpServer {
        raft,
        store,
        blob,
        realtime,
        peer_token,
    });

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Ok((addr, handle))
}

fn authorize(headers: &HeaderMap, token: &Option<String>) -> bool {
    let Some(expected) = token.as_ref() else {
        return true;
    };
    headers
        .get("x-wrela-peer-token")
        .and_then(|val| val.to_str().ok())
        .map(|val| val == expected)
        .unwrap_or(false)
}

fn unauthorized<T>() -> (StatusCode, Json<RpcEnvelope<T>>) {
    (StatusCode::UNAUTHORIZED, Json(RpcEnvelope::err("unauthorized")))
}

async fn append_entries(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<AppendEntriesRequest<TypeConfig>>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp: Result<AppendEntriesResponse<NodeId>, RaftError<NodeId>> =
        state.raft.append_entries(req).await;
    match resp {
        Ok(ok) => (StatusCode::OK, Json(RpcEnvelope::ok(ok))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn install_snapshot(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<InstallSnapshotRequest<TypeConfig>>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp: Result<InstallSnapshotResponse<NodeId>, RaftError<NodeId, InstallSnapshotError>> =
        state.raft.install_snapshot(req).await;
    match resp {
        Ok(ok) => (StatusCode::OK, Json(RpcEnvelope::ok(ok))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn vote(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<VoteRequest<NodeId>>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp: Result<VoteResponse<NodeId>, RaftError<NodeId>> = state.raft.vote(req).await;
    match resp {
        Ok(ok) => (StatusCode::OK, Json(RpcEnvelope::ok(ok))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn realtime_fanout(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<FanoutRequest>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    realtime::deliver_fanout_with(state.realtime.clone(), req).await;
    (StatusCode::OK, Json(RpcEnvelope::ok(true)))
}

async fn pubsub_publish(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<PubSubMessage>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    pubsub::handle_publish(req).await;
    (StatusCode::OK, Json(RpcEnvelope::ok(true)))
}

async fn storage_read(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<StorageReadRequest>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp = state.raft.ensure_linearizable().await;
    if let Err(err) = resp {
        return (StatusCode::OK, Json(RpcEnvelope::err(err.to_string())));
    }
    let stored = {
        let guard = state.store.state_machine.read().await;
        guard.get_value(&req.key)
    };
    match stored {
        Ok(Some(record)) => match record_bytes(&state.blob, &record).await {
            Ok(bytes) => (StatusCode::OK, Json(RpcEnvelope::ok(StorageReadResponse { value: Some(bytes) }))),
            Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err))),
        },
        Ok(None) => (StatusCode::OK, Json(RpcEnvelope::ok(StorageReadResponse { value: None }))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn storage_read_version(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<StorageReadRequest>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp = state.raft.ensure_linearizable().await;
    if let Err(err) = resp {
        return (StatusCode::OK, Json(RpcEnvelope::err(err.to_string())));
    }
    let stored = {
        let guard = state.store.state_machine.read().await;
        guard.get_value(&req.key)
    };
    match stored {
        Ok(Some(record)) => match record_bytes(&state.blob, &record).await {
            Ok(bytes) => (StatusCode::OK, Json(RpcEnvelope::ok(StorageReadVersionResponse {
                value: Some(bytes),
                version: Some(record.version),
            }))),
            Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err))),
        },
        Ok(None) => (StatusCode::OK, Json(RpcEnvelope::ok(StorageReadVersionResponse {
            value: None,
            version: None,
        }))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn storage_scan(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<StorageScanRequest>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp = state.raft.ensure_linearizable().await;
    if let Err(err) = resp {
        return (StatusCode::OK, Json(RpcEnvelope::err(err.to_string())));
    }
    let records = {
        let guard = state.store.state_machine.read().await;
        guard.scan_range(req.start.as_deref(), req.end.as_deref(), req.limit)
    };
    let records = match records {
        Ok(records) => records,
        Err(err) => {
            return (StatusCode::OK, Json(RpcEnvelope::err(err.to_string())));
        }
    };
    let mut entries = Vec::with_capacity(records.len());
    for (key, record) in records {
        let bytes = match record_bytes(&state.blob, &record).await {
            Ok(bytes) => bytes,
            Err(err) => return (StatusCode::OK, Json(RpcEnvelope::err(err))),
        };
        entries.push(StorageScanEntry {
            key,
            value: bytes,
            version: record.version,
        });
    }
    (StatusCode::OK, Json(RpcEnvelope::ok(StorageScanResponse { entries })))
}

async fn storage_prefix(
    State(state): State<HttpServer>,
    headers: HeaderMap,
    Json(req): Json<StoragePrefixRequest>,
) -> impl IntoResponse {
    if !authorize(&headers, &state.peer_token) {
        return unauthorized();
    }
    let resp = state.raft.ensure_linearizable().await;
    if let Err(err) = resp {
        return (StatusCode::OK, Json(RpcEnvelope::err(err.to_string())));
    }
    let keys = {
        let guard = state.store.state_machine.read().await;
        guard.list_prefix_keys(&req.prefix, req.limit)
    };
    match keys {
        Ok(keys) => (StatusCode::OK, Json(RpcEnvelope::ok(StoragePrefixResponse { keys }))),
        Err(err) => (StatusCode::OK, Json(RpcEnvelope::err(err.to_string()))),
    }
}

async fn record_bytes(blob: &BlobBackend, record: &StoredRecord) -> Result<Vec<u8>, String> {
    match &record.value {
        StoredValue::Inline(bytes) => Ok(bytes.clone()),
        StoredValue::Blob(blob_ref) => blob.get(blob_ref).await.map_err(|err| err.to_string()),
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Serialize, Deserialize)]
struct StorageLeaderResponse {
    node_id: NodeId,
    leader_id: Option<NodeId>,
}

#[cfg(any(test, feature = "test-utils"))]
async fn storage_leader(
    State(state): State<HttpServer>,
    headers: HeaderMap,
) -> Json<RpcEnvelope<StorageLeaderResponse>> {
    if !authorize(&headers, &state.peer_token) {
        return Json(RpcEnvelope::err("unauthorized"));
    }
    let metrics = state.raft.metrics().borrow().clone();
    let leader_id = metrics.current_leader;
    let node_id = metrics.id;
    Json(RpcEnvelope::ok(StorageLeaderResponse {
        node_id,
        leader_id,
    }))
}
