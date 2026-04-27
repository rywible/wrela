//! Reference host library and inspector model (RFC 0011 Phase 70 / C3).
//!
//! This crate is the only place where the runtime layer (`wgpu`, `winit`,
//! `cpal`, `RawInputRing`, `FrameInFlightSemaphore`) and the compiler layer
//! (`LiveEngineHost`, `EngineFrameReport`) are explicitly woven together.
//! All of the C3 acceptance pieces live here:
//!
//!   1. winit window + event loop          → [`ReferenceHostApp`]
//!   2. wgpu instance + surface + swapchain → [`SurfaceState`]
//!   3. raw input pump + drained sampler    → [`RawInputRing`] + [`RawInputRingLateSampler`]
//!   4. single-frame-in-flight pacing       → [`FrameInFlightSemaphore`]
//!
//! Layering doctrine: this crate is allowed to import both `wrela::` and
//! `wrela_runtime::` because it is the wiring layer; the runtime crate
//! itself never imports the compiler.

use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};
use wrela::audio_exec::{AudioSnapshotPublisher as RuntimeAudioSnapshotPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan};
use wrela::engine_frame::{
    AudioSnapshotPublisher, EngineFrameContext, EngineFrameError, EngineFrameReport,
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineGpuTimingPolicy, EngineJobAffinity,
    EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter,
    EngineSubsystemDescriptor, EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
    InputSubsystemAdapter, LiveEngineHost, LiveProjectConfig, PhysicsSubsystemAdapter,
    RawInputRingLateSampler, ResidencySubsystemAdapter, SaveAdapterFrameState, SavePublisher,
    SystemSubsystemAdapter,
};
use wrela::gpu_runtime::{GpuLimitRequest, GpuRuntimeContext};
use wrela::hir::{self, FunctionRole};
use wrela::input_contract::{InputFrame, InputMapBinding};
use wrela::input_map_plan::InputMapPlan;
use wrela::persistence::PersistenceProject;
use wrela::physics_contract::{PhysicsBodyDescriptor, PhysicsBodyId};
use wrela::physics_exec::{PhysicsBodyState, PhysicsSolver};
use wrela::physics_plan::PhysicsPlan;
use wrela::presentation_contract::{
    FrameContract, LightingContract, PresentationObservabilityProfile, RealtimeQualityContract,
    RealtimeQualityTier, ViewContract,
};
use wrela::presentation_exec::framegraph::PresentationFramegraph;
use wrela::presentation_exec::resources::AttachmentResourceSet;
use wrela::presentation_exec::swapchain::{AcquiredTexture, SwapchainError, SwapchainHandle};
use wrela::presentation_plan::{PresentationObservability, PresentationPlan};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::residency::follow::{FollowTarget, Transform3};
use wrela::residency::{
    RegionId, RegionLine, RegionResidencyService, ResidencyCandidate, ResidencyPolicy,
};
use wrela::state_advance::{
    ChangeClass, ChangeSummary, IdentityTransitionEvent, IdentityTransitionKind,
    StateAdvanceResult, WorldTransitionRecord,
};
use wrela::system_contract::{
    SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_exec::SystemMirInvoker;
use wrela::system_plan::{SystemPlan, SystemProgram};
use wrela_runtime::audio::voice::VoiceLedger;
use wrela_runtime::platform::frame_pacing::FrameInFlightSemaphore;
use wrela_runtime::platform::input::{RawInputKind, TimestampedRawEvent};
use wrela_runtime::platform::input_pump::{RawInputConsumer, RawInputProducer, RawInputRing};
use wrela_runtime::platform::surface::{
    RawPresentModePolicy, SurfaceExtent, select_wgpu_present_mode,
};
use wrela_runtime::platform::window::WindowConfig;

pub mod inspector;

#[derive(Debug, Clone)]
pub struct ReferenceProjectExecutor {
    project_label: SmolStr,
    function_count: usize,
    system_count: usize,
}

impl ReferenceProjectExecutor {
    fn reference_default() -> Self {
        Self {
            project_label: SmolStr::new("reference_host"),
            function_count: 0,
            system_count: 0,
        }
    }

    fn from_loaded_project(project_label: SmolStr, project: &hir::project::LoadedProject) -> Self {
        let mut function_count = 0usize;
        let mut system_count = 0usize;
        for (_, function) in project.module.functions.iter() {
            function_count += 1;
            if function.role == FunctionRole::System {
                system_count += 1;
            }
        }
        Self {
            project_label,
            function_count,
            system_count,
        }
    }

    fn from_entry_path(path: &PathBuf) -> Result<Self, String> {
        let project = hir::project::load_project_with_entrypoint(path, false)
            .map_err(|errors| format!("load project `{}`: {:?}", path.display(), errors))?;
        let label = path
            .file_stem()
            .and_then(|os| os.to_str())
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new("reference_host_project"));
        Ok(Self::from_loaded_project(label, &project))
    }
}

impl wrela::engine_frame::EngineStateAdvanceExecutor for ReferenceProjectExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(wrela::world_identity::SnapshotEpoch(
            previous.epoch().0.saturating_add(1),
        ));
        let event = IdentityTransitionEvent::new(
            IdentityTransitionKind::Preserved,
            self.project_label.clone(),
            Some(format!("{}:{}", previous.capture_name(), previous.epoch().0).into()),
            Some(format!("{}:{}", next.capture_name(), next.epoch().0).into()),
            format!(
                "reference project transition; functions={} systems={} inputs={}",
                self.function_count,
                self.system_count,
                input.inputs.inputs.len()
            ),
        );
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                vec![event],
            ),
            ChangeSummary::new(ChangeClass::Behavior, "reference project transition"),
        ))
    }
}

struct ReferenceSystemInvoker;

impl SystemMirInvoker for ReferenceSystemInvoker {
    fn invoke(&self, _mir_function_id: u32, _input: &InputFrame) -> Result<(), String> {
        Ok(())
    }
}

fn reference_input_map() -> InputMapPlan {
    InputMapPlan::new(
        "reference_host",
        vec![
            InputMapBinding::new("PrimaryAction", "mouse", "mouse.left"),
            InputMapBinding::new("PointerMove", "mouse", "mouse.move"),
            InputMapBinding::new("MoveForward", "keyboard", "key.KeyW"),
        ],
    )
    .expect("reference host input map")
}

fn reference_system_program() -> SystemProgram {
    SystemProgram::new([SystemPlan::new(
        SystemId::new("ReferenceHostObserveInput"),
        SystemContractId::new("reference_host.observe_input"),
        SystemPhase::PreSim,
        SystemAccessSummary::default()
            .reads(SystemResourceId::InputFrame)
            .reads(SystemResourceId::Snapshot)
            .writes(SystemResourceId::Resource(
                "reference_host.system_state".into(),
            )),
        1,
    )])
    .expect("reference host system program")
}

fn reference_residency_service() -> RegionResidencyService {
    RegionResidencyService::new(
        ResidencyPolicy {
            candidate_window: 10.0,
            ..ResidencyPolicy::default()
        },
        Box::new(RegionLine {
            regions: vec![ResidencyCandidate {
                region_id: RegionId::new("reference_origin"),
                center: [0.0, 0.0, 0.0],
                bytes: 128,
                compatibility_hash: 1,
            }],
        }),
    )
}

fn reference_physics_solver() -> PhysicsSolver {
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    PhysicsSolver::new(
        PhysicsPlan::cpu(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.1, 0.0])],
    )
}

fn reference_persistence_project() -> PersistenceProject {
    PersistenceProject {
        project_id: "reference_host".into(),
        wrela_version: "reference-host".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::new(),
    }
}

#[derive(Clone)]
struct ReferenceLiveControls {
    save_frame_state: Arc<Mutex<SaveAdapterFrameState>>,
}

impl ReferenceLiveControls {
    fn request_save(&self, request: bool) {
        if let Ok(mut state) = self.save_frame_state.lock() {
            state.request = request;
        }
    }
}

type PresentationSurfaceSlot = Arc<Mutex<Option<Arc<Mutex<SurfaceState>>>>>;

fn reference_live_subsystems(
    runtime: &EngineFrameRuntime,
    presentation_surface: PresentationSurfaceSlot,
    save_requested: bool,
) -> (Vec<Box<dyn EngineSubsystemAdapter>>, ReferenceLiveControls) {
    let input = InputSubsystemAdapter::new(
        reference_input_map(),
        runtime.materialized_tick_input_slot(),
    );
    let system = SystemSubsystemAdapter::with_invoker(
        reference_system_program(),
        input.shared_frame(),
        Arc::new(ReferenceSystemInvoker),
    );
    let residency = ResidencySubsystemAdapter::with_state_outcome(
        reference_residency_service(),
        FollowTarget {
            transform: Transform3 {
                translation: [0.0, 0.0, 0.0],
            },
            velocity: None,
        },
        runtime.state_advance_outcome_slot(),
    );
    let physics = PhysicsSubsystemAdapter::new(reference_physics_solver(), 1.0 / 60.0);
    let audio = AudioSnapshotPublisher::new(
        RuntimeAudioSnapshotPublisher::new(AudioConfig::default(), Arc::new(VoiceLedger::new())),
        AudioDspPlan {
            voices: vec![sine_voice(1, 1, 1.0)],
        },
        0,
    );
    let save = SavePublisher::with_state_outcome(
        save_requested,
        runtime.state_advance_outcome_slot(),
        reference_persistence_project(),
        0,
        0,
        Vec::new(),
    );
    let controls = ReferenceLiveControls {
        save_frame_state: save.frame_state(),
    };
    (
        vec![
            Box::new(input),
            Box::new(system),
            Box::new(residency),
            Box::new(physics),
            Box::new(audio),
            Box::new(ReferencePresentationAdapter::with_surface_slot(
                presentation_surface,
            )),
            Box::new(save),
        ],
        controls,
    )
}

pub fn new_headless_host() -> LiveEngineHost {
    new_headless_host_with_controls(false).0
}

fn new_headless_host_with_controls(
    save_requested: bool,
) -> (LiveEngineHost, ReferenceLiveControls) {
    new_headless_host_with_controls_and_executor(
        save_requested,
        ReferenceProjectExecutor::reference_default(),
    )
}

fn new_headless_host_with_controls_and_executor(
    save_requested: bool,
    executor: ReferenceProjectExecutor,
) -> (LiveEngineHost, ReferenceLiveControls) {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("reference_host_smoke"));
    let runtime = EngineFrameRuntime::new(Box::new(executor));
    let (subsystems, controls) =
        reference_live_subsystems(&runtime, Arc::new(Mutex::new(None)), save_requested);
    let mut host = LiveEngineHost::new_headless(
        runtime,
        LiveProjectConfig {
            scenario_id: "reference_host".into(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        },
        EngineFrameRuntimePolicy::live(),
        snapshot,
        60.0,
    );
    host.set_subsystems(subsystems);
    (host, controls)
}

/// Build a live host that drains the supplied raw input ring as its late
/// input sampler. Used by [`ReferenceHostApp`] so platform key/mouse events
/// pumped during a winit frame become first-class `TickInputEvent`s in the
/// next engine frame.
fn new_input_driven_host(
    consumer: RawInputConsumer,
    input_clock_origin: std::time::Instant,
    presentation_surface: PresentationSurfaceSlot,
    executor: ReferenceProjectExecutor,
) -> (LiveEngineHost, ReferenceLiveControls) {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("reference_host_interactive"));
    let runtime = EngineFrameRuntime::new(Box::new(executor));
    let (subsystems, controls) = reference_live_subsystems(&runtime, presentation_surface, false);
    let sampler = Arc::new(RawInputRingLateSampler::with_clock(
        consumer,
        Arc::new(move || {
            input_clock_origin
                .elapsed()
                .as_nanos()
                .min(u64::MAX as u128) as u64
        }),
    ));
    let mut host = LiveEngineHost::with_late_sampler(
        runtime,
        LiveProjectConfig {
            scenario_id: "reference_host".into(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        },
        EngineFrameRuntimePolicy::live(),
        snapshot,
        60.0,
        sampler,
    );
    host.set_subsystems(subsystems);
    (host, controls)
}

pub fn run_headless_smoke(frames: u32) -> Result<Vec<EngineFrameReport>, String> {
    let (mut host, controls) = new_headless_host_with_controls(false);
    run_headless_host_smoke(&mut host, &controls, frames)
}

pub fn run_headless_smoke_for_project(
    frames: u32,
    project_path: PathBuf,
) -> Result<Vec<EngineFrameReport>, String> {
    let executor = ReferenceProjectExecutor::from_entry_path(&project_path)?;
    let (mut host, controls) = new_headless_host_with_controls_and_executor(false, executor);
    run_headless_host_smoke(&mut host, &controls, frames)
}

fn run_headless_host_smoke(
    host: &mut LiveEngineHost,
    controls: &ReferenceLiveControls,
    frames: u32,
) -> Result<Vec<EngineFrameReport>, String> {
    let mut reports = Vec::new();
    for frame in 0..frames {
        controls.request_save(frame == 0);
        let tick = host
            .advance(1.0 / 60.0)
            .map_err(|e| format!("engine tick: {e}"))?;
        if tick.outputs.is_empty() {
            return Err("expected at least one simulation tick output".into());
        }
        for output in tick.outputs {
            if !output.report.violations.is_empty() {
                return Err(format!(
                    "unexpected violations: {:?}",
                    output.report.violations
                ));
            }
            reports.push(output.report);
        }
    }
    Ok(reports)
}

#[derive(Debug, Clone)]
pub struct ReferenceHostConfig {
    pub window: WindowConfig,
    pub frames: Option<u32>,
    pub project_path: Option<PathBuf>,
}

impl Default for ReferenceHostConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            frames: None,
            project_path: None,
        }
    }
}

pub fn run_interactive(config: ReferenceHostConfig) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|err| format!("event loop: {err}"))?;
    let mut app = ReferenceHostApp::new(config)?;
    event_loop
        .run_app(&mut app)
        .map_err(|err| format!("reference host app: {err}"))
}

/// All wgpu state that depends on a live winit window. Created lazily on
/// `resumed()` because winit `0.30` only allows window creation while we
/// hold an `&ActiveEventLoop`.
struct SurfaceState {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    gpu: Arc<GpuRuntimeContext>,
    config: wgpu::SurfaceConfiguration,
    selected_present_mode: wgpu::PresentMode,
    present_mode_was_downgraded: bool,
}

impl SurfaceState {
    fn create(window: Arc<Window>) -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|err| format!("create_surface: {err}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .map_err(|err| format!("request_adapter: {err:?}"))?;
        let adapter_info = adapter.get_info();
        let adapter_limits = adapter.limits();
        let requested_limits =
            wgpu::Limits::downlevel_defaults().using_resolution(adapter_limits.clone());
        let requested_features = wgpu::Features::empty();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("wrela-reference-host"),
            required_features: requested_features,
            required_limits: requested_limits.clone(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|err| format!("request_device: {err}"))?;
        let gpu = Arc::new(GpuRuntimeContext {
            device,
            queue,
            adapter_info,
            adapter_limits,
            requested_limits,
            requested_features,
            limit_request: GpuLimitRequest::default(),
            timestamp_support: false,
        });
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let physical = window.inner_size();
        let extent = SurfaceExtent {
            width: physical.width.max(1),
            height: physical.height.max(1),
        };
        let present_mode_selection = select_wgpu_present_mode(
            RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo,
            &caps.present_modes,
        );
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: extent.width,
            height: extent.height,
            present_mode: present_mode_selection.mode,
            desired_maximum_frame_latency: 1,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        Ok(Self {
            instance,
            surface,
            gpu,
            config,
            selected_present_mode: present_mode_selection.mode,
            present_mode_was_downgraded: present_mode_selection.fallback_reason.is_some(),
        })
    }

    fn resize(&mut self, extent: SurfaceExtent) {
        if extent.width == 0 || extent.height == 0 {
            return;
        }
        self.config.width = extent.width;
        self.config.height = extent.height;
        self.surface.configure(&self.gpu.device, &self.config);
    }

    fn gpu_context(&self) -> Arc<GpuRuntimeContext> {
        Arc::clone(&self.gpu)
    }

    fn extent(&self) -> SurfaceExtent {
        SurfaceExtent {
            width: self.config.width,
            height: self.config.height,
        }
    }
}

#[derive(Clone)]
struct ReferenceSwapchainHandle {
    surface: Arc<Mutex<SurfaceState>>,
}

impl ReferenceSwapchainHandle {
    fn new(surface: Arc<Mutex<SurfaceState>>) -> Self {
        Self { surface }
    }
}

impl SwapchainHandle for ReferenceSwapchainHandle {
    fn acquire(&self) -> Result<AcquiredTexture, SwapchainError> {
        let texture = {
            let surface = self
                .surface
                .lock()
                .map_err(|_| SwapchainError::Acquire("surface lock poisoned".into()))?;
            match surface.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                wgpu::CurrentSurfaceTexture::Outdated => {
                    return Err(SwapchainError::Acquire("surface outdated".into()));
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    return Err(SwapchainError::Acquire("surface lost".into()));
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    return Err(SwapchainError::Acquire("surface acquire timeout".into()));
                }
                wgpu::CurrentSurfaceTexture::Occluded => {
                    return Err(SwapchainError::Acquire("surface occluded".into()));
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(SwapchainError::Acquire("surface validation failure".into()));
                }
            }
        };
        Ok(AcquiredTexture::new(texture, Arc::new(self.clone())))
    }

    fn submit_present(&self, texture: wgpu::SurfaceTexture) -> Result<(), SwapchainError> {
        texture.present();
        Ok(())
    }

    fn current_format(&self) -> wgpu::TextureFormat {
        self.surface
            .lock()
            .map(|surface| surface.config.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)
    }

    fn current_extent(&self) -> wgpu::Extent3d {
        self.surface
            .lock()
            .map(|surface| wgpu::Extent3d {
                width: surface.config.width,
                height: surface.config.height,
                depth_or_array_layers: 1,
            })
            .unwrap_or(wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            })
    }
}

#[derive(Clone)]
struct ReferencePreacquiredSwapchainHandle {
    delegate: wrela::presentation_exec::swapchain::DynSwapchainHandle,
    acquired: Arc<Mutex<Option<AcquiredTexture>>>,
}

impl ReferencePreacquiredSwapchainHandle {
    fn new(
        delegate: wrela::presentation_exec::swapchain::DynSwapchainHandle,
        acquired: Arc<Mutex<Option<AcquiredTexture>>>,
    ) -> Self {
        Self { delegate, acquired }
    }
}

impl SwapchainHandle for ReferencePreacquiredSwapchainHandle {
    fn acquire(&self) -> Result<AcquiredTexture, SwapchainError> {
        self.acquired
            .lock()
            .map_err(|_| SwapchainError::Acquire("preacquired texture lock poisoned".into()))?
            .take()
            .ok_or_else(|| SwapchainError::Acquire("missing preacquired texture".into()))
    }

    fn submit_present(&self, texture: wgpu::SurfaceTexture) -> Result<(), SwapchainError> {
        self.delegate.submit_present(texture)
    }

    fn current_format(&self) -> wgpu::TextureFormat {
        self.delegate.current_format()
    }

    fn current_extent(&self) -> wgpu::Extent3d {
        self.delegate.current_extent()
    }
}

#[derive(Debug, Default, Clone)]
struct ReferencePresentationMetrics {
    queue_submit_count: u32,
    notes: Vec<String>,
}

fn reference_swapchain_plan() -> PresentationPlan {
    PresentationPlan {
        name: SmolStr::new("reference_host_swapchain"),
        view: ViewContract::canonical(),
        frame: FrameContract {
            outputs: Vec::new(),
            primary_hit: None,
            temporal: None,
            quality: RealtimeQualityContract::named(RealtimeQualityTier::Realtime60),
            lighting: LightingContract::legacy_preview(false),
            observability: PresentationObservabilityProfile::preview_compatibility(),
        },
        passes: Vec::new(),
        frame_artifacts: Vec::new(),
        bindings: Vec::new(),
        observability: PresentationObservability::preview_compatibility(),
    }
}

struct ReferencePresentationAdapter {
    surface: PresentationSurfaceSlot,
}

impl ReferencePresentationAdapter {
    fn with_surface_slot(surface: PresentationSurfaceSlot) -> Self {
        Self { surface }
    }
}

impl EngineSubsystemAdapter for ReferencePresentationAdapter {
    fn build(
        &mut self,
        builder: &mut wrela::engine_frame::EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let has_surface = self
            .surface
            .lock()
            .map(|surface| surface.is_some())
            .unwrap_or(false);
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "reference_host.presentation".to_string(),
            runs_after: vec![EngineSubsystemKind::StateAdvance],
            requires_gpu: has_surface,
            allows_hot_path_readback: false,
        };
        let [acquire_label, present_label] = PresentationFramegraph::swapchain_reporting_labels();
        let surface = Arc::clone(&self.surface);
        let metrics = Arc::new(Mutex::new(ReferencePresentationMetrics::default()));
        let metrics_for_job = Arc::clone(&metrics);
        let acquired_texture = Arc::new(Mutex::new(None::<AcquiredTexture>));
        let acquire = if has_surface {
            let surface_for_acquire = Arc::clone(&surface);
            let acquired_for_job = Arc::clone(&acquired_texture);
            builder.add_job(
                EngineSubsystemKind::Presentation,
                acquire_label,
                EngineJobAffinity::External,
                EngineSpanDomain::PresentWait,
                Vec::new(),
                false,
                move || {
                    let surface = surface_for_acquire
                        .lock()
                        .map_err(|_| {
                            EngineFrameError::Message(
                                "reference host presentation surface slot poisoned".into(),
                            )
                        })?
                        .clone()
                        .ok_or_else(|| {
                            EngineFrameError::Message(
                                "surface-backed presentation missing surface".into(),
                            )
                        })?;
                    let swapchain = ReferenceSwapchainHandle::new(surface);
                    let texture = swapchain
                        .acquire()
                        .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                    let mut acquired = acquired_for_job.lock().map_err(|_| {
                        EngineFrameError::Message("preacquired texture slot poisoned".into())
                    })?;
                    if let Some(stale) = acquired.take() {
                        stale.discard();
                    }
                    *acquired = Some(texture);
                    Ok(())
                },
            )
        } else {
            builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                acquire_label,
                EngineJobAffinity::External,
                EngineSpanDomain::PresentWait,
                Vec::new(),
                false,
                1,
            )
        };
        let submit_accounting = if has_surface {
            Some(builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                "presentation.swapchain_submit",
                EngineJobAffinity::Gpu,
                EngineSpanDomain::Gpu,
                vec![acquire],
                true,
                1,
            ))
        } else {
            None
        };
        let present = builder.add_job(
            EngineSubsystemKind::Presentation,
            present_label,
            EngineJobAffinity::External,
            EngineSpanDomain::PresentWait,
            submit_accounting.map_or_else(|| vec![acquire], |submit| vec![submit]),
            false,
            move || {
                let Some(surface) = surface
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message(
                            "reference host presentation surface slot poisoned".into(),
                        )
                    })?
                    .clone()
                else {
                    let mut metrics = metrics_for_job.lock().map_err(|_| {
                        EngineFrameError::Message(
                            "reference host presentation metrics poisoned".into(),
                        )
                    })?;
                    *metrics = ReferencePresentationMetrics::default();
                    return Ok(());
                };
                let (native, extent) = {
                    let surface_guard = surface.lock().map_err(|_| {
                        EngineFrameError::Message("reference host surface lock poisoned".into())
                    })?;
                    let _ = &surface_guard.instance;
                    (surface_guard.gpu_context(), surface_guard.extent())
                };
                let attachments = AttachmentResourceSet {
                    width: extent.width,
                    height: extent.height,
                    attachments: BTreeMap::new(),
                };
                let delegate = Arc::new(ReferenceSwapchainHandle::new(surface))
                    as wrela::presentation_exec::swapchain::DynSwapchainHandle;
                let swapchain = Arc::new(ReferencePreacquiredSwapchainHandle::new(
                    delegate,
                    Arc::clone(&acquired_texture),
                ))
                    as wrela::presentation_exec::swapchain::DynSwapchainHandle;
                let mut framegraph =
                    PresentationFramegraph::from_plan_and_gpu_resources_with_swapchain(
                        reference_swapchain_plan(),
                        attachments,
                        native,
                        0,
                        Some(swapchain),
                    );
                let submission = framegraph
                    .submit_segment(false)
                    .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                let mut metrics = metrics_for_job.lock().map_err(|_| {
                    EngineFrameError::Message("reference host presentation metrics poisoned".into())
                })?;
                metrics.queue_submit_count = submission.queue_submit_count;
                metrics.notes = submission
                    .swapchain_observation_labels
                    .iter()
                    .map(|label| format!("framegraph_observed={label}"))
                    .collect();
                if metrics.notes.is_empty() {
                    metrics
                        .notes
                        .push("presentation_framegraph_swapchain_not_observed".to_string());
                }
                Ok(())
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![acquire],
            vec![present],
            move |timeline, _ctx: &mut EngineFrameContext| {
                let elapsed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Presentation)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let metrics = metrics.lock().map_err(|_| {
                    EngineFrameError::Message("reference host presentation metrics poisoned".into())
                })?;
                let mut notes = vec!["presentation_framegraph_swapchain_observed".to_string()];
                notes.extend(metrics.notes.clone());
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: 1,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: elapsed,
                    self_reported_runtime_micros: None,
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: metrics.queue_submit_count,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: elapsed,
                    notes,
                })
            },
        ))
    }
}

struct ReferenceHostApp {
    config: ReferenceHostConfig,
    host: LiveEngineHost,
    _controls: ReferenceLiveControls,
    inspector: inspector::InspectorState,
    window: Option<Arc<Window>>,
    surface: Option<Arc<Mutex<SurfaceState>>>,
    presentation_surface: PresentationSurfaceSlot,
    input_producer: RawInputProducer,
    frame_pacing: Arc<FrameInFlightSemaphore>,
    frame_count: u32,
    started_at: std::time::Instant,
    last_tick_at: std::time::Instant,
}

impl ReferenceHostApp {
    fn new(config: ReferenceHostConfig) -> Result<Self, String> {
        // RFC 0011 C3: a single SPSC ring is shared between the platform
        // thread (winit pump) and the engine thread (`LateInputSampler`).
        let (input_producer, input_consumer) = RawInputRing::default().into_split();
        let presentation_surface = Arc::new(Mutex::new(None));
        let started_at = std::time::Instant::now();
        let executor = match &config.project_path {
            Some(path) => ReferenceProjectExecutor::from_entry_path(path)?,
            None => ReferenceProjectExecutor::reference_default(),
        };
        let (host, controls) = new_input_driven_host(
            input_consumer,
            started_at,
            Arc::clone(&presentation_surface),
            executor,
        );
        Ok(Self {
            config,
            host,
            _controls: controls,
            inspector: inspector::InspectorState::default(),
            window: None,
            surface: None,
            presentation_surface,
            input_producer,
            // RFC 0011 C3: live policy uses `max_frames_in_flight = 1` so the
            // semaphore size matches the latency-first stance.
            frame_pacing: Arc::new(FrameInFlightSemaphore::new(1)),
            frame_count: 0,
            started_at,
            last_tick_at: started_at,
        })
    }

    fn now_nanos(&self) -> u64 {
        self.started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }

    fn now_micros(&self) -> u64 {
        self.started_at.elapsed().as_micros().min(u64::MAX as u128) as u64
    }

    fn push_raw_event(&mut self, source: &'static str, detail: SmolStr, kind: RawInputKind) {
        self.input_producer.push_event(TimestampedRawEvent::new(
            SmolStr::new_static(source),
            detail,
            kind,
            self.now_micros(),
            self.now_nanos(),
        ));
    }

    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        // RFC 0011 C3: hold the frame-in-flight slot for the entire engine
        // tick + GPU submit/present pair. Live policy is 1, so this is a
        // no-op fast-path when the previous frame already presented.
        self.frame_pacing.acquire();
        let now = std::time::Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_tick_at)
            .as_secs_f64()
            .clamp(1.0 / 240.0, 0.25);
        self.last_tick_at = now;
        let result = self.host.advance(elapsed);
        let mut submitted_work = false;
        match result {
            Ok(tick) => {
                for output in tick.outputs {
                    self.frame_count = self.frame_count.saturating_add(1);
                    submitted_work |= output.report.gpu_runtime.queue_submit_count > 0;
                    self.inspector = inspector::InspectorState::from_report(&output.report);
                    if let Some(window) = &self.window {
                        let ring_state = self.input_producer.ring_state();
                        let present_label = self
                            .surface
                            .as_ref()
                            .and_then(|s| s.lock().ok())
                            .map(|s| {
                                if s.present_mode_was_downgraded {
                                    format!("{:?}*", s.selected_present_mode)
                                } else {
                                    format!("{:?}", s.selected_present_mode)
                                }
                            })
                            .unwrap_or_else(|| "noswapchain".to_string());
                        window.set_title(&format!(
                            "Wrela Reference Host | frame={} tick={} rows={} violations={} input_depth={} present_mode={}",
                            output.report.frame_index,
                            output.report.identity.simulation_tick,
                            self.inspector.rows.len(),
                            output.report.violations.len(),
                            ring_state.depth,
                            present_label,
                        ));
                    }
                }
            }
            Err(err) => {
                if let Some(window) = &self.window {
                    window.set_title(&format!("Wrela Reference Host | engine error: {err}"));
                }
                self.frame_pacing.release();
                event_loop.exit();
                return;
            }
        }
        let queue = self
            .surface
            .as_ref()
            .and_then(|surface| surface.lock().ok())
            .map(|surface| surface.gpu.queue.clone());
        if submitted_work {
            if let Some(queue) = queue {
                self.frame_pacing.release_after_submitted_work_done(&queue);
            } else {
                self.frame_pacing.release();
            }
        } else {
            self.frame_pacing.release();
        }
        if let Some(limit) = self.config.frames
            && self.frame_count >= limit
        {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for ReferenceHostApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title(self.config.window.title.clone())
            .with_inner_size(self.config.window.physical_size());
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                match SurfaceState::create(window.clone()) {
                    Ok(surface) => {
                        let surface = Arc::new(Mutex::new(surface));
                        if let Ok(mut slot) = self.presentation_surface.lock() {
                            *slot = Some(surface.clone());
                        }
                        self.surface = Some(surface);
                    }
                    Err(err) => {
                        eprintln!("reference host wgpu surface creation failed: {err}");
                        if let Ok(mut slot) = self.presentation_surface.lock() {
                            *slot = None;
                        }
                        // The presentation adapter was registered at startup
                        // with this shared slot, so `None` gives us a
                        // deterministic headless presentation fallback.
                    }
                }
                self.window = Some(window);
            }
            Err(err) => {
                eprintln!("reference host window creation failed: {err}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(surface) = self.surface.as_ref()
                    && let Ok(mut surface) = surface.lock()
                {
                    surface.resize(SurfaceExtent {
                        width: size.width,
                        height: size.height,
                    });
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state,
                        ..
                    },
                ..
            } => {
                let code = match physical_key {
                    PhysicalKey::Code(code) => SmolStr::new(format!("{code:?}")),
                    PhysicalKey::Unidentified(_) => SmolStr::new("Unidentified"),
                };
                let pressed = matches!(state, ElementState::Pressed);
                self.push_raw_event(
                    "keyboard",
                    SmolStr::new(format!("key.{code}")),
                    RawInputKind::Key { code, pressed },
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let label = match button {
                    MouseButton::Left => SmolStr::new_static("left"),
                    MouseButton::Right => SmolStr::new_static("right"),
                    MouseButton::Middle => SmolStr::new_static("middle"),
                    MouseButton::Back => SmolStr::new_static("back"),
                    MouseButton::Forward => SmolStr::new_static("forward"),
                    MouseButton::Other(code) => SmolStr::new(format!("other_{code}")),
                };
                let pressed = matches!(state, ElementState::Pressed);
                self.push_raw_event(
                    "mouse",
                    SmolStr::new(format!("mouse.{label}")),
                    RawInputKind::MouseButton {
                        button: label,
                        pressed,
                    },
                );
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.push_raw_event(
                    "mouse",
                    SmolStr::new_static("mouse.move"),
                    RawInputKind::MouseDelta {
                        x: position.x as i32,
                        y: position.y as i32,
                    },
                );
            }
            WindowEvent::RedrawRequested => self.tick(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.tick(event_loop);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
