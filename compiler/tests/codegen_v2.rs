use std::fs;
use std::process::Command;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::hir::project::load_project;
use wrela::mir;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().expect("src parent")).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn expected_int_exit(value: i64) -> i32 {
    (value as i32) & 0xFF
}

fn compile_and_run_native_source(source: &str, executable_name: &str) -> std::process::Output {
    let module = load_module_from_source(source);
    compile_and_run_native_module(module, executable_name)
}

fn compile_and_run_native_inline_source(
    source: &str,
    executable_name: &str,
) -> std::process::Output {
    let module = lower_inline_module_from_source(source);
    compile_and_run_native_module(module, executable_name)
}

fn compile_and_run_native_module(
    module: hir::Module,
    executable_name: &str,
) -> std::process::Output {
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join(executable_name);
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");
    Command::new(&out).output().expect("run failed")
}

#[test]
fn native_v2_numeric_range_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn run() -> Integer {
    mutable total = 0
    for i in 1...3 {
        total += i
    }
    if "a" + "b" == "ab" {
        total += 100
    }
    return total
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_numeric_range_smoke");
    let expected = expected_int_exit(106);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_result_fallback_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn try_read(flag: Boolean) -> Result[Integer] {
    if flag {
        return 9
    }
    return error "bad"
}

fn run() -> Integer {
    return try_read(flag=false) ?? 5
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_result_fallback_smoke");
    let expected = expected_int_exit(5);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_class_method_dispatch_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
class Counter {
    value: Integer

    fn get_value() -> Integer {
        return self.value
    }
}

fn run() -> Integer {
    counter = Counter(value=7)
    return counter.get_value()
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_class_method_dispatch_smoke");
    let expected = expected_int_exit(7);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_bytes_len_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use {
    get_bytes_from_string,
    get_length
}
from data/bytes

fn run() -> Integer {
    return get_length(get_bytes_from_string("hello"))
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_bytes_len_smoke");
    let expected = expected_int_exit(5);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_substrate_scalars_array_and_approx_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn middle(values: Array[I32, 3]) -> I32 {
    assert approx 1.0 ~= 1.0009 within 0.001
    return values[1]
}

fn sum(values: Array[I32, 3]) -> I32 {
    return values[0] + middle(values=values)
}

fn main() -> Integer {
    return sum(values=[4, 6, 7])
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_substrate_smoke");
    let expected = expected_int_exit(10);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_vec2_surface_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    base = vec2(3.0, 4.0)
    unit = normalize(base)
    shifted = base + vec2(1.0, -1.0)
    restored = (shifted * 0.5) / 0.5
    assert approx base.x ~= 3.0 within 0.0001
    assert approx base.y ~= 4.0 within 0.0001
    assert approx length(base) ~= 5.0 within 0.0001
    assert approx dot(unit, vec2(0.6, 0.8)) ~= 1.0 within 0.0001
    assert approx restored.x ~= 4.0 within 0.0001
    assert approx restored.y ~= 3.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_vec2_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_vec3_dot_normalize_and_field_access_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    value = vec3(3.0, 0.0, 4.0)
    unit = normalize(value)
    projection = dot(unit, vec3(1.0, 0.0, 0.0))
    assert approx unit.x ~= 0.6 within 0.0001
    assert approx unit.y ~= 0.0 within 0.0001
    assert approx unit.z ~= 0.8 within 0.0001
    assert approx projection ~= 0.6 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_vec3_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_mat4_vec4_path_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    transform = mat4_cols(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(0.0, 0.0, 0.0, 1.0)
    )
    point = vec4(1.0, 2.0, 3.0, 1.0)
    result = transform * point
    assert approx result.x ~= 1.0 within 0.0001
    assert approx result.y ~= 2.0 within 0.0001
    assert approx result.z ~= 3.0 within 0.0001
    assert approx result.w ~= 1.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_mat4_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_vec_and_mat_arithmetic_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    left = vec3(1.0, 2.0, 3.0)
    right = vec3(4.0, 5.0, 6.0)
    sum = left + right
    delta = right - left
    scaled = sum * 0.5
    restored = scaled / 0.5
    assert approx restored.x ~= 5.0 within 0.0001
    assert approx restored.y ~= 7.0 within 0.0001
    assert approx restored.z ~= 9.0 within 0.0001
    assert approx delta.x ~= 3.0 within 0.0001
    assert approx delta.y ~= 3.0 within 0.0001
    assert approx delta.z ~= 3.0 within 0.0001

    transform = mat4_cols(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(4.0, 5.0, 6.0, 1.0)
    )
    adjusted = (transform + mat4_identity() - mat4_identity()) * 0.5 / 0.5
    result = adjusted * vec4(1.0, 2.0, 3.0, 1.0)
    assert approx result.x ~= 5.0 within 0.0001
    assert approx result.y ~= 7.0 within 0.0001
    assert approx result.z ~= 9.0 within 0.0001
    assert approx result.w ~= 1.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_vec_mat_arith_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_aggregate_surface_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    q = normalize(quat(0.0, 3.0, 4.0, 0.0))
    assert approx q.y ~= 0.6 within 0.0001
    assert approx q.z ~= 0.8 within 0.0001
    assert approx length(q) ~= 1.0 within 0.0001

    basis = mat3_cols(
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        vec3(0.0, 0.0, 1.0)
    )
    assert approx (mat3_identity() * vec3(4.0, 5.0, 6.0)).y ~= 5.0 within 0.0001
    projected = basis * vec3(1.0, 2.0, 3.0)
    assert approx projected.x ~= 1.0 within 0.0001
    assert approx projected.y ~= 2.0 within 0.0001
    assert approx projected.z ~= 3.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_portable_math_surface_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_virtual_gpu_compute_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
kernel fn run_kernel(snapshot: GpuBuffer[I32], counts: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    lid = local_invocation_id()
    wid = workgroup_id()
    num = num_workgroups()
    size = workgroup_size()

    if gid[0] == u32(0) and lid[0] == u32(0) and wid[0] == u32(0) {
        gpu_buffer_set(buffer=snapshot, index=0, value=i32(gid[0]))
        gpu_buffer_set(buffer=snapshot, index=1, value=i32(lid[0]))
        gpu_buffer_set(buffer=snapshot, index=2, value=i32(wid[0]))
        gpu_buffer_set(buffer=snapshot, index=3, value=i32(num[0]))
        gpu_buffer_set(buffer=snapshot, index=4, value=i32(size[0]))
        gpu_buffer_set(
            buffer=snapshot,
            index=5,
            value=i32(gpu_buffer_len(buffer=counts))
        )
    }

    gpu_buffer_set(buffer=counts, index=gid[0], value=i32(1))
}

fn main() -> Integer {
    snapshot = gpu_buffer_new(
        length=6,
        default_value=i32(0)
    )
    counts = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        snapshot=snapshot,
        counts=counts,
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )

    assert value gpu_buffer_get(buffer=snapshot, index=0) == 0
    assert value gpu_buffer_get(buffer=snapshot, index=1) == 0
    assert value gpu_buffer_get(buffer=snapshot, index=2) == 0
    assert value gpu_buffer_get(buffer=snapshot, index=3) == 2
    assert value gpu_buffer_get(buffer=snapshot, index=4) == 2
    assert value gpu_buffer_get(buffer=snapshot, index=5) == 4
    assert value gpu_buffer_get(buffer=counts, index=0) == 1
    assert value gpu_buffer_get(buffer=counts, index=1) == 1
    assert value gpu_buffer_get(buffer=counts, index=2) == 1
    assert value gpu_buffer_get(buffer=counts, index=3) == 1
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_virtual_gpu_compute_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_virtual_gpu_atomic_schedule_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn main() -> Integer {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )

    assert value gpu_atomic_i32_load(atomic=counter) == 4
    assert value gpu_atomic_i32_drop(atomic=counter) == true
    assert value gpu_buffer_get(buffer=observed, index=0) == 3
    assert value gpu_buffer_get(buffer=observed, index=1) == 2
    assert value gpu_buffer_get(buffer=observed, index=2) == 1
    assert value gpu_buffer_get(buffer=observed, index=3) == 0
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_virtual_gpu_atomic_schedule_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_virtual_gpu_workgroup_reverse_schedule_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn main() -> Integer {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_workgroup_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )

    assert value gpu_atomic_i32_load(atomic=counter) == 4
    assert value gpu_buffer_get(buffer=observed, index=0) == 2
    assert value gpu_buffer_get(buffer=observed, index=1) == 3
    assert value gpu_buffer_get(buffer=observed, index=2) == 0
    assert value gpu_buffer_get(buffer=observed, index=3) == 1
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_virtual_gpu_workgroup_reverse_schedule_smoke",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_virtual_gpu_round_robin_workgroups_schedule_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn main() -> Integer {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_round_robin_workgroups(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )

    assert value gpu_atomic_i32_load(atomic=counter) == 4
    assert value gpu_buffer_get(buffer=observed, index=0) == 0
    assert value gpu_buffer_get(buffer=observed, index=1) == 2
    assert value gpu_buffer_get(buffer=observed, index=2) == 1
    assert value gpu_buffer_get(buffer=observed, index=3) == 3
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_virtual_gpu_round_robin_workgroups_schedule_smoke",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_virtual_gpu_workgroup_shuffle_schedule_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn main() -> Integer {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=8,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_workgroup_shuffle(seed=u32(7)),
        workgroups_x=u32(4),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )

    assert value gpu_atomic_i32_load(atomic=counter) == 8
    assert value gpu_buffer_get(buffer=observed, index=0) == 4
    assert value gpu_buffer_get(buffer=observed, index=1) == 5
    assert value gpu_buffer_get(buffer=observed, index=2) == 0
    assert value gpu_buffer_get(buffer=observed, index=3) == 1
    assert value gpu_buffer_get(buffer=observed, index=4) == 6
    assert value gpu_buffer_get(buffer=observed, index=5) == 7
    assert value gpu_buffer_get(buffer=observed, index=6) == 2
    assert value gpu_buffer_get(buffer=observed, index=7) == 3
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_virtual_gpu_workgroup_shuffle_schedule_smoke",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_value_struct_roundtrip_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
value Pair {
    x: I32
    y: I32
}

fn swap(pair: Pair) -> Pair {
    return Pair(x=pair.y, y=pair.x)
}

fn sum(pair: Pair) -> I32 {
    return pair.x + pair.y
}

fn main() -> Integer {
    original = Pair(x=4, y=6)
    swapped = swap(pair=original)
    assert value swapped.x == 6
    assert value swapped.y == 4
    return sum(pair=swapped)
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_portable_value_struct_roundtrip");
    let expected = expected_int_exit(10);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_builtin_record_transport_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
value SceneProbe {
    surface: Surface
    medium: Medium
    hit: Hit3
    contact: Contact
    light: Light
    support: Support3
    camera: Camera
    pose: Transform3
}

fn main() -> Integer {
    handle = ActorHandle(id=u64(7), generation=u32(3))
    payload = Payload(entity_id=u64(7), material_id=u64(11), actor=handle)
    hit = Hit3(
        hit=true,
        distance=f32(4.0),
        position=vec3(1.0, 2.0, 3.0),
        normal=vec3(0.0, 1.0, 0.0),
        steps=0,
        feature_id=u64(0),
        payload=payload
    )
    surface = Surface(
        albedo=vec3(0.25, 0.5, 0.75),
        roughness=f32(0.125),
        metalness=f32(0.25),
        clearcoat=f32(0.5),
        clearcoat_roughness=f32(0.75),
        sheen=f32(0.1),
        emissive=vec3(1.0, 0.0, 2.0)
    )
    medium = Medium(
        density=f32(0.5),
        emission=vec3(0.5, 0.25, 0.75),
        anisotropy=f32(-0.25)
    )
    bounds = Bounds3(
        min=vec3(0.0, 1.0, 2.0),
        max=vec3(6.0, 7.0, 8.0)
    )
    support = Support3(bounds=bounds)
    contact = Contact(
        hit=true,
        position=hit.position,
        normal=hit.normal,
        penetration=f32(0.5),
        payload=payload
    )
    light = Light(
        position=vec3(2.0, 4.0, 6.0),
        direction=vec3(0.0, -1.0, 0.0),
        intensity=vec3(8.0, 6.0, 4.0),
        range=f32(12.0)
    )
    camera = Camera(
        position=vec3(0.0, 1.0, 2.0),
        forward=vec3(0.0, 0.0, -1.0),
        up=vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees=f32(60.0)
    )
    pose = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    )
    scene = SceneProbe(
        surface=surface,
        medium=medium,
        hit=hit,
        contact=contact,
        light=light,
        support=support,
        camera=camera,
        pose=pose
    )
    assert value scene.hit.payload.actor.id == 7
    assert value scene.hit.payload.actor.generation == 3
    assert approx scene.surface.roughness ~= 0.125 within 0.0001
    assert approx scene.medium.anisotropy ~= -0.25 within 0.0001
    assert value scene.support.bounds == bounds
    assert value scene.pose == pose
    assert value scene.contact.payload.material_id == 11
    assert approx scene.light.range ~= 12.0 within 0.0001
    assert approx scene.camera.vertical_fov_degrees ~= 60.0 within 0.0001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_portable_builtin_record_transport");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_bounds_and_transform_helpers_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    bounds2_box = Bounds2(
        min=vec2(1.0, 2.0),
        max=vec2(5.0, 6.0)
    )
    bounds3_box = Bounds3(
        min=vec3(0.0, 1.0, 2.0),
        max=vec3(6.0, 7.0, 8.0)
    )
    ray = Ray3(
        origin=vec3(1.0, 2.0, 3.0),
        direction=normalize(vec3(0.0, 1.0, 0.0))
    )
    pose = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    )

    center2 = bounds2_center(bounds=bounds2_box)
    size2 = bounds2_size(bounds=bounds2_box)
    center3 = bounds3_center(bounds=bounds3_box)
    size3 = bounds3_size(bounds=bounds3_box)
    identity = transform3_identity()
    composed = compose_transform3(
        left=pose,
        right=inverse_transform3(transform=identity)
    )
    point = transform_point(transform=composed, point=ray.origin)
    vector = transform_vector(transform=composed, vector=ray.direction)
    normal = transform_normal(transform=composed, normal=vec3(0.0, 1.0, 0.0))

    assert approx center2.x ~= 3.0 within 0.0001
    assert approx center2.y ~= 4.0 within 0.0001
    assert approx size2.x ~= 4.0 within 0.0001
    assert approx size2.y ~= 4.0 within 0.0001
    assert approx center3.x ~= 3.0 within 0.0001
    assert approx center3.y ~= 4.0 within 0.0001
    assert approx center3.z ~= 5.0 within 0.0001
    assert approx size3.x ~= 6.0 within 0.0001
    assert approx size3.y ~= 6.0 within 0.0001
    assert approx size3.z ~= 6.0 within 0.0001
    assert approx point.x ~= 1.0 within 0.0001
    assert approx point.y ~= 2.0 within 0.0001
    assert approx point.z ~= 3.0 within 0.0001
    assert approx vector.y ~= 1.0 within 0.0001
    assert approx normal.y ~= 1.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_bounds_and_transform_helpers");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_structural_field_wrappers_affect_sampling() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance translated_sphere(p: Vec3) -> F32 {
    transform = vec3(2.0, 0.0, 0.0) {
        sphere(radius=1.0)
    }
}

field conservative distance mirrored_box(p: Vec3) -> F32 {
    mirror = vec3(1.0, 0.0, 0.0) {
        transform = vec3(1.0, 0.0, 0.0) {
            box(half=vec3(0.5, 0.5, 0.5))
        }
    }
}

field exact distance repeated_sphere(p: Vec3) -> F32 {
    repeat = vec3(2.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}

field conservative distance instanced_sphere(p: Vec3) -> F32 {
    instance = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, 1.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, -1.0, 1.0)
        )
    ) {
        sphere(radius=0.5)
    }
}

fn main() -> Integer {
    translated_scene = capture translated_sphere
    mirrored_scene = capture mirrored_box
    repeated_scene = capture repeated_sphere
    instanced_scene = capture instanced_sphere
    translated_sample = distance_at(capture=translated_scene, point=vec3(2.0, 0.0, 0.0))
    mirrored_sample = distance_at(capture=mirrored_scene, point=vec3(-1.0, 0.0, 0.0))
    repeated_sample = distance_at(capture=repeated_scene, point=vec3(2.0, 0.0, 0.0))
    instanced_sample = distance_at(capture=instanced_scene, point=vec3(0.0, 0.0, 1.0))

    assert approx translated_sample ~= -1.0 within 0.001
    assert approx mirrored_sample ~= -0.5 within 0.001
    assert approx repeated_sample ~= -0.5 within 0.001
    assert approx instanced_sample ~= -0.5 within 0.001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_structural_field_wrappers_affect_sampling",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_field_query_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance shifted_sphere(p: Vec3) -> F32 {
    return sphere(p=p - vec3(0.0, 0.0, 1.0), radius=1.0)
}

fn main() -> Integer {
    sphere_scene = capture sphere_field
    shifted_scene = capture shifted_sphere
    sampled_distance = distance_at(capture=sphere_scene, point=vec3(0.0, 0.0, 2.0))
    sampled_normal = normal_at(capture=sphere_scene, point=vec3(0.0, 0.0, 2.0))
    shifted_distance = distance_at(capture=shifted_scene, point=vec3(0.0, 0.0, 3.0))

    assert approx sampled_distance ~= 1.0 within 0.001
    assert approx sampled_normal.x ~= 0.0 within 0.01
    assert approx sampled_normal.y ~= 0.0 within 0.01
    assert approx sampled_normal.z ~= 1.0 within 0.01
    assert approx shifted_distance ~= 1.0 within 0.001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_field_query_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_field_primitive_catalog_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance box_field(p: Vec3) -> F32 {
    box(half = vec3(1.0, 2.0, 3.0))
}

field exact distance capsule_field(p: Vec3) -> F32 {
    capsule(a = vec3(0.0, -1.0, 0.0), b = vec3(0.0, 1.0, 0.0), radius = 0.5)
}

field exact distance cylinder_field(p: Vec3) -> F32 {
    cylinder(radius = 0.5, half_height = 1.0)
}

field exact distance plane_field(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.25)
}

field exact distance torus_field(p: Vec3) -> F32 {
    torus(major_radius = 2.0, minor_radius = 0.5)
}

fn main() -> Integer {
    sphere_scene = capture sphere_field
    box_scene = capture box_field
    capsule_scene = capture capsule_field
    cylinder_scene = capture cylinder_field
    plane_scene = capture plane_field
    torus_scene = capture torus_field
    sphere_sample = distance_at(capture=sphere_scene, point=vec3(0.0, 0.0, 0.0))
    box_sample = distance_at(capture=box_scene, point=vec3(0.0, 0.0, 0.0))
    capsule_sample = distance_at(capture=capsule_scene, point=vec3(0.0, 0.0, 0.0))
    cylinder_sample = distance_at(capture=cylinder_scene, point=vec3(0.0, 0.0, 0.0))
    plane_sample = distance_at(capture=plane_scene, point=vec3(0.0, 0.0, 0.0))
    torus_sample = distance_at(capture=torus_scene, point=vec3(2.0, 0.0, 0.0))

    assert approx sphere_sample ~= -1.0 within 0.001
    assert approx box_sample ~= -1.0 within 0.001
    assert approx capsule_sample ~= -0.5 within 0.001
    assert approx cylinder_sample ~= -0.5 within 0.001
    assert approx plane_sample ~= 0.25 within 0.001
    assert approx torus_sample ~= -0.5 within 0.001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_field_primitive_catalog_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_host_raymarch_material_ppm_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape sphere_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

fn render_ppm() -> String {
    sphere_scene = capture sphere_shape
    camera = Camera(
        position=vec3(0.0, 0.0, 3.0),
        forward=vec3(0.0, 0.0, -1.0),
        up=vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees=60.0
    )
    light = Light(
        position=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        intensity=vec3(255.0, 255.0, 255.0),
        range=10.0
    )

    hit = trace_shape(
        capture=sphere_scene,
        origin=camera.position,
        direction=camera.forward,
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=sphere_scene, hit=hit)
    to_light = normalize(light.position - hit.position)
    ndotl = max(dot(hit.normal, to_light), 0.0)

    r = i32(clamp(surface.albedo.x * ndotl * light.intensity.x + surface.emissive.x, 0.0, 255.0))
    g = i32(clamp(surface.albedo.y * ndotl * light.intensity.y + surface.emissive.y, 0.0, 255.0))
    b = i32(clamp(surface.albedo.z * ndotl * light.intensity.z + surface.emissive.z, 0.0, 255.0))

    mutable ppm = "P3\n1 1\n255\n"
    ppm += "{r} {g} {b}"
    return ppm
}

fn main() -> Integer {
    __wr_print(render_ppm())
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_host_raymarch_material_ppm_smoke");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "P3\n1 1\n255\n255 0 0",
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = expected_int_exit(0);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_repeat_wrapper_does_not_regress_trace_steps() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance repeat_box(p: Vec3) -> F32 {
    repeat = vec3(2.0, 0.0, 0.0) {
        box(half=vec3(0.5, 0.5, 0.5))
    }
}

field conservative distance manual_box(p: Vec3) -> F32 {
    repeated = vec3(
        p.x - 2.0 * floor(p.x / 2.0 + 0.5),
        p.y,
        p.z
    )
    repeated_abs = vec3(abs(repeated.x), abs(repeated.y), abs(repeated.z))
    q = repeated_abs - vec3(0.5, 0.5, 0.5)
    outside = length(max(q, vec3(0.0, 0.0, 0.0)))
    inside = min(max(q.x, max(q.y, q.z)), 0.0)
    return outside + inside
}

fn main() -> Integer {
    sample_before = __wr_metrics_get(__wr_metrics_field_sample_id())
    repeat_scene = capture repeat_box
    manual_scene = capture manual_box
    mutable repeat_traveled = 0.0
    mutable repeat_steps = 0
    mutable repeat_hit = Hit3(
        hit=false,
        distance=0.0,
        position=vec3(0.0, 0.0, 3.0),
        normal=vec3(0.0, 0.0, 1.0),
        steps=0,
        feature_id=i64(0),
        payload=Payload(
            entity_id=u64(0),
            material_id=u64(0),
            actor=ActorHandle(id=u64(0), generation=u32(0))
        )
    )
    while repeat_steps < 96 and repeat_traveled <= 6.0 {
        rp = vec3(0.0, 0.0, 3.0) + vec3(0.0, 0.0, -1.0) * repeat_traveled
        rd = distance_at(capture=repeat_scene, point=rp)
        if abs(rd) <= 0.05 {
            repeat_hit = Hit3(
                hit=true,
                distance=repeat_traveled,
                position=rp,
                normal=vec3(0.0, 0.0, 1.0),
                steps=i64(repeat_steps + 1),
                feature_id=i64(0),
                payload=Payload(
                    entity_id=u64(0),
                    material_id=u64(0),
                    actor=ActorHandle(id=u64(0), generation=u32(0))
                )
            )
            break
        }
        rs = max(rd * 0.75, 0.05)
        repeat_traveled += rs
        repeat_steps += 1
    }
    manual_samples_before = __wr_metrics_get(__wr_metrics_field_sample_id())
    repeat_hit_samples = manual_samples_before - sample_before
    mutable manual_traveled = 0.0
    mutable manual_steps = 0
    mutable manual_hit = Hit3(
        hit=false,
        distance=0.0,
        position=vec3(0.0, 0.0, 3.0),
        normal=vec3(0.0, 0.0, 1.0),
        steps=0,
        feature_id=i64(0),
        payload=Payload(
            entity_id=u64(0),
            material_id=u64(0),
            actor=ActorHandle(id=u64(0), generation=u32(0))
        )
    )
    while manual_steps < 96 and manual_traveled <= 6.0 {
        mp = vec3(0.0, 0.0, 3.0) + vec3(0.0, 0.0, -1.0) * manual_traveled
        md = distance_at(capture=manual_scene, point=mp)
        if abs(md) <= 0.05 {
            manual_hit = Hit3(
                hit=true,
                distance=manual_traveled,
                position=mp,
                normal=vec3(0.0, 0.0, 1.0),
                steps=i64(manual_steps + 1),
                feature_id=i64(0),
                payload=Payload(
                    entity_id=u64(0),
                    material_id=u64(0),
                    actor=ActorHandle(id=u64(0), generation=u32(0))
                )
            )
            break
        }
        ms = max(md * 0.75, 0.05)
        manual_traveled += ms
        manual_steps += 1
    }
    manual_sample_delta = __wr_metrics_get(__wr_metrics_field_sample_id()) - manual_samples_before
    manual_hit_samples = manual_sample_delta
    assert value manual_samples_before - sample_before > 0
    assert value manual_sample_delta > 0
    assert value repeat_hit.hit == true
    assert value manual_hit.hit == true
    assert value repeat_hit.steps > 0
    assert value manual_hit.steps > 0
    assert value repeat_hit.steps <= manual_hit.steps
    assert value repeat_hit_samples <= manual_hit_samples
    assert value repeat_hit_samples <= manual_sample_delta
    __wr_print("ok")
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_repeat_wrapper_does_not_regress_trace_steps",
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "ok",
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = expected_int_exit(0);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_authored_support_pruning_preserves_hit_identity() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance near_orb(p: Vec3) -> F32 {
    sphere(radius=0.65)
}

field conservative distance far_supported(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    )
    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_plain(p: Vec3) -> F32 {
    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_orb
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

shape far_supported_shape {
    field = far_supported
    material = shade
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(2),
        actor=ActorHandle(id=u64(2), generation=u32(0))
    )
}

shape far_plain_shape {
    field = far_plain
    material = shade
    payload = Payload(
        entity_id=u64(3),
        material_id=u64(3),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_supported_shape
    }
}

shape plain_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_plain_shape
    }
}

fn main() -> Integer {
    supported_pruned_before = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    supported_scene_capture = capture supported_scene
    supported_hit = trace_shape(
        capture=supported_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    supported_surface = surface_at(capture=supported_scene_capture, hit=supported_hit)
    supported_pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())

    plain_pruned_before = supported_pruned_after
    plain_scene_capture = capture plain_scene
    plain_hit = trace_shape(
        capture=plain_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    plain_surface = surface_at(capture=plain_scene_capture, hit=plain_hit)
    plain_pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())

    if supported_hit.hit != true { return 1 }
    if plain_hit.hit != true { return 2 }
    if supported_hit.feature_id != plain_hit.feature_id { return 3 }
    if abs(supported_hit.distance - plain_hit.distance) > 0.0001 { return 4 }
    if supported_surface.albedo.x != plain_surface.albedo.x { return 5 }
    if supported_pruned_after - supported_pruned_before <= 0 { return 6 }
    if plain_pruned_after - plain_pruned_before != 0 { return 7 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_authored_support_pruning_preserves_hit_identity",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_exact_trace_metrics_track_hits() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance orb(p: Vec3) -> F32 {
    transform = vec3(0.8, 0.0, 0.0) {
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
        entity_id=u64(2),
        material_id=u64(22),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

fn main() -> Integer {
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
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
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
    assert value hit.payload.material_id == u64(22)
    assert approx surface.albedo.x ~= 32.0 within 0.001
    assert approx surface.albedo.z ~= 255.0 within 0.001
    assert value exact_after - exact_before == 1
    assert value conservative_after - conservative_before == 0
    assert value hit_count_after - hit_count_before == 1
    assert value hit_steps_after - hit_steps_before == hit.steps
    assert value hit_samples_after - hit_samples_before == field_samples_after_trace - field_samples_before
    assert value bucket_after - bucket_before == 1

    __wr_print("ok")
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_exact_trace_metrics_track_hits");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "ok",
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = expected_int_exit(0);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_authored_support_prunes_custom_field_branches() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance near_orb(p: Vec3) -> F32 {
    sphere(radius=0.65)
}

field conservative distance far_custom_supported(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(7.5, -1.0, -1.0),
        max=vec3(8.5, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(7.5, -1.0, -1.0),
        max=vec3(8.5, 1.0, 1.0)
    )
    return length(p - vec3(8.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_custom_plain(p: Vec3) -> F32 {
    return length(p - vec3(8.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.5, 0.5, 0.5),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_orb
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

shape far_supported_shape {
    field = far_custom_supported
    material = shade
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(2),
        actor=ActorHandle(id=u64(2), generation=u32(0))
    )
}

shape far_plain_shape {
    field = far_custom_plain
    material = shade
    payload = Payload(
        entity_id=u64(3),
        material_id=u64(3),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_supported_shape
    }
}

shape plain_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_plain_shape
    }
}

fn main() -> Integer {
    pruned_before = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    scene_capture = capture supported_scene
    supported_hit = trace_shape(
        capture=scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    pruned_mid = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    plain_capture = capture plain_scene
    plain_hit = trace_shape(
        capture=plain_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    return (pruned_mid - pruned_before) - (pruned_after - pruned_mid)
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_authored_support_prunes_custom_field_branches",
    );
    assert!(
        output.status.code().unwrap_or(-1) > 0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_authored_support_bounds_enable_support_pruning() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    fn scene_source(include_authored_support: bool) -> String {
        let support_clause = if include_authored_support {
            r#"    support = Support3(bounds=Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    )
"#
        } else {
            ""
        };

        format!(
            r#"
field conservative distance near_field(p: Vec3) -> F32 {{
    return sphere(p=p, radius=0.5)
}}

field conservative distance far_field(p: Vec3) -> F32 {{
{support_clause}    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}}

material shade(hit: Hit3) -> Surface {{
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}}

shape near_shape {{
    field = near_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}}

shape far_shape {{
    field = far_field
    material = shade
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(2),
        actor=ActorHandle(id=u64(2), generation=u32(0))
    )
}}

shape scene_shape {{
    union {{
        provenance_policy = nearest
        use near_shape
        use far_shape
    }}
}}

fn main() -> Integer {{
    pruned_before = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    scene_capture = capture scene_shape

    hit = trace_shape(
        capture=scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )

    pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    return pruned_after - pruned_before
}}
"#,
            support_clause = support_clause,
        )
    }

    let authored_output = compile_and_run_native_inline_source(
        &scene_source(true),
        "wr_v2_authored_support_bounds_enable_support_pruning",
    );
    let authored_delta = authored_output.status.code().unwrap_or(-1);

    let control_output = compile_and_run_native_inline_source(
        &scene_source(false),
        "wr_v2_authored_support_bounds_disable_support_pruning",
    );
    let control_delta = control_output.status.code().unwrap_or(-1);

    assert_eq!(
        control_delta,
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&control_output.stdout),
        String::from_utf8_lossy(&control_output.stderr)
    );
    assert!(
        authored_delta > control_delta,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&authored_output.stdout),
        String::from_utf8_lossy(&authored_output.stderr)
    );
}

#[test]
fn native_v2_shape_provenance_tracks_boolean_winners() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance left_orb(p: Vec3) -> F32 {
    transform = vec3(-0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

field exact distance right_orb(p: Vec3) -> F32 {
    transform = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

field exact distance body(p: Vec3) -> F32 {
    sphere(radius=0.8)
}

field exact distance cutter(p: Vec3) -> F32 {
    sphere(radius=0.5)
}

field exact distance near_orb(p: Vec3) -> F32 {
    transform = vec3(0.0, 0.0, 1.6) {
        sphere(radius=0.45)
    }
}

field exact distance far_orb(p: Vec3) -> F32 {
    transform = vec3(0.0, 0.0, 0.9) {
        sphere(radius=0.45)
    }
}

material left_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(220.0, 60.0, 60.0),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material right_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(60.0, 120.0, 220.0),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material body_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(220.0, 180.0, 120.0),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material cutter_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(100.0, 200.0, 120.0),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape left_shape {
    field = left_orb
    material = left_surface
    payload = Payload(
        entity_id=u64(31),
        material_id=u64(31),
        actor=ActorHandle(id=u64(31), generation=u32(0))
    )
}

shape right_shape {
    field = right_orb
    material = right_surface
    payload = Payload(
        entity_id=u64(32),
        material_id=u64(32),
        actor=ActorHandle(id=u64(32), generation=u32(0))
    )
}

shape scene_lr {
    union {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape scene_rl {
    union {
        provenance_policy = nearest
        use right_shape
        use left_shape
    }
}

shape body_shape {
    field = body
    material = body_surface
    payload = Payload(
        entity_id=u64(41),
        material_id=u64(41),
        actor=ActorHandle(id=u64(41), generation=u32(0))
    )
}

shape cutter_shape {
    field = cutter
    material = cutter_surface
    payload = Payload(
        entity_id=u64(42),
        material_id=u64(42),
        actor=ActorHandle(id=u64(42), generation=u32(0))
    )
}

shape near_shape {
    field = near_orb
    material = left_surface
    payload = Payload(
        entity_id=u64(61),
        material_id=u64(61),
        actor=ActorHandle(id=u64(61), generation=u32(0))
    )
}

shape far_shape {
    field = far_orb
    material = right_surface
    payload = Payload(
        entity_id=u64(62),
        material_id=u64(62),
        actor=ActorHandle(id=u64(62), generation=u32(0))
    )
}

shape nearest_scene {
    union {
        provenance_policy = nearest
        use far_shape
        use near_shape
    }
}

shape ordered_scene {
    union {
        provenance_policy = ordered
        use far_shape
        use near_shape
    }
}

shape overlap_scene {
    intersection {
        provenance_policy = nearest
        use near_shape
        use far_shape
    }
}

shape carved_shape {
    subtract {
        provenance_policy = right
        use body_shape
        use cutter_shape
    }
}

shape carved_left_shape {
    subtract {
        provenance_policy = left
        use body_shape
        use cutter_shape
    }
}

fn main() -> Integer {
    left_scene = capture left_shape
    right_scene = capture right_shape
    body_scene = capture body_shape
    cutter_scene = capture cutter_shape
    near_scene = capture near_shape
    far_scene = capture far_shape
    nearest_scene_capture = capture nearest_scene
    ordered_scene_capture = capture ordered_scene
    overlap_scene_capture = capture overlap_scene
    carved_scene_capture = capture carved_shape
    carved_left_scene_capture = capture carved_left_shape
    left_lr = trace_shape(
        capture=left_scene,
        origin=vec3(-0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    right_lr = trace_shape(
        capture=right_scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    left_rl = trace_shape(
        capture=left_scene,
        origin=vec3(-0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    right_rl = trace_shape(
        capture=right_scene,
        origin=vec3(0.8, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    carve_hit = trace_shape(
        capture=carved_scene_capture,
        origin=vec3(0.0, 0.0, 0.0),
        direction=vec3(1.0, 0.0, 0.0),
        max_distance=4.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    near_direct = trace_shape(
        capture=near_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    far_direct = trace_shape(
        capture=far_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    nearest_hit = trace_shape(
        capture=nearest_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    ordered_hit = trace_shape(
        capture=ordered_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    overlap_hit = trace_shape(
        capture=overlap_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    cutter_hit = trace_shape(
        capture=cutter_scene,
        origin=vec3(0.0, 0.0, 0.0),
        direction=vec3(1.0, 0.0, 0.0),
        max_distance=4.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    body_hit = trace_shape(
        capture=body_scene,
        origin=vec3(0.0, 0.0, 0.0),
        direction=vec3(1.0, 0.0, 0.0),
        max_distance=4.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    carve_left_hit = trace_shape(
        capture=carved_left_scene_capture,
        origin=vec3(0.0, 0.0, 0.0),
        direction=vec3(1.0, 0.0, 0.0),
        max_distance=4.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    left_hit_surface = surface_at(capture=left_scene, hit=left_lr)
    right_hit_surface = surface_at(capture=right_scene, hit=right_lr)
    carve_hit_surface = surface_at(capture=carved_scene_capture, hit=carve_hit)
    overlap_hit_surface = surface_at(capture=overlap_scene_capture, hit=overlap_hit)
    carve_left_hit_surface = surface_at(capture=carved_left_scene_capture, hit=carve_left_hit)

    assert value left_lr.hit == true
    assert value right_lr.hit == true
    assert value left_rl.hit == true
    assert value right_rl.hit == true
    assert value left_lr.payload.entity_id == u64(31)
    assert value right_lr.payload.entity_id == u64(32)
    assert value left_rl.payload.entity_id == u64(31)
    assert value right_rl.payload.entity_id == u64(32)
    assert value carve_hit.hit == true
    assert value carve_hit.payload.entity_id == u64(42)
    assert value near_direct.feature_id != u64(0)
    assert value far_direct.feature_id != u64(0)
    assert value nearest_hit.hit == true
    assert value ordered_hit.hit == true
    assert value overlap_hit.hit == true
    assert value nearest_hit.payload.entity_id == u64(61)
    assert value ordered_hit.payload.entity_id == u64(62)
    assert value overlap_hit.payload.entity_id == u64(62)
    assert value nearest_hit.feature_id == near_direct.feature_id
    assert value ordered_hit.feature_id == far_direct.feature_id
    assert value overlap_hit.feature_id == far_direct.feature_id
    assert value carve_hit.feature_id == cutter_hit.feature_id
    assert value carve_left_hit.hit == true
    assert value carve_left_hit.payload.entity_id == u64(41)
    assert value carve_left_hit.feature_id == body_hit.feature_id
    assert approx left_hit_surface.albedo.x ~= 220.0 within 0.001
    assert approx right_hit_surface.albedo.z ~= 220.0 within 0.001
    assert approx carve_hit_surface.albedo.x ~= 100.0 within 0.001
    assert approx overlap_hit_surface.albedo.z ~= 220.0 within 0.001
    assert approx carve_left_hit_surface.albedo.x ~= 220.0 within 0.001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_shape_provenance_tracks_boolean_winners",
    );
    assert_eq!(output.status.code().unwrap_or(-1), 0, "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr));
}

#[test]
fn native_v2_boolean_provenance_ties_and_wrapper_stack_are_stable() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance tie_left_field(p: Vec3) -> F32 {
    sphere(radius=0.65)
}

field exact distance tie_right_field(p: Vec3) -> F32 {
    sphere(radius=0.65)
}

field conservative distance wrapped_stack(p: Vec3) -> F32 {
    mirror = vec3(1.0, 0.0, 0.0) {
        repeat = vec3(2.0, 0.0, 0.0) {
            transform = vec3(0.0, 0.0, 0.0) {
                instance = Transform3(
                    matrix=mat4_cols(
                        vec4(1.0, 0.0, 0.0, 0.0),
                        vec4(0.0, 1.0, 0.0, 0.0),
                        vec4(0.0, 0.0, 1.0, 0.0),
                        vec4(0.0, 0.0, 0.0, 1.0)
                    ),
                    inverse=mat4_cols(
                        vec4(1.0, 0.0, 0.0, 0.0),
                        vec4(0.0, 1.0, 0.0, 0.0),
                        vec4(0.0, 0.0, 1.0, 0.0),
                        vec4(0.0, 0.0, 0.0, 1.0)
                    )
                ) {
                    sphere(radius=0.5)
                }
            }
        }
    }
}

field conservative distance wrapped_decoy(p: Vec3) -> F32 {
    transform = vec3(4.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}

material left_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(220.0, 60.0, 60.0),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material right_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(60.0, 120.0, 220.0),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material wrapped_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(180.0, 220.0, 120.0),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape tie_left {
    field = tie_left_field
    material = left_surface
    payload = Payload(
        entity_id=u64(101),
        material_id=u64(101),
        actor=ActorHandle(id=u64(101), generation=u32(0))
    )
}

shape tie_right {
    field = tie_right_field
    material = right_surface
    payload = Payload(
        entity_id=u64(202),
        material_id=u64(202),
        actor=ActorHandle(id=u64(202), generation=u32(0))
    )
}

shape tie_union {
    union {
        provenance_policy = nearest
        use tie_left
        use tie_right
    }
}

shape tie_intersection {
    intersection {
        provenance_policy = nearest
        use tie_left
        use tie_right
    }
}

shape wrapped_shape {
    field = wrapped_stack
    material = wrapped_surface
    payload = Payload(
        entity_id=u64(303),
        material_id=u64(303),
        actor=ActorHandle(id=u64(303), generation=u32(0))
    )
}

shape wrapped_decoy_shape {
    field = wrapped_decoy
    material = right_surface
    payload = Payload(
        entity_id=u64(404),
        material_id=u64(404),
        actor=ActorHandle(id=u64(404), generation=u32(0))
    )
}

shape wrapped_scene {
    union {
        provenance_policy = nearest
        use wrapped_decoy_shape
        use wrapped_shape
    }
}

fn main() -> Integer {
    tie_left_scene = capture tie_left
    tie_right_scene = capture tie_right
    tie_union_scene = capture tie_union
    tie_intersection_scene = capture tie_intersection
    wrapped_scene_capture = capture wrapped_scene
    wrapped_shape_scene = capture wrapped_shape
    tie_left_hit = trace_shape(
        capture=tie_left_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    tie_right_hit = trace_shape(
        capture=tie_right_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    tie_union_hit = trace_shape(
        capture=tie_union_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    tie_intersection_hit = trace_shape(
        capture=tie_intersection_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    wrapped_direct_hit = trace_shape(
        capture=wrapped_shape_scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    wrapped_scene_hit = trace_shape(
        capture=wrapped_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    tie_union_surface = surface_at(capture=tie_union_scene, hit=tie_union_hit)
    tie_intersection_surface = surface_at(capture=tie_intersection_scene, hit=tie_intersection_hit)
    wrapped_scene_surface = surface_at(capture=wrapped_scene_capture, hit=wrapped_scene_hit)

    assert value tie_left_hit.hit == true
    assert value tie_right_hit.hit == true
    assert value tie_union_hit.hit == true
    assert value tie_intersection_hit.hit == true
    assert value tie_union_hit.payload.entity_id == u64(101)
    assert value tie_intersection_hit.payload.entity_id == u64(101)
    assert value tie_union_hit.feature_id == tie_left_hit.feature_id
    assert value tie_intersection_hit.feature_id == tie_left_hit.feature_id
    assert approx tie_union_surface.albedo.x ~= 220.0 within 0.001
    assert approx tie_intersection_surface.albedo.x ~= 220.0 within 0.001
    assert value wrapped_direct_hit.hit == true
    assert value wrapped_scene_hit.hit == true
    assert value wrapped_direct_hit.feature_id == wrapped_scene_hit.feature_id
    assert value wrapped_direct_hit.payload.entity_id == wrapped_scene_hit.payload.entity_id
    assert approx wrapped_scene_surface.albedo.z ~= 120.0 within 0.001
    __wr_print("ok")
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_boolean_provenance_ties_and_wrapper_stack_are_stable",
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "ok",
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = expected_int_exit(0);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_semantic_field_composition_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance left_x(p: Vec3) -> F32 {
    return p.x
}

field conservative distance left_y(p: Vec3) -> F32 {
    return p.y
}

field conservative distance cap_z(p: Vec3) -> F32 {
    return p.z
}

field conservative distance notch(p: Vec3) -> F32 {
    return p.x - 0.5
}

field conservative distance composed(p: Vec3) -> F32 {
    subtract {
        provenance_policy = right
        intersection {
            provenance_policy = nearest
            union {
                provenance_policy = nearest
                use left_x
                use left_y
            }
            use cap_z
        }
        use notch
    }
}

fn main() -> Integer {
    composed_scene = capture composed
    sample_a = distance_at(capture=composed_scene, point=vec3(1.0, 2.0, 3.0))
    sample_b = distance_at(capture=composed_scene, point=vec3(-1.0, 2.0, 0.5))
    assert approx sample_a ~= 3.0 within 0.001
    assert approx sample_b ~= 1.5 within 0.001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_semantic_field_composition_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_cast_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    assert approx f32(7) ~= 7.0 within 0.0001
    assert value i32(7.8) == 7
    assert value u32(7.8) == 7
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_portable_cast_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_minmax_clamp_mix_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    assert approx min(2.0, 3.0) ~= 2.0 within 0.0001
    assert approx max(2.0, 3.0) ~= 3.0 within 0.0001
    assert approx clamp(5.0, 0.0, 4.0) ~= 4.0 within 0.0001
    assert approx mix(10.0, 20.0, 0.25) ~= 12.5 within 0.0001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_portable_minmax_clamp_mix_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_unary_intrinsics_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    assert approx abs(-3.5) ~= 3.5 within 0.0001
    assert approx sign(-3.5) ~= -1.0 within 0.0001
    assert approx floor(1.8) ~= 1.0 within 0.0001
    assert approx ceil(1.2) ~= 2.0 within 0.0001
    assert approx fract(1.25) ~= 0.25 within 0.0001
    assert approx sin(0.0) ~= 0.0 within 0.0001
    assert approx cos(0.0) ~= 1.0 within 0.0001
    assert approx sqrt(9.0) ~= 3.0 within 0.0001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_portable_unary_intrinsics_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_pow_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    assert approx pow(2.0, 3.0) ~= 8.0 within 0.0001
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(source, "wr_v2_portable_pow_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_portable_vector_intrinsics_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn main() -> Integer {
    assert approx distance(vec3(0.0, 0.0, 0.0), vec3(0.0, 3.0, 4.0)) ~= 5.0 within 0.0001
    reflected = reflect(vec3(1.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0))
    assert approx reflected.y ~= 1.0 within 0.0001
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_portable_vector_intrinsics_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn native_v2_query_batch_records_and_dispatch_parity_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape sphere_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u64(7),
        material_id=u64(9),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

fn main() -> Integer {
    scene = capture sphere_shape
    scene_again = capture sphere_shape
    points = [
        PointQuery(point=vec3(0.0, 0.0, 2.0)),
        PointQuery(point=vec3(0.0, 0.0, 3.0)),
        PointQuery(point=vec3(0.5, 0.0, 2.0))
    ]
    rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        RayQuery(
            origin=vec3(0.5, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    shadow_rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 1.0, 0.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    cpu_distances = distance_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_cpu()
    )
    vgpu_distances = distance_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_virtual_gpu()
    )
    cpu_normals = normal_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_cpu()
    )
    vgpu_normals = normal_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_virtual_gpu()
    )
    cpu_hits = trace_shape_batch(
        capture=scene,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    vgpu_hits = trace_shape_batch(
        capture=scene,
        rays=rays,
        backend=dispatch_backend_virtual_gpu()
    )
    auto_surfaces = surface_at_batch(
        capture=scene,
        hits=cpu_hits,
        backend=dispatch_backend_auto()
    )
    vgpu_surfaces = surface_at_batch(
        capture=scene,
        hits=vgpu_hits,
        backend=dispatch_backend_virtual_gpu()
    )
    cpu_occlusion = occluded_batch(
        capture=scene,
        rays=shadow_rays,
        backend=dispatch_backend_cpu()
    )
    vgpu_occlusion = occluded_batch(
        capture=scene,
        rays=shadow_rays,
        backend=dispatch_backend_virtual_gpu()
    )

    if scene.scene_id != scene_again.scene_id {
        return 1
    }
    if scene.root_feature_id != scene_again.root_feature_id {
        return 2
    }
    if abs(cpu_distances[0].distance - 1.0) > 0.001 {
        return 3
    }
    if abs(cpu_distances[1].distance - 2.0) > 0.001 {
        return 4
    }
    if abs(cpu_distances[2].distance - vgpu_distances[2].distance) > 0.001 {
        return 5
    }
    if abs(cpu_normals[0].normal.z - 1.0) > 0.01 {
        return 6
    }
    if abs(vgpu_normals[0].normal.z - cpu_normals[0].normal.z) > 0.01 {
        return 7
    }
    if not cpu_hits[0].hit or not cpu_hits[1].hit {
        return 8
    }
    if vgpu_hits[0].feature_id != cpu_hits[0].feature_id {
        return 9
    }
    if abs(auto_surfaces[0].albedo.x - 1.0) > 0.001 {
        return 10
    }
    if abs(vgpu_surfaces[0].albedo.x - auto_surfaces[0].albedo.x) > 0.001 {
        return 11
    }
    if not cpu_occlusion[0].occluded or cpu_occlusion[1].occluded {
        return 12
    }
    if vgpu_occlusion[0].occluded != cpu_occlusion[0].occluded {
        return 13
    }
    if vgpu_occlusion[1].occluded != cpu_occlusion[1].occluded {
        return 14
    }
    return 0
}
"#;

    let output =
        compile_and_run_native_inline_source(source, "wr_v2_query_batch_records_and_parity_smoke");
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_shape_queries_require_shape_capture_smoke() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

fn main() -> Integer {
    scene = capture sphere_field
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    if hit.hit {
        return 1
    }
    return 0
}
"#;
    let (errors, _info) = hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
    assert!(
        errors.iter().any(|err| matches!(
            err,
            hir::typeck::TypeError::ShapeQueryTargetMustBeShape { query, .. }
                if query.as_str() == "trace_shape"
        )),
        "expected stored field capture shape query rejection, got: {errors:?}"
    );
}

#[test]
fn native_v2_scene_queries_require_capture_created_by_builtin() {
    let source = r#"
fn main() -> Integer {
    forged = FieldCapture(
        scene_id=u64(999),
        epoch=u64(0),
        root_feature_id=u64(0)
    )
    _ = distance_at(capture=forged, point=vec3(0.0, 0.0, 0.0))
    return 0
}
"#;
    let (errors, _info) = hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
    assert!(
        errors.iter().any(|err| matches!(
            err,
            hir::typeck::TypeError::OpaqueBuiltinConstructionForbidden { name, .. }
                if name.as_str() == "FieldCapture"
        )),
        "expected forged capture constructor rejection, got: {errors:?}"
    );
}

#[test]
fn native_v2_batch_queries_reject_unknown_backend_ids() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

shape sphere_shape {
    field = sphere_field
    material = default_surface
    payload = Payload()
}

material default_surface(hit: Hit3) -> Surface {
    return Surface()
}

fn main() -> Integer {
    scene = capture sphere_shape
    points = [
        PointQuery(point=vec3(0.0, 0.0, 2.0))
    ]
    _ = distance_at_batch(capture=scene, points=points, backend=99)
    return 0
}
"#;
    let (errors, _info) = hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
    assert!(
        errors.iter().any(|err| matches!(
            err,
            hir::typeck::TypeError::ArgumentTypeMismatch { name, expected, found, .. }
                if name.as_str() == "backend"
                    && expected == "DispatchBackend"
                    && found == "Integer"
        )),
        "expected raw backend integer rejection, got: {errors:?}"
    );
}
