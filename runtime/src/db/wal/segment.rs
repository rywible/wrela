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
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let mut out = Vec::new();
        let mut offset = 0usize;
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
