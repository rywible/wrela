//! Private RPC layer using tonic gRPC.
//!
//! Replaces the legacy custom wire protocol with real gRPC (HTTP/2 multiplexing,
//! protobuf, connection reuse via tonic).

use crate::db::rpc::errors::status_to_rpc_error;
use crate::db::rpc::grpc::{
    GrpcEdgeService, PointReadRequest, RangeReadRequest, RemoteWriteTransport, WriteBatchRequest,
    WriteBatchResponse,
};
use crate::db::rpc::tonic_service::wrpc::wrela_db_client::WrelaDbClient;
use crate::db::rpc::tonic_service::{
    WrelaDbServiceImpl, point_read_request_to_proto, range_read_request_to_proto,
    write_batch_request_to_proto,
};
use crate::kernel::runtime;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};

/// Run async work to completion, avoiding "block from within runtime" panics.
/// If called from a Tokio worker thread, uses block_in_place first.
fn block_on_async<F>(f: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    let rt = runtime::tokio_runtime();
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| rt.block_on(f))
    } else {
        rt.block_on(f)
    }
}
use hickory_resolver::Resolver;
use hickory_resolver::proto::rr::{RData, RecordType};
use hickory_resolver::system_conf::read_system_conf;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, RwLock as TokioRwLock, Semaphore};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};

pub type NodeAddressResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn rpc_unavailable(message: impl Into<String>) -> crate::db::rpc::errors::RpcError {
    crate::db::rpc::errors::RpcError {
        code: crate::db::rpc::errors::RpcStatusCode::Unavailable,
        message: message.into(),
        retry: Some(crate::db::rpc::errors::RetryHint { retry_after_ms: 25 }),
        leader: None,
    }
}

fn rpc_unavailable_for_leader(
    leader_node_id: impl Into<String>,
    message: impl Into<String>,
) -> crate::db::rpc::errors::RpcError {
    crate::db::rpc::errors::RpcError {
        code: crate::db::rpc::errors::RpcStatusCode::Unavailable,
        message: message.into(),
        retry: Some(crate::db::rpc::errors::RetryHint { retry_after_ms: 25 }),
        leader: Some(crate::db::rpc::errors::LeaderHint {
            leader_node_id: leader_node_id.into(),
        }),
    }
}

fn ensure_http_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

fn parse_machine_id_candidate(raw: &str, app_name: &str) -> Option<String> {
    let token = raw.trim().trim_matches(|ch: char| {
        matches!(
            ch,
            '"' | '\'' | ',' | ';' | '[' | ']' | '(' | ')' | '{' | '}' | '`'
        )
    });
    if token.is_empty() {
        return None;
    }

    if let Some((_, value)) = token.split_once('=')
        && let Some(id) = parse_machine_id_candidate(value, app_name)
    {
        return Some(id);
    }

    let host_suffix = format!(".vm.{app_name}.internal");
    if let Some(prefix) = token.strip_suffix(&host_suffix)
        && is_valid_machine_id(prefix)
    {
        return Some(prefix.to_string());
    }

    if is_valid_machine_id(token) {
        return Some(token.to_string());
    }

    None
}

fn is_valid_machine_id(candidate: &str) -> bool {
    let trimmed = candidate.trim();
    let len = trimmed.len();
    if !(3..=64).contains(&len) {
        return false;
    }
    let mut has_digit = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if !(ch.is_ascii_lowercase() || ch == '-') {
            return false;
        }
    }
    has_digit
}

pub fn parse_machine_ids_from_txt_records(records: &[String], app_name: &str) -> Vec<String> {
    let mut machine_ids = BTreeSet::new();
    for record in records {
        for token in record
            .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';')
            .filter(|token| !token.is_empty())
        {
            if let Some(id) = parse_machine_id_candidate(token, app_name) {
                machine_ids.insert(id);
            }
        }
    }
    machine_ids.into_iter().collect()
}

pub fn fly_private_rpc_addresses(
    machine_ids: &[String],
    app_name: &str,
    private_rpc_port: u16,
) -> HashMap<String, String> {
    machine_ids
        .iter()
        .map(|machine_id| {
            (
                machine_id.clone(),
                format!("{machine_id}.vm.{app_name}.internal:{private_rpc_port}"),
            )
        })
        .collect()
}

pub fn discover_fly_machine_ids_via_dns(
    app_name: &str,
    timeout: Duration,
) -> Result<Vec<String>, String> {
    let query_name = format!("vms.{app_name}.internal.");
    let (config, mut opts) =
        read_system_conf().unwrap_or_else(|_| (ResolverConfig::default(), ResolverOpts::default()));
    opts.timeout = timeout;
    opts.attempts = 1;
    let resolver =
        Resolver::new(config, opts).map_err(|err| format!("dns resolver init failed: {err}"))?;
    let lookup = resolver
        .lookup(query_name.as_str(), RecordType::TXT)
        .map_err(|err| format!("dns txt lookup failed for {query_name}: {err}"))?;
    let mut records = Vec::new();
    for data in lookup.iter() {
        if let RData::TXT(txt) = data {
            let record = txt
                .txt_data()
                .iter()
                .map(|chunk| String::from_utf8_lossy(chunk).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            if !record.trim().is_empty() {
                records.push(record);
            }
        }
    }
    Ok(parse_machine_ids_from_txt_records(&records, app_name))
}

#[derive(Debug)]
pub struct PrivateRpcServer {
    stop: Arc<AtomicBool>,
    listen_addr: String,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl PrivateRpcServer {
    pub fn listen_addr(&self) -> &str {
        &self.listen_addr
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for PrivateRpcServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn start_private_rpc_server(
    bind_addr: &str,
    service: Arc<RwLock<GrpcEdgeService>>,
    _io_timeout: Duration,
) -> Result<PrivateRpcServer, std::io::Error> {
    let addr: std::net::SocketAddr = bind_addr
        .parse()
        .map_err(|e| std::io::Error::other(format!("invalid bind addr {bind_addr}: {e}")))?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_signal = stop.clone();

    let svc = crate::db::rpc::tonic_service::wrpc::wrela_db_server::WrelaDbServer::new(
        WrelaDbServiceImpl::new(service),
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let listen_addr = block_on_async(async {
        let rt = runtime::tokio_runtime();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let listen_addr = listener.local_addr()?.to_string();
        let incoming = TcpListenerStream::new(listener);
        rt.spawn(async move {
            let _ = Server::builder()
                .add_service(svc)
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });
        Result::<_, std::io::Error>::Ok(listen_addr)
    })?;

    Ok(PrivateRpcServer {
        stop: stop_signal,
        listen_addr,
        shutdown_tx: Some(shutdown_tx),
    })
}

async fn connect_channel(
    target_addr: &str,
    timeout: Duration,
) -> Result<Channel, crate::db::rpc::errors::RpcError> {
    let endpoint = ensure_http_endpoint(target_addr);
    Endpoint::from_shared(endpoint)
        .map_err(|e| rpc_unavailable(format!("invalid endpoint: {e}")))?
        .connect_timeout(timeout)
        .timeout(timeout)
        .concurrency_limit(32)
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(std::time::Duration::from_secs(30))
        .connect()
        .await
        .map_err(|e| rpc_unavailable(format!("grpc connect failed: {e}")))
}

/// Connection pool for gRPC channels. Caches channels per address for reuse.
struct ChannelPool {
    channels: TokioRwLock<HashMap<String, Vec<Channel>>>,
    timeout: Duration,
    channels_per_target: usize,
    next_selection: AtomicU64,
}

impl ChannelPool {
    fn new(timeout: Duration) -> Self {
        Self {
            channels: TokioRwLock::new(HashMap::new()),
            timeout,
            channels_per_target: private_rpc_channels_per_target_default(),
            next_selection: AtomicU64::new(0),
        }
    }

    fn select_channel(&self, addr: &str, channels: &[Channel]) -> Option<Channel> {
        if channels.is_empty() {
            return None;
        }
        if channels.len() == 1 {
            return Some(channels[0].clone());
        }
        let mut hasher = DefaultHasher::new();
        addr.hash(&mut hasher);
        let addr_hash = hasher.finish();
        let next = self.next_selection.fetch_add(1, Ordering::Relaxed);
        let idx = ((next ^ addr_hash) as usize) % channels.len();
        channels.get(idx).cloned()
    }

    async fn connect_channels(
        &self,
        addr: &str,
    ) -> Result<Vec<Channel>, crate::db::rpc::errors::RpcError> {
        let mut channels = Vec::with_capacity(self.channels_per_target);
        for _ in 0..self.channels_per_target {
            match connect_channel(addr, self.timeout).await {
                Ok(channel) => channels.push(channel),
                Err(err) => {
                    if channels.is_empty() {
                        return Err(err);
                    }
                    break;
                }
            }
        }
        Ok(channels)
    }

    async fn get_or_connect(
        &self,
        addr: &str,
    ) -> Result<Channel, crate::db::rpc::errors::RpcError> {
        {
            let guard = self.channels.read().await;
            if let Some(channels) = guard.get(addr)
                && let Some(channel) = self.select_channel(addr, channels)
            {
                return Ok(channel);
            }
        }
        let created = self.connect_channels(addr).await?;
        let mut guard = self.channels.write().await;
        if let Some(existing) = guard.get(addr)
            && let Some(channel) = self.select_channel(addr, existing)
        {
            return Ok(channel);
        }
        let selected = self.select_channel(addr, &created).ok_or_else(|| {
            rpc_unavailable("private rpc channel pool failed to create channel set")
        })?;
        guard.insert(addr.to_string(), created);
        Ok(selected)
    }
}

static REPLICATION_CHANNEL_POOL: OnceLock<Arc<ChannelPool>> = OnceLock::new();
static REPLICATION_RPC_IN_FLIGHT: OnceLock<Arc<Semaphore>> = OnceLock::new();
static REPLICATION_RPC_MAX_IN_FLIGHT: OnceLock<usize> = OnceLock::new();
static REPLICATION_RPC_PERMIT_TIMEOUT: OnceLock<Duration> = OnceLock::new();
static REPLICATION_RPC_BACKPRESSURE_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static REPLICATION_RPC_BACKPRESSURE_CLOSED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplicationRpcInFlightSnapshot {
    pub max_in_flight: u64,
    pub in_flight: u64,
    pub available_permits: u64,
    pub backpressure_timeouts: u64,
    pub backpressure_closed: u64,
}

const REPLICATION_RPC_MAX_IN_FLIGHT_DEFAULT: usize = 512;
const REPLICATION_RPC_PERMIT_TIMEOUT_DEFAULT: Duration = Duration::from_millis(25);

fn private_rpc_channels_per_target_default() -> usize {
    let n = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(2);
    (n / 2).min(4).max(2)
}

fn replication_rpc_permit_timeout() -> Duration {
    *REPLICATION_RPC_PERMIT_TIMEOUT.get_or_init(|| REPLICATION_RPC_PERMIT_TIMEOUT_DEFAULT)
}

fn replication_channel_pool(timeout: Duration) -> Arc<ChannelPool> {
    REPLICATION_CHANNEL_POOL
        .get_or_init(|| Arc::new(ChannelPool::new(timeout)))
        .clone()
}

fn replication_rpc_in_flight_semaphore() -> Arc<Semaphore> {
    let max = *REPLICATION_RPC_MAX_IN_FLIGHT.get_or_init(|| REPLICATION_RPC_MAX_IN_FLIGHT_DEFAULT);
    REPLICATION_RPC_IN_FLIGHT
        .get_or_init(|| Arc::new(Semaphore::new(max)))
        .clone()
}

pub fn replication_rpc_in_flight_snapshot() -> ReplicationRpcInFlightSnapshot {
    let max = *REPLICATION_RPC_MAX_IN_FLIGHT.get_or_init(|| REPLICATION_RPC_MAX_IN_FLIGHT_DEFAULT);
    let semaphore = replication_rpc_in_flight_semaphore();
    let available = semaphore.available_permits() as u64;
    let max_u64 = max as u64;
    ReplicationRpcInFlightSnapshot {
        max_in_flight: max_u64,
        in_flight: max_u64.saturating_sub(available),
        available_permits: available,
        backpressure_timeouts: REPLICATION_RPC_BACKPRESSURE_TIMEOUTS.load(Ordering::Relaxed),
        backpressure_closed: REPLICATION_RPC_BACKPRESSURE_CLOSED.load(Ordering::Relaxed),
    }
}

async fn acquire_replication_rpc_permit(
    timeout: Duration,
) -> Result<OwnedSemaphorePermit, crate::db::rpc::errors::RpcError> {
    let semaphore = replication_rpc_in_flight_semaphore();
    let permit_fut = semaphore.acquire_owned();
    let permit_timeout = replication_rpc_permit_timeout().min(timeout);
    match tokio::time::timeout(permit_timeout, permit_fut).await {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => {
            REPLICATION_RPC_BACKPRESSURE_CLOSED.fetch_add(1, Ordering::Relaxed);
            Err(rpc_unavailable(
                "REPLICATION_RPC_BACKPRESSURE: semaphore closed",
            ))
        }
        Err(_) => {
            REPLICATION_RPC_BACKPRESSURE_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
            Err(rpc_unavailable(
                "REPLICATION_RPC_BACKPRESSURE: timed out waiting for in-flight permit",
            ))
        }
    }
}

async fn pooled_client(
    target_addr: &str,
    timeout: Duration,
) -> Result<WrelaDbClient<Channel>, crate::db::rpc::errors::RpcError> {
    let pool = replication_channel_pool(timeout);
    let channel = pool.get_or_connect(target_addr).await?;
    Ok(WrelaDbClient::new(channel))
}

pub async fn write_batch_over_private_rpc_async(
    target_addr: &str,
    request: WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    let mut client = pooled_client(target_addr, timeout).await?;
    let proto_req = write_batch_request_to_proto(request);
    let proto_resp = client
        .write_batch(tonic::Request::new(proto_req))
        .await
        .map_err(status_to_rpc_error)?
        .into_inner();
    Ok(WriteBatchResponse {
        commit_version: proto_resp.commit_version,
        idempotent_replay: proto_resp.idempotent_replay,
        follower_wal_fsync_ns: proto_resp.follower_wal_fsync_ns,
    })
}

pub fn write_batch_over_private_rpc(
    target_addr: &str,
    request: WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    block_on_async(write_batch_over_private_rpc_async(
        target_addr,
        request,
        timeout,
    ))
}

pub async fn replicate_write_batch_over_private_rpc_async(
    target_addr: &str,
    request: WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    let proto_req = write_batch_request_to_proto(request);
    replicate_write_batch_proto_over_private_rpc_async(target_addr, proto_req, timeout).await
}

pub async fn replicate_write_batch_proto_over_private_rpc_async(
    target_addr: &str,
    proto_req: crate::db::rpc::tonic_service::wrpc::WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    let _permit = acquire_replication_rpc_permit(timeout).await?;
    let mut client = pooled_client(target_addr, timeout).await?;
    let proto_resp = client
        .replica_write_batch(tonic::Request::new(proto_req))
        .await
        .map_err(status_to_rpc_error)?
        .into_inner();
    Ok(WriteBatchResponse {
        commit_version: proto_resp.commit_version,
        idempotent_replay: proto_resp.idempotent_replay,
        follower_wal_fsync_ns: proto_resp.follower_wal_fsync_ns,
    })
}

/// Replicate a write batch, preferring a persistent bidirectional stream when
/// available. Falls back to the unary `ReplicaWriteBatch` RPC if the stream
/// is not established or the follower returns UNIMPLEMENTED (older version).
pub async fn replicate_write_batch_proto_prefer_stream_async(
    target_addr: &str,
    proto_req: crate::db::rpc::tonic_service::wrpc::WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    // Try streaming path first.
    if let Some(result) = replicate_via_stream_async(target_addr, &proto_req, timeout).await {
        return result;
    }
    // Fallback to unary.
    replicate_write_batch_proto_over_private_rpc_async(target_addr, proto_req, timeout).await
}

pub async fn replica_install_sorted_run_chunk_over_private_rpc_async(
    target_addr: &str,
    proto_req: crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest,
    timeout: Duration,
) -> Result<
    crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkResponse,
    crate::db::rpc::errors::RpcError,
> {
    let _permit = acquire_replication_rpc_permit(timeout).await?;
    let mut client = pooled_client(target_addr, timeout).await?;
    client
        .replica_install_sorted_run_chunk(tonic::Request::new(proto_req))
        .await
        .map_err(status_to_rpc_error)
        .map(|resp| resp.into_inner())
}

pub fn replica_install_sorted_run_chunk_over_private_rpc(
    target_addr: &str,
    proto_req: crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest,
    timeout: Duration,
) -> Result<
    crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkResponse,
    crate::db::rpc::errors::RpcError,
> {
    block_on_async(replica_install_sorted_run_chunk_over_private_rpc_async(
        target_addr,
        proto_req,
        timeout,
    ))
}

pub fn replicate_write_batch_over_private_rpc(
    target_addr: &str,
    request: WriteBatchRequest,
    timeout: Duration,
) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
    block_on_async(replicate_write_batch_over_private_rpc_async(
        target_addr,
        request,
        timeout,
    ))
}

pub async fn point_read_over_private_rpc_async(
    target_addr: &str,
    request: PointReadRequest,
    timeout: Duration,
) -> Result<Option<Vec<u8>>, crate::db::rpc::errors::RpcError> {
    let mut client = pooled_client(target_addr, timeout).await?;
    let proto_req = point_read_request_to_proto(request);
    let proto_resp = client
        .point_read(tonic::Request::new(proto_req))
        .await
        .map_err(status_to_rpc_error)?
        .into_inner();
    Ok(proto_resp.value.map(|b| b.into()))
}

pub fn point_read_over_private_rpc(
    target_addr: &str,
    request: PointReadRequest,
    timeout: Duration,
) -> Result<Option<Vec<u8>>, crate::db::rpc::errors::RpcError> {
    block_on_async(point_read_over_private_rpc_async(
        target_addr,
        request,
        timeout,
    ))
}

pub async fn range_read_over_private_rpc_async(
    target_addr: &str,
    request: RangeReadRequest,
    timeout: Duration,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, crate::db::rpc::errors::RpcError> {
    let mut client = pooled_client(target_addr, timeout).await?;
    let proto_req = range_read_request_to_proto(request);
    let proto_resp = client
        .range_read(tonic::Request::new(proto_req))
        .await
        .map_err(status_to_rpc_error)?
        .into_inner();
    Ok(proto_resp
        .rows
        .into_iter()
        .map(|r| (r.key.into(), r.value.into(), r.version))
        .collect())
}

pub fn range_read_over_private_rpc(
    target_addr: &str,
    request: RangeReadRequest,
    timeout: Duration,
) -> Result<Vec<(Vec<u8>, Vec<u8>, u64)>, crate::db::rpc::errors::RpcError> {
    block_on_async(range_read_over_private_rpc_async(
        target_addr,
        request,
        timeout,
    ))
}

pub fn build_private_write_transport(
    resolver: NodeAddressResolver,
    timeout: Duration,
) -> RemoteWriteTransport {
    let pool = Arc::new(ChannelPool::new(timeout));
    Arc::new(move |target_node_id, request| {
        let Some(address) = resolver(target_node_id) else {
            return Err(rpc_unavailable_for_leader(
                target_node_id.to_string(),
                format!("private rpc resolver has no address for leader {target_node_id}"),
            ));
        };
        let pool = pool.clone();
        let address = address.clone();
        let request = request.clone();
        block_on_async(async move {
            let channel = pool.get_or_connect(&address).await?;
            let mut client = WrelaDbClient::new(channel);
            let proto_req = write_batch_request_to_proto(request);
            let proto_resp = client
                .write_batch(tonic::Request::new(proto_req))
                .await
                .map_err(status_to_rpc_error)?
                .into_inner();
            Ok(WriteBatchResponse {
                commit_version: proto_resp.commit_version,
                idempotent_replay: proto_resp.idempotent_replay,
                follower_wal_fsync_ns: proto_resp.follower_wal_fsync_ns,
            })
        })
    })
}

// ---------------------------------------------------------------------------
// Streaming replication: persistent bidirectional gRPC stream pool
// ---------------------------------------------------------------------------

use crate::db::rpc::tonic_service::wrpc::ReplicationStreamBatch;
use tokio::sync::{Mutex as TokioMutex, mpsc, oneshot};

/// A single persistent bidirectional stream to a follower.
struct ReplicationStream {
    /// Send batches into the stream.
    batch_tx: mpsc::Sender<ReplicationStreamBatch>,
    /// Pending acks keyed by sequence number.
    pending: Arc<
        TokioMutex<
            HashMap<
                u64,
                oneshot::Sender<Result<WriteBatchResponse, crate::db::rpc::errors::RpcError>>,
            >,
        >,
    >,
    /// Monotonically increasing sequence counter.
    next_sequence: Arc<AtomicU64>,
    /// Set to true when the ack reader detects the stream is dead.
    dead: Arc<AtomicBool>,
}

impl ReplicationStream {
    /// Establish a new bidirectional stream to the given address.
    async fn connect(
        target_addr: &str,
        timeout: Duration,
    ) -> Result<Self, crate::db::rpc::errors::RpcError> {
        let pool = replication_channel_pool(timeout);
        let channel = pool.get_or_connect(target_addr).await?;
        let mut client = WrelaDbClient::new(channel);

        let (batch_tx, batch_rx) = mpsc::channel::<ReplicationStreamBatch>(256);
        let pending: Arc<
            TokioMutex<
                HashMap<
                    u64,
                    oneshot::Sender<Result<WriteBatchResponse, crate::db::rpc::errors::RpcError>>,
                >,
            >,
        > = Arc::new(TokioMutex::new(HashMap::new()));
        let dead = Arc::new(AtomicBool::new(false));

        // Convert mpsc::Receiver into a Stream for tonic.
        let outbound = tokio_stream::wrappers::ReceiverStream::new(batch_rx);

        let response = client
            .replicate_stream(tonic::Request::new(outbound))
            .await
            .map_err(status_to_rpc_error)?;

        let mut inbound = response.into_inner();
        let pending_for_reader = pending.clone();
        let dead_for_reader = dead.clone();

        // Spawn ack reader task.
        tokio::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(ack) => {
                        let resp = if ack.error.is_empty() {
                            Ok(WriteBatchResponse {
                                commit_version: ack.commit_version,
                                idempotent_replay: false,
                                follower_wal_fsync_ns: ack.follower_wal_fsync_ns,
                            })
                        } else {
                            Err(rpc_unavailable(format!("stream ack error: {}", ack.error)))
                        };
                        let mut guard = pending_for_reader.lock().await;
                        if let Some(tx) = guard.remove(&ack.sequence) {
                            let _ = tx.send(resp);
                        }
                    }
                    Err(_) => {
                        // Stream is dead — fail all pending.
                        dead_for_reader.store(true, Ordering::Relaxed);
                        let mut guard = pending_for_reader.lock().await;
                        for (_, tx) in guard.drain() {
                            let _ = tx.send(Err(rpc_unavailable("replication stream closed")));
                        }
                        break;
                    }
                }
            }
            // Stream ended normally — mark dead so pool re-establishes.
            dead_for_reader.store(true, Ordering::Relaxed);
            let mut guard = pending_for_reader.lock().await;
            for (_, tx) in guard.drain() {
                let _ = tx.send(Err(rpc_unavailable("replication stream ended")));
            }
        });

        Ok(Self {
            batch_tx,
            pending,
            next_sequence: Arc::new(AtomicU64::new(1)),
            dead,
        })
    }

    fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    /// Send a batch and return a future that resolves with the ack.
    async fn send(
        &self,
        mut batch: ReplicationStreamBatch,
    ) -> Result<WriteBatchResponse, crate::db::rpc::errors::RpcError> {
        let seq = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        batch.sequence = seq;

        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.pending.lock().await;
            guard.insert(seq, tx);
        }

        if self.batch_tx.send(batch).await.is_err() {
            // Channel closed — remove pending and fail.
            let mut guard = self.pending.lock().await;
            guard.remove(&seq);
            return Err(rpc_unavailable("replication stream send channel closed"));
        }

        rx.await
            .unwrap_or_else(|_| Err(rpc_unavailable("replication stream ack channel dropped")))
    }
}

/// Pool of persistent replication streams, one per target address.
pub struct ReplicationStreamPool {
    streams: TokioRwLock<HashMap<String, Arc<ReplicationStream>>>,
    timeout: Duration,
}

static REPLICATION_STREAM_POOL: OnceLock<Arc<ReplicationStreamPool>> = OnceLock::new();

fn replication_stream_pool(timeout: Duration) -> Arc<ReplicationStreamPool> {
    REPLICATION_STREAM_POOL
        .get_or_init(|| {
            Arc::new(ReplicationStreamPool {
                streams: TokioRwLock::new(HashMap::new()),
                timeout,
            })
        })
        .clone()
}

impl ReplicationStreamPool {
    async fn get_or_connect(
        &self,
        target_addr: &str,
    ) -> Result<Arc<ReplicationStream>, crate::db::rpc::errors::RpcError> {
        // Fast path: read lock.
        {
            let guard = self.streams.read().await;
            if let Some(stream) = guard.get(target_addr) {
                if !stream.is_dead() {
                    return Ok(stream.clone());
                }
            }
        }
        // Slow path: write lock, reconnect.
        let mut guard = self.streams.write().await;
        // Double-check after acquiring write lock.
        if let Some(stream) = guard.get(target_addr) {
            if !stream.is_dead() {
                return Ok(stream.clone());
            }
        }
        let stream = Arc::new(ReplicationStream::connect(target_addr, self.timeout).await?);
        guard.insert(target_addr.to_string(), stream.clone());
        Ok(stream)
    }
}

/// Try to send a replication batch via persistent stream. Returns None if the
/// follower returned UNIMPLEMENTED (older version), in which case the caller
/// should fall back to the unary path.
/// Try to send a replication batch via persistent stream. Returns `None` on
/// any failure (connection, send, ack error), signalling the caller to fall
/// back to the unary RPC path. The streaming path is purely an optimisation;
/// it must never block the unary fallback.
pub async fn replicate_via_stream_async(
    target_addr: &str,
    proto_req: &crate::db::rpc::tonic_service::wrpc::WriteBatchRequest,
    timeout: Duration,
) -> Option<Result<WriteBatchResponse, crate::db::rpc::errors::RpcError>> {
    let pool = replication_stream_pool(timeout);
    let stream = match pool.get_or_connect(target_addr).await {
        Ok(s) => s,
        Err(_) => return None, // fall back to unary
    };

    let batch = ReplicationStreamBatch {
        sequence: 0, // will be set by send()
        ops: proto_req.ops.clone(),
        wal_payload: proto_req.wal_payload.clone().unwrap_or_default(),
        idempotency_token: proto_req.idempotency_token.clone(),
        handle: proto_req.handle,
        expected_home_epoch: proto_req.expected_home_epoch,
        expected_shard_map_epoch: proto_req.expected_shard_map_epoch,
        ownership_token: proto_req.ownership_token.clone(),
    };

    let result = tokio::time::timeout(timeout, stream.send(batch)).await;
    match result {
        Ok(Ok(resp)) => Some(Ok(resp)),
        Ok(Err(_)) | Err(_) => None, // any error → fall back to unary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::keyspace::encode_user_key;
    use crate::db::{close_db, open_db, read_point, resolve_owner};
    use bytes::Bytes;

    fn ownership_fence_for(handle: i64, key: &[u8]) -> (u64, u64, String) {
        let owner = resolve_owner(handle, b"core".to_vec(), key.to_vec()).expect("resolve owner");
        (
            owner.home_epoch,
            owner.shard_map_epoch,
            owner.ownership_token,
        )
    }

    #[test]
    fn private_rpc_round_trip_writes_and_reads_through_bound_handle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = open_db(dir.path()).expect("open db");
        let mut local_service = GrpcEdgeService::new("node-a", "node-a");
        local_service.bind_handle(handle);
        let service = Arc::new(RwLock::new(local_service));
        let mut server =
            start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(500))
                .expect("start server");
        let addr = server.listen_addr().to_string();

        std::thread::sleep(Duration::from_millis(100));
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-private");

        let first = write_batch_over_private_rpc(
            &addr,
            WriteBatchRequest {
                handle: 0,
                ops: vec![crate::db::types::BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-private"),
                    value: Bytes::from_static(b"v-private"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-private-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token: ownership_token.clone(),
            },
            Duration::from_secs(5),
        )
        .expect("first write");
        let replay = write_batch_over_private_rpc(
            &addr,
            WriteBatchRequest {
                handle: 999_999,
                ops: vec![crate::db::types::BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-private"),
                    value: Bytes::from_static(b"v-private"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-private-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
            },
            Duration::from_secs(5),
        )
        .expect("replay write");
        assert_eq!(first.commit_version, replay.commit_version);
        assert!(replay.idempotent_replay);

        let remote_read = point_read_over_private_rpc(
            &addr,
            PointReadRequest {
                handle: 0,
                namespace: b"core".to_vec(),
                key: b"k-private".to_vec(),
            },
            Duration::from_secs(5),
        )
        .expect("point read");
        assert_eq!(remote_read, Some(b"v-private".to_vec()));
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-private".to_vec()).expect("local read"),
            Some(b"v-private".to_vec())
        );

        server.shutdown();
        assert!(close_db(handle));
    }

    #[test]
    fn private_rpc_replica_write_succeeds_on_follower_service() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = open_db(dir.path()).expect("open db");
        let mut local_service = GrpcEdgeService::new("node-b", "node-a");
        local_service.bind_handle(handle);
        let service = Arc::new(RwLock::new(local_service));
        let mut server =
            start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(500))
                .expect("start server");
        let addr = server.listen_addr().to_string();

        std::thread::sleep(Duration::from_millis(100));
        let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
            ownership_fence_for(handle, b"k-replica-private");

        let first = replicate_write_batch_over_private_rpc(
            &addr,
            WriteBatchRequest {
                handle: 0,
                ops: vec![crate::db::types::BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-replica-private"),
                    value: Bytes::from_static(b"v-replica-private"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-replica-private-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token: ownership_token.clone(),
            },
            Duration::from_secs(5),
        )
        .expect("first replica write");
        let replay = replicate_write_batch_over_private_rpc(
            &addr,
            WriteBatchRequest {
                handle: 4242,
                ops: vec![crate::db::types::BatchOp::Put {
                    namespace: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k-replica-private"),
                    value: Bytes::from_static(b"v-replica-private"),
                    expected_version: None,
                }],
                idempotency_token: Some("tok-replica-private-1".to_string()),
                expected_home_epoch,
                expected_shard_map_epoch,
                ownership_token,
            },
            Duration::from_secs(5),
        )
        .expect("replay replica write");
        assert!(
            replay.commit_version >= first.commit_version,
            "replica replay produces a new version (idempotency dedup is leader-only)"
        );
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-replica-private".to_vec()).expect("read"),
            Some(b"v-replica-private".to_vec())
        );

        server.shutdown();
        assert!(close_db(handle));
    }

    #[test]
    fn private_rpc_point_read_ignores_requested_handle_when_service_is_bound() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = open_db(dir.path()).expect("open db");
        crate::db::submit_put(
            handle,
            b"core".to_vec(),
            b"k-bound-read".to_vec(),
            b"v-bound-read".to_vec(),
            None,
        )
        .expect("seed put");
        let mut local_service = GrpcEdgeService::new("node-a", "node-a");
        local_service.bind_handle(handle);
        let service = Arc::new(RwLock::new(local_service));
        let mut server =
            start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(500))
                .expect("start server");
        let addr = server.listen_addr().to_string();

        std::thread::sleep(Duration::from_millis(100));

        let value = point_read_over_private_rpc(
            &addr,
            PointReadRequest {
                handle: 999_999,
                namespace: b"core".to_vec(),
                key: b"k-bound-read".to_vec(),
            },
            Duration::from_secs(5),
        )
        .expect("point read");
        assert_eq!(value, Some(b"v-bound-read".to_vec()));

        server.shutdown();
        assert!(close_db(handle));
    }

    #[test]
    fn txt_machine_id_parser_accepts_plain_ids_and_vm_host_tokens() {
        let records = vec![
            "f12345ab machine_id=f98765cd".to_string(),
            "f45678ef.vm.demo.internal".to_string(),
            "foo=ignored bad!token".to_string(),
        ];
        let ids = parse_machine_ids_from_txt_records(&records, "demo");
        assert_eq!(
            ids,
            vec![
                "f12345ab".to_string(),
                "f45678ef".to_string(),
                "f98765cd".to_string()
            ]
        );
    }

    #[test]
    fn txt_machine_id_parser_rejects_invalid_tokens() {
        let records = vec![
            "machine_id=UPPERCASE".to_string(),
            "machine_id=two.words".to_string(),
            "machine_id=ab".to_string(),
            "machine_id=".to_string(),
        ];
        let ids = parse_machine_ids_from_txt_records(&records, "demo");
        assert!(ids.is_empty());
    }

    #[test]
    fn fly_private_rpc_addresses_build_expected_map() {
        let ids = vec!["m-1".to_string(), "m-2".to_string()];
        let map = fly_private_rpc_addresses(&ids, "demo-app", 19091);
        assert_eq!(
            map.get("m-1").map(String::as_str),
            Some("m-1.vm.demo-app.internal:19091")
        );
        assert_eq!(
            map.get("m-2").map(String::as_str),
            Some("m-2.vm.demo-app.internal:19091")
        );
    }

    #[test]
    fn private_rpc_sorted_run_chunk_handles_rejection_then_replay_convergence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = open_db(dir.path()).expect("open db");
        let mut local_service = GrpcEdgeService::new("node-b", "node-a");
        local_service.bind_handle(handle);
        let service = Arc::new(RwLock::new(local_service));
        let mut server =
            start_private_rpc_server("127.0.0.1:0", service, Duration::from_millis(500))
                .expect("start server");
        let addr = server.listen_addr().to_string();
        std::thread::sleep(Duration::from_millis(100));

        let key1 = encode_user_key(b"core", b"k-rpc-sorted-1").expect("encode key1");
        let key2 = encode_user_key(b"core", b"k-rpc-sorted-2").expect("encode key2");
        let chunk0 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key1,
                41,
                b"v1".to_vec(),
                None,
            )]);
        let chunk1 =
            crate::db::lsm::sstable::encode_block(&[crate::db::lsm::sstable::SsTableEntry::live(
                key2,
                42,
                b"v2".to_vec(),
                None,
            )]);

        let stale = replica_install_sorted_run_chunk_over_private_rpc(
            &addr,
            crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
                handle: 0,
                term: 0,
                chunk_stream_id: 901,
                chunk_index: 0,
                total_chunks: 2,
                payload: chunk0.clone().into(),
            },
            Duration::from_secs(5),
        )
        .expect("stale call should still return typed rejection");
        assert!(!stale.accepted);
        assert_eq!(stale.rejection_reason, "SORTED_RUN_STALE_TERM_REJECTED");

        let out_of_order = replica_install_sorted_run_chunk_over_private_rpc(
            &addr,
            crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
                handle: 0,
                term: 9,
                chunk_stream_id: 901,
                chunk_index: 1,
                total_chunks: 2,
                payload: chunk1.clone().into(),
            },
            Duration::from_secs(5),
        )
        .expect("out-of-order call");
        assert!(!out_of_order.accepted);
        assert_eq!(
            out_of_order.rejection_reason,
            "SORTED_RUN_OUT_OF_ORDER_CHUNK"
        );
        assert_eq!(out_of_order.next_chunk_index, 0);

        let first = replica_install_sorted_run_chunk_over_private_rpc(
            &addr,
            crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
                handle: 0,
                term: 9,
                chunk_stream_id: 901,
                chunk_index: 0,
                total_chunks: 2,
                payload: chunk0.clone().into(),
            },
            Duration::from_secs(5),
        )
        .expect("chunk0");
        assert!(first.accepted);
        assert_eq!(first.next_chunk_index, 1);

        let replay = replica_install_sorted_run_chunk_over_private_rpc(
            &addr,
            crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
                handle: 0,
                term: 9,
                chunk_stream_id: 901,
                chunk_index: 0,
                total_chunks: 2,
                payload: chunk0.into(),
            },
            Duration::from_secs(5),
        )
        .expect("chunk0 replay");
        assert!(replay.accepted);
        assert_eq!(replay.next_chunk_index, 1);

        let second = replica_install_sorted_run_chunk_over_private_rpc(
            &addr,
            crate::db::rpc::tonic_service::wrpc::SortedRunCatchUpChunkRequest {
                handle: 0,
                term: 9,
                chunk_stream_id: 901,
                chunk_index: 1,
                total_chunks: 2,
                payload: chunk1.into(),
            },
            Duration::from_secs(5),
        )
        .expect("chunk1");
        assert!(second.accepted);
        assert_eq!(second.next_chunk_index, 2);

        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-rpc-sorted-1".to_vec()).expect("read key1"),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            read_point(handle, b"core".to_vec(), b"k-rpc-sorted-2".to_vec()).expect("read key2"),
            Some(b"v2".to_vec())
        );

        server.shutdown();
        assert!(close_db(handle));
    }
}
