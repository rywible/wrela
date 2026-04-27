//! Save/load via compatibility hashes (RFC 0011 Phase 69).
//!
//! Split into focused modules per RFC 0011 L7:
//!
//! - [`header`]: header type, schema version, [`compare_header`].
//! - [`snapshot`]: on-disk record + zstd encode/decode pipeline.
//! - [`load_plan`]: turn a record + project into a runnable [`LoadPlan`].

#![forbid(unsafe_code)]

pub mod header;
pub mod load_plan;
pub mod snapshot;

use std::io;
use thiserror::Error;

pub use header::{
    CURRENT_CBOR_SCHEMA_VERSION, HeaderCompatibility, PersistenceProject, PersistentHandle,
    SaveIncompatibility, SnapshotSaveHeader, compare_header,
};
pub use load_plan::{LoadPlan, load_snapshot_record};
pub use snapshot::{
    DEFAULT_ZSTD_LEVEL, SnapshotLedgerRecord, SnapshotSavePayload, SnapshotSaveRecord,
    decompress_payload, read_record, save_snapshot_record, write_record,
};

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("cbor encode: {0}")]
    Encode(String),
    #[error("cbor decode: {0}")]
    Decode(String),
    #[error("save incompatible: {0}")]
    Incompatible(#[from] SaveIncompatibility),
}
