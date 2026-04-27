//! Load planning: take a verified save record + project, produce a snapshot
//! handle and the plan needed to restore world state. (RFC 0011 H6/L7).
//!
//! The load plan is intentionally separated from the on-disk format so that
//! callers can decide *how* to apply the ledger (synchronously, lazily, on a
//! background thread, etc.) without re-decoding the file.

use super::PersistenceError;
use super::header::{HeaderCompatibility, PersistenceProject, compare_header};
use super::snapshot::{SnapshotLedgerRecord, SnapshotSaveRecord, decompress_payload};
use crate::engine_frame::{
    EngineResourceAccess, EngineResourceAccessMode, EngineResourceEpochState, EngineResourceId,
    EngineResourceLedger, EngineResourceResidency, EngineResourceState, EngineSubsystemKind,
};
use crate::query_exec::ids::stable_region_snapshot_handle_at_epoch;
use crate::world_identity::{SnapshotEpoch, WorldSnapshotHandle};
use smol_str::SmolStr;

/// What the engine should apply after a successful load. Keep this owned and
/// inert: it must outlive the original record so callers can drop the file
/// data once the plan is constructed.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadPlan {
    pub snapshot_epoch: SnapshotEpoch,
    pub sim_tick: u64,
    pub presentation_frame: u64,
    pub ledger: Vec<SnapshotLedgerRecord>,
    /// Header-comparison warnings that did not block the load (e.g. schema
    /// migration up). Surfaced so the host can show them to the user.
    pub warnings: Vec<String>,
}

impl LoadPlan {
    pub fn resource_ledger_for_load(&self) -> EngineResourceLedger {
        EngineResourceLedger {
            accesses: vec![
                EngineResourceAccess {
                    subsystem: EngineSubsystemKind::Save,
                    resource: EngineResourceId::SaveRecord {
                        epoch: self.snapshot_epoch.0,
                    },
                    mode: EngineResourceAccessMode::Read,
                },
                EngineResourceAccess {
                    subsystem: EngineSubsystemKind::Save,
                    resource: EngineResourceId::WorldSnapshot {
                        epoch: self.snapshot_epoch.0,
                    },
                    mode: EngineResourceAccessMode::Write,
                },
            ],
            states: vec![EngineResourceState {
                resource: EngineResourceId::WorldSnapshot {
                    epoch: self.snapshot_epoch.0,
                },
                residency: EngineResourceResidency::CpuAuthoritative,
                epoch_state: EngineResourceEpochState::Valid {
                    epoch: self.snapshot_epoch.0,
                },
                producer: EngineSubsystemKind::Save,
            }],
            violations: Vec::new(),
        }
    }
}

/// Decompress, verify and bind a save record to a runnable [`LoadPlan`].
///
/// RFC 0011 H6: this is the world-state-restore entry point. The returned
/// handle is bound to the saved snapshot epoch (or the sim_tick if the saved
/// epoch was somehow lost), so the caller can hand it straight to the
/// runtime as `previous_snapshot` for a frame.
pub fn load_snapshot_record(
    record: SnapshotSaveRecord,
    project: &PersistenceProject,
) -> Result<(WorldSnapshotHandle, LoadPlan), PersistenceError> {
    let warnings = match compare_header(&record.header, project) {
        HeaderCompatibility::Incompatible { reason } => {
            return Err(PersistenceError::Incompatible(reason));
        }
        HeaderCompatibility::CompatibleMigrateUp { warnings } => warnings,
        HeaderCompatibility::Exact => Vec::new(),
    };
    let payload = decompress_payload(&record)?;
    let epoch = SnapshotEpoch(
        record
            .header
            .snapshot_epoch
            .max(record.header.sim_tick)
            .max(1),
    );
    let snapshot =
        stable_region_snapshot_handle_at_epoch(&SmolStr::new(&record.header.project_id), epoch);
    Ok((
        snapshot,
        LoadPlan {
            snapshot_epoch: epoch,
            sim_tick: record.header.sim_tick,
            presentation_frame: record.header.presentation_frame,
            ledger: payload.ledger,
            warnings,
        },
    ))
}
