use smol_str::SmolStr;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use wrela::collision_plan::{CollisionPlan, CollisionQueryKind};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{lower_world_query_plan, KernelStructValue, KernelValue};
use wrela::parser::{self, ast::AstNode};
use wrela::query_exec::{execute_world_query, stable_region_scene_capture_id, QueryExecContext};
use wrela::query_plan::WorldQueryPlan;

struct CountingAlloc;
static COUNT_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn main() {
    let source = build_linear_clutter_fixture(96);
    let ctx = typed_query_context(&source);
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("bench_region"));
    let capture = KernelValue::Capture(SmolStr::new("bench_region"));
    let domain = scene_domain(scene_id, 1, true, false, false);
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(
        wrela::query_plan::WorldQueryKind::Distance,
    ));
    let args = [
        capture.clone(),
        domain.clone(),
        KernelValue::Vec3([0.0, 0.0, 0.25]),
    ];

    for _ in 0..10 {
        let _ = execute_world_query(&ctx, &plan, &args).expect("warmup query");
    }
    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_ALLOCATIONS.store(true, Ordering::Relaxed);
    let query_start = Instant::now();
    for _ in 0..200 {
        let _ = execute_world_query(&ctx, &plan, &args).expect("measured query");
    }
    let query_elapsed_ms = query_start.elapsed().as_secs_f64() * 1000.0;
    COUNT_ALLOCATIONS.store(false, Ordering::Relaxed);
    let query_allocations = ALLOCATIONS.swap(0, Ordering::Relaxed);

    let collision_plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let collision_args = [
        region_capture(scene_id, 2),
        scene_domain(scene_id, 1, true, false, false),
        collision_point_input([0.0, 0.0, 0.25]),
    ];
    for _ in 0..10 {
        let _ = collision_plan
            .execute(&ctx, &collision_args)
            .expect("warmup collision");
    }
    let collision_start = Instant::now();
    for _ in 0..500 {
        let _ = collision_plan
            .execute(&ctx, &collision_args)
            .expect("measured collision");
    }
    let collision_elapsed_ms = collision_start.elapsed().as_secs_f64() * 1000.0;

    println!(
        "query_allocations={} query_elapsed_ms={:.3} collision_elapsed_ms={:.3}",
        query_allocations, query_elapsed_ms, collision_elapsed_ms
    );
}

fn typed_query_context(source: &str) -> QueryExecContext {
    let node = parser::parse(source);
    let root = parser::ast::Root::cast(node).expect("root");
    let module = hir_lower::lower(root);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    QueryExecContext::compile(&module, &type_info)
}

fn build_linear_clutter_fixture(shape_count: usize) -> String {
    let mut out = String::new();
    out.push_str(
        r#"
material bench_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

"#,
    );
    let center = shape_count as f32 / 2.0;
    for idx in 0..shape_count {
        let x = (idx as f32 - center) * 2.0;
        out.push_str(&format!(
            "field exact distance bench_field_{idx}(p: Vec3) -> F32 {{\n    translate = vec3({x:.3}, 0.0, 0.0) {{\n        sphere(radius = 0.5)\n    }}\n}}\n\nshape bench_shape_{idx} {{\n    field = bench_field_{idx}\n    material = bench_surface\n}}\n\n"
        ));
    }
    out.push_str("region bench_region() {\n");
    for idx in 0..shape_count {
        out.push_str(&format!("    place shape_{idx} = bench_shape_{idx}\n"));
    }
    out.push_str("}\n");
    out
}

fn scene_domain(
    scene_id: u32,
    detail: i32,
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
                    fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(detail))],
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
