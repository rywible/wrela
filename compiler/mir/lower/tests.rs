//! Owns MIR-lowering regression tests across helper synthesis, query lowering,
//! and semantic preservation surfaces.
//! Does not own MIR lowering implementation.
//!
//! Key invariants:
//! - tests here should assert authored-meaning preservation and helper routing,
//!   not just incidental MIR layout.
//! - regression fixtures must stay aligned with the canonical lowered helper
//!   naming and symbol rules used by the implementation.
//!
//! Primary entrypoints:
//! - MIR-lowering regression tests in this module
//!
//! Failure modes / common pitfalls:
//! - overfitting assertions to accidental MIR formatting makes legitimate
//!   refactors expensive without improving semantic coverage.

use super::*;
use crate::hir::lower as hir_lower;
use crate::hir::typeck;
use crate::parser::ast;
use crate::parser::ast::AstNode;
use crate::parser::parse;
use std::collections::{BTreeSet, HashMap, HashSet};

fn direct_call_targets(func: &MirFunction) -> BTreeSet<SmolStr> {
    let mut targets = BTreeSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let MirStmt::Assign {
                value:
                    Rvalue::Call {
                        target: CallTarget::Function(name),
                        ..
                    },
                ..
            } = stmt
            {
                targets.insert(name.clone());
            }
        }
    }
    targets
}

#[test]
fn test_lower_marks_suspendable() {
    let input = "\
class Whale {\n    fn swim() -> Boolean {\n        return true\n    }\n}\n\nfn f() -> Result[Boolean] {\n    w = detach Whale() * 1\n    return await w.swim()\n}\n";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let mir = lower_module(&module);
    let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(func.suspendable);
}

#[test]
fn test_lower_if_creates_blocks() {
    let input = "fn f() -> Nothing {\n    if true {\n        x = 1\n    } else {\n        x = 2\n    }\n}\n";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let mir = lower_module(&module);
    let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
    assert!(func.blocks.len() >= 3);
}

#[test]
fn test_lower_member_assign_sets_field() {
    let input = "\
class Counter {
mutable value: Integer
fn add(delta: Integer) -> Nothing {
    self.value += delta
}
}
";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);
    let func = mir_module
        .functions
        .iter()
        .find(|func| func.name == "Counter.add")
        .expect("missing Counter.add");
    let has_set_field = func.blocks.iter().any(|block| {
        block.stmts.iter().any(
            |stmt| matches!(stmt, MirStmt::SetField { field, .. } if field.as_str() == "value"),
        )
    });
    assert!(has_set_field, "expected SetField for member assign");
}

#[test]
fn test_lower_field_defaults_emits_set_fields() {
    let input = "\
class Foo {
x: Integer = 1
y: List[Integer] = [1, 2]
z: Map[String, Integer] = {\"a\": 1}
}

fn run() -> Nothing {
a = Foo()
b = Foo(x=5)
}
";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let mir_module = lower_module(&module);
    let func = mir_module
        .functions
        .iter()
        .find(|func| func.name == "run")
        .expect("missing run");

    let mut set_x = 0usize;
    let mut set_y = 0usize;
    let mut set_z = 0usize;
    let mut build_list = 0usize;
    let mut build_map = 0usize;

    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                MirStmt::SetField { field, .. } if field.as_str() == "x" => set_x += 1,
                MirStmt::SetField { field, .. } if field.as_str() == "y" => set_y += 1,
                MirStmt::SetField { field, .. } if field.as_str() == "z" => set_z += 1,
                _ => {}
            }
            if let MirStmt::Assign { value, .. } = stmt {
                match value {
                    Rvalue::BuildList { .. } => build_list += 1,
                    Rvalue::BuildMap { .. } => build_map += 1,
                    _ => {}
                }
            }
        }
    }

    assert_eq!(set_x, 2, "expected default and override for x");
    assert_eq!(set_y, 2, "expected defaults for y in both instances");
    assert_eq!(set_z, 2, "expected defaults for z in both instances");
    assert!(build_list >= 1, "expected BuildList for default list");
    assert!(build_map >= 1, "expected BuildMap for default map");
}

#[test]
fn test_capture_field_queries_lower_without_indirect_calls() {
    let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
sphere(radius = 1.0)
}

fn run() -> Nothing {
scene = capture sphere_field
distance = distance_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
normal = normal_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value: Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }

    assert!(
        indirect_calls.is_empty(),
        "unexpected indirect calls: {indirect_calls:?}"
    );
}

#[test]
fn test_batch_query_calls_route_through_phase9_generated_helpers() {
    let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(1.0, 0.0, 0.0),
    roughness=0.4,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = sphere_field
material = shade
payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

fn run() -> Nothing {
field_scene = capture sphere_field
shape_scene = capture orb_shape
points = [PointQuery(point=vec3(0.0, 0.0, 2.0))]
rays = [RayQuery(
    origin=vec3(0.0, 0.0, 3.0),
    direction=vec3(0.0, 0.0, -1.0),
    max_distance=6.0,
    min_step=0.05,
    hit_epsilon=0.001,
    max_steps=96
)]
distance_results = distance_at_batch(capture=field_scene, points=points, backend=dispatch_backend_cpu())
trace_results = trace_shape_batch(capture=shape_scene, rays=rays, backend=dispatch_backend_virtual_gpu())
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);
    let run = mir_module
        .functions
        .iter()
        .find(|func| func.name == "run")
        .expect("missing run");
    let targets = direct_call_targets(run);
    assert!(targets.contains("__wr_field_distance_batch_queries"));
    assert!(targets.contains("__wr_scene_trace_batch_queries"));
    assert!(
        mir_module
            .functions
            .iter()
            .any(|func| func.name == "__wr_field_distance_batch_queries")
    );
    assert!(
        mir_module
            .functions
            .iter()
            .any(|func| func.name == "__wr_scene_trace_batch_queries")
    );
}

#[test]
fn test_shadowed_family_namespace_method_call_does_not_lower_as_query() {
    let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
sphere(radius = 1.0)
}

class Probe {
fn distance(capture: FieldCapture, point: Vec3) -> F32 {
    return 7.0
}
}

fn run() -> F32 {
spatial = Probe()
field_scene = capture sphere_field
return spatial.distance(capture=field_scene, point=vec3(0.0, 0.0, 2.0))
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let (run_idx, run) = module
        .functions
        .iter()
        .find(|(_, func)| func.name == "run")
        .expect("missing run");
    let body = run.body.as_ref().expect("run body");
    let call_expr = body
        .exprs
        .iter()
        .find_map(|(expr_id, expr)| {
            let Expr::Call { callee, .. } = expr else {
                return None;
            };
            let Expr::Member { object, member, .. } = &body.exprs[*callee] else {
                return None;
            };
            let Expr::Variable(object_name) = &body.exprs[*object] else {
                return None;
            };
            (object_name.as_str() == "spatial" && member.as_str() == "distance").then_some(expr_id)
        })
        .expect("spatial.distance call");
    let fn_info = type_info.function(run_idx).expect("run type info");
    let lowerer = FunctionLowerer::new(
        SmolStr::new("run"),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
        false,
        Some(fn_info),
    );
    assert!(
        lowerer.parse_scalar_query(body, call_expr).is_none(),
        "shadowed method call was parsed as an intrinsic query"
    );
}

#[test]
fn test_generated_batch_helpers_execute_concrete_scene_paths() {
    let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(1.0, 0.0, 0.0),
    roughness=0.4,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = sphere_field
material = shade
payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    let distance_helper = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_field_distance_batch_queries")
        .expect("distance batch helper");
    let trace_helper = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_scene_trace_batch_queries")
        .expect("trace batch helper");

    let distance_targets = direct_call_targets(distance_helper);
    let trace_targets = direct_call_targets(trace_helper);
    assert!(distance_targets.contains("sphere_field"));
    assert!(trace_targets.contains("__wr_shape_trace_orb_shape"));
    assert!(!distance_targets.contains("__wr_field_distance_capture"));
    assert!(!distance_targets.contains("__wr_field_normal_capture"));
    assert!(!distance_targets.contains("__wr_shape_distance_capture"));
    assert!(!distance_targets.contains("__wr_shape_normal_capture"));
    assert!(!trace_targets.contains("__wr_scene_trace_capture"));
    assert!(!trace_targets.contains("__wr_scene_surface_capture"));
    assert!(trace_targets.contains("__wr_gpu_dispatch_begin"));
    assert!(trace_targets.contains("__wr_gpu_dispatch_select_invocation"));
    assert!(trace_targets.contains("__wr_gpu_dispatch_end"));
}

#[test]
fn test_phase9_helpers_route_opaque_scenes_to_conservative_kernels() {
    let input = r#"field exact distance semantic_field(p: Vec3) -> F32 {
sphere(radius = 1.0)
}

field conservative distance opaque_field(p: Vec3) -> F32 {
support = Support3(bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
))
bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
)
return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
return Surface(
    albedo = vec3(1.0, 0.0, 0.0),
    roughness = 0.4,
    metalness = 0.0,
    clearcoat = 0.0,
    clearcoat_roughness = 0.0,
    sheen = 0.0,
    emissive = vec3(0.0, 0.0, 0.0)
)
}

shape semantic_scene {
field = semantic_field
material = shade
payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape opaque_scene {
field = opaque_field
material = shade
payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    for helper in [
        "__wr_shape_distance_semantic_scene",
        "__wr_shape_distance_conservative_semantic_scene",
        "__wr_shape_distance_opaque_scene",
        "__wr_shape_distance_conservative_opaque_scene",
        "__wr_shape_trace_semantic_scene",
        "__wr_shape_trace_conservative_semantic_scene",
        "__wr_shape_trace_opaque_scene",
        "__wr_shape_trace_conservative_opaque_scene",
    ] {
        assert!(
            mir_module.functions.iter().any(|func| func.name == helper),
            "expected generated helper `{helper}` to exist"
        );
    }

    let shape_distance_capture = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_shape_distance_capture")
        .expect("shape distance capture helper");
    let scene_trace_capture = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_scene_trace_capture")
        .expect("scene trace capture helper");
    let shape_distance_batch = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_shape_distance_batch_queries")
        .expect("shape distance batch helper");
    let scene_trace_batch = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_scene_trace_batch_queries")
        .expect("scene trace batch helper");

    let shape_distance_capture_targets = direct_call_targets(shape_distance_capture);
    let scene_trace_capture_targets = direct_call_targets(scene_trace_capture);
    let shape_distance_batch_targets = direct_call_targets(shape_distance_batch);
    let scene_trace_batch_targets = direct_call_targets(scene_trace_batch);

    for targets in [
        &shape_distance_capture_targets,
        &shape_distance_batch_targets,
    ] {
        assert!(targets.contains("__wr_shape_distance_semantic_scene"));
        assert!(targets.contains("__wr_shape_distance_conservative_opaque_scene"));
        assert!(!targets.contains("__wr_shape_distance_conservative_semantic_scene"));
        assert!(!targets.contains("__wr_shape_distance_opaque_scene"));
    }

    for targets in [&scene_trace_capture_targets, &scene_trace_batch_targets] {
        assert!(targets.contains("__wr_shape_trace_semantic_scene"));
        assert!(targets.contains("__wr_shape_trace_conservative_opaque_scene"));
        assert!(!targets.contains("__wr_shape_trace_conservative_semantic_scene"));
        assert!(!targets.contains("__wr_shape_trace_opaque_scene"));
    }
}

#[test]
fn test_scalar_shape_trace_skips_support_prune_scaffold_for_opaque_branches() {
    let input = r#"field exact distance near_field(p: Vec3) -> F32 {
sphere(radius = 0.65)
}

field conservative distance far_custom(p: Vec3) -> F32 {
support = Support3(bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
))
bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
)
return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_semantic(p: Vec3) -> F32 {
translate = vec3(10.0, 0.0, 0.0) {
    sphere(radius = 0.5)
}
}

material shade(hit: Hit3) -> Surface {
return Surface(
    albedo = vec3(1.0, 0.0, 0.0),
    roughness = 0.2,
    metalness = 0.0,
    clearcoat = 0.0,
    clearcoat_roughness = 0.0,
    sheen = 0.0,
    emissive = vec3(0.0, 0.0, 0.0)
)
}

shape near_shape {
field = near_field
material = shade
payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape far_custom_shape {
field = far_custom
material = shade
payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}

shape far_semantic_shape {
field = far_semantic
material = shade
payload = Payload(entity_id = u32(3), material_id = u32(3), actor = ActorHandle(id = u32(3), generation = u32(0)))
}

shape supported_scene {
union {
    provenance_policy = nearest
    use near_shape
    use far_custom_shape
}
}

shape semantic_scene {
union {
    provenance_policy = nearest
    use near_shape
    use far_semantic_shape
}
}

fn main() -> Integer {
supported = trace_shape(
    capture = capture supported_scene,
    ray = ray_query(
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
)
semantic = trace_shape(
    capture = capture semantic_scene,
    ray = ray_query(
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
)
if supported.hit && semantic.hit {
    return 0
}
return 1
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    let function_names = mir_module
        .functions
        .iter()
        .map(|func| func.name.clone())
        .collect::<Vec<_>>();
    let metric_callers = mir_module
        .functions
        .iter()
        .filter_map(|func| {
            direct_call_targets(func)
                .contains("__wr_metrics_scene_trace_support_pruned_branch")
                .then_some(func.name.clone())
        })
        .collect::<Vec<_>>();

    assert!(
        !metric_callers
            .iter()
            .any(|name| name.contains("supported_scene")),
        "optimized MIR still prunes opaque scene branches: callers={metric_callers:?} functions={function_names:?}"
    );
    assert!(
        metric_callers
            .iter()
            .any(|name| name.contains("semantic_scene")),
        "optimized MIR lost semantic support pruning: callers={metric_callers:?} functions={function_names:?}"
    );
}

#[test]
fn test_opt_scalar_shape_trace_skips_support_prune_scaffold_for_opaque_branches() {
    let input = r#"field exact distance near_field(p: Vec3) -> F32 {
sphere(radius = 0.65)
}

field conservative distance far_custom(p: Vec3) -> F32 {
support = Support3(bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
))
bounds = Bounds3(
    min = vec3(8.0, -1.0, -1.0),
    max = vec3(12.0, 1.0, 1.0)
)
return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_semantic(p: Vec3) -> F32 {
translate = vec3(10.0, 0.0, 0.0) {
    sphere(radius = 0.5)
}
}

material shade(hit: Hit3) -> Surface {
return Surface(
    albedo = vec3(1.0, 0.0, 0.0),
    roughness = 0.2,
    metalness = 0.0,
    clearcoat = 0.0,
    clearcoat_roughness = 0.0,
    sheen = 0.0,
    emissive = vec3(0.0, 0.0, 0.0)
)
}

shape near_shape {
field = near_field
material = shade
payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape far_custom_shape {
field = far_custom
material = shade
payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}

shape far_semantic_shape {
field = far_semantic
material = shade
payload = Payload(entity_id = u32(3), material_id = u32(3), actor = ActorHandle(id = u32(3), generation = u32(0)))
}

shape supported_scene {
union {
    provenance_policy = nearest
    use near_shape
    use far_custom_shape
}
}

shape semantic_scene {
union {
    provenance_policy = nearest
    use near_shape
    use far_semantic_shape
}
}

fn main() -> Integer {
supported = trace_shape(
    capture = capture supported_scene,
    ray = ray_query(
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
)
semantic = trace_shape(
    capture = capture semantic_scene,
    ray = ray_query(
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
)
if supported.hit && semantic.hit {
    return 0
}
return 1
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let check_ir = crate::hir::checkir::extract_module(&module);
    let mut mir_module = lower_module_with_types(&module, &type_info);
    let analysis = crate::mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        crate::mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

    let function_names = mir_module
        .functions
        .iter()
        .map(|func| func.name.clone())
        .collect::<Vec<_>>();
    let metric_callers = mir_module
        .functions
        .iter()
        .filter_map(|func| {
            direct_call_targets(func)
                .contains("__wr_metrics_scene_trace_support_pruned_branch")
                .then_some(func.name.clone())
        })
        .collect::<Vec<_>>();

    assert!(
        !metric_callers
            .iter()
            .any(|name| name.contains("supported_scene")),
        "optimized MIR still prunes opaque scene branches: callers={metric_callers:?} functions={function_names:?}"
    );
    assert!(
        metric_callers
            .iter()
            .any(|name| name.contains("semantic_scene")),
        "optimized MIR lost semantic support pruning: callers={metric_callers:?} functions={function_names:?}"
    );
}

#[test]
fn test_shape_queries_with_semantic_wrappers_lower_without_indirect_calls() {
    let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
translate = vec3(0.8, 0.0, 0.0) {
    sphere(radius=0.65)
}
}

material orb_surface(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(32.0, 64.0, 255.0),
    roughness=0.2,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = translated_sphere
material = orb_surface
payload = Payload(
    entity_id=u32(2),
    material_id=u32(22),
    actor=ActorHandle(id=u32(202), generation=u32(0))
)
}

fn run() -> Nothing {
scene = capture orb_shape
hit = trace_shape(
    capture=scene,
    ray=ray_query(
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
)
surface = surface_at(capture=scene, hit=hit)
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value: Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }

    assert!(
        indirect_calls.is_empty(),
        "unexpected indirect calls: {indirect_calls:?}"
    );
}

#[test]
fn test_shape_queries_with_semantic_wrappers_stay_direct_after_opt() {
    let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
translate = vec3(0.8, 0.0, 0.0) {
    sphere(radius=0.65)
}
}

material orb_surface(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(32.0, 64.0, 255.0),
    roughness=0.2,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = translated_sphere
material = orb_surface
payload = Payload(
    entity_id=u32(2),
    material_id=u32(22),
    actor=ActorHandle(id=u32(202), generation=u32(0))
)
}

fn run() -> Nothing {
scene = capture orb_shape
hit = trace_shape(
    capture=scene,
    ray=ray_query(
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
)
surface = surface_at(capture=scene, hit=hit)
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let check_ir = crate::hir::checkir::extract_module(&module);
    let mut mir_module = lower_module_with_types(&module, &type_info);
    let analysis = crate::mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        crate::mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value: Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }

    assert!(
        indirect_calls.is_empty(),
        "unexpected indirect calls after optimization: {indirect_calls:?}"
    );
}

#[test]
fn test_trace_metrics_shape_query_path_stays_direct_after_opt() {
    let input = r#"field exact distance orb(p: Vec3) -> F32 {
translate = vec3(0.8, 0.0, 0.0) {
    sphere(radius=0.65)
}
}

material orb_surface(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(32.0, 64.0, 255.0),
    roughness=0.2,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = orb
material = orb_surface
payload = Payload(
    entity_id=u32(2),
    material_id=u32(22),
    actor=ActorHandle(id=u32(202), generation=u32(0))
)
}

fn run() -> Nothing {
orb_scene = capture orb_shape
exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
hit = trace_shape(
    capture=orb_scene,
    ray=ray_query(
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
)
field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
surface = surface_at(capture=orb_scene, hit=hit)
exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let check_ir = crate::hir::checkir::extract_module(&module);
    let mut mir_module = lower_module_with_types(&module, &type_info);
    let analysis = crate::mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        crate::mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value: Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }

    assert!(
        indirect_calls.is_empty(),
        "unexpected indirect calls after optimization: {indirect_calls:?}"
    );
}

#[test]
fn test_trace_metrics_assertions_stay_direct_after_opt() {
    let input = r#"field exact distance orb(p: Vec3) -> F32 {
translate = vec3(0.8, 0.0, 0.0) {
    sphere(radius=0.65)
}
}

material orb_surface(hit: Hit3) -> Surface {
return Surface(
    albedo=vec3(32.0, 64.0, 255.0),
    roughness=0.2,
    metalness=0.0,
    clearcoat=0.0,
    clearcoat_roughness=0.0,
    sheen=0.0,
    emissive=vec3(0.0, 0.0, 0.0)
)
}

shape orb_shape {
field = orb
material = orb_surface
payload = Payload(
    entity_id=u32(2),
    material_id=u32(22),
    actor=ActorHandle(id=u32(202), generation=u32(0))
)
}

fn run() -> Nothing {
orb_scene = capture orb_shape
exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
conservative_before = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
hit_count_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
hit_steps_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
hit_samples_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
field_samples_before = __wr_metrics_get(__wr_metrics_field_sample_id())
bucket_before = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

hit = trace_shape(
    capture=orb_scene,
    ray=ray_query(
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
)
field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
surface = surface_at(capture=orb_scene, hit=hit)

exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
conservative_after = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
hit_count_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
hit_steps_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
hit_samples_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
field_samples_after = __wr_metrics_get(__wr_metrics_field_sample_id())
bucket_after = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
    + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

assert value hit.hit == true
assert value hit.payload.material_id == u32(22)
assert approx surface.albedo.x ~= 32.0 within 0.001
assert approx surface.albedo.z ~= 255.0 within 0.001
assert value exact_after - exact_before == 1
assert value conservative_after - conservative_before == 0
assert value hit_count_after - hit_count_before == 1
assert value hit_steps_after - hit_steps_before == hit.steps
assert value hit_samples_after - hit_samples_before == field_samples_after_trace - field_samples_before
assert value bucket_after - bucket_before == 1
}
"#;
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let check_ir = crate::hir::checkir::extract_module(&module);
    let mut mir_module = lower_module_with_types(&module, &type_info);
    let analysis = crate::mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        crate::mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value: Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }

    assert!(
        indirect_calls.is_empty(),
        "unexpected indirect calls after optimization: {indirect_calls:?}"
    );
}

#[test]
fn test_lower_integer_range_for_uses_typed_induction_fast_path() {
    let input = "\
fn run() -> Integer {
start = 1
stop = 4
mutable total = 0
for i in start...stop {
    total += i
}
return total
}
";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);
    let func = mir_module
        .functions
        .iter()
        .find(|func| func.name == "run")
        .expect("missing run");

    assert!(
        func.locals
            .iter()
            .any(|local| local.name.as_str() == "i" && local.ty == MirType::Integer),
        "expected typed loop variable for integer range",
    );
    assert!(
        func.locals
            .iter()
            .any(|local| local.name.starts_with("$range_idx") && local.ty == MirType::Integer),
        "expected typed integer induction local",
    );
    assert!(
        func.locals
            .iter()
            .any(|local| local.name.starts_with("$range_step") && local.ty == MirType::Integer),
        "expected typed integer step local",
    );

    for block in &func.blocks {
        for stmt in &block.stmts {
            assert!(
                !matches!(stmt, MirStmt::IterInit { .. } | MirStmt::IterNext { .. }),
                "typed integer range loop should not use iterator protocol",
            );
            if let MirStmt::Assign { value, .. } = stmt {
                assert!(
                    !matches!(
                        value,
                        Rvalue::Binary {
                            op: crate::hir::BinaryOp::Range,
                            ..
                        }
                    ),
                    "typed integer range loop should not materialize range object",
                );
            }
        }
    }
}

#[test]
fn test_lower_member_field_ops_emit_slot_hints() {
    let input = "\
class Counter {
mutable value: Integer
mutable other: Integer

fn bump() -> Nothing {
    self.value += 1
    self.other = 4
}
}

fn run() -> Integer {
c = Counter(value=1, other=2)
c.value += 3
return c.other
}
";
    let node = parse(input);
    let root = ast::Root::cast(node).unwrap();
    let module = hir_lower::lower(root);
    let (_type_errors, type_info) = typeck::check_module_with_info(&module);
    let mir_module = lower_module_with_types(&module, &type_info);

    let mut saw_get_value_slot = false;
    let mut saw_get_other_slot = false;
    let mut saw_set_value_slot = false;
    let mut saw_set_other_slot = false;

    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                match stmt {
                    MirStmt::Assign {
                        value:
                            Rvalue::GetField {
                                field,
                                slot: Some(slot),
                                ..
                            },
                        ..
                    } if field.as_str() == "value" && *slot == 0 => saw_get_value_slot = true,
                    MirStmt::Assign {
                        value:
                            Rvalue::GetField {
                                field,
                                slot: Some(slot),
                                ..
                            },
                        ..
                    } if field.as_str() == "other" && *slot == 1 => saw_get_other_slot = true,
                    MirStmt::SetField {
                        field,
                        slot: Some(slot),
                        ..
                    } if field.as_str() == "value" && *slot == 0 => saw_set_value_slot = true,
                    MirStmt::SetField {
                        field,
                        slot: Some(slot),
                        ..
                    } if field.as_str() == "other" && *slot == 1 => saw_set_other_slot = true,
                    _ => {}
                }
            }
        }
    }

    assert!(saw_get_value_slot, "expected slot-hinted get for value");
    assert!(saw_get_other_slot, "expected slot-hinted get for other");
    assert!(saw_set_value_slot, "expected slot-hinted set for value");
    assert!(saw_set_other_slot, "expected slot-hinted set for other");
}
