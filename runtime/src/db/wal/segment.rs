#[cfg(test)]
use crate::db::wal::format::MAGIC;
use crate::db::wal::format::{HEADER_BYTES, Record, decode_at, encode_to, has_wal_magic_at};
use crate::db::{
    DEFAULT_WAL_GROUP_COMMIT_MAX_BYTES, DEFAULT_WAL_GROUP_COMMIT_MAX_OPS,
    DEFAULT_WAL_GROUP_COMMIT_WINDOW_US, DEFAULT_WAL_SEGMENT_PREALLOCATE_BYTES,
    DEFAULT_WAL_WRITEV_ENABLED,
};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, IoSlice, Read, Seek, SeekFrom, Write};
#[allow(unused_imports)]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(all(not(test), target_os = "linux"))]
use tokio_uring::fs::File as UringFile;

/// macOS: use F_BARRIERFSYNC (faster) instead of F_FULLFSYNC via sync_data().
/// Guarantees write ordering without full NVMe controller flush.
#[cfg(target_os = "macos")]
fn wal_sync_data(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    const F_BARRIERFSYNC: i32 = 85; // from bsd/sys/fcntl.h
    let ret = unsafe { libc::fcntl(file.as_raw_fd(), F_BARRIERFSYNC) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
fn wal_sync_data(file: &File) -> io::Result<()> {
    file.sync_data()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// Stop at the first decode error (current default behavior).
    StopAtCorruption,
    /// Skip corrupt regions and attempt to recover trailing records.
    SkipCorruption,
}

impl Default for ReplayMode {
    fn default() -> Self {
        Self::StopAtCorruption
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRegion {
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Clone)]
pub struct ReplayResult {
    pub records: Vec<Record>,
    pub skipped: Vec<SkippedRegion>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WalAppendMetrics {
    pub queue_wait_ns: u64,
    pub encode_ns: u64,
    pub fdatasync_ns: u64,
    pub mutex_wait_ns: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalFlushStats {
    pub flushes: u64,
    pub avg_ops_per_flush: f64,
    pub avg_bytes_per_flush: f64,
    pub fsync_failures: u64,
    pub forced_flushes_on_close: u64,
}

#[derive(Debug, Clone, Copy)]
struct WalGroupCommitConfig {
    window: Duration,
    max_ops: usize,
    max_bytes: usize,
    preallocate_bytes: usize,
    writev_enabled: bool,
}

impl Default for WalGroupCommitConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_micros(DEFAULT_WAL_GROUP_COMMIT_WINDOW_US),
            max_ops: DEFAULT_WAL_GROUP_COMMIT_MAX_OPS,
            max_bytes: DEFAULT_WAL_GROUP_COMMIT_MAX_BYTES,
            preallocate_bytes: DEFAULT_WAL_SEGMENT_PREALLOCATE_BYTES,
            writev_enabled: DEFAULT_WAL_WRITEV_ENABLED,
        }
    }
}

/// Completion for a WAL append, yielded when the flush finishes.
#[derive(Debug, Clone)]
pub struct WalBatchCompletion {
    pub offset: u64,
    pub metrics: WalAppendMetrics,
    pub completed_at: Instant,
}

#[derive(Debug)]
struct WalBatchRequest {
    bytes: Vec<u8>,
    ops: usize,
    encode_ns: u64,
    enqueued_at: Instant,
    tx: Sender<io::Result<WalBatchCompletion>>,
}

#[derive(Debug)]
struct WalBarrierRequest {
    forced: bool,
    tx: Sender<io::Result<()>>,
}

#[derive(Debug)]
enum WalRequest {
    Batch(WalBatchRequest),
    Barrier(WalBarrierRequest),
}

#[derive(Debug, Default)]
struct WalFlushStatsAccum {
    flushes: u64,
    total_ops: u128,
    total_bytes: u128,
    fsync_failures: u64,
    forced_flushes_on_close: u64,
}

impl WalFlushStatsAccum {
    fn snapshot(&self) -> WalFlushStats {
        let (avg_ops_per_flush, avg_bytes_per_flush) = if self.flushes == 0 {
            (0.0, 0.0)
        } else {
            (
                self.total_ops as f64 / self.flushes as f64,
                self.total_bytes as f64 / self.flushes as f64,
            )
        };
        WalFlushStats {
            flushes: self.flushes,
            avg_ops_per_flush,
            avg_bytes_per_flush,
            fsync_failures: self.fsync_failures,
            forced_flushes_on_close: self.forced_flushes_on_close,
        }
    }
}

#[derive(Debug)]
struct WalCoordinatorState {
    queue: VecDeque<WalRequest>,
    stop: bool,
    stats: WalFlushStatsAccum,
}

#[derive(Debug)]
struct WalCoordinator {
    state: Mutex<WalCoordinatorState>,
    cv: Condvar,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Debug)]
pub struct WalSegment {
    file: Arc<Mutex<File>>,
    coordinator: Arc<WalCoordinator>,
    #[cfg(test)]
    failpoints: Arc<Mutex<WalTestFailpoints>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct WalTestFailpoints {
    fail_before_batch_write: bool,
    fail_on_sync: bool,
    fail_after_records: Option<usize>,
}

impl WalSegment {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        let file = Arc::new(Mutex::new(file));
        let config = WalGroupCommitConfig::default();
        #[cfg(test)]
        let failpoints = Arc::new(Mutex::new(WalTestFailpoints::default()));

        let coordinator = Arc::new(WalCoordinator {
            state: Mutex::new(WalCoordinatorState {
                queue: VecDeque::new(),
                stop: false,
                stats: WalFlushStatsAccum::default(),
            }),
            cv: Condvar::new(),
            threads: Mutex::new(Vec::new()),
        });

        #[cfg(test)]
        {
            let thread_file = file.clone();
            let thread_coordinator = coordinator.clone();
            let thread_failpoints = failpoints.clone();
            let thread_config = config;
            let worker = thread::Builder::new()
                .name("wrela-wal-flush".to_string())
                .spawn(move || {
                    wal_flush_loop(
                        thread_file,
                        thread_coordinator,
                        thread_config,
                        Some(thread_failpoints),
                    );
                })?;
            if let Ok(mut guard) = coordinator.threads.lock() {
                guard.push(worker);
            }
        }

        #[cfg(all(not(test), target_os = "linux"))]
        {
            if env_bool("WRELADB_WAL_URING_ENABLED", true) {
                if let Err(e) = start_wal_flush_uring(
                    path.to_path_buf(),
                    file.clone(),
                    coordinator.clone(),
                    config,
                ) {
                    super::super::runtime_startup_trace(format!(
                        "WAL tokio-uring init failed, falling back to sync: {}",
                        e
                    ));
                    start_wal_flush_sync_fallback(file.clone(), coordinator.clone(), config);
                }
            } else {
                start_wal_flush_sync_fallback(file.clone(), coordinator.clone(), config);
            }
        }

        #[cfg(all(not(test), not(target_os = "linux")))]
        {
            start_wal_flush_sync_fallback(file.clone(), coordinator.clone(), config);
        }

        Ok(Self {
            file,
            coordinator,
            #[cfg(test)]
            failpoints,
        })
    }

    #[cfg(test)]
    fn open_with_config(path: &Path, config: WalGroupCommitConfig) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        let file = Arc::new(Mutex::new(file));
        let failpoints = Arc::new(Mutex::new(WalTestFailpoints::default()));
        let coordinator = Arc::new(WalCoordinator {
            state: Mutex::new(WalCoordinatorState {
                queue: VecDeque::new(),
                stop: false,
                stats: WalFlushStatsAccum::default(),
            }),
            cv: Condvar::new(),
            threads: Mutex::new(Vec::new()),
        });
        let thread_file = file.clone();
        let thread_coordinator = coordinator.clone();
        let thread_failpoints = failpoints.clone();
        let worker = thread::Builder::new()
            .name("wrela-wal-flush-test".to_string())
            .spawn(move || {
                wal_flush_loop(
                    thread_file,
                    thread_coordinator,
                    config,
                    Some(thread_failpoints),
                )
            })?;
        if let Ok(mut guard) = coordinator.threads.lock() {
            guard.push(worker);
        }
        Ok(Self {
            file,
            coordinator,
            failpoints,
        })
    }

    pub fn append(&self, record: &Record) -> io::Result<u64> {
        self.append_batch(std::slice::from_ref(record))
    }

    pub fn append_batch(&self, records: &[Record]) -> io::Result<u64> {
        self.append_batch_with_metrics(records)
            .map(|result| result.0)
    }

    pub fn append_batch_with_metrics(
        &self,
        records: &[Record],
    ) -> io::Result<(u64, WalAppendMetrics)> {
        let estimated_len: usize = records
            .iter()
            .map(|record| {
                HEADER_BYTES + record.namespace.len() + record.key.len() + record.value.len()
            })
            .sum();
        let mut bytes = Vec::with_capacity(estimated_len);
        let encode_started = Instant::now();
        #[cfg(test)]
        for (record_idx, record) in records.iter().enumerate() {
            let fail_after_records = self
                .failpoints
                .lock()
                .expect("WAL failpoint lock")
                .fail_after_records;
            if let Some(limit) = fail_after_records
                && record_idx >= limit
            {
                return Err(io::Error::other("injected wal batch write failure"));
            }
            encode_to(record, &mut bytes);
        }
        #[cfg(not(test))]
        for record in records {
            encode_to(record, &mut bytes);
        }
        let encode_ns = encode_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;

        #[cfg(test)]
        {
            let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
            if failpoints.fail_before_batch_write {
                failpoints.fail_before_batch_write = false;
                return Err(io::Error::other("injected wal write failure"));
            }
        }

        let (tx, rx) = bounded(1);
        let request = WalRequest::Batch(WalBatchRequest {
            bytes,
            ops: records.len(),
            encode_ns,
            enqueued_at: Instant::now(),
            tx,
        });

        {
            let mut state = self
                .coordinator
                .state
                .lock()
                .map_err(|_| io::Error::other("WAL coordinator lock poisoned"))?;
            if state.stop {
                return Err(io::Error::other("WAL coordinator stopped"));
            }
            state.queue.push_back(request);
            self.coordinator.cv.notify_one();
        }

        let completion = rx
            .recv()
            .map_err(|_| io::Error::other("WAL flush coordinator dropped completion"))??;
        Ok((completion.offset, completion.metrics))
    }

    /// Append pre-encoded WAL bytes. Caller must have already encoded records via `encode_to`.
    /// Used when WAL encoding is done under a lock and sync is deferred to outside the lock.
    pub fn append_raw_bytes_with_metrics(
        &self,
        bytes: Vec<u8>,
        ops: usize,
        encode_ns: u64,
    ) -> io::Result<(u64, WalAppendMetrics)> {
        self.append_raw_bytes_with_metrics_slice(&bytes, ops, encode_ns)
    }

    /// Like `append_raw_bytes_with_metrics` but takes a slice; copies internally so the caller
    /// can reuse a thread-local buffer.
    pub fn append_raw_bytes_with_metrics_slice(
        &self,
        bytes: &[u8],
        ops: usize,
        encode_ns: u64,
    ) -> io::Result<(u64, WalAppendMetrics)> {
        #[cfg(test)]
        {
            let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
            if failpoints.fail_before_batch_write {
                failpoints.fail_before_batch_write = false;
                return Err(io::Error::other("injected wal write failure"));
            }
        }

        let (tx, rx) = bounded(1);
        let request = WalRequest::Batch(WalBatchRequest {
            bytes: bytes.to_vec(),
            ops,
            encode_ns,
            enqueued_at: Instant::now(),
            tx,
        });

        {
            let mut state = self
                .coordinator
                .state
                .lock()
                .map_err(|_| io::Error::other("WAL coordinator lock poisoned"))?;
            if state.stop {
                return Err(io::Error::other("WAL coordinator stopped"));
            }
            state.queue.push_back(request);
            self.coordinator.cv.notify_one();
        }

        let completion = rx
            .recv()
            .map_err(|_| io::Error::other("WAL flush coordinator dropped completion"))??;
        Ok((completion.offset, completion.metrics))
    }

    /// Submit WAL bytes for flush without blocking. Returns a receiver that yields
    /// the completion when the flush finishes. Used for pipelined writer lanes.
    pub fn append_raw_bytes_submit(
        &self,
        bytes: Vec<u8>,
        ops: usize,
        encode_ns: u64,
    ) -> io::Result<Receiver<io::Result<WalBatchCompletion>>> {
        self.append_raw_bytes_submit_slice(&bytes, ops, encode_ns)
    }

    /// Like `append_raw_bytes_submit` but takes a slice; copies internally so the caller
    /// can reuse a thread-local buffer.
    pub fn append_raw_bytes_submit_slice(
        &self,
        bytes: &[u8],
        ops: usize,
        encode_ns: u64,
    ) -> io::Result<Receiver<io::Result<WalBatchCompletion>>> {
        #[cfg(test)]
        {
            let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
            if failpoints.fail_before_batch_write {
                failpoints.fail_before_batch_write = false;
                return Err(io::Error::other("injected wal write failure"));
            }
        }

        let (tx, rx) = bounded(1);
        let request = WalRequest::Batch(WalBatchRequest {
            bytes: bytes.to_vec(),
            ops,
            encode_ns,
            enqueued_at: Instant::now(),
            tx,
        });

        {
            let mut state = self
                .coordinator
                .state
                .lock()
                .map_err(|_| io::Error::other("WAL coordinator lock poisoned"))?;
            if state.stop {
                return Err(io::Error::other("WAL coordinator stopped"));
            }
            state.queue.push_back(request);
            self.coordinator.cv.notify_one();
        }

        Ok(rx)
    }

    /// Bypass the coordinator pipeline entirely: lock the file, seek to end,
    /// write, fsync, and return. Used by the follower direct path where only
    /// one writer exists per RPC — the coordinator's queue, condvar, linger,
    /// and two-thread write/sync pipeline are pure overhead in that case.
    pub fn write_and_sync_direct(
        &self,
        bytes: &[u8],
        ops: usize,
    ) -> io::Result<(u64, WalAppendMetrics)> {
        let started = Instant::now();
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("WAL file lock poisoned"))?;
        let mutex_wait_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;

        let offset = file.seek(SeekFrom::End(0))? as u64;
        file.write_all(bytes)?;
        let write_done = Instant::now();

        wal_sync_data(&file)?;
        let fdatasync_ns = write_done.elapsed().as_nanos().min(u64::MAX as u128) as u64;

        // Update coordinator stats for observability.
        if let Ok(mut state) = self.coordinator.state.lock() {
            state.stats.flushes += 1;
            state.stats.total_ops += ops as u128;
            state.stats.total_bytes += bytes.len() as u128;
        }

        Ok((
            offset,
            WalAppendMetrics {
                queue_wait_ns: 0,
                encode_ns: 0,
                fdatasync_ns,
                mutex_wait_ns,
            },
        ))
    }

    /// Write bytes to WAL without syncing. Returns the file offset before the
    /// write. Used by the pipelined follower path to overlap memtable apply
    /// with the subsequent sync.
    pub fn write_direct(&self, bytes: &[u8], ops: usize) -> io::Result<u64> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("WAL file lock poisoned"))?;
        let offset = file.seek(SeekFrom::End(0))? as u64;
        file.write_all(bytes)?;

        // Update coordinator stats for observability.
        if let Ok(mut state) = self.coordinator.state.lock() {
            state.stats.flushes += 1;
            state.stats.total_ops += ops as u128;
            state.stats.total_bytes += bytes.len() as u128;
        }

        Ok(offset)
    }

    /// Sync (fdatasync) the WAL file without writing. Returns the sync
    /// duration in nanoseconds. Used after `write_direct` once the memtable
    /// apply has completed, preserving the Raft durability invariant (ack is
    /// only sent after sync).
    pub fn sync_direct(&self) -> io::Result<u64> {
        let started = Instant::now();
        let file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("WAL file lock poisoned"))?;
        wal_sync_data(&file)?;
        Ok(started.elapsed().as_nanos().min(u64::MAX as u128) as u64)
    }

    pub fn force_flush(&self) -> io::Result<()> {
        self.enqueue_barrier(false)
    }

    pub fn force_flush_on_close(&self) -> io::Result<()> {
        self.enqueue_barrier(true)
    }

    fn enqueue_barrier(&self, forced: bool) -> io::Result<()> {
        let (tx, rx) = bounded(1);
        {
            let mut state = self
                .coordinator
                .state
                .lock()
                .map_err(|_| io::Error::other("WAL coordinator lock poisoned"))?;
            if state.stop {
                return Err(io::Error::other("WAL coordinator stopped"));
            }
            state
                .queue
                .push_back(WalRequest::Barrier(WalBarrierRequest { forced, tx }));
            self.coordinator.cv.notify_one();
        }
        rx.recv()
            .map_err(|_| io::Error::other("WAL barrier completion dropped"))??;
        Ok(())
    }

    pub fn flush_stats(&self) -> WalFlushStats {
        self.coordinator
            .state
            .lock()
            .map(|state| state.stats.snapshot())
            .unwrap_or_default()
    }

    pub fn replay(&self) -> io::Result<Vec<Record>> {
        let result = self.replay_with_mode(ReplayMode::StopAtCorruption)?;
        Ok(result.records)
    }

    pub fn replay_with_mode(&self, mode: ReplayMode) -> io::Result<ReplayResult> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("WAL lock poisoned"))?;
        file.seek(SeekFrom::Start(0))?;
        let mut records = Vec::new();
        let mut skipped = Vec::new();
        let mut bytes = Vec::new();
        let mut offset = 0usize;

        loop {
            let mut chunk = [0u8; 8192];
            let read = file.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            Self::decode_loop(&bytes, &mut offset, &mut records, &mut skipped, mode)?;
            if offset >= 64 * 1024 {
                bytes.drain(..offset);
                offset = 0;
            }
        }

        Self::decode_loop(&bytes, &mut offset, &mut records, &mut skipped, mode)?;
        Ok(ReplayResult { records, skipped })
    }

    pub fn shutdown(&self) {
        if let Ok(mut state) = self.coordinator.state.lock() {
            state.stop = true;
            self.coordinator.cv.notify_all();
        }
        if let Ok(mut threads) = self.coordinator.threads.lock() {
            for worker in threads.drain(..) {
                let _ = worker.join();
            }
        }
    }

    const MAX_SCAN_ATTEMPTS: usize = 64;

    fn decode_loop(
        bytes: &[u8],
        offset: &mut usize,
        records: &mut Vec<Record>,
        skipped: &mut Vec<SkippedRegion>,
        mode: ReplayMode,
    ) -> io::Result<()> {
        loop {
            match decode_at(bytes, *offset) {
                Ok(Some((record, next))) => {
                    records.push(record);
                    *offset = next;
                }
                Ok(None) => break,
                Err(_) if mode == ReplayMode::SkipCorruption => {
                    let skip_start = *offset;
                    let mut scan_from = *offset + 1;
                    let mut attempts = 0;
                    let found = loop {
                        if attempts >= Self::MAX_SCAN_ATTEMPTS {
                            break false;
                        }
                        attempts += 1;
                        match Self::scan_for_next_magic(bytes, scan_from) {
                            Some(candidate) => match decode_at(bytes, candidate) {
                                Ok(Some(_)) => {
                                    skipped.push(SkippedRegion {
                                        offset: skip_start,
                                        length: candidate - skip_start,
                                    });
                                    *offset = candidate;
                                    break true;
                                }
                                _ => {
                                    scan_from = candidate + 1;
                                }
                            },
                            None => break false,
                        }
                    };
                    if !found {
                        skipped.push(SkippedRegion {
                            offset: skip_start,
                            length: bytes.len() - skip_start,
                        });
                        *offset = bytes.len();
                        break;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    fn scan_for_next_magic(bytes: &[u8], start: usize) -> Option<usize> {
        if bytes.len() < HEADER_BYTES {
            return None;
        }
        let end = bytes.len().saturating_sub(4);
        for i in start..=end {
            if has_wal_magic_at(bytes, i) {
                return Some(i);
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn fail_next_batch_write(&self) {
        let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
        failpoints.fail_before_batch_write = true;
    }

    #[cfg(test)]
    pub(crate) fn fail_next_sync(&self) {
        let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
        failpoints.fail_on_sync = true;
    }

    #[cfg(test)]
    pub(crate) fn fail_batch_after_records(&self, record_count: usize) {
        let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
        failpoints.fail_after_records = Some(record_count);
    }

    #[cfg(test)]
    pub(crate) fn clear_failpoints(&self) {
        let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
        *failpoints = WalTestFailpoints::default();
    }
}

impl Drop for WalSegment {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
type FlushFailpointsHandle = Arc<Mutex<WalTestFailpoints>>;

#[cfg(not(test))]
type FlushFailpointsHandle = ();

/// Message from bridge to consumer: batch of work, barrier, or shutdown.
#[cfg(all(not(test), target_os = "linux"))]
enum FlushMessage {
    Work(Vec<WalBatchRequest>),
    Barrier(WalBarrierRequest),
    Shutdown,
}

#[cfg(all(not(test), target_os = "linux"))]
fn start_wal_flush_uring(
    path: PathBuf,
    _file: Arc<Mutex<File>>,
    coordinator: Arc<WalCoordinator>,
    config: WalGroupCommitConfig,
) -> io::Result<()> {
    let (tx, rx) = flume::bounded::<FlushMessage>(4);
    let bridge_coord = coordinator.clone();
    let consumer_coord = coordinator.clone();
    let bridge = thread::Builder::new()
        .name("wrela-wal-bridge".to_string())
        .spawn(move || wal_bridge_loop(bridge_coord, tx, config))?;
    let uring = thread::Builder::new()
        .name("wrela-wal-uring".to_string())
        .spawn(move || {
            tokio_uring::start(
                async move { wal_consumer_uring(path, rx, consumer_coord, config).await },
            )
        })?;
    coordinator
        .threads
        .lock()
        .map_err(|_| io::Error::other("threads lock poisoned"))?
        .extend([bridge, uring]);
    Ok(())
}

#[cfg(not(test))]
fn start_wal_flush_sync_fallback(
    file: Arc<Mutex<File>>,
    coordinator: Arc<WalCoordinator>,
    config: WalGroupCommitConfig,
) {
    let coord = coordinator.clone();
    let worker = thread::Builder::new()
        .name("wrela-wal-flush".to_string())
        .spawn(move || wal_flush_loop(file, coord, config, None::<()>))
        .expect("spawn WAL flush thread");
    coordinator
        .threads
        .lock()
        .expect("threads lock")
        .push(worker);
}

#[cfg(all(not(test), target_os = "linux"))]
fn wal_bridge_loop(
    coordinator: Arc<WalCoordinator>,
    tx: flume::Sender<FlushMessage>,
    config: WalGroupCommitConfig,
) {
    loop {
        let msg = get_work_from_coordinator(&coordinator, &config);
        match msg {
            FlushMessage::Shutdown => {
                let _ = tx.send(FlushMessage::Shutdown);
                return;
            }
            other => {
                if tx.send(other).is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(all(not(test), target_os = "linux"))]
fn get_work_from_coordinator(
    coordinator: &WalCoordinator,
    config: &WalGroupCommitConfig,
) -> FlushMessage {
    loop {
        let mut state = match coordinator.state.lock() {
            Ok(s) => s,
            Err(_) => return FlushMessage::Shutdown,
        };
        while state.queue.is_empty() && !state.stop {
            state = match coordinator.cv.wait(state) {
                Ok(s) => s,
                Err(_) => return FlushMessage::Shutdown,
            };
        }
        if state.queue.is_empty() && state.stop {
            return FlushMessage::Shutdown;
        }
        if let Some(WalRequest::Barrier(barrier)) = state.queue.pop_front() {
            if barrier.forced {
                state.stats.forced_flushes_on_close =
                    state.stats.forced_flushes_on_close.saturating_add(1);
            }
            drop(state);
            let _ = barrier.tx.send(Ok(()));
            continue;
        }
        let started = Instant::now();
        let mut group = Vec::new();
        let mut total_ops = 0usize;
        let mut total_bytes = 0usize;
        while let Some(front) = state.queue.front() {
            match front {
                WalRequest::Barrier(_) => break,
                WalRequest::Batch(batch) => {
                    let next_ops = total_ops.saturating_add(batch.ops);
                    let next_bytes = total_bytes.saturating_add(batch.bytes.len());
                    let fits = group.is_empty()
                        || (next_ops <= config.max_ops && next_bytes <= config.max_bytes);
                    if !fits {
                        break;
                    }
                    let Some(WalRequest::Batch(req)) = state.queue.pop_front() else {
                        break;
                    };
                    total_ops = total_ops.saturating_add(req.ops);
                    total_bytes = total_bytes.saturating_add(req.bytes.len());
                    group.push(req);
                    if total_ops >= config.max_ops || total_bytes >= config.max_bytes {
                        break;
                    }
                }
            }
        }
        if !group.is_empty() && !config.window.is_zero() {
            while started.elapsed() < config.window {
                if matches!(state.queue.front(), Some(WalRequest::Barrier(_))) {
                    break;
                }
                match state.queue.front() {
                    Some(WalRequest::Batch(next)) => {
                        let next_ops = total_ops.saturating_add(next.ops);
                        let next_bytes = total_bytes.saturating_add(next.bytes.len());
                        if next_ops > config.max_ops || next_bytes > config.max_bytes {
                            break;
                        }
                        let Some(WalRequest::Batch(req)) = state.queue.pop_front() else {
                            break;
                        };
                        total_ops = total_ops.saturating_add(req.ops);
                        total_bytes = total_bytes.saturating_add(req.bytes.len());
                        group.push(req);
                        if total_ops >= config.max_ops || total_bytes >= config.max_bytes {
                            break;
                        }
                        continue;
                    }
                    Some(WalRequest::Barrier(_)) => break,
                    None => {
                        let timeout = config.window.saturating_sub(started.elapsed());
                        if timeout.is_zero() {
                            break;
                        }
                        let waited = coordinator.cv.wait_timeout(state, timeout);
                        let (new_state, wr) = match waited {
                            Ok(t) => t,
                            Err(_) => return FlushMessage::Shutdown,
                        };
                        state = new_state;
                        if wr.timed_out() {
                            break;
                        }
                    }
                }
            }
        }
        if !group.is_empty() {
            return FlushMessage::Work(group);
        }
    }
}

#[cfg(all(not(test), target_os = "linux"))]
async fn wal_consumer_uring(
    path: PathBuf,
    rx: flume::Receiver<FlushMessage>,
    coordinator: Arc<WalCoordinator>,
    config: WalGroupCommitConfig,
) {
    let file = match UringFile::open(path.clone()).await {
        Ok(f) => f,
        Err(e) => {
            let err_text = e.to_string();
            while let Ok(m) = rx.recv_async().await {
                match m {
                    FlushMessage::Work(work) => {
                        dispatch_group_error(work, io::Error::other(err_text.clone()));
                    }
                    FlushMessage::Barrier(barrier) => {
                        let _ = barrier.tx.send(Err(io::Error::other(err_text.clone())));
                    }
                    FlushMessage::Shutdown => break,
                }
            }
            return;
        }
    };
    let meta = std::fs::metadata(&path).ok();
    let mut offset = meta.map(|m| m.len()).unwrap_or(0);
    let mut preallocated_until = offset;
    loop {
        let msg = match rx.recv_async().await {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            FlushMessage::Shutdown => break,
            FlushMessage::Barrier(barrier) => {
                let _ = barrier.tx.send(Ok(()));
            }
            FlushMessage::Work(work) => {
                let flush_started = Instant::now();
                let total_ops: usize = work.iter().map(|r| r.ops).sum();
                let total_bytes: usize = work.iter().map(|r| r.bytes.len()).sum();
                // Transfer ownership of buffers to uring instead of cloning.
                let (bufs, completion_infos): (
                    Vec<Vec<u8>>,
                    Vec<(Sender<io::Result<WalBatchCompletion>>, u64, Instant)>,
                ) = work
                    .into_iter()
                    .map(|r| (r.bytes, (r.tx, r.encode_ns, r.enqueued_at)))
                    .unzip();
                let write_offset = offset;
                offset = offset.saturating_add(total_bytes as u64);
                preallocate_best_effort_uring(
                    &file,
                    &mut preallocated_until,
                    offset,
                    config.preallocate_bytes,
                )
                .await;
                let (res, bufs) = file.writev_at(bufs, write_offset).await;
                if let Err(e) = res {
                    if let Ok(mut state) = coordinator.state.lock() {
                        state.stats.fsync_failures = state.stats.fsync_failures.saturating_add(1);
                    }
                    let _bufs = bufs;
                    dispatch_group_error_from_completion_infos(
                        completion_infos,
                        io::Error::new(io::ErrorKind::Other, e.to_string()),
                    );
                    continue;
                }
                if let Err(e) = file.sync_data().await {
                    if let Ok(mut state) = coordinator.state.lock() {
                        state.stats.fsync_failures = state.stats.fsync_failures.saturating_add(1);
                    }
                    dispatch_group_error_from_completion_infos(
                        completion_infos,
                        io::Error::new(io::ErrorKind::Other, e.to_string()),
                    );
                    continue;
                }
                if let Ok(mut state) = coordinator.state.lock() {
                    state.stats.flushes = state.stats.flushes.saturating_add(1);
                    state.stats.total_ops = state.stats.total_ops.saturating_add(total_ops as u128);
                    state.stats.total_bytes =
                        state.stats.total_bytes.saturating_add(total_bytes as u128);
                }
                let fdatasync_ns = flush_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                let mut running_offset = write_offset;
                for ((tx, encode_ns, enqueued_at), buf) in completion_infos.into_iter().zip(bufs) {
                    let queue_wait_ns = flush_started
                        .saturating_duration_since(enqueued_at)
                        .as_nanos()
                        .min(u64::MAX as u128) as u64;
                    let _ = tx.send(Ok(WalBatchCompletion {
                        offset: running_offset,
                        metrics: WalAppendMetrics {
                            queue_wait_ns,
                            encode_ns,
                            fdatasync_ns,
                            mutex_wait_ns: 0,
                        },
                        completed_at: Instant::now(),
                    }));
                    running_offset = running_offset.saturating_add(buf.len() as u64);
                }
            }
        }
    }
    let _ = file.close().await;
}

#[cfg(all(not(test), target_os = "linux"))]
async fn preallocate_best_effort_uring(
    file: &UringFile,
    preallocated_until: &mut u64,
    write_end: u64,
    step: usize,
) {
    if step == 0 || write_end <= *preallocated_until {
        return;
    }
    let step = step as u64;
    let mut target = (*preallocated_until).max(write_end);
    while target < write_end.saturating_add(step) {
        target = target.saturating_add(step);
    }
    let off = *preallocated_until as u64;
    let len = target.saturating_sub(*preallocated_until);
    if len > 0 {
        if file
            .fallocate(off, len, libc::FALLOC_FL_KEEP_SIZE)
            .await
            .is_ok()
        {
            *preallocated_until = target;
        }
    }
}

/// Message sent from the flush loop to the sync thread so we can overlap
/// writing batch N+1 with fsync of batch N.
enum WalSyncMessage {
    Group {
        work: Vec<WalBatchRequest>,
        start_offset: u64,
        end_offset: u64,
        total_ops: usize,
        total_bytes: usize,
        flush_started: Instant,
        mutex_wait_ns: u64,
    },
    Barrier {
        write_offset: u64,
        tx: Sender<io::Result<()>>,
        forced: bool,
    },
}

fn wal_sync_loop(
    sync_rx: Receiver<WalSyncMessage>,
    file: Arc<Mutex<File>>,
    coordinator: Arc<WalCoordinator>,
    last_synced_offset: Arc<AtomicU64>,
) {
    let mut last_synced = last_synced_offset.load(Ordering::Relaxed);
    while let Ok(msg) = sync_rx.recv() {
        match msg {
            WalSyncMessage::Group {
                work,
                start_offset,
                end_offset,
                total_ops,
                total_bytes,
                flush_started,
                mutex_wait_ns,
            } => {
                let sync_started = Instant::now();
                let locked_file = match file.lock() {
                    Ok(guard) => guard,
                    Err(_) => {
                        dispatch_group_error(work, io::Error::other("WAL file lock poisoned"));
                        continue;
                    }
                };
                if let Err(err) = wal_sync_data(&locked_file) {
                    drop(locked_file);
                    if let Ok(mut state) = coordinator.state.lock() {
                        state.stats.fsync_failures = state.stats.fsync_failures.saturating_add(1);
                        state.stop = true;
                    }
                    coordinator.cv.notify_all();
                    dispatch_group_error(work, err);
                    return;
                }
                let fdatasync_ns = sync_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                last_synced = end_offset;
                last_synced_offset.store(last_synced, Ordering::Relaxed);
                drop(locked_file);

                if let Ok(mut state) = coordinator.state.lock() {
                    state.stats.flushes = state.stats.flushes.saturating_add(1);
                    state.stats.total_ops = state.stats.total_ops.saturating_add(total_ops as u128);
                    state.stats.total_bytes =
                        state.stats.total_bytes.saturating_add(total_bytes as u128);
                }

                let mut running_offset = start_offset;
                for request in work {
                    let queue_wait_ns = flush_started
                        .saturating_duration_since(request.enqueued_at)
                        .as_nanos()
                        .min(u64::MAX as u128) as u64;
                    let completion = WalBatchCompletion {
                        offset: running_offset,
                        metrics: WalAppendMetrics {
                            queue_wait_ns,
                            encode_ns: request.encode_ns,
                            fdatasync_ns,
                            mutex_wait_ns,
                        },
                        completed_at: Instant::now(),
                    };
                    running_offset = running_offset.saturating_add(request.bytes.len() as u64);
                    let _ = request.tx.send(Ok(completion));
                }
            }
            WalSyncMessage::Barrier {
                write_offset,
                tx: barrier_tx,
                forced,
            } => {
                if forced {
                    if let Ok(mut state) = coordinator.state.lock() {
                        state.stats.forced_flushes_on_close =
                            state.stats.forced_flushes_on_close.saturating_add(1);
                    }
                }
                if write_offset > last_synced {
                    if let Ok(locked_file) = file.lock() {
                        let _ = wal_sync_data(&locked_file);
                    }
                    last_synced = write_offset;
                    last_synced_offset.store(last_synced, Ordering::Relaxed);
                }
                let _ = barrier_tx.send(Ok(()));
            }
        }
    }
}

fn wal_flush_loop(
    file: Arc<Mutex<File>>,
    coordinator: Arc<WalCoordinator>,
    config: WalGroupCommitConfig,
    #[cfg(test)] failpoints: Option<FlushFailpointsHandle>,
    #[cfg(not(test))] _failpoints: Option<FlushFailpointsHandle>,
) {
    let mut preallocated_until = file
        .lock()
        .ok()
        .and_then(|locked| locked.metadata().ok())
        .map(|meta| meta.len())
        .unwrap_or(0);
    let mut write_offset = preallocated_until;
    let last_synced_atomic = Arc::new(AtomicU64::new(preallocated_until));
    let (sync_tx, sync_rx) = bounded(64);
    let sync_tx_for_cleanup = sync_tx.clone();
    let sync_handle = thread::Builder::new()
        .name("wrela-wal-sync".to_string())
        .spawn({
            let file = file.clone();
            let coordinator = coordinator.clone();
            let last_synced_offset = last_synced_atomic.clone();
            move || wal_sync_loop(sync_rx, file, coordinator, last_synced_offset)
        })
        .expect("spawn WAL sync thread");

    let mut cleanup = Some((sync_tx_for_cleanup, sync_handle));
    let mut do_return = || {
        if let Some((tx, handle)) = cleanup.take() {
            drop(tx);
            let _ = handle.join();
        }
    };

    loop {
        let work = {
            let mut state = match coordinator.state.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    drop(sync_tx);
                    do_return();
                    return;
                }
            };
            while state.queue.is_empty() && !state.stop {
                state = match coordinator.cv.wait(state) {
                    Ok(guard) => guard,
                    Err(_) => {
                        drop(sync_tx);
                        do_return();
                        return;
                    }
                };
            }
            if state.queue.is_empty() && state.stop {
                break;
            }

            if let Some(WalRequest::Barrier(_)) = state.queue.front()
                && let Some(WalRequest::Barrier(barrier)) = state.queue.pop_front()
            {
                drop(state);
                if sync_tx
                    .send(WalSyncMessage::Barrier {
                        write_offset,
                        tx: barrier.tx,
                        forced: barrier.forced,
                    })
                    .is_err()
                {
                    do_return();
                    return;
                }
                continue;
            }

            let started = Instant::now();
            let mut group = Vec::new();
            let mut total_ops = 0usize;
            let mut total_bytes = 0usize;

            while let Some(front) = state.queue.front() {
                match front {
                    WalRequest::Barrier(_) => break,
                    WalRequest::Batch(batch) => {
                        let next_ops = total_ops.saturating_add(batch.ops);
                        let next_bytes = total_bytes.saturating_add(batch.bytes.len());
                        let fits = group.is_empty()
                            || (next_ops <= config.max_ops && next_bytes <= config.max_bytes);
                        if !fits {
                            break;
                        }
                        let Some(WalRequest::Batch(request)) = state.queue.pop_front() else {
                            break;
                        };
                        total_ops = total_ops.saturating_add(request.ops);
                        total_bytes = total_bytes.saturating_add(request.bytes.len());
                        group.push(request);
                        if total_ops >= config.max_ops || total_bytes >= config.max_bytes {
                            break;
                        }
                    }
                }
            }

            // Adaptive: skip linger when we already drained a backlog (multiple batches).
            // Only wait for more when we got a single batch.
            let effective_window = if group.len() > 1
                || total_ops >= config.max_ops
                || total_bytes >= config.max_bytes
            {
                Duration::ZERO
            } else {
                config.window
            };

            if !group.is_empty() && !effective_window.is_zero() {
                while started.elapsed() < effective_window {
                    if matches!(state.queue.front(), Some(WalRequest::Barrier(_))) {
                        break;
                    }
                    match state.queue.front() {
                        Some(WalRequest::Batch(next)) => {
                            let next_ops = total_ops.saturating_add(next.ops);
                            let next_bytes = total_bytes.saturating_add(next.bytes.len());
                            if next_ops > config.max_ops || next_bytes > config.max_bytes {
                                break;
                            }
                            let Some(WalRequest::Batch(request)) = state.queue.pop_front() else {
                                break;
                            };
                            total_ops = total_ops.saturating_add(request.ops);
                            total_bytes = total_bytes.saturating_add(request.bytes.len());
                            group.push(request);
                            if total_ops >= config.max_ops || total_bytes >= config.max_bytes {
                                break;
                            }
                            continue;
                        }
                        Some(WalRequest::Barrier(_)) => break,
                        None => {
                            let timeout = effective_window.saturating_sub(started.elapsed());
                            if timeout.is_zero() {
                                break;
                            }
                            let waited = coordinator.cv.wait_timeout(state, timeout);
                            let (new_state, wait_result) = match waited {
                                Ok(tuple) => tuple,
                                Err(_) => return,
                            };
                            state = new_state;
                            if wait_result.timed_out() {
                                break;
                            }
                        }
                    }
                }
            }

            if group.is_empty() {
                continue;
            }

            drop(state);
            group
        };

        let flush_started = Instant::now();
        let total_ops = work.iter().map(|request| request.ops).sum::<usize>();
        let total_bytes = work
            .iter()
            .map(|request| request.bytes.len())
            .sum::<usize>();

        let mutex_wait_started = Instant::now();
        let mut locked_file = match file.lock() {
            Ok(guard) => guard,
            Err(_) => {
                let err = io::Error::other("WAL file lock poisoned");
                dispatch_group_error(work, err);
                continue;
            }
        };
        let mutex_wait_ns = mutex_wait_started
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64;

        let offset = write_offset;
        if locked_file.seek(SeekFrom::Start(offset)).is_err() {
            dispatch_group_error(work, io::Error::other("WAL seek failed"));
            continue;
        }

        preallocate_best_effort(
            &locked_file,
            &mut preallocated_until,
            offset.saturating_add(total_bytes as u64),
            config.preallocate_bytes,
        );

        if let Err(err) = write_group_bytes(&mut locked_file, &work, config.writev_enabled) {
            let _ = locked_file.set_len(offset);
            let _ = locked_file.seek(SeekFrom::Start(offset));
            dispatch_group_error(work, err);
            continue;
        }

        #[cfg(test)]
        {
            if let Some(handle) = failpoints.as_ref()
                && let Ok(mut points) = handle.lock()
                && points.fail_on_sync
            {
                points.fail_on_sync = false;
                let _ = locked_file.set_len(offset);
                let _ = locked_file.seek(SeekFrom::Start(offset));
                let err = io::Error::other("injected wal sync failure");
                if let Ok(mut state) = coordinator.state.lock() {
                    state.stats.fsync_failures = state.stats.fsync_failures.saturating_add(1);
                }
                dispatch_group_error(work, err);
                continue;
            }
        }

        let end_offset = offset.saturating_add(total_bytes as u64);
        drop(locked_file);
        let msg = WalSyncMessage::Group {
            work,
            start_offset: offset,
            end_offset,
            total_ops,
            total_bytes,
            flush_started,
            mutex_wait_ns,
        };
        if let Err(e) = sync_tx.send(msg) {
            if let WalSyncMessage::Group { work: w, .. } = e.into_inner() {
                dispatch_group_error(w, io::Error::other("WAL sync thread disconnected"));
            }
            drop(sync_tx);
            do_return();
            return;
        }
        write_offset = end_offset;
    }
    drop(sync_tx);
    do_return();
}

fn dispatch_group_error(group: Vec<WalBatchRequest>, err: io::Error) {
    let message = err.to_string();
    let kind = err.kind();
    for request in group {
        let _ = request.tx.send(Err(io::Error::new(kind, message.clone())));
    }
}

#[cfg(all(not(test), target_os = "linux"))]
fn dispatch_group_error_from_completion_infos(
    completion_infos: Vec<(Sender<io::Result<WalBatchCompletion>>, u64, Instant)>,
    err: io::Error,
) {
    let message = err.to_string();
    let kind = err.kind();
    for (tx, _, _) in completion_infos {
        let _ = tx.send(Err(io::Error::new(kind, message.clone())));
    }
}

fn write_group_bytes(
    file: &mut File,
    group: &[WalBatchRequest],
    writev_enabled: bool,
) -> io::Result<()> {
    if group.is_empty() {
        return Ok(());
    }
    if !writev_enabled {
        for request in group {
            file.write_all(&request.bytes)?;
        }
        return Ok(());
    }

    let mut request_idx = 0usize;
    let mut request_offset = 0usize;
    while request_idx < group.len() {
        let mut slices = Vec::new();
        let mut idx = request_idx;
        let mut offset = request_offset;
        while idx < group.len() && slices.len() < 1024 {
            let bytes = &group[idx].bytes;
            slices.push(IoSlice::new(&bytes[offset..]));
            idx += 1;
            offset = 0;
        }

        let written = file.write_vectored(&slices)?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "WAL vectored write produced zero bytes",
            ));
        }

        let mut remaining = written;
        while request_idx < group.len() {
            let current = &group[request_idx].bytes;
            let available = current.len().saturating_sub(request_offset);
            if remaining < available {
                request_offset = request_offset.saturating_add(remaining);
                break;
            }
            remaining = remaining.saturating_sub(available);
            request_idx = request_idx.saturating_add(1);
            request_offset = 0;
            if remaining == 0 {
                break;
            }
        }
    }
    Ok(())
}

fn preallocate_best_effort(file: &File, preallocated_until: &mut u64, write_end: u64, step: usize) {
    if step == 0 || write_end <= *preallocated_until {
        return;
    }
    let step = step as u64;
    let mut target = (*preallocated_until).max(write_end);
    while target < write_end.saturating_add(step) {
        target = target.saturating_add(step);
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd;
        let offset = *preallocated_until as libc::off_t;
        let len = target.saturating_sub(*preallocated_until) as libc::off_t;
        if len > 0 {
            let rc = unsafe { libc::posix_fallocate(file.as_raw_fd(), offset, len) };
            if rc == 0 {
                *preallocated_until = target;
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = (file, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::wal::format::encode;
    use bytes::Bytes;

    #[test]
    fn replay_truncates_torn_tail_without_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");
        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            version: 1,
        })
        .expect("append record");

        let partial = encode(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::from_static(b"v2"),
            version: 2,
        });
        let mut file = OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .expect("open for partial write");
        file.write_all(&partial[..partial.len() / 2])
            .expect("write torn tail");
        file.sync_data().expect("sync torn tail");

        let replayed = wal.replay().expect("replay");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].key, b"k1".to_vec());
    }

    #[test]
    fn replay_handles_large_logs_incrementally() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");
        for i in 0..10_000u64 {
            wal.append(&Record {
                kind: crate::db::wal::format::RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: format!("k{i}").into_bytes().into(),
                value: Bytes::from_static(b"v"),
                version: i,
            })
            .expect("append");
        }
        let replayed = wal.replay().expect("replay");
        assert_eq!(replayed.len(), 10_000);
    }

    #[test]
    fn replay_mid_file_corruption_default_stops_with_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        for i in 1..=2u64 {
            wal.append(&Record {
                kind: crate::db::wal::format::RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: format!("k{i}").into_bytes().into(),
                value: format!("v{i}").into_bytes().into(),
                version: i,
            })
            .expect("append");
        }

        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .expect("open for corruption");
            let mut corrupt = Vec::new();
            corrupt.extend_from_slice(&MAGIC);
            corrupt.push(0xFF);
            corrupt.extend_from_slice(&[0u8; HEADER_BYTES - 5]);
            file.write_all(&corrupt).expect("inject corruption");
            file.sync_data().expect("sync");
        }

        let result = wal.replay_with_mode(ReplayMode::StopAtCorruption);
        assert!(result.is_err(), "StopAtCorruption must propagate error");
    }

    #[test]
    fn fail_next_batch_write_preserves_pre_write_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            version: 1,
        })
        .expect("append first");

        wal.fail_next_batch_write();

        let err = wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::from_static(b"v2"),
            version: 2,
        });
        assert!(err.is_err(), "write must fail with injected error");

        let replayed = wal.replay().expect("replay after failure");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].key, b"k1".to_vec());
    }

    #[test]
    fn fail_next_sync_rolls_back_written_data() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            version: 1,
        })
        .expect("append first");

        wal.fail_next_sync();

        let err = wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::from_static(b"v2"),
            version: 2,
        });
        assert!(err.is_err(), "sync failure must propagate");

        let replayed = wal.replay().expect("replay after sync failure");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].key, b"k1".to_vec());
    }

    #[test]
    fn fail_batch_after_records_rejects_partial_batch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k0"),
            value: Bytes::from_static(b"v0"),
            version: 0,
        })
        .expect("append baseline");

        wal.fail_batch_after_records(1);

        let batch = vec![
            Record {
                kind: crate::db::wal::format::RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k1"),
                value: Bytes::from_static(b"v1"),
                version: 1,
            },
            Record {
                kind: crate::db::wal::format::RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k2"),
                value: Bytes::from_static(b"v2"),
                version: 2,
            },
            Record {
                kind: crate::db::wal::format::RecordKind::Put,
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k3"),
                value: Bytes::from_static(b"v3"),
                version: 3,
            },
        ];
        let err = wal.append_batch(&batch);
        assert!(err.is_err(), "partial batch must fail");

        wal.clear_failpoints();

        let replayed = wal.replay().expect("replay after partial batch");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].key, b"k0".to_vec());
    }

    #[test]
    fn replay_mid_file_corruption_skip_mode_recovers_trailing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"v1"),
            version: 1,
        })
        .expect("append");

        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .expect("open for corruption");
            file.write_all(b"CORRUPT_DATA_GARBAGE_1234567890")
                .expect("inject corruption");
            file.sync_data().expect("sync");
        }

        let r2 = encode(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::from_static(b"v2"),
            version: 2,
        });
        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .expect("open for r2");
            file.write_all(&r2).expect("write r2");
            file.sync_data().expect("sync r2");
        }

        let result = wal
            .replay_with_mode(ReplayMode::SkipCorruption)
            .expect("replay");
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].key, b"k1".to_vec());
        assert_eq!(result.records[1].key, b"k2".to_vec());
        assert!(!result.skipped.is_empty());
        assert!(result.skipped[0].length > 0);
    }

    #[test]
    fn skip_corruption_ignores_false_magic_in_payload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");

        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k1"),
            value: Bytes::from_static(b"WAL1"),
            version: 1,
        })
        .expect("append r1");

        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .expect("open for corruption");
            file.write_all(b"BADDATA_GARBAGE").expect("inject");
            file.sync_data().expect("sync");
        }

        let r2 = encode(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k2"),
            value: Bytes::from_static(b"v2"),
            version: 2,
        });
        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .expect("open for r2");
            file.write_all(&r2).expect("write r2");
            file.sync_data().expect("sync r2");
        }

        let result = wal
            .replay_with_mode(ReplayMode::SkipCorruption)
            .expect("replay");
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].key, b"k1".to_vec());
        assert_eq!(result.records[1].key, b"k2".to_vec());
        assert!(!result.skipped.is_empty());
    }

    #[test]
    fn concurrent_appends_can_share_one_group_commit_flush() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = Arc::new(
            WalSegment::open_with_config(
                &wal_path,
                WalGroupCommitConfig {
                    window: Duration::from_millis(5),
                    max_ops: 4_096,
                    max_bytes: 8 * 1024 * 1024,
                    preallocate_bytes: 0,
                    writev_enabled: true,
                },
            )
            .expect("open wal"),
        );

        let barrier = Arc::new(std::sync::Barrier::new(3));
        let mut joins = Vec::new();
        for idx in 0..2u8 {
            let worker_wal = wal.clone();
            let worker_barrier = barrier.clone();
            joins.push(std::thread::spawn(move || {
                worker_barrier.wait();
                worker_wal
                    .append(&Record {
                        kind: crate::db::wal::format::RecordKind::Put,
                        namespace: Bytes::from_static(b"core"),
                        key: vec![b'k', idx].into(),
                        value: vec![b'v', idx].into(),
                        version: idx as u64 + 1,
                    })
                    .expect("append")
            }));
        }
        barrier.wait();
        for join in joins {
            let _ = join.join().expect("join");
        }

        let stats = wal.flush_stats();
        assert!(
            stats.flushes <= 2,
            "expected grouped flushes, got {stats:?}"
        );
    }

    #[test]
    fn force_flush_on_close_is_counted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");
        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k"),
            value: Bytes::from_static(b"v"),
            version: 1,
        })
        .expect("append");
        wal.force_flush_on_close().expect("force flush");
        let stats = wal.flush_stats();
        assert!(stats.forced_flushes_on_close >= 1);
    }
}
