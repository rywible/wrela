use crate::db::wal::format::{Record, decode_at, encode};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub struct WalSegment {
    file: Mutex<File>,
    #[cfg(test)]
    failpoints: Mutex<WalTestFailpoints>,
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
        Ok(Self {
            file: Mutex::new(file),
            #[cfg(test)]
            failpoints: Mutex::new(WalTestFailpoints::default()),
        })
    }

    pub fn append(&self, record: &Record) -> io::Result<u64> {
        self.append_batch(std::slice::from_ref(record))
    }

    pub fn append_batch(&self, records: &[Record]) -> io::Result<u64> {
        let mut bytes = Vec::new();
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
            bytes.extend_from_slice(&encode(record));
        }
        #[cfg(not(test))]
        for record in records {
            bytes.extend_from_slice(&encode(record));
        }

        let mut file = self.file.lock().expect("WAL lock");
        let offset = file.seek(SeekFrom::End(0))?;
        #[cfg(test)]
        {
            let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
            if failpoints.fail_before_batch_write {
                failpoints.fail_before_batch_write = false;
                return Err(io::Error::other("injected wal write failure"));
            }
        }
        file.write_all(&bytes)?;
        #[cfg(test)]
        {
            let mut failpoints = self.failpoints.lock().expect("WAL failpoint lock");
            if failpoints.fail_on_sync {
                failpoints.fail_on_sync = false;
                file.set_len(offset)?;
                file.seek(SeekFrom::Start(offset))?;
                return Err(io::Error::other("injected wal sync failure"));
            }
        }
        file.sync_data()?;
        Ok(offset)
    }

    pub fn replay(&self) -> io::Result<Vec<Record>> {
        let mut file = self.file.lock().expect("WAL lock");
        file.seek(SeekFrom::Start(0))?;
        let mut out = Vec::new();
        let mut bytes = Vec::new();
        let mut offset = 0usize;

        loop {
            let mut chunk = [0u8; 8192];
            let read = file.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            while let Some((record, next)) = decode_at(&bytes, offset)? {
                out.push(record);
                offset = next;
            }
            if offset >= 64 * 1024 {
                bytes.drain(..offset);
                offset = 0;
            }
        }

        while let Some((record, next)) = decode_at(&bytes, offset)? {
            out.push(record);
            offset = next;
        }
        Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_truncates_torn_tail_without_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal_path = dir.path().join("wal.log");
        let wal = WalSegment::open(&wal_path).expect("open wal");
        wal.append(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: b"core".to_vec(),
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
            version: 1,
        })
        .expect("append record");

        let partial = encode(&Record {
            kind: crate::db::wal::format::RecordKind::Put,
            namespace: b"core".to_vec(),
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
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
                namespace: b"core".to_vec(),
                key: format!("k{i}").into_bytes(),
                value: b"v".to_vec(),
                version: i,
            })
            .expect("append");
        }
        let replayed = wal.replay().expect("replay");
        assert_eq!(replayed.len(), 10_000);
    }
}
