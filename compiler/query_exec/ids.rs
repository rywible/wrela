use crate::world_identity::{
    ArtifactKeySeed, AuthoredContentId, EntityLineageId, PortableSceneCaptureProjection,
    SemanticEntityHandle, SnapshotCaptureKind, SnapshotEntityId, SnapshotEpoch,
    WorldSnapshotHandle, WorldSnapshotId,
};
use smol_str::SmolStr;

pub fn stable_portable_id(parts: &[&[u8]]) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 0x811c9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in *part {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

pub fn stable_semantic_id(parts: &[&[u8]]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET_BASIS;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

pub fn stable_shape_capture_id(shape_name: &SmolStr) -> u32 {
    stable_portable_id(&[shape_name.as_bytes()])
}

pub fn stable_shape_scene_capture_id(shape_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::shape::", shape_name.as_bytes()])
}

pub fn stable_field_scene_capture_id(field_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::field::", field_name.as_bytes()])
}

pub fn stable_region_scene_capture_id(region_name: &SmolStr) -> u32 {
    stable_portable_id(&[b"scene::region::", region_name.as_bytes()])
}

pub fn stable_authored_content_id(kind: SnapshotCaptureKind, name: &SmolStr) -> AuthoredContentId {
    AuthoredContentId(stable_semantic_id(&[
        b"authored::",
        capture_kind_tag(kind),
        name.as_bytes(),
    ]))
}

pub fn stable_entity_lineage_id(kind: SnapshotCaptureKind, name: &SmolStr) -> EntityLineageId {
    EntityLineageId(stable_semantic_id(&[
        b"lineage::",
        capture_kind_tag(kind),
        name.as_bytes(),
    ]))
}

pub fn stable_world_snapshot_id(kind: SnapshotCaptureKind, name: &SmolStr) -> WorldSnapshotId {
    WorldSnapshotId(stable_semantic_id(&[
        b"snapshot::",
        capture_kind_tag(kind),
        name.as_bytes(),
    ]))
}

pub fn stable_snapshot_entity_id(
    artifact_key_seed: ArtifactKeySeed,
    lineage_id: EntityLineageId,
) -> SnapshotEntityId {
    SnapshotEntityId(stable_semantic_id(&[
        b"snapshot-entity::",
        &artifact_key_seed.0.to_le_bytes(),
        &lineage_id.0.to_le_bytes(),
    ]))
}

pub fn stable_artifact_key_seed(
    snapshot_id: WorldSnapshotId,
    epoch: SnapshotEpoch,
) -> ArtifactKeySeed {
    ArtifactKeySeed(stable_semantic_id(&[
        b"artifact-key::",
        &snapshot_id.0.to_le_bytes(),
        &epoch.0.to_le_bytes(),
    ]))
}

pub fn stable_snapshot_handle(kind: SnapshotCaptureKind, name: &SmolStr) -> WorldSnapshotHandle {
    stable_snapshot_handle_at_epoch(kind, name, SnapshotEpoch::INITIAL)
}

pub fn stable_snapshot_handle_at_epoch(
    kind: SnapshotCaptureKind,
    name: &SmolStr,
    epoch: SnapshotEpoch,
) -> WorldSnapshotHandle {
    let authored_content_id = stable_authored_content_id(kind, name);
    let lineage_id = stable_entity_lineage_id(kind, name);
    let snapshot_id = stable_world_snapshot_id(kind, name);
    let artifact_key_seed = stable_artifact_key_seed(snapshot_id, epoch);
    let snapshot_entity_id = stable_snapshot_entity_id(artifact_key_seed, lineage_id);
    let portable = PortableSceneCaptureProjection {
        scene_id: match kind {
            SnapshotCaptureKind::Field => stable_field_scene_capture_id(name),
            SnapshotCaptureKind::Shape => stable_shape_scene_capture_id(name),
            SnapshotCaptureKind::Region => stable_region_scene_capture_id(name),
        },
        epoch: epoch.portable_projection(),
        root_feature_id: match kind {
            SnapshotCaptureKind::Shape => stable_shape_capture_id(name),
            SnapshotCaptureKind::Field | SnapshotCaptureKind::Region => 0,
        },
    };
    WorldSnapshotHandle::new(
        name.clone(),
        kind,
        snapshot_id,
        epoch,
        artifact_key_seed,
        SemanticEntityHandle::new(authored_content_id, lineage_id, snapshot_entity_id),
        portable,
    )
}

pub fn stable_field_snapshot_handle(name: &SmolStr) -> WorldSnapshotHandle {
    stable_snapshot_handle(SnapshotCaptureKind::Field, name)
}

pub fn stable_shape_snapshot_handle(name: &SmolStr) -> WorldSnapshotHandle {
    stable_snapshot_handle(SnapshotCaptureKind::Shape, name)
}

pub fn stable_region_snapshot_handle(name: &SmolStr) -> WorldSnapshotHandle {
    stable_snapshot_handle(SnapshotCaptureKind::Region, name)
}

pub fn stable_field_snapshot_handle_at_epoch(
    name: &SmolStr,
    epoch: SnapshotEpoch,
) -> WorldSnapshotHandle {
    stable_snapshot_handle_at_epoch(SnapshotCaptureKind::Field, name, epoch)
}

pub fn stable_shape_snapshot_handle_at_epoch(
    name: &SmolStr,
    epoch: SnapshotEpoch,
) -> WorldSnapshotHandle {
    stable_snapshot_handle_at_epoch(SnapshotCaptureKind::Shape, name, epoch)
}

pub fn stable_region_snapshot_handle_at_epoch(
    name: &SmolStr,
    epoch: SnapshotEpoch,
) -> WorldSnapshotHandle {
    stable_snapshot_handle_at_epoch(SnapshotCaptureKind::Region, name, epoch)
}

fn capture_kind_tag(kind: SnapshotCaptureKind) -> &'static [u8] {
    match kind {
        SnapshotCaptureKind::Field => b"field",
        SnapshotCaptureKind::Shape => b"shape",
        SnapshotCaptureKind::Region => b"region",
    }
}
