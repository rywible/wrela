use crate::db::wal::format::{Record, decode_at, encode};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub struct WalSegment {
    file: Mutex<File>,
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
        })
    }

    pub fn append(&self, record: &Record) -> io::Result<u64> {
        let bytes = encode(record);
        let mut file = self.file.lock().expect("WAL lock");
        let offset = file.seek(SeekFrom::End(0))?;
        file.write_all(&bytes)?;
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
            loop {
                match decode_at(&bytes, offset)? {
                    Some((record, next)) => {
                        out.push(record);
                        offset = next;
                    }
                    None => break,
                }
            }
            if offset > 0 && offset >= 64 * 1024 {
                bytes.drain(..offset);
                offset = 0;
            }
        }

        loop {
            match decode_at(&bytes, offset)? {
                Some((record, next)) => {
                    out.push(record);
                    offset = next;
                }
                None => break,
            }
        }
        Ok(out)
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
