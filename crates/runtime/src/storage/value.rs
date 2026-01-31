use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlobRef {
    pub key: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum StoredValue {
    Inline(Vec<u8>),
    Blob(BlobRef),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredRecord {
    pub version: u64,
    pub value: StoredValue,
}
