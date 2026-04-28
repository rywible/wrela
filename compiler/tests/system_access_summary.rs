use wrela::hir::{self, FunctionRole};
use wrela::input_contract::InputFrame;
use wrela::mir::passes::system_access::{
    build_system_program_from_module, summarize_system_access,
};
use wrela::parser::{ast, ast::AstNode, parse};
use wrela::system_contract::{EventTypeId, SystemPhase, SystemResourceId};
use wrela::system_exec::{CompiledSystemRuntime, SystemValue};
use wrela::time_semantics::SimulationTick;
use wrela::world_identity::SnapshotEpoch;

fn lower_module(input: &str) -> hir::Module {
    let node = parse(input);
    let root = ast::Root::cast(node).expect("root");
    hir::lower::lower(root)
}

#[test]
fn phase_attribute_lowers_to_system_metadata() {
    let module = lower_module(
        r#"
@phase(sim)
system IntegrateTransforms() -> Nothing {
    return
}
"#,
    );
    let system = module
        .functions
        .iter()
        .find_map(|(_, func)| (func.role == FunctionRole::System).then_some(func))
        .expect("system");
    let metadata = system.system_metadata.as_ref().expect("system metadata");

    assert_eq!(metadata.phase.as_deref(), Some("sim"));
}

#[test]
fn annotation_driven_access_summary_uses_params_conservatively() {
    let module = lower_module(
        r#"
resource Transform {
    x: F32
}
resource Velocity {
    x: F32
}
event FrameEvent {
    tick: Integer
}
@phase(sim)
system IntegrateTransforms(input: InputFrame, velocity: Velocity, @mut transform: Transform, events: EventEmitter[FrameEvent]) -> Nothing {
    return
}
"#,
    );
    let system = module
        .functions
        .iter()
        .find_map(|(_, func)| (func.name == "IntegrateTransforms").then_some(func))
        .expect("system");
    let summary = summarize_system_access(system);

    assert!(summary.reads.contains(&SystemResourceId::InputFrame));
    assert!(
        summary
            .reads
            .contains(&SystemResourceId::Resource("Velocity".into()))
    );
    assert!(
        summary
            .writes
            .contains(&SystemResourceId::Resource("Transform".into()))
    );
    assert!(
        summary
            .emits_events
            .contains(&EventTypeId::new("FrameEvent"))
    );
}

#[test]
fn system_program_builder_uses_phase_attribute_and_access_summary() {
    let module = lower_module(
        r#"
resource Transform {
    x: F32
}
@phase(post_sim)
system EmitFrameEvents(transform: Transform) -> Nothing {
    return
}
"#,
    );

    let program = build_system_program_from_module(&module).expect("system program");
    let plans = program.phase(SystemPhase::PostSim);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].id.0.as_str(), "EmitFrameEvents");
    assert!(
        plans[0]
            .access
            .reads
            .contains(&SystemResourceId::Resource("Transform".into()))
    );
}

#[test]
fn compiled_system_runtime_from_project_builds_program_and_executes_hir_body() {
    let source = r#"
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
"#;
    let project = hir::project::LoadedProject {
        module: lower_module(source),
        entry_source: source.to_string(),
        warnings: Vec::new(),
        function_effects: Vec::new(),
        source_modules: Vec::new(),
        module_sources: std::collections::HashMap::new(),
        provenance: hir::project::ProjectProvenance::default(),
    };
    let mut runtime = CompiledSystemRuntime::from_project(&project).expect("compiled runtime");

    assert_eq!(runtime.program.phase(SystemPhase::Sim).len(), 1);
    let report = runtime
        .executor
        .run_program(
            &runtime.program,
            &InputFrame {
                epoch: SnapshotEpoch(0),
                tick: SimulationTick::new(0),
                actions: Default::default(),
            },
        )
        .expect("compiled system runtime should execute authored system body");
    assert_eq!(report.records.len(), 1);
}

#[test]
fn authored_system_runtime_binds_context_resources_dt_and_event_emitters() {
    let source = r#"
resource Transform {
    x: F32
}
event FrameEvent {
    tick: Integer
}
@phase(sim)
system IntegrateTransforms(input: InputFrame, @mut transform: Transform, events: EventEmitter[FrameEvent]) -> Nothing {
    transform.x = transform.x + dt()
    events.send(input.action_count)
    return
}
"#;
    let project = hir::project::LoadedProject {
        module: lower_module(source),
        entry_source: source.to_string(),
        warnings: Vec::new(),
        function_effects: Vec::new(),
        source_modules: Vec::new(),
        module_sources: std::collections::HashMap::new(),
        provenance: hir::project::ProjectProvenance::default(),
    };
    let mut runtime = CompiledSystemRuntime::from_project(&project).expect("compiled runtime");
    runtime
        .executor
        .set_default_simulation_dt_seconds(1.0 / 120.0);
    runtime
        .executor
        .resources()
        .lock()
        .expect("resources")
        .set_member("Transform".into(), "x".into(), SystemValue::Float(1.0));

    let report = runtime
        .executor
        .run_program(
            &runtime.program,
            &InputFrame {
                epoch: SnapshotEpoch(7),
                tick: SimulationTick::new(11),
                actions: Default::default(),
            },
        )
        .expect("authored system should execute with invocation context");

    let x = runtime
        .executor
        .resources()
        .lock()
        .expect("resources")
        .get_member(&"Transform".into(), &"x".into())
        .cloned();
    assert_eq!(x, Some(SystemValue::Float(1.0 + (1.0 / 120.0))));
    assert_eq!(report.records.len(), 1);
    assert!(
        report.records[0]
            .emitted_events
            .contains(&EventTypeId::new("FrameEvent"))
    );
}
