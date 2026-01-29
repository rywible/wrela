use std::error::Error;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::Arc;

use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use openraft::storage::LogState;
use openraft::storage::Snapshot;
use openraft::AnyError;
use openraft::BasicNode;
use openraft::Entry;
use openraft::EntryPayload;
use openraft::ErrorVerb;
use openraft::LogId;
use openraft::OptionalSend;
use openraft::RaftLogReader;
use openraft::RaftSnapshotBuilder;
use openraft::RaftStorage;
use openraft::RaftTypeConfig;
use openraft::SnapshotMeta;
use openraft::StorageError;
use openraft::StorageIOError;
use openraft::StoredMembership;
use openraft::TokioRuntime;
use openraft::Vote;
use rocksdb::ColumnFamily;
use rocksdb::ColumnFamilyDescriptor;
use rocksdb::Direction;
use rocksdb::Options;
use rocksdb::WriteBatch;
use rocksdb::DB;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::RwLock;

pub type NodeId = u64;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = KvRequest,
        R = KvResponse,
        NodeId = NodeId,
        Node = BasicNode,
        Entry = Entry<TypeConfig>,
        SnapshotData = Cursor<Vec<u8>>,
        AsyncRuntime = TokioRuntime
);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KvCommand {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KvRequest {
    Batch { ops: Vec<KvCommand> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum KvResponse {
    Applied,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KvSnapshot {
    pub meta: SnapshotMeta<NodeId, BasicNode>,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SerializableKvStateMachine {
    pub last_applied_log: Option<LogId<NodeId>>,
    pub last_membership: StoredMembership<NodeId, BasicNode>,
    pub data: Vec<(Vec<u8>, Vec<u8>)>,
}

impl From<&KvStateMachine> for SerializableKvStateMachine {
    fn from(state: &KvStateMachine) -> Self {
        let mut data = Vec::new();

        let it = state.db.iterator_cf(state.cf_sm_data(), rocksdb::IteratorMode::Start);
        for item in it {
            let (key, value) = item.expect("invalid kv record");
            data.push((key.to_vec(), value.to_vec()));
        }

        Self {
            last_applied_log: state.get_last_applied_log().expect("last_applied_log"),
            last_membership: state.get_last_membership().expect("last_membership"),
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KvStateMachine {
    pub db: Arc<DB>,
}

fn sm_r_err<E: Error + 'static>(e: E) -> StorageError<NodeId> {
    StorageIOError::read_state_machine(&e).into()
}

fn sm_w_err<E: Error + 'static>(e: E) -> StorageError<NodeId> {
    StorageIOError::write_state_machine(&e).into()
}

impl KvStateMachine {
    fn cf_sm_meta(&self) -> &ColumnFamily {
        self.db.cf_handle("sm_meta").unwrap()
    }

    fn cf_sm_data(&self) -> &ColumnFamily {
        self.db.cf_handle("sm_data").unwrap()
    }

    pub fn get_value(&self, key: &[u8]) -> StorageResult<Option<Vec<u8>>> {
        self.db
            .get_cf(self.cf_sm_data(), key)
            .map_err(sm_r_err)
    }

    fn get_last_membership(&self) -> StorageResult<StoredMembership<NodeId, BasicNode>> {
        self.db
            .get_cf(self.cf_sm_meta(), "last_membership".as_bytes())
            .map_err(sm_r_err)
            .and_then(|value| {
                value
                    .map(|v| serde_json::from_slice(&v).map_err(sm_r_err))
                    .unwrap_or_else(|| Ok(StoredMembership::default()))
            })
    }

    fn get_last_applied_log(&self) -> StorageResult<Option<LogId<NodeId>>> {
        self.db
            .get_cf(self.cf_sm_meta(), "last_applied_log".as_bytes())
            .map_err(sm_r_err)
            .and_then(|value| value.map(|v| serde_json::from_slice(&v).map_err(sm_r_err)).transpose())
    }

    fn apply_entry(
        &self,
        log_id: LogId<NodeId>,
        membership: Option<StoredMembership<NodeId, BasicNode>>,
        ops: Option<&[KvCommand]>,
    ) -> StorageResult<()> {
        let mut batch = WriteBatch::default();
        batch.put_cf(
            self.cf_sm_meta(),
            "last_applied_log".as_bytes(),
            serde_json::to_vec(&log_id).map_err(sm_w_err)?,
        );

        if let Some(mem) = membership {
            batch.put_cf(
                self.cf_sm_meta(),
                "last_membership".as_bytes(),
                serde_json::to_vec(&mem).map_err(sm_w_err)?,
            );
        }

        if let Some(ops) = ops {
            for op in ops {
                match op {
                    KvCommand::Put { key, value } => {
                        batch.put_cf(self.cf_sm_data(), key, value);
                    }
                    KvCommand::Delete { key } => {
                        batch.delete_cf(self.cf_sm_data(), key);
                    }
                }
            }
        }

        self.db.write(batch).map_err(sm_w_err)
    }

    fn from_serializable(sm: SerializableKvStateMachine, db: Arc<DB>) -> StorageResult<Self> {
        let r = Self { db };

        for (key, value) in sm.data {
            r.db.put_cf(r.cf_sm_data(), key, value).map_err(sm_w_err)?;
        }

        if let Some(log_id) = sm.last_applied_log {
            r.db
                .put_cf(
                    r.cf_sm_meta(),
                    "last_applied_log".as_bytes(),
                    serde_json::to_vec(&log_id).map_err(sm_w_err)?,
                )
                .map_err(sm_w_err)?;
        }

        r.db
            .put_cf(
                r.cf_sm_meta(),
                "last_membership".as_bytes(),
                serde_json::to_vec(&sm.last_membership).map_err(sm_w_err)?,
            )
            .map_err(sm_w_err)?;

        Ok(r)
    }
}

#[derive(Debug)]
pub struct KvStore {
    db: Arc<DB>,
    pub state_machine: RwLock<KvStateMachine>,
}

type StorageResult<T> = Result<T, StorageError<NodeId>>;

fn id_to_bin(id: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    buf.write_u64::<BigEndian>(id).unwrap();
    buf
}

fn bin_to_id(buf: &[u8]) -> u64 {
    (&buf[0..8]).read_u64::<BigEndian>().unwrap()
}

mod meta {
    use openraft::ErrorSubject;
    use openraft::LogId;

    use super::KvSnapshot;
    use super::NodeId;

    pub(crate) trait StoreMeta {
        const KEY: &'static str;
        type Value: serde::Serialize + serde::de::DeserializeOwned;
        fn subject(v: Option<&Self::Value>) -> ErrorSubject<NodeId>;
    }

    pub(crate) struct LastPurged {}
    pub(crate) struct SnapshotIndex {}
    pub(crate) struct Vote {}
    pub(crate) struct Snapshot {}

    impl StoreMeta for LastPurged {
        const KEY: &'static str = "last_purged_log_id";
        type Value = LogId<u64>;

        fn subject(_v: Option<&Self::Value>) -> ErrorSubject<NodeId> {
            ErrorSubject::Store
        }
    }

    impl StoreMeta for SnapshotIndex {
        const KEY: &'static str = "snapshot_index";
        type Value = u64;

        fn subject(_v: Option<&Self::Value>) -> ErrorSubject<NodeId> {
            ErrorSubject::Store
        }
    }

    impl StoreMeta for Vote {
        const KEY: &'static str = "vote";
        type Value = openraft::Vote<NodeId>;

        fn subject(_v: Option<&Self::Value>) -> ErrorSubject<NodeId> {
            ErrorSubject::Vote
        }
    }

    impl StoreMeta for Snapshot {
        const KEY: &'static str = "snapshot";
        type Value = KvSnapshot;

        fn subject(v: Option<&Self::Value>) -> ErrorSubject<NodeId> {
            ErrorSubject::Snapshot(Some(v.unwrap().meta.signature()))
        }
    }
}

impl KvStore {
    fn cf_meta(&self) -> &ColumnFamily {
        self.db.cf_handle("meta").unwrap()
    }

    fn cf_logs(&self) -> &ColumnFamily {
        self.db.cf_handle("logs").unwrap()
    }

    fn get_meta<M: meta::StoreMeta>(&self) -> Result<Option<M::Value>, StorageError<NodeId>> {
        let v = self
            .db
            .get_cf(self.cf_meta(), M::KEY)
            .map_err(|e| StorageIOError::new(M::subject(None), ErrorVerb::Read, AnyError::new(&e)))?;

        let t = match v {
            None => None,
            Some(bytes) => Some(
                serde_json::from_slice(&bytes)
                    .map_err(|e| StorageIOError::new(M::subject(None), ErrorVerb::Read, AnyError::new(&e)))?,
            ),
        };
        Ok(t)
    }

    fn put_meta<M: meta::StoreMeta>(&self, value: &M::Value) -> Result<(), StorageError<NodeId>> {
        let json_value = serde_json::to_vec(value)
            .map_err(|e| StorageIOError::new(M::subject(Some(value)), ErrorVerb::Write, AnyError::new(&e)))?;

        self.db
            .put_cf(self.cf_meta(), M::KEY, json_value)
            .map_err(|e| StorageIOError::new(M::subject(Some(value)), ErrorVerb::Write, AnyError::new(&e)))?;

        Ok(())
    }
}

impl RaftLogReader<TypeConfig> for Arc<KvStore> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<NodeId>> {
        let mut res = Vec::new();
        let start = match range.start_bound() {
            std::ops::Bound::Included(x) => *x,
            std::ops::Bound::Excluded(x) => *x + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let it = self
            .db
            .iterator_cf(self.cf_logs(), rocksdb::IteratorMode::From(&id_to_bin(start), Direction::Forward));
        for item_res in it {
            let (id, val) = item_res.map_err(read_logs_err)?;
            let id = bin_to_id(&id);
            if !range.contains(&id) {
                break;
            }
            let entry: Entry<_> = serde_json::from_slice(&val).map_err(read_logs_err)?;
            assert_eq!(id, entry.log_id.index);
            res.push(entry);
        }
        Ok(res)
    }
}

impl RaftSnapshotBuilder<TypeConfig> for Arc<KvStore> {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        let data;
        let last_applied_log;
        let last_membership;

        {
            let state_machine = SerializableKvStateMachine::from(&*self.state_machine.read().await);
            data = serde_json::to_vec(&state_machine).map_err(|e| StorageIOError::read_state_machine(&e))?;
            last_applied_log = state_machine.last_applied_log;
            last_membership = state_machine.last_membership;
        }

        let snapshot_idx: u64 = self.get_meta::<meta::SnapshotIndex>()?.unwrap_or_default() + 1;
        self.put_meta::<meta::SnapshotIndex>(&snapshot_idx)?;

        let snapshot_id = if let Some(last) = last_applied_log {
            format!("{}-{}-{}", last.leader_id, last.index, snapshot_idx)
        } else {
            format!("--{}", snapshot_idx)
        };

        let meta = SnapshotMeta {
            last_log_id: last_applied_log,
            last_membership,
            snapshot_id,
        };

        let snapshot = KvSnapshot {
            meta: meta.clone(),
            data: data.clone(),
        };

        self.put_meta::<meta::Snapshot>(&snapshot)?;

        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(data)),
        })
    }
}

impl RaftStorage<TypeConfig> for Arc<KvStore> {
    type LogReader = Self;
    type SnapshotBuilder = Self;

    async fn get_log_state(&mut self) -> StorageResult<LogState<TypeConfig>> {
        let last = self.db.iterator_cf(self.cf_logs(), rocksdb::IteratorMode::End).next();

        let last_log_id = match last {
            None => None,
            Some(res) => {
                let (_log_index, entry_bytes) = res.map_err(read_logs_err)?;
                let ent = serde_json::from_slice::<Entry<TypeConfig>>(&entry_bytes).map_err(read_logs_err)?;
                Some(ent.log_id)
            }
        };

        let last_purged_log_id = self.get_meta::<meta::LastPurged>()?;

        let last_log_id = match last_log_id {
            None => last_purged_log_id,
            Some(x) => Some(x),
        };

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        self.put_meta::<meta::Vote>(vote)
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        self.get_meta::<meta::Vote>()
    }

    async fn append_to_log<I>(&mut self, entries: I) -> StorageResult<()>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
    {
        for entry in entries {
            let id = id_to_bin(entry.log_id.index);
            assert_eq!(bin_to_id(&id), entry.log_id.index);
            self.db
                .put_cf(
                    self.cf_logs(),
                    id,
                    serde_json::to_vec(&entry).map_err(|e| StorageIOError::write_logs(&e))?,
                )
                .map_err(|e| StorageIOError::write_logs(&e))?;
        }
        Ok(())
    }

    async fn delete_conflict_logs_since(&mut self, log_id: LogId<NodeId>) -> StorageResult<()> {
        let from = id_to_bin(log_id.index);
        let to = id_to_bin(0xff_ff_ff_ff_ff_ff_ff_ff);
        self.db
            .delete_range_cf(self.cf_logs(), &from, &to)
            .map_err(|e| StorageIOError::write_logs(&e).into())
    }

    async fn purge_logs_upto(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        self.put_meta::<meta::LastPurged>(&log_id)?;

        let from = id_to_bin(0);
        let to = id_to_bin(log_id.index + 1);
        self.db
            .delete_range_cf(self.cf_logs(), &from, &to)
            .map_err(|e| StorageIOError::write_logs(&e).into())
    }

    async fn last_applied_state(
        &mut self,
    ) -> Result<(Option<LogId<NodeId>>, StoredMembership<NodeId, BasicNode>), StorageError<NodeId>> {
        let state_machine = self.state_machine.read().await;
        Ok((
            state_machine.get_last_applied_log()?,
            state_machine.get_last_membership()?,
        ))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<TypeConfig>],
    ) -> Result<Vec<KvResponse>, StorageError<NodeId>> {
        let mut res = Vec::with_capacity(entries.len());
        let sm = self.state_machine.write().await;

        for entry in entries {
            match &entry.payload {
                EntryPayload::Blank => {
                    sm.apply_entry(entry.log_id, None, None)?;
                    res.push(KvResponse::Applied);
                }
                EntryPayload::Normal(req) => match req {
                    KvRequest::Batch { ops } => {
                        sm.apply_entry(entry.log_id, None, Some(ops))?;
                        res.push(KvResponse::Applied);
                    }
                },
                EntryPayload::Membership(mem) => {
                    let stored = StoredMembership::new(Some(entry.log_id), mem.clone());
                    sm.apply_entry(entry.log_id, Some(stored), None)?;
                    res.push(KvResponse::Applied);
                }
            }
        }

        self.db.flush_wal(true).map_err(|e| StorageIOError::write_logs(&e))?;
        Ok(res)
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<<TypeConfig as RaftTypeConfig>::SnapshotData>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, BasicNode>,
        snapshot: Box<<TypeConfig as RaftTypeConfig>::SnapshotData>,
    ) -> Result<(), StorageError<NodeId>> {
        let new_snapshot = KvSnapshot {
            meta: meta.clone(),
            data: snapshot.into_inner(),
        };

        {
            let updated_state_machine: SerializableKvStateMachine =
                serde_json::from_slice(&new_snapshot.data)
                    .map_err(|e| StorageIOError::read_snapshot(Some(new_snapshot.meta.signature()), &e))?;
            let mut state_machine = self.state_machine.write().await;
            *state_machine = KvStateMachine::from_serializable(updated_state_machine, self.db.clone())?;
        }

        self.put_meta::<meta::Snapshot>(&new_snapshot)?;
        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        let curr_snap = self.get_meta::<meta::Snapshot>()?;
        match curr_snap {
            Some(snapshot) => {
                let data = snapshot.data.clone();
                Ok(Some(Snapshot {
                    meta: snapshot.meta,
                    snapshot: Box::new(Cursor::new(data)),
                }))
            }
            None => Ok(None),
        }
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }
}

impl KvStore {
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Arc<KvStore> {
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);

        #[cfg(not(any(test, feature = "test-utils")))]
        let meta = ColumnFamilyDescriptor::new("meta", Options::default());
        #[cfg(not(any(test, feature = "test-utils")))]
        let sm_meta = ColumnFamilyDescriptor::new("sm_meta", Options::default());
        #[cfg(not(any(test, feature = "test-utils")))]
        let sm_data = ColumnFamilyDescriptor::new("sm_data", Options::default());
        #[cfg(not(any(test, feature = "test-utils")))]
        let logs = ColumnFamilyDescriptor::new("logs", Options::default());

        #[cfg(any(test, feature = "test-utils"))]
        let db = {
            use tokio::time::{sleep, Duration};
            let mut tries = 0u32;
            loop {
                let meta = ColumnFamilyDescriptor::new("meta", Options::default());
                let sm_meta = ColumnFamilyDescriptor::new("sm_meta", Options::default());
                let sm_data = ColumnFamilyDescriptor::new("sm_data", Options::default());
                let logs = ColumnFamilyDescriptor::new("logs", Options::default());

                match DB::open_cf_descriptors(
                    &db_opts,
                    db_path.as_ref(),
                    vec![meta, sm_meta, sm_data, logs],
                ) {
                    Ok(db) => break db,
                    Err(err) => {
                        let msg = err.to_string();
                        if msg.contains("LOCK") && tries < 50 {
                            tries += 1;
                            sleep(Duration::from_millis(25)).await;
                            continue;
                        }
                        panic!("open kv store: {err}");
                    }
                }
            }
        };

        #[cfg(not(any(test, feature = "test-utils")))]
        let db = DB::open_cf_descriptors(&db_opts, db_path, vec![meta, sm_meta, sm_data, logs])
            .expect("open kv store");

        let db = Arc::new(db);
        let state_machine = RwLock::new(KvStateMachine { db: db.clone() });
        Arc::new(KvStore { db, state_machine })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn log_len(&self) -> usize {
        self.db
            .iterator_cf(self.cf_logs(), rocksdb::IteratorMode::Start)
            .count()
    }
}

fn read_logs_err(e: impl Error + 'static) -> StorageError<NodeId> {
    StorageError::IO {
        source: StorageIOError::read_logs(&e),
    }
}
