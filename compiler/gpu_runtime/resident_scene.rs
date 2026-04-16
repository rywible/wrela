use crate::gpu_runtime::device::GpuLimitRequest;
use crate::gpu_runtime::layout::GpuLayoutIdentity;
use crate::world_identity::SnapshotIdentityReport;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuResidentSceneKey {
    pub snapshot: SnapshotIdentityReport,
    pub detail: i32,
    pub layout: GpuLayoutIdentity,
    pub selection_signature: u64,
}

impl Hash for GpuResidentSceneKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.snapshot.capture_name.hash(state);
        self.snapshot.kind.hash(state);
        self.snapshot.snapshot_id.hash(state);
        self.snapshot.epoch.hash(state);
        self.snapshot.authored_content_id.hash(state);
        self.snapshot.lineage_id.hash(state);
        self.snapshot.snapshot_entity_id.hash(state);
        self.snapshot.portable_scene_id.hash(state);
        self.snapshot.portable_root_feature_id.hash(state);
        self.detail.hash(state);
        self.layout.hash(state);
        self.selection_signature.hash(state);
    }
}

impl GpuResidentSceneKey {
    pub fn new(snapshot: SnapshotIdentityReport, detail: i32, layout: GpuLayoutIdentity) -> Self {
        Self {
            snapshot,
            detail,
            layout,
            selection_signature: 0,
        }
    }

    pub fn feature_mask(&self) -> u64 {
        self.layout.feature_mask
    }

    pub const fn with_selection_signature(mut self, selection_signature: u64) -> Self {
        self.selection_signature = selection_signature;
        self
    }
}

#[derive(Debug)]
pub struct GpuResidentScenePayload {
    pub world_shapes: wgpu::Buffer,
    pub accel_nodes: wgpu::Buffer,
    pub accel_children: wgpu::Buffer,
    pub shape_meta: wgpu::Buffer,
    pub cache_bricks: wgpu::Buffer,
    pub bind_group_scene: wgpu::BindGroup,
}

#[derive(Debug)]
pub struct GpuResidentScene<P = GpuResidentScenePayload> {
    pub key: GpuResidentSceneKey,
    pub payload: P,
}

impl<P> GpuResidentScene<P> {
    pub fn new(key: GpuResidentSceneKey, payload: P) -> Self {
        Self { key, payload }
    }
}

#[derive(Debug)]
pub struct GpuResidentSceneCache<P = GpuResidentScenePayload> {
    entries: HashMap<GpuResidentSceneKey, Arc<GpuResidentScene<P>>>,
}

impl<P> GpuResidentSceneCache<P> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get(&self, key: &GpuResidentSceneKey) -> Option<Arc<GpuResidentScene<P>>> {
        self.entries.get(key).cloned()
    }

    pub fn get_or_insert_with<E, F>(
        &mut self,
        key: GpuResidentSceneKey,
        build: F,
    ) -> Result<Arc<GpuResidentScene<P>>, E>
    where
        F: FnOnce(&GpuResidentSceneKey) -> Result<GpuResidentScene<P>, E>,
    {
        if let Some(existing) = self.entries.get(&key) {
            return Ok(existing.clone());
        }
        let built = Arc::new(build(&key)?);
        self.entries.insert(key, built.clone());
        Ok(built)
    }

    pub fn get_or_insert_snapshot<E, F>(
        &mut self,
        snapshot: SnapshotIdentityReport,
        detail: i32,
        layout: GpuLayoutIdentity,
        selection_signature: u64,
        build: F,
    ) -> Result<Arc<GpuResidentScene<P>>, E>
    where
        F: FnOnce(&GpuResidentSceneKey) -> Result<GpuResidentScene<P>, E>,
    {
        let key = GpuResidentSceneKey::new(snapshot, detail, layout)
            .with_selection_signature(selection_signature);
        self.get_or_insert_with(key, build)
    }
}

fn resident_scene_registry() -> &'static Mutex<HashMap<(GpuLimitRequest, &'static str), usize>> {
    static REQUEST_CACHES: OnceLock<Mutex<HashMap<(GpuLimitRequest, &'static str), usize>>> =
        OnceLock::new();
    REQUEST_CACHES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn shared_resident_scene_cache_for_request<P: 'static>(
    request: GpuLimitRequest,
) -> &'static Mutex<GpuResidentSceneCache<P>> {
    let registry = resident_scene_registry();
    let type_name = std::any::type_name::<P>();
    let mut guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
    let ptr = guard.entry((request, type_name)).or_insert_with(|| {
        let boxed = Box::new(Mutex::new(GpuResidentSceneCache::<P>::new()));
        Box::into_raw(boxed) as usize
    });
    unsafe { &*(*ptr as *const Mutex<GpuResidentSceneCache<P>>) }
}

pub fn clear_shared_resident_scene_caches_for_type<P: 'static>() {
    let registry = resident_scene_registry();
    let type_name = std::any::type_name::<P>();
    let guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
    for ((_, cached_type_name), ptr) in guard.iter() {
        if *cached_type_name != type_name {
            continue;
        }
        let cache = unsafe { &*(*ptr as *const Mutex<GpuResidentSceneCache<P>>) };
        let mut cache_guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        cache_guard.clear();
    }
}

impl<P> Default for GpuResidentSceneCache<P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_identity::{
        AuthoredContentId, EntityLineageId, SnapshotCaptureKind, SnapshotEntityId, SnapshotEpoch,
        WorldSnapshotId,
    };

    fn snapshot_report() -> SnapshotIdentityReport {
        SnapshotIdentityReport {
            capture_name: "scene".to_string(),
            kind: SnapshotCaptureKind::Shape,
            snapshot_id: WorldSnapshotId(7),
            epoch: SnapshotEpoch(3),
            authored_content_id: AuthoredContentId(11),
            lineage_id: EntityLineageId(13),
            snapshot_entity_id: SnapshotEntityId(17),
            portable_scene_id: 19,
            portable_root_feature_id: 23,
        }
    }

    #[test]
    fn cache_reuses_existing_scene() {
        let mut cache: GpuResidentSceneCache<String> = GpuResidentSceneCache::new();
        let key = GpuResidentSceneKey::new(snapshot_report(), 2, GpuLayoutIdentity::new(29, 31));

        let first = cache
            .get_or_insert_with(key.clone(), |key| {
                Ok::<_, ()>(GpuResidentScene::new(key.clone(), "first".to_string()))
            })
            .expect("first insert");
        let second = cache
            .get_or_insert_with(key, |key| {
                Ok::<_, ()>(GpuResidentScene::new(key.clone(), "second".to_string()))
            })
            .expect("cache hit");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.payload, "first");
    }

    #[test]
    fn key_exposes_feature_mask() {
        let key = GpuResidentSceneKey::new(snapshot_report(), 2, GpuLayoutIdentity::new(29, 41));
        assert_eq!(key.feature_mask(), 41);
    }
}
