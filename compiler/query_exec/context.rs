use crate::acceleration::build::{SharedAccelerationCatalog, build_shared_acceleration_forests};
use crate::hir;
use crate::hir::typeck::TypeInfo;
use crate::query_plan::SceneSummary;
use crate::query_exec::ids::{
    stable_field_snapshot_handle, stable_region_snapshot_handle, stable_shape_snapshot_handle,
};
use crate::query_exec::region::{RegionExecCase, build_region_exec_cases};
use crate::scene_ir;
use crate::scene_ir::{DistanceSemantics, SupportClass};
use crate::scene_ir::{ShapeLeafId, ShapeLeafRef, ShapeLeafScene};
use crate::world_identity::{SnapshotCaptureKind, SnapshotIdentityReport, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_QUERY_EXEC_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct QueryExecContext {
    pub module: hir::Module,
    pub type_info: TypeInfo,
    pub scene: scene_ir::SceneIrModule,
    pub functions_by_name: HashMap<SmolStr, hir::Function>,
    pub field_graphs: BTreeMap<SmolStr, hir::FieldGraph>,
    pub field_bodies: BTreeMap<SmolStr, hir::Body>,
    pub field_metadata: BTreeMap<SmolStr, hir::FieldMetadata>,
    pub shape_graphs: BTreeMap<SmolStr, hir::ShapeGraph>,
    pub value_class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    pub fields_by_name: HashMap<SmolStr, hir::Function>,
    pub regions_by_name: HashMap<SmolStr, hir::Function>,
    pub region_cases: Vec<RegionExecCase>,
    pub shared_acceleration: SharedAccelerationCatalog,
    pub field_names: HashSet<SmolStr>,
    pub shape_names: HashSet<SmolStr>,
    pub field_snapshots: BTreeMap<SmolStr, WorldSnapshotHandle>,
    pub shape_snapshots: BTreeMap<SmolStr, WorldSnapshotHandle>,
    pub region_snapshots: BTreeMap<SmolStr, WorldSnapshotHandle>,
    pub(crate) wgsl_shader_cache_context_id: u64,
    field_scene_index: HashMap<u32, SmolStr>,
    shape_scene_index: HashMap<u32, SmolStr>,
    shape_root_feature_index: HashMap<u32, SmolStr>,
    region_scene_index: HashMap<u32, SmolStr>,
}

impl QueryExecContext {
    pub fn compile(module: &hir::Module, type_info: &TypeInfo) -> Self {
        let module = module.clone();
        let (rechecked_errors, module_type_info) =
            crate::hir::typeck::check_module_with_info(&module);
        debug_assert!(
            rechecked_errors.is_empty(),
            "query_exec context typecheck on cloned module failed: {rechecked_errors:?}"
        );
        let scene = scene_ir::lower_module(&module);
        let functions_by_name = module
            .functions
            .iter()
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let field_graphs = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_bodies = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.body
                    .as_ref()
                    .map(|body| (func.name.clone(), body.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_metadata = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field
                    .as_ref()
                    .map(|metadata| (func.name.clone(), metadata.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let shape_graphs = module
            .shapes
            .iter()
            .filter_map(|(_, shape)| {
                shape
                    .graph
                    .as_ref()
                    .map(|graph| (shape.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let value_class_fields = module
            .classes
            .iter()
            .filter(|(_, class)| class.role == hir::ClassRole::Value)
            .map(|(_, class)| {
                (
                    class.name.clone(),
                    class
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        let fields_by_name = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Field))
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let regions_by_name = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Region))
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let region_cases = build_region_exec_cases(&module);
        let field_names = fields_by_name.keys().cloned().collect::<HashSet<_>>();
        let shape_names = module
            .shapes
            .iter()
            .map(|(_, shape)| shape.name.clone())
            .collect::<HashSet<_>>();
        let field_snapshots = field_names
            .iter()
            .map(|name| (name.clone(), stable_field_snapshot_handle(name)))
            .collect::<BTreeMap<_, _>>();
        let shape_snapshots = shape_names
            .iter()
            .map(|name| (name.clone(), stable_shape_snapshot_handle(name)))
            .collect::<BTreeMap<_, _>>();
        let region_snapshots = regions_by_name
            .keys()
            .map(|name| (name.clone(), stable_region_snapshot_handle(name)))
            .collect::<BTreeMap<_, _>>();
        let field_scene_index = field_snapshots
            .iter()
            .map(|(name, snapshot)| (snapshot.portable_scene_id(), name.clone()))
            .collect::<HashMap<_, _>>();
        let shape_scene_index = shape_snapshots
            .iter()
            .map(|(name, snapshot)| (snapshot.portable_scene_id(), name.clone()))
            .collect::<HashMap<_, _>>();
        let shape_root_feature_index = shape_snapshots
            .iter()
            .map(|(name, snapshot)| (snapshot.portable_root_feature_id(), name.clone()))
            .collect::<HashMap<_, _>>();
        let region_scene_index = region_snapshots
            .iter()
            .map(|(name, snapshot)| (snapshot.portable_scene_id(), name.clone()))
            .collect::<HashMap<_, _>>();
        let wgsl_shader_cache_context_id =
            NEXT_QUERY_EXEC_CONTEXT_ID.fetch_add(1, Ordering::Relaxed);

        let mut context = Self {
            module,
            type_info: if rechecked_errors.is_empty() {
                module_type_info
            } else {
                type_info.clone()
            },
            scene,
            functions_by_name,
            field_graphs,
            field_bodies,
            field_metadata,
            shape_graphs,
            value_class_fields,
            fields_by_name,
            regions_by_name,
            region_cases,
            shared_acceleration: SharedAccelerationCatalog::default(),
            field_names,
            shape_names,
            field_snapshots,
            shape_snapshots,
            region_snapshots,
            wgsl_shader_cache_context_id,
            field_scene_index,
            shape_scene_index,
            shape_root_feature_index,
            region_scene_index,
        };
        context.shared_acceleration = build_shared_acceleration_forests(&context);
        context
    }

    pub fn field_snapshot_handle(&self, name: &SmolStr) -> Option<&WorldSnapshotHandle> {
        self.field_snapshots.get(name)
    }

    pub fn shape_snapshot_handle(&self, name: &SmolStr) -> Option<&WorldSnapshotHandle> {
        self.shape_snapshots.get(name)
    }

    pub fn region_snapshot_handle(&self, name: &SmolStr) -> Option<&WorldSnapshotHandle> {
        self.region_snapshots.get(name)
    }

    pub fn world_acceleration_forest(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Option<&crate::acceleration::AccelerationForest> {
        self.shared_acceleration.world(capture, detail)
    }

    pub fn union_acceleration_forest(
        &self,
        shape: &SmolStr,
    ) -> Option<&crate::acceleration::AccelerationForest> {
        self.shared_acceleration.union(shape)
    }

    pub fn shared_cache_catalog(&self) -> &crate::acceleration::cache::SharedBrickCacheCatalog {
        &self.shared_acceleration.cache_catalog
    }

    pub fn shape_cache_support(
        &self,
        shape: &SmolStr,
    ) -> Option<&crate::acceleration::cache::SupportBrickCache> {
        self.shared_acceleration.cache_catalog.shape_support(shape)
    }

    pub fn shape_cache_distance(
        &self,
        shape: &SmolStr,
    ) -> Option<&crate::acceleration::cache::DistanceBrickCache> {
        self.shared_acceleration.cache_catalog.shape_distance(shape)
    }

    pub fn world_cache_support(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Option<&crate::acceleration::cache::SupportBrickCache> {
        self.shared_acceleration
            .cache_catalog
            .world_support(capture, detail)
    }

    pub fn world_cache_distance(
        &self,
        capture: &SmolStr,
        detail: i32,
    ) -> Option<&crate::acceleration::cache::DistanceBrickCache> {
        self.shared_acceleration
            .cache_catalog
            .world_distance(capture, detail)
    }

    pub fn snapshot_handle_for_capture_name(&self, name: &SmolStr) -> Option<&WorldSnapshotHandle> {
        self.field_snapshot_handle(name)
            .or_else(|| self.shape_snapshot_handle(name))
            .or_else(|| self.region_snapshot_handle(name))
    }

    pub fn snapshot_handle_for_kind(
        &self,
        kind: SnapshotCaptureKind,
        name: &SmolStr,
    ) -> Option<&WorldSnapshotHandle> {
        match kind {
            SnapshotCaptureKind::Field => self.field_snapshot_handle(name),
            SnapshotCaptureKind::Shape => self.shape_snapshot_handle(name),
            SnapshotCaptureKind::Region => self.region_snapshot_handle(name),
        }
    }

    pub fn snapshot_report_for_capture_name(
        &self,
        name: &SmolStr,
    ) -> Option<SnapshotIdentityReport> {
        self.snapshot_handle_for_capture_name(name)
            .map(WorldSnapshotHandle::report)
    }

    pub fn field_name_for_scene_id(&self, scene_id: u32) -> Option<&SmolStr> {
        self.field_scene_index.get(&scene_id)
    }

    pub fn shape_name_for_scene_id(&self, scene_id: u32) -> Option<&SmolStr> {
        self.shape_scene_index.get(&scene_id)
    }

    pub fn shape_name_for_root_feature_id(&self, root_feature_id: u32) -> Option<&SmolStr> {
        self.shape_root_feature_index.get(&root_feature_id)
    }

    pub fn region_name_for_scene_id(&self, scene_id: u32) -> Option<&SmolStr> {
        self.region_scene_index.get(&scene_id)
    }

    pub fn field_scene_id(&self, name: &SmolStr) -> u32 {
        self.field_snapshot_handle(name)
            .map(WorldSnapshotHandle::portable_scene_id)
            .unwrap_or_default()
    }

    pub fn shape_scene_id(&self, name: &SmolStr) -> u32 {
        self.shape_snapshot_handle(name)
            .map(WorldSnapshotHandle::portable_scene_id)
            .unwrap_or_default()
    }

    pub fn shape_root_feature_id(&self, name: &SmolStr) -> u32 {
        self.shape_snapshot_handle(name)
            .map(WorldSnapshotHandle::portable_root_feature_id)
            .unwrap_or_default()
    }

    pub fn region_scene_id(&self, name: &SmolStr) -> u32 {
        self.region_snapshot_handle(name)
            .map(WorldSnapshotHandle::portable_scene_id)
            .unwrap_or_default()
    }

    pub fn region_scene_summary(&self, name: &SmolStr, detail: i32) -> Option<SceneSummary> {
        let scene_id = self.region_scene_id(name);
        let region_case = self.region_cases.iter().find(|case| case.scene_id == scene_id)?;
        let shapes = region_case.shapes_for_detail(detail).ok()?;
        let mut summaries = Vec::with_capacity(shapes.len());
        for shape_name in shapes {
            let shape_scene = self.scene.shapes.get(shape_name)?;
            let identity_source_count = shape_scene
                .feature_leaves
                .values()
                .filter_map(|leaf_ref| {
                    self.scene
                        .shapes
                        .get(&leaf_ref.scene)
                        .and_then(|scene| scene.leaves.get(&leaf_ref.leaf))
                        .and_then(|leaf| self.scene.fields.get(&leaf.field))
                })
                .map(|field| field.identity_sources.len() as u32)
                .sum::<u32>();
            summaries.push((shape_scene, identity_source_count));
        }
        if summaries.is_empty() {
            return Some(SceneSummary {
                name: Some(name.clone()),
                ..SceneSummary::default()
            });
        }

        let support_class = if summaries
            .iter()
            .any(|(scene, _)| matches!(scene.support_class, SupportClass::Unbounded))
        {
            SupportClass::Unbounded
        } else if summaries
            .iter()
            .any(|(scene, _)| matches!(scene.support_class, SupportClass::Periodic))
        {
            SupportClass::Periodic
        } else if summaries
            .iter()
            .any(|(scene, _)| matches!(scene.support_class, SupportClass::Unknown))
        {
            SupportClass::Unknown
        } else {
            SupportClass::Bounded
        };
        let semantics = if summaries
            .iter()
            .any(|(scene, _)| matches!(scene.semantics, DistanceSemantics::UnknownOpaque))
        {
            DistanceSemantics::UnknownOpaque
        } else if summaries.len() == 1 {
            summaries[0].0.semantics
        } else {
            DistanceSemantics::ConservativeLowerBound
        };
        let opaque_boundary = summaries.iter().any(|(scene, _)| scene.opaque_boundary);
        let can_coarse_support_pruning = !opaque_boundary
            && matches!(support_class, SupportClass::Bounded)
            && summaries
                .iter()
                .all(|(scene, _)| scene.can_coarse_support_pruning);

        Some(SceneSummary {
            name: Some(name.clone()),
            semantics,
            support_class,
            can_coarse_support_pruning,
            opaque_boundary,
            evidence_summary: SceneSummary::default().evidence_summary,
            semantic_root: summaries
                .first()
                .map(|(scene, _)| scene.root_node_id.0)
                .unwrap_or_default(),
            support_root: summaries
                .first()
                .map(|(scene, _)| scene.root_support_id.0)
                .unwrap_or_default(),
            node_count: summaries
                .iter()
                .map(|(scene, _)| scene.node_records.len() as u32)
                .sum(),
            support_node_count: summaries
                .iter()
                .map(|(scene, _)| scene.support_records.len() as u32)
                .sum(),
            leaf_count: summaries
                .iter()
                .map(|(scene, _)| scene.feature_leaves.len() as u32)
                .sum(),
            identity_source_count: summaries.iter().map(|(_, count)| *count).sum(),
        })
    }

    pub fn shape_leaf_ref(&self, shape: &SmolStr, feature_id: u32) -> Option<&ShapeLeafRef> {
        self.scene
            .shapes
            .get(shape)
            .and_then(|scene| scene.feature_leaves.get(&feature_id))
    }

    pub fn shape_leaf(&self, scene: &SmolStr, leaf: ShapeLeafId) -> Option<&ShapeLeafScene> {
        self.scene
            .shapes
            .get(scene)
            .and_then(|shape_scene| shape_scene.leaves.get(&leaf))
    }
}
