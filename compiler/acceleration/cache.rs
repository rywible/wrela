use crate::acceleration::CacheArtifactScope;
use crate::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactLogicalField, ArtifactLogicalSchema,
    ArtifactValidityPredicate, ArtifactValidityRule, SemanticArtifactContract,
    SemanticArtifactKind,
};
use crate::artifact_key::ArtifactPolicyDigestMode;
use crate::query_exec::context::QueryExecContext;
use crate::query_exec::cpu::DirectQueryEvaluator;
use crate::query_exec::ids::{
    stable_region_snapshot_handle, stable_semantic_id, stable_shape_snapshot_handle,
};
use crate::scene_ir::SupportClass;
use crate::semantic_evidence::SemanticEvidenceSummary;
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;
use std::collections::BTreeMap;

pub const DISTANCE_BRICK_CACHE_VERSION: u32 = 1;
pub const SUPPORT_BRICK_CACHE_VERSION: u32 = 1;
const DEFAULT_BRICK_EDGE: u32 = 4;
const DEFAULT_TARGET_BRICKS_PER_AXIS: u32 = 8;
const DEFAULT_MIN_BRICK_SIZE: f32 = 0.25;
const DEFAULT_NARROW_BAND_WIDTH: f32 = 0.75;
const DEFAULT_MAX_CANDIDATE_BRICKS: u32 = 512;
const DEFAULT_MAX_OCCUPIED_BRICKS: u32 = 320;
const DEFAULT_MAX_DISTANCE_SAMPLES: u32 = 256;
const DEFAULT_MAX_UPLOAD_BYTES: u64 = 96 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheSnapshotKind {
    ShapeSnapshot,
    WorldSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheBudget {
    enabled: bool,
}

impl CacheBudget {
    pub const fn enabled() -> Self {
        Self { enabled: true }
    }

    pub const fn disabled() -> Self {
        Self { enabled: false }
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }
}

impl Default for CacheBudget {
    fn default() -> Self {
        Self::enabled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSourceDescriptor {
    pub snapshot_kind: CacheSnapshotKind,
    pub snapshot_root: SmolStr,
    pub semantic_root: SmolStr,
    pub support_class: SupportClass,
    pub opaque_boundary: bool,
    pub can_coarse_support_pruning: bool,
    pub budget: CacheBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSourceRejection {
    pub reason: SmolStr,
    pub detail: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSourceReport {
    pub source: CacheSourceDescriptor,
    pub rejections: Vec<CacheSourceRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheCatalogReport {
    pub source_reports: Vec<CacheSourceReport>,
    pub requested_distance_bricks: u32,
    pub emitted_distance_bricks: u32,
    pub emitted_support_bricks: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportCacheArtifactRecord {
    pub contract: SemanticArtifactContract,
    pub cache: SupportBrickCache,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistanceCacheArtifactRecord {
    pub contract: SemanticArtifactContract,
    pub cache: DistanceBrickCache,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CacheArtifactRecord {
    Support(SupportCacheArtifactRecord),
    Distance(DistanceCacheArtifactRecord),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotCacheCatalog {
    pub snapshot_kind: CacheSnapshotKind,
    pub snapshot_root: SmolStr,
    pub artifacts: Vec<CacheArtifactRecord>,
    pub report: CacheCatalogReport,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SharedCacheCatalog {
    pub shape_snapshots: BTreeMap<SmolStr, SnapshotCacheCatalog>,
    pub world_snapshots: BTreeMap<(SmolStr, i32), SnapshotCacheCatalog>,
}

impl SharedCacheCatalog {
    pub fn shape(&self, shape: &SmolStr) -> Option<&SnapshotCacheCatalog> {
        self.shape_snapshots.get(shape)
    }

    pub fn world(&self, capture: &SmolStr, detail: i32) -> Option<&SnapshotCacheCatalog> {
        self.world_snapshots.get(&(capture.clone(), detail))
    }
}

pub fn cache_source_descriptor(
    snapshot_kind: CacheSnapshotKind,
    snapshot_root: impl Into<SmolStr>,
    semantic_root: impl Into<SmolStr>,
    support_class: SupportClass,
    opaque_boundary: bool,
    can_coarse_support_pruning: bool,
) -> CacheSourceDescriptor {
    CacheSourceDescriptor {
        snapshot_kind,
        snapshot_root: snapshot_root.into(),
        semantic_root: semantic_root.into(),
        support_class,
        opaque_boundary,
        can_coarse_support_pruning,
        budget: CacheBudget::default(),
    }
}

pub fn build_snapshot_cache_catalog(
    snapshot_kind: CacheSnapshotKind,
    snapshot_root: impl Into<SmolStr>,
    sources: Vec<CacheSourceDescriptor>,
) -> Option<SnapshotCacheCatalog> {
    let snapshot_root = snapshot_root.into();
    if sources.is_empty() {
        return None;
    }

    let mut artifacts = Vec::new();
    let mut source_reports = Vec::new();
    let mut requested_distance_bricks = 0u32;
    let mut emitted_distance_bricks = 0u32;
    let mut emitted_support_bricks = 0u32;

    for source in sources {
        requested_distance_bricks = requested_distance_bricks.saturating_add(1);
        let mut rejections = Vec::new();
        if !source.budget.is_enabled() {
            rejections.push(CacheSourceRejection {
                reason: SmolStr::new("budget-disabled"),
                detail: SmolStr::new("cache budget disabled by source descriptor"),
            });
        }
        if !matches!(source.support_class, SupportClass::Bounded)
            || source.opaque_boundary
            || !source.can_coarse_support_pruning
        {
            rejections.push(CacheSourceRejection {
                reason: SmolStr::new("unsupported-support-data"),
                detail: SmolStr::new("source does not provide coarse support pruning"),
            });
        }
        source_reports.push(CacheSourceReport {
            source: source.clone(),
            rejections: rejections.clone(),
        });
        if !rejections.is_empty() {
            continue;
        }

        let support_snapshot = snapshot_handle_for_kind(snapshot_kind, &source.semantic_root);
        let distance_snapshot = snapshot_handle_for_kind(snapshot_kind, &source.semantic_root);
        let support_cache = SupportBrickCache {
            snapshot: support_snapshot,
            schema: support_schema(&source.semantic_root, 1.0),
            report: BrickCacheBuildReport {
                build_status: CacheBuildStatus::Ready,
                rejection_reasons: Vec::new(),
                budget_policy: BrickCacheBudgetPolicy::default(),
                candidate_bricks: 1,
                occupied_bricks: 1,
                narrow_band_bricks: 1,
                distance_samples: 1,
                memory_bytes: 32,
                upload_bytes: 32,
                build_cost_micros: 1,
            },
            bricks: Vec::new(),
        };
        let distance_cache = DistanceBrickCache {
            snapshot: distance_snapshot,
            schema: distance_schema(&source.semantic_root, 1.0),
            report: BrickCacheBuildReport {
                build_status: CacheBuildStatus::Ready,
                rejection_reasons: Vec::new(),
                budget_policy: BrickCacheBudgetPolicy::default(),
                candidate_bricks: 1,
                occupied_bricks: 1,
                narrow_band_bricks: 1,
                distance_samples: 1,
                memory_bytes: 36,
                upload_bytes: 36,
                build_cost_micros: 1,
            },
            bricks: Vec::new(),
        };

        let support_contract = cache_artifact_contract(
            snapshot_kind,
            &snapshot_root,
            &source,
            "support_brick_cache",
        );
        let distance_contract = cache_artifact_contract(
            snapshot_kind,
            &snapshot_root,
            &source,
            "distance_brick_cache",
        );
        artifacts.push(CacheArtifactRecord::Support(SupportCacheArtifactRecord {
            contract: support_contract,
            cache: support_cache,
        }));
        artifacts.push(CacheArtifactRecord::Distance(DistanceCacheArtifactRecord {
            contract: distance_contract,
            cache: distance_cache,
        }));
        emitted_support_bricks = emitted_support_bricks.saturating_add(1);
        emitted_distance_bricks = emitted_distance_bricks.saturating_add(1);
    }

    Some(SnapshotCacheCatalog {
        snapshot_kind,
        snapshot_root,
        artifacts,
        report: CacheCatalogReport {
            source_reports,
            requested_distance_bricks,
            emitted_distance_bricks,
            emitted_support_bricks,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SupportBounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn detail_label(detail: i32) -> &'static str {
    match detail {
        0 => "coarse",
        1 => "fine",
        _ => "detail",
    }
}

fn normalize_support_bounds(bounds: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            bounds.min[0].min(bounds.max[0]),
            bounds.min[1].min(bounds.max[1]),
            bounds.min[2].min(bounds.max[2]),
        ],
        max: [
            bounds.min[0].max(bounds.max[0]),
            bounds.min[1].max(bounds.max[1]),
            bounds.min[2].max(bounds.max[2]),
        ],
    }
}

fn merge_union_support_bounds(lhs: SupportBounds, rhs: SupportBounds) -> SupportBounds {
    SupportBounds {
        min: [
            lhs.min[0].min(rhs.min[0]),
            lhs.min[1].min(rhs.min[1]),
            lhs.min[2].min(rhs.min[2]),
        ],
        max: [
            lhs.max[0].max(rhs.max[0]),
            lhs.max[1].max(rhs.max[1]),
            lhs.max[2].max(rhs.max[2]),
        ],
    }
}

fn cache_artifact_contract(
    snapshot_kind: CacheSnapshotKind,
    snapshot_root: &SmolStr,
    source: &CacheSourceDescriptor,
    artifact_name: &str,
) -> SemanticArtifactContract {
    let kind_name = match snapshot_kind {
        CacheSnapshotKind::ShapeSnapshot => "shape_snapshot",
        CacheSnapshotKind::WorldSnapshot => "world_snapshot",
    };
    let support_class_name = format!("{:?}", source.support_class);
    let logical_schema = ArtifactLogicalSchema {
        namespace: SmolStr::new("acceleration"),
        name: SmolStr::new(artifact_name),
        fields: vec![
            ArtifactLogicalField::new("snapshot_kind", kind_name),
            ArtifactLogicalField::new("snapshot_root", snapshot_root.clone()),
            ArtifactLogicalField::new("semantic_root", source.semantic_root.clone()),
            ArtifactLogicalField::new("support_class", support_class_name),
            ArtifactLogicalField::new("opaque_boundary", source.opaque_boundary.to_string()),
            ArtifactLogicalField::new(
                "coarse_support_pruning",
                source.can_coarse_support_pruning.to_string(),
            ),
        ],
    };
    SemanticArtifactContract {
        id: SmolStr::new(format!(
            "cache::{kind_name}::{artifact_name}::{}::{}",
            snapshot_root, source.semantic_root
        )),
        kind: SemanticArtifactKind::Query,
        logical_schema,
        compatibility: ArtifactCompatibilityRelation::exact_snapshot(
            ArtifactPolicyDigestMode::Exact,
            SemanticEvidenceSummary::contract_bound(),
        ),
        acceleration: None,
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::CurrentSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
        ]),
        producer: SmolStr::new("shared_cache"),
        consumer: SmolStr::new("shared_cache"),
        deterministic: true,
        version: 1,
        transition: None,
        evidence_summary: SemanticEvidenceSummary::contract_bound(),
    }
}

fn snapshot_handle_for_kind(
    snapshot_kind: CacheSnapshotKind,
    name: &SmolStr,
) -> WorldSnapshotHandle {
    match snapshot_kind {
        CacheSnapshotKind::ShapeSnapshot => stable_shape_snapshot_handle(name),
        CacheSnapshotKind::WorldSnapshot => stable_region_snapshot_handle(name),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheBudgetClass {
    Build,
    Upload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheRebuildMode {
    SnapshotDerivedSparse,
    ViewLocalTemporal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheDisableReason {
    ArtifactUnavailable,
    SnapshotMismatch,
    LayoutMismatch,
    UnsupportedQualityTier,
    InsufficientNarrowBandCoverage,
    ArtifactVersionMismatch,
    MemoryBudgetExceeded,
    BuildBudgetExhausted,
    UploadBudgetExhausted,
    UnboundedSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheBuildStatus {
    Ready,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrickCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrickBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrickRayInterval {
    pub start_t: f32,
    pub end_t: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrickCacheBudgetPolicy {
    pub max_candidate_bricks: u32,
    pub max_occupied_bricks: u32,
    pub max_distance_samples: u32,
    pub max_upload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistanceBrickCacheSchema {
    pub version: u32,
    pub brick_edge: u32,
    pub voxel_size: f32,
    pub narrow_band_width: f32,
    pub semantic_root: SmolStr,
    pub conservative_empty_space: bool,
    pub exact_narrow_band: bool,
    pub artifact_scope: CacheArtifactScope,
    pub layout_signature: u64,
    pub snapshot_compatibility_scope: SmolStr,
    pub rebuild_mode: CacheRebuildMode,
    pub build_budget_class: CacheBudgetClass,
    pub upload_budget_class: CacheBudgetClass,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportBrickCacheSchema {
    pub version: u32,
    pub brick_edge: u32,
    pub voxel_size: f32,
    pub narrow_band_width: f32,
    pub semantic_root: SmolStr,
    pub conservative_empty_space: bool,
    pub exact_narrow_band: bool,
    pub artifact_scope: CacheArtifactScope,
    pub layout_signature: u64,
    pub snapshot_compatibility_scope: SmolStr,
    pub rebuild_mode: CacheRebuildMode,
    pub build_budget_class: CacheBudgetClass,
    pub upload_budget_class: CacheBudgetClass,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportBrickRecord {
    pub coord: BrickCoord,
    pub bounds: BrickBounds,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistanceBrickRecord {
    pub coord: BrickCoord,
    pub bounds: BrickBounds,
    pub exact_distance_sample: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrickCacheBuildReport {
    pub build_status: CacheBuildStatus,
    pub rejection_reasons: Vec<CacheDisableReason>,
    pub budget_policy: BrickCacheBudgetPolicy,
    pub candidate_bricks: u32,
    pub occupied_bricks: u32,
    pub narrow_band_bricks: u32,
    pub distance_samples: u32,
    pub memory_bytes: u64,
    pub upload_bytes: u64,
    pub build_cost_micros: u128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportBrickCache {
    pub snapshot: WorldSnapshotHandle,
    pub schema: SupportBrickCacheSchema,
    pub report: BrickCacheBuildReport,
    pub bricks: Vec<SupportBrickRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistanceBrickCache {
    pub snapshot: WorldSnapshotHandle,
    pub schema: DistanceBrickCacheSchema,
    pub report: BrickCacheBuildReport,
    pub bricks: Vec<DistanceBrickRecord>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SharedBrickCacheCatalog {
    pub shape_support: BTreeMap<SmolStr, SupportBrickCache>,
    pub shape_distance: BTreeMap<SmolStr, DistanceBrickCache>,
    pub world_support: BTreeMap<(SmolStr, i32), SupportBrickCache>,
    pub world_distance: BTreeMap<(SmolStr, i32), DistanceBrickCache>,
}

impl Default for BrickCacheBudgetPolicy {
    fn default() -> Self {
        Self {
            max_candidate_bricks: DEFAULT_MAX_CANDIDATE_BRICKS,
            max_occupied_bricks: DEFAULT_MAX_OCCUPIED_BRICKS,
            max_distance_samples: DEFAULT_MAX_DISTANCE_SAMPLES,
            max_upload_bytes: DEFAULT_MAX_UPLOAD_BYTES,
        }
    }
}

impl BrickCacheBuildReport {
    pub fn disabled(
        budget_policy: BrickCacheBudgetPolicy,
        rejection_reasons: Vec<CacheDisableReason>,
        build_cost_micros: u128,
    ) -> Self {
        Self {
            build_status: CacheBuildStatus::Disabled,
            rejection_reasons,
            budget_policy,
            candidate_bricks: 0,
            occupied_bricks: 0,
            narrow_band_bricks: 0,
            distance_samples: 0,
            memory_bytes: 0,
            upload_bytes: 0,
            build_cost_micros,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.build_status, CacheBuildStatus::Ready)
    }
}

impl SupportBrickCache {
    pub fn is_ready(&self) -> bool {
        self.report.is_ready()
    }

    pub fn layout_signature(&self) -> u64 {
        self.schema.layout_signature
    }

    pub fn first_occupied_interval(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
        start_t: f32,
        max_t: f32,
    ) -> Option<BrickRayInterval> {
        first_occupied_interval_for_bounds(
            self.bricks.iter().map(|brick| brick.bounds),
            origin,
            direction,
            start_t,
            max_t,
        )
    }
}

impl DistanceBrickCache {
    pub fn is_ready(&self) -> bool {
        self.report.is_ready()
    }

    pub fn layout_signature(&self) -> u64 {
        self.schema.layout_signature
    }
}

impl SharedBrickCacheCatalog {
    pub fn shape_support(&self, shape: &SmolStr) -> Option<&SupportBrickCache> {
        self.shape_support.get(shape)
    }

    pub fn shape_distance(&self, shape: &SmolStr) -> Option<&DistanceBrickCache> {
        self.shape_distance.get(shape)
    }

    pub fn world_support(&self, capture: &SmolStr, detail: i32) -> Option<&SupportBrickCache> {
        self.world_support.get(&(capture.clone(), detail))
    }

    pub fn world_distance(&self, capture: &SmolStr, detail: i32) -> Option<&DistanceBrickCache> {
        self.world_distance.get(&(capture.clone(), detail))
    }
}

pub fn cache_budget_class_name(class: CacheBudgetClass) -> &'static str {
    match class {
        CacheBudgetClass::Build => "build",
        CacheBudgetClass::Upload => "upload",
    }
}

pub fn cache_rebuild_mode_name(mode: CacheRebuildMode) -> &'static str {
    match mode {
        CacheRebuildMode::SnapshotDerivedSparse => "snapshot_derived_sparse",
        CacheRebuildMode::ViewLocalTemporal => "view_local_temporal",
    }
}

pub fn cache_disable_reason_name(reason: CacheDisableReason) -> &'static str {
    match reason {
        CacheDisableReason::ArtifactUnavailable => "artifact_unavailable",
        CacheDisableReason::SnapshotMismatch => "snapshot_mismatch",
        CacheDisableReason::LayoutMismatch => "layout_mismatch",
        CacheDisableReason::UnsupportedQualityTier => "unsupported_quality_tier",
        CacheDisableReason::InsufficientNarrowBandCoverage => "insufficient_narrow_band_coverage",
        CacheDisableReason::ArtifactVersionMismatch => "artifact_version_mismatch",
        CacheDisableReason::MemoryBudgetExceeded => "memory_budget_exceeded",
        CacheDisableReason::BuildBudgetExhausted => "build_budget_exhausted",
        CacheDisableReason::UploadBudgetExhausted => "upload_budget_exhausted",
        CacheDisableReason::UnboundedSupport => "unbounded_support",
    }
}

fn deterministic_build_cost_micros(
    candidate_bricks: u32,
    occupied_bricks: u32,
    samples: u32,
) -> u128 {
    u128::from(candidate_bricks)
        .saturating_mul(16)
        .saturating_add(u128::from(occupied_bricks).saturating_mul(4))
        .saturating_add(u128::from(samples))
}

pub fn build_shape_brick_cache_catalog(ctx: &QueryExecContext) -> SharedBrickCacheCatalog {
    let evaluator = DirectQueryEvaluator::new(ctx);
    let mut shape_names = ctx.scene.shapes.keys().cloned().collect::<Vec<_>>();
    shape_names.sort();
    let mut catalog = SharedBrickCacheCatalog::default();
    for shape in shape_names {
        let (support, distance) = build_shape_brick_caches(ctx, &evaluator, &shape);
        catalog.shape_support.insert(shape.clone(), support);
        catalog.shape_distance.insert(shape, distance);
    }
    catalog
}

pub fn build_shared_brick_cache_catalog(ctx: &QueryExecContext) -> SharedBrickCacheCatalog {
    let evaluator = DirectQueryEvaluator::new(ctx);
    let mut catalog = build_shape_brick_cache_catalog(ctx);
    let mut region_cases = ctx.region_cases.clone();
    region_cases.sort_by(|left, right| {
        left.region_name
            .cmp(&right.region_name)
            .then(left.scene_id.cmp(&right.scene_id))
    });

    for case in &region_cases {
        for detail in [0, 1] {
            let Ok(shape_names) = case.shapes_for_detail(detail) else {
                continue;
            };
            if shape_names.is_empty() {
                continue;
            }
            let (support, distance) =
                build_world_brick_caches(ctx, &evaluator, &case.region_name, detail, shape_names);
            catalog
                .world_support
                .insert((case.region_name.clone(), detail), support);
            catalog
                .world_distance
                .insert((case.region_name.clone(), detail), distance);
        }
    }

    catalog
}

fn build_world_brick_caches(
    ctx: &QueryExecContext,
    evaluator: &DirectQueryEvaluator<'_>,
    capture: &SmolStr,
    detail: i32,
    shape_names: &[SmolStr],
) -> (SupportBrickCache, DistanceBrickCache) {
    let budget = BrickCacheBudgetPolicy::default();
    let snapshot = ctx
        .region_snapshot_handle(capture)
        .cloned()
        .unwrap_or_else(|| stable_region_snapshot_handle(capture));
    let semantic_root = SmolStr::new(format!("{}::{}", capture, detail_label(detail)));
    let mut support_bounds = Vec::new();
    let mut rejection_reasons = Vec::new();

    for shape in shape_names {
        match evaluator.shape_support_bounds_world(shape).ok().flatten() {
            Some((min, max)) => support_bounds.push(SupportBounds { min, max }),
            None => rejection_reasons.push(CacheDisableReason::UnboundedSupport),
        }
    }

    let Some(mut merged_bounds) = support_bounds
        .iter()
        .copied()
        .reduce(merge_union_support_bounds)
    else {
        let support_schema = support_schema(
            &semantic_root,
            DEFAULT_MIN_BRICK_SIZE / DEFAULT_BRICK_EDGE as f32,
        );
        let distance_schema = distance_schema(
            &semantic_root,
            DEFAULT_MIN_BRICK_SIZE / DEFAULT_BRICK_EDGE as f32,
        );
        let report = BrickCacheBuildReport::disabled(
            budget,
            if rejection_reasons.is_empty() {
                vec![CacheDisableReason::UnboundedSupport]
            } else {
                rejection_reasons
            },
            deterministic_build_cost_micros(0, 0, 0),
        );
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    };

    merged_bounds = normalize_support_bounds(merged_bounds);
    let extent = [
        (merged_bounds.max[0] - merged_bounds.min[0]).abs(),
        (merged_bounds.max[1] - merged_bounds.min[1]).abs(),
        (merged_bounds.max[2] - merged_bounds.min[2]).abs(),
    ];
    let max_extent = extent[0]
        .max(extent[1])
        .max(extent[2])
        .max(DEFAULT_MIN_BRICK_SIZE);
    let brick_size =
        (max_extent / DEFAULT_TARGET_BRICKS_PER_AXIS as f32).max(DEFAULT_MIN_BRICK_SIZE);
    let voxel_size = brick_size / DEFAULT_BRICK_EDGE as f32;
    let support_schema = support_schema(&semantic_root, voxel_size);
    let distance_schema = distance_schema(&semantic_root, voxel_size);
    let dims = grid_dims(merged_bounds.min, merged_bounds.max, brick_size);
    let candidate_bricks = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
    if candidate_bricks > budget.max_candidate_bricks {
        let report = BrickCacheBuildReport::disabled(
            budget,
            {
                let mut reasons = rejection_reasons;
                reasons.push(CacheDisableReason::BuildBudgetExhausted);
                reasons
            },
            deterministic_build_cost_micros(candidate_bricks, 0, 0),
        );
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    }

    let half = brick_size * 0.5;
    let half_diag = (half * half * 3.0).sqrt();
    let mut support_bricks = Vec::new();
    let mut distance_bricks = Vec::new();
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let coord = BrickCoord {
                    x: x as i32,
                    y: y as i32,
                    z: z as i32,
                };
                let bounds = brick_bounds(merged_bounds.min, brick_size, coord);
                let center = [
                    (bounds.min[0] + bounds.max[0]) * 0.5,
                    (bounds.min[1] + bounds.max[1]) * 0.5,
                    (bounds.min[2] + bounds.max[2]) * 0.5,
                ];
                let Ok(distance) = evaluator.eval_world_distance(capture, detail, center) else {
                    continue;
                };
                if distance > half_diag {
                    continue;
                }
                support_bricks.push(SupportBrickRecord { coord, bounds });
                if distance.abs() <= DEFAULT_NARROW_BAND_WIDTH + half_diag {
                    distance_bricks.push(DistanceBrickRecord {
                        coord,
                        bounds,
                        exact_distance_sample: distance,
                    });
                }
            }
        }
    }
    support_bricks.sort_by_key(|brick| (brick.coord.z, brick.coord.y, brick.coord.x));
    distance_bricks.sort_by_key(|brick| (brick.coord.z, brick.coord.y, brick.coord.x));

    let support_memory = (support_bricks.len() as u64) * 32;
    let distance_memory = (distance_bricks.len() as u64) * 36;
    let support_upload = support_memory;
    let distance_upload = distance_memory;
    if support_upload.max(distance_upload) > budget.max_upload_bytes {
        let mut reasons = rejection_reasons;
        reasons.push(CacheDisableReason::UploadBudgetExhausted);
        let report = BrickCacheBuildReport::disabled(
            budget,
            reasons,
            deterministic_build_cost_micros(candidate_bricks, 0, 0),
        );
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    }

    let build_cost_micros = deterministic_build_cost_micros(
        candidate_bricks,
        support_bricks.len() as u32,
        distance_bricks.len() as u32,
    );
    let support_report = BrickCacheBuildReport {
        build_status: CacheBuildStatus::Ready,
        rejection_reasons: rejection_reasons.clone(),
        budget_policy: budget,
        candidate_bricks,
        occupied_bricks: support_bricks.len() as u32,
        narrow_band_bricks: distance_bricks.len() as u32,
        distance_samples: distance_bricks.len() as u32,
        memory_bytes: support_memory,
        upload_bytes: support_upload,
        build_cost_micros,
    };
    let distance_report = BrickCacheBuildReport {
        build_status: CacheBuildStatus::Ready,
        rejection_reasons,
        budget_policy: budget,
        candidate_bricks,
        occupied_bricks: support_bricks.len() as u32,
        narrow_band_bricks: distance_bricks.len() as u32,
        distance_samples: distance_bricks.len() as u32,
        memory_bytes: distance_memory,
        upload_bytes: distance_upload,
        build_cost_micros,
    };
    (
        SupportBrickCache {
            snapshot: snapshot.clone(),
            schema: support_schema,
            report: support_report,
            bricks: support_bricks,
        },
        DistanceBrickCache {
            snapshot,
            schema: distance_schema,
            report: distance_report,
            bricks: distance_bricks,
        },
    )
}

fn build_shape_brick_caches(
    ctx: &QueryExecContext,
    evaluator: &DirectQueryEvaluator<'_>,
    shape: &SmolStr,
) -> (SupportBrickCache, DistanceBrickCache) {
    let budget = BrickCacheBudgetPolicy::default();
    let snapshot = ctx
        .shape_snapshot_handle(shape)
        .cloned()
        .unwrap_or_else(|| stable_shape_snapshot_handle(shape));
    let Some((min, max)) = evaluator.shape_support_bounds_world(shape).ok().flatten() else {
        let support_schema =
            support_schema(shape, DEFAULT_MIN_BRICK_SIZE / DEFAULT_BRICK_EDGE as f32);
        let distance_schema =
            distance_schema(shape, DEFAULT_MIN_BRICK_SIZE / DEFAULT_BRICK_EDGE as f32);
        let report = BrickCacheBuildReport::disabled(
            budget,
            vec![CacheDisableReason::UnboundedSupport],
            deterministic_build_cost_micros(0, 0, 0),
        );
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    };

    let extent = [
        (max[0] - min[0]).abs(),
        (max[1] - min[1]).abs(),
        (max[2] - min[2]).abs(),
    ];
    let max_extent = extent[0]
        .max(extent[1])
        .max(extent[2])
        .max(DEFAULT_MIN_BRICK_SIZE);
    let brick_size =
        (max_extent / DEFAULT_TARGET_BRICKS_PER_AXIS as f32).max(DEFAULT_MIN_BRICK_SIZE);
    let voxel_size = brick_size / DEFAULT_BRICK_EDGE as f32;
    let support_schema = support_schema(shape, voxel_size);
    let distance_schema = distance_schema(shape, voxel_size);
    let dims = grid_dims(min, max, brick_size);
    let candidate_bricks = dims[0].saturating_mul(dims[1]).saturating_mul(dims[2]);
    if candidate_bricks > budget.max_candidate_bricks {
        let report = BrickCacheBuildReport::disabled(
            budget,
            vec![CacheDisableReason::BuildBudgetExhausted],
            deterministic_build_cost_micros(candidate_bricks, 0, 0),
        );
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    }

    let half = brick_size * 0.5;
    let half_diag = (half * half * 3.0).sqrt();
    let mut support_bricks = Vec::new();
    let mut distance_bricks = Vec::new();
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let coord = BrickCoord {
                    x: x as i32,
                    y: y as i32,
                    z: z as i32,
                };
                let bounds = brick_bounds(min, brick_size, coord);
                let center = [
                    (bounds.min[0] + bounds.max[0]) * 0.5,
                    (bounds.min[1] + bounds.max[1]) * 0.5,
                    (bounds.min[2] + bounds.max[2]) * 0.5,
                ];
                let Ok(distance) = evaluator.eval_shape_distance(shape, center) else {
                    continue;
                };
                if distance > half_diag {
                    continue;
                }
                support_bricks.push(SupportBrickRecord { coord, bounds });
                if distance.abs() <= DEFAULT_NARROW_BAND_WIDTH + half_diag {
                    distance_bricks.push(DistanceBrickRecord {
                        coord,
                        bounds,
                        exact_distance_sample: distance,
                    });
                }
            }
        }
    }
    support_bricks.sort_by_key(|brick| (brick.coord.z, brick.coord.y, brick.coord.x));
    distance_bricks.sort_by_key(|brick| (brick.coord.z, brick.coord.y, brick.coord.x));

    let support_memory = (support_bricks.len() as u64) * 32;
    let distance_memory = (distance_bricks.len() as u64) * 36;
    let support_upload = support_memory;
    let distance_upload = distance_memory;
    let mut rejection_reasons = Vec::new();
    if support_bricks.len() as u32 > budget.max_occupied_bricks {
        rejection_reasons.push(CacheDisableReason::MemoryBudgetExceeded);
    }
    if distance_bricks.len() as u32 > budget.max_distance_samples {
        rejection_reasons.push(CacheDisableReason::InsufficientNarrowBandCoverage);
    }
    if support_upload.max(distance_upload) > budget.max_upload_bytes {
        rejection_reasons.push(CacheDisableReason::UploadBudgetExhausted);
    }
    let build_cost_micros = deterministic_build_cost_micros(
        candidate_bricks,
        support_bricks.len() as u32,
        distance_bricks.len() as u32,
    );
    if !rejection_reasons.is_empty() {
        let report = BrickCacheBuildReport::disabled(budget, rejection_reasons, build_cost_micros);
        return (
            SupportBrickCache {
                snapshot: snapshot.clone(),
                schema: support_schema,
                report: report.clone(),
                bricks: Vec::new(),
            },
            DistanceBrickCache {
                snapshot,
                schema: distance_schema,
                report,
                bricks: Vec::new(),
            },
        );
    }

    let support_report = BrickCacheBuildReport {
        build_status: CacheBuildStatus::Ready,
        rejection_reasons: Vec::new(),
        budget_policy: budget,
        candidate_bricks,
        occupied_bricks: support_bricks.len() as u32,
        narrow_band_bricks: distance_bricks.len() as u32,
        distance_samples: distance_bricks.len() as u32,
        memory_bytes: support_memory,
        upload_bytes: support_upload,
        build_cost_micros,
    };
    let distance_report = BrickCacheBuildReport {
        build_status: CacheBuildStatus::Ready,
        rejection_reasons: Vec::new(),
        budget_policy: budget,
        candidate_bricks,
        occupied_bricks: support_bricks.len() as u32,
        narrow_band_bricks: distance_bricks.len() as u32,
        distance_samples: distance_bricks.len() as u32,
        memory_bytes: distance_memory,
        upload_bytes: distance_upload,
        build_cost_micros,
    };
    (
        SupportBrickCache {
            snapshot: snapshot.clone(),
            schema: support_schema,
            report: support_report,
            bricks: support_bricks,
        },
        DistanceBrickCache {
            snapshot,
            schema: distance_schema,
            report: distance_report,
            bricks: distance_bricks,
        },
    )
}

fn support_schema(shape: &SmolStr, voxel_size: f32) -> SupportBrickCacheSchema {
    SupportBrickCacheSchema {
        version: SUPPORT_BRICK_CACHE_VERSION,
        brick_edge: DEFAULT_BRICK_EDGE,
        voxel_size,
        narrow_band_width: DEFAULT_NARROW_BAND_WIDTH,
        semantic_root: shape.clone(),
        conservative_empty_space: true,
        exact_narrow_band: false,
        artifact_scope: CacheArtifactScope::SharedSnapshot,
        layout_signature: cache_layout_signature(
            SUPPORT_BRICK_CACHE_VERSION,
            shape,
            voxel_size,
            false,
        ),
        snapshot_compatibility_scope: SmolStr::new("exact_snapshot"),
        rebuild_mode: CacheRebuildMode::SnapshotDerivedSparse,
        build_budget_class: CacheBudgetClass::Build,
        upload_budget_class: CacheBudgetClass::Upload,
    }
}

fn distance_schema(shape: &SmolStr, voxel_size: f32) -> DistanceBrickCacheSchema {
    DistanceBrickCacheSchema {
        version: DISTANCE_BRICK_CACHE_VERSION,
        brick_edge: DEFAULT_BRICK_EDGE,
        voxel_size,
        narrow_band_width: DEFAULT_NARROW_BAND_WIDTH,
        semantic_root: shape.clone(),
        conservative_empty_space: true,
        exact_narrow_band: true,
        artifact_scope: CacheArtifactScope::SharedSnapshot,
        layout_signature: cache_layout_signature(
            DISTANCE_BRICK_CACHE_VERSION,
            shape,
            voxel_size,
            true,
        ),
        snapshot_compatibility_scope: SmolStr::new("exact_snapshot"),
        rebuild_mode: CacheRebuildMode::SnapshotDerivedSparse,
        build_budget_class: CacheBudgetClass::Build,
        upload_budget_class: CacheBudgetClass::Upload,
    }
}

fn cache_layout_signature(
    version: u32,
    semantic_root: &SmolStr,
    voxel_size: f32,
    exact_narrow_band: bool,
) -> u64 {
    stable_semantic_id(&[
        b"acceleration::brick_cache::v1",
        &version.to_le_bytes(),
        semantic_root.as_str().as_bytes(),
        &DEFAULT_BRICK_EDGE.to_le_bytes(),
        &voxel_size.to_le_bytes(),
        &DEFAULT_NARROW_BAND_WIDTH.to_le_bytes(),
        &[u8::from(exact_narrow_band)],
    ])
}

fn grid_dims(min: [f32; 3], max: [f32; 3], brick_size: f32) -> [u32; 3] {
    let _ = min;
    [
        (((max[0] - min[0]).abs() / brick_size).ceil() as u32).max(1),
        (((max[1] - min[1]).abs() / brick_size).ceil() as u32).max(1),
        (((max[2] - min[2]).abs() / brick_size).ceil() as u32).max(1),
    ]
}

fn brick_bounds(origin_min: [f32; 3], brick_size: f32, coord: BrickCoord) -> BrickBounds {
    let offset = [
        coord.x as f32 * brick_size,
        coord.y as f32 * brick_size,
        coord.z as f32 * brick_size,
    ];
    let min = [
        origin_min[0] + offset[0],
        origin_min[1] + offset[1],
        origin_min[2] + offset[2],
    ];
    BrickBounds {
        min,
        max: [
            min[0] + brick_size,
            min[1] + brick_size,
            min[2] + brick_size,
        ],
    }
}

fn first_occupied_interval_for_bounds(
    bounds: impl Iterator<Item = BrickBounds>,
    origin: [f32; 3],
    direction: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<BrickRayInterval> {
    bounds
        .filter_map(|bounds| ray_aabb_interval(bounds, origin, direction, start_t, max_t))
        .min_by(|left, right| {
            left.start_t
                .partial_cmp(&right.start_t)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    left.end_t
                        .partial_cmp(&right.end_t)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        })
}

fn ray_aabb_interval(
    bounds: BrickBounds,
    origin: [f32; 3],
    direction: [f32; 3],
    start_t: f32,
    max_t: f32,
) -> Option<BrickRayInterval> {
    let mut entry = start_t.max(0.0);
    let mut exit = max_t;
    for axis in 0..3 {
        let dir = direction[axis];
        if dir.abs() <= f32::EPSILON {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir;
        let mut axis_entry = (bounds.min[axis] - origin[axis]) * inv;
        let mut axis_exit = (bounds.max[axis] - origin[axis]) * inv;
        if axis_entry > axis_exit {
            std::mem::swap(&mut axis_entry, &mut axis_exit);
        }
        entry = entry.max(axis_entry);
        exit = exit.min(axis_exit);
        if exit < entry {
            return None;
        }
    }
    (exit >= entry).then_some(BrickRayInterval {
        start_t: entry.max(start_t),
        end_t: exit.min(max_t),
    })
}
