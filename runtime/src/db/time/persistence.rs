use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn state_file_path(wal_path: &Path) -> PathBuf {
    wal_path.with_extension("hlc")
}

pub fn load_hlc_state(wal_path: &Path) -> std::io::Result<Option<u64>> {
    let path = state_file_path(wal_path);
    if !path.exists() {
        return Ok(None);
    }

    let mut buf = [0u8; 8];
    let mut file = File::open(path)?;
    file.read_exact(&mut buf)?;
    Ok(Some(u64::from_le_bytes(buf)))
}

pub fn persist_hlc_state(wal_path: &Path, packed_ts: u64) -> std::io::Result<()> {
    let path = state_file_path(wal_path);
    let tmp_path = path.with_extension("hlc.tmp");

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(&packed_ts.to_le_bytes())?;
        file.sync_data()?;
    }

    fs::rename(&tmp_path, &path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_data();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_and_loads_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wal = dir.path().join("wal.log");
        persist_hlc_state(&wal, 42).expect("persist");
        let loaded = load_hlc_state(&wal).expect("load");
        assert_eq!(loaded, Some(42));
    }
}
