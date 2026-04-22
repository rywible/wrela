//! Shared frame-live session core for the CLI launcher and the native app host.
//! Owns presentation preparation, click decoding, provenance resolution, and
//! source-watch reload behavior. Does not own CLI parsing, app UI chrome, or
//! OS-specific process launching.

use crate::diag::catalog::{ProjectDiagKind, project_descriptor};
use crate::diag::suppress::suppress_cascades;
use crate::diag::{DiagRecord, DiagSeverity, DiagStage, dedupe_records};
use crate::hir;
use crate::kernel::KernelValue;
use crate::presentation_contract::FrameAttachmentKind;
use miette::SourceSpan;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const FRAME_LIVE_RELOAD_POLL_MS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePixel {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRefKind {
    Field,
    Shape,
    Region,
}

impl SourceRefKind {
    pub fn label(self) -> &'static str {
        match self {
            SourceRefKind::Field => "field",
            SourceRefKind::Shape => "shape",
            SourceRefKind::Region => "region",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionSourceRef {
    pub kind: SourceRefKind,
    pub symbol: String,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionRecord {
    pub generation: u64,
    pub window_pixel: FramePixel,
    pub frame_pixel: FramePixel,
    pub hit: bool,
    pub region_name: Option<String>,
    pub shape_name: Option<String>,
    pub field_name: Option<String>,
    pub root_shape_id: Option<u32>,
    pub feature_id: Option<u32>,
    pub instance_id: Option<u32>,
    pub repeat_id: Option<u32>,
    pub world_position: Option<[f32; 3]>,
    pub normal: Option<[f32; 3]>,
    pub primary_source: Option<SelectionSourceRef>,
    pub source_refs: Vec<SelectionSourceRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameLiveQueryBackend {
    Auto,
    Cpu,
    VirtualGpu,
    Wgsl,
}

impl FrameLiveQueryBackend {
    pub fn into_dispatch_backend(self) -> crate::query_plan::DispatchBackend {
        match self {
            FrameLiveQueryBackend::Auto => crate::query_plan::DispatchBackend::Auto,
            FrameLiveQueryBackend::Cpu => crate::query_plan::DispatchBackend::Cpu,
            FrameLiveQueryBackend::VirtualGpu => crate::query_plan::DispatchBackend::VirtualGpu,
            FrameLiveQueryBackend::Wgsl => crate::query_plan::DispatchBackend::Wgsl,
        }
    }
}

impl From<crate::query_plan::DispatchBackend> for FrameLiveQueryBackend {
    fn from(value: crate::query_plan::DispatchBackend) -> Self {
        match value {
            crate::query_plan::DispatchBackend::Auto => Self::Auto,
            crate::query_plan::DispatchBackend::Cpu => Self::Cpu,
            crate::query_plan::DispatchBackend::VirtualGpu => Self::VirtualGpu,
            crate::query_plan::DispatchBackend::Wgsl => Self::Wgsl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameLiveCameraConfig {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub vertical_fov_degrees: f32,
}

impl FrameLiveCameraConfig {
    pub fn canonical(self) -> crate::presentation_contract::CanonicalCameraInput {
        crate::presentation_contract::CanonicalCameraInput {
            position: self.position,
            forward: self.forward,
            up: self.up,
            vertical_fov_degrees: self.vertical_fov_degrees,
        }
    }
}

impl From<crate::presentation_contract::CanonicalCameraInput> for FrameLiveCameraConfig {
    fn from(value: crate::presentation_contract::CanonicalCameraInput) -> Self {
        Self {
            position: value.position,
            forward: value.forward,
            up: value.up,
            vertical_fov_degrees: value.vertical_fov_degrees,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameLiveLaunchConfig {
    pub entry_path: PathBuf,
    pub view: Option<String>,
    pub region: Option<String>,
    pub domain: Option<String>,
    pub camera: FrameLiveCameraConfig,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_index: u32,
    pub delta_seconds: f32,
    pub query_backend: FrameLiveQueryBackend,
}

#[derive(Debug, Clone)]
pub struct FrameLiveDiagnostic {
    pub record: DiagRecord,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameLiveErrorKind {
    Usage,
    Parse,
    Type,
    Codegen,
}

#[derive(Debug, Clone)]
pub struct FrameLiveError {
    kind: FrameLiveErrorKind,
    message: String,
    diagnostics: Vec<FrameLiveDiagnostic>,
}

impl FrameLiveError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            kind: FrameLiveErrorKind::Usage,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    pub fn parse(message: impl Into<String>, diagnostics: Vec<FrameLiveDiagnostic>) -> Self {
        Self {
            kind: FrameLiveErrorKind::Parse,
            message: message.into(),
            diagnostics,
        }
    }

    pub fn type_error(message: impl Into<String>, diagnostics: Vec<FrameLiveDiagnostic>) -> Self {
        Self {
            kind: FrameLiveErrorKind::Type,
            message: message.into(),
            diagnostics,
        }
    }

    pub fn codegen(message: impl Into<String>) -> Self {
        Self {
            kind: FrameLiveErrorKind::Codegen,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    pub fn kind(&self) -> FrameLiveErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn diagnostics(&self) -> &[FrameLiveDiagnostic] {
        &self.diagnostics
    }

    pub fn render_human(&self) -> String {
        if self.diagnostics.is_empty() {
            return self.message.clone();
        }
        let mut out = Vec::new();
        out.push(self.message.clone());
        for diagnostic in &self.diagnostics {
            let primary = diagnostic.record.labels.first();
            let location = primary.map(|label| {
                let (line, column) = line_col_at_offset(&diagnostic.source, label.span.offset);
                format!("{}:{}:{}", label.span.path, line, column)
            });
            if let Some(location) = location {
                out.push(format!("{} [{}]", diagnostic.record.message, location));
            } else {
                out.push(diagnostic.record.message.clone());
            }
        }
        out.join("\n")
    }
}

#[derive(Debug, Clone)]
pub struct CompiledPresentationBundle {
    pub module: hir::Module,
    pub module_sources: HashMap<PathBuf, String>,
    pub provenance: hir::project::ProjectProvenance,
    pub query_ctx: crate::query_exec::QueryExecContext,
    pub plans: Vec<crate::presentation_plan::PresentationPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedPresentationExecution {
    pub plan: crate::presentation_plan::PresentationPlan,
    pub input: crate::presentation_exec::PresentationExecutionInput,
    pub semantic_domain: String,
    pub execution_policy: crate::presentation_exec::PresentationExecutionPolicy,
    pub camera: crate::presentation_contract::CanonicalCameraInput,
    pub viewport: crate::presentation_contract::CanonicalViewportInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomainExecutionInputs {
    pub frame_domain: crate::kernel::KernelValue,
    pub semantic_domain: String,
    pub execution_policy: crate::presentation_exec::PresentationExecutionPolicy,
}

#[derive(Debug, Clone)]
pub struct ReadyPresentationExecution {
    pub bundle: CompiledPresentationBundle,
    pub prepared: PreparedPresentationExecution,
    pub region_name: SmolStr,
    pub domain_name: SmolStr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameLiveFrame {
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub color_buffer: Vec<u32>,
    pub view_name: String,
    pub region_name: String,
}

#[derive(Debug, Clone)]
struct LiveFrameState {
    ready: ReadyPresentationExecution,
    result: crate::presentation_exec::PresentationExecutionResult,
    color_buffer: Vec<u32>,
    generation: u64,
}

type SourceSnapshot = Vec<(PathBuf, Option<SystemTime>)>;

pub struct FrameLiveSession {
    config: FrameLiveLaunchConfig,
    watched_sources: Vec<PathBuf>,
    last_snapshot: SourceSnapshot,
    state: LiveFrameState,
}

impl FrameLiveSession {
    pub fn load(config: FrameLiveLaunchConfig) -> Result<Self, FrameLiveError> {
        let state = render_live_state(&config, 1)?;
        let watched_sources = watched_source_paths(&state.ready.bundle.module_sources);
        let last_snapshot = snapshot_source_paths(&watched_sources);
        Ok(Self {
            config,
            watched_sources,
            last_snapshot,
            state,
        })
    }

    pub fn frame(&self) -> FrameLiveFrame {
        FrameLiveFrame {
            generation: self.state.generation,
            width: self.state.result.width,
            height: self.state.result.height,
            color_buffer: self.state.color_buffer.clone(),
            view_name: self.state.ready.prepared.plan.name.to_string(),
            region_name: self.state.ready.region_name.to_string(),
        }
    }

    pub fn window_title(&self) -> String {
        format!(
            "wrela frame-live | view={} | region={} | generation={}",
            self.state.ready.prepared.plan.name,
            self.state.ready.region_name,
            self.state.generation
        )
    }

    pub fn selection_record(
        &self,
        window_pixel: FramePixel,
        frame_pixel: FramePixel,
    ) -> Result<SelectionRecord, String> {
        selection_record_for_frame_pixel(
            &self.state.ready.bundle,
            &self.state.ready.region_name,
            &self.state.result,
            window_pixel,
            frame_pixel,
            self.state.generation,
        )
    }

    pub fn headless_selection_record(
        &self,
        requested_pixel: Option<FramePixel>,
    ) -> Option<SelectionRecord> {
        let pixel = requested_pixel
            .map(|pixel| FramePixel {
                x: pixel.x.min(self.state.result.width.saturating_sub(1)),
                y: pixel.y.min(self.state.result.height.saturating_sub(1)),
            })
            .or_else(|| first_hit_pixel(&self.state.result))
            .unwrap_or(FramePixel { x: 0, y: 0 });
        self.selection_record(pixel, pixel).ok()
    }

    pub fn force_reload(&mut self) -> Result<FrameLiveFrame, FrameLiveError> {
        self.reload_generation(self.state.generation + 1)
    }

    pub fn reload_if_sources_changed(&mut self) -> Result<Option<FrameLiveFrame>, FrameLiveError> {
        if !sources_changed(&self.watched_sources, &mut self.last_snapshot) {
            return Ok(None);
        }
        self.reload_generation(self.state.generation + 1).map(Some)
    }

    fn reload_generation(&mut self, generation: u64) -> Result<FrameLiveFrame, FrameLiveError> {
        let candidate = render_live_state(&self.config, generation)?;
        if candidate.result.width != self.state.result.width
            || candidate.result.height != self.state.result.height
        {
            return Err(FrameLiveError::codegen(format!(
                "frame-live reload rejected because the rendered size changed from {}x{} to {}x{}; restart the command for the new fixed size",
                self.state.result.width,
                self.state.result.height,
                candidate.result.width,
                candidate.result.height
            )));
        }
        let watched_sources = watched_source_paths(&candidate.ready.bundle.module_sources);
        let last_snapshot = snapshot_source_paths(&watched_sources);
        self.watched_sources = watched_sources;
        self.last_snapshot = last_snapshot;
        self.state = candidate;
        Ok(self.frame())
    }
}

pub fn map_window_click_to_frame_pixel(
    cursor_position: (f64, f64),
    window_size: (f64, f64),
    frame_size: (u32, u32),
) -> FramePixel {
    let (window_width, window_height) = window_size;
    let (frame_width, frame_height) = frame_size;
    FramePixel {
        x: map_axis(cursor_position.0, window_width, frame_width),
        y: map_axis(cursor_position.1, window_height, frame_height),
    }
}

pub fn frame_sample_index(pixel: FramePixel, frame_width: u32) -> usize {
    (pixel.y.saturating_mul(frame_width).saturating_add(pixel.x)) as usize
}

pub fn choose_primary_source_ref(source_refs: &[SelectionSourceRef]) -> Option<SelectionSourceRef> {
    source_refs
        .iter()
        .min_by_key(|source| source_ref_priority(source.kind))
        .cloned()
}

pub fn selection_record_for_frame_pixel(
    bundle: &CompiledPresentationBundle,
    region_name: &SmolStr,
    result: &crate::presentation_exec::PresentationExecutionResult,
    window_pixel: FramePixel,
    frame_pixel: FramePixel,
    generation: u64,
) -> Result<SelectionRecord, String> {
    let primary_hit = result
        .attachments
        .attachment("primary_hit")
        .ok_or_else(|| "missing primary_hit attachment".to_string())?;
    let index = frame_sample_index(frame_pixel, result.width);
    let hit_value = primary_hit
        .decode(index)
        .map_err(|err| format!("decode primary_hit[{index}] failed: {err}"))?;
    let hit = hit_details(&hit_value)?;
    let mut source_refs = Vec::new();
    if hit.hit {
        if let Some(field_name) = hit.field_name(bundle).and_then(|field_name| {
            field_source_ref(bundle, &field_name).inspect(|source| source_refs.push(source.clone()))
        }) {
            let _ = field_name;
        }
        if let Some(shape_name) = hit.shape_name(bundle).and_then(|shape_name| {
            shape_source_ref(bundle, &shape_name).inspect(|source| source_refs.push(source.clone()))
        }) {
            let _ = shape_name;
        }
        if let Some(region_source) = region_source_ref(bundle, region_name) {
            source_refs.push(region_source);
        }
    }
    let field_name = hit.field_name(bundle).map(|name| name.to_string());
    let shape_name = hit.shape_name(bundle).map(|name| name.to_string());
    let region_name_string = Some(region_name.to_string());
    let primary_source = choose_primary_source_ref(&source_refs);
    Ok(SelectionRecord {
        generation,
        window_pixel,
        frame_pixel,
        hit: hit.hit,
        region_name: region_name_string,
        shape_name,
        field_name,
        root_shape_id: hit.hit.then_some(hit.root_shape_id),
        feature_id: hit.hit.then_some(hit.feature_id),
        instance_id: hit.hit.then_some(hit.instance_id),
        repeat_id: hit.hit.then_some(hit.repeat_id),
        world_position: hit.hit.then_some(hit.position),
        normal: hit.hit.then_some(hit.normal),
        primary_source,
        source_refs,
    })
}

pub fn render_selection_record_human(record: &SelectionRecord) -> String {
    let mut lines = Vec::new();
    lines.push(format!("selection generation={}", record.generation));
    lines.push(format!(
        "  window_pixel=({}, {}) frame_pixel=({}, {})",
        record.window_pixel.x, record.window_pixel.y, record.frame_pixel.x, record.frame_pixel.y
    ));
    lines.push(format!("  hit={}", record.hit));
    if let Some(region_name) = &record.region_name {
        lines.push(format!("  region={region_name}"));
    }
    if let Some(shape_name) = &record.shape_name {
        lines.push(format!("  shape={shape_name}"));
    }
    if let Some(field_name) = &record.field_name {
        lines.push(format!("  field={field_name}"));
    }
    if let Some(root_shape_id) = record.root_shape_id {
        lines.push(format!("  root_shape_id={root_shape_id}"));
    }
    if let Some(feature_id) = record.feature_id {
        lines.push(format!("  feature_id={feature_id}"));
    }
    if let Some(instance_id) = record.instance_id {
        lines.push(format!("  instance_id={instance_id}"));
    }
    if let Some(repeat_id) = record.repeat_id {
        lines.push(format!("  repeat_id={repeat_id}"));
    }
    if let Some(position) = record.world_position {
        lines.push(format!(
            "  world_position=({:.3}, {:.3}, {:.3})",
            position[0], position[1], position[2]
        ));
    }
    if let Some(normal) = record.normal {
        lines.push(format!(
            "  normal=({:.3}, {:.3}, {:.3})",
            normal[0], normal[1], normal[2]
        ));
    }
    if let Some(primary_source) = &record.primary_source {
        lines.push(format!(
            "  primary_source={} {}:{}:{}",
            primary_source.symbol,
            primary_source.path.display(),
            primary_source.line,
            primary_source.column
        ));
    }
    if !record.source_refs.is_empty() {
        lines.push("  source_refs:".to_string());
        for source in &record.source_refs {
            lines.push(format!(
                "    {} {} {}:{}:{}",
                source.kind.label(),
                source.symbol,
                source.path.display(),
                source.line,
                source.column
            ));
        }
    }
    lines.join("\n")
}

pub fn watched_source_paths(module_sources: &HashMap<PathBuf, String>) -> Vec<PathBuf> {
    let mut paths = module_sources
        .keys()
        .filter(|path| is_source_file(path.as_path()))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub fn snapshot_source_paths(paths: &[PathBuf]) -> SourceSnapshot {
    let mut snapshot = paths
        .iter()
        .map(|path| {
            let modified = std::fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok());
            (path.clone(), modified)
        })
        .collect::<Vec<_>>();
    snapshot.sort_by(|a, b| a.0.cmp(&b.0));
    snapshot
}

pub fn sources_changed(paths: &[PathBuf], last: &mut SourceSnapshot) -> bool {
    let current = snapshot_source_paths(paths);
    if current != *last {
        *last = current;
        return true;
    }
    false
}

pub fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("wr") | Some("sp")
    )
}

pub fn compile_presentation_bundle(
    entry_path: &Path,
    query_backend: crate::query_plan::DispatchBackend,
) -> Result<CompiledPresentationBundle, FrameLiveError> {
    let project =
        crate::hir::project::load_project_with_entrypoint(entry_path, false).map_err(|errors| {
            let diagnostics = errors
                .into_iter()
                .map(|err| FrameLiveDiagnostic {
                    record: project_diag_record(
                        err.kind,
                        DiagSeverity::Error,
                        err.message,
                        err.path.display().to_string(),
                        err.span,
                    ),
                    source: err.source,
                })
                .collect::<Vec<_>>();
            FrameLiveError::parse(
                format!("failed to load project `{}`", entry_path.display()),
                diagnostics,
            )
        })?;

    let module = project.module.clone();
    let source = project.entry_source.clone();
    let source_name = entry_path.display().to_string();
    let mut source_by_path = project.module_sources.clone();
    let provenance = project.provenance.clone();
    source_by_path
        .entry(entry_path.to_path_buf())
        .or_insert_with(|| source.clone());

    let semantic = crate::hir::semantic::check_module(&module);
    let (type_errors, type_info) = crate::hir::typeck::check_module_with_info(&module);
    let mut records = Vec::new();
    for err in semantic.errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        records.push(DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        ));
    }
    for err in type_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        records.push(DiagRecord::from_diagnostic(
            DiagStage::Type,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        ));
    }
    if !records.is_empty() {
        let diagnostics = suppress_cascades(dedupe_records(records))
            .into_iter()
            .map(|record| {
                let source_for_record = source_by_path
                    .get(Path::new(
                        &record
                            .labels
                            .first()
                            .map(|label| label.span.path.clone())
                            .unwrap_or_else(|| source_name.clone()),
                    ))
                    .cloned()
                    .unwrap_or_else(|| source.clone());
                FrameLiveDiagnostic {
                    record,
                    source: source_for_record,
                }
            })
            .collect::<Vec<_>>();
        return Err(FrameLiveError::type_error(
            format!(
                "failed to prepare presentation execution for `{}`",
                entry_path.display()
            ),
            diagnostics,
        ));
    }

    let mir_module =
        crate::mir::lower::lower_module_with_types_and_backend(&module, &type_info, query_backend);
    let validation_errors = crate::mir::validate::validate_module(&mir_module);
    if !validation_errors.is_empty() {
        return Err(FrameLiveError::codegen(
            validation_errors
                .into_iter()
                .map(|err| err.message)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }

    let query_ctx = crate::query_exec::QueryExecContext::compile(&module, &type_info);
    let plans = crate::presentation_plan::plans_for_module(&module, query_backend);
    for plan in &plans {
        let validation_errors = plan.validate();
        if !validation_errors.is_empty() {
            return Err(FrameLiveError::codegen(
                validation_errors
                    .into_iter()
                    .map(|err| err.message.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
    }

    Ok(CompiledPresentationBundle {
        module,
        module_sources: source_by_path,
        provenance,
        query_ctx,
        plans,
    })
}

pub fn load_prepared_presentation_execution(
    entry_path: &Path,
    query_backend: crate::query_plan::DispatchBackend,
    requested_view: Option<&str>,
    requested_region: Option<&str>,
    requested_domain: Option<&str>,
    camera: crate::presentation_contract::CanonicalCameraInput,
    width: Option<u32>,
    height: Option<u32>,
    frame_index: u32,
    delta_seconds: f32,
    query_trace_solver_mode: crate::query_exec::QueryTraceSolverMode,
) -> Result<ReadyPresentationExecution, FrameLiveError> {
    let bundle = compile_presentation_bundle(entry_path, query_backend)?;
    let plan = select_view_plan(&bundle, requested_view).map_err(FrameLiveError::usage)?;
    let view_func = bundle
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == plan.name)
        .map(|(_, func)| func)
        .expect("selected presentation plan should map back to a function");
    let region_name =
        select_region_name(&bundle.module, requested_region).map_err(FrameLiveError::usage)?;
    let domain_name = select_domain_name(&bundle.module, view_func, requested_domain)
        .map_err(FrameLiveError::usage)?;
    let prepared = prepare_presentation_execution(
        &bundle.module,
        &bundle.query_ctx,
        plan,
        view_func,
        region_name.clone(),
        domain_name.clone(),
        camera,
        width,
        height,
        frame_index,
        delta_seconds,
        query_backend,
        query_trace_solver_mode,
        false,
    )
    .map_err(FrameLiveError::usage)?;
    Ok(ReadyPresentationExecution {
        bundle,
        prepared,
        region_name,
        domain_name,
    })
}

pub fn select_view_plan<'a>(
    bundle: &'a CompiledPresentationBundle,
    requested: Option<&str>,
) -> Result<&'a crate::presentation_plan::PresentationPlan, String> {
    let mut candidates = bundle
        .plans
        .iter()
        .filter(|plan| {
            bundle
                .module
                .functions
                .iter()
                .any(|(_, func)| func.name == plan.name && func.role == hir::FunctionRole::View)
        })
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|plan| plan.name == requested)
            .ok_or_else(|| format!("missing view `{requested}`"));
    }
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("presentation execution requires --view when multiple view plans exist".to_string())
    }
}

pub fn select_region_name(
    module: &hir::Module,
    requested: Option<&str>,
) -> Result<SmolStr, String> {
    let mut candidates = module
        .functions
        .iter()
        .filter(|(_, func)| func.role == hir::FunctionRole::Region)
        .map(|(_, func)| func.name.clone())
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|name| name == requested)
            .ok_or_else(|| format!("missing region `{requested}`"));
    }
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("presentation execution requires --region when multiple regions exist".to_string())
    }
}

pub fn select_domain_name(
    module: &hir::Module,
    view: &hir::Function,
    requested: Option<&str>,
) -> Result<SmolStr, String> {
    if let Some(requested) = requested {
        return module
            .functions
            .iter()
            .find(|(_, func)| func.name == requested && func.role == hir::FunctionRole::Domain)
            .map(|(_, func)| func.name.clone())
            .ok_or_else(|| format!("missing domain `{requested}`"));
    }
    if let Some(domain_body) = view
        .presentation
        .as_ref()
        .and_then(|metadata| metadata.frame.domain.as_ref())
        && let Some(name) = body_called_function_name(domain_body)
    {
        return Ok(name);
    }
    let mut candidates = module
        .functions
        .iter()
        .filter(|(_, func)| func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func.name.clone())
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err(
            "presentation execution requires --domain when the view does not name a single domain"
                .to_string(),
        )
    }
}

pub fn prepare_presentation_execution(
    module: &hir::Module,
    query_ctx: &crate::query_exec::QueryExecContext,
    base_plan: &crate::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    region_name: SmolStr,
    domain_name: SmolStr,
    camera: crate::presentation_contract::CanonicalCameraInput,
    width_override: Option<u32>,
    height_override: Option<u32>,
    frame_index: u32,
    delta_seconds: f32,
    query_backend: crate::query_plan::DispatchBackend,
    query_trace_solver_mode: crate::query_exec::QueryTraceSolverMode,
    disable_export_attachment: bool,
) -> Result<PreparedPresentationExecution, String> {
    let region_snapshot = query_ctx
        .region_snapshot_handle(&region_name)
        .cloned()
        .ok_or_else(|| format!("missing region snapshot for `{region_name}`"))?;
    let domain_func = module
        .functions
        .iter()
        .find(|(_, func)| func.name == domain_name && func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func)
        .ok_or_else(|| format!("missing domain `{domain_name}`"))?;
    let width = resolve_view_dimension(view_func, width_override, true)?;
    let height = resolve_view_dimension(view_func, height_override, false)?;
    let domain_inputs = domain_execution_inputs(module, domain_func, &region_name, query_backend)?;
    let mut plan = base_plan.clone();
    let domain_metadata = domain_func
        .domain
        .as_ref()
        .ok_or_else(|| format!("selected domain `{domain_name}` is missing domain metadata"))?;
    plan.apply_participant_policy(domain_metadata.radiance, domain_metadata.media);
    if disable_export_attachment {
        strip_presentation_export_attachment(&mut plan);
    }
    let validation_errors = plan.validate();
    if !validation_errors.is_empty() {
        return Err(format!(
            "presentation execution plan `{}` failed validation after participant policy: {}",
            plan.name,
            validation_errors
                .into_iter()
                .map(|err| err.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let bindings = bind_presentation_function_params(view_func, &region_snapshot, camera);
    let lighting = authored_presentation_lighting_inputs(view_func, &bindings)?;
    let compatibility_projection =
        authored_compatibility_projection_input(&plan, view_func, &bindings, camera)?;
    let frame_state = crate::presentation_exec::frame_state_value(
        camera,
        camera,
        crate::presentation_contract::CanonicalViewportInput { width, height },
        [0.0, 0.0],
        frame_index,
        delta_seconds,
    );
    Ok(PreparedPresentationExecution {
        plan,
        input: crate::presentation_exec::PresentationExecutionInput {
            region_snapshot,
            frame_domain: domain_inputs.frame_domain,
            frame_state,
            history: None,
            resident_history_attachments: None,
            materialize_cpu_attachments: true,
            runtime_summary_only: false,
            collect_gpu_timing_readback: true,
            lighting,
            compatibility_projection,
            execution_policy: domain_inputs.execution_policy,
            query_trace_solver_mode,
            quality_override: None,
            backend: query_backend,
        },
        semantic_domain: domain_inputs.semantic_domain,
        execution_policy: domain_inputs.execution_policy,
        camera,
        viewport: crate::presentation_contract::CanonicalViewportInput { width, height },
    })
}

pub fn strip_presentation_export_attachment(plan: &mut crate::presentation_plan::PresentationPlan) {
    let export_binding_ids = plan
        .passes
        .iter()
        .filter_map(|pass| match &pass.kind {
            crate::presentation_plan::PresentationPassKind::ExportAttachment { .. } => {
                pass.binding.clone()
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    plan.passes.retain(|pass| {
        !matches!(
            pass.kind,
            crate::presentation_plan::PresentationPassKind::ExportAttachment { .. }
        )
    });
    if export_binding_ids.is_empty() {
        return;
    }
    plan.bindings
        .retain(|binding| !export_binding_ids.contains(&binding.id));
}

pub fn bind_presentation_function_params(
    function: &hir::Function,
    region_snapshot: &crate::world_identity::WorldSnapshotHandle,
    camera: crate::presentation_contract::CanonicalCameraInput,
) -> HashMap<SmolStr, crate::kernel::KernelValue> {
    let mut bindings = HashMap::new();
    for param in &function.params {
        match param.ty.as_ref().map(|ty| ty.name.as_str()) {
            Some("RegionCapture") => {
                bindings.insert(param.name.clone(), region_snapshot.capture_value());
            }
            Some("Camera") => {
                bindings.insert(param.name.clone(), preview_camera_value(camera));
            }
            _ => {}
        }
    }
    bindings
}

pub fn authored_presentation_lighting_inputs(
    view_func: &hir::Function,
    bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
) -> Result<crate::presentation_contract::PresentationLightingInputs, String> {
    let metadata = view_func.presentation.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing presentation metadata",
            view_func.name
        )
    })?;
    if metadata.lighting.lights.is_some() {
        return Err(format!(
            "presentation execution does not yet support plural `lights` metadata on `{}`; author `key_light` instead",
            view_func.name
        ));
    }
    let grouped = metadata.lighting.grouped.as_ref();
    let key_light = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "light").map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .light
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_light(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting key_light")?,
            "presentation lighting key_light",
        )?,
        None => default_preview_key_light(),
    };
    let fill_direction = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_direction")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .fill_dir
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting fill_direction",
            )?,
            "presentation lighting fill_direction",
        )?,
        None => normalize_preview_vec3([-0.9, 0.45, 0.2]),
    };
    let fill_strength = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_strength")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .fill_strength
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_f32(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting fill_strength",
            )?,
            "presentation lighting fill_strength",
        )?,
        None => 0.22,
    };
    let ambient_color = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "ambient_color")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .ambient_color
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(
                body,
                expr_id,
                bindings,
                "presentation lighting ambient_color",
            )?,
            "presentation lighting ambient_color",
        )?,
        None => [0.12, 0.12, 0.12],
    };
    Ok(crate::presentation_contract::PresentationLightingInputs {
        key_light,
        fill_direction,
        fill_strength,
        ambient_color,
    })
}

pub fn authored_compatibility_projection_input(
    plan: &crate::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
    camera: crate::presentation_contract::CanonicalCameraInput,
) -> Result<Option<crate::presentation_contract::LegacyCompatibilityProjectionInput>, String> {
    if !plan.view.compatibility_projection.legacy_path_active {
        return Ok(None);
    }
    let metadata = view_func.presentation.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing presentation metadata",
            view_func.name
        )
    })?;
    let world_up = match metadata.compatibility.world_up.as_ref() {
        Some(body) => preview_expect_vec3(
            &preview_eval_body(body, bindings, "presentation compatibility world_up")?,
            "presentation compatibility world_up",
        )?,
        None => camera.up,
    };
    let view_scale = match metadata.compatibility.view_scale.as_ref() {
        Some(body) => preview_expect_f32(
            &preview_eval_body(body, bindings, "presentation compatibility view_scale")?,
            "presentation compatibility view_scale",
        )?,
        None => 0.72,
    };
    Ok(Some(
        crate::presentation_contract::LegacyCompatibilityProjectionInput {
            world_up,
            view_scale,
        },
    ))
}

pub fn preview_eval_body(
    body: &hir::Body,
    base_bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    let mut bindings = base_bindings.clone();
    let mut last_value = None;
    for stmt in &body.root_stmts {
        match &body.stmts[*stmt] {
            hir::Stmt::Expr(expr) => {
                last_value = Some(preview_eval_expr(body, *expr, &bindings, context)?);
            }
            hir::Stmt::Return(Some(expr)) => {
                return preview_eval_expr(body, *expr, &bindings, context);
            }
            hir::Stmt::Let { name, value, .. }
            | hir::Stmt::Assign {
                name,
                op: hir::AssignOp::Assign,
                value,
                ..
            } => {
                let value = preview_eval_expr(body, *value, &bindings, context)?;
                bindings.insert(name.clone(), value);
            }
            hir::Stmt::IgnoreResult { expr } => {
                preview_eval_expr(body, *expr, &bindings, context)?;
            }
            _ => {
                return Err(format!(
                    "{context} only supports literal, arithmetic, constructor, and member-expression bodies"
                ));
            }
        }
    }
    last_value.ok_or_else(|| format!("{context} requires a terminal expression"))
}

pub fn preview_eval_expr(
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
    bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(literal) => preview_literal_value(literal, context),
        hir::Expr::Variable(name) => bindings
            .get(name)
            .cloned()
            .ok_or_else(|| format!("{context} cannot resolve `{name}`")),
        hir::Expr::Unary { op, expr, .. } => {
            let value = preview_eval_expr(body, *expr, bindings, context)?;
            preview_apply_unary(*op, value, context)
        }
        hir::Expr::Binary { lhs, op, rhs, .. } => {
            let lhs = preview_eval_expr(body, *lhs, bindings, context)?;
            let rhs = preview_eval_expr(body, *rhs, bindings, context)?;
            preview_apply_binary(lhs, *op, rhs, context)
        }
        hir::Expr::Call { callee, args, .. } => {
            let hir::Expr::Variable(name) = &body.exprs[*callee] else {
                return Err(format!(
                    "{context} does not support indirect preview-evaluation calls"
                ));
            };
            if name == "capture" {
                let Some(target_expr) = preview_named_or_pos_expr(args, "scene", 0) else {
                    return Err(format!("{context} is missing `scene` for capture"));
                };
                let Some(region_name) = preview_capture_region_name(body, target_expr) else {
                    return Err(format!(
                        "{context} could not resolve the capture scene target"
                    ));
                };
                return Ok(crate::kernel::KernelValue::Capture(region_name));
            }
            preview_eval_call(name, body, args, bindings, context)
        }
        hir::Expr::Member { object, member, .. } => {
            let object = preview_eval_expr(body, *object, bindings, context)?;
            preview_struct_field(&object, member, context)
        }
        _ => Err(format!(
            "{context} only supports literal, arithmetic, constructor, and member expressions"
        )),
    }
}

pub fn preview_eval_call(
    callee: &SmolStr,
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    let (positional, mut named) = preview_eval_call_arguments(body, args, bindings, context)?;
    match callee.as_str() {
        "vec3" => Ok(crate::kernel::KernelValue::Vec3([
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "x", 0, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "y", 1, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "z", 2, context)?,
                context,
            )?,
        ])),
        "normalize" => {
            let value = preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?;
            Ok(crate::kernel::KernelValue::Vec3(normalize_preview_vec3(
                preview_expect_vec3(&value, context)?,
            )))
        }
        "Light" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let direction = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "direction", 1, context)?,
                context,
            )?;
            let intensity = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "intensity", 2, context)?,
                context,
            )?;
            let range = preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "range", 3, context)?,
                context,
            )?;
            Ok(crate::presentation_exec::light_value(
                crate::presentation_contract::CanonicalLightInput {
                    position,
                    direction,
                    intensity,
                    range,
                },
            ))
        }
        "Camera" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let forward = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "forward", 1, context)?,
                context,
            )?;
            let up = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "up", 2, context)?,
                context,
            )?;
            let vertical_fov_degrees = preview_expect_f32(
                &preview_named_or_pos_value(
                    &mut named,
                    &positional,
                    "vertical_fov_degrees",
                    3,
                    context,
                )?,
                context,
            )?;
            Ok(preview_camera_value(
                crate::presentation_contract::CanonicalCameraInput {
                    position,
                    forward,
                    up,
                    vertical_fov_degrees,
                },
            ))
        }
        "f32" => Ok(crate::kernel::KernelValue::F32(preview_expect_f32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "i32" => Ok(crate::kernel::KernelValue::I32(preview_expect_i32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "u32" => Ok(crate::kernel::KernelValue::U32(preview_expect_u32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        _ => Err(format!(
            "{context} does not support preview evaluation for call `{callee}`"
        )),
    }
}

pub fn preview_eval_call_arguments(
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &HashMap<SmolStr, crate::kernel::KernelValue>,
    context: &str,
) -> Result<
    (
        Vec<crate::kernel::KernelValue>,
        HashMap<SmolStr, crate::kernel::KernelValue>,
    ),
    String,
> {
    let mut positional = Vec::new();
    let mut named = HashMap::new();
    for arg in args {
        match arg {
            hir::Arg::Positional { value, .. } => {
                positional.push(preview_eval_expr(body, *value, bindings, context)?);
            }
            hir::Arg::Named { name, value, .. } => {
                named.insert(
                    name.clone(),
                    preview_eval_expr(body, *value, bindings, context)?,
                );
            }
        }
    }
    Ok((positional, named))
}

pub fn preview_named_or_pos_expr(
    args: &[hir::Arg],
    name: &str,
    index: usize,
) -> Option<hir::Idx<hir::Expr>> {
    args.iter()
        .find_map(|arg| match arg {
            hir::Arg::Named {
                name: arg_name,
                value,
                ..
            } if arg_name == name => Some(*value),
            _ => None,
        })
        .or_else(|| {
            args.iter()
                .filter_map(|arg| match arg {
                    hir::Arg::Positional { value, .. } => Some(*value),
                    _ => None,
                })
                .nth(index)
        })
}

pub fn preview_named_or_pos_value(
    named: &mut HashMap<SmolStr, crate::kernel::KernelValue>,
    positional: &[crate::kernel::KernelValue],
    name: &str,
    index: usize,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    named
        .remove(name)
        .or_else(|| positional.get(index).cloned())
        .ok_or_else(|| format!("{context} is missing `{name}`"))
}

pub fn preview_capture_region_name(
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<SmolStr> {
    match &body.exprs[expr_id] {
        hir::Expr::Variable(name) => Some(name.clone()),
        hir::Expr::Call { callee, .. } => match &body.exprs[*callee] {
            hir::Expr::Variable(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub fn preview_literal_value(
    literal: &hir::Literal,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    match literal {
        hir::Literal::Integer(value) => Ok(crate::kernel::KernelValue::I32(*value as i32)),
        hir::Literal::Float(value) => Ok(crate::kernel::KernelValue::F32(*value as f32)),
        hir::Literal::Boolean(value) => Ok(crate::kernel::KernelValue::Bool(*value)),
        _ => Err(format!("{context} does not support that literal kind")),
    }
}

pub fn preview_apply_unary(
    op: hir::UnaryOp,
    value: crate::kernel::KernelValue,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    match (op, value) {
        (hir::UnaryOp::Neg, crate::kernel::KernelValue::I32(value)) => {
            Ok(crate::kernel::KernelValue::I32(-value))
        }
        (hir::UnaryOp::Neg, crate::kernel::KernelValue::F32(value)) => {
            Ok(crate::kernel::KernelValue::F32(-value))
        }
        (hir::UnaryOp::Neg, crate::kernel::KernelValue::Vec3(value)) => {
            Ok(crate::kernel::KernelValue::Vec3([
                -value[0], -value[1], -value[2],
            ]))
        }
        _ => Err(format!("{context} does not support that unary operation")),
    }
}

pub fn preview_apply_binary(
    lhs: crate::kernel::KernelValue,
    op: hir::BinaryOp,
    rhs: crate::kernel::KernelValue,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    match op {
        hir::BinaryOp::Add => match (&lhs, &rhs) {
            (crate::kernel::KernelValue::Vec3(lhs), crate::kernel::KernelValue::Vec3(rhs)) => {
                Ok(crate::kernel::KernelValue::Vec3([
                    lhs[0] + rhs[0],
                    lhs[1] + rhs[1],
                    lhs[2] + rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs + rhs, |lhs, rhs| lhs + rhs),
        },
        hir::BinaryOp::Sub => match (&lhs, &rhs) {
            (crate::kernel::KernelValue::Vec3(lhs), crate::kernel::KernelValue::Vec3(rhs)) => {
                Ok(crate::kernel::KernelValue::Vec3([
                    lhs[0] - rhs[0],
                    lhs[1] - rhs[1],
                    lhs[2] - rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs - rhs, |lhs, rhs| lhs - rhs),
        },
        hir::BinaryOp::Mul => match (&lhs, &rhs) {
            (crate::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(crate::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            (scalar, crate::kernel::KernelValue::Vec3(value)) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(crate::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs * rhs, |lhs, rhs| lhs * rhs),
        },
        hir::BinaryOp::Div => match (&lhs, &rhs) {
            (crate::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(crate::kernel::KernelValue::Vec3([
                    value[0] / scalar,
                    value[1] / scalar,
                    value[2] / scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs / rhs, |lhs, rhs| lhs / rhs),
        },
        _ => Err(format!("{context} does not support that binary operation")),
    }
}

pub fn preview_numeric_binary(
    lhs: crate::kernel::KernelValue,
    rhs: crate::kernel::KernelValue,
    integer_op: impl FnOnce(i32, i32) -> i32,
    float_op: impl FnOnce(f32, f32) -> f32,
) -> Result<crate::kernel::KernelValue, String> {
    match (&lhs, &rhs) {
        (crate::kernel::KernelValue::I32(lhs), crate::kernel::KernelValue::I32(rhs)) => {
            Ok(crate::kernel::KernelValue::I32(integer_op(*lhs, *rhs)))
        }
        _ => Ok(crate::kernel::KernelValue::F32(float_op(
            preview_scalar_f32(&lhs)?,
            preview_scalar_f32(&rhs)?,
        ))),
    }
}

pub fn preview_scalar_f32(value: &crate::kernel::KernelValue) -> Result<f32, String> {
    match value {
        crate::kernel::KernelValue::I32(value) => Ok(*value as f32),
        crate::kernel::KernelValue::U32(value) => Ok(*value as f32),
        crate::kernel::KernelValue::F32(value) => Ok(*value),
        _ => Err("expected a scalar numeric value".to_string()),
    }
}

pub fn preview_struct_field(
    value: &crate::kernel::KernelValue,
    field_name: &str,
    context: &str,
) -> Result<crate::kernel::KernelValue, String> {
    let crate::kernel::KernelValue::Struct(record) = value else {
        return Err(format!(
            "{context} expected a struct value for .{field_name}"
        ));
    };
    record
        .fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, value)| value.clone())
        .ok_or_else(|| format!("{context} could not find field `{field_name}`"))
}

pub fn preview_expect_f32(
    value: &crate::kernel::KernelValue,
    context: &str,
) -> Result<f32, String> {
    preview_scalar_f32(value).map_err(|_| format!("{context} expected an f32-compatible value"))
}

pub fn preview_expect_i32(
    value: &crate::kernel::KernelValue,
    context: &str,
) -> Result<i32, String> {
    match value {
        crate::kernel::KernelValue::I32(value) => Ok(*value),
        crate::kernel::KernelValue::U32(value) => Ok(*value as i32),
        crate::kernel::KernelValue::F32(value) => Ok(*value as i32),
        _ => Err(format!("{context} expected an i32-compatible value")),
    }
}

pub fn preview_expect_u32(
    value: &crate::kernel::KernelValue,
    context: &str,
) -> Result<u32, String> {
    match value {
        crate::kernel::KernelValue::I32(value) => Ok((*value).max(0) as u32),
        crate::kernel::KernelValue::U32(value) => Ok(*value),
        crate::kernel::KernelValue::F32(value) => Ok(value.max(0.0) as u32),
        _ => Err(format!("{context} expected a u32-compatible value")),
    }
}

pub fn preview_expect_vec3(
    value: &crate::kernel::KernelValue,
    context: &str,
) -> Result<[f32; 3], String> {
    match value {
        crate::kernel::KernelValue::Vec3(value) => Ok(*value),
        _ => Err(format!("{context} expected a vec3 value")),
    }
}

pub fn preview_expect_light(
    value: &crate::kernel::KernelValue,
    context: &str,
) -> Result<crate::presentation_contract::CanonicalLightInput, String> {
    let position =
        preview_expect_vec3(&preview_struct_field(value, "position", context)?, context)?;
    let direction =
        preview_expect_vec3(&preview_struct_field(value, "direction", context)?, context)?;
    let intensity =
        preview_expect_vec3(&preview_struct_field(value, "intensity", context)?, context)?;
    let range = preview_expect_f32(&preview_struct_field(value, "range", context)?, context)?;
    Ok(crate::presentation_contract::CanonicalLightInput {
        position,
        direction,
        intensity,
        range,
    })
}

pub fn preview_camera_value(
    camera: crate::presentation_contract::CanonicalCameraInput,
) -> crate::kernel::KernelValue {
    crate::kernel::KernelValue::Struct(crate::kernel::KernelStructValue {
        name: SmolStr::new("Camera"),
        fields: vec![
            (
                SmolStr::new("position"),
                crate::kernel::KernelValue::Vec3(camera.position),
            ),
            (
                SmolStr::new("forward"),
                crate::kernel::KernelValue::Vec3(camera.forward),
            ),
            (
                SmolStr::new("up"),
                crate::kernel::KernelValue::Vec3(camera.up),
            ),
            (
                SmolStr::new("vertical_fov_degrees"),
                crate::kernel::KernelValue::F32(camera.vertical_fov_degrees),
            ),
        ],
    })
}

pub fn default_preview_key_light() -> crate::presentation_contract::CanonicalLightInput {
    crate::presentation_contract::CanonicalLightInput {
        position: [2.4, 2.8, 2.4],
        direction: normalize_preview_vec3([-0.8, -0.9, -0.9]),
        intensity: [1.0, 0.98, 0.95],
        range: 12.0,
    }
}

pub fn normalize_preview_vec3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
    if len_sq <= f32::EPSILON {
        return value;
    }
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}

pub fn body_called_function_name(body: &hir::Body) -> Option<SmolStr> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some(name.clone())
}

pub fn body_terminal_expr_id(body: &hir::Body) -> Option<hir::Idx<hir::Expr>> {
    let stmt = body.root_stmts.last()?;
    match body.stmts[*stmt] {
        hir::Stmt::Expr(expr) => Some(expr),
        hir::Stmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

pub fn body_terminal_call_args<'a>(body: &'a hir::Body) -> Option<(&'a SmolStr, &'a [hir::Arg])> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, args, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some((name, args.as_slice()))
}

pub fn helper_call_named_expr_id(
    body: &hir::Body,
    helper_name: &str,
    arg_name: &str,
) -> Option<hir::Idx<hir::Expr>> {
    let (callee, args) = body_terminal_call_args(body)?;
    if callee != helper_name {
        return None;
    }
    args.iter().find_map(|arg| match arg {
        hir::Arg::Named { name, value, .. } if name == arg_name => Some(*value),
        _ => None,
    })
}

pub fn resolve_view_dimension(
    view: &hir::Function,
    override_value: Option<u32>,
    width: bool,
) -> Result<u32, String> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    let metadata = view
        .presentation
        .as_ref()
        .ok_or_else(|| "selected view is missing presentation metadata".to_string())?;
    let label = if width { "width" } else { "height" };
    if let Some(viewport_body) = metadata.view.viewport.as_ref()
        && let Some(value) = helper_call_named_expr_id(viewport_body, "viewport", label)
    {
        return eval_expr_u32(viewport_body, value).ok_or_else(|| {
            format!(
                "presentation execution cannot evaluate non-literal view {label}; pass --{label} explicitly"
            )
        });
    }
    let body = if width {
        metadata.view.width.as_ref()
    } else {
        metadata.view.height.as_ref()
    }
    .ok_or_else(|| {
        format!("presentation execution requires --{label} when the view omits {label}")
    })?;
    eval_body_u32(body).ok_or_else(|| {
        format!(
            "presentation execution cannot evaluate non-literal view {label}; pass --{label} explicitly"
        )
    })
}

pub fn eval_body_u32(body: &hir::Body) -> Option<u32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_u32(body, expr_id)
}

pub fn eval_body_i32_in_module(module: &hir::Module, body: &hir::Body) -> Option<i32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_i32_in_module(module, body, expr_id)
}

pub fn eval_expr_i32_in_module(
    module: &hir::Module,
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<i32> {
    eval_expr_f32_in_module(module, body, expr_id).map(|value| value as i32)
}

pub fn eval_body_f32_in_module(module: &hir::Module, body: &hir::Body) -> Option<f32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_f32_in_module(module, body, expr_id)
}

pub fn eval_expr_u32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<u32> {
    eval_expr_f32(body, expr_id).map(|value| value.max(0.0) as u32)
}

pub fn eval_expr_f32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<f32> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as f32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as f32),
        _ => None,
    }
}

pub fn eval_expr_f32_in_module(
    module: &hir::Module,
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<f32> {
    fn eval(
        module: &hir::Module,
        body: &hir::Body,
        expr_id: hir::Idx<hir::Expr>,
        stack: &mut HashSet<SmolStr>,
    ) -> Option<f32> {
        match &body.exprs[expr_id] {
            hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as f32),
            hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as f32),
            hir::Expr::Unary {
                op: hir::body::UnaryOp::Neg,
                expr,
                ..
            } => eval(module, body, *expr, stack).map(|value| -value),
            hir::Expr::Call {
                callee,
                args,
                type_args,
            } if args.is_empty() && type_args.is_empty() => {
                let hir::Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                if !stack.insert(name.clone()) {
                    return None;
                }
                let value = module
                    .functions
                    .iter()
                    .find(|(_, func)| func.name == *name && func.params.is_empty())
                    .and_then(|(_, func)| func.body.as_ref())
                    .and_then(|helper_body| {
                        let helper_expr = body_terminal_expr_id(helper_body)?;
                        eval(module, helper_body, helper_expr, stack)
                    });
                stack.remove(name);
                value
            }
            _ => None,
        }
    }

    let mut stack = HashSet::new();
    eval(module, body, expr_id, &mut stack)
}

pub fn domain_execution_inputs(
    module: &hir::Module,
    domain: &hir::Function,
    region_name: &SmolStr,
    query_backend: crate::query_plan::DispatchBackend,
) -> Result<DomainExecutionInputs, String> {
    let metadata = domain.domain.as_ref().expect("domain metadata");
    let geometry_detail = match metadata.geometry_detail {
        hir::DomainGeometryDetail::Coarse => 0,
        hir::DomainGeometryDetail::Fine => 1,
    };
    let _ = query_backend;
    let policy_metadata = domain.domain_execution_policy.as_ref().ok_or_else(|| {
        format!(
            "domain `{}` is missing lowered execution policy metadata",
            domain.name
        )
    })?;
    let primary_rays = crate::presentation_exec::RayBudgetPolicy {
        max_distance: authored_domain_f32(module, policy_metadata.max_distance.as_ref())
            .unwrap_or(16.0),
        min_step: authored_domain_f32(module, policy_metadata.min_step.as_ref()).unwrap_or(0.01),
        hit_epsilon: authored_domain_f32(module, policy_metadata.hit_epsilon.as_ref())
            .unwrap_or(0.001),
        max_steps: authored_domain_i32(module, policy_metadata.max_steps.as_ref()).unwrap_or(128),
    };
    let execution_policy = crate::presentation_exec::PresentationExecutionPolicy::new(
        policy_metadata.required_guarantee,
        policy_metadata.selected_method,
        primary_rays,
    );
    Ok(DomainExecutionInputs {
        frame_domain: crate::presentation_exec::scene_domain_value(
            crate::query_exec::stable_region_scene_capture_id(region_name),
            geometry_detail,
            metadata.material,
            metadata.radiance,
            metadata.media,
        ),
        semantic_domain: crate::presentation_exec::render_semantic_domain_report(
            crate::query_exec::stable_region_scene_capture_id(region_name),
            geometry_detail,
            metadata.material,
            metadata.radiance,
            metadata.media,
        ),
        execution_policy,
    })
}

pub fn authored_domain_f32(module: &hir::Module, body: Option<&hir::Body>) -> Option<f32> {
    body.and_then(|body| eval_body_f32_in_module(module, body))
}

pub fn authored_domain_i32(module: &hir::Module, body: Option<&hir::Body>) -> Option<i32> {
    body.and_then(|body| eval_body_i32_in_module(module, body))
}

fn render_live_state(
    config: &FrameLiveLaunchConfig,
    generation: u64,
) -> Result<LiveFrameState, FrameLiveError> {
    let ready = load_prepared_presentation_execution(
        &config.entry_path,
        config.query_backend.into_dispatch_backend(),
        config.view.as_deref(),
        config.region.as_deref(),
        config.domain.as_deref(),
        config.camera.canonical(),
        config.width,
        config.height,
        config.frame_index,
        config.delta_seconds,
        crate::query_exec::QueryTraceSolverMode::Hybrid,
    )?;
    let result = crate::presentation_exec::execute_plan(
        &ready.bundle.query_ctx,
        &ready.prepared.plan,
        &ready.prepared.input,
    )
    .map_err(|err| FrameLiveError::codegen(format!("presentation execution error: {err}")))?;
    let color_buffer = color_buffer_for_result(&result)
        .map_err(|err| FrameLiveError::codegen(format!("frame-live color export error: {err}")))?;
    Ok(LiveFrameState {
        ready,
        result,
        color_buffer,
        generation,
    })
}

fn color_buffer_for_result(
    result: &crate::presentation_exec::PresentationExecutionResult,
) -> Result<Vec<u32>, String> {
    let color_attachment = crate::presentation_exec::debug::attachment_name_for_kind(
        result,
        FrameAttachmentKind::Color,
    )
    .map_err(|err| err.to_string())?;
    let pixels = result
        .attachments
        .decode_attachment(color_attachment)
        .map_err(|err| err.to_string())?;
    Ok(pixels
        .into_iter()
        .map(|value| match value {
            KernelValue::Vec3(color) => {
                let r = encode_color_lane(color[0]) as u32;
                let g = encode_color_lane(color[1]) as u32;
                let b = encode_color_lane(color[2]) as u32;
                (r << 16) | (g << 8) | b
            }
            _ => 0,
        })
        .collect())
}

fn encode_color_lane(value: f32) -> u8 {
    value.clamp(0.0, 255.0).round() as u8
}

fn map_axis(cursor: f64, window_extent: f64, frame_extent: u32) -> u32 {
    if frame_extent <= 1 || !window_extent.is_finite() || window_extent <= 0.0 {
        return 0;
    }
    let normalized = (cursor / window_extent).clamp(0.0, 1.0);
    let unclamped = (normalized * frame_extent as f64).floor() as u32;
    unclamped.min(frame_extent.saturating_sub(1))
}

fn source_ref_priority(kind: SourceRefKind) -> u8 {
    match kind {
        SourceRefKind::Field => 0,
        SourceRefKind::Shape => 1,
        SourceRefKind::Region => 2,
    }
}

#[derive(Debug, Clone, Copy)]
struct HitDetails {
    hit: bool,
    position: [f32; 3],
    normal: [f32; 3],
    root_shape_id: u32,
    feature_id: u32,
    instance_id: u32,
    repeat_id: u32,
}

impl HitDetails {
    fn shape_name<'a>(&self, bundle: &'a CompiledPresentationBundle) -> Option<&'a SmolStr> {
        self.hit.then(|| {
            bundle
                .query_ctx
                .shape_name_for_root_feature_id(self.root_shape_id)
        })?
    }

    fn field_name<'a>(&self, bundle: &'a CompiledPresentationBundle) -> Option<&'a SmolStr> {
        let shape_name = self.shape_name(bundle)?;
        let leaf_ref = bundle
            .query_ctx
            .scene
            .shapes
            .get(shape_name)?
            .feature_leaves
            .get(&self.feature_id)?;
        bundle
            .query_ctx
            .scene
            .shapes
            .get(&leaf_ref.scene)?
            .leaves
            .get(&leaf_ref.leaf)
            .map(|leaf| &leaf.field)
    }
}

fn hit_details(value: &KernelValue) -> Result<HitDetails, String> {
    let hit = expect_bool(hit_field(value, "hit")?, "hit")?;
    Ok(HitDetails {
        hit,
        position: expect_vec3(hit_field(value, "position")?, "position")?,
        normal: expect_vec3(hit_field(value, "normal")?, "normal")?,
        root_shape_id: expect_u32(hit_field(value, "root_shape_id")?, "root_shape_id")?,
        feature_id: expect_u32(hit_field(value, "feature_id")?, "feature_id")?,
        instance_id: expect_u32(hit_field(value, "instance_id")?, "instance_id")?,
        repeat_id: expect_u32(hit_field(value, "repeat_id")?, "repeat_id")?,
    })
}

fn hit_field<'a>(value: &'a KernelValue, name: &str) -> Result<&'a KernelValue, String> {
    let KernelValue::Struct(record) = value else {
        return Err(format!("expected Hit3 record, found {value:?}"));
    };
    record
        .fields
        .iter()
        .find(|(field_name, _)| field_name == name)
        .map(|(_, value)| value)
        .ok_or_else(|| format!("missing Hit3 field `{name}`"))
}

fn expect_bool(value: &KernelValue, name: &str) -> Result<bool, String> {
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(format!("expected bool for `{name}`, found {other:?}")),
    }
}

fn expect_u32(value: &KernelValue, name: &str) -> Result<u32, String> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(format!("expected u32 for `{name}`, found {other:?}")),
    }
}

fn expect_vec3(value: &KernelValue, name: &str) -> Result<[f32; 3], String> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(format!("expected vec3 for `{name}`, found {other:?}")),
    }
}

fn field_source_ref(
    bundle: &CompiledPresentationBundle,
    field_name: &SmolStr,
) -> Option<SelectionSourceRef> {
    let function = bundle
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == *field_name)
        .map(|(_, func)| func)?;
    let span = function.name_span?;
    let path = bundle
        .provenance
        .function_owner_path_by_name
        .get(field_name)?;
    source_ref_from_path_and_offset(
        bundle,
        SourceRefKind::Field,
        field_name.as_str(),
        path,
        span.start().into(),
    )
}

fn region_source_ref(
    bundle: &CompiledPresentationBundle,
    region_name: &SmolStr,
) -> Option<SelectionSourceRef> {
    let function = bundle
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == *region_name)
        .map(|(_, func)| func)?;
    let span = function.name_span?;
    let path = bundle
        .provenance
        .function_owner_path_by_name
        .get(region_name)?;
    source_ref_from_path_and_offset(
        bundle,
        SourceRefKind::Region,
        region_name.as_str(),
        path,
        span.start().into(),
    )
}

fn shape_source_ref(
    bundle: &CompiledPresentationBundle,
    shape_name: &SmolStr,
) -> Option<SelectionSourceRef> {
    let shape = bundle
        .module
        .shapes
        .iter()
        .find(|(_, shape)| shape.name == *shape_name)
        .map(|(_, shape)| shape)?;
    let span = shape.name_span?;
    let path = bundle.provenance.shape_owner_path_by_name.get(shape_name)?;
    source_ref_from_path_and_offset(
        bundle,
        SourceRefKind::Shape,
        shape_name.as_str(),
        path,
        span.start().into(),
    )
}

fn source_ref_from_path_and_offset(
    bundle: &CompiledPresentationBundle,
    kind: SourceRefKind,
    symbol: &str,
    path: &PathBuf,
    offset: usize,
) -> Option<SelectionSourceRef> {
    let source = bundle.module_sources.get(path)?;
    let (line, column) = line_col_at_offset(source, offset);
    Some(SelectionSourceRef {
        kind,
        symbol: symbol.to_string(),
        path: path.clone(),
        line,
        column,
    })
}

fn line_col_at_offset(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let mut line = 1usize;
    let mut column = 1usize;
    for byte in source.as_bytes().iter().take(clamped) {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn first_hit_pixel(
    result: &crate::presentation_exec::PresentationExecutionResult,
) -> Option<FramePixel> {
    let hits = result.attachments.decode_attachment("primary_hit").ok()?;
    hits.iter().position(is_hit_value).map(|index| FramePixel {
        x: (index as u32) % result.width.max(1),
        y: (index as u32) / result.width.max(1),
    })
}

fn is_hit_value(value: &KernelValue) -> bool {
    hit_details(value).map(|hit| hit.hit).unwrap_or(false)
}

fn project_diag_record(
    kind: ProjectDiagKind,
    severity: DiagSeverity,
    message: String,
    path: String,
    span: SourceSpan,
) -> DiagRecord {
    let descriptor = project_descriptor(kind);
    DiagRecord::new(descriptor.stage, severity, message, path, span)
        .with_code(Some(descriptor.code.to_string()))
        .with_help(Some(descriptor.help_template.to_string()))
}

fn resolve_path_from_owner_spans(
    span: SourceSpan,
    provenance: &hir::project::ProjectProvenance,
    default_path: &str,
) -> String {
    let offset = span.offset();
    let mut candidates = provenance
        .function_owner_span_by_id
        .iter()
        .filter_map(|(function_id, owner_span)| {
            let start = usize::from(owner_span.start());
            let end = usize::from(owner_span.end());
            if offset >= start && offset <= end {
                provenance
                    .function_owner_path_by_id
                    .get(function_id)
                    .map(|path| (end.saturating_sub(start), path.display().to_string()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(width, _)| *width);
    candidates
        .first()
        .map(|(_, path)| path.clone())
        .unwrap_or_else(|| default_path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn window_click_mapping_preserves_top_left_origin_and_clamps() {
        assert_eq!(
            map_window_click_to_frame_pixel((0.0, 0.0), (320.0, 180.0), (640, 360)),
            FramePixel { x: 0, y: 0 }
        );
        assert_eq!(
            map_window_click_to_frame_pixel((10.25, 20.75), (320.0, 180.0), (640, 360)),
            FramePixel { x: 20, y: 41 }
        );
        assert_eq!(
            map_window_click_to_frame_pixel((320.0, 180.0), (320.0, 180.0), (640, 360)),
            FramePixel { x: 639, y: 359 }
        );
        assert_eq!(
            frame_sample_index(FramePixel { x: 639, y: 359 }, 640),
            230_399
        );
    }

    #[test]
    fn source_precedence_prefers_field_then_shape_then_region() {
        let region = SelectionSourceRef {
            kind: SourceRefKind::Region,
            symbol: "scene_region".into(),
            path: PathBuf::from("/tmp/scene.wr"),
            line: 42,
            column: 5,
        };
        let shape = SelectionSourceRef {
            kind: SourceRefKind::Shape,
            symbol: "scene_shape".into(),
            path: PathBuf::from("/tmp/scene.wr"),
            line: 28,
            column: 7,
        };
        let field = SelectionSourceRef {
            kind: SourceRefKind::Field,
            symbol: "sample_sphere".into(),
            path: PathBuf::from("/tmp/scene.wr"),
            line: 3,
            column: 1,
        };

        assert_eq!(
            choose_primary_source_ref(&[region.clone(), shape.clone(), field.clone()]),
            Some(field.clone())
        );
        assert_eq!(
            choose_primary_source_ref(&[region.clone(), shape.clone()]),
            Some(shape)
        );
        assert_eq!(choose_primary_source_ref(&[region.clone()]), Some(region));
        assert_eq!(choose_primary_source_ref(&[]), None);
    }

    #[test]
    fn launch_config_round_trips_as_json() {
        let config = FrameLiveLaunchConfig {
            entry_path: PathBuf::from("/tmp/world/src/main.wr"),
            view: Some("main_view".to_string()),
            region: Some("scene_region".to_string()),
            domain: Some("scene_domain".to_string()),
            camera: FrameLiveCameraConfig {
                position: [1.0, 2.0, 3.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 60.0,
            },
            width: Some(640),
            height: Some(360),
            frame_index: 7,
            delta_seconds: 1.0 / 60.0,
            query_backend: FrameLiveQueryBackend::Cpu,
        };
        let json = serde_json::to_string(&config).expect("serialize config");
        let decoded: FrameLiveLaunchConfig =
            serde_json::from_str(&json).expect("deserialize config");
        assert_eq!(decoded, config);
    }

    #[test]
    fn watched_source_paths_follow_exact_loaded_module_sources() {
        let temp = frame_live_tempdir();
        let project_root = temp.path();
        let entry = project_root.join("src").join("main.wr");
        let import = project_root.join("shared").join("terrain.wr");
        let ignored = project_root.join("src").join("notes.txt");
        fs::create_dir_all(entry.parent().expect("entry parent")).expect("create src");
        fs::create_dir_all(import.parent().expect("import parent")).expect("create shared");
        fs::write(&entry, "// entry").expect("write entry");
        fs::write(&import, "// import").expect("write import");
        fs::write(&ignored, "ignore me").expect("write ignored");

        let mut module_sources = HashMap::new();
        module_sources.insert(entry.clone(), "// entry".to_string());
        module_sources.insert(import.clone(), "// import".to_string());
        module_sources.insert(ignored, "ignore me".to_string());

        let watched = watched_source_paths(&module_sources);
        let mut expected = vec![entry, import];
        expected.sort();

        assert_eq!(watched, expected);
    }

    #[test]
    fn sources_changed_detects_explicit_watched_file_updates() {
        let temp = frame_live_tempdir();
        let watched = temp.path().join("shared").join("terrain.wr");
        fs::create_dir_all(watched.parent().expect("watched parent")).expect("create parent");
        fs::write(&watched, "before").expect("write watched file");
        let watched_paths = vec![watched.clone()];
        let mut snapshot = snapshot_source_paths(&watched_paths);

        assert!(!sources_changed(&watched_paths, &mut snapshot));

        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&watched, "after").expect("rewrite watched file");

        assert!(sources_changed(&watched_paths, &mut snapshot));
    }

    #[test]
    fn selection_record_resolves_field_shape_and_region_sources_on_real_hit() {
        let temp = frame_live_tempdir();
        let entry = write_frame_live_fixture(temp.path());
        let ready = load_prepared_presentation_execution(
            &entry,
            crate::query_plan::DispatchBackend::Cpu,
            Some("cli_plan_view"),
            None,
            None,
            crate::presentation_contract::CanonicalCameraInput {
                position: [0.0, 0.0, 2.5],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 60.0,
            },
            Some(8),
            Some(8),
            0,
            1.0 / 60.0,
            crate::query_exec::QueryTraceSolverMode::Hybrid,
        )
        .expect("load prepared presentation execution");
        let result = crate::presentation_exec::execute_plan(
            &ready.bundle.query_ctx,
            &ready.prepared.plan,
            &ready.prepared.input,
        )
        .expect("execute plan");
        let hits = result
            .attachments
            .decode_attachment("primary_hit")
            .expect("primary_hit attachment");
        let hit_index = hits
            .iter()
            .position(hit_flag)
            .expect("expected at least one hit pixel");
        let pixel = FramePixel {
            x: (hit_index as u32) % result.width,
            y: (hit_index as u32) / result.width,
        };

        let record = selection_record_for_frame_pixel(
            &ready.bundle,
            &ready.region_name,
            &result,
            pixel,
            pixel,
            7,
        )
        .expect("selection record");

        assert!(record.hit);
        assert_eq!(record.region_name.as_deref(), Some("cli_plan_region"));
        assert_eq!(record.shape_name.as_deref(), Some("cli_plan_shape"));
        assert_eq!(record.field_name.as_deref(), Some("cli_plan_field"));
        assert_eq!(
            record.primary_source.as_ref().map(|source| source.kind),
            Some(SourceRefKind::Field)
        );
        assert!(
            record
                .source_refs
                .iter()
                .any(|source| source.kind == SourceRefKind::Field
                    && source.symbol == "cli_plan_field"
                    && source.path == entry
                    && source.line > 0
                    && source.column > 0)
        );
        assert!(
            record
                .source_refs
                .iter()
                .any(|source| source.kind == SourceRefKind::Shape
                    && source.symbol == "cli_plan_shape")
        );
        assert!(record.source_refs.iter().any(
            |source| source.kind == SourceRefKind::Region && source.symbol == "cli_plan_region"
        ));
    }

    fn hit_flag(value: &KernelValue) -> bool {
        match value {
            KernelValue::Struct(record) if record.name == SmolStr::new("Hit3") => record
                .fields
                .iter()
                .find(|(name, _)| name == "hit")
                .and_then(|(_, value)| match value {
                    KernelValue::Bool(value) => Some(*value),
                    _ => None,
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    fn frame_live_tempdir() -> TempDir {
        tempfile::Builder::new()
            .prefix("wrela-frame-live-")
            .tempdir_in(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .expect("workspace root"),
            )
            .expect("tempdir")
    }

    fn write_frame_live_fixture(root: &Path) -> PathBuf {
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let entry = src_dir.join("main.wr");
        fs::write(
            &entry,
            r#"
field exact distance cli_plan_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material cli_plan_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.8),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape cli_plan_shape {
    field = cli_plan_field
    material = cli_plan_material
}

region cli_plan_region() {
    place scene = cli_plan_shape
}

domain cli_plan_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 64
}

view cli_plan_view(world: RegionCapture, camera: Camera) {
    domain = cli_plan_domain(world = world)
    viewport = viewport(width = 8, height = 8)
    quality = realtime_quality(
        target_fps = 120,
        allow_dynamic_resolution = false,
        primary_max_steps = 48
    )
    lighting = key_light(
        light = Light(
            position = camera.position + vec3(0.5, 1.0, 0.5),
            direction = normalize(vec3(-0.4, -0.7, -0.2)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        )
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
}
"#,
        )
        .expect("write frame live fixture");
        entry
    }
}
