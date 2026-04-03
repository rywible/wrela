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
