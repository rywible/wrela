//! Save record encode/decode pipeline for snapshots (RFC 0011 H6/L7).
//!
//! Wire layout for a save record:
//!
//! ```text
//! +---------------------+
//! | CBOR(SnapshotSaveRecord) |   <- header + body bytes
//! +---------------------+
//! ```
//!
//! The body bytes are the **zstd-compressed** CBOR encoding of
//! `SnapshotSavePayload`. Compression is opaque to compatibility checking:
//! the uncompressed payload only ever needs to be touched after the header
//! has been validated.

use super::PersistenceError;
use super::header::{PersistenceProject, SnapshotSaveHeader};
use ciborium::value::Value;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default zstd compression level for save bodies. Level 3 is fast enough to
/// run inline in the engine frame while still cutting our typical save sizes
/// roughly in half on the test fixtures.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// One opaque value owned by the snapshot ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotLedgerRecord {
    pub handle: super::header::PersistentHandle,
    pub type_id: String,
    pub payload: Value,
}

/// The full snapshot body, decompressed. Only stable across a single schema
/// version: bumping `cbor_schema_version` is mandatory whenever this struct
/// changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSavePayload {
    pub ledger: Vec<SnapshotLedgerRecord>,
}

/// On-disk save record: header + zstd-compressed CBOR payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSaveRecord {
    pub header: SnapshotSaveHeader,
    /// Zstd-compressed CBOR encoding of [`SnapshotSavePayload`].
    pub body: Vec<u8>,
}

/// Build a [`SnapshotSaveRecord`] from a snapshot handle, project metadata
/// and a ledger of components/resources/etc.
///
/// RFC 0011 H6:
/// - `saved_at_unix_nanos` is filled from `SystemTime::now()`. Hosts running
///   without a real clock can post-process the record and zero the field.
/// - Body is zstd-compressed before being written.
pub fn save_snapshot_record(
    snapshot: &crate::world_identity::WorldSnapshotHandle,
    project: &PersistenceProject,
    sim_tick: u64,
    presentation_frame: u64,
    ledger: Vec<SnapshotLedgerRecord>,
) -> Result<SnapshotSaveRecord, PersistenceError> {
    let mut payload_cbor = Vec::new();
    ciborium::into_writer(&SnapshotSavePayload { ledger }, &mut payload_cbor)
        .map_err(|err| PersistenceError::Encode(err.to_string()))?;
    let body = zstd::encode_all(payload_cbor.as_slice(), DEFAULT_ZSTD_LEVEL)
        .map_err(|err| PersistenceError::Encode(format!("zstd: {err}")))?;
    let saved_at_unix_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Ok(SnapshotSaveRecord {
        header: SnapshotSaveHeader {
            wrela_version: project.wrela_version.clone(),
            snapshot_epoch: snapshot.epoch().0,
            project_id: project.project_id.clone(),
            engine_compatibility_hash: project.engine_compatibility_hash,
            generator_compatibility_hashes: project.generator_compatibility_hashes.clone(),
            archetype_schema_hashes: project.archetype_schema_hashes.clone(),
            sim_tick,
            presentation_frame,
            saved_at_unix_nanos,
            cbor_schema_version: super::header::CURRENT_CBOR_SCHEMA_VERSION,
        },
        body,
    })
}

/// Decompress the body of a save record back into a payload.
pub fn decompress_payload(
    record: &SnapshotSaveRecord,
) -> Result<SnapshotSavePayload, PersistenceError> {
    let raw = zstd::decode_all(record.body.as_slice())
        .map_err(|err| PersistenceError::Decode(format!("zstd: {err}")))?;
    ciborium::from_reader(raw.as_slice()).map_err(|err| PersistenceError::Decode(err.to_string()))
}

pub fn write_record(path: &Path, record: &SnapshotSaveRecord) -> Result<(), PersistenceError> {
    let mut bytes = Vec::new();
    ciborium::into_writer(record, &mut bytes)
        .map_err(|err| PersistenceError::Encode(err.to_string()))?;
    std::fs::write(path, bytes).map_err(|err: io::Error| PersistenceError::Io(err))?;
    Ok(())
}

pub fn read_record(path: &Path) -> Result<SnapshotSaveRecord, PersistenceError> {
    let bytes = std::fs::read(path).map_err(|err: io::Error| PersistenceError::Io(err))?;
    ciborium::from_reader(bytes.as_slice()).map_err(|err| PersistenceError::Decode(err.to_string()))
}
