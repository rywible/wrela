use crate::kernel::{KernelStructValue, KernelValue};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

macro_rules! identity_newtype {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(pub u64);
    };
}

identity_newtype!(AuthoredContentId);
identity_newtype!(EntityLineageId);
identity_newtype!(SnapshotEntityId);
identity_newtype!(WorldSnapshotId);
identity_newtype!(SnapshotEpoch);
identity_newtype!(ArtifactKeySeed);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SnapshotCaptureKind {
    Field,
    Shape,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SemanticEntityHandle {
    authored_content_id: AuthoredContentId,
    lineage_id: EntityLineageId,
    snapshot_entity_id: SnapshotEntityId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortableSceneCaptureProjection {
    pub scene_id: u32,
    pub epoch: u32,
    pub root_feature_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldSnapshotHandle {
    capture_name: SmolStr,
    kind: SnapshotCaptureKind,
    snapshot_id: WorldSnapshotId,
    epoch: SnapshotEpoch,
    artifact_key_seed: ArtifactKeySeed,
    root_entity: SemanticEntityHandle,
    portable: PortableSceneCaptureProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotIdentityReport {
    pub capture_name: String,
    pub kind: SnapshotCaptureKind,
    pub snapshot_id: WorldSnapshotId,
    pub epoch: SnapshotEpoch,
    pub authored_content_id: AuthoredContentId,
    pub lineage_id: EntityLineageId,
    pub snapshot_entity_id: SnapshotEntityId,
    pub portable_scene_id: u32,
    pub portable_root_feature_id: u32,
}

impl SnapshotEpoch {
    pub const INITIAL: Self = Self(1);

    pub fn portable_projection(self) -> u32 {
        u32::try_from(self.0).unwrap_or(u32::MAX)
    }
}

impl SemanticEntityHandle {
    pub(crate) fn new(
        authored_content_id: AuthoredContentId,
        lineage_id: EntityLineageId,
        snapshot_entity_id: SnapshotEntityId,
    ) -> Self {
        Self {
            authored_content_id,
            lineage_id,
            snapshot_entity_id,
        }
    }

    pub fn authored_content_id(&self) -> AuthoredContentId {
        self.authored_content_id
    }

    pub fn lineage_id(&self) -> EntityLineageId {
        self.lineage_id
    }

    pub fn snapshot_entity_id(&self) -> SnapshotEntityId {
        self.snapshot_entity_id
    }
}

impl WorldSnapshotHandle {
    pub(crate) fn new(
        capture_name: SmolStr,
        kind: SnapshotCaptureKind,
        snapshot_id: WorldSnapshotId,
        epoch: SnapshotEpoch,
        artifact_key_seed: ArtifactKeySeed,
        root_entity: SemanticEntityHandle,
        portable: PortableSceneCaptureProjection,
    ) -> Self {
        Self {
            capture_name,
            kind,
            snapshot_id,
            epoch,
            artifact_key_seed,
            root_entity,
            portable,
        }
    }

    pub fn capture_name(&self) -> &SmolStr {
        &self.capture_name
    }

    pub fn kind(&self) -> SnapshotCaptureKind {
        self.kind
    }

    pub fn snapshot_id(&self) -> WorldSnapshotId {
        self.snapshot_id
    }

    pub fn epoch(&self) -> SnapshotEpoch {
        self.epoch
    }

    pub fn artifact_key_seed(&self) -> ArtifactKeySeed {
        self.artifact_key_seed
    }

    pub fn root_entity(&self) -> &SemanticEntityHandle {
        &self.root_entity
    }

    pub fn portable_projection(&self) -> PortableSceneCaptureProjection {
        self.portable
    }

    pub fn portable_scene_id(&self) -> u32 {
        self.portable.scene_id
    }

    pub fn portable_epoch(&self) -> u32 {
        self.portable.epoch
    }

    pub fn portable_root_feature_id(&self) -> u32 {
        self.portable.root_feature_id
    }

    pub fn capture_value(&self) -> KernelValue {
        KernelValue::Struct(KernelStructValue {
            name: SmolStr::new(match self.kind {
                SnapshotCaptureKind::Field => "FieldCapture",
                SnapshotCaptureKind::Shape => "ShapeCapture",
                SnapshotCaptureKind::Region => "RegionCapture",
            }),
            fields: vec![
                (
                    SmolStr::new("scene_id"),
                    KernelValue::U32(self.portable.scene_id),
                ),
                (SmolStr::new("epoch"), KernelValue::U32(self.portable.epoch)),
                (
                    SmolStr::new("root_feature_id"),
                    KernelValue::U32(self.portable.root_feature_id),
                ),
            ],
        })
    }

    pub fn with_epoch(&self, epoch: SnapshotEpoch) -> Self {
        let artifact_key_seed =
            crate::query_exec::ids::stable_artifact_key_seed(self.snapshot_id, epoch);
        let snapshot_entity_id = crate::query_exec::ids::stable_snapshot_entity_id(
            artifact_key_seed,
            self.root_entity.lineage_id,
        );
        Self::new(
            self.capture_name.clone(),
            self.kind,
            self.snapshot_id,
            epoch,
            artifact_key_seed,
            SemanticEntityHandle::new(
                self.root_entity.authored_content_id,
                self.root_entity.lineage_id,
                snapshot_entity_id,
            ),
            PortableSceneCaptureProjection {
                scene_id: self.portable.scene_id,
                epoch: epoch.portable_projection(),
                root_feature_id: self.portable.root_feature_id,
            },
        )
    }

    pub fn report(&self) -> SnapshotIdentityReport {
        SnapshotIdentityReport {
            capture_name: self.capture_name.to_string(),
            kind: self.kind,
            snapshot_id: self.snapshot_id,
            epoch: self.epoch,
            authored_content_id: self.root_entity.authored_content_id,
            lineage_id: self.root_entity.lineage_id,
            snapshot_entity_id: self.root_entity.snapshot_entity_id,
            portable_scene_id: self.portable.scene_id,
            portable_root_feature_id: self.portable.root_feature_id,
        }
    }
}
