use smol_str::SmolStr;
use std::path::PathBuf;
use std::process::Command;
use wrela::collision_contract::CollisionResult;
use wrela::collision_exec::cpu::execute as execute_collision_plan;
use wrela::collision_plan::{CollisionPlan, CollisionQueryKind};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue, lower_world_query_plan};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::presentation_plan::PresentationPlan;
use wrela::query_exec::{QueryExecContext, execute_world_query, stable_region_scene_capture_id};
use wrela::query_plan::{DispatchBackend, WorldQueryKind, WorldQueryPlan};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_module(source: &str) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    let ctx = QueryExecContext::compile(&module, &type_info);
    (module, type_info, ctx)
}

fn view_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing view function `{name}`"))
}

fn smoke_world_source() -> &'static str {
    r#"
field exact distance smoke_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material smoke_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.5, 0.7),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape smoke_shape {
    field = smoke_field
    material = smoke_material
}

region smoke_region() {
    place scene = smoke_shape
}

domain smoke_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.02
    hit_epsilon = 0.0005
    max_steps = 128
}

view smoke_view(world: RegionCapture, camera: Camera) {
    domain = smoke_domain(world = world)
    width = 4
    height = 4
    key_light = Light(
        position = vec3(1.8, 2.4, 2.2),
        direction = normalize(vec3(-0.5, -0.8, -0.6)),
        intensity = vec3(1.0, 0.98, 0.95),
        range = 8.0
    )
    fill_direction = normalize(vec3(-0.7, 0.45, 0.2))
    fill_strength = 0.22
    ambient_color = vec3(0.12, 0.12, 0.12)
}
"#
}

fn scene_domain(scene_id: u32) -> KernelValue {
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

fn region_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

fn collision_point_input(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

#[test]
fn repo_smoke_frontend_typecheck_and_query_exec_cpu_roundtrip() {
    let (_module, _type_info, ctx) = typed_module(smoke_world_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("smoke_region"));
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::Distance,
        DispatchBackend::Cpu,
    ));
    let result = execute_world_query(
        &ctx,
        &plan,
        &[
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            KernelValue::Vec3([0.0, 0.0, 0.0]),
        ],
    )
    .expect("execute distance query");
    match result {
        KernelValue::F32(distance) => {
            assert!(distance < 0.0, "expected inside hit, got {distance}")
        }
        other => panic!("expected F32 distance result, got {other:?}"),
    }
}

#[test]
fn repo_smoke_presentation_plan_compiles_named_view() {
    let (module, _type_info, _ctx) = typed_module(smoke_world_source());
    let view = view_function(&module, "smoke_view");
    let plan = PresentationPlan::from_view_function(view, DispatchBackend::Auto)
        .expect("presentation plan");
    assert!(
        !plan.passes.is_empty(),
        "expected at least one presentation pass"
    );
}

#[test]
fn repo_smoke_collision_point_occupancy_executes_on_cpu() {
    let (_module, _type_info, ctx) = typed_module(smoke_world_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("smoke_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::PointOccupancyWorld,
        DispatchBackend::Cpu,
    );
    let (result, _trace) = execute_collision_plan(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 1),
            scene_domain(scene_id),
            collision_point_input([0.0, 0.0, 0.0]),
        ],
    )
    .expect("execute collision plan");
    match result {
        CollisionResult::Occupancy(occupancy) => assert!(occupancy.occupied),
        other => panic!("expected occupancy result, got {other:?}"),
    }
}

#[test]
fn repo_smoke_cli_help_mentions_fast_and_full_lane_aliases() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--help")
        .output()
        .expect("run wrela --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fast|full|spec|integration|sim|model|default"));
    assert!(stdout.contains("fast=spec+default, full=all lanes"));
}

#[test]
fn repo_smoke_micro_perf_manifest_loads_via_wrela_perf() {
    let bench_root = repo_root().join("benchmarks/micro");
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = temp.path().join("micro_smoke.toml");
    let baseline = temp.path().join("micro_smoke.json");
    std::fs::write(
        &manifest_path,
        r#"
version = 1
suite = "micro_repo_smoke"

[profiles.smoke]
warmup_pairs = 1
measure_pairs = 1
coverage = "critical"

[[scenarios]]
id = "check_given_boolean_lane"
test_name = "tests/micro::test_check_given_boolean_lane_ops_12000000"
ops = 12000000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false
"#,
    )
    .expect("write micro smoke manifest");

    // Use a one-scenario perf run so the fast lane proves the real manifest-loading path
    // without pulling the full benchmark suite into the default repo verification budget.
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(&bench_root)
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=smoke")
        .arg(format!("--benchmark-manifest={}", manifest_path.display()))
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(".")
        .output()
        .expect("run wrela perf");
    assert!(
        output.status.success(),
        "micro perf smoke failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(baseline.exists(), "expected micro perf baseline");

    let payload: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read micro baseline"))
            .expect("valid micro perf baseline");
    let cases = payload
        .get("summary")
        .and_then(|value| value.get("cases"))
        .and_then(|value| value.as_array())
        .expect("summary.cases array");
    assert_eq!(cases.len(), 1);
    assert_eq!(
        cases[0].get("name").and_then(|value| value.as_str()),
        Some("tests/micro::test_check_given_boolean_lane_ops_12000000")
    );
}
