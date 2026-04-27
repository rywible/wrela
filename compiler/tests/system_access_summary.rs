use wrela::hir::{self, FunctionRole};
use wrela::mir::passes::system_access::{
    build_system_program_from_module, summarize_system_access,
};
use wrela::parser::{ast, ast::AstNode, parse};
use wrela::system_contract::{EventTypeId, SystemPhase, SystemResourceId};
use wrela::system_exec::{CompiledSystemRuntime, CompiledSystemRuntimeError};

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
fn compiled_system_runtime_from_project_fails_with_typed_unsupported_backend() {
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
    let err =
        CompiledSystemRuntime::from_project(&project).expect_err("production MIR backend missing");

    assert!(matches!(
        err,
        CompiledSystemRuntimeError::UnsupportedBackend { system_count: 1 }
    ));
}
