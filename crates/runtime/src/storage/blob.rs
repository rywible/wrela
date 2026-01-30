use std::path::{Path, PathBuf};

use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_types::region::Region;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::config::{BlobConfig, S3Config};
use super::value::BlobRef;

#[derive(Debug)]
pub struct BlobError(pub String);

impl std::fmt::Display for BlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BlobError {}

#[derive(Clone, Debug)]
pub enum BlobBackend {
    File(FileBlobStore),
    S3(S3BlobStore),
}

impl BlobBackend {
    pub async fn from_config(config: &BlobConfig) -> Result<Self, BlobError> {
        if let Some(s3) = config.s3.clone() {
            let store = S3BlobStore::new(s3).await?;
            Ok(Self::S3(store))
        } else {
            Ok(Self::File(FileBlobStore::new(&config.file_path)))
        }
    }

    pub async fn put(&self, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        match self {
            Self::File(store) => store.put(bytes).await,
            Self::S3(store) => store.put(bytes).await,
        }
    }

    pub async fn get(&self, blob: &BlobRef) -> Result<Vec<u8>, BlobError> {
        match self {
            Self::File(store) => store.get(blob).await,
            Self::S3(store) => store.get(blob).await,
        }
    }

    pub async fn delete(&self, blob: &BlobRef) -> Result<(), BlobError> {
        match self {
            Self::File(store) => store.delete(blob).await,
            Self::S3(store) => store.delete(blob).await,
        }
    }

    pub async fn put_named(&self, key: &str, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        match self {
            Self::File(store) => store.put_named(key, bytes).await,
            Self::S3(store) => store.put_named(key, bytes).await,
        }
    }

    pub async fn get_named(&self, key: &str) -> Result<Vec<u8>, BlobError> {
        match self {
            Self::File(store) => store.get_named(key).await,
            Self::S3(store) => store.get_named(key).await,
        }
    }

    pub async fn delete_key(&self, key: &str) -> Result<(), BlobError> {
        match self {
            Self::File(store) => store.delete_key(key).await,
            Self::S3(store) => store.delete_key(key).await,
        }
    }

    pub async fn list_prefix(&self, prefix: &str) -> Result<Vec<BlobRef>, BlobError> {
        match self {
            Self::File(store) => store.list_prefix(prefix).await,
            Self::S3(store) => store.list_prefix(prefix).await,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileBlobStore {
    base: PathBuf,
}

impl FileBlobStore {
    pub fn new(path: &str) -> Self {
        Self {
            base: PathBuf::from(path),
        }
    }

    fn path_for_key(&self, key: &str) -> PathBuf {
        let (dir, file) = key.split_at(2);
        self.base.join(dir).join(file)
    }

    fn path_for_named(&self, key: &str) -> PathBuf {
        self.base.join(key)
    }

    async fn ensure_parent(path: &Path) -> Result<(), BlobError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| BlobError(format!("blob mkdir: {err}")))?;
        }
        Ok(())
    }

    pub async fn put(&self, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        let key = random_key();
        let path = self.path_for_key(&key);
        Self::ensure_parent(&path).await?;

        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)
            .await
            .map_err(|err| BlobError(format!("blob create: {err}")))?;
        file.write_all(bytes)
            .await
            .map_err(|err| BlobError(format!("blob write: {err}")))?;
        file.flush()
            .await
            .map_err(|err| BlobError(format!("blob flush: {err}")))?;
        drop(file);
        fs::rename(&tmp_path, &path)
            .await
            .map_err(|err| BlobError(format!("blob rename: {err}")))?;

        Ok(BlobRef {
            key,
            size: bytes.len() as u64,
        })
    }

    pub async fn put_named(&self, key: &str, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        let path = self.path_for_named(key);
        Self::ensure_parent(&path).await?;

        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)
            .await
            .map_err(|err| BlobError(format!("blob create: {err}")))?;
        file.write_all(bytes)
            .await
            .map_err(|err| BlobError(format!("blob write: {err}")))?;
        file.flush()
            .await
            .map_err(|err| BlobError(format!("blob flush: {err}")))?;
        drop(file);
        fs::rename(&tmp_path, &path)
            .await
            .map_err(|err| BlobError(format!("blob rename: {err}")))?;

        Ok(BlobRef {
            key: key.to_string(),
            size: bytes.len() as u64,
        })
    }

    pub async fn get(&self, blob: &BlobRef) -> Result<Vec<u8>, BlobError> {
        let path = self.path_for_key(&blob.key);
        fs::read(&path)
            .await
            .map_err(|err| BlobError(format!("blob read: {err}")))
    }

    pub async fn get_named(&self, key: &str) -> Result<Vec<u8>, BlobError> {
        let path = self.path_for_named(key);
        fs::read(&path)
            .await
            .map_err(|err| BlobError(format!("blob read: {err}")))
    }

    pub async fn delete(&self, blob: &BlobRef) -> Result<(), BlobError> {
        let path = self.path_for_key(&blob.key);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(BlobError(format!("blob delete: {err}"))),
        }
    }

    pub async fn delete_key(&self, key: &str) -> Result<(), BlobError> {
        let path = self.path_for_named(key);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(BlobError(format!("blob delete: {err}"))),
        }
    }

    pub async fn list_prefix(&self, prefix: &str) -> Result<Vec<BlobRef>, BlobError> {
        let base = self.base.clone();
        let root = self.base.join(prefix);
        let mut out = Vec::new();
        list_dir(&base, &root, &mut out).await?;
        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct S3BlobStore {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
}

impl S3BlobStore {
    pub async fn new(cfg: S3Config) -> Result<Self, BlobError> {
        let creds = Credentials::new(
            cfg.access_key,
            cfg.secret_key,
            None,
            None,
            "wrela-storage",
        );
        let region = Region::new(cfg.region);
        let region_provider = RegionProviderChain::first_try(region.clone()).or_default_provider();
        let mut loader = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .credentials_provider(creds);
        if let Some(endpoint) = cfg.endpoint.as_ref() {
            loader = loader.endpoint_url(endpoint);
        }
        let shared_config = loader.load().await;
        let client = aws_sdk_s3::Client::new(&shared_config);
        let prefix = normalize_prefix(cfg.prefix);
        Ok(Self {
            client,
            bucket: cfg.bucket,
            prefix,
        })
    }

    fn object_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix, key)
        }
    }

    pub async fn put(&self, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        let key = random_key();
        let object_key = self.object_key(&key);
        let body = ByteStream::from(bytes.to_vec());
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(body)
            .send()
            .await
            .map_err(|err| BlobError(format!("s3 put: {err}")))?;
        Ok(BlobRef {
            key: object_key,
            size: bytes.len() as u64,
        })
    }

    pub async fn put_named(&self, key: &str, bytes: &[u8]) -> Result<BlobRef, BlobError> {
        let object_key = self.object_key(key);
        let body = ByteStream::from(bytes.to_vec());
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(body)
            .send()
            .await
            .map_err(|err| BlobError(format!("s3 put: {err}")))?;
        Ok(BlobRef {
            key: key.to_string(),
            size: bytes.len() as u64,
        })
    }

    pub async fn get(&self, blob: &BlobRef) -> Result<Vec<u8>, BlobError> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&blob.key)
            .send()
            .await
            .map_err(|err| BlobError(format!("s3 get: {err}")))?;
        let data = resp
            .body
            .collect()
            .await
            .map_err(|err| BlobError(format!("s3 read: {err}")))?;
        Ok(data.into_bytes().to_vec())
    }

    pub async fn get_named(&self, key: &str) -> Result<Vec<u8>, BlobError> {
        let object_key = self.object_key(key);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
            .map_err(|err| BlobError(format!("s3 get: {err}")))?;
        let data = resp
            .body
            .collect()
            .await
            .map_err(|err| BlobError(format!("s3 read: {err}")))?;
        Ok(data.into_bytes().to_vec())
    }

    pub async fn delete(&self, blob: &BlobRef) -> Result<(), BlobError> {
        let resp = self
            .client
            .delete_object()
            .bucket(&self.bucket)
            .key(&blob.key)
            .send()
            .await;
        match resp {
            Ok(_) => Ok(()),
            Err(err) => Err(BlobError(format!("s3 delete: {err}"))),
        }
    }

    pub async fn delete_key(&self, key: &str) -> Result<(), BlobError> {
        let object_key = self.object_key(key);
        let resp = self
            .client
            .delete_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await;
        match resp {
            Ok(_) => Ok(()),
            Err(err) => Err(BlobError(format!("s3 delete: {err}"))),
        }
    }

    pub async fn list_prefix(&self, prefix: &str) -> Result<Vec<BlobRef>, BlobError> {
        let mut out = Vec::new();
        let object_prefix = self.object_key(prefix);
        let mut token = None;
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&object_prefix);
            if let Some(tok) = token.as_ref() {
                req = req.continuation_token(tok);
            }
            let resp = req
                .send()
                .await
                .map_err(|err| BlobError(format!("s3 list: {err}")))?;
            for obj in resp.contents() {
                if let Some(key) = obj.key() {
                    let raw = self.strip_prefix(key);
                    out.push(BlobRef {
                        key: raw,
                        size: obj.size().unwrap_or(0) as u64,
                    });
                }
            }
            if resp.is_truncated().unwrap_or(false) {
                token = resp.next_continuation_token().map(|s| s.to_string());
                if token.is_none() {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn strip_prefix(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            return key.to_string();
        }
        let trimmed = self.prefix.trim_matches('/');
        if let Some(rest) = key.strip_prefix(trimmed) {
            let rest = rest.trim_start_matches('/');
            return rest.to_string();
        }
        key.to_string()
    }
}

fn random_key() -> String {
    Uuid::new_v4().simple().to_string()
}

fn normalize_prefix(prefix: Option<String>) -> String {
    let Some(prefix) = prefix else {
        return String::new();
    };
    let trimmed = prefix.trim_matches('/');
    trimmed.to_string()
}

async fn list_dir(base: &Path, dir: &Path, out: &mut Vec<BlobRef>) -> Result<(), BlobError> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut entries = match fs::read_dir(&current).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(BlobError(format!("blob list: {err}"))),
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| BlobError(format!("blob list: {err}")))?
        {
            let path = entry.path();
        let meta = match entry.metadata().await {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(BlobError(format!("blob meta: {err}"))),
        };
            if meta.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                let key = rel.to_string_lossy().replace('\\', "/");
                out.push(BlobRef {
                    key,
                    size: meta.len() as u64,
                });
            }
        }
    }
    Ok(())
}
