use crate::db::checkpoint::store::CheckpointStore;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Builder as S3ConfigBuilder, Region};
use aws_sdk_s3::primitives::ByteStream;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct S3CheckpointStore {
    client: Arc<Client>,
    bucket: String,
    prefix: String,
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub prefix: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub path_style: bool,
}

#[derive(Debug)]
pub enum S3CheckpointError {
    Client(String),
    MissingObject(String),
    Runtime(String),
}

impl fmt::Display for S3CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(msg) => write!(f, "s3 client error: {msg}"),
            Self::MissingObject(key) => write!(f, "s3 object not found: {key}"),
            Self::Runtime(msg) => write!(f, "s3 runtime error: {msg}"),
        }
    }
}

impl S3CheckpointStore {
    pub fn from_config(config: S3Config) -> Result<Self, S3CheckpointError> {
        let rt = crate::kernel::runtime::tokio_runtime();
        let shared = rt.block_on(async {
            let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(Region::new(config.region.clone()));
            if let Some(endpoint) = &config.endpoint {
                loader = loader.endpoint_url(endpoint);
            }
            let loaded = loader.load().await;
            let mut builder = S3ConfigBuilder::from(&loaded).force_path_style(config.path_style);
            if let Some(endpoint) = &config.endpoint {
                builder = builder.endpoint_url(endpoint);
            }
            builder.build()
        });

        Ok(Self {
            client: Arc::new(Client::from_conf(shared)),
            bucket: config.bucket,
            prefix: config.prefix.trim_matches('/').to_string(),
        })
    }

    fn object_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.prefix, key)
        }
    }

    fn with_runtime<T>(fut: impl std::future::Future<Output = T>) -> Result<T, S3CheckpointError> {
        let rt = crate::kernel::runtime::tokio_runtime();
        Ok(rt.block_on(fut))
    }
}

impl CheckpointStore for S3CheckpointStore {
    type Error = S3CheckpointError;

    fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Self::Error> {
        let object_key = self.object_key(key);
        Self::with_runtime(async {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(object_key)
                .body(ByteStream::from(data.to_vec()))
                .send()
                .await
                .map_err(|err| S3CheckpointError::Client(err.to_string()))
                .map(|_| ())
        })?
    }

    fn get_object(&self, key: &str) -> Result<Vec<u8>, Self::Error> {
        let object_key = self.object_key(key);
        Self::with_runtime(async {
            let output = self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(&object_key)
                .send()
                .await
                .map_err(|err| {
                    let msg = err.to_string();
                    if msg.contains("NoSuchKey") || msg.contains("Not Found") {
                        S3CheckpointError::MissingObject(object_key.clone())
                    } else {
                        S3CheckpointError::Client(msg)
                    }
                })?;
            let bytes = output
                .body
                .collect()
                .await
                .map_err(|err| S3CheckpointError::Client(err.to_string()))?;
            Ok(bytes.into_bytes().to_vec())
        })?
    }

    fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, Self::Error> {
        let full_prefix = self.object_key(prefix);
        Self::with_runtime(async {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(full_prefix.clone())
                .send()
                .await
                .map_err(|err| S3CheckpointError::Client(err.to_string()))?;
            let mut out = Vec::new();
            for obj in output.contents() {
                if let Some(key) = obj.key() {
                    if self.prefix.is_empty() {
                        out.push(key.to_string());
                    } else if let Some(stripped) = key.strip_prefix(&(self.prefix.clone() + "/")) {
                        out.push(stripped.to_string());
                    }
                }
            }
            out.sort();
            Ok(out)
        })?
    }

    fn delete_object(&self, key: &str) -> Result<(), Self::Error> {
        let object_key = self.object_key(key);
        Self::with_runtime(async {
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(object_key)
                .send()
                .await
                .map_err(|err| S3CheckpointError::Client(err.to_string()))
                .map(|_| ())
        })?
    }

    fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        let object_key = self.object_key(key);
        Self::with_runtime(async {
            let res = self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(object_key)
                .send()
                .await;
            match res {
                Ok(_) => Ok(true),
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("Not Found") || msg.contains("NoSuchKey") {
                        Ok(false)
                    } else {
                        Err(S3CheckpointError::Client(msg))
                    }
                }
            }
        })?
    }
}
