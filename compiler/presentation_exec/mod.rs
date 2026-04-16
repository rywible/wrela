pub(crate) mod clipmap;
pub mod controller;
pub mod cost;
mod cpu;
pub mod debug;
pub mod framegraph;
mod gpu_primary;
pub mod gpu_resources;
pub mod resources;
mod temporal;
mod wgsl;

use crate::acceleration::clipmap::ViewDistanceClipmapArtifact;
use crate::acceleration::{AccelerationNodeKind, BoundDescriptorKind};
use crate::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactEvidenceCompatibility, ArtifactLogicalField,
    ArtifactLogicalSchema, ArtifactPolicyCompatibility, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactValidityPredicate, ArtifactValidityRule,
    SemanticArtifactContract, SemanticArtifactKind,
};
use crate::artifact_key::ArtifactReuseKey;
use crate::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, StoredArtifact,
};
pub use crate::execution_policy::{
    PresentationExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
use crate::gpu_runtime::{GpuRuntimeMetrics, classify_execution_bound};
use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::presentation_contract::{
    AttachmentLifetime, AttachmentResolutionClass, AttachmentResolutionScale, CanonicalCameraInput,
    CanonicalLightInput, CanonicalRayBudget, CanonicalViewportInput, FrameAttachmentContract,
    FrameAttachmentKind, FrameContract, HistoryCompatibilityKey,
    LegacyCompatibilityProjectionInput, PresentationLightingInputs, RealtimeQualityContract,
    RealtimeQualityState, canonical_screen_sample_query, legacy_preview_screen_sample_query,
};
use crate::presentation_plan::{PresentationPlan, PrimaryVisibilityPassContract};
use crate::query_exec::cpu::DirectQueryEvaluator;
use crate::query_exec::{
    BatchQueryExecutionTrace, QueryExecContext, QueryExecError, QueryTraceSolverMode,
    execute_batch_query_with_solver_mode_with_snapshot_on,
};
use crate::query_plan::{ArtifactContract, ArtifactSchema, BatchQueryPlan, DispatchBackend};
use crate::query_solver::{
    RaySolverArtifactReuseResolution, RaySolverContinuationResolution, RaySolverDiagnosticSummary,
    RaySolverIntentDisposition, RaySolverMethod, RaySolverPlan, ray_solver_method_name,
};
use crate::semantic_evidence::SemanticEvidenceSummary;
use crate::world_identity::{SnapshotEpoch, SnapshotIdentityReport, WorldSnapshotHandle};
use resources::{
    AttachmentResourceSet, PresentationResourceError, allocate_attachment_resources_without_history,
};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::env;
use thiserror::Error;

pub use self::controller::AdaptivePresentationController;
pub use cost::{
    PresentationAttachmentBytes, PresentationFrameCostReport, PresentationPassCost,
    PresentationQualityReport, quality_report, radiance_mode_name, render_execution_policy_report,
    render_frame_cost_report, render_semantic_domain_report,
};
pub use framegraph::{PresentationFramegraph, PresentationFramegraphPass};
pub use gpu_resources::{
    AttachmentBacking, GpuAttachmentArena, GpuAttachmentArenaError, GpuAttachmentSlot,
};
pub use resources::{
    AttachmentResource, FrameAttachmentLayout, PresentationResourceError as ResourceError,
    allocate_attachment_resources as allocate_frame_attachment_resources,
    allocate_attachment_resources_with_history as allocate_frame_attachment_resources_with_history,
    allocate_attachment_resources_without_history as allocate_frame_attachment_resources_without_history,
    attachment_element_abi, frame_attachment_layout,
};

#[derive(Debug, Error)]
pub enum PresentationExecError {
    #[error("presentation plan '{plan}' does not contain a screen-sample generation pass")]
    MissingScreenSamplePass { plan: SmolStr },
    #[error("presentation plan '{plan}' does not contain a primary-visibility pass")]
    MissingPrimaryVisibilityPass { plan: SmolStr },
    #[error("presentation execution expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("missing field '{field}' on '{record}'")]
    MissingField { record: String, field: SmolStr },
    #[error("unsupported presentation plan: {message}")]
    UnsupportedPlan { message: String },
    #[error(transparent)]
    Query(#[from] QueryExecError),
    #[error(transparent)]
    Resource(#[from] PresentationResourceError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationExecutionInput {
    pub region_snapshot: WorldSnapshotHandle,
    pub frame_domain: KernelValue,
    pub frame_state: KernelValue,
    pub history: Option<PresentationTemporalHistory>,
    pub materialize_cpu_attachments: bool,
    pub lighting: PresentationLightingInputs,
    pub compatibility_projection: Option<LegacyCompatibilityProjectionInput>,
    pub execution_policy: PresentationExecutionPolicy,
    pub query_trace_solver_mode: QueryTraceSolverMode,
    pub quality_override: Option<RealtimeQualityState>,
    pub backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationTemporalHistorySlot {
    pub slot: u8,
    pub attachment: SmolStr,
    pub compatibility: HistoryCompatibilityKey,
    pub reuse_key: ArtifactReuseKey,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationTemporalHistory {
    pub presentation_frame: u32,
    pub snapshot: SnapshotIdentityReport,
    pub snapshot_handle: WorldSnapshotHandle,
    pub attachments: AttachmentResourceSet,
    pub slots: Vec<PresentationTemporalHistorySlot>,
    pub clipmap: Option<ViewDistanceClipmapArtifact>,
}

impl PresentationExecutionInput {
    pub fn region_capture_name(&self) -> &SmolStr {
        self.region_snapshot.capture_name()
    }

    pub fn region_capture_value(&self) -> KernelValue {
        self.region_snapshot.capture_value()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationRayStepDistribution {
    pub zero: u32,
    pub short: u32,
    pub medium: u32,
    pub long: u32,
    pub extreme: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationMetrics {
    pub sample_count: u32,
    pub hit_count: u32,
    pub miss_count: u32,
    pub candidate_count: u32,
    pub candidates_before_pruning: u32,
    pub candidates_after_pruning: u32,
    pub candidate_reduction: u32,
    pub trace_steps_total: u32,
    pub trace_steps_max: u32,
    pub ray_step_distribution: PresentationRayStepDistribution,
    pub dispatch_items: u32,
    pub dispatch_workgroups: [u32; 3],
    pub solver_summary: Option<RaySolverDiagnosticSummary>,
    pub solver_methods: Vec<RaySolverMethod>,
    pub dense_fallback_count: u32,
    pub continuation_available_count: u32,
    pub continuation_consumed_count: u32,
    pub continuation_rejected_count: u32,
    pub continuation_unavailable_count: u32,
    pub continuation_diagnostics: Vec<String>,
    pub acceleration_node_visits: u32,
    pub union_cluster_visits: u32,
    pub ray_support_interval_rejections: u32,
    pub ray_support_entry_jumps: u32,
    pub repeat_cell_skips: u32,
    pub cache_brick_visits: u32,
    pub cache_brick_hits: u32,
    pub cache_brick_misses: u32,
    pub cache_interval_advances: u32,
    pub accepted_relaxed_steps: u32,
    pub rejected_relaxed_steps: u32,
    pub solver_relaxed_attempts: u32,
    pub solver_relaxed_no_root_advances: u32,
    pub solver_relaxed_brackets: u32,
    pub solver_relaxed_unresolved: u32,
    pub solver_interval_attempts: u32,
    pub solver_interval_no_root_advances: u32,
    pub solver_interval_brackets: u32,
    pub solver_interval_unresolved: u32,
    pub solver_refinement_attempts: u32,
    pub solver_refinement_failures: u32,
    pub solver_repeat_attempts: u32,
    pub solver_repeat_supported: u32,
    pub solver_repeat_inapplicable: u32,
    pub solver_repeat_unsupported: u32,
    pub solver_repeat_unsupported_form: u32,
    pub solver_repeat_unsupported_bounds: u32,
    pub solver_repeat_cells_enumerated: u32,
    pub analytic_transformed_hits: u32,
    pub interval_subdivisions: u32,
    pub interval_proof_successes: u32,
    pub observer_continuation_seed_hits: u32,
    pub field_samples: u32,
    pub gpu_runtime: GpuRuntimeMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationExecutionResult {
    pub plan_name: SmolStr,
    pub backend: DispatchBackend,
    pub width: u32,
    pub height: u32,
    pub screen_samples: Vec<KernelValue>,
    pub attachments: AttachmentResourceSet,
    pub history: Option<PresentationTemporalHistory>,
    pub metrics: PresentationMetrics,
    pub frame_cost: PresentationFrameCostReport,
    pub query_trace: BatchQueryExecutionTrace,
}

#[derive(Debug, Clone)]
pub struct AdaptivePresentationSession {
    controller: AdaptivePresentationController,
    history: Option<PresentationTemporalHistory>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PassRuntimeStats {
    pub pass_id: String,
    pub pass_kind: String,
    pub work_items: u32,
    pub elapsed_micros: u128,
    pub gpu_elapsed_micros: Option<u128>,
    pub dispatch_count: u32,
    pub attachment_bytes_read: u64,
    pub attachment_bytes_written: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TileCullingStats {
    pub total_tiles: u32,
    pub active_tiles: u32,
    pub skipped_samples: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TileCandidateQueueState {
    Empty,
    Singleton,
    Packeted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileCandidateSpan {
    pub tile_index: u32,
    pub candidate_start: u32,
    pub candidate_len: u32,
    pub state: TileCandidateQueueState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileCandidateDispatchPacket {
    pub tile_indices: Vec<u32>,
    pub state: TileCandidateQueueState,
    pub candidate_shapes: Vec<SmolStr>,
    pub sample_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileCandidateArtifact {
    pub enabled: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub tile_size: u32,
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub total_samples: u32,
    pub active_samples: u32,
    pub skipped_samples: u32,
    pub candidate_shapes: Vec<SmolStr>,
    pub tile_spans: Vec<TileCandidateSpan>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TileCandidateStats {
    pub total_samples: u32,
    pub active_samples: u32,
    pub packet_count: u32,
    pub packet_size: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ParticipantQueryWorkItem {
    pub target_index: usize,
    pub point_query: KernelValue,
    pub point_direction_query: KernelValue,
}

#[derive(Debug, Clone)]
pub(crate) struct TileCullingMask {
    pub active_samples: Vec<usize>,
    pub stats: TileCullingStats,
    pub candidate_table: TileCandidateArtifact,
}

pub(crate) const PRESENTATION_WORKGROUP_SIZE_CANDIDATES: [u32; 3] = [32, 64, 128];
pub(crate) const PRESENTATION_TILE_SIZE: u32 = 8;
// Resident post-visibility passes converged on 64 as the best default after Phase 46:
// it still fills an 8x8 tile packet in one group while avoiding the occupancy drop
// from always pushing to the largest legal candidate on mixed scatter/build passes.
pub(crate) const PRESENTATION_RETUNED_DEFAULT_WORKGROUP_SIZE: u32 = 64;

pub fn validate_presentation_workgroup_size(
    requested_workgroup_size: u32,
    adapter_limits: &wgpu::Limits,
) -> Result<u32, PresentationExecError> {
    if !PRESENTATION_WORKGROUP_SIZE_CANDIDATES.contains(&requested_workgroup_size) {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "presentation WGSL workgroup size {requested_workgroup_size} is not in the legal set {:?}",
                PRESENTATION_WORKGROUP_SIZE_CANDIDATES
            ),
        });
    }
    if requested_workgroup_size > adapter_limits.max_compute_workgroup_size_x
        || requested_workgroup_size > adapter_limits.max_compute_invocations_per_workgroup
    {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "presentation WGSL workgroup size {requested_workgroup_size} is incompatible with adapter limits x={} invocations={}",
                adapter_limits.max_compute_workgroup_size_x,
                adapter_limits.max_compute_invocations_per_workgroup
            ),
        });
    }
    Ok(requested_workgroup_size)
}

fn supported_presentation_workgroup_sizes(adapter_limits: &wgpu::Limits) -> Vec<u32> {
    PRESENTATION_WORKGROUP_SIZE_CANDIDATES
        .iter()
        .copied()
        .filter_map(|candidate| {
            validate_presentation_workgroup_size(candidate, adapter_limits).ok()
        })
        .collect()
}

fn requested_presentation_workgroup_size_override() -> Result<Option<u32>, PresentationExecError> {
    env::var(crate::query_exec::WGSL_WORKGROUP_SIZE_OVERRIDE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| PresentationExecError::UnsupportedPlan {
                    message: format!(
                        "{} must be an integer legal WGSL workgroup size in {:?}, found `{}`",
                        crate::query_exec::WGSL_WORKGROUP_SIZE_OVERRIDE_ENV,
                        PRESENTATION_WORKGROUP_SIZE_CANDIDATES,
                        value.trim()
                    ),
                })
        })
        .transpose()
}

pub fn select_presentation_workgroup_size(
    adapter_limits: &wgpu::Limits,
) -> Result<u32, PresentationExecError> {
    let supported_sizes = supported_presentation_workgroup_sizes(adapter_limits);
    if supported_sizes.is_empty() {
        return Err(PresentationExecError::UnsupportedPlan {
            message: format!(
                "adapter limits only allow x={} invocations={} per workgroup, which is below the smallest legal presentation workgroup size of 32",
                adapter_limits.max_compute_workgroup_size_x,
                adapter_limits.max_compute_invocations_per_workgroup
            ),
        });
    }
    if let Some(requested_workgroup_size) = requested_presentation_workgroup_size_override()? {
        return validate_presentation_workgroup_size(requested_workgroup_size, adapter_limits);
    }
    if supported_sizes.contains(&PRESENTATION_RETUNED_DEFAULT_WORKGROUP_SIZE) {
        return Ok(PRESENTATION_RETUNED_DEFAULT_WORKGROUP_SIZE);
    }
    supported_sizes
        .first()
        .copied()
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: "presentation WGSL adapter reported no supported workgroup sizes".to_string(),
        })
}

pub(crate) fn packetize_sample_indices(indices: &[usize], packet_size: u32) -> Vec<Vec<usize>> {
    let packet_size = packet_size.max(1) as usize;
    indices
        .chunks(packet_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

pub(crate) fn tile_candidate_stats(
    total_samples: usize,
    active_samples: usize,
    packet_count: usize,
    packet_size: u32,
) -> TileCandidateStats {
    TileCandidateStats {
        total_samples: total_samples as u32,
        active_samples: active_samples as u32,
        packet_count: packet_count as u32,
        packet_size: packet_size.max(1),
    }
}

pub(crate) fn tile_candidate_packet_sample_count(packets: &[TileCandidateDispatchPacket]) -> usize {
    packets
        .iter()
        .map(|packet| packet.sample_indices.len())
        .sum()
}

pub(crate) fn tile_candidate_packet_fragment_count(
    packets: &[TileCandidateDispatchPacket],
    packet_size: u32,
) -> usize {
    let packet_size = packet_size.max(1) as usize;
    packets
        .iter()
        .map(|packet| packet.sample_indices.len().div_ceil(packet_size))
        .sum()
}

pub(crate) fn build_tile_candidate_artifact(
    viewport: CanonicalViewportInput,
    tile_candidates: &[Vec<SmolStr>],
    enabled: bool,
) -> TileCandidateArtifact {
    let tile_size = PRESENTATION_TILE_SIZE;
    let tiles_x = viewport.width.div_ceil(tile_size);
    let tiles_y = viewport.height.div_ceil(tile_size);
    let total_samples = viewport.width.saturating_mul(viewport.height);
    if !enabled {
        return TileCandidateArtifact {
            enabled: false,
            viewport_width: viewport.width,
            viewport_height: viewport.height,
            tile_size,
            tiles_x,
            tiles_y,
            total_samples,
            active_samples: 0,
            skipped_samples: total_samples,
            candidate_shapes: Vec::new(),
            tile_spans: Vec::new(),
        };
    }

    let mut candidate_shapes = Vec::new();
    let mut tile_spans = Vec::new();
    let mut active_samples = 0u32;
    for tile_index in 0..(tiles_x * tiles_y) {
        let indices = tile_candidates
            .get(tile_index as usize)
            .cloned()
            .unwrap_or_default();
        let candidate_start = candidate_shapes.len() as u32;
        let candidate_len = indices.len() as u32;
        let state = match candidate_len {
            0 => TileCandidateQueueState::Empty,
            1 => TileCandidateQueueState::Singleton,
            _ => TileCandidateQueueState::Packeted,
        };
        if candidate_len > 0 {
            active_samples = active_samples
                .saturating_add(tile_sample_count(viewport, tile_size, tiles_x, tile_index));
        }
        candidate_shapes.extend(indices.iter().cloned());
        tile_spans.push(TileCandidateSpan {
            tile_index,
            candidate_start,
            candidate_len,
            state,
        });
    }

    TileCandidateArtifact {
        enabled: true,
        viewport_width: viewport.width,
        viewport_height: viewport.height,
        tile_size,
        tiles_x,
        tiles_y,
        total_samples,
        active_samples,
        skipped_samples: total_samples.saturating_sub(active_samples),
        candidate_shapes,
        tile_spans,
    }
}

pub(crate) fn tile_candidate_dispatch_packets(
    artifact: &TileCandidateArtifact,
    packet_size: u32,
) -> Vec<TileCandidateDispatchPacket> {
    if !artifact.enabled {
        return Vec::new();
    }
    let mut grouped =
        BTreeMap::<(TileCandidateQueueState, Vec<SmolStr>), (Vec<u32>, Vec<usize>)>::new();
    for span in &artifact.tile_spans {
        if span.candidate_len == 0 {
            continue;
        }
        let candidate_start = span.candidate_start as usize;
        let candidate_end = candidate_start + span.candidate_len as usize;
        let candidate_shapes = artifact.candidate_shapes[candidate_start..candidate_end].to_vec();
        let sample_indices = tile_sample_indices(artifact, span.tile_index);
        if sample_indices.is_empty() {
            continue;
        }
        let entry = grouped
            .entry((span.state, candidate_shapes))
            .or_insert_with(|| (Vec::new(), Vec::new()));
        entry.0.push(span.tile_index);
        entry.1.extend(sample_indices);
    }
    let mut packets = Vec::new();
    for ((state, candidate_shapes), (tile_indices, sample_indices)) in grouped {
        match state {
            TileCandidateQueueState::Empty => {}
            TileCandidateQueueState::Singleton => {
                packets.push(TileCandidateDispatchPacket {
                    tile_indices: tile_indices.clone(),
                    state,
                    candidate_shapes: candidate_shapes.clone(),
                    sample_indices,
                });
            }
            TileCandidateQueueState::Packeted => {
                for chunk in packetize_sample_indices(&sample_indices, packet_size) {
                    packets.push(TileCandidateDispatchPacket {
                        tile_indices: tile_indices.clone(),
                        state,
                        candidate_shapes: candidate_shapes.clone(),
                        sample_indices: chunk,
                    });
                }
            }
        }
    }
    packets
}

pub(crate) fn build_tile_candidate_span_words(
    artifact: &TileCandidateArtifact,
    active_samples: &[usize],
    _packet_size: u32,
) -> Vec<u32> {
    let mut spans = vec![0u32; artifact.total_samples as usize * 2];
    if artifact.total_samples == 0 {
        return spans;
    }

    let mut write_span = |sample_index: usize, candidate_start: u32, candidate_len: u32| {
        let span_index = sample_index.saturating_mul(2);
        if span_index + 1 < spans.len() {
            spans[span_index] = candidate_start;
            spans[span_index + 1] = candidate_len;
        }
    };

    if !artifact.enabled {
        for &sample_index in active_samples {
            write_span(sample_index, u32::MAX, u32::MAX);
        }
        return spans;
    }

    for &sample_index in active_samples {
        let sample_index = sample_index.min(artifact.total_samples.saturating_sub(1) as usize);
        let sample_x = sample_index as u32 % artifact.viewport_width.max(1);
        let sample_y = sample_index as u32 / artifact.viewport_width.max(1);
        let tile_x = sample_x / artifact.tile_size.max(1);
        let tile_y = sample_y / artifact.tile_size.max(1);
        let tile_index = (tile_y * artifact.tiles_x + tile_x) as usize;
        if let Some(span) = artifact.tile_spans.get(tile_index) {
            write_span(sample_index, span.candidate_start, span.candidate_len);
        }
    }

    spans
}

fn tile_sample_count(
    viewport: CanonicalViewportInput,
    tile_size: u32,
    tiles_x: u32,
    tile_index: u32,
) -> u32 {
    let tile_x = tile_index % tiles_x;
    let tile_y = tile_index / tiles_x;
    let start_x = tile_x.saturating_mul(tile_size);
    let start_y = tile_y.saturating_mul(tile_size);
    let width = viewport.width.saturating_sub(start_x).min(tile_size);
    let height = viewport.height.saturating_sub(start_y).min(tile_size);
    width.saturating_mul(height)
}

fn tile_sample_indices(artifact: &TileCandidateArtifact, tile_index: u32) -> Vec<usize> {
    let tile_x = tile_index % artifact.tiles_x;
    let tile_y = tile_index / artifact.tiles_x;
    let start_x = tile_x.saturating_mul(artifact.tile_size);
    let start_y = tile_y.saturating_mul(artifact.tile_size);
    let end_x = start_x
        .saturating_add(artifact.tile_size)
        .min(artifact.viewport_width);
    let end_y = start_y
        .saturating_add(artifact.tile_size)
        .min(artifact.viewport_height);
    let mut out = Vec::new();
    for y in start_y..end_y {
        for x in start_x..end_x {
            out.push((y * artifact.viewport_width + x) as usize);
        }
    }
    out
}

fn build_observer_local_tile_candidate_artifact(
    ctx: &QueryExecContext,
    capture_name: &SmolStr,
    detail: i32,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
) -> Option<TileCandidateArtifact> {
    let forest = ctx.world_acceleration_forest(capture_name, detail)?;
    let tile_size = PRESENTATION_TILE_SIZE;
    let tiles_x = viewport.width.div_ceil(tile_size);
    let tiles_y = viewport.height.div_ceil(tile_size);
    let mut tile_candidates = vec![Vec::<SmolStr>::new(); (tiles_x * tiles_y) as usize];
    let mut saw_leaf_candidate = false;

    for node in &forest.nodes {
        if !matches!(node.kind, AccelerationNodeKind::LeafCandidate) {
            continue;
        }
        saw_leaf_candidate = true;
        let shape = node.leaf_payload.as_ref()?.semantic_id.clone();
        let bounds = acceleration_leaf_bounds(node)?;
        let coverage = match projected_bounds_tile_range(
            tiles_x, tiles_y, tile_size, camera, viewport, bounds.0, bounds.1,
        ) {
            Ok(Some(coverage)) => coverage,
            Ok(None) => continue,
            Err(()) => return None,
        };
        for tile_y in coverage.2..=coverage.3 {
            for tile_x in coverage.0..=coverage.1 {
                let tile_index = (tile_y * tiles_x + tile_x) as usize;
                if let Some(candidates) = tile_candidates.get_mut(tile_index) {
                    candidates.push(shape.clone());
                }
            }
        }
    }

    if !saw_leaf_candidate {
        return None;
    }

    for candidates in &mut tile_candidates {
        candidates.sort_unstable();
        candidates.dedup();
    }

    Some(build_tile_candidate_artifact(
        viewport,
        &tile_candidates,
        true,
    ))
}

fn acceleration_leaf_bounds(
    node: &crate::acceleration::AccelerationNode,
) -> Option<([f32; 3], [f32; 3])> {
    node.bounds.iter().find_map(|bound| {
        if !matches!(bound.kind, BoundDescriptorKind::AxisAlignedBounds) {
            return None;
        }
        parse_bounds_summary(&bound.summary)
    })
}

fn parse_bounds_summary(summary: &str) -> Option<([f32; 3], [f32; 3])> {
    let (min, max) = summary.split_once("|max=")?;
    let min = min.strip_prefix("min=")?;
    Some((parse_summary_vec3(min)?, parse_summary_vec3(max)?))
}

fn parse_summary_vec3(summary: &str) -> Option<[f32; 3]> {
    let parts = summary
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [x, y, z] = parts.try_into().ok()?;
    Some([x, y, z])
}

pub fn execute_plan(
    ctx: &QueryExecContext,
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> Result<PresentationExecutionResult, PresentationExecError> {
    match input.backend {
        DispatchBackend::Wgsl => wgsl::execute_plan(ctx, plan, input),
        DispatchBackend::Cpu | DispatchBackend::Auto | DispatchBackend::VirtualGpu => {
            cpu::execute_plan(ctx, plan, input)
        }
    }
}

pub fn resolved_quality_state(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> RealtimeQualityState {
    input
        .quality_override
        .clone()
        .unwrap_or_else(|| plan.frame.quality.initial_state())
}

impl AdaptivePresentationSession {
    pub fn new(contract: RealtimeQualityContract) -> Self {
        Self {
            controller: AdaptivePresentationController::new(contract),
            history: None,
        }
    }

    pub fn with_window(mut self, moving_average_window: usize) -> Self {
        self.controller = self.controller.clone().with_window(moving_average_window);
        self
    }

    pub fn controller(&self) -> &AdaptivePresentationController {
        &self.controller
    }

    pub fn history(&self) -> Option<&PresentationTemporalHistory> {
        self.history.as_ref()
    }

    pub fn execute_frame(
        &mut self,
        ctx: &QueryExecContext,
        plan: &PresentationPlan,
        input: &PresentationExecutionInput,
    ) -> Result<PresentationExecutionResult, PresentationExecError> {
        let mut frame_input = input.clone();
        frame_input.history = self.history.clone();
        frame_input.quality_override = Some(self.controller.quality().clone());
        let result = execute_plan(ctx, plan, &frame_input)?;
        self.history = result.history.clone();
        let _ = self.controller.observe_frame(&result.frame_cost);
        Ok(result)
    }
}

pub(crate) fn effective_plan_for_quality(
    plan: &PresentationPlan,
    quality: &RealtimeQualityState,
) -> PresentationPlan {
    let mut effective = plan.clone();
    effective.apply_participant_policy(quality.radiance_enabled(), quality.media_enabled);
    let internal_divisor = internal_resolution_divisor(quality.internal_resolution_scale);
    if internal_divisor > 1 {
        for attachment in &mut effective.frame.outputs {
            if matches!(
                attachment.kind,
                FrameAttachmentKind::Surface
                    | FrameAttachmentKind::Radiance
                    | FrameAttachmentKind::Medium
            ) {
                apply_attachment_divisor(attachment, internal_divisor);
            }
        }
    }
    if quality.half_res_participants {
        for attachment in &mut effective.frame.outputs {
            if matches!(
                attachment.kind,
                FrameAttachmentKind::Radiance | FrameAttachmentKind::Medium
            ) {
                apply_attachment_divisor(attachment, 2);
            }
        }
    }
    effective
}

pub(crate) fn adjusted_ray_budget(
    policy: PresentationExecutionPolicy,
    quality: &RealtimeQualityState,
) -> CanonicalRayBudget {
    let budget = policy.primary_rays;
    CanonicalRayBudget {
        max_steps: budget.max_steps.min(quality.primary_max_steps),
        max_distance: budget.max_distance,
        min_step: budget.min_step,
        hit_epsilon: budget.hit_epsilon,
    }
}

pub(crate) fn full_attachment_byte_size(attachments: &AttachmentResourceSet, name: &str) -> u64 {
    attachments
        .attachment(name)
        .map(|attachment| attachment.bytes.len() as u64)
        .unwrap_or_default()
}

pub(crate) fn attachment_byte_reports(
    attachments: &AttachmentResourceSet,
    arena: Option<&GpuAttachmentArena>,
) -> Vec<PresentationAttachmentBytes> {
    attachments
        .attachments
        .iter()
        .map(|(name, attachment)| PresentationAttachmentBytes {
            attachment: name.to_string(),
            width: attachment.layout.width,
            height: attachment.layout.height,
            total_size_bytes: attachment.bytes.len() as u64,
            backing: attachment_backing_report(
                arena
                    .and_then(|gpu_attachments| gpu_attachments.attachment(name.as_str()))
                    .map(|slot| match &slot.backing {
                        AttachmentBacking::CpuBytes(_) => "cpu_bytes",
                        AttachmentBacking::GpuBuffer { .. } => "gpu_buffer",
                    })
                    .unwrap_or("cpu_bytes"),
                &attachment.layout.attachment,
            ),
        })
        .collect()
}

fn attachment_backing_report(backing: &str, attachment: &FrameAttachmentContract) -> String {
    format!(
        "{}({})",
        backing,
        resources::attachment_policy_description(attachment)
    )
}

pub(crate) fn encode_values_at_indices(
    attachments: &mut AttachmentResourceSet,
    name: &str,
    indices: &[usize],
    values: &[KernelValue],
) -> Result<(), PresentationExecError> {
    let Some(attachment) = attachments.attachment_mut(name) else {
        return Ok(());
    };
    for (index, value) in indices.iter().zip(values) {
        attachment.encode(*index, value)?;
    }
    Ok(())
}

pub(crate) fn shade_lookup_value(
    attachments: &AttachmentResourceSet,
    name: &str,
    full_index: usize,
    fallback: &KernelValue,
) -> Result<KernelValue, PresentationExecError> {
    let Some(attachment) = attachments.attachment(name) else {
        return Ok(fallback.clone());
    };
    if attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return attachment
            .decode(full_index)
            .map_err(PresentationExecError::Resource);
    }
    let x = (full_index as u32) % attachments.width.max(1);
    let y = (full_index as u32) / attachments.width.max(1);
    let scaled_x = x / attachment.layout.attachment.scale.divisor_x.max(1);
    let scaled_y = y / attachment.layout.attachment.scale.divisor_y.max(1);
    let scaled_index = (scaled_y * attachment.layout.width + scaled_x) as usize;
    attachment
        .decode(scaled_index)
        .map_err(PresentationExecError::Resource)
}

pub(crate) fn participant_query_work_items(
    input: &PresentationExecutionInput,
    screen_samples: &[KernelValue],
    hits: &[KernelValue],
    attachments: &AttachmentResourceSet,
    attachment_name: &str,
    miss_sample_distance: f32,
    include_misses: bool,
) -> Result<Vec<ParticipantQueryWorkItem>, PresentationExecError> {
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let view_camera = expect_struct(field(view, "camera")?, "Camera")?;
    let camera_position = expect_vec3(field(view_camera, "position")?)?;
    let Some(attachment) = attachments.attachment(attachment_name) else {
        return Ok(Vec::new());
    };
    let scaled = attachment.layout.width != attachments.width
        || attachment.layout.height != attachments.height;
    let mut items = Vec::new();
    let mut scaled_cells = BTreeMap::new();
    for (index, (sample, hit)) in screen_samples.iter().zip(hits).enumerate() {
        let is_hit = hit_flag(hit)?;
        if !include_misses && !is_hit {
            continue;
        }
        let ray = expect_struct(
            field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
            "RayQuery",
        )?;
        let ray_direction = expect_vec3(field(ray, "direction")?)?;
        let point = if is_hit {
            hit_position(hit)?
        } else {
            [
                camera_position[0] + ray_direction[0] * miss_sample_distance,
                camera_position[1] + ray_direction[1] * miss_sample_distance,
                camera_position[2] + ray_direction[2] * miss_sample_distance,
            ]
        };
        let target_index = attachment_target_index(attachments, attachment, index);
        let item = ParticipantQueryWorkItem {
            target_index,
            point_query: point_query_value(point),
            point_direction_query: point_direction_query_value(point, ray_direction),
        };
        if scaled {
            scaled_cells.entry(target_index).or_insert(item);
        } else {
            items.push(item);
        }
    }
    if scaled {
        items.extend(scaled_cells.into_values());
    }
    Ok(items)
}

pub(crate) fn participant_query_work_items_without_screen_samples(
    input: &PresentationExecutionInput,
    camera_input: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    hits: &[KernelValue],
    attachments: &AttachmentResourceSet,
    attachment_name: &str,
    miss_sample_distance: f32,
    include_misses: bool,
) -> Result<Vec<ParticipantQueryWorkItem>, PresentationExecError> {
    let frame = expect_struct(&input.frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let view_camera = expect_struct(field(view, "camera")?, "Camera")?;
    let camera_position = expect_vec3(field(view_camera, "position")?)?;
    let Some(attachment) = attachments.attachment(attachment_name) else {
        return Ok(Vec::new());
    };
    let scaled = attachment.layout.width != attachments.width
        || attachment.layout.height != attachments.height;
    let mut items = Vec::new();
    let mut scaled_cells = BTreeMap::new();
    for (index, hit) in hits.iter().enumerate() {
        let is_hit = hit_flag(hit)?;
        if !include_misses && !is_hit {
            continue;
        }
        let ray_direction = view_ray_direction_for_index(
            input,
            camera_input,
            viewport,
            jitter_pixels,
            legacy_projection,
            index,
        );
        let point = if is_hit {
            hit_position(hit)?
        } else {
            [
                camera_position[0] + ray_direction[0] * miss_sample_distance,
                camera_position[1] + ray_direction[1] * miss_sample_distance,
                camera_position[2] + ray_direction[2] * miss_sample_distance,
            ]
        };
        let target_index = attachment_target_index(attachments, attachment, index);
        let item = ParticipantQueryWorkItem {
            target_index,
            point_query: point_query_value(point),
            point_direction_query: point_direction_query_value(point, ray_direction),
        };
        if scaled {
            scaled_cells.entry(target_index).or_insert(item);
        } else {
            items.push(item);
        }
    }
    if scaled {
        items.extend(scaled_cells.into_values());
    }
    Ok(items)
}

pub(crate) fn view_ray_direction_for_index(
    input: &PresentationExecutionInput,
    camera_input: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    index: usize,
) -> [f32; 3] {
    let x = (index as u32) % viewport.width.max(1);
    let y = (index as u32) / viewport.width.max(1);
    let budget = CanonicalRayBudget {
        max_distance: 0.0,
        min_step: 0.0,
        hit_epsilon: 0.0,
        max_steps: 0,
    };
    if legacy_projection {
        legacy_preview_screen_sample_query(
            camera_input,
            viewport,
            x,
            y,
            jitter_pixels,
            budget,
            input
                .compatibility_projection
                .unwrap_or(LegacyCompatibilityProjectionInput {
                    world_up: camera_input.up,
                    view_scale: 0.72,
                }),
        )
        .ray
        .direction
    } else {
        canonical_screen_sample_query(camera_input, viewport, x, y, jitter_pixels, budget)
            .ray
            .direction
    }
}

pub(crate) fn tile_culling_mask(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    legacy_projection: bool,
) -> Result<Option<TileCullingMask>, PresentationExecError> {
    if legacy_projection {
        return Ok(None);
    }
    let evaluator = DirectQueryEvaluator::new_with_snapshot(ctx, Some(&input.region_snapshot));
    let detail = frame_domain_geometry_detail(&input.frame_domain).unwrap_or(0);
    let bounds = evaluator.region_shape_support_bounds(input.region_capture_name(), detail)?;
    let tile_size = PRESENTATION_TILE_SIZE;
    let tiles_x = viewport.width.div_ceil(tile_size);
    let tiles_y = viewport.height.div_ceil(tile_size);
    let candidate_table = build_observer_local_tile_candidate_artifact(
        ctx,
        input.region_capture_name(),
        detail,
        camera,
        viewport,
    )
    .unwrap_or_else(|| build_tile_candidate_artifact(viewport, &[], false));
    let mut active = vec![false; (tiles_x * tiles_y) as usize];
    let mut saw_coverage = false;
    for (_, min, max) in bounds {
        if mark_projected_bounds_tiles(
            &mut active,
            tiles_x,
            tiles_y,
            tile_size,
            camera,
            viewport,
            min,
            max,
        ) {
            saw_coverage = true;
        }
    }
    if !saw_coverage && candidate_table.enabled {
        for span in &candidate_table.tile_spans {
            if span.candidate_len == 0 {
                continue;
            }
            if let Some(slot) = active.get_mut(span.tile_index as usize) {
                *slot = true;
                saw_coverage = true;
            }
        }
    }
    if !saw_coverage {
        return Ok(None);
    }
    let mut coarse_active_samples = Vec::new();
    let mut skipped_samples = Vec::new();
    for y in 0..viewport.height {
        for x in 0..viewport.width {
            let tile_x = x / tile_size;
            let tile_y = y / tile_size;
            let tile_index = (tile_y * tiles_x + tile_x) as usize;
            let sample_index = (y * viewport.width + x) as usize;
            if active.get(tile_index).copied().unwrap_or(true) {
                coarse_active_samples.push(sample_index);
            } else {
                skipped_samples.push(sample_index);
            }
        }
    }
    let active_tiles = active.iter().filter(|tile| **tile).count() as u32;
    let active_samples = if candidate_table.enabled {
        coarse_active_samples.clone()
    } else {
        coarse_active_samples
    };
    Ok(Some(TileCullingMask {
        active_samples,
        stats: TileCullingStats {
            total_tiles: tiles_x * tiles_y,
            active_tiles,
            skipped_samples: skipped_samples.len() as u32,
        },
        candidate_table,
    }))
}

fn mark_projected_bounds_tiles(
    active: &mut [bool],
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    min: [f32; 3],
    max: [f32; 3],
) -> bool {
    let Ok(range) =
        projected_bounds_tile_range(tiles_x, tiles_y, tile_size, camera, viewport, min, max)
    else {
        return false;
    };
    let Some((min_tile_x, max_tile_x, min_tile_y, max_tile_y)) = range else {
        return true;
    };
    for tile_y in min_tile_y..=max_tile_y {
        for tile_x in min_tile_x..=max_tile_x {
            let index = (tile_y * tiles_x + tile_x) as usize;
            if let Some(slot) = active.get_mut(index) {
                *slot = true;
            }
        }
    }
    true
}

fn projected_bounds_tile_range(
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    min: [f32; 3],
    max: [f32; 3],
) -> Result<Option<(u32, u32, u32, u32)>, ()> {
    let corners = [
        [min[0], min[1], min[2]],
        [min[0], min[1], max[2]],
        [min[0], max[1], min[2]],
        [min[0], max[1], max[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
    ];
    let forward = normalize3(camera.forward, [0.0, 0.0, -1.0]);
    let right = normalize3(cross3(forward, camera.up), [1.0, 0.0, 0.0]);
    let up = normalize3(cross3(right, forward), [0.0, 1.0, 0.0]);
    let aspect = viewport.width.max(1) as f32 / viewport.height.max(1) as f32;
    let vertical_scale = (camera.vertical_fov_degrees.to_radians() * 0.5).tan();
    let horizontal_scale = aspect * vertical_scale;
    let mut projected = Vec::new();
    for corner in corners {
        let rel = [
            corner[0] - camera.position[0],
            corner[1] - camera.position[1],
            corner[2] - camera.position[2],
        ];
        let depth = rel[0] * forward[0] + rel[1] * forward[1] + rel[2] * forward[2];
        if depth <= 0.0 {
            return Ok(Some((
                0,
                tiles_x.saturating_sub(1),
                0,
                tiles_y.saturating_sub(1),
            )));
        }
        let x = (rel[0] * right[0] + rel[1] * right[1] + rel[2] * right[2])
            / (depth * horizontal_scale);
        let y = (rel[0] * up[0] + rel[1] * up[1] + rel[2] * up[2]) / (depth * vertical_scale);
        projected.push([x, y]);
    }
    let min_ndc_x = projected.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_ndc_x = projected
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_ndc_y = projected.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_ndc_y = projected
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    if min_ndc_x > 1.0 || max_ndc_x < -1.0 || min_ndc_y > 1.0 || max_ndc_y < -1.0 {
        return Ok(None);
    }
    let min_px = (((min_ndc_x.clamp(-1.0, 1.0) + 1.0) * 0.5) * viewport.width as f32)
        .floor()
        .max(0.0) as u32;
    let max_px = (((max_ndc_x.clamp(-1.0, 1.0) + 1.0) * 0.5) * viewport.width as f32).ceil() as u32;
    let min_py = (((1.0 - max_ndc_y.clamp(-1.0, 1.0)) * 0.5) * viewport.height as f32)
        .floor()
        .max(0.0) as u32;
    let max_py =
        (((1.0 - min_ndc_y.clamp(-1.0, 1.0)) * 0.5) * viewport.height as f32).ceil() as u32;
    let min_tile_x = (min_px / tile_size).min(tiles_x.saturating_sub(1));
    let max_tile_x = (max_px.div_ceil(tile_size)).min(tiles_x).saturating_sub(1);
    let min_tile_y = (min_py / tile_size).min(tiles_y.saturating_sub(1));
    let max_tile_y = (max_py.div_ceil(tile_size)).min(tiles_y).saturating_sub(1);
    Ok(Some((min_tile_x, max_tile_x, min_tile_y, max_tile_y)))
}

pub fn frame_state_value(
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    frame_index: u32,
    delta_seconds: f32,
) -> KernelValue {
    frame_state_value_with_history(
        camera,
        previous_camera,
        viewport,
        viewport,
        jitter_pixels,
        jitter_pixels,
        frame_index,
        frame_index.saturating_sub(1),
        delta_seconds,
        frame_index == 0,
        SnapshotEpoch::INITIAL,
        SnapshotEpoch::INITIAL,
    )
}

pub fn frame_state_value_with_history(
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    previous_viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    previous_jitter_pixels: [f32; 2],
    frame_index: u32,
    previous_frame_index: u32,
    delta_seconds: f32,
    history_reset: bool,
    current_snapshot_epoch: SnapshotEpoch,
    previous_snapshot_epoch: SnapshotEpoch,
) -> KernelValue {
    frame_state_value_with_temporal_context(
        camera,
        previous_camera,
        viewport,
        previous_viewport,
        jitter_pixels,
        previous_jitter_pixels,
        frame_index,
        previous_frame_index,
        delta_seconds,
        history_reset,
        frame_index,
        previous_frame_index,
        frame_index,
        frame_index as f32 * delta_seconds.max(0.0),
        current_snapshot_epoch,
        previous_snapshot_epoch,
        false,
        0,
        true,
        false,
        false,
    )
}

pub fn frame_state_value_with_temporal_context(
    camera: CanonicalCameraInput,
    previous_camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    previous_viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    previous_jitter_pixels: [f32; 2],
    frame_index: u32,
    previous_frame_index: u32,
    delta_seconds: f32,
    history_reset: bool,
    presentation_frame: u32,
    previous_presentation_frame: u32,
    simulation_tick: u32,
    wall_clock_seconds: f32,
    current_snapshot_epoch: SnapshotEpoch,
    previous_snapshot_epoch: SnapshotEpoch,
    change_summary_present: bool,
    change_class: u32,
    change_compatible: bool,
    change_topology_changed: bool,
    change_identity_changed: bool,
) -> KernelValue {
    let observer_time = observer_time_value(
        presentation_frame,
        previous_presentation_frame,
        simulation_tick,
        wall_clock_seconds,
        delta_seconds,
    );
    let snapshot_transition = snapshot_transition_context_value(
        current_snapshot_epoch,
        previous_snapshot_epoch,
        change_summary_present,
        change_class,
        change_compatible,
        change_topology_changed,
        change_identity_changed,
    );
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("FrameState"),
        fields: vec![
            (
                SmolStr::new("view"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ViewState"),
                    fields: vec![
                        (SmolStr::new("camera"), camera_value(camera)),
                        (
                            SmolStr::new("previous_camera"),
                            camera_value(previous_camera),
                        ),
                        (SmolStr::new("viewport"), viewport_value(viewport)),
                        (
                            SmolStr::new("previous_viewport"),
                            viewport_value(previous_viewport),
                        ),
                        (SmolStr::new("jitter"), KernelValue::Vec2(jitter_pixels)),
                        (
                            SmolStr::new("previous_jitter"),
                            KernelValue::Vec2(previous_jitter_pixels),
                        ),
                    ],
                }),
            ),
            (SmolStr::new("frame_index"), KernelValue::U32(frame_index)),
            (
                SmolStr::new("previous_frame_index"),
                KernelValue::U32(previous_frame_index),
            ),
            (
                SmolStr::new("delta_seconds"),
                KernelValue::F32(delta_seconds),
            ),
            (
                SmolStr::new("history_reset"),
                KernelValue::Bool(history_reset),
            ),
            (SmolStr::new("observer_time"), observer_time),
            (SmolStr::new("snapshot_transition"), snapshot_transition),
        ],
    })
}

fn observer_time_value(
    presentation_frame: u32,
    previous_presentation_frame: u32,
    simulation_tick: u32,
    wall_clock_seconds: f32,
    delta_seconds: f32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ObserverTime"),
        fields: vec![
            (
                SmolStr::new("presentation_frame"),
                presentation_frame_value(presentation_frame),
            ),
            (
                SmolStr::new("previous_presentation_frame"),
                presentation_frame_value(previous_presentation_frame),
            ),
            (
                SmolStr::new("simulation_tick"),
                simulation_tick_value(simulation_tick),
            ),
            (
                SmolStr::new("wall_clock_stamp"),
                wall_clock_stamp_value(wall_clock_seconds),
            ),
            (
                SmolStr::new("delta_seconds"),
                KernelValue::F32(delta_seconds),
            ),
        ],
    })
}

fn presentation_frame_value(index: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PresentationFrame"),
        fields: vec![(SmolStr::new("index"), KernelValue::U32(index))],
    })
}

fn simulation_tick_value(tick: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SimulationTick"),
        fields: vec![(SmolStr::new("tick"), KernelValue::U32(tick))],
    })
}

fn wall_clock_stamp_value(seconds: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WallClockStamp"),
        fields: vec![(SmolStr::new("seconds"), KernelValue::F32(seconds))],
    })
}

fn snapshot_epoch_value(epoch: SnapshotEpoch) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SnapshotEpoch"),
        fields: vec![(
            SmolStr::new("epoch"),
            KernelValue::U32(epoch.portable_projection()),
        )],
    })
}

fn transition_change_summary_value(
    change_class: u32,
    compatible: bool,
    topology_changed: bool,
    identity_changed: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("TransitionChangeSummary"),
        fields: vec![
            (SmolStr::new("change_class"), KernelValue::U32(change_class)),
            (SmolStr::new("compatible"), KernelValue::Bool(compatible)),
            (
                SmolStr::new("topology_changed"),
                KernelValue::Bool(topology_changed),
            ),
            (
                SmolStr::new("identity_changed"),
                KernelValue::Bool(identity_changed),
            ),
        ],
    })
}

fn snapshot_transition_context_value(
    current_snapshot_epoch: SnapshotEpoch,
    previous_snapshot_epoch: SnapshotEpoch,
    has_change_summary: bool,
    change_class: u32,
    compatible: bool,
    topology_changed: bool,
    identity_changed: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SnapshotTransitionContext"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                snapshot_epoch_value(current_snapshot_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                snapshot_epoch_value(previous_snapshot_epoch),
            ),
            (
                SmolStr::new("has_change_summary"),
                KernelValue::Bool(has_change_summary),
            ),
            (
                SmolStr::new("change_summary"),
                transition_change_summary_value(
                    change_class,
                    compatible,
                    topology_changed,
                    identity_changed,
                ),
            ),
        ],
    })
}

pub fn scene_domain_value(
    scene_id: u32,
    geometry_detail: i32,
    material: bool,
    radiance: bool,
    media: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(
                        SmolStr::new("geometry_detail"),
                        KernelValue::I32(geometry_detail),
                    )],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(material))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(radiance)),
                        (SmolStr::new("media"), KernelValue::Bool(media)),
                    ],
                }),
            ),
        ],
    })
}

pub fn light_value(light: CanonicalLightInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Light"),
        fields: vec![
            (SmolStr::new("position"), KernelValue::Vec3(light.position)),
            (
                SmolStr::new("direction"),
                KernelValue::Vec3(light.direction),
            ),
            (
                SmolStr::new("intensity"),
                KernelValue::Vec3(light.intensity),
            ),
            (SmolStr::new("range"), KernelValue::F32(light.range)),
        ],
    })
}

pub fn lighting_inputs_value(lighting: PresentationLightingInputs) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PresentationLightingInputs"),
        fields: vec![
            (SmolStr::new("key_light"), light_value(lighting.key_light)),
            (
                SmolStr::new("fill_direction"),
                KernelValue::Vec3(lighting.fill_direction),
            ),
            (
                SmolStr::new("fill_strength"),
                KernelValue::F32(lighting.fill_strength),
            ),
            (
                SmolStr::new("ambient_color"),
                KernelValue::Vec3(lighting.ambient_color),
            ),
        ],
    })
}

fn execute_batch_contract(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    snapshot: &WorldSnapshotHandle,
    solver_mode: QueryTraceSolverMode,
    contract_id: crate::query_contract::QueryContractId,
    args: &[KernelValue],
) -> Result<(Vec<KernelValue>, BatchQueryExecutionTrace), PresentationExecError> {
    let batch_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(contract_id, backend, None).map_err(|message| {
            PresentationExecError::UnsupportedPlan {
                message: message.to_string(),
            }
        })?,
    );
    let (values, trace) = execute_batch_query_with_solver_mode_with_snapshot_on(
        ctx,
        backend,
        Some(snapshot),
        &batch_plan,
        args,
        solver_mode,
    )?;
    Ok((expect_array(&values)?.to_vec(), trace))
}

fn point_query_value(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointQuery"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn point_direction_query_value(point: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("PointDirectionQuery"),
        fields: vec![
            (SmolStr::new("point"), KernelValue::Vec3(point)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
        ],
    })
}

fn allocate_execution_attachments(
    frame: &FrameContract,
    frame_state: &KernelValue,
    width: u32,
    height: u32,
    current_snapshot: &WorldSnapshotHandle,
    history: Option<&PresentationTemporalHistory>,
) -> Result<AttachmentResourceSet, PresentationExecError> {
    if let Some(history) = history {
        if history_slots_match(frame, frame_state, width, height, current_snapshot, history)? {
            match crate::presentation_exec::allocate_frame_attachment_resources_with_history(
                frame,
                width,
                height,
                Some(&history.attachments),
            ) {
                Ok(resources) => return Ok(resources),
                Err(
                    PresentationResourceError::MissingHistoryAttachment { .. }
                    | PresentationResourceError::HistoryLayoutMismatch { .. },
                ) => {}
                Err(err) => return Err(PresentationExecError::Resource(err)),
            }
        }
    }
    allocate_attachment_resources_without_history(frame, width, height)
        .map_err(PresentationExecError::Resource)
}

fn build_temporal_history(
    plan: &PresentationPlan,
    frame_state: &KernelValue,
    attachments: &AttachmentResourceSet,
    current_snapshot: &WorldSnapshotHandle,
    clipmap: Option<&ViewDistanceClipmapArtifact>,
) -> Result<Option<PresentationTemporalHistory>, PresentationExecError> {
    let Some(temporal) = &plan.frame.temporal else {
        return Ok(None);
    };
    let frame = frame_state_temporal_components(frame_state)?;
    Ok(Some(PresentationTemporalHistory {
        presentation_frame: frame.presentation_frame,
        snapshot: current_snapshot.report(),
        snapshot_handle: current_snapshot.clone(),
        attachments: attachments.clone(),
        slots: temporal
            .history_slots
            .iter()
            .map(|slot| {
                let attachment = attachments
                    .attachment(slot.attachment.as_str())
                    .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                        message: format!(
                            "temporal history slot '{}' references missing attachment '{}'",
                            slot.slot, slot.attachment
                        ),
                    })?;
                Ok::<PresentationTemporalHistorySlot, PresentationExecError>(
                    PresentationTemporalHistorySlot {
                        slot: slot.slot,
                        attachment: slot.attachment.clone(),
                        compatibility: slot.compatibility.clone(),
                        reuse_key: slot.reuse_key(
                            current_snapshot,
                            attachment.layout.compatibility_signature(),
                        ),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
        clipmap: clipmap.cloned(),
    }))
}

fn history_slots_match(
    frame: &FrameContract,
    frame_state: &KernelValue,
    width: u32,
    height: u32,
    current_snapshot: &WorldSnapshotHandle,
    history: &PresentationTemporalHistory,
) -> Result<bool, PresentationExecError> {
    let Some(temporal) = &frame.temporal else {
        return Ok(false);
    };
    let components = frame_state_temporal_components(frame_state)?;
    let change_budget = crate::presentation_exec::temporal::frame_change_budget_class(&components);
    if !temporal.transition_compatibility.allows(change_budget) {
        return Ok(false);
    }
    if crate::presentation_exec::temporal::required_temporal_evidence_failure(temporal, &components)
        .is_some()
    {
        return Ok(false);
    }
    let mut store = ArtifactStore::default();
    for slot in &history.slots {
        let Some(attachment) = history.attachments.attachment(slot.attachment.as_str()) else {
            return Ok(false);
        };
        let Some(current_attachment) = frame.attachment(slot.attachment.as_str()) else {
            return Ok(false);
        };
        let Some(current_slot) = temporal
            .history_slots
            .iter()
            .find(|candidate| candidate.slot == slot.slot)
        else {
            return Ok(false);
        };
        let contract =
            presentation_history_artifact_contract(temporal, current_slot, current_attachment);
        store.insert(StoredArtifact {
            contract,
            metadata: ArtifactInstanceMetadata {
                snapshot: history.snapshot_handle.clone(),
                reuse_key: slot.reuse_key.clone(),
                policy_digest: slot.reuse_key.policy_digest,
                presentation_frame: Some(history.presentation_frame),
                layout_signature: Some(attachment.layout.compatibility_signature()),
                history_compatibility_hash: Some(slot.compatibility.compatibility_hash()),
                evidence_summary: SemanticEvidenceSummary::contract_bound(),
            },
            payload: (),
        });
    }
    for slot in &temporal.history_slots {
        let Some(attachment) = frame
            .outputs
            .iter()
            .find(|candidate| candidate.name == slot.attachment)
        else {
            return Ok(false);
        };
        let layout = frame_attachment_layout(frame, attachment, width, height)
            .map_err(PresentationExecError::Resource)?;
        let contract = presentation_history_artifact_contract(temporal, slot, attachment);
        let reuse_key = slot.reuse_key(&current_snapshot, layout.compatibility_signature());
        let (artifact, _) = store.lookup(&ArtifactLookupRequest {
            contract,
            reuse_key: Some(reuse_key),
            current_snapshot: current_snapshot.clone(),
            previous_snapshot_epoch: Some(components.previous_snapshot_epoch),
            change_class: Some(change_budget),
            policy_digest: Some(slot.compatibility.compatibility_hash()),
            presentation_frame: Some(components.presentation_frame),
            layout_signature: Some(layout.compatibility_signature()),
            history_compatibility_hash: Some(slot.compatibility.compatibility_hash()),
            evidence_summary: Some(SemanticEvidenceSummary::contract_bound()),
        });
        if artifact.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn presentation_history_artifact_contract(
    temporal: &crate::presentation_contract::TemporalContract,
    slot: &crate::presentation_contract::TemporalHistorySlotContract,
    attachment: &FrameAttachmentContract,
) -> SemanticArtifactContract {
    SemanticArtifactContract {
        id: SmolStr::new(format!("artifact.{}", attachment.name)),
        kind: SemanticArtifactKind::PresentationHistory,
        logical_schema: ArtifactLogicalSchema {
            namespace: SmolStr::new("presentation"),
            name: SmolStr::new("history-slot"),
            fields: vec![
                ArtifactLogicalField::new("attachment", attachment.name.clone()),
                ArtifactLogicalField::new("kind", format!("{:?}", attachment.kind)),
                ArtifactLogicalField::new(
                    "element_schema",
                    format!("{:?}", attachment.element_schema),
                ),
                ArtifactLogicalField::new("history_slot", slot.slot.to_string()),
                ArtifactLogicalField::new("history_role", format!("{:?}", slot.role)),
                ArtifactLogicalField::new(
                    "history_compatibility_hash",
                    slot.compatibility.compatibility_hash().to_string(),
                ),
            ],
        },
        compatibility: ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::PreviousSnapshotEpoch,
            transition: ArtifactTransitionRelation {
                compatibility: Some(temporal.transition_compatibility),
                requires_previous_snapshot: true,
            },
            policy: ArtifactPolicyCompatibility {
                mode: crate::artifact_key::ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: SemanticEvidenceSummary::contract_bound().origin,
                scope: SemanticEvidenceSummary::contract_bound().scope,
            },
        },
        acceleration: None,
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::LayoutSignatureMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::HistoryCompatibilityMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::CompatibleChange(
                temporal.transition_compatibility,
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::MaxPresentationFrameAge(
                u64::from(slot.max_age_frames),
            )),
            if temporal.requires_snapshot_lineage_match {
                ArtifactValidityRule::predicate(
                    ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
                )
            } else {
                ArtifactValidityRule::Always
            },
        ]),
        producer: SmolStr::new("temporal_resolve"),
        consumer: SmolStr::new("presentation.frame"),
        deterministic: true,
        version: crate::presentation_contract::PRESENTATION_CONTRACT_VERSION,
        transition: None,
        evidence_summary: SemanticEvidenceSummary::contract_bound(),
    }
}

fn presentation_metrics(
    hits: &[KernelValue],
    query_trace: &BatchQueryExecutionTrace,
    solver_summary: Option<RaySolverDiagnosticSummary>,
    continuation_diagnostics: Vec<String>,
    gpu_runtime: GpuRuntimeMetrics,
) -> PresentationMetrics {
    let mut distribution = PresentationRayStepDistribution {
        zero: 0,
        short: 0,
        medium: 0,
        long: 0,
        extreme: 0,
    };
    let mut trace_steps_total = 0;
    for hit in hits {
        let steps = hit_steps(hit).unwrap_or_default();
        trace_steps_total += steps;
        match steps {
            0 => distribution.zero += 1,
            1..=8 => distribution.short += 1,
            9..=32 => distribution.medium += 1,
            33..=64 => distribution.long += 1,
            _ => distribution.extreme += 1,
        }
    }
    let observability = &query_trace.observability;
    let hit_count = hits
        .iter()
        .filter(|hit| hit_flag(hit).unwrap_or(false))
        .count() as u32;
    let miss_count = hits.len() as u32 - hit_count;
    PresentationMetrics {
        sample_count: hits.len() as u32,
        hit_count,
        miss_count,
        candidate_count: observability.candidate_count,
        candidates_before_pruning: observability.candidates_before_pruning,
        candidates_after_pruning: observability.candidates_after_pruning,
        candidate_reduction: observability
            .candidates_before_pruning
            .saturating_sub(observability.candidates_after_pruning),
        trace_steps_total,
        trace_steps_max: observability.trace_steps_max,
        ray_step_distribution: distribution,
        dispatch_items: observability.dispatch_items,
        dispatch_workgroups: [
            observability.dispatch_workgroups_x,
            observability.dispatch_workgroups_y,
            observability.dispatch_workgroups_z,
        ],
        solver_summary,
        solver_methods: observability.solver_methods.clone(),
        dense_fallback_count: observability.solver_dense_fallback_rays
            + observability.solver_generated_dense_fallback_rays,
        continuation_available_count: observability.solver_continuation_available,
        continuation_consumed_count: observability.solver_continuation_consumed,
        continuation_rejected_count: observability.solver_continuation_rejected,
        continuation_unavailable_count: observability.solver_continuation_unavailable,
        continuation_diagnostics,
        acceleration_node_visits: observability.acceleration_node_visits,
        union_cluster_visits: observability.union_cluster_visits,
        ray_support_interval_rejections: observability.ray_support_interval_rejections,
        ray_support_entry_jumps: observability.ray_support_entry_jumps,
        repeat_cell_skips: observability.repeat_cell_skips,
        cache_brick_visits: observability.cache_brick_visits,
        cache_brick_hits: observability.cache_brick_hits,
        cache_brick_misses: observability.cache_brick_misses,
        cache_interval_advances: observability.cache_interval_advances,
        accepted_relaxed_steps: observability.accepted_relaxed_steps,
        rejected_relaxed_steps: observability.rejected_relaxed_steps,
        solver_relaxed_attempts: observability.solver_relaxed_attempts,
        solver_relaxed_no_root_advances: observability.solver_relaxed_no_root_advances,
        solver_relaxed_brackets: observability.solver_relaxed_brackets,
        solver_relaxed_unresolved: observability.solver_relaxed_unresolved,
        solver_interval_attempts: observability.solver_interval_attempts,
        solver_interval_no_root_advances: observability.solver_interval_no_root_advances,
        solver_interval_brackets: observability.solver_interval_brackets,
        solver_interval_unresolved: observability.solver_interval_unresolved,
        solver_refinement_attempts: observability.solver_refinement_attempts,
        solver_refinement_failures: observability.solver_refinement_failures,
        solver_repeat_attempts: observability.solver_repeat_attempts,
        solver_repeat_supported: observability.solver_repeat_supported,
        solver_repeat_inapplicable: observability.solver_repeat_inapplicable,
        solver_repeat_unsupported: observability.solver_repeat_unsupported,
        solver_repeat_unsupported_form: observability.solver_repeat_unsupported_form,
        solver_repeat_unsupported_bounds: observability.solver_repeat_unsupported_bounds,
        solver_repeat_cells_enumerated: observability.solver_repeat_cells_enumerated,
        analytic_transformed_hits: observability.analytic_transformed_hits,
        interval_subdivisions: observability.interval_subdivisions,
        interval_proof_successes: observability.interval_proof_successes,
        observer_continuation_seed_hits: observability.observer_continuation_seed_hits,
        field_samples: observability.field_samples,
        gpu_runtime,
    }
}

fn runtime_primary_solver_summary(
    solver_context: Option<&(RaySolverPlan, Vec<ArtifactContract>)>,
    continuation_counts: &temporal::ContinuationCounts,
) -> Option<RaySolverDiagnosticSummary> {
    let (solver_plan, artifact_contracts) = solver_context?;
    Some(
        solver_plan
            .with_artifact_reuse_resolution(presentation_artifact_reuse_resolution(
                artifact_contracts,
            ))
            .with_continuation_resolution(presentation_continuation_resolution(continuation_counts))
            .diagnostic_summary(),
    )
}

fn presentation_artifact_reuse_resolution(
    artifact_contracts: &[ArtifactContract],
) -> RaySolverArtifactReuseResolution {
    let compatible_artifacts = artifact_contracts
        .iter()
        .filter_map(|artifact| match artifact.schema {
            ArtifactSchema::SupportSummary { .. } => Some("support-summary"),
            ArtifactSchema::CaptureCache { .. } => Some("capture-cache"),
            ArtifactSchema::CullingTable { .. } => Some("culling-table"),
            _ => None,
        })
        .collect::<Vec<_>>();
    if compatible_artifacts.is_empty() {
        return RaySolverArtifactReuseResolution {
            disposition: RaySolverIntentDisposition::Rejected,
            reasons: vec![SmolStr::new(
                "primary visibility plan exposes no compatible solver artifacts",
            )],
        };
    }
    RaySolverArtifactReuseResolution {
        disposition: RaySolverIntentDisposition::Used,
        reasons: vec![
            SmolStr::new(format!(
                "primary visibility reused compatible artifacts: {}",
                compatible_artifacts.join(", ")
            )),
            SmolStr::new(
                "artifact compatibility stays governed by the primary visibility query plan",
            ),
        ],
    }
}

fn presentation_continuation_resolution(
    continuation_counts: &temporal::ContinuationCounts,
) -> RaySolverContinuationResolution {
    let disposition = if continuation_counts.consumed > 0 {
        RaySolverIntentDisposition::Used
    } else if continuation_counts.rejected > 0 {
        RaySolverIntentDisposition::Rejected
    } else {
        RaySolverIntentDisposition::Unavailable
    };
    let mut reasons = continuation_counts
        .diagnostics
        .iter()
        .map(|entry| SmolStr::new(entry.as_str()))
        .collect::<Vec<_>>();
    if reasons.is_empty() {
        reasons.push(SmolStr::new(format!(
            "continuation counts available={} consumed={} rejected={} unavailable={}",
            continuation_counts.available,
            continuation_counts.consumed,
            continuation_counts.rejected,
            continuation_counts.unavailable
        )));
    }
    RaySolverContinuationResolution {
        disposition,
        reasons,
    }
}

pub(crate) fn build_frame_cost_report(
    frame_domain: &KernelValue,
    execution_policy: PresentationExecutionPolicy,
    backend: DispatchBackend,
    width: u32,
    height: u32,
    quality_contract: &RealtimeQualityContract,
    quality: &RealtimeQualityState,
    metrics: &PresentationMetrics,
    tile_cull: TileCullingStats,
    tile_candidate_stats: TileCandidateStats,
    packet_scheduling_active: bool,
    selected_workgroup_size: u32,
    surface_resolve_count: u32,
    participant_resolve_count: u32,
    attachment_bytes: Vec<PresentationAttachmentBytes>,
    passes: Vec<PassRuntimeStats>,
    framegraph_exceptions: Vec<String>,
    mut active_acceleration_artifacts: Vec<String>,
) -> PresentationFrameCostReport {
    let semantic_domain = semantic_domain_report(frame_domain);
    let execution_policy = render_execution_policy_report(
        &execution_policy,
        backend,
        &quality_contract.degradation_order,
    );
    let legal_degradations = quality_contract
        .degradation_order
        .iter()
        .map(|step| cost::quality_degradation_name(*step).to_string())
        .collect::<Vec<_>>();
    let hit_compaction_enabled = quality.hit_compaction_enabled;
    let active_degradations_empty = quality.active_degradations.is_empty();
    let quality = quality_report(quality, width, height);
    let primary_hit_rate = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.hit_count as f32 / metrics.sample_count as f32
    };
    let average_trace_steps = if metrics.sample_count == 0 {
        0.0
    } else {
        metrics.trace_steps_total as f32 / metrics.sample_count as f32
    };
    let support_prune_effectiveness = if metrics.candidates_before_pruning == 0 {
        0.0
    } else {
        metrics.candidate_reduction as f32 / metrics.candidates_before_pruning as f32
    };
    let tile_cull_efficiency = if tile_cull.total_tiles == 0 {
        0.0
    } else {
        1.0 - (tile_cull.active_tiles as f32 / tile_cull.total_tiles as f32)
    };
    let tile_candidate_reduction = tile_candidate_stats
        .total_samples
        .saturating_sub(tile_candidate_stats.active_samples);
    let tile_candidate_effectiveness = if tile_candidate_stats.total_samples == 0 {
        0.0
    } else {
        tile_candidate_reduction as f32 / tile_candidate_stats.total_samples as f32
    };
    let packet_capacity = tile_candidate_stats
        .packet_count
        .saturating_mul(tile_candidate_stats.packet_size);
    let packet_compaction_ratio = if packet_capacity == 0 {
        0.0
    } else {
        tile_candidate_stats.active_samples as f32 / packet_capacity as f32
    };
    let history_reuse_total = metrics.continuation_available_count
        + metrics.continuation_consumed_count
        + metrics.continuation_rejected_count
        + metrics.continuation_unavailable_count;
    let history_reuse_rate = if history_reuse_total == 0 {
        0.0
    } else {
        metrics.continuation_consumed_count as f32 / history_reuse_total as f32
    };
    if metrics.candidate_reduction > 0 {
        active_acceleration_artifacts.push("support_pruning".to_string());
    }
    if hit_compaction_enabled {
        active_acceleration_artifacts.push("hit_compaction".to_string());
    }
    if tile_cull.total_tiles > 0 && tile_cull.active_tiles < tile_cull.total_tiles {
        active_acceleration_artifacts.push("view_tile_culling".to_string());
    }
    if packet_scheduling_active {
        active_acceleration_artifacts.push("packet_scheduling".to_string());
    }
    active_acceleration_artifacts.extend(
        metrics
            .solver_methods
            .iter()
            .map(|method| ray_solver_method_name(*method).to_string()),
    );
    let mut deduped_artifacts = BTreeMap::new();
    for artifact in active_acceleration_artifacts {
        deduped_artifacts
            .entry(artifact.clone())
            .or_insert(artifact);
    }
    let cpu_time_total_micros = passes.iter().map(|pass| pass.elapsed_micros).sum::<u128>();
    let execution_bound =
        classify_execution_bound(cpu_time_total_micros, &metrics.gpu_runtime, passes.len())
            .to_string();
    let passes = passes
        .into_iter()
        .map(|pass| PresentationPassCost {
            pass_id: pass.pass_id,
            pass_kind: pass.pass_kind,
            work_items: pass.work_items,
            elapsed_micros: pass.elapsed_micros,
            gpu_elapsed_micros: pass.gpu_elapsed_micros,
            dispatch_count: pass.dispatch_count,
            attachment_bytes_read: pass.attachment_bytes_read,
            attachment_bytes_written: pass.attachment_bytes_written,
            notes: pass.notes,
        })
        .collect::<Vec<_>>();
    let bottleneck_pass = passes
        .iter()
        .max_by_key(|pass| (pass.elapsed_micros, pass.work_items))
        .map(|pass| pass.pass_id.clone());
    let mut performance_gain_sources = Vec::new();
    if support_prune_effectiveness > 0.0 {
        performance_gain_sources.push("support_pruning".to_string());
    }
    if tile_cull_efficiency > 0.0 {
        performance_gain_sources.push("tile_culling".to_string());
    }
    if tile_candidate_stats.packet_count > 0
        && tile_candidate_stats.total_samples > tile_candidate_stats.active_samples
    {
        performance_gain_sources.push("tile_candidate_table".to_string());
    }
    if packet_scheduling_active {
        performance_gain_sources.push("packet_scheduling".to_string());
    }
    if !active_degradations_empty {
        performance_gain_sources.push("quality_degradation_active".to_string());
    }
    if performance_gain_sources.is_empty() {
        performance_gain_sources.push("backend_speed".to_string());
    }
    PresentationFrameCostReport {
        semantic_domain,
        execution_policy,
        legal_degradations,
        output_width: width,
        output_height: height,
        internal_width: quality.internal_width,
        internal_height: quality.internal_height,
        quality,
        primary_hit_rate,
        average_trace_steps,
        max_trace_steps: metrics.trace_steps_max,
        candidate_count_before_pruning: metrics.candidates_before_pruning,
        candidate_count_after_pruning: metrics.candidates_after_pruning,
        support_prune_effectiveness,
        tile_cull_total_tiles: tile_cull.total_tiles,
        tile_cull_active_tiles: tile_cull.active_tiles,
        tile_cull_efficiency,
        tile_candidate_total_samples: tile_candidate_stats.total_samples,
        tile_candidate_active_samples: tile_candidate_stats.active_samples,
        tile_candidate_reduction,
        tile_candidate_effectiveness,
        tile_candidate_packet_count: tile_candidate_stats.packet_count,
        tile_candidate_packet_size: tile_candidate_stats.packet_size,
        packet_compaction_ratio,
        packet_scheduling_active,
        selected_workgroup_size,
        surface_resolve_count,
        participant_resolve_count,
        history_reuse_rate,
        continuation_diagnostics: metrics.continuation_diagnostics.clone(),
        acceleration_node_visits: metrics.acceleration_node_visits,
        union_cluster_visits: metrics.union_cluster_visits,
        ray_support_interval_rejections: metrics.ray_support_interval_rejections,
        ray_support_entry_jumps: metrics.ray_support_entry_jumps,
        repeat_cell_skips: metrics.repeat_cell_skips,
        cache_brick_visits: metrics.cache_brick_visits,
        cache_brick_hits: metrics.cache_brick_hits,
        cache_brick_misses: metrics.cache_brick_misses,
        cache_interval_advances: metrics.cache_interval_advances,
        accepted_relaxed_steps: metrics.accepted_relaxed_steps,
        rejected_relaxed_steps: metrics.rejected_relaxed_steps,
        solver_relaxed_attempts: metrics.solver_relaxed_attempts,
        solver_relaxed_no_root_advances: metrics.solver_relaxed_no_root_advances,
        solver_relaxed_brackets: metrics.solver_relaxed_brackets,
        solver_relaxed_unresolved: metrics.solver_relaxed_unresolved,
        solver_interval_attempts: metrics.solver_interval_attempts,
        solver_interval_no_root_advances: metrics.solver_interval_no_root_advances,
        solver_interval_brackets: metrics.solver_interval_brackets,
        solver_interval_unresolved: metrics.solver_interval_unresolved,
        solver_refinement_attempts: metrics.solver_refinement_attempts,
        solver_refinement_failures: metrics.solver_refinement_failures,
        solver_repeat_attempts: metrics.solver_repeat_attempts,
        solver_repeat_supported: metrics.solver_repeat_supported,
        solver_repeat_inapplicable: metrics.solver_repeat_inapplicable,
        solver_repeat_unsupported: metrics.solver_repeat_unsupported,
        solver_repeat_unsupported_form: metrics.solver_repeat_unsupported_form,
        solver_repeat_unsupported_bounds: metrics.solver_repeat_unsupported_bounds,
        solver_repeat_cells_enumerated: metrics.solver_repeat_cells_enumerated,
        analytic_transformed_hits: metrics.analytic_transformed_hits,
        interval_subdivisions: metrics.interval_subdivisions,
        interval_proof_successes: metrics.interval_proof_successes,
        observer_continuation_seed_hits: metrics.observer_continuation_seed_hits,
        field_samples: metrics.field_samples,
        cpu_time_total_micros,
        execution_bound,
        gpu_runtime: metrics.gpu_runtime.clone(),
        attachment_bytes,
        passes,
        framegraph_exceptions,
        active_acceleration_artifacts: deduped_artifacts.into_values().collect(),
        bottleneck_pass,
        performance_gain_sources,
    }
}

fn semantic_domain_report(frame_domain: &KernelValue) -> String {
    let Ok(frame_domain) = expect_struct(frame_domain, "SceneDomain") else {
        return "unavailable".to_string();
    };
    let scene_id = field(frame_domain, "scene_id")
        .and_then(expect_u32)
        .unwrap_or_default();
    let geometry_detail = frame_domain_geometry_detail(&KernelValue::Struct(frame_domain.clone()))
        .unwrap_or_default();
    let material = frame_domain_flag(frame_domain, "surface", "SurfaceDomainContract", "material");
    let radiance = frame_domain_flag(
        frame_domain,
        "participants",
        "ParticipantDomainContract",
        "radiance",
    );
    let media = frame_domain_flag(
        frame_domain,
        "participants",
        "ParticipantDomainContract",
        "media",
    );
    render_semantic_domain_report(scene_id, geometry_detail, material, radiance, media)
}

fn frame_domain_flag(
    frame_domain: &KernelStructValue,
    contract_field: &str,
    contract_name: &str,
    flag: &str,
) -> bool {
    let Ok(contract) =
        field(frame_domain, contract_field).and_then(|value| expect_struct(value, contract_name))
    else {
        return false;
    };
    field(contract, flag).and_then(expect_bool).unwrap_or(false)
}

fn materialize_primary_visibility_attachments(
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &PrimaryVisibilityPassContract,
) -> Result<(), PresentationExecError> {
    for (index, hit) in hits.iter().enumerate() {
        let attachment_hit = normalize_hit_for_attachment(hit)?;
        if let Some(primary_hit) =
            attachments.attachment_mut(contract.primary_hit_attachment.as_str())
        {
            primary_hit.encode(index, &attachment_hit)?;
        }
        if let Some(depth_attachment) = &contract.depth_attachment
            && let Some(depth) = attachments.attachment_mut(depth_attachment.as_str())
        {
            depth.encode(
                index,
                &KernelValue::F32(hit_depth(&attachment_hit).unwrap_or(f32::INFINITY)),
            )?;
        }
        if let Some(world_normal_attachment) = &contract.world_normal_attachment
            && let Some(world_normal) = attachments.attachment_mut(world_normal_attachment.as_str())
        {
            world_normal.encode(
                index,
                &KernelValue::Vec3(hit_world_normal(&attachment_hit).unwrap_or([0.0, 0.0, 0.0])),
            )?;
        }
    }
    Ok(())
}

fn generate_screen_samples(
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
    camera: CanonicalCameraInput,
    viewport: CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    ray_budget: CanonicalRayBudget,
) -> Vec<KernelValue> {
    let mut samples = Vec::with_capacity(viewport.width.saturating_mul(viewport.height) as usize);
    for y in 0..viewport.height {
        for x in 0..viewport.width {
            let sample = if plan.view.compatibility_projection.legacy_path_active {
                legacy_preview_screen_sample_query(
                    camera,
                    viewport,
                    x,
                    y,
                    jitter_pixels,
                    ray_budget,
                    input
                        .compatibility_projection
                        .unwrap_or(LegacyCompatibilityProjectionInput {
                            world_up: camera.up,
                            view_scale: 0.72,
                        }),
                )
            } else {
                canonical_screen_sample_query(camera, viewport, x, y, jitter_pixels, ray_budget)
            };
            samples.push(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("ScreenSampleQuery"),
                fields: vec![
                    (SmolStr::new("pixel"), KernelValue::Vec2(sample.pixel)),
                    (SmolStr::new("uv"), KernelValue::Vec2(sample.uv)),
                    (
                        SmolStr::new("ray"),
                        KernelValue::Struct(KernelStructValue {
                            name: SmolStr::new("RayQuery"),
                            fields: vec![
                                (SmolStr::new("origin"), KernelValue::Vec3(sample.ray.origin)),
                                (
                                    SmolStr::new("direction"),
                                    KernelValue::Vec3(sample.ray.direction),
                                ),
                                (
                                    SmolStr::new("max_distance"),
                                    KernelValue::F32(sample.ray.max_distance),
                                ),
                                (
                                    SmolStr::new("min_step"),
                                    KernelValue::F32(sample.ray.min_step),
                                ),
                                (
                                    SmolStr::new("hit_epsilon"),
                                    KernelValue::F32(sample.ray.hit_epsilon),
                                ),
                                (
                                    SmolStr::new("max_steps"),
                                    KernelValue::I32(sample.ray.max_steps),
                                ),
                            ],
                        }),
                    ),
                ],
            }));
        }
    }
    samples
}

pub(crate) fn internal_resolution_divisor(scale: f32) -> u32 {
    if scale <= 0.25 + f32::EPSILON {
        4
    } else if scale <= 0.5 + f32::EPSILON {
        2
    } else {
        1
    }
}

pub(crate) fn internal_resolution_viewport(
    viewport: CanonicalViewportInput,
    quality: &RealtimeQualityState,
) -> CanonicalViewportInput {
    let divisor = internal_resolution_divisor(quality.internal_resolution_scale);
    CanonicalViewportInput {
        width: viewport.width.div_ceil(divisor),
        height: viewport.height.div_ceil(divisor),
    }
}

pub(crate) fn expand_internal_hits(
    internal_hits: &[KernelValue],
    output_viewport: CanonicalViewportInput,
    internal_viewport: CanonicalViewportInput,
) -> Vec<KernelValue> {
    if output_viewport == internal_viewport {
        return internal_hits.to_vec();
    }
    let mut hits =
        Vec::with_capacity(output_viewport.width.saturating_mul(output_viewport.height) as usize);
    for index in 0..output_viewport.width.saturating_mul(output_viewport.height) as usize {
        let x = index as u32 % output_viewport.width.max(1);
        let y = index as u32 / output_viewport.width.max(1);
        let internal_x = (x.saturating_mul(internal_viewport.width)) / output_viewport.width.max(1);
        let internal_y =
            (y.saturating_mul(internal_viewport.height)) / output_viewport.height.max(1);
        let internal_index = (internal_y * internal_viewport.width + internal_x) as usize;
        hits.push(
            internal_hits
                .get(internal_index)
                .cloned()
                .unwrap_or_else(primary_hit_miss_value),
        );
    }
    hits
}

pub(crate) fn attachment_hit_work_items(
    attachments: &AttachmentResourceSet,
    attachment_name: &str,
    hits: &[KernelValue],
    compact_hits: bool,
) -> Result<Vec<(usize, KernelValue)>, PresentationExecError> {
    let Some(attachment) = attachments.attachment(attachment_name) else {
        return Ok(Vec::new());
    };
    if !compact_hits
        && attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return Ok(hits.iter().cloned().enumerate().collect());
    }
    let mut deduped = BTreeMap::new();
    for (index, hit) in hits.iter().enumerate() {
        if compact_hits && !hit_flag(hit)? {
            continue;
        }
        let target_index = attachment_target_index(attachments, attachment, index);
        deduped.entry(target_index).or_insert_with(|| hit.clone());
    }
    Ok(deduped.into_iter().collect())
}

fn screen_sample_ray(sample: &KernelValue) -> Result<KernelValue, PresentationExecError> {
    let sample = expect_struct(sample, "ScreenSampleQuery")?;
    Ok(field(sample, "ray")?.clone())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FrameStateTemporalComponents {
    pub camera: CanonicalCameraInput,
    pub previous_camera: CanonicalCameraInput,
    pub viewport: CanonicalViewportInput,
    pub previous_viewport: CanonicalViewportInput,
    pub jitter: [f32; 2],
    pub previous_jitter: [f32; 2],
    pub frame_index: u32,
    pub previous_frame_index: u32,
    pub delta_seconds: f32,
    pub history_reset: bool,
    pub presentation_frame: u32,
    pub previous_presentation_frame: u32,
    pub simulation_tick: u32,
    pub wall_clock_seconds: f32,
    pub current_snapshot_epoch: SnapshotEpoch,
    pub previous_snapshot_epoch: SnapshotEpoch,
    pub change_summary_present: bool,
    pub change_class: u32,
    pub change_compatible: bool,
    pub change_topology_changed: bool,
    pub change_identity_changed: bool,
}

fn frame_state_components(
    frame_state: &KernelValue,
) -> Result<(CanonicalCameraInput, CanonicalViewportInput, [f32; 2]), PresentationExecError> {
    let components = frame_state_temporal_components(frame_state)?;
    Ok((components.camera, components.viewport, components.jitter))
}

pub(super) fn frame_state_temporal_components(
    frame_state: &KernelValue,
) -> Result<FrameStateTemporalComponents, PresentationExecError> {
    let frame = expect_struct(frame_state, "FrameState")?;
    let view = expect_struct(field(frame, "view")?, "ViewState")?;
    let camera = expect_struct(field(view, "camera")?, "Camera")?;
    let previous_camera = expect_struct(field(view, "previous_camera")?, "Camera")?;
    let viewport = expect_struct(field(view, "viewport")?, "Viewport")?;
    let previous_viewport = expect_struct(field(view, "previous_viewport")?, "Viewport")?;
    let jitter = expect_vec2(field(view, "jitter")?)?;
    let previous_jitter = expect_vec2(field(view, "previous_jitter")?)?;
    let frame_index = expect_u32(field(frame, "frame_index")?)?;
    let previous_frame_index = expect_u32(field(frame, "previous_frame_index")?)?;
    let delta_seconds = expect_f32(field(frame, "delta_seconds")?)?;
    let history_reset = expect_bool(field(frame, "history_reset")?)?;
    let observer_time = expect_struct(field(frame, "observer_time")?, "ObserverTime")?;
    let presentation_frame = expect_struct(
        field(observer_time, "presentation_frame")?,
        "PresentationFrame",
    )?;
    let previous_presentation_frame = expect_struct(
        field(observer_time, "previous_presentation_frame")?,
        "PresentationFrame",
    )?;
    let simulation_tick =
        expect_struct(field(observer_time, "simulation_tick")?, "SimulationTick")?;
    let wall_clock_stamp =
        expect_struct(field(observer_time, "wall_clock_stamp")?, "WallClockStamp")?;
    let snapshot_transition = expect_struct(
        field(frame, "snapshot_transition")?,
        "SnapshotTransitionContext",
    )?;
    let current_snapshot_epoch = expect_struct(
        field(snapshot_transition, "current_snapshot_epoch")?,
        "SnapshotEpoch",
    )?;
    let previous_snapshot_epoch = expect_struct(
        field(snapshot_transition, "previous_snapshot_epoch")?,
        "SnapshotEpoch",
    )?;
    let change_summary = expect_struct(
        field(snapshot_transition, "change_summary")?,
        "TransitionChangeSummary",
    )?;
    Ok(FrameStateTemporalComponents {
        camera: CanonicalCameraInput {
            position: expect_vec3(field(camera, "position")?)?,
            forward: expect_vec3(field(camera, "forward")?)?,
            up: expect_vec3(field(camera, "up")?)?,
            vertical_fov_degrees: expect_f32(field(camera, "vertical_fov_degrees")?)?,
        },
        previous_camera: CanonicalCameraInput {
            position: expect_vec3(field(previous_camera, "position")?)?,
            forward: expect_vec3(field(previous_camera, "forward")?)?,
            up: expect_vec3(field(previous_camera, "up")?)?,
            vertical_fov_degrees: expect_f32(field(previous_camera, "vertical_fov_degrees")?)?,
        },
        viewport: CanonicalViewportInput {
            width: expect_u32(field(viewport, "width")?)?,
            height: expect_u32(field(viewport, "height")?)?,
        },
        previous_viewport: CanonicalViewportInput {
            width: expect_u32(field(previous_viewport, "width")?)?,
            height: expect_u32(field(previous_viewport, "height")?)?,
        },
        jitter,
        previous_jitter,
        frame_index,
        previous_frame_index,
        delta_seconds,
        history_reset,
        presentation_frame: expect_u32(field(presentation_frame, "index")?)?,
        previous_presentation_frame: expect_u32(field(previous_presentation_frame, "index")?)?,
        simulation_tick: expect_u32(field(simulation_tick, "tick")?)?,
        wall_clock_seconds: expect_f32(field(wall_clock_stamp, "seconds")?)?,
        current_snapshot_epoch: SnapshotEpoch(u64::from(expect_u32(field(
            current_snapshot_epoch,
            "epoch",
        )?)?)),
        previous_snapshot_epoch: SnapshotEpoch(u64::from(expect_u32(field(
            previous_snapshot_epoch,
            "epoch",
        )?)?)),
        change_summary_present: expect_bool(field(snapshot_transition, "has_change_summary")?)?,
        change_class: expect_u32(field(change_summary, "change_class")?)?,
        change_compatible: expect_bool(field(change_summary, "compatible")?)?,
        change_topology_changed: expect_bool(field(change_summary, "topology_changed")?)?,
        change_identity_changed: expect_bool(field(change_summary, "identity_changed")?)?,
    })
}

fn camera_value(camera: CanonicalCameraInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Camera"),
        fields: vec![
            (SmolStr::new("position"), KernelValue::Vec3(camera.position)),
            (SmolStr::new("forward"), KernelValue::Vec3(camera.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(camera.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(camera.vertical_fov_degrees),
            ),
        ],
    })
}

fn viewport_value(viewport: CanonicalViewportInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Viewport"),
        fields: vec![
            (SmolStr::new("width"), KernelValue::U32(viewport.width)),
            (SmolStr::new("height"), KernelValue::U32(viewport.height)),
        ],
    })
}

fn frame_domain_geometry_detail(frame_domain: &KernelValue) -> Result<i32, PresentationExecError> {
    let frame_domain = expect_struct(frame_domain, "SceneDomain")?;
    let spatial = expect_struct(field(frame_domain, "spatial")?, "SpatialDomainContract")?;
    match field(spatial, "geometry_detail")? {
        KernelValue::I32(value) => Ok(*value),
        other => Err(type_mismatch("I32", other)),
    }
}

fn expect_array(value: &KernelValue) -> Result<&[KernelValue], PresentationExecError> {
    match value {
        KernelValue::Array(values) => Ok(values),
        other => Err(type_mismatch("Array", other)),
    }
}

fn expect_struct<'a>(
    value: &'a KernelValue,
    expected: &str,
) -> Result<&'a KernelStructValue, PresentationExecError> {
    match value {
        KernelValue::Struct(struct_value) if struct_value.name == expected => Ok(struct_value),
        KernelValue::Struct(struct_value) => Err(PresentationExecError::TypeMismatch {
            expected: expected.to_string(),
            found: struct_value.name.to_string(),
        }),
        other => Err(type_mismatch(expected, other)),
    }
}

fn field<'a>(
    struct_value: &'a KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, PresentationExecError> {
    struct_value
        .fields
        .iter()
        .find_map(|(field_name, value)| (field_name == name).then_some(value))
        .ok_or_else(|| PresentationExecError::MissingField {
            record: struct_value.name.to_string(),
            field: SmolStr::new(name),
        })
}

fn expect_vec2(value: &KernelValue) -> Result<[f32; 2], PresentationExecError> {
    match value {
        KernelValue::Vec2(value) => Ok(*value),
        other => Err(type_mismatch("Vec2", other)),
    }
}

fn expect_vec3(value: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(type_mismatch("Vec3", other)),
    }
}

fn expect_f32(value: &KernelValue) -> Result<f32, PresentationExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(type_mismatch("F32", other)),
    }
}

fn expect_u32(value: &KernelValue) -> Result<u32, PresentationExecError> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(type_mismatch("U32", other)),
    }
}

fn expect_bool(value: &KernelValue) -> Result<bool, PresentationExecError> {
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(type_mismatch("Boolean", other)),
    }
}

fn hit_flag(value: &KernelValue) -> Result<bool, PresentationExecError> {
    let hit = expect_struct(value, "Hit3")?;
    expect_bool(field(hit, "hit")?)
}

fn hit_depth(hit: &KernelValue) -> Result<f32, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let did_hit = match field(hit, "hit")? {
        KernelValue::Bool(value) => *value,
        other => return Err(type_mismatch("Boolean", other)),
    };
    if did_hit {
        expect_f32(field(hit, "distance")?)
    } else {
        Ok(f32::INFINITY)
    }
}

fn hit_position(hit: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    expect_vec3(field(hit, "position")?)
}

fn hit_world_normal(hit: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let did_hit = match field(hit, "hit")? {
        KernelValue::Bool(value) => *value,
        other => return Err(type_mismatch("Boolean", other)),
    };
    if did_hit {
        expect_vec3(field(hit, "normal")?)
    } else {
        Ok([0.0, 0.0, 0.0])
    }
}

fn hit_steps(hit: &KernelValue) -> Result<u32, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    match field(hit, "steps")? {
        KernelValue::I32(value) => Ok((*value).max(0) as u32),
        other => Err(type_mismatch("I32", other)),
    }
}

fn normalize_hit_for_attachment(hit: &KernelValue) -> Result<KernelValue, PresentationExecError> {
    let hit = expect_struct(hit, "Hit3")?;
    let mut fields = hit.fields.clone();
    if let Some((_, payload)) = fields.iter_mut().find(|(name, _)| name == "payload")
        && matches!(payload, KernelValue::Nothing)
    {
        *payload = default_payload_value();
    }
    Ok(KernelValue::Struct(KernelStructValue {
        name: hit.name.clone(),
        fields,
    }))
}

pub(crate) fn primary_hit_miss_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Hit3"),
        fields: vec![
            (SmolStr::new("hit"), KernelValue::Bool(false)),
            (SmolStr::new("distance"), KernelValue::F32(f32::INFINITY)),
            (SmolStr::new("position"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (SmolStr::new("normal"), KernelValue::Vec3([0.0, 0.0, 0.0])),
            (
                SmolStr::new("local_position"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (
                SmolStr::new("local_normal"),
                KernelValue::Vec3([0.0, 0.0, 0.0]),
            ),
            (
                SmolStr::new("shading_frame"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("Transform3"),
                    fields: vec![
                        (
                            SmolStr::new("matrix"),
                            KernelValue::Mat4([
                                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                                0.0, 0.0, 1.0,
                            ]),
                        ),
                        (
                            SmolStr::new("inverse"),
                            KernelValue::Mat4([
                                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                                0.0, 0.0, 1.0,
                            ]),
                        ),
                    ],
                }),
            ),
            (SmolStr::new("steps"), KernelValue::I32(0)),
            (SmolStr::new("feature_id"), KernelValue::U32(0)),
            (SmolStr::new("instance_id"), KernelValue::U32(0)),
            (SmolStr::new("repeat_id"), KernelValue::U32(0)),
            (SmolStr::new("root_shape_id"), KernelValue::U32(0)),
            (SmolStr::new("payload"), default_payload_value()),
        ],
    })
}

fn default_payload_value() -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("Payload"),
        fields: vec![
            (SmolStr::new("entity_id"), KernelValue::U32(0)),
            (SmolStr::new("material_id"), KernelValue::U32(0)),
            (
                SmolStr::new("actor"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ActorHandle"),
                    fields: vec![
                        (SmolStr::new("id"), KernelValue::U32(0)),
                        (SmolStr::new("generation"), KernelValue::U32(0)),
                    ],
                }),
            ),
        ],
    })
}

fn attachment_target_index(
    attachments: &AttachmentResourceSet,
    attachment: &crate::presentation_exec::resources::AttachmentResource,
    full_index: usize,
) -> usize {
    if attachment.layout.width == attachments.width
        && attachment.layout.height == attachments.height
    {
        return full_index;
    }
    let x = (full_index as u32) % attachments.width.max(1);
    let y = (full_index as u32) / attachments.width.max(1);
    let scaled_x = x / attachment.layout.attachment.scale.divisor_x.max(1);
    let scaled_y = y / attachment.layout.attachment.scale.divisor_y.max(1);
    (scaled_y * attachment.layout.width + scaled_x) as usize
}

fn apply_attachment_divisor(attachment: &mut FrameAttachmentContract, requested_divisor: u32) {
    if matches!(attachment.lifetime, AttachmentLifetime::HistorySlot(_)) {
        return;
    }
    let combined_divisor = attachment
        .scale
        .divisor_x
        .max(attachment.scale.divisor_y)
        .saturating_mul(requested_divisor)
        .clamp(1, 4);
    attachment.scale = match combined_divisor {
        4 => AttachmentResolutionScale::quarter(),
        2 => AttachmentResolutionScale::half(),
        _ => AttachmentResolutionScale::full(),
    };
    attachment.resolution = match combined_divisor {
        4 => AttachmentResolutionClass::QuarterViewport,
        2 => AttachmentResolutionClass::HalfViewport,
        _ => AttachmentResolutionClass::Viewport,
    };
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

fn normalize3(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = dot3(value, value);
    if len_sq <= f32::EPSILON {
        fallback
    } else {
        let inv_len = len_sq.sqrt().recip();
        [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
    }
}

fn type_mismatch(expected: &str, found: &KernelValue) -> PresentationExecError {
    PresentationExecError::TypeMismatch {
        expected: expected.to_string(),
        found: kernel_value_kind(found).to_string(),
    }
}

fn kernel_value_kind(value: &KernelValue) -> &'static str {
    match value {
        KernelValue::Nothing => "Nothing",
        KernelValue::Bool(_) => "Boolean",
        KernelValue::I32(_) => "I32",
        KernelValue::U32(_) => "U32",
        KernelValue::F32(_) => "F32",
        KernelValue::Vec2(_) => "Vec2",
        KernelValue::Vec3(_) => "Vec3",
        KernelValue::Vec4(_) => "Vec4",
        KernelValue::Mat3(_) => "Mat3",
        KernelValue::Mat4(_) => "Mat4",
        KernelValue::Quat(_) => "Quat",
        KernelValue::Array(_) => "Array",
        KernelValue::Struct(_) => "Struct",
        KernelValue::Capture(_) => "Capture",
        KernelValue::DispatchBackend(_) => "DispatchBackend",
        KernelValue::GpuBuffer(_) => "GpuBuffer",
        KernelValue::GpuAtomicI32(_) => "GpuAtomicI32",
        KernelValue::GpuAtomicU32(_) => "GpuAtomicU32",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalCameraInput, CanonicalViewportInput, TileCandidateQueueState,
        build_tile_candidate_artifact, build_tile_candidate_span_words,
        projected_bounds_tile_range, tile_candidate_dispatch_packets,
    };
    use smol_str::SmolStr;

    #[test]
    fn tile_candidate_artifact_builds_tile_spans_and_candidate_queues() {
        let viewport = CanonicalViewportInput {
            width: 24,
            height: 8,
        };
        let artifact = build_tile_candidate_artifact(
            viewport,
            &[
                vec![SmolStr::new("shape.left")],
                vec![SmolStr::new("shape.right"), SmolStr::new("shape.extra")],
                Vec::new(),
            ],
            true,
        );

        assert!(artifact.enabled);
        assert_eq!(artifact.tile_spans.len(), 3);
        assert_eq!(
            artifact.candidate_shapes,
            vec![
                SmolStr::new("shape.left"),
                SmolStr::new("shape.right"),
                SmolStr::new("shape.extra"),
            ]
        );
        assert_eq!(
            artifact.tile_spans[0].state,
            TileCandidateQueueState::Singleton
        );
        assert_eq!(
            artifact.tile_spans[1].state,
            TileCandidateQueueState::Packeted
        );
        assert_eq!(artifact.tile_spans[2].state, TileCandidateQueueState::Empty);
        assert_eq!(artifact.active_samples, 128);
        assert_eq!(artifact.skipped_samples, 64);

        let packets = tile_candidate_dispatch_packets(&artifact, 8);
        assert_eq!(packets.len(), 9);
        assert_eq!(packets[0].state, TileCandidateQueueState::Singleton);
        assert_eq!(packets[0].tile_indices, vec![0]);
        assert_eq!(
            packets[0].candidate_shapes,
            vec![SmolStr::new("shape.left")]
        );
        assert_eq!(packets[0].sample_indices.len(), 64);
        assert_eq!(packets[0].sample_indices.first(), Some(&0));
        assert_eq!(packets[0].sample_indices.last(), Some(&175));
        assert_eq!(packets[1].state, TileCandidateQueueState::Packeted);
        assert_eq!(packets[1].tile_indices, vec![1]);
        assert_eq!(
            packets[1].candidate_shapes,
            vec![SmolStr::new("shape.right"), SmolStr::new("shape.extra")]
        );
        assert_eq!(packets[1].sample_indices.len(), 8);

        let disabled = build_tile_candidate_artifact(
            viewport,
            &[
                vec![SmolStr::new("shape.left")],
                vec![SmolStr::new("shape.right"), SmolStr::new("shape.extra")],
            ],
            false,
        );
        assert!(!disabled.enabled);
        assert!(disabled.tile_spans.is_empty());
        assert!(tile_candidate_dispatch_packets(&disabled, 8).is_empty());
    }

    #[test]
    fn tile_candidate_span_words_cover_active_samples_and_skip_gaps() {
        let viewport = CanonicalViewportInput {
            width: 16,
            height: 8,
        };
        let active_samples = (0..128usize).collect::<Vec<_>>();
        let artifact = build_tile_candidate_artifact(
            viewport,
            &[vec![SmolStr::new("shape.left")], Vec::new()],
            true,
        );

        let spans = build_tile_candidate_span_words(&artifact, &active_samples, 8);
        assert_eq!(spans.len(), 256);
        assert_eq!(spans[0], 0);
        assert_eq!(spans[1], 1);
        assert_eq!(spans[7 * 2], 0);
        assert_eq!(spans[7 * 2 + 1], 1);
        assert_eq!(spans[8 * 2], 1);
        assert_eq!(spans[8 * 2 + 1], 0);

        let disabled = build_tile_candidate_artifact(
            viewport,
            &[vec![SmolStr::new("shape.left")], Vec::new()],
            false,
        );
        let disabled_spans = build_tile_candidate_span_words(&disabled, &active_samples, 8);
        assert_eq!(disabled_spans[0], u32::MAX);
        assert_eq!(disabled_spans[1], u32::MAX);
        assert_eq!(disabled_spans[8 * 2], u32::MAX);
        assert_eq!(disabled_spans[8 * 2 + 1], u32::MAX);
        assert_eq!(disabled_spans[64 * 2], u32::MAX);
        assert_eq!(disabled_spans[64 * 2 + 1], u32::MAX);
    }

    #[test]
    fn tile_candidate_span_words_keep_enabled_empty_tiles_as_zero_length_misses() {
        let viewport = CanonicalViewportInput {
            width: 16,
            height: 8,
        };
        let active_samples = (0..128usize).collect::<Vec<_>>();
        let artifact = build_tile_candidate_artifact(viewport, &[Vec::new(), Vec::new()], true);

        let spans = build_tile_candidate_span_words(&artifact, &active_samples, 8);
        assert_eq!(spans[0], 0);
        assert_eq!(spans[1], 0);
        assert_eq!(spans[64 * 2], 0);
        assert_eq!(spans[64 * 2 + 1], 0);
    }

    #[test]
    fn projected_bounds_tile_range_keeps_full_screen_coverage_when_bounds_cross_camera_plane() {
        let viewport = CanonicalViewportInput {
            width: 16,
            height: 16,
        };
        let camera = CanonicalCameraInput {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 75.0,
        };

        let coverage = projected_bounds_tile_range(
            2,
            2,
            8,
            camera,
            viewport,
            [-0.5, -0.5, -1.0],
            [0.5, 0.5, 0.5],
        )
        .expect("camera-plane-crossing bounds should stay conservative")
        .expect("coverage should remain available");

        assert_eq!(coverage, (0, 1, 0, 1));
    }
}
