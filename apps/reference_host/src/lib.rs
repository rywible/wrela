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

use ciborium::value::Value;
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};
use wrela::audio_contract::{MediaSample, VoiceId};
use wrela::audio_exec::{AudioSnapshotPublisher as RuntimeAudioSnapshotPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan, AudioVoicePlan};
use wrela::engine_frame::{
    AudioAdapterFrameState, AudioSnapshotPublisher, EngineFrameContext, EngineFrameError,
    EngineFrameReport, EngineFrameRuntime, EngineFrameRuntimePolicy, EngineGpuTimingPolicy,
    EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain,
    EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind, EngineSubsystemPlan,
    EngineSubsystemReport, InputSubsystemAdapter, LiveEngineHost, LiveProjectConfig,
    PhysicsSubsystemAdapter, RawInputRingLateSampler, ResidencySubsystemAdapter,
    ResolvedPresentMode, SaveAdapterFrameState, SaveLedgerSource, SavePublisher,
    SystemSubsystemAdapter, estimated_present_to_photons_nanos,
};
use wrela::gpu_runtime::{GpuLimitRequest, GpuRuntimeContext};
use wrela::hir::{self, FunctionRole, RuntimeFunctionMetadata};
use wrela::input_contract::{InputFrame, InputMapBinding, SemanticActionState};
use wrela::input_map_plan::InputMapPlan;
use wrela::persistence::{PersistenceProject, PersistentHandle, SnapshotLedgerRecord};
use wrela::physics_contract::{PhysicsBodyDescriptor, PhysicsBodyId};
use wrela::physics_exec::{
    CollisionExecPhysicsCollisionBatchExecutor, PhysicsBodyState, PhysicsCollisionWorld,
    PhysicsSolver,
};
use wrela::physics_plan::PhysicsPlan;
use wrela::presentation_contract::{
    FrameContract, LightingContract, PresentationObservabilityProfile, RealtimeQualityContract,
    RealtimeQualityTier, ViewContract,
};
use wrela::presentation_exec::framegraph::PresentationFramegraph;
use wrela::presentation_exec::resources::allocate_attachment_resources_without_history;
use wrela::presentation_exec::swapchain::{AcquiredTexture, SwapchainError, SwapchainHandle};
use wrela::presentation_plan::{PresentationObservability, PresentationPlan};
use wrela::query_contract::DispatchBackend as QueryDispatchBackend;
use wrela::query_exec::ids::{stable_region_scene_capture_id, stable_semantic_id};
use wrela::query_exec::{QueryExecContext, stable_region_snapshot_handle};
use wrela::query_plan::DispatchBackend;
use wrela::residency::follow::{FollowTarget, Transform3};
use wrela::residency::{
    RegionId, RegionLine, RegionResidencyService, ResidencyCandidate, ResidencyPolicy,
    ResidencyReport,
};
use wrela::state_advance::{
    ChangeClass, ChangeSummary, IdentityTransitionEvent, IdentityTransitionKind,
    StateAdvanceResult, WorldTransitionRecord,
};
use wrela::system_contract::{
    SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_exec::{CompiledSystemRuntime, SystemInvocationContext, SystemMirInvoker};
use wrela::system_plan::{SystemPlan, SystemProgram};
use wrela::world_identity::WorldSnapshotHandle;
use wrela_runtime::audio::device::{AudioDeviceConfig, VoiceOutputStream};
use wrela_runtime::audio::ring::{SampleRing, StereoFrame};
use wrela_runtime::audio::voice::{VoiceLedger, VoiceRenderer};
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

#[derive(Clone)]
struct ReferenceProjectRuntime {
    executor: ReferenceProjectExecutor,
    input_map: InputMapPlan,
    system_program: SystemProgram,
    system_notes: Vec<String>,
    allow_reference_system_invoker: bool,
    system_invoker: Option<Arc<dyn SystemMirInvoker>>,
    presentation_plan: PresentationPlan,
    residency_policy: ResidencyPolicy,
    residency_candidates: Vec<ResidencyCandidate>,
    physics_solver: PhysicsSolver,
    audio_plan: AudioDspPlan,
    persistence_project: PersistenceProject,
    persistence_ledger: Vec<SnapshotLedgerRecord>,
}

struct ReferenceRuntimeLedgerSource {
    seed_ledger: Vec<SnapshotLedgerRecord>,
    input_frame: Arc<Mutex<Option<InputFrame>>>,
    physics_solver: Arc<Mutex<PhysicsSolver>>,
    residency_report: Arc<Mutex<Option<ResidencyReport>>>,
    audio_state: Arc<Mutex<wrela::engine_frame::AudioAdapterFrameState>>,
}

impl std::fmt::Debug for ReferenceRuntimeLedgerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceRuntimeLedgerSource")
            .field("seed_records", &self.seed_ledger.len())
            .finish_non_exhaustive()
    }
}

impl SaveLedgerSource for ReferenceRuntimeLedgerSource {
    fn collect(
        &self,
        snapshot: &WorldSnapshotHandle,
    ) -> Result<Vec<SnapshotLedgerRecord>, EngineFrameError> {
        let mut ledger = Vec::new();
        ledger.push(SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                b"reference_host".as_slice(),
                b"state_advance".as_slice(),
                snapshot.capture_name().as_bytes(),
            ]),
            type_id: "RuntimeStateAdvance".to_string(),
            payload: Value::Map(vec![
                (
                    Value::Text("snapshot_epoch".into()),
                    Value::Integer(snapshot.epoch().0.into()),
                ),
                (
                    Value::Text("capture".into()),
                    Value::Text(snapshot.capture_name().to_string()),
                ),
            ]),
        });
        let input_actions = self
            .input_frame
            .lock()
            .map_err(|_| EngineFrameError::Message("input frame lock poisoned".into()))?
            .as_ref()
            .map(|frame| {
                frame
                    .actions
                    .iter()
                    .map(|(id, state)| {
                        Value::Map(vec![
                            (Value::Text("id".into()), Value::Text(id.0.to_string())),
                            (Value::Text("state".into()), input_action_state_value(state)),
                        ])
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ledger.push(SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                b"reference_host".as_slice(),
                b"input".as_slice(),
            ]),
            type_id: "InputFrame".to_string(),
            payload: Value::Map(vec![(
                Value::Text("actions".into()),
                Value::Array(input_actions),
            )]),
        });
        let physics_bodies = self
            .physics_solver
            .lock()
            .map_err(|_| EngineFrameError::Message("physics solver lock poisoned".into()))?
            .bodies()
            .iter()
            .map(|body| {
                Value::Map(vec![
                    (Value::Text("id".into()), Value::Integer(body.id.0.into())),
                    (Value::Text("position".into()), vec3_value(body.position)),
                    (
                        Value::Text("velocity".into()),
                        vec3_value(body.linear_velocity),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        ledger.push(SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                b"reference_host".as_slice(),
                b"physics".as_slice(),
            ]),
            type_id: "PhysicsBodyState".to_string(),
            payload: Value::Map(vec![(
                Value::Text("bodies".into()),
                Value::Array(physics_bodies),
            )]),
        });
        let resident_regions = self
            .residency_report
            .lock()
            .map_err(|_| EngineFrameError::Message("residency report lock poisoned".into()))?
            .as_ref()
            .map(|report| report.resident_region_ids.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|id| Value::Text(id.0.to_string()))
            .collect::<Vec<_>>();
        ledger.push(SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                b"reference_host".as_slice(),
                b"residency".as_slice(),
            ]),
            type_id: "ResidentRegions".to_string(),
            payload: Value::Map(vec![(
                Value::Text("resident_region_ids".into()),
                Value::Array(resident_regions),
            )]),
        });
        let (audio_voices, audio_tick) = {
            let guard = self
                .audio_state
                .lock()
                .map_err(|_| EngineFrameError::Message("audio state lock poisoned".into()))?;
            let voices = guard
                .plan
                .voices
                .iter()
                .map(|voice| {
                    Value::Map(vec![
                        (Value::Text("id".into()), Value::Integer(voice.id.0.into())),
                        (
                            Value::Text("source_audio_field".into()),
                            voice
                                .source_audio_field
                                .as_ref()
                                .map(|field| Value::Text(field.to_string()))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            Value::Text("source_audio_signature".into()),
                            Value::Integer(voice.source_audio_signature.into()),
                        ),
                        (
                            Value::Text("source_frequency_hz".into()),
                            Value::Float(f64::from(voice.source_frequency_hz)),
                        ),
                        (Value::Text("position".into()), vec3_value(voice.position)),
                        (Value::Text("velocity".into()), vec3_value(voice.velocity)),
                        (
                            Value::Text("gain".into()),
                            Value::Float(f64::from(voice.gain)),
                        ),
                        (
                            Value::Text("priority".into()),
                            Value::Integer(voice.priority.into()),
                        ),
                        (
                            Value::Text("occlusion_db".into()),
                            Value::Float(f64::from(voice.media.occlusion_db)),
                        ),
                        (
                            Value::Text("reverb_send".into()),
                            Value::Float(f64::from(voice.media.reverb_send)),
                        ),
                        (
                            Value::Text("lowpass_hz".into()),
                            Value::Float(f64::from(voice.media.lowpass_hz)),
                        ),
                        (Value::Text("gate".into()), Value::Bool(voice.gate)),
                    ])
                })
                .collect::<Vec<_>>();
            (voices, guard.tick)
        };
        ledger.push(SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                b"reference_host".as_slice(),
                b"audio".as_slice(),
            ]),
            type_id: "AudioVoiceLedger".to_string(),
            payload: Value::Map(vec![
                (Value::Text("voices".into()), Value::Array(audio_voices)),
                (
                    Value::Text("tick".into()),
                    Value::Integer(audio_tick.into()),
                ),
            ]),
        });
        ledger.extend(self.seed_ledger.clone());
        Ok(ledger)
    }
}

fn vec3_value(value: [f32; 3]) -> Value {
    Value::Array(
        value
            .into_iter()
            .map(|component| Value::Float(f64::from(component)))
            .collect(),
    )
}

fn input_action_state_value(state: &wrela::input_contract::SemanticActionState) -> Value {
    match state {
        wrela::input_contract::SemanticActionState::Button {
            pressed,
            just_pressed,
            just_released,
        } => Value::Map(vec![
            (Value::Text("kind".into()), Value::Text("button".into())),
            (Value::Text("pressed".into()), Value::Bool(*pressed)),
            (
                Value::Text("just_pressed".into()),
                Value::Bool(*just_pressed),
            ),
            (
                Value::Text("just_released".into()),
                Value::Bool(*just_released),
            ),
        ]),
        wrela::input_contract::SemanticActionState::Axis1 { value } => Value::Map(vec![
            (Value::Text("kind".into()), Value::Text("axis1".into())),
            (Value::Text("value".into()), Value::Float(f64::from(*value))),
        ]),
        wrela::input_contract::SemanticActionState::Axis2 { x, y } => Value::Map(vec![
            (Value::Text("kind".into()), Value::Text("axis2".into())),
            (Value::Text("x".into()), Value::Float(f64::from(*x))),
            (Value::Text("y".into()), Value::Float(f64::from(*y))),
        ]),
    }
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
}

impl ReferenceProjectRuntime {
    fn reference_default() -> Self {
        Self {
            executor: ReferenceProjectExecutor::reference_default(),
            input_map: reference_input_map(),
            system_program: reference_system_program(),
            system_notes: vec![
                "reference host using built-in system program; no project system loaded"
                    .to_string(),
            ],
            allow_reference_system_invoker: true,
            system_invoker: None,
            presentation_plan: reference_swapchain_plan(),
            residency_policy: ResidencyPolicy {
                candidate_window: 10.0,
                ..ResidencyPolicy::default()
            },
            residency_candidates: reference_residency_candidates(),
            physics_solver: reference_physics_solver(),
            audio_plan: reference_audio_plan(),
            persistence_project: reference_persistence_project(),
            persistence_ledger: reference_persistence_ledger(),
        }
    }

    fn from_loaded_project(
        project_label: SmolStr,
        project: &hir::project::LoadedProject,
    ) -> Result<Self, String> {
        let executor =
            ReferenceProjectExecutor::from_loaded_project(project_label.clone(), project);
        let (input_map, input_note) = project_input_map(project, &project_label)?;
        let (presentation_plan, presentation_note) =
            project_presentation_plan(project, &project_label)?;
        let (residency_policy, residency_candidates, residency_note) =
            project_residency_service(project, &project_label);
        let (physics_solver, physics_note) = project_physics_solver(project, &project_label);
        let (audio_plan, audio_note) = project_audio_plan(project, &project_label)?;
        let (persistence_project, persistence_ledger, persistence_note) =
            project_persistence_project(project, &project_label);
        let compiled_runtime =
            CompiledSystemRuntime::from_project(project).map_err(|err| err.to_string())?;
        let system_count = compiled_runtime
            .program
            .phases
            .iter()
            .map(Vec::len)
            .sum::<usize>();
        let compiled_invoker = compiled_runtime.executor.invoker();
        let (system_program, system_note, allow_reference_system_invoker, system_invoker) =
            if system_count == 0 {
                (
                    reference_system_program(),
                    "reference host using built-in system program; project defines no systems"
                        .to_string(),
                    true,
                    None,
                )
            } else {
                (
                    compiled_runtime.program,
                    format!("reference host scheduling {system_count} authored project system(s)"),
                    false,
                    Some(compiled_invoker),
                )
            };
        Ok(Self {
            executor,
            input_map,
            system_program,
            system_notes: vec![
                input_note,
                system_note,
                presentation_note,
                residency_note,
                physics_note,
                audio_note,
                persistence_note,
            ],
            allow_reference_system_invoker,
            system_invoker,
            presentation_plan,
            residency_policy,
            residency_candidates,
            physics_solver,
            audio_plan,
            persistence_project,
            persistence_ledger,
        })
    }

    fn from_entry_path(path: &PathBuf) -> Result<Self, String> {
        let project = hir::project::load_project_with_entrypoint(path, false)
            .map_err(|errors| format!("load project `{}`: {:?}", path.display(), errors))?;
        let label = path
            .file_stem()
            .and_then(|os| os.to_str())
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new("reference_host_project"));
        Self::from_loaded_project(label, &project)
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
    fn invoke(
        &self,
        mir_function_id: u32,
        _ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        if mir_function_id == 1 {
            Ok(())
        } else {
            Err(format!(
                "reference host cannot execute authored system MIR function {mir_function_id}"
            ))
        }
    }
}

fn project_input_map(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> Result<(InputMapPlan, String), String> {
    let mut maps = project.module.functions.iter().filter_map(|(_, function)| {
        match function.runtime_metadata.as_ref() {
            Some(RuntimeFunctionMetadata::InputMap(metadata)) => {
                Some((function.name.clone(), metadata))
            }
            _ => None,
        }
    });
    let Some((map_name, metadata)) = maps.next() else {
        return Ok((
            reference_input_map(),
            "reference host using built-in input map; project defines no input_map".to_string(),
        ));
    };
    let bindings = metadata
        .actions
        .iter()
        .flat_map(|action| {
            action.bindings.iter().map(|binding| {
                let (source, detail) = input_binding_source_and_detail(&binding.path);
                InputMapBinding::new(action.name.clone(), source, detail)
            })
        })
        .collect::<Vec<_>>();
    let input_map = InputMapPlan::new(map_name.clone(), bindings)
        .map_err(|err| format!("project input_map `{map_name}`: {err}"))?;
    Ok((
        input_map,
        format!(
            "reference host using project-derived input_map `{map_name}` for `{project_label}`"
        ),
    ))
}

fn project_presentation_plan(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> Result<(PresentationPlan, String), String> {
    let mut plans =
        wrela::presentation_plan::plans_for_module(&project.module, DispatchBackend::Wgsl);
    let Some(plan) = plans.pop() else {
        return Ok((
            reference_swapchain_plan(),
            "reference host using built-in presentation plan; project defines no view".to_string(),
        ));
    };
    let validation_errors = wrela::presentation_plan::validate_plan(&plan);
    if !validation_errors.is_empty() {
        return Err(format!(
            "project presentation plan `{}` for `{project_label}` failed validation: {:?}",
            plan.name, validation_errors
        ));
    }
    Ok((
        plan.clone(),
        format!(
            "reference host using project-derived presentation plan `{}` for `{project_label}`",
            plan.name
        ),
    ))
}

fn project_residency_service(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> (ResidencyPolicy, Vec<ResidencyCandidate>, String) {
    let policy = project_residency_policy(project);
    let candidates = project
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.role == FunctionRole::Region)
        .enumerate()
        .map(|(idx, (_, function))| {
            let name = function.name.as_str();
            let metadata = match function.runtime_metadata.as_ref() {
                Some(RuntimeFunctionMetadata::Clauses(metadata)) => Some(metadata),
                _ => None,
            };
            let spacing = runtime_clause_f32(metadata, "spacing")
                .or_else(|| runtime_clause_f32(metadata, "topology.spacing"))
                .unwrap_or(8.0);
            let center = [
                runtime_clause_f32(metadata, "center.x")
                    .or_else(|| runtime_clause_f32(metadata, "x"))
                    .unwrap_or(idx as f32 * spacing),
                runtime_clause_f32(metadata, "center.y")
                    .or_else(|| runtime_clause_f32(metadata, "y"))
                    .unwrap_or(0.0),
                runtime_clause_f32(metadata, "center.z")
                    .or_else(|| runtime_clause_f32(metadata, "z"))
                    .unwrap_or(0.0),
            ];
            ResidencyCandidate {
                region_id: RegionId::new(name),
                center,
                bytes: runtime_clause_i32(metadata, "bytes")
                    .or_else(|| runtime_clause_i32(metadata, "max_upload_bytes"))
                    .map(|value| value.max(0) as u64)
                    .unwrap_or_else(|| {
                        (runtime_clause_f32(metadata, "support.radius").unwrap_or(1.0) * 256.0)
                            .max(128.0) as u64
                    }),
                compatibility_hash: stable_semantic_id(&[
                    project_label.as_str().as_bytes(),
                    b"region",
                    name.as_bytes(),
                    format!("{:?}", function.runtime_metadata).as_bytes(),
                ]),
            }
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return (
            policy.clone(),
            reference_residency_candidates(),
            "reference host using built-in residency service; project defines no regions"
                .to_string(),
        );
    }
    (
        policy.clone(),
        candidates,
        format!(
            "reference host using {} project-derived region residency candidate(s), candidate_window={:.3}, admits/frame={}, evicts/frame={}",
            project
                .module
                .functions
                .iter()
                .filter(|(_, function)| function.role == FunctionRole::Region)
                .count(),
            policy.candidate_window,
            policy.max_admits_per_frame,
            policy.max_evicts_per_frame
        ),
    )
}

fn project_residency_policy(project: &hir::project::LoadedProject) -> ResidencyPolicy {
    let mut policy = ResidencyPolicy::default();
    for (_, function) in project.module.functions.iter() {
        if function.role != FunctionRole::View {
            continue;
        }
        let metadata = match function.runtime_metadata.as_ref() {
            Some(RuntimeFunctionMetadata::Clauses(metadata)) => Some(metadata),
            _ => None,
        };
        if let Some(candidate_window) = runtime_clause_f32(metadata, "candidate_window")
            .or_else(|| runtime_clause_f32(metadata, "residency.candidate_window"))
        {
            policy.candidate_window = candidate_window.max(0.0);
        }
        if let Some(max_admits) = runtime_clause_i32(metadata, "max_admits_per_frame")
            .or_else(|| runtime_clause_i32(metadata, "residency.max_admits_per_frame"))
        {
            policy.max_admits_per_frame = max_admits.max(0) as u32;
        }
        if let Some(max_evicts) = runtime_clause_i32(metadata, "max_evicts_per_frame")
            .or_else(|| runtime_clause_i32(metadata, "residency.max_evicts_per_frame"))
        {
            policy.max_evicts_per_frame = max_evicts.max(0) as u32;
        }
        if let Some(upload_bytes) = runtime_clause_i32(metadata, "max_upload_bytes_per_frame")
            .or_else(|| runtime_clause_i32(metadata, "residency.max_upload_bytes_per_frame"))
        {
            policy.max_upload_bytes_per_frame = upload_bytes.max(0) as u64;
        }
    }
    policy
}

fn project_physics_solver(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> (PhysicsSolver, String) {
    let descriptors = project
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.role == FunctionRole::Body)
        .map(|(_, function)| project_physics_body_descriptor(project_label, function))
        .collect::<Vec<_>>();
    if descriptors.is_empty() {
        return (
            reference_physics_solver(),
            "reference host using built-in physics solver; project defines no body declarations"
                .to_string(),
        );
    }
    let states = descriptors
        .iter()
        .enumerate()
        .map(|(idx, descriptor)| {
            PhysicsBodyState::new(descriptor.id, [idx as f32 * 1.25, descriptor.radius, 0.0])
        })
        .collect::<Vec<_>>();
    let plan = PhysicsPlan::collision_backed(descriptors.clone());
    if let Some(world) = project_collision_world(project) {
        let (type_errors, type_info) = hir::typeck::check_module_with_info(&project.module);
        if !type_errors.is_empty() {
            return (
                PhysicsSolver::new(PhysicsPlan::cpu(descriptors.clone()), states),
                format!(
                    "reference host using {} project-derived physics body declaration(s) with CPU oracle fallback; collision_exec context failed typecheck",
                    descriptors.len()
                ),
            );
        }
        let query_ctx = Arc::new(QueryExecContext::compile(&project.module, &type_info));
        let executor = Arc::new(CollisionExecPhysicsCollisionBatchExecutor::new(query_ctx));
        return (
            PhysicsSolver::with_collision_executor(plan, states, executor)
                .with_collision_world(world),
            format!(
                "reference host using {} project-derived physics body declaration(s) with collision_exec world",
                descriptors.len()
            ),
        );
    }
    (
        PhysicsSolver::new(PhysicsPlan::cpu(descriptors.clone()), states),
        format!(
            "reference host using {} project-derived physics body declaration(s) with CPU oracle fallback; project defines no region/domain collision world",
            descriptors.len()
        ),
    )
}

fn project_collision_world(project: &hir::project::LoadedProject) -> Option<PhysicsCollisionWorld> {
    let region_name = project
        .module
        .functions
        .iter()
        .find(|(_, function)| function.role == FunctionRole::Region)
        .map(|(_, function)| function.name.clone())?;
    let domain_name = project
        .module
        .functions
        .iter()
        .find(|(_, function)| function.role == FunctionRole::Domain)
        .map(|(_, function)| function.name.clone())?;
    let scene_id = stable_region_scene_capture_id(&region_name);
    Some(PhysicsCollisionWorld {
        capture: region_capture_value(scene_id, 1),
        domain: scene_domain_value(scene_id, &domain_name),
        backend: QueryDispatchBackend::Cpu,
    })
}

fn region_capture_value(scene_id: u32, epoch: u32) -> wrela::kernel::KernelValue {
    use wrela::kernel::{KernelStructValue, KernelValue};
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

fn scene_domain_value(scene_id: u32, _domain_name: &SmolStr) -> wrela::kernel::KernelValue {
    use wrela::kernel::{KernelStructValue, KernelValue};
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(1))],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(true))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(false)),
                        (SmolStr::new("media"), KernelValue::Bool(false)),
                    ],
                }),
            ),
        ],
    })
}

fn project_physics_body_descriptor(
    project_label: &SmolStr,
    function: &hir::Function,
) -> PhysicsBodyDescriptor {
    let metadata = match function.runtime_metadata.as_ref() {
        Some(RuntimeFunctionMetadata::Clauses(metadata)) => Some(metadata),
        _ => None,
    };
    let mass = runtime_clause_f32(metadata, "mass")
        .unwrap_or(1.0)
        .max(0.001);
    let radius = runtime_clause_f32(metadata, "radius")
        .or_else(|| runtime_clause_f32(metadata, "support.radius"))
        .unwrap_or(0.5)
        .max(0.001);
    let mut descriptor = PhysicsBodyDescriptor::dynamic_sphere(
        PhysicsBodyId(stable_semantic_id(&[
            project_label.as_str().as_bytes(),
            b"body",
            function.name.as_str().as_bytes(),
        ])),
        mass,
        radius,
    );
    if let Some(class) = runtime_clause_value(metadata, "class") {
        match class.as_str() {
            "static" => {
                descriptor.class = wrela::physics_contract::PhysicsBodyClass::Static;
                descriptor.inverse_mass = 0.0;
            }
            "kinematic" => {
                descriptor.class = wrela::physics_contract::PhysicsBodyClass::Kinematic;
                descriptor.inverse_mass = 0.0;
            }
            _ => {}
        }
    }
    descriptor
}

fn project_audio_plan(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> Result<(AudioDspPlan, String), String> {
    let audio_rt_errors = wrela::audio_exec::rt_check::check_audio_rt_module(&project.module);
    if !audio_rt_errors.is_empty() {
        return Err(format!(
            "project audio_rt validation failed: {:?}",
            audio_rt_errors
        ));
    }
    let audio_fields = project
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.role == FunctionRole::AudioField)
        .map(|(_, function)| {
            (
                function.name.clone(),
                authored_audio_field_signature(project_label, function),
                wrela::audio_exec::compile_audio_field_program(function),
            )
        })
        .collect::<Vec<_>>();
    let voices = project
        .module
        .functions
        .iter()
        .filter(|(_, function)| function.role == FunctionRole::Voice)
        .map(|(_, function)| project_audio_voice(project_label, function, &audio_fields))
        .collect::<Result<Vec<_>, _>>()?;
    if voices.is_empty() {
        return Ok((
            reference_audio_plan(),
            "reference host using built-in audio plan; project defines no voices".to_string(),
        ));
    }
    if audio_fields.is_empty() {
        return Err(format!(
            "project defines {} voice declaration(s) but no audio field source",
            voices.len()
        ));
    }
    Ok((
        AudioDspPlan {
            voices: voices.clone(),
        },
        format!(
            "reference host using {} project-derived voice declaration(s)",
            voices.len()
        ),
    ))
}

fn project_audio_voice(
    project_label: &SmolStr,
    function: &hir::Function,
    audio_fields: &[(SmolStr, u64, wrela::audio_plan::DspProgram)],
) -> Result<AudioVoicePlan, String> {
    let metadata = match function.runtime_metadata.as_ref() {
        Some(RuntimeFunctionMetadata::Clauses(metadata)) => Some(metadata),
        _ => None,
    };
    let source_audio_field = function
        .params
        .iter()
        .filter_map(|param| param.ty.as_ref().map(|ty| ty.name.clone()))
        .find(|name| audio_fields.iter().any(|(field, _, _)| field == name))
        .or_else(|| audio_fields.first().map(|(field, _, _)| field.clone()));
    let Some(source_audio_field) = source_audio_field else {
        return Err(format!(
            "voice `{}` has no audio field source parameter",
            function.name
        ));
    };
    let source_audio_signature = audio_fields
        .iter()
        .find(|(name, _, _)| name == &source_audio_field)
        .map(|(_, signature, _)| *signature)
        .unwrap_or_else(|| {
            stable_semantic_id(&[
                project_label.as_str().as_bytes(),
                b"audio_field",
                source_audio_field.as_str().as_bytes(),
            ])
        });
    let source_program = audio_fields
        .iter()
        .find(|(name, _, _)| name == &source_audio_field)
        .map(|(_, _, program)| *program)
        .unwrap_or_default();
    Ok(AudioVoicePlan {
        id: VoiceId(stable_semantic_id(&[
            project_label.as_str().as_bytes(),
            b"voice",
            function.name.as_str().as_bytes(),
        ])),
        source_audio_field: Some(source_audio_field),
        source_audio_signature,
        source_program,
        source_frequency_hz: runtime_clause_f32(metadata, "freq")
            .or_else(|| runtime_clause_f32(metadata, "frequency"))
            .unwrap_or_else(|| 220.0 + (source_audio_signature % 48) as f32 * 7.5),
        position: [0.0, 0.0, 0.0],
        velocity: [0.0, 0.0, 0.0],
        gain: runtime_clause_f32(metadata, "gain").unwrap_or(1.0),
        priority: runtime_clause_i32(metadata, "priority").unwrap_or(1),
        media: MediaSample::default(),
        gate: true,
    })
}

fn authored_audio_field_signature(project_label: &SmolStr, function: &hir::Function) -> u64 {
    stable_semantic_id(&[
        project_label.as_str().as_bytes(),
        b"audio_field",
        function.name.as_str().as_bytes(),
        format!("{:?}", function.body).as_bytes(),
        format!("{:?}", function.runtime_metadata).as_bytes(),
    ])
}

fn project_persistence_project(
    project: &hir::project::LoadedProject,
    project_label: &SmolStr,
) -> (PersistenceProject, Vec<SnapshotLedgerRecord>, String) {
    let mut generator_compatibility_hashes = BTreeMap::new();
    let mut archetype_schema_hashes = BTreeMap::new();
    for (_, function) in project.module.functions.iter() {
        let hash = stable_semantic_id(&[
            project_label.as_str().as_bytes(),
            b"persistence",
            function.name.as_str().as_bytes(),
            format!("{:?}", function.role).as_bytes(),
        ]);
        match function.role {
            FunctionRole::Region | FunctionRole::Field | FunctionRole::Body => {
                generator_compatibility_hashes.insert(function.name.to_string(), hash);
            }
            FunctionRole::System | FunctionRole::Voice | FunctionRole::InputMap => {
                archetype_schema_hashes.insert(function.name.to_string(), hash);
            }
            _ => {}
        }
    }
    let ledger = project
        .module
        .functions
        .iter()
        .filter(|(_, function)| {
            matches!(
                function.role,
                FunctionRole::System
                    | FunctionRole::InputMap
                    | FunctionRole::Body
                    | FunctionRole::Voice
                    | FunctionRole::Region
            )
        })
        .map(|(_, function)| SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                project_label.as_str().as_bytes(),
                function.name.as_str().as_bytes(),
            ]),
            type_id: format!("{:?}", function.role),
            payload: Value::Text(function.name.to_string()),
        })
        .collect::<Vec<_>>();
    (
        PersistenceProject {
            project_id: project_label.to_string(),
            wrela_version: "reference-host".into(),
            engine_compatibility_hash: stable_semantic_id(&[
                project_label.as_str().as_bytes(),
                b"engine",
            ]),
            generator_compatibility_hashes,
            archetype_schema_hashes,
        },
        ledger,
        format!("reference host using project-derived persistence identity `{project_label}`"),
    )
}

fn runtime_clause_value<'a>(
    metadata: Option<&'a hir::RuntimeClausesMetadata>,
    name: &str,
) -> Option<&'a SmolStr> {
    metadata
        .and_then(|metadata| find_runtime_clause(&metadata.clauses, name))
        .and_then(|clause| clause.value.as_ref())
}

fn runtime_clause_f32(metadata: Option<&hir::RuntimeClausesMetadata>, name: &str) -> Option<f32> {
    runtime_clause_value(metadata, name).and_then(|value| value.parse::<f32>().ok())
}

fn runtime_clause_i32(metadata: Option<&hir::RuntimeClausesMetadata>, name: &str) -> Option<i32> {
    runtime_clause_value(metadata, name).and_then(|value| value.parse::<i32>().ok())
}

fn find_runtime_clause<'a>(
    clauses: &'a [hir::RuntimeClauseMetadata],
    name: &str,
) -> Option<&'a hir::RuntimeClauseMetadata> {
    for clause in clauses {
        if clause.name == name {
            return Some(clause);
        }
        if let Some(found) = find_runtime_clause(&clause.clauses, name) {
            return Some(found);
        }
    }
    None
}

fn input_binding_source_and_detail(path: &SmolStr) -> (SmolStr, SmolStr) {
    let path = path.as_str();
    let (source, detail) = match path.split_once('.') {
        Some(("key" | "keyboard", key)) => ("keyboard", canonical_keyboard_detail(key)),
        Some(("mouse", _)) => ("mouse", SmolStr::new(path)),
        Some((source, _)) => (source, SmolStr::new(path)),
        None => (path, SmolStr::new(path)),
    };
    (SmolStr::new(source), detail)
}

fn canonical_keyboard_detail(key: &str) -> SmolStr {
    if let Some(code) = key.strip_prefix("Key") {
        return SmolStr::new(format!("key.Key{code}"));
    }
    if key.len() == 1 {
        let mut chars = key.chars();
        let ch = chars.next().expect("single char");
        return SmolStr::new(format!("key.Key{}", ch.to_ascii_uppercase()));
    }
    SmolStr::new(format!("key.{key}"))
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

fn reference_residency_candidates() -> Vec<ResidencyCandidate> {
    vec![ResidencyCandidate {
        region_id: RegionId::new("reference_origin"),
        center: [0.0, 0.0, 0.0],
        bytes: 128,
        compatibility_hash: 1,
    }]
}

fn residency_service_from_candidates(
    policy: ResidencyPolicy,
    candidates: Vec<ResidencyCandidate>,
) -> RegionResidencyService {
    RegionResidencyService::new(
        policy,
        Box::new(RegionLine {
            regions: candidates,
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

fn reference_persistence_ledger() -> Vec<SnapshotLedgerRecord> {
    vec![SnapshotLedgerRecord {
        handle: PersistentHandle::from_stable_semantic_parts(&[b"reference_host", b"state"]),
        type_id: "ReferenceHostState".into(),
        payload: Value::Text("reference_host".into()),
    }]
}

fn reference_audio_plan() -> AudioDspPlan {
    AudioDspPlan {
        voices: vec![sine_voice(1, 1, 1.0)],
    }
}

#[derive(Clone)]
struct ReferenceLiveControls {
    save_frame_state: Arc<Mutex<SaveAdapterFrameState>>,
    save_record: Arc<Mutex<Option<wrela::persistence::SnapshotSaveRecord>>>,
}

impl ReferenceLiveControls {
    fn request_save(&self, request: bool) {
        if let Ok(mut state) = self.save_frame_state.lock() {
            state.request = request;
        }
    }

    fn take_save_record(&self) -> Option<wrela::persistence::SnapshotSaveRecord> {
        self.save_record
            .lock()
            .ok()
            .and_then(|record| record.clone())
    }
}

type PresentationSurfaceSlot = Arc<Mutex<Option<Arc<Mutex<SurfaceState>>>>>;

const REFERENCE_FALLBACK_PRESENT_HZ: f64 = 120.0;

fn reference_live_subsystems(
    runtime: &EngineFrameRuntime,
    presentation_surface: PresentationSurfaceSlot,
    surface_pixel_readback: Option<Arc<Mutex<SurfacePixelReadback>>>,
    save_requested: bool,
    project_runtime: &ReferenceProjectRuntime,
    simulation_hz: f64,
) -> (Vec<Box<dyn EngineSubsystemAdapter>>, ReferenceLiveControls) {
    let input = InputSubsystemAdapter::new(
        project_runtime.input_map.clone(),
        runtime.materialized_tick_input_slot(),
    );
    let input_frame = input.shared_frame();
    let system = if project_runtime.allow_reference_system_invoker {
        SystemSubsystemAdapter::with_invoker(
            project_runtime.system_program.clone(),
            Arc::clone(&input_frame),
            Arc::new(ReferenceSystemInvoker),
        )
        .with_report_notes(project_runtime.system_notes.clone())
    } else {
        SystemSubsystemAdapter::with_invoker(
            project_runtime.system_program.clone(),
            Arc::clone(&input_frame),
            project_runtime
                .system_invoker
                .clone()
                .expect("authored system invoker"),
        )
        .with_report_notes(project_runtime.system_notes.clone())
    };
    let residency = ResidencySubsystemAdapter::with_state_outcome(
        residency_service_from_candidates(
            project_runtime.residency_policy.clone(),
            project_runtime.residency_candidates.clone(),
        ),
        FollowTarget {
            transform: Transform3 {
                translation: [0.0, 0.0, 0.0],
            },
            velocity: None,
        },
        runtime.state_advance_outcome_slot(),
    );
    let residency_report = residency.report();
    let physics = PhysicsSubsystemAdapter::new(
        project_runtime.physics_solver.clone(),
        (1.0 / simulation_hz.max(f64::EPSILON)) as f32,
    );
    let physics_solver = physics.solver();
    let audio_config = AudioConfig::default();
    let audio_ledger = Arc::new(VoiceLedger::new());
    let audio_output = Arc::new(ReferenceAudioOutput::new(
        audio_config.clone(),
        Arc::clone(&audio_ledger),
    ));
    let runtime_audio = RuntimeAudioSnapshotPublisher::with_runtime_underrun_counter(
        audio_config,
        Arc::clone(&audio_ledger),
        audio_output.underrun_counter(),
    );
    let audio = AudioSnapshotPublisher::new(runtime_audio, project_runtime.audio_plan.clone(), 0);
    let audio_state = audio.frame_state();
    let save = SavePublisher::with_state_outcome(
        save_requested,
        runtime.state_advance_outcome_slot(),
        project_runtime.persistence_project.clone(),
        0,
        0,
        project_runtime.persistence_ledger.clone(),
    );
    save.set_ledger_source(Arc::new(ReferenceRuntimeLedgerSource {
        seed_ledger: project_runtime.persistence_ledger.clone(),
        input_frame: Arc::clone(&input_frame),
        physics_solver: Arc::clone(&physics_solver),
        residency_report: Arc::clone(&residency_report),
        audio_state: Arc::clone(&audio_state),
    }));
    let controls = ReferenceLiveControls {
        save_frame_state: save.frame_state(),
        save_record: save.record(),
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
                project_runtime.presentation_plan.clone(),
                PresentationLiveStateRefs {
                    input_frame,
                    residency_report,
                    physics_solver,
                    audio_state,
                    audio_output,
                    surface_pixel_readback,
                },
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
        ReferenceProjectRuntime::reference_default(),
    )
}

fn new_headless_host_with_controls_and_executor(
    save_requested: bool,
    project_runtime: ReferenceProjectRuntime,
) -> (LiveEngineHost, ReferenceLiveControls) {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("reference_host_smoke"));
    let runtime = EngineFrameRuntime::new(Box::new(project_runtime.executor.clone()));
    let simulation_hz = REFERENCE_FALLBACK_PRESENT_HZ;
    let (subsystems, controls) = reference_live_subsystems(
        &runtime,
        Arc::new(Mutex::new(None)),
        None,
        save_requested,
        &project_runtime,
        simulation_hz,
    );
    let mut host = LiveEngineHost::new_headless(
        runtime,
        LiveProjectConfig {
            scenario_id: "reference_host".into(),
            default_query_requests: Vec::new(),
            simulation_hz_override: None,
        },
        EngineFrameRuntimePolicy::live(),
        snapshot,
        simulation_hz,
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
    surface_pixel_readback: Option<Arc<Mutex<SurfacePixelReadback>>>,
    project_runtime: ReferenceProjectRuntime,
    simulation_hz: f64,
) -> (LiveEngineHost, ReferenceLiveControls) {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("reference_host_interactive"));
    let runtime = EngineFrameRuntime::new(Box::new(project_runtime.executor.clone()));
    let (subsystems, controls) = reference_live_subsystems(
        &runtime,
        presentation_surface,
        surface_pixel_readback,
        false,
        &project_runtime,
        simulation_hz,
    );
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
        simulation_hz,
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
    let project_runtime = ReferenceProjectRuntime::from_entry_path(&project_path)?;
    let (mut host, controls) = new_headless_host_with_controls_and_executor(false, project_runtime);
    run_headless_host_smoke(&mut host, &controls, frames)
}

pub fn run_headless_save_for_project(
    frames: u32,
    project_path: PathBuf,
) -> Result<wrela::persistence::SnapshotSaveRecord, String> {
    let project_runtime = ReferenceProjectRuntime::from_entry_path(&project_path)?;
    let (mut host, controls) = new_headless_host_with_controls_and_executor(true, project_runtime);
    let frames = frames.max(1);
    for frame in 0..frames {
        controls.request_save(frame == frames - 1);
        let tick = host
            .advance(host.simulation_step_secs())
            .map_err(|e| format!("engine tick: {e}"))?;
        if tick.outputs.is_empty() {
            return Err("expected at least one simulation tick output".into());
        }
    }
    controls
        .take_save_record()
        .ok_or_else(|| "headless save did not produce a save record".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedInputToPixelObservation {
    pub input_to_pixel_nanos: u64,
    pub changed_pixel_linear_index: u32,
}

pub fn run_rendered_input_to_pixel_for_project(
    project_path: PathBuf,
    samples: u32,
) -> Result<RenderedInputToPixelObservation, String> {
    let event_loop = EventLoop::new().map_err(|err| format!("event loop: {err}"))?;
    let mut app = RenderedLatencyProbeApp::new(project_path, samples.max(1))?;
    event_loop
        .run_app(&mut app)
        .map_err(|err| format!("rendered latency app: {err}"))?;
    app.finish()
}

#[derive(Default)]
struct SurfacePixelReadback {
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    buffer: Option<wgpu::Buffer>,
    latest_pixels: Option<Vec<u8>>,
}

impl SurfacePixelReadback {
    const BYTES_PER_PIXEL: u32 = 4;

    fn latest_pixels(&self) -> Option<Vec<u8>> {
        self.latest_pixels.clone()
    }

    fn submit_surface_texture_copy(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        extent: SurfaceExtent,
    ) -> Result<(), String> {
        let width = extent.width.max(1);
        let height = extent.height.max(1);
        let unpadded_bytes_per_row = width.saturating_mul(Self::BYTES_PER_PIXEL);
        let padded_bytes_per_row = align_to_copy_bytes_per_row(unpadded_bytes_per_row)
            .max(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        if self.buffer.is_none()
            || self.width != width
            || self.height != height
            || self.padded_bytes_per_row != padded_bytes_per_row
        {
            self.width = width;
            self.height = height;
            self.padded_bytes_per_row = padded_bytes_per_row;
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.reference_host.surface_pixel_readback"),
                size: u64::from(padded_bytes_per_row) * u64::from(height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| "surface pixel readback buffer missing".to_string())?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wrela.reference_host.surface_pixel_readback.encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        Ok(())
    }

    fn collect_latest_pixels(&mut self, device: &wgpu::Device) -> Result<(), String> {
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| "surface pixel readback buffer missing".to_string())?;
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|err| format!("surface pixel readback device poll failed: {err}"))?;
        receiver
            .recv()
            .map_err(|err| format!("surface pixel readback channel failed: {err}"))?
            .map_err(|err| format!("surface pixel readback failed: {err}"))?;
        let mapped = slice.get_mapped_range();
        let unpadded_bytes_per_row = self.width.saturating_mul(Self::BYTES_PER_PIXEL);
        let mut pixels =
            Vec::with_capacity((self.width * self.height * Self::BYTES_PER_PIXEL) as usize);
        for row in 0..self.height as usize {
            let start = row * self.padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&mapped[start..end]);
        }
        drop(mapped);
        buffer.unmap();
        self.latest_pixels = Some(pixels);
        Ok(())
    }
}

fn align_to_copy_bytes_per_row(bytes: u32) -> u32 {
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    bytes.div_ceil(alignment).saturating_mul(alignment)
}

fn first_changed_pixel_index(before: &[u8], after: &[u8]) -> Option<u32> {
    before
        .chunks_exact(SurfacePixelReadback::BYTES_PER_PIXEL as usize)
        .zip(after.chunks_exact(SurfacePixelReadback::BYTES_PER_PIXEL as usize))
        .enumerate()
        .find_map(|(idx, (before, after))| (before != after).then_some(idx as u32))
}

fn percentile_nanos(sorted_samples: &[u64], percentile: f64) -> u64 {
    if sorted_samples.is_empty() {
        return 0;
    }
    let clamped = percentile.clamp(0.0, 1.0);
    let idx = ((sorted_samples.len() - 1) as f64 * clamped).round() as usize;
    sorted_samples[idx]
}

fn run_headless_host_smoke(
    host: &mut LiveEngineHost,
    controls: &ReferenceLiveControls,
    frames: u32,
) -> Result<Vec<EngineFrameReport>, String> {
    run_headless_host_smoke_with_options(host, controls, frames, false)
}

fn run_headless_host_smoke_with_options(
    host: &mut LiveEngineHost,
    controls: &ReferenceLiveControls,
    frames: u32,
    allow_motion_to_photon_budget_violation: bool,
) -> Result<Vec<EngineFrameReport>, String> {
    let mut reports = Vec::new();
    for frame in 0..frames {
        controls.request_save(frame == 0);
        let tick = host
            .advance(host.simulation_step_secs())
            .map_err(|e| format!("engine tick: {e}"))?;
        if tick.outputs.is_empty() {
            return Err("expected at least one simulation tick output".into());
        }
        for output in tick.outputs {
            let unexpected_violations = output
                .report
                .violations
                .iter()
                .filter(|violation| {
                    !(allow_motion_to_photon_budget_violation
                        && violation.as_str() == "presentation.motion_to_photon_over_budget")
                })
                .cloned()
                .collect::<Vec<_>>();
            if !unexpected_violations.is_empty() {
                return Err(format!(
                    "unexpected violations at frame {} latency_ms={:.3}: {:?}",
                    output.report.frame_index,
                    output.report.latency.total_estimate_nanos as f64 / 1_000_000.0,
                    unexpected_violations
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

struct RenderedLatencyProbeApp {
    host: LiveEngineHost,
    controls: ReferenceLiveControls,
    window: Option<Arc<Window>>,
    surface: Option<Arc<Mutex<SurfaceState>>>,
    presentation_surface: PresentationSurfaceSlot,
    input_producer: RawInputProducer,
    surface_pixel_readback: Arc<Mutex<SurfacePixelReadback>>,
    samples_target: u32,
    samples_done: u32,
    measurements: Vec<u64>,
    first_changed_pixel: Option<u32>,
    baseline_pixels: Option<Vec<u8>>,
    started_at: std::time::Instant,
    result: Option<Result<RenderedInputToPixelObservation, String>>,
}

impl RenderedLatencyProbeApp {
    fn new(project_path: PathBuf, samples_target: u32) -> Result<Self, String> {
        let (input_producer, input_consumer) = RawInputRing::default().into_split();
        let presentation_surface = Arc::new(Mutex::new(None));
        let surface_pixel_readback = Arc::new(Mutex::new(SurfacePixelReadback::default()));
        let started_at = std::time::Instant::now();
        let project_runtime = ReferenceProjectRuntime::from_entry_path(&project_path)?;
        let (host, controls) = new_input_driven_host(
            input_consumer,
            started_at,
            Arc::clone(&presentation_surface),
            Some(Arc::clone(&surface_pixel_readback)),
            project_runtime,
            REFERENCE_FALLBACK_PRESENT_HZ,
        );
        Ok(Self {
            host,
            controls,
            window: None,
            surface: None,
            presentation_surface,
            input_producer,
            surface_pixel_readback,
            samples_target,
            samples_done: 0,
            measurements: Vec::with_capacity(samples_target as usize),
            first_changed_pixel: None,
            baseline_pixels: None,
            started_at,
            result: None,
        })
    }

    fn finish(self) -> Result<RenderedInputToPixelObservation, String> {
        self.result
            .unwrap_or_else(|| Err("rendered latency probe exited without a result".to_string()))
    }

    fn now_nanos(&self) -> u64 {
        self.started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }

    fn now_micros(&self) -> u64 {
        self.started_at.elapsed().as_micros().min(u64::MAX as u128) as u64
    }

    fn push_key_w(&mut self, pressed: bool) {
        let nanos = self.now_nanos().max(1);
        self.input_producer.push_event(TimestampedRawEvent::new(
            "keyboard",
            "key.KeyW",
            RawInputKind::Key {
                code: SmolStr::new("KeyW"),
                pressed,
            },
            self.now_micros().max(1),
            nanos,
        ));
    }

    fn drive_one_frame(&mut self) -> Result<EngineFrameReport, String> {
        self.controls.request_save(false);
        let tick = self
            .host
            .advance(self.host.simulation_step_secs())
            .map_err(|err| format!("rendered latency engine tick: {err}"))?;
        tick.outputs
            .into_iter()
            .last()
            .map(|output| output.report)
            .ok_or_else(|| "rendered latency probe produced no frame output".to_string())
    }

    fn latest_pixels(&self) -> Result<Vec<u8>, String> {
        self.surface_pixel_readback
            .lock()
            .map_err(|_| "surface pixel readback lock poisoned".to_string())?
            .latest_pixels()
            .ok_or_else(|| "surface pixel probe did not capture a frame".to_string())
    }

    fn drive_probe(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.surface.is_none() {
            return Ok(());
        }
        if self.baseline_pixels.is_none() {
            self.drive_one_frame()?;
            self.baseline_pixels = Some(self.latest_pixels()?);
        }
        while self.samples_done < self.samples_target {
            let baseline = self
                .baseline_pixels
                .as_ref()
                .cloned()
                .ok_or_else(|| "rendered latency probe missing baseline".to_string())?;
            let input_event_at = std::time::Instant::now();
            self.push_key_w(true);
            let report = self.drive_one_frame()?;
            let input_report = report
                .subsystems
                .iter()
                .find(|subsystem| subsystem.kind == EngineSubsystemKind::Input)
                .ok_or_else(|| "rendered latency run missing input subsystem".to_string())?;
            if input_report.work_items == 0 {
                return Err("perf-latency input did not reach InputFrame".to_string());
            }
            let changed_pixels = self.latest_pixels()?;
            let changed_pixel_linear_index = first_changed_pixel_index(&baseline, &changed_pixels)
                .ok_or_else(|| "surface readback did not observe a changed pixel".to_string())?;
            let input_to_pixel_nanos =
                input_event_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            self.measurements.push(input_to_pixel_nanos.max(1));
            self.first_changed_pixel
                .get_or_insert(changed_pixel_linear_index);

            self.push_key_w(false);
            self.drive_one_frame()?;
            self.baseline_pixels = Some(self.latest_pixels()?);
            self.samples_done = self.samples_done.saturating_add(1);
        }
        self.measurements.sort_unstable();
        self.result = Some(Ok(RenderedInputToPixelObservation {
            input_to_pixel_nanos: percentile_nanos(&self.measurements, 0.99),
            changed_pixel_linear_index: self.first_changed_pixel.unwrap_or(0),
        }));
        event_loop.exit();
        Ok(())
    }

    fn record_error(&mut self, event_loop: &ActiveEventLoop, err: String) {
        self.result = Some(Err(err));
        event_loop.exit();
    }
}

impl ApplicationHandler for RenderedLatencyProbeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("Wrela Reference Host Latency Probe")
            .with_inner_size(winit::dpi::PhysicalSize::new(64, 64));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                window.set_visible(true);
                window.focus_window();
                match SurfaceState::create(window.clone()) {
                    Ok(surface) => {
                        let surface = Arc::new(Mutex::new(surface));
                        if let Ok(mut slot) = self.presentation_surface.lock() {
                            *slot = Some(surface.clone());
                        }
                        self.surface = Some(surface);
                        window.request_redraw();
                    }
                    Err(err) => self.record_error(event_loop, err),
                }
                self.window = Some(window);
            }
            Err(err) => self.record_error(event_loop, format!("latency probe window: {err}")),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => self.record_error(
                event_loop,
                "rendered latency probe window closed before completion".to_string(),
            ),
            WindowEvent::RedrawRequested => {
                if self.result.is_none()
                    && let Err(err) = self.drive_probe(event_loop)
                {
                    self.record_error(event_loop, err);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        } else if self.result.is_none()
            && self.surface.is_some()
            && let Err(err) = self.drive_probe(event_loop)
        {
            self.record_error(event_loop, err);
        }
    }
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
    refresh_hz: f64,
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
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
            refresh_hz: REFERENCE_FALLBACK_PRESENT_HZ,
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

fn resolved_present_mode_from_wgpu(mode: wgpu::PresentMode) -> ResolvedPresentMode {
    match mode {
        wgpu::PresentMode::Mailbox | wgpu::PresentMode::Immediate => ResolvedPresentMode::Mailbox,
        wgpu::PresentMode::FifoRelaxed => ResolvedPresentMode::FifoRelaxed,
        wgpu::PresentMode::Fifo | wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync => {
            ResolvedPresentMode::Fifo
        }
    }
}

fn reference_display_latency_estimate_nanos(surface: &PresentationSurfaceSlot) -> Option<u64> {
    let Some(surface) = surface.lock().ok().and_then(|guard| guard.clone()) else {
        return None;
    };
    let Ok(surface) = surface.lock() else {
        return None;
    };
    Some(estimated_present_to_photons_nanos(
        resolved_present_mode_from_wgpu(surface.selected_present_mode),
        surface.refresh_hz,
    ))
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
    pending_present: Arc<Mutex<Option<wgpu::SurfaceTexture>>>,
}

impl ReferencePreacquiredSwapchainHandle {
    fn new(
        delegate: wrela::presentation_exec::swapchain::DynSwapchainHandle,
        acquired: Arc<Mutex<Option<AcquiredTexture>>>,
        pending_present: Arc<Mutex<Option<wgpu::SurfaceTexture>>>,
    ) -> Self {
        Self {
            delegate,
            acquired,
            pending_present,
        }
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
        let mut pending = self
            .pending_present
            .lock()
            .map_err(|_| SwapchainError::Present("pending present lock poisoned".into()))?;
        if let Some(stale) = pending.take() {
            stale.present();
        }
        *pending = Some(texture);
        Ok(())
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
    plan: PresentationPlan,
    live_state: PresentationLiveStateRefs,
}

impl ReferencePresentationAdapter {
    fn with_surface_slot(
        surface: PresentationSurfaceSlot,
        plan: PresentationPlan,
        live_state: PresentationLiveStateRefs,
    ) -> Self {
        Self {
            surface,
            plan,
            live_state,
        }
    }
}

#[derive(Clone)]
struct PresentationLiveStateRefs {
    input_frame: Arc<Mutex<Option<InputFrame>>>,
    residency_report: Arc<Mutex<Option<ResidencyReport>>>,
    physics_solver: Arc<Mutex<PhysicsSolver>>,
    audio_state: Arc<Mutex<AudioAdapterFrameState>>,
    audio_output: Arc<ReferenceAudioOutput>,
    surface_pixel_readback: Option<Arc<Mutex<SurfacePixelReadback>>>,
}

struct ReferenceAudioOutput {
    ledger: Arc<VoiceLedger>,
    renderer: Mutex<VoiceRenderer>,
    mode: ReferenceAudioOutputMode,
    sample_block: Mutex<Vec<StereoFrame>>,
    block_size: usize,
    sample_rate: u32,
}

enum ReferenceAudioOutputMode {
    Device(VoiceOutputStream),
    Null {
        ring: SampleRing,
        underruns: Arc<AtomicU64>,
        reason: String,
    },
}

impl ReferenceAudioOutput {
    fn new(config: AudioConfig, ledger: Arc<VoiceLedger>) -> Self {
        let device_config = AudioDeviceConfig {
            sample_rate: config.sample_rate,
            block_size: config.block_size,
        };
        let force_null = std::env::var_os("WRELA_AUDIO_NULL").is_some()
            || std::env::var_os("WRELA_TEST_OFFSCREEN").is_some();
        let mode = if force_null {
            ReferenceAudioOutputMode::Null {
                ring: SampleRing::with_capacity((config.block_size as usize).max(1) * 8),
                underruns: Arc::new(AtomicU64::new(0)),
                reason: "null output forced by test/headless environment".to_string(),
            }
        } else {
            match wrela_runtime::audio::device::build_default_voice_output_stream(
                device_config,
                Arc::clone(&ledger),
            ) {
                Ok(stream) => match stream.play() {
                    Ok(()) => ReferenceAudioOutputMode::Device(stream),
                    Err(err) => ReferenceAudioOutputMode::Null {
                        ring: SampleRing::with_capacity((config.block_size as usize).max(1) * 8),
                        underruns: Arc::new(AtomicU64::new(0)),
                        reason: format!("default output stream play failed: {err}"),
                    },
                },
                Err(err) => ReferenceAudioOutputMode::Null {
                    ring: SampleRing::with_capacity((config.block_size as usize).max(1) * 8),
                    underruns: Arc::new(AtomicU64::new(0)),
                    reason: format!("default output stream unavailable: {err}"),
                },
            }
        };
        let block_size = (config.block_size as usize).max(1);
        Self {
            ledger,
            renderer: Mutex::new(VoiceRenderer::new(config.sample_rate)),
            mode,
            sample_block: Mutex::new(vec![StereoFrame::SILENCE; block_size]),
            block_size,
            sample_rate: config.sample_rate,
        }
    }

    fn underrun_counter(&self) -> Arc<AtomicU64> {
        match &self.mode {
            ReferenceAudioOutputMode::Device(stream) => stream.underrun_counter(),
            ReferenceAudioOutputMode::Null { underruns, .. } => Arc::clone(underruns),
        }
    }

    fn render_latest(&self) -> Result<ReferenceAudioOutputReport, EngineFrameError> {
        let snapshot = self.ledger.load();
        if let ReferenceAudioOutputMode::Device(_) = &self.mode {
            return Ok(ReferenceAudioOutputReport {
                mode: self.mode_label().to_string(),
                render_path: "callback".to_string(),
                fallback_reason: None,
                ledger_tick: snapshot.tick,
                voices: snapshot.voices.len(),
                rendered_frames: 0,
                pushed_frames: 0,
                sample_rate: self.sample_rate,
                block_size: self.block_size,
            });
        }
        let mut renderer = self
            .renderer
            .lock()
            .map_err(|_| EngineFrameError::Message("reference audio renderer poisoned".into()))?;
        let mut block = self
            .sample_block
            .lock()
            .map_err(|_| EngineFrameError::Message("reference audio block poisoned".into()))?;
        if block.len() != self.block_size {
            block.resize(self.block_size, StereoFrame::SILENCE);
        }
        let rendered = renderer.render_block(&snapshot.voices, &mut block);
        let pushed = match &self.mode {
            ReferenceAudioOutputMode::Device(_) => 0,
            ReferenceAudioOutputMode::Null { ring, .. } => {
                let pushed = ring.push_block(&block[..rendered]);
                let mut drain = vec![StereoFrame::SILENCE; pushed];
                let _ = ring.pop_block(&mut drain);
                pushed
            }
        };
        Ok(ReferenceAudioOutputReport {
            mode: self.mode_label().to_string(),
            render_path: "null_sink".to_string(),
            fallback_reason: self.null_reason().map(str::to_string),
            ledger_tick: snapshot.tick,
            voices: snapshot.voices.len(),
            rendered_frames: rendered,
            pushed_frames: pushed,
            sample_rate: self.sample_rate,
            block_size: self.block_size,
        })
    }

    fn mode_label(&self) -> &'static str {
        match &self.mode {
            ReferenceAudioOutputMode::Device(_) => "device",
            ReferenceAudioOutputMode::Null { .. } => "null",
        }
    }

    fn null_reason(&self) -> Option<&str> {
        match &self.mode {
            ReferenceAudioOutputMode::Device(_) => None,
            ReferenceAudioOutputMode::Null { reason, .. } => Some(reason.as_str()),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ReferenceAudioOutputReport {
    mode: String,
    render_path: String,
    fallback_reason: Option<String>,
    ledger_tick: u64,
    voices: usize,
    rendered_frames: usize,
    pushed_frames: usize,
    sample_rate: u32,
    block_size: usize,
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
            runs_after: vec![
                EngineSubsystemKind::StateAdvance,
                EngineSubsystemKind::Input,
                EngineSubsystemKind::System,
                EngineSubsystemKind::Residency,
                EngineSubsystemKind::Physics,
                EngineSubsystemKind::Audio,
            ],
            requires_gpu: has_surface,
            allows_hot_path_readback: false,
        };
        let [acquire_label, present_label] = PresentationFramegraph::swapchain_reporting_labels();
        let surface = Arc::clone(&self.surface);
        let plan = self.plan.clone();
        let live_state = self.live_state.clone();
        let display_latency_estimate_nanos = if has_surface {
            reference_display_latency_estimate_nanos(&surface)
        } else {
            Some(0)
        };
        let metrics = Arc::new(Mutex::new(ReferencePresentationMetrics::default()));
        let metrics_for_job = Arc::clone(&metrics);
        let audio_output_report = Arc::new(Mutex::new(None::<ReferenceAudioOutputReport>));
        let audio_output = Arc::clone(&live_state.audio_output);
        let audio_output_report_for_job = Arc::clone(&audio_output_report);
        let audio_render = builder.add_job(
            EngineSubsystemKind::Presentation,
            "audio.render_to_output",
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let report = audio_output.render_latest()?;
                *audio_output_report_for_job.lock().map_err(|_| {
                    EngineFrameError::Message("reference audio output report poisoned".into())
                })? = Some(report);
                Ok(())
            },
        );
        let acquired_texture = Arc::new(Mutex::new(None::<AcquiredTexture>));
        let pending_present_texture = Arc::new(Mutex::new(None::<wgpu::SurfaceTexture>));
        let acquire = if has_surface {
            let surface_for_acquire = Arc::clone(&surface);
            let acquired_for_job = Arc::clone(&acquired_texture);
            builder.add_job(
                EngineSubsystemKind::Presentation,
                acquire_label,
                EngineJobAffinity::External,
                EngineSpanDomain::PresentWait,
                vec![audio_render],
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
                vec![audio_render],
                false,
                1,
            )
        };
        let render_submit = if has_surface {
            let surface_for_submit = Arc::clone(&surface);
            let acquired_for_submit = Arc::clone(&acquired_texture);
            let pending_for_submit = Arc::clone(&pending_present_texture);
            let metrics_for_submit = Arc::clone(&metrics);
            builder.add_job(
                EngineSubsystemKind::Presentation,
                "presentation.swapchain_submit",
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                vec![acquire],
                true,
                move || {
                    let surface = surface_for_submit
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
                    let (native, extent) = {
                        let surface_guard = surface.lock().map_err(|_| {
                            EngineFrameError::Message("reference host surface lock poisoned".into())
                        })?;
                        let _ = &surface_guard.instance;
                        (surface_guard.gpu_context(), surface_guard.extent())
                    };
                    let attachments = allocate_attachment_resources_without_history(
                        &plan.frame,
                        extent.width,
                        extent.height,
                    )
                    .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                    let delegate = Arc::new(ReferenceSwapchainHandle::new(surface))
                        as wrela::presentation_exec::swapchain::DynSwapchainHandle;
                    let swapchain = Arc::new(ReferencePreacquiredSwapchainHandle::new(
                        delegate,
                        acquired_for_submit,
                        pending_for_submit,
                    ))
                        as wrela::presentation_exec::swapchain::DynSwapchainHandle;
                    let mut framegraph =
                        PresentationFramegraph::from_plan_and_gpu_resources_with_swapchain(
                            plan.clone(),
                            attachments,
                            native,
                            0,
                            Some(swapchain),
                        );
                    let submission = framegraph
                        .submit_segment(false)
                        .map_err(|err| EngineFrameError::Message(err.to_string()))?;
                    let mut metrics = metrics_for_submit.lock().map_err(|_| {
                        EngineFrameError::Message(
                            "reference host presentation metrics poisoned".into(),
                        )
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
            )
        } else {
            builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                "presentation.swapchain_submit",
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                vec![acquire],
                false,
                1,
            )
        };
        let gpu_complete = has_surface.then(|| {
            builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                "presentation.swapchain_gpu_complete",
                EngineJobAffinity::Gpu,
                EngineSpanDomain::Gpu,
                vec![render_submit],
                false,
                1,
            )
        });
        let present_dependencies =
            gpu_complete.map_or_else(|| vec![render_submit], |gpu| vec![gpu]);
        let pending_for_present = Arc::clone(&pending_present_texture);
        let surface_for_present = Arc::clone(&surface);
        let live_state_for_present = live_state.clone();
        let present = builder.add_job(
            EngineSubsystemKind::Presentation,
            present_label,
            EngineJobAffinity::External,
            EngineSpanDomain::PresentWait,
            present_dependencies,
            false,
            move || {
                if let Some(texture) = pending_for_present
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message(
                            "reference host pending present texture lock poisoned".into(),
                        )
                    })?
                    .take()
                {
                    present_reference_surface_frame(
                        &surface_for_present,
                        &live_state_for_present,
                        texture,
                    )?;
                    return Ok(());
                }
                let has_surface = surface_for_present
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message(
                            "reference host presentation surface slot poisoned".into(),
                        )
                    })?
                    .is_some();
                if !has_surface {
                    let mut metrics = metrics_for_job.lock().map_err(|_| {
                        EngineFrameError::Message(
                            "reference host presentation metrics poisoned".into(),
                        )
                    })?;
                    *metrics = ReferencePresentationMetrics::default();
                    return Ok(());
                }
                Err(EngineFrameError::Message(
                    "surface-backed presentation missing queued texture".into(),
                ))
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![audio_render],
            vec![present],
            move |timeline, ctx: &mut EngineFrameContext| {
                ctx.estimated_present_to_photons_nanos = display_latency_estimate_nanos;
                let elapsed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Presentation)
                    .map(|span| span.elapsed_micros())
                    .sum();
                let cpu_critical_path_micros = timeline
                    .spans
                    .iter()
                    .filter(|span| {
                        span.subsystem == EngineSubsystemKind::Presentation
                            && matches!(
                                span.domain,
                                EngineSpanDomain::Cpu | EngineSpanDomain::External
                            )
                    })
                    .map(|span| span.elapsed_micros())
                    .sum();
                let gpu_critical_path_micros = timeline
                    .spans
                    .iter()
                    .filter(|span| {
                        span.subsystem == EngineSubsystemKind::Presentation
                            && matches!(
                                span.domain,
                                EngineSpanDomain::Gpu | EngineSpanDomain::GpuWait
                            )
                    })
                    .map(|span| span.elapsed_micros())
                    .sum::<u128>();
                let wait_time_micros = timeline
                    .spans
                    .iter()
                    .filter(|span| {
                        span.subsystem == EngineSubsystemKind::Presentation
                            && span.domain == EngineSpanDomain::PresentWait
                    })
                    .map(|span| span.elapsed_micros())
                    .sum();
                let metrics = metrics.lock().map_err(|_| {
                    EngineFrameError::Message("reference host presentation metrics poisoned".into())
                })?;
                let mut notes = vec!["presentation_framegraph_swapchain_observed".to_string()];
                if surface
                    .lock()
                    .ok()
                    .and_then(|surface| surface.clone())
                    .and_then(|surface| {
                        surface
                            .lock()
                            .ok()
                            .map(|surface| surface.present_mode_was_downgraded)
                    })
                    .unwrap_or(false)
                {
                    ctx.violations
                        .push("presentation.fallback_to_vsync_fifo".to_string());
                    notes.push("presentation_fallback_to_vsync_fifo=true".to_string());
                }
                notes.extend(metrics.notes.clone());
                notes.extend(presentation_live_state_notes(&live_state));
                if let Some(report) = audio_output_report
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("reference audio output report poisoned".into())
                    })?
                    .clone()
                {
                    notes.push(format!(
                        "audio_output mode={} renderer={} tick={} voices={} rendered_frames={} pushed_frames={} sample_rate={} block_size={}",
                        report.mode,
                        report.render_path,
                        report.ledger_tick,
                        report.voices,
                        report.rendered_frames,
                        report.pushed_frames,
                        report.sample_rate,
                        report.block_size
                    ));
                    if let Some(reason) = report.fallback_reason {
                        notes.push(format!("audio_output_fallback={reason}"));
                    }
                } else {
                    notes.push("audio_output missing_report".to_string());
                }
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: 1,
                    cpu_critical_path_micros,
                    gpu_critical_path_micros: (gpu_critical_path_micros > 0)
                        .then_some(gpu_critical_path_micros),
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
                    wait_time_micros,
                    notes,
                })
            },
        ))
    }
}

fn presentation_live_state_notes(live_state: &PresentationLiveStateRefs) -> Vec<String> {
    let input_actions = live_state
        .input_frame
        .lock()
        .ok()
        .and_then(|frame| frame.as_ref().map(|frame| frame.actions.len()))
        .unwrap_or(0);
    let resident_regions = live_state
        .residency_report
        .lock()
        .ok()
        .and_then(|report| {
            report
                .as_ref()
                .map(|report| report.resident_region_ids.len())
        })
        .unwrap_or(0);
    let physics_bodies = live_state
        .physics_solver
        .lock()
        .map(|solver| solver.bodies().len())
        .unwrap_or(0);
    let audio_voices = live_state
        .audio_state
        .lock()
        .map(|state| state.plan.voices.len())
        .unwrap_or(0);
    vec![
        format!("live_input_actions={input_actions}"),
        format!("live_resident_regions={resident_regions}"),
        format!("live_physics_bodies={physics_bodies}"),
        format!("live_audio_voices={audio_voices}"),
    ]
}

fn present_reference_surface_frame(
    surface_slot: &PresentationSurfaceSlot,
    live_state: &PresentationLiveStateRefs,
    texture: wgpu::SurfaceTexture,
) -> Result<(), EngineFrameError> {
    let surface = surface_slot
        .lock()
        .map_err(|_| EngineFrameError::Message("reference host surface slot poisoned".into()))?
        .clone()
        .ok_or_else(|| {
            EngineFrameError::Message("reference surface presentation missing surface".into())
        })?;
    let surface = surface
        .lock()
        .map_err(|_| EngineFrameError::Message("reference host surface lock poisoned".into()))?;
    submit_reference_surface_content(
        &surface.gpu.device,
        &surface.gpu.queue,
        &texture.texture,
        reference_surface_color(live_state)?,
    )?;
    let Some(readback) = &live_state.surface_pixel_readback else {
        texture.present();
        return Ok(());
    };
    {
        readback
            .lock()
            .map_err(|_| EngineFrameError::Message("surface pixel readback lock poisoned".into()))?
            .submit_surface_texture_copy(
                &surface.gpu.device,
                &surface.gpu.queue,
                &texture.texture,
                surface.extent(),
            )
            .map_err(EngineFrameError::Message)?;
    }
    texture.present();
    readback
        .lock()
        .map_err(|_| EngineFrameError::Message("surface pixel readback lock poisoned".into()))?
        .collect_latest_pixels(&surface.gpu.device)
        .map_err(EngineFrameError::Message)
}

fn submit_reference_surface_content(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    color: wgpu::Color,
) -> Result<(), EngineFrameError> {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("wrela.reference_host.surface_content.encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wrela.reference_host.surface_content.render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit(Some(encoder.finish()));
    Ok(())
}

fn reference_surface_color(
    live_state: &PresentationLiveStateRefs,
) -> Result<wgpu::Color, EngineFrameError> {
    let input_active = live_state
        .input_frame
        .lock()
        .map_err(|_| EngineFrameError::Message("input frame lock poisoned".into()))?
        .as_ref()
        .map(input_frame_has_active_actions)
        .unwrap_or(false);
    let resident_regions = live_state
        .residency_report
        .lock()
        .ok()
        .and_then(|report| {
            report
                .as_ref()
                .map(|report| report.resident_region_ids.len())
        })
        .unwrap_or(0);
    let physics_bodies = live_state
        .physics_solver
        .lock()
        .map(|solver| solver.bodies().len())
        .unwrap_or(0);
    let audio_voices = live_state
        .audio_state
        .lock()
        .map(|state| state.plan.voices.len())
        .unwrap_or(0);
    Ok(wgpu::Color {
        r: if input_active { 0.86 } else { 0.12 },
        g: 0.18 + ((physics_bodies % 6) as f64 * 0.08),
        b: 0.22 + ((resident_regions.saturating_add(audio_voices) % 6) as f64 * 0.07),
        a: 1.0,
    })
}

fn input_frame_has_active_actions(frame: &InputFrame) -> bool {
    frame.actions.values().any(|state| match state {
        SemanticActionState::Button { pressed, .. } => *pressed,
        SemanticActionState::Axis1 { value } => value.abs() > f32::EPSILON,
        SemanticActionState::Axis2 { x, y } => x.abs() > f32::EPSILON || y.abs() > f32::EPSILON,
    })
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
        let project_runtime = match &config.project_path {
            Some(path) => ReferenceProjectRuntime::from_entry_path(path)?,
            None => ReferenceProjectRuntime::reference_default(),
        };
        let (host, controls) = new_input_driven_host(
            input_consumer,
            started_at,
            Arc::clone(&presentation_surface),
            None,
            project_runtime,
            REFERENCE_FALLBACK_PRESENT_HZ,
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

#[cfg(test)]
mod pixel_probe_tests {
    use super::*;

    fn write_project_fixture(name: &str, source: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wrela_reference_host_lib_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let path = dir.join("main.wr");
        std::fs::write(&path, source).expect("fixture source");
        path
    }

    #[test]
    fn project_runtime_uses_project_input_map_and_notes_system_fallback() {
        let project_path = write_project_fixture(
            "input_map_only",
            r#"
input_map Controls {
    action MoveForward = key.w
}

fn run() -> Integer {
    return 0
}
"#,
        );

        let runtime =
            ReferenceProjectRuntime::from_entry_path(&project_path).expect("project runtime");

        assert_eq!(runtime.input_map.id.0.as_str(), "Controls");
        assert_eq!(runtime.input_map.bindings.len(), 1);
        assert_eq!(runtime.input_map.bindings[0].source.as_str(), "keyboard");
        assert_eq!(runtime.input_map.bindings[0].detail.as_str(), "key.KeyW");
        assert!(runtime.allow_reference_system_invoker);
        assert!(
            runtime
                .system_notes
                .iter()
                .any(|note| note.contains("project-derived input_map `Controls`")),
            "missing project input note: {:?}",
            runtime.system_notes
        );
        assert!(
            runtime
                .system_notes
                .iter()
                .any(|note| note.contains("project defines no systems")),
            "missing system fallback note: {:?}",
            runtime.system_notes
        );
    }

    #[test]
    fn project_runtime_schedules_authored_systems() {
        let project_path = write_project_fixture(
            "authored_system",
            r#"
resource Transform {
    x: F32
}

@phase(sim)
system IntegrateTransforms(@mut transform: Transform) -> Nothing {
    return
}

fn run() -> Integer {
    return 0
}
"#,
        );

        let runtime =
            ReferenceProjectRuntime::from_entry_path(&project_path).expect("project runtime");

        assert!(!runtime.allow_reference_system_invoker);
        assert!(
            runtime
                .system_notes
                .iter()
                .any(|note| note.contains("scheduling 1 authored project system")),
            "unexpected notes: {:?}",
            runtime.system_notes
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_pixel_probe_reports_actual_first_pixel_delta() {
        let before = vec![0, 0, 0, 255, 0, 0, 0, 255];
        let after = vec![0, 0, 0, 255, 255, 255, 255, 255];

        assert_eq!(first_changed_pixel_index(&before, &after), Some(1));
        assert_eq!(first_changed_pixel_index(&before, &before), None);
    }
}
