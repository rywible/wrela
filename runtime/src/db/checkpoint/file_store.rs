use crate::db::checkpoint::store::CheckpointStore;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileCheckpointStore {
    root: PathBuf,
}

impl FileCheckpointStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    fn fsync_parent(path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            let dir = std::fs::File::open(parent)?;
            dir.sync_all()?;
        }
        Ok(())
    }
}

impl CheckpointStore for FileCheckpointStore {
    type Error = std::io::Error;

    fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Self::Error> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_data()?;
        std::fs::rename(&tmp, &path)?;
        Self::fsync_parent(&path)?;
        Ok(())
    }

    fn get_object(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        std::fs::read(self.path_for(key))
    }

    fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        let start = self.path_for(prefix);
        if !start.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let mut stack = vec![start.clone()];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file() {
                    if let Ok(rel) = p.strip_prefix(&self.root) {
                        out.push(rel.to_string_lossy().to_string());
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    fn delete_object(&self, key: &str) -> Result<(), Self::Error> {
        let path = self.path_for(key);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self.path_for(key).exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "wrela_checkpoint_file_store_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("epoch")
                .as_nanos(),
        ));
        std::fs::create_dir_all(&base).expect("create");
        base
    }

    #[test]
    fn file_store_roundtrip() {
        let root = temp_dir();
        let store = FileCheckpointStore::new(&root);
        store
            .put_object("checkpoints/a/manifest.json", b"{}")
            .expect("put");
        assert!(store.exists("checkpoints/a/manifest.json").expect("exists"));
        let payload = store
            .get_object("checkpoints/a/manifest.json")
            .expect("get");
        assert_eq!(payload, b"{}".to_vec());
        let listed = store.list_prefix("checkpoints").expect("list");
        assert_eq!(listed, vec!["checkpoints/a/manifest.json".to_string()]);
        store
            .delete_object("checkpoints/a/manifest.json")
            .expect("delete");
        assert!(!store.exists("checkpoints/a/manifest.json").expect("exists"));
    }
}
