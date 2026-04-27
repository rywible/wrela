//! Persistence header types and compatibility comparison (RFC 0011 Phase 69, H6/L4).
//!
//! The header is the small compatibility-checked prelude on top of every save
//! body. Compatibility is checked before the body is decompressed/deserialized
//! so we never spend cycles on a save we know we can't load.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Current on-disk schema version for the compatibility-checked prelude.
///
/// Bump this whenever you add or change the meaning of a field in
/// [`SnapshotSaveHeader`]. This is independent of `wrela_version`: it captures
/// the structural shape of the *file format*, not the toolchain release.
pub const CURRENT_CBOR_SCHEMA_VERSION: u32 = 1;

/// Stable identifier handed back to authors when persisting their content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersistentHandle {
    stable_semantic_id: u64,
}

impl PersistentHandle {
    pub fn from_stable_semantic_parts(parts: &[&[u8]]) -> Self {
        Self {
            stable_semantic_id: crate::query_exec::ids::stable_semantic_id(parts),
        }
    }

    pub fn stable_semantic_id(self) -> u64 {
        self.stable_semantic_id
    }
}

/// Header fields embedded in every save record.
///
/// Bumping `cbor_schema_version` is mandatory whenever this struct gains or
/// drops a field; see [`compare_header`] for the comparison contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSaveHeader {
    pub wrela_version: String,
    /// Snapshot epoch captured from `WorldSnapshotHandle` at save time.
    #[serde(default)]
    pub snapshot_epoch: u64,
    pub project_id: String,
    pub engine_compatibility_hash: u64,
    pub generator_compatibility_hashes: BTreeMap<String, u64>,
    pub archetype_schema_hashes: BTreeMap<String, u64>,
    pub sim_tick: u64,
    pub presentation_frame: u64,
    /// Wall-clock at save time, **nanoseconds since the Unix epoch**.
    /// Zero is reserved for "unknown / not measured" so that ports running
    /// without a real-time clock (e.g. deterministic test harnesses) can opt
    /// out without misrepresenting save time.
    pub saved_at_unix_nanos: u64,
    /// Structural version of the on-disk format. Compared in
    /// [`compare_header`]; saves at a higher schema than the running engine
    /// are rejected as toolchain-downgrades.
    pub cbor_schema_version: u32,
}

/// Result of comparing a saved header against the running project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderCompatibility {
    Exact,
    CompatibleMigrateUp { warnings: Vec<String> },
    Incompatible { reason: SaveIncompatibility },
}

/// Reasons a save record is rejected by [`compare_header`].
///
/// RFC 0011 L4: each variant uses field names that match what the variant
/// actually carries.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SaveIncompatibility {
    #[error("wrela toolchain downgrade: saved {saved}, running {running}")]
    ToolchainDowngrade { saved: String, running: String },
    #[error("save schema is newer than running engine: saved={saved}, running={running}")]
    SaveSchemaNewerThanEngine { saved: u32, running: u32 },
    #[error("engine compatibility hash mismatch: saved={saved_hash}, running={running_hash}")]
    EngineCompatibilityHashMismatch { saved_hash: u64, running_hash: u64 },
    #[error("generator diverged: {name} saved={saved_hash} running={running_hash}")]
    GeneratorDiverged {
        name: String,
        saved_hash: u64,
        running_hash: u64,
    },
    #[error(
        "generator removed from project: {name} (saved hash {saved_hash}, no longer registered)"
    )]
    GeneratorRemoved { name: String, saved_hash: u64 },
    #[error("archetype schema changed: {name} saved={saved_hash} running={running_hash}")]
    ArchetypeSchemaChanged {
        name: String,
        saved_hash: u64,
        running_hash: u64,
    },
    #[error("project id mismatch: saved {saved}, running {running}")]
    ProjectIdMismatch { saved: String, running: String },
}

/// Project-side compatibility metadata used when comparing headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceProject {
    pub project_id: String,
    pub wrela_version: String,
    pub engine_compatibility_hash: u64,
    pub generator_compatibility_hashes: BTreeMap<String, u64>,
    pub archetype_schema_hashes: BTreeMap<String, u64>,
}

/// Compare a saved header against the running project metadata.
///
/// The check is performed in priority order:
///
/// 1. project id match
/// 2. on-disk schema version <= running schema version (H6)
/// 3. engine compatibility hash match (L4 names the fields honestly)
/// 4. archetype + generator hashes match for every shared key
/// 5. removed generators are flagged (H6)
/// 6. wrela toolchain version is not a downgrade
pub fn compare_header(
    header: &SnapshotSaveHeader,
    project: &PersistenceProject,
) -> HeaderCompatibility {
    if header.project_id != project.project_id {
        return HeaderCompatibility::Incompatible {
            reason: SaveIncompatibility::ProjectIdMismatch {
                saved: header.project_id.clone(),
                running: project.project_id.clone(),
            },
        };
    }
    // H6: refuse saves whose schema is newer than what the running engine
    // understands. Older schemas migrate up via `CompatibleMigrateUp`.
    if header.cbor_schema_version > CURRENT_CBOR_SCHEMA_VERSION {
        return HeaderCompatibility::Incompatible {
            reason: SaveIncompatibility::SaveSchemaNewerThanEngine {
                saved: header.cbor_schema_version,
                running: CURRENT_CBOR_SCHEMA_VERSION,
            },
        };
    }
    if header.engine_compatibility_hash != project.engine_compatibility_hash {
        return HeaderCompatibility::Incompatible {
            reason: SaveIncompatibility::EngineCompatibilityHashMismatch {
                saved_hash: header.engine_compatibility_hash,
                running_hash: project.engine_compatibility_hash,
            },
        };
    }
    // H6: a saved generator missing from the running project is a hard
    // incompatibility — the load plan can't reproduce it. Flag this before
    // checking value equality so authors get a precise diagnostic.
    for (name, saved_hash) in &header.generator_compatibility_hashes {
        match project.generator_compatibility_hashes.get(name) {
            None => {
                return HeaderCompatibility::Incompatible {
                    reason: SaveIncompatibility::GeneratorRemoved {
                        name: name.clone(),
                        saved_hash: *saved_hash,
                    },
                };
            }
            Some(running_hash) if running_hash != saved_hash => {
                return HeaderCompatibility::Incompatible {
                    reason: SaveIncompatibility::GeneratorDiverged {
                        name: name.clone(),
                        saved_hash: *saved_hash,
                        running_hash: *running_hash,
                    },
                };
            }
            Some(_) => {}
        }
    }
    for (name, saved_hash) in &header.archetype_schema_hashes {
        if let Some(running_hash) = project.archetype_schema_hashes.get(name)
            && running_hash != saved_hash
        {
            return HeaderCompatibility::Incompatible {
                reason: SaveIncompatibility::ArchetypeSchemaChanged {
                    name: name.clone(),
                    saved_hash: *saved_hash,
                    running_hash: *running_hash,
                },
            };
        }
    }
    let mut warnings = Vec::new();
    if header.cbor_schema_version < CURRENT_CBOR_SCHEMA_VERSION {
        warnings.push(format!(
            "save schema migration {}->{}",
            header.cbor_schema_version, CURRENT_CBOR_SCHEMA_VERSION
        ));
    }
    match simple_version_tuple(&header.wrela_version)
        .zip(simple_version_tuple(&project.wrela_version))
    {
        Some((saved, running)) if saved > running => {
            return HeaderCompatibility::Incompatible {
                reason: SaveIncompatibility::ToolchainDowngrade {
                    saved: header.wrela_version.clone(),
                    running: project.wrela_version.clone(),
                },
            };
        }
        Some((saved, running)) if saved < running => {
            warnings.push(format!(
                "saved with {}, running {} (upgrade path)",
                header.wrela_version, project.wrela_version
            ));
            return HeaderCompatibility::CompatibleMigrateUp { warnings };
        }
        _ => {}
    }
    if header.wrela_version != project.wrela_version {
        warnings.push(format!(
            "saved with {}, running {}",
            header.wrela_version, project.wrela_version
        ));
        return HeaderCompatibility::CompatibleMigrateUp { warnings };
    }
    if warnings.is_empty() {
        HeaderCompatibility::Exact
    } else {
        HeaderCompatibility::CompatibleMigrateUp { warnings }
    }
}

fn simple_version_tuple(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}
