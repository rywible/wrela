use std::fs;
use std::process::Command;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::hir::project::load_project;
use wrela::mir;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_plan::DispatchBackend;

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().expect("src parent")).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should have repo parent")
        .to_path_buf()
}

fn load_project_module(project_root: &str) -> hir::Module {
    let entry_path = repo_root().join(project_root).join("src").join("main.wr");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn collect_indirect_calls(module: &hir::Module) -> Vec<(smol_str::SmolStr, mir::CallTarget)> {
    let (_type_errors, type_info) = hir::typeck::check_module_with_info(module);
    let check_ir = hir::checkir::extract_module(module);
    let mut mir_module = mir::lower::lower_module_with_types(module, &type_info);
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

    let mut indirect_calls = Vec::new();
    for func in &mir_module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let mir::Stmt::Assign {
                    value: mir::Rvalue::Call { target, .. },
                    ..
                } = stmt
                    && matches!(target, mir::CallTarget::Indirect(_))
                {
                    indirect_calls.push((func.name.clone(), target.clone()));
                }
            }
        }
    }
    indirect_calls
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
    compile_and_run_native_module_impl(module, executable_name, false, DispatchBackend::Auto)
}

fn compile_and_run_native_module_with_backend(
    module: hir::Module,
    executable_name: &str,
    default_query_backend: DispatchBackend,
) -> std::process::Output {
    compile_and_run_native_module_impl(module, executable_name, false, default_query_backend)
}

fn compile_and_run_native_module_with_indirect_calls(
    module: hir::Module,
    executable_name: &str,
) -> std::process::Output {
    compile_and_run_native_module_impl(module, executable_name, true, DispatchBackend::Auto)
}

fn compile_and_run_native_inline_source_with_backend(
    source: &str,
    executable_name: &str,
    default_query_backend: DispatchBackend,
) -> std::process::Output {
    let module = lower_inline_module_from_source(source);
    compile_and_run_native_module_with_backend(module, executable_name, default_query_backend)
}

fn compile_and_run_native_project_source_with_backend(
    source: &str,
    executable_name: &str,
    default_query_backend: DispatchBackend,
) -> std::process::Output {
    let module = load_module_from_source(source);
    compile_and_run_native_module_with_backend(module, executable_name, default_query_backend)
}

fn compile_and_run_native_project_with_replaced_run(
    project_root: &str,
    run_source: &str,
    executable_name: &str,
    default_query_backend: DispatchBackend,
) -> std::process::Output {
    let entry_path = repo_root().join(project_root).join("src").join("main.wr");
    let mut source = fs::read_to_string(&entry_path)
        .unwrap_or_else(|err| panic!("read project source {} failed: {err}", entry_path.display()));
    let run_start = source
        .rfind("fn run() -> Integer {")
        .unwrap_or_else(|| panic!("missing run() in {}", entry_path.display()));
    source.truncate(run_start);
    source.push_str(run_source);
    compile_and_run_native_project_source_with_backend(
        &source,
        executable_name,
        default_query_backend,
    )
}

fn compile_and_run_native_module_impl(
    module: hir::Module,
    executable_name: &str,
    allow_indirect_calls: bool,
    default_query_backend: DispatchBackend,
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
    let mut mir_module =
        mir::lower::lower_module_with_types_and_backend(&module, &type_info, default_query_backend);
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");
    if !allow_indirect_calls {
        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let mir::Stmt::Assign {
                        value: mir::Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, mir::CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }
        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls before native codegen: {indirect_calls:?}"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join(executable_name);
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out)
        .unwrap_or_else(|err| panic!("codegen failed: {}", err.0));
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
fn native_v2_loop_carried_branch_local_boolean_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    let source = r#"
fn run() -> Integer {
    mutable seen = false
    mutable step = 0
    while step < 1 {
        if true {
            seen = true
        }
        step += 1
    }
    if seen {
        return 1
    }
    return 0
}
"#;

    let output =
        compile_and_run_native_source(source, "wr_v2_loop_carried_branch_local_boolean_smoke");
    let expected = expected_int_exit(1);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_loop_exit_boolean_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    let source = r#"
fn run() -> Integer {
    mutable seen = false
    mutable step = 0
    while step < 4 and not seen {
        if step == 2 {
            seen = true
        }
        step += 1
    }
    if seen {
        return step
    }
    return 0
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_loop_exit_boolean_smoke");
    let expected = expected_int_exit(3);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preview_projects_lower_without_indirect_calls() {
    for project_root in [
        "language/preview",
        "language/preview_boolean",
        "language/preview_repetition",
        "language/preview_thinstack",
    ] {
        let module = load_project_module(project_root);
        let indirect_calls = collect_indirect_calls(&module);
        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls for {project_root}: {indirect_calls:?}"
        );
    }
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
    handle = ActorHandle(id=u32(7), generation=u32(3))
    payload = Payload(entity_id=u32(7), material_id=u32(11), actor=handle)
    hit = Hit3(
        hit=true,
        distance=f32(4.0),
        position=vec3(1.0, 2.0, 3.0),
        normal=vec3(0.0, 1.0, 0.0),
        shading_frame=transform3_identity(),
        steps=0,
        feature_id=u32(0),
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
    translate = vec3(2.0, 0.0, 0.0) {
        sphere(radius=1.0)
    }
}

field conservative distance mirrored_box(p: Vec3) -> F32 {
    mirror_array = vec3(1.0, 0.0, 0.0) {
        translate = vec3(1.0, 0.0, 0.0) {
            box(half=vec3(0.5, 0.5, 0.5))
        }
    }
}

field conservative distance repeated_sphere(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}

field conservative distance instanced_sphere(p: Vec3) -> F32 {
    instance_array = Transform3(
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
    translate = vec3(0.0, 0.0, 1.0) {
        sphere(radius=1.0)
    }
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
fn native_v2_phase6_construction_operators_sample_correctly() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance extruded_disc(p: Vec3) -> F32 {
    extrude = f32(1.6) {
        circle2(radius = 0.75)
    }
}

field conservative distance revolved_orb(p: Vec3) -> F32 {
    revolve {
        circle2(radius = 0.5)
    }
}

field conservative distance swept_beam(p: Vec3) -> F32 {
    sweep = vec3(0.0, 1.6, 0.0) {
        circle2(radius = 0.15)
    }
}

field conservative distance lofted_form(p: Vec3) -> F32 {
    loft = f32(1.2) {
        from rect2(half = vec2(0.25, 0.18))
        to rounded_rect2(half = vec2(0.42, 0.28), radius = 0.08)
    }
}

field conservative distance polygon_plate(p: Vec3) -> F32 {
    extrude = f32(0.4) {
        polygon2(vertices = [
            vec2(-0.4, -0.3),
            vec2(0.5, -0.2),
            vec2(0.3, 0.4),
            vec2(-0.3, 0.35)
        ])
    }
}

field conservative distance capsule_rib(p: Vec3) -> F32 {
    extrude = f32(0.18) {
        capsule2(a = vec2(-0.24, 0.0), b = vec2(0.24, 0.0), radius = 0.06)
    }
}

field conservative distance segment_strip(p: Vec3) -> F32 {
    extrude = f32(0.12) {
        segment2(a = vec2(-0.28, 0.0), b = vec2(0.28, 0.0))
    }
}

field conservative distance polyline_strip(p: Vec3) -> F32 {
    extrude = f32(0.16) {
        polyline2(vertices = [
            vec2(-0.28, -0.10),
            vec2(0.0, 0.14),
            vec2(0.28, -0.10)
        ])
    }
}

fn main() -> Integer {
    disc_scene = capture extruded_disc
    orb_scene = capture revolved_orb
    beam_scene = capture swept_beam
    loft_scene = capture lofted_form
    plate_scene = capture polygon_plate

    disc_center = distance_at(capture=disc_scene, point=vec3(0.0, 0.0, 0.0))
    orb_center = distance_at(capture=orb_scene, point=vec3(0.0, 0.0, 0.0))
    beam_center = distance_at(capture=beam_scene, point=vec3(0.0, 0.0, 0.0))
    loft_center = distance_at(capture=loft_scene, point=vec3(0.0, 0.0, 0.0))
    plate_center = distance_at(capture=plate_scene, point=vec3(0.05, 0.0, 0.05))
    disc_outside = distance_at(capture=disc_scene, point=vec3(0.0, 1.2, 0.0))
    beam_outside = distance_at(capture=beam_scene, point=vec3(0.45, 0.0, 0.0))

    assert approx disc_center ~= -0.75 within 0.001
    assert approx orb_center ~= -0.5 within 0.001
    assert approx beam_center ~= -0.15 within 0.001
    assert value loft_center < 0.0
    assert value plate_center <= 0.0
    assert value disc_outside > 0.0
    assert value beam_outside > 0.0
    return 0
}
"#;

    let output = compile_and_run_native_module_with_indirect_calls(
        lower_inline_module_from_source(source),
        "wr_v2_phase6_construction_operators_sample_correctly",
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
fn native_v2_phase6_constructed_fields_enable_support_pruning() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance near_disc(p: Vec3) -> F32 {
    extrude = f32(1.2) {
        circle2(radius = 0.55)
    }
}

field conservative distance far_loft(p: Vec3) -> F32 {
    translate = vec3(9.5, 0.0, 0.0) {
        loft = f32(1.4) {
            from circle2(radius = 0.32)
            to rounded_rect2(half = vec2(0.50, 0.32), radius = 0.08)
        }
    }
}

material warm_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.7, 0.4),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_disc
    material = warm_surface
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_shape {
    field = far_loft
    material = warm_surface
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape scene_shape {
    union {
        provenance_policy = nearest
        use near_shape
        use far_shape
    }
}

fn main() -> Integer {
    pruned_before = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    candidates_before = __wr_metrics_get(__wr_metrics_scene_trace_candidate_branch_id())
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
    candidates_after = __wr_metrics_get(__wr_metrics_scene_trace_candidate_branch_id())

    if hit.hit != true { return 1 }
    if hit.payload.material_id != u32(1) { return 2 }
    if pruned_after - pruned_before <= 0 { return 3 }
    if candidates_after - candidates_before < 2 { return 4 }
    return 0
}
"#;

    let output = compile_and_run_native_module_with_indirect_calls(
        lower_inline_module_from_source(source),
        "wr_v2_phase6_constructed_fields_enable_support_pruning",
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
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
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
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        box(half=vec3(0.5, 0.5, 0.5))
    }
}

field conservative distance manual_box(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        box(half=vec3(0.5, 0.5, 0.5))
    }
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
        shading_frame=transform3_identity(),
        steps=0,
        feature_id=i32(0),
        payload=Payload(
            entity_id=u32(0),
            material_id=u32(0),
            actor=ActorHandle(id=u32(0), generation=u32(0))
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
                shading_frame=transform3_identity(),
                steps=i32(repeat_steps + 1),
                feature_id=i32(0),
                payload=Payload(
                    entity_id=u32(0),
                    material_id=u32(0),
                    actor=ActorHandle(id=u32(0), generation=u32(0))
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
        shading_frame=transform3_identity(),
        steps=0,
        feature_id=i32(0),
        payload=Payload(
            entity_id=u32(0),
            material_id=u32(0),
            actor=ActorHandle(id=u32(0), generation=u32(0))
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
                shading_frame=transform3_identity(),
                steps=i32(manual_steps + 1),
                feature_id=i32(0),
                payload=Payload(
                    entity_id=u32(0),
                    material_id=u32(0),
                    actor=ActorHandle(id=u32(0), generation=u32(0))
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
fn native_v2_authored_support_quarantine_preserves_hit_identity() {
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

field conservative distance far_semantic(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
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
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_supported_shape {
    field = far_supported
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape far_semantic_shape {
    field = far_semantic
    material = shade
    payload = Payload(
        entity_id=u32(3),
        material_id=u32(3),
        actor=ActorHandle(id=u32(3), generation=u32(0))
    )
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_supported_shape
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

    semantic_pruned_before = supported_pruned_after
    semantic_scene_capture = capture semantic_scene
    semantic_hit = trace_shape(
        capture=semantic_scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    semantic_surface = surface_at(capture=semantic_scene_capture, hit=semantic_hit)
    semantic_pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())

    if supported_hit.hit != true { return 1 }
    if semantic_hit.hit != true { return 2 }
    if supported_hit.feature_id != semantic_hit.feature_id { return 3 }
    if abs(supported_hit.distance - semantic_hit.distance) > 0.0001 { return 4 }
    if supported_surface.albedo.x != semantic_surface.albedo.x { return 5 }
    if supported_pruned_after - supported_pruned_before != 0 { return 6 }
    if semantic_pruned_after - semantic_pruned_before <= 0 { return 7 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_authored_support_quarantine_preserves_hit_identity",
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
    local_frame = inverse_transform3(transform=hit.shading_frame)
    local_origin = transform_point(transform=local_frame, point=hit.position)

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
    assert approx local_origin.x ~= 0.0 within 0.001
    assert approx local_origin.y ~= 0.0 within 0.001
    assert approx local_origin.z ~= 0.0 within 0.001
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
fn native_v2_authored_support_quarantines_custom_field_branches() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance near_orb(p: Vec3) -> F32 {
    sphere(radius=0.65)
}

field conservative distance far_custom_supported(p: Vec3) -> F32 {
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

field conservative distance far_semantic(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}

material near_shade(hit: Hit3) -> Surface {
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

material far_shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.0, 1.0, 0.0),
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
    material = near_shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_supported_shape {
    field = far_custom_supported
    material = far_shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape far_semantic_shape {
    field = far_semantic
    material = far_shade
    payload = Payload(
        entity_id=u32(3),
        material_id=u32(3),
        actor=ActorHandle(id=u32(3), generation=u32(0))
    )
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_supported_shape
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
    pruned_before = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    candidate_before = __wr_metrics_get(__wr_metrics_scene_trace_candidate_branch_id())
    near_capture = capture near_shape
    near_hit = trace_shape(
        capture=near_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
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
    candidate_mid = __wr_metrics_get(__wr_metrics_scene_trace_candidate_branch_id())
    supported_surface = surface_at(capture=scene_capture, hit=supported_hit)
    semantic_pruned_before = pruned_mid
    semantic_candidate_before = candidate_mid
    semantic_capture = capture semantic_scene
    semantic_hit = trace_shape(
        capture=semantic_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    semantic_pruned_after = __wr_metrics_get(__wr_metrics_scene_trace_support_pruned_branch_id())
    semantic_candidate_after = __wr_metrics_get(__wr_metrics_scene_trace_candidate_branch_id())
    semantic_surface = surface_at(capture=semantic_capture, hit=semantic_hit)
    if near_hit.hit != true { return 12 }
    if supported_hit.hit != true { return 1 }
    if semantic_hit.hit != true { return 2 }
    if supported_hit.payload.entity_id != near_hit.payload.entity_id {
        if supported_hit.payload.entity_id == u32(2) { return 17 }
        if supported_hit.payload.entity_id == u32(0) { return 18 }
        return 19
    }
    if semantic_hit.payload.entity_id != near_hit.payload.entity_id { return 20 }
    if supported_hit.feature_id != near_hit.feature_id {
        if supported_hit.feature_id == u32(753004254) { return 13 }
        if supported_hit.feature_id == u32(0) { return 14 }
        return 15
    }
    if semantic_hit.feature_id != near_hit.feature_id { return 16 }
    if supported_hit.feature_id != semantic_hit.feature_id { return 3 }
    if abs(supported_hit.distance - semantic_hit.distance) > 0.0001 { return 4 }
    if candidate_mid - candidate_before <= 0 { return 5 }
    if semantic_candidate_after - semantic_candidate_before <= 0 { return 6 }
    if pruned_mid - pruned_before != 0 { return 7 }
    if semantic_pruned_after - semantic_pruned_before <= 0 { return 8 }
    if supported_surface.albedo.x != semantic_surface.albedo.x { return 9 }
    if supported_surface.albedo.y != semantic_surface.albedo.y { return 10 }
    if supported_surface.albedo.z != semantic_surface.albedo.z { return 11 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_authored_support_quarantines_custom_field_branches",
    );
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected_int_exit(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_authored_support_bounds_quarantine_custom_fields() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    fn authored_scene_source() -> &'static str {
        r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius=0.5)
}

field conservative distance far_field(p: Vec3) -> F32 {
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
    field = near_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_shape {
    field = far_field
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape scene_shape {
    union {
        provenance_policy = nearest
        use near_shape
        use far_shape
    }
}

fn main() -> Integer {
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
}
"#
    }

    fn semantic_scene_source() -> &'static str {
        r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius=0.5)
}

field conservative distance far_field(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
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
    field = near_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_shape {
    field = far_field
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape scene_shape {
    union {
        provenance_policy = nearest
        use near_shape
        use far_shape
    }
}

fn main() -> Integer {
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
}
"#
    }

    let authored_output = compile_and_run_native_inline_source(
        authored_scene_source(),
        "wr_v2_authored_support_bounds_quarantine_custom_fields",
    );
    let authored_delta = authored_output.status.code().unwrap_or(-1);

    let semantic_output = compile_and_run_native_inline_source(
        semantic_scene_source(),
        "wr_v2_semantic_support_bounds_enable_support_pruning",
    );
    let semantic_delta = semantic_output.status.code().unwrap_or(-1);

    assert!(
        authored_delta == expected_int_exit(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&authored_output.stdout),
        String::from_utf8_lossy(&authored_output.stderr)
    );
    assert!(
        semantic_delta > 0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&semantic_output.stdout),
        String::from_utf8_lossy(&semantic_output.stderr)
    );
}

#[test]
fn native_v2_shape_provenance_tracks_boolean_winners() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance left_orb(p: Vec3) -> F32 {
    translate = vec3(-0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

field exact distance right_orb(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
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
    translate = vec3(0.0, 0.0, 1.6) {
        sphere(radius=0.45)
    }
}

field exact distance far_orb(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, 0.9) {
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
        entity_id=u32(31),
        material_id=u32(31),
        actor=ActorHandle(id=u32(31), generation=u32(0))
    )
}

shape right_shape {
    field = right_orb
    material = right_surface
    payload = Payload(
        entity_id=u32(32),
        material_id=u32(32),
        actor=ActorHandle(id=u32(32), generation=u32(0))
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
        entity_id=u32(41),
        material_id=u32(41),
        actor=ActorHandle(id=u32(41), generation=u32(0))
    )
}

shape cutter_shape {
    field = cutter
    material = cutter_surface
    payload = Payload(
        entity_id=u32(42),
        material_id=u32(42),
        actor=ActorHandle(id=u32(42), generation=u32(0))
    )
}

shape near_shape {
    field = near_orb
    material = left_surface
    payload = Payload(
        entity_id=u32(61),
        material_id=u32(61),
        actor=ActorHandle(id=u32(61), generation=u32(0))
    )
}

shape far_shape {
    field = far_orb
    material = right_surface
    payload = Payload(
        entity_id=u32(62),
        material_id=u32(62),
        actor=ActorHandle(id=u32(62), generation=u32(0))
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
    assert value left_lr.payload.entity_id == u32(31)
    assert value right_lr.payload.entity_id == u32(32)
    assert value left_rl.payload.entity_id == u32(31)
    assert value right_rl.payload.entity_id == u32(32)
    assert value carve_hit.hit == true
    assert value carve_hit.payload.entity_id == u32(42)
    assert value near_direct.feature_id != u32(0)
    assert value far_direct.feature_id != u32(0)
    assert value nearest_hit.hit == true
    assert value ordered_hit.hit == true
    assert value overlap_hit.hit == true
    assert value nearest_hit.payload.entity_id == u32(61)
    assert value ordered_hit.payload.entity_id == u32(62)
    assert value overlap_hit.payload.entity_id == u32(62)
    assert value nearest_hit.feature_id == near_direct.feature_id
    assert value ordered_hit.feature_id == far_direct.feature_id
    assert value overlap_hit.feature_id == far_direct.feature_id
    assert value carve_hit.feature_id == cutter_hit.feature_id
    assert value carve_left_hit.hit == true
    assert value carve_left_hit.payload.entity_id == u32(41)
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
    assert_eq!(
        output.status.code().unwrap_or(-1),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
    mirror_array = vec3(1.0, 0.0, 0.0) {
        repeat_linear = vec3(2.0, 0.0, 0.0) {
            translate = vec3(0.0, 0.0, 0.0) {
                instance_array = Transform3(
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
    translate = vec3(4.0, 0.0, 0.0) {
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
        entity_id=u32(101),
        material_id=u32(101),
        actor=ActorHandle(id=u32(101), generation=u32(0))
    )
}

shape tie_right {
    field = tie_right_field
    material = right_surface
    payload = Payload(
        entity_id=u32(202),
        material_id=u32(202),
        actor=ActorHandle(id=u32(202), generation=u32(0))
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
        entity_id=u32(303),
        material_id=u32(303),
        actor=ActorHandle(id=u32(303), generation=u32(0))
    )
}

shape wrapped_decoy_shape {
    field = wrapped_decoy
    material = right_surface
    payload = Payload(
        entity_id=u32(404),
        material_id=u32(404),
        actor=ActorHandle(id=u32(404), generation=u32(0))
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
    assert value tie_union_hit.payload.entity_id == u32(101)
    assert value tie_intersection_hit.payload.entity_id == u32(101)
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
    plane(normal = vec3(1.0, 0.0, 0.0), offset = 0.0)
}

field conservative distance left_y(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.0)
}

field conservative distance cap_z(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 0.0, 1.0), offset = 0.0)
}

field conservative distance notch(p: Vec3) -> F32 {
    plane(normal = vec3(1.0, 0.0, 0.0), offset = -0.5)
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
        entity_id=u32(7),
        material_id=u32(9),
        actor=ActorHandle(id=u32(1), generation=u32(0))
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
fn native_v2_phase10_wgsl_batch_queries_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material scene_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape scene_shape {
    field = scene_field
    material = scene_surface
    payload = Payload(
        entity_id=u32(11),
        material_id=u32(22),
        actor=ActorHandle(id=u32(33), generation=u32(0))
    )
}

fn main() -> Integer {
    scene_capture = capture scene_shape
    field = capture scene_field
    points = [
        PointQuery(point=vec3(0.0, 0.0, 2.0)),
        PointQuery(point=vec3(0.0, 0.0, 3.0))
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
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 1.0, 0.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    cpu_field_distances = distance_at_batch(
        capture=field,
        points=points,
        backend=dispatch_backend_cpu()
    )
    auto_field_distances = distance_at_batch(
        capture=field,
        points=points,
        backend=dispatch_backend_auto()
    )
    cpu_shape_normals = normal_at_batch(
        capture=scene_capture,
        points=points,
        backend=dispatch_backend_cpu()
    )
    auto_shape_normals = normal_at_batch(
        capture=scene_capture,
        points=points,
        backend=dispatch_backend_auto()
    )
    cpu_hits = trace_shape_batch(
        capture=scene_capture,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    auto_hits = trace_shape_batch(
        capture=scene_capture,
        rays=rays,
        backend=dispatch_backend_auto()
    )
    cpu_surfaces = surface_at_batch(
        capture=scene_capture,
        hits=cpu_hits,
        backend=dispatch_backend_cpu()
    )
    auto_surfaces = surface_at_batch(
        capture=scene_capture,
        hits=auto_hits,
        backend=dispatch_backend_auto()
    )
    cpu_occlusions = occluded_batch(
        capture=scene_capture,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    auto_occlusions = occluded_batch(
        capture=scene_capture,
        rays=rays,
        backend=dispatch_backend_auto()
    )

    if abs(cpu_field_distances[0].distance - auto_field_distances[0].distance) > 0.01 { return 1 }
    if abs(cpu_field_distances[1].distance - auto_field_distances[1].distance) > 0.01 { return 2 }
    if abs(cpu_shape_normals[0].normal.z - auto_shape_normals[0].normal.z) > 0.01 { return 3 }
    if cpu_hits[0].hit != auto_hits[0].hit { return 4 }
    if abs(cpu_hits[0].distance - auto_hits[0].distance) > 0.01 { return 5 }
    if cpu_hits[0].feature_id != auto_hits[0].feature_id { return 6 }
    if abs(cpu_surfaces[0].albedo.x - auto_surfaces[0].albedo.x) > 0.01 { return 7 }
    if cpu_occlusions[0].occluded != auto_occlusions[0].occluded { return 8 }
    if cpu_occlusions[1].occluded != auto_occlusions[1].occluded { return 9 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source_with_backend(
        source,
        "wr_v2_phase10_wgsl_batch_queries_smoke",
        DispatchBackend::Wgsl,
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
fn native_v2_phase10_wgsl_capture_queries_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material scene_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.18, 0.28, 0.58),
        roughness=0.24,
        metalness=0.06,
        clearcoat=0.08,
        clearcoat_roughness=0.05,
        sheen=0.02,
        emissive=vec3(0.01, 0.02, 0.03)
    )
}

radiance field scene_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    horizon = clamp(0.5 + direction.y * 0.5, 0.0, 1.0)
    return vec3(0.06, 0.09, 0.16) * (1.0 - horizon) + vec3(0.14, 0.24, 0.48) * horizon
}

volume field scene_medium(p: Vec3, surface_distance: F32) -> Medium {
    density = clamp(0.04 + clamp(0.18 - abs(surface_distance), 0.0, 0.18) * 0.45, 0.0, 0.12)
    return Medium(
        density=density,
        emission=vec3(0.03, 0.02, 0.01) * density,
        anisotropy=0.12
    )
}

shape scene_shape {
    field = scene_field
    material = scene_surface
    radiance = scene_radiance
    volume = scene_medium
    payload = Payload(
        entity_id=u32(17),
        material_id=u32(23),
        actor=ActorHandle(id=u32(31), generation=u32(0))
    )
}

fn main() -> Integer {
    scene_capture = capture scene_shape
    field_capture = capture scene_field
    probe = vec3(0.0, 0.0, 1.2)

    sampled_distance = distance_at(capture=field_capture, point=probe)
    sampled_normal = normal_at(capture=scene_capture, point=probe)
    sampled_hit = trace_shape(
        capture=scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    sampled_surface = surface_at(capture=scene_capture, hit=sampled_hit)
    sampled_radiance = radiance_at(
        capture=scene_capture,
        point=sampled_hit.position,
        direction=normalize(vec3(0.0, 1.0, 1.0))
    )
    sampled_medium = medium_at(capture=scene_capture, point=sampled_hit.position)

    if abs(sampled_distance - 0.6) > 0.01 { return 1 }
    if abs(sampled_normal.z - 1.0) > 0.01 { return 2 }
    if not sampled_hit.hit { return 3 }
    if abs(sampled_hit.distance - 2.4) > 0.01 { return 4 }
    if abs(sampled_hit.position.z - 0.6) > 0.02 { return 5 }
    if abs(sampled_surface.albedo.z - 0.58) > 0.01 { return 6 }
    if abs(sampled_radiance.y - 0.218) > 0.02 { return 7 }
    if abs(sampled_medium.density - 0.12) > 0.01 { return 8 }
    if abs(sampled_medium.emission.x - 0.0036) > 0.005 { return 9 }
    if abs(sampled_medium.anisotropy - 0.12) > 0.01 { return 10 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source_with_backend(
        source,
        "wr_v2_phase10_wgsl_capture_queries_smoke",
        DispatchBackend::Wgsl,
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
    let (errors, _info) =
        hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
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
        scene_id=u32(999),
        epoch=u32(0),
        root_feature_id=u32(0)
    )
    _ = distance_at(capture=forged, point=vec3(0.0, 0.0, 0.0))
    return 0
}
"#;
    let (errors, _info) =
        hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
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
fn native_v2_phase7_queries_require_shape_capture_smoke() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

fn main() -> Integer {
    scene = capture sphere_field
    _ = radiance_at(
        capture=scene,
        point=vec3(0.0, 0.0, 0.0),
        direction=vec3(0.0, 0.0, -1.0)
    )
    _ = medium_at(capture=scene, point=vec3(0.0, 0.0, 0.0))
    return 0
}
"#;
    let (errors, _info) =
        hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
    let rejection_count = errors
        .iter()
        .filter(|err| {
            matches!(
                err,
                hir::typeck::TypeError::ShapeQueryTargetMustBeShape { query, .. }
                    if query.as_str() == "radiance_at" || query.as_str() == "medium_at"
            )
        })
        .count();
    assert_eq!(
        rejection_count, 2,
        "expected radiance_at and medium_at field capture rejection, got: {errors:?}"
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
    let (errors, _info) =
        hir::typeck::check_module_with_info(&lower_inline_module_from_source(source));
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

#[test]
fn native_v2_phase5_smooth_and_deformation_cost_regression() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance control_field(p: Vec3) -> F32 {
    rounded_box(half=vec3(0.60, 0.48, 0.36), radius=0.10)
}

field conservative distance smooth_left(p: Vec3) -> F32 {
    translate = vec3(-0.55, 0.0, 0.0) {
        use control_field
    }
}

field conservative distance smooth_right(p: Vec3) -> F32 {
    translate = vec3(0.55, 0.0, 0.0) {
        ellipsoid(radii=vec3(0.58, 0.38, 0.46))
    }
}

field conservative distance smooth_scene_field(p: Vec3) -> F32 {
    smooth_union {
        smoothing = f32(0.18)
        use smooth_left
        use smooth_right
    }
}

field conservative distance deform_source(p: Vec3) -> F32 {
    slab(thickness=0.18)
}

field conservative distance bend_field(p: Vec3) -> F32 {
    bend = vec3(0.0, 0.30, 0.0) {
        use deform_source
    }
}

field conservative distance twist_field(p: Vec3) -> F32 {
    twist = vec3(0.0, 0.30, 0.0) {
        use deform_source
    }
}

field conservative distance taper_field(p: Vec3) -> F32 {
    taper = vec3(0.0, 0.20, 0.0) {
        use deform_source
    }
}

field conservative distance displace_field(p: Vec3) -> F32 {
    displace = vec3(0.06, 0.0, 0.0) {
        use deform_source
    }
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

shape control_shape {
    field = control_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape smooth_shape {
    field = smooth_scene_field
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape bend_shape {
    field = bend_field
    material = shade
    payload = Payload(
        entity_id=u32(3),
        material_id=u32(3),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape twist_shape {
    field = twist_field
    material = shade
    payload = Payload(
        entity_id=u32(4),
        material_id=u32(4),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape taper_shape {
    field = taper_field
    material = shade
    payload = Payload(
        entity_id=u32(5),
        material_id=u32(5),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape displace_shape {
    field = displace_field
    material = shade
    payload = Payload(
        entity_id=u32(6),
        material_id=u32(6),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

fn main() -> Integer {
    control_before = __wr_metrics_get(__wr_metrics_field_sample_id())
    control_capture = capture control_shape
    control_hit = trace_shape(
        capture=control_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    control_after = __wr_metrics_get(__wr_metrics_field_sample_id())

    smooth_before = control_after
    smooth_capture = capture smooth_shape
    smooth_hit = trace_shape(
        capture=smooth_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    smooth_after = __wr_metrics_get(__wr_metrics_field_sample_id())

    deform_before = smooth_after
    deform_capture = capture bend_shape
    bend_hit = trace_shape(
        capture=deform_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    twist_capture = capture twist_shape
    twist_hit = trace_shape(
        capture=twist_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    taper_capture = capture taper_shape
    taper_hit = trace_shape(
        capture=taper_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    displace_capture = capture displace_shape
    displace_hit = trace_shape(
        capture=displace_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    deform_after = __wr_metrics_get(__wr_metrics_field_sample_id())

    control_samples = control_after - control_before
    smooth_samples = smooth_after - smooth_before
    deform_samples = deform_after - deform_before

    if control_hit.hit != true { return 1 }
    if smooth_hit.hit != true { return 2 }
    if bend_hit.hit != true { return 3 }
    if twist_hit.hit != true { return 4 }
    if taper_hit.hit != true { return 5 }
    if displace_hit.hit != true { return 6 }
    if smooth_samples <= control_samples { return 7 }
    if deform_samples <= control_samples { return 8 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase5_smooth_and_deformation_cost_regression",
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
fn native_v2_phase7_radiance_and_volume_surface_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance phase7_shell(p: Vec3) -> F32 {
    sphere(radius = 0.45)
}

radiance field phase7_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    sky_t = clamp(0.5 + direction.y * 0.5, 0.0, 1.0)
    feature_bias = clamp(f32(feature_id), 0.0, 1.0)
    point_bias = clamp(0.2 - abs(p.z), 0.0, 0.2) * 5.0
    return vec3(0.10, 0.18, 0.28) * (1.0 - sky_t)
        + vec3(0.36, 0.54, 0.82) * sky_t
        + vec3(0.08, 0.04, 0.02) * point_bias * feature_bias
}

volume field phase7_volume(p: Vec3, surface_distance: F32) -> Medium {
    surface_bias = clamp(0.2 - abs(surface_distance), 0.0, 0.2) * 0.4
    density = clamp(0.08 + abs(p.y) * 0.02 + abs(p.x) * 0.01 + surface_bias, 0.0, 0.16)
    return Medium(
        density=density,
        emission=vec3(0.06, 0.08, 0.10) * density + vec3(0.0, 0.01, 0.02) * surface_bias,
        anisotropy=0.12
    )
}

material phase7_surface(hit: Hit3) -> Surface {
    ridge = clamp(abs(hit.local_position.y) * 0.8 + abs(hit.local_normal.x) * 0.2, 0.0, 1.0)
    return Surface(
        albedo=vec3(0.26, 0.34, 0.44) + vec3(0.08, 0.06, 0.04) * ridge,
        roughness=0.16 + ridge * 0.18,
        metalness=0.12 + clamp(hit.local_normal.z, 0.0, 1.0) * 0.18,
        clearcoat=0.14 + clamp(hit.local_normal.y, 0.0, 1.0) * 0.16,
        clearcoat_roughness=0.08 + abs(hit.local_position.x) * 0.12,
        sheen=0.06 + abs(hit.local_normal.x) * 0.10,
        emissive=vec3(0.04, 0.02, 0.01) * clamp(hit.local_normal.z, 0.0, 1.0)
    )
}

shape phase7_scene_shape {
    field = phase7_shell
    material = phase7_surface
    radiance = phase7_radiance
    volume = phase7_volume
    payload = Payload(
        entity_id=u32(901),
        material_id=u32(901),
        actor=ActorHandle(id=u32(901), generation=u32(0))
    )
}

fn compute_ambient_occlusion(scene_capture: ShapeCapture, hit_position: Vec3, hit_normal: Vec3) -> F32 {
    sample_a = distance_at(capture=scene_capture, point=hit_position + hit_normal * 0.08)
    sample_b = distance_at(capture=scene_capture, point=hit_position + hit_normal * 0.18)
    occlusion = clamp((0.08 - sample_a) * 2.0 + (0.18 - sample_b) * 1.1, 0.0, 1.0)
    return 1.0 - occlusion * 0.75
}

fn main() -> Integer {
    scene_capture = capture phase7_scene_shape
    hit = trace_shape(
        capture=scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene_capture, hit=hit)
    radiance_sample = radiance_at(
        capture=scene_capture,
        point=hit.position,
        direction=normalize(vec3(0.0, 1.0, 1.0)),
    )
    medium_sample = medium_at(capture=scene_capture, point=hit.position)
    ambient = compute_ambient_occlusion(
        scene_capture=scene_capture,
        hit_position=hit.position,
        hit_normal=hit.normal
    )

    if hit.hit != true { return 1 }
    if hit.feature_id == u32(0) { return 2 }
    if hit.local_position.z <= 0.0 { return 3 }
    if hit.local_normal.z <= 0.0 { return 4 }
    if surface.metalness <= 0.12 { return 5 }
    if surface.clearcoat < 0.14 { return 6 }
    if surface.sheen <= 0.05 { return 7 }
    if surface.emissive.x <= 0.0 { return 8 }
    if radiance_sample.z <= radiance_sample.x { return 9 }
    if abs(medium_sample.anisotropy - 0.12) > 0.001 { return 10 }
    if medium_sample.density <= 0.08 { return 11 }
    if ambient <= 0.0 { return 12 }
    if ambient > 1.0 { return 13 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase7_radiance_and_volume_surface_smoke",
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
fn native_v2_phase9_rotated_trace_reports_leaf_local_normal() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance rotated_sphere_field(p: Vec3) -> F32 {
    rotate = vec3(0.0, 1.5707963, 0.0) {
        sphere(radius = 1.0)
    }
}

material rotated_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape rotated_shape {
    field = rotated_sphere_field
    material = rotated_surface
    payload = Payload(
        entity_id=u32(950),
        material_id=u32(950),
        actor=ActorHandle(id=u32(950), generation=u32(0))
    )
}

fn main() -> Integer {
    scene = capture rotated_shape
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )

    if hit.hit != true { return 1 }
    if hit.normal.z < 0.95 { return 2 }
    if abs(abs(hit.local_normal.x) - 1.0) > 0.05 { return 3 }
    if abs(hit.local_normal.z) > 0.05 { return 4 }
    if abs(abs(hit.local_position.x) - 1.0) > 0.05 { return 5 }
    if abs(hit.local_position.z) > 0.05 { return 6 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase9_rotated_trace_reports_leaf_local_normal",
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
fn native_v2_phase9_rotated_ellipsoid_matches_reference_local_frame() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance phase5_ellipsoid_field(p: Vec3) -> F32 {
    ellipsoid(radii=vec3(0.58, 0.38, 0.46))
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape phase5_ellipsoid_shape {
    field = phase5_ellipsoid_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

field conservative distance phase9_rotated_ellipsoid_field(p: Vec3) -> F32 {
    rotate = vec3(0.35, 0.0, 0.0) {
        use phase5_ellipsoid_field
    }
}

shape phase9_rotated_ellipsoid_shape {
    field = phase9_rotated_ellipsoid_field
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

fn main() -> Integer {
    angle = 0.35
    inverse_rotation = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, cos(angle), -sin(angle), 0.0),
            vec4(0.0, sin(angle), cos(angle), 0.0),
            vec4(0.0, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, cos(angle), sin(angle), 0.0),
            vec4(0.0, -sin(angle), cos(angle), 0.0),
            vec4(0.0, 0.0, 0.0, 1.0)
        )
    )
    world_origin = vec3(0.18, 0.07, 3.0)
    world_direction = vec3(0.0, 0.0, -1.0)
    reference_origin = transform_point(transform=inverse_rotation, point=world_origin)
    reference_direction = transform_vector(transform=inverse_rotation, vector=world_direction)
    rotated_scene = capture phase9_rotated_ellipsoid_shape
    reference_scene = capture phase5_ellipsoid_shape
    rotated_hit = trace_shape(
        capture=rotated_scene,
        origin=world_origin,
        direction=world_direction,
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    reference_hit = trace_shape(
        capture=reference_scene,
        origin=reference_origin,
        direction=reference_direction,
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    rotated_hits = trace_shape_batch(
        capture=rotated_scene,
        rays=[
            RayQuery(
                origin=world_origin,
                direction=world_direction,
                max_distance=6.0,
                min_step=0.05,
                hit_epsilon=0.001,
                max_steps=96
            )
        ],
        backend=dispatch_backend_cpu()
    )
    reference_hits = trace_shape_batch(
        capture=reference_scene,
        rays=[
            RayQuery(
                origin=reference_origin,
                direction=reference_direction,
                max_distance=6.0,
                min_step=0.05,
                hit_epsilon=0.001,
                max_steps=96
            )
        ],
        backend=dispatch_backend_cpu()
    )

    if not rotated_hit.hit or not reference_hit.hit { return 1 }
    if not rotated_hits[0].hit or not reference_hits[0].hit { return 2 }
    if abs(rotated_hit.local_position.x - reference_hit.local_position.x) > 0.01 { return 3 }
    if abs(rotated_hit.local_position.y - reference_hit.local_position.y) > 0.01 { return 4 }
    if abs(rotated_hit.local_position.z - reference_hit.local_position.z) > 0.01 { return 5 }
    if abs(rotated_hit.local_normal.x - reference_hit.local_normal.x) > 0.01 { return 6 }
    if abs(rotated_hit.local_normal.y - reference_hit.local_normal.y) > 0.01 { return 7 }
    if abs(rotated_hit.local_normal.z - reference_hit.local_normal.z) > 0.01 { return 8 }
    if abs(rotated_hits[0].local_position.x - reference_hits[0].local_position.x) > 0.01 { return 9 }
    if abs(rotated_hits[0].local_position.y - reference_hits[0].local_position.y) > 0.01 { return 10 }
    if abs(rotated_hits[0].local_position.z - reference_hits[0].local_position.z) > 0.01 { return 11 }
    if abs(rotated_hits[0].local_normal.x - reference_hits[0].local_normal.x) > 0.01 { return 12 }
    if abs(rotated_hits[0].local_normal.y - reference_hits[0].local_normal.y) > 0.01 { return 13 }
    if abs(rotated_hits[0].local_normal.z - reference_hits[0].local_normal.z) > 0.01 { return 14 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase9_rotated_ellipsoid_matches_reference_local_frame",
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
fn native_v2_phase9_rotated_exact_primitives_match_reference_local_normals() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance phase9_capsule_reference(p: Vec3) -> F32 {
    capsule(a = vec3(0.0, -0.8, 0.0), b = vec3(0.0, 0.8, 0.0), radius = 0.35)
}

field exact distance phase9_capsule_rotated(p: Vec3) -> F32 {
    rotate = vec3(0.55, 0.0, 0.0) {
        capsule(a = vec3(0.0, -0.8, 0.0), b = vec3(0.0, 0.8, 0.0), radius = 0.35)
    }
}

field exact distance phase9_cylinder_reference(p: Vec3) -> F32 {
    cylinder(radius = 0.45, half_height = 0.9)
}

field exact distance phase9_cylinder_rotated(p: Vec3) -> F32 {
    rotate = vec3(0.55, 0.0, 0.0) {
        cylinder(radius = 0.45, half_height = 0.9)
    }
}

field exact distance phase9_torus_reference(p: Vec3) -> F32 {
    torus(major_radius = 1.0, minor_radius = 0.28)
}

field exact distance phase9_torus_rotated(p: Vec3) -> F32 {
    rotate = vec3(0.55, 0.0, 0.0) {
        torus(major_radius = 1.0, minor_radius = 0.28)
    }
}

material phase9_local_normal_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape phase9_capsule_reference_shape {
    field = phase9_capsule_reference
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(960),
        material_id=u32(960),
        actor=ActorHandle(id=u32(960), generation=u32(0))
    )
}

shape phase9_capsule_rotated_shape {
    field = phase9_capsule_rotated
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(961),
        material_id=u32(961),
        actor=ActorHandle(id=u32(961), generation=u32(0))
    )
}

shape phase9_cylinder_reference_shape {
    field = phase9_cylinder_reference
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(962),
        material_id=u32(962),
        actor=ActorHandle(id=u32(962), generation=u32(0))
    )
}

shape phase9_cylinder_rotated_shape {
    field = phase9_cylinder_rotated
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(963),
        material_id=u32(963),
        actor=ActorHandle(id=u32(963), generation=u32(0))
    )
}

shape phase9_torus_reference_shape {
    field = phase9_torus_reference
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(964),
        material_id=u32(964),
        actor=ActorHandle(id=u32(964), generation=u32(0))
    )
}

shape phase9_torus_rotated_shape {
    field = phase9_torus_rotated
    material = phase9_local_normal_surface
    payload = Payload(
        entity_id=u32(965),
        material_id=u32(965),
        actor=ActorHandle(id=u32(965), generation=u32(0))
    )
}

fn main() -> Integer {
    angle = 0.55
    inverse_rotation = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, cos(angle), -sin(angle), 0.0),
            vec4(0.0, sin(angle), cos(angle), 0.0),
            vec4(0.0, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, cos(angle), sin(angle), 0.0),
            vec4(0.0, -sin(angle), cos(angle), 0.0),
            vec4(0.0, 0.0, 0.0, 1.0)
        )
    )

    capsule_world_origin = vec3(0.18, 0.55, 3.0)
    cylinder_world_origin = vec3(0.22, 0.4, 3.0)
    torus_world_origin = vec3(1.15, 0.12, 3.0)
    world_direction = vec3(0.0, 0.0, -1.0)
    reference_direction = transform_vector(transform=inverse_rotation, vector=world_direction)

    capsule_reference_origin = transform_point(transform=inverse_rotation, point=capsule_world_origin)
    cylinder_reference_origin = transform_point(transform=inverse_rotation, point=cylinder_world_origin)
    torus_reference_origin = transform_point(transform=inverse_rotation, point=torus_world_origin)

    capsule_rotated_scene = capture phase9_capsule_rotated_shape
    capsule_reference_scene = capture phase9_capsule_reference_shape
    cylinder_rotated_scene = capture phase9_cylinder_rotated_shape
    cylinder_reference_scene = capture phase9_cylinder_reference_shape
    torus_rotated_scene = capture phase9_torus_rotated_shape
    torus_reference_scene = capture phase9_torus_reference_shape

    capsule_rotated_hit = trace_shape(
        capture=capsule_rotated_scene,
        origin=capsule_world_origin,
        direction=world_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )
    capsule_reference_hit = trace_shape(
        capture=capsule_reference_scene,
        origin=capsule_reference_origin,
        direction=reference_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )

    cylinder_rotated_hit = trace_shape(
        capture=cylinder_rotated_scene,
        origin=cylinder_world_origin,
        direction=world_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )
    cylinder_reference_hit = trace_shape(
        capture=cylinder_reference_scene,
        origin=cylinder_reference_origin,
        direction=reference_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )

    torus_rotated_hit = trace_shape(
        capture=torus_rotated_scene,
        origin=torus_world_origin,
        direction=world_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )
    torus_reference_hit = trace_shape(
        capture=torus_reference_scene,
        origin=torus_reference_origin,
        direction=reference_direction,
        max_distance=8.0,
        min_step=0.02,
        hit_epsilon=0.001,
        max_steps=128
    )

    if not capsule_rotated_hit.hit or not capsule_reference_hit.hit { return 1 }
    if not cylinder_rotated_hit.hit or not cylinder_reference_hit.hit { return 2 }
    if not torus_rotated_hit.hit or not torus_reference_hit.hit { return 3 }

    if abs(capsule_rotated_hit.local_position.x - capsule_reference_hit.local_position.x) > 0.02 { return 4 }
    if abs(capsule_rotated_hit.local_position.y - capsule_reference_hit.local_position.y) > 0.02 { return 5 }
    if abs(capsule_rotated_hit.local_position.z - capsule_reference_hit.local_position.z) > 0.02 { return 6 }
    if abs(capsule_rotated_hit.local_normal.x - capsule_reference_hit.local_normal.x) > 0.02 { return 7 }
    if abs(capsule_rotated_hit.local_normal.y - capsule_reference_hit.local_normal.y) > 0.02 { return 8 }
    if abs(capsule_rotated_hit.local_normal.z - capsule_reference_hit.local_normal.z) > 0.02 { return 9 }

    if abs(cylinder_rotated_hit.local_position.x - cylinder_reference_hit.local_position.x) > 0.02 { return 10 }
    if abs(cylinder_rotated_hit.local_position.y - cylinder_reference_hit.local_position.y) > 0.02 { return 11 }
    if abs(cylinder_rotated_hit.local_position.z - cylinder_reference_hit.local_position.z) > 0.02 { return 12 }
    if abs(cylinder_rotated_hit.local_normal.x - cylinder_reference_hit.local_normal.x) > 0.02 { return 13 }
    if abs(cylinder_rotated_hit.local_normal.y - cylinder_reference_hit.local_normal.y) > 0.02 { return 14 }
    if abs(cylinder_rotated_hit.local_normal.z - cylinder_reference_hit.local_normal.z) > 0.02 { return 15 }

    if abs(torus_rotated_hit.local_position.x - torus_reference_hit.local_position.x) > 0.02 { return 16 }
    if abs(torus_rotated_hit.local_position.y - torus_reference_hit.local_position.y) > 0.02 { return 17 }
    if abs(torus_rotated_hit.local_position.z - torus_reference_hit.local_position.z) > 0.02 { return 18 }
    if abs(torus_rotated_hit.local_normal.x - torus_reference_hit.local_normal.x) > 0.02 { return 19 }
    if abs(torus_rotated_hit.local_normal.y - torus_reference_hit.local_normal.y) > 0.02 { return 20 }
    if abs(torus_rotated_hit.local_normal.z - torus_reference_hit.local_normal.z) > 0.02 { return 21 }

    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase9_rotated_exact_primitives_match_reference_local_normals",
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
fn native_v2_phase9_virtual_gpu_trace_batch_preserves_identity_and_local_frame() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field conservative distance wrapped_batch_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        instance_array = Transform3(
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
            rotate = vec3(0.0, 1.5707963, 0.0) {
                sphere(radius = 0.5)
            }
        }
    }
}

material wrapped_batch_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.4, 0.5),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape wrapped_batch_shape {
    field = wrapped_batch_field
    material = wrapped_batch_surface
    payload = Payload(
        entity_id=u32(951),
        material_id=u32(951),
        actor=ActorHandle(id=u32(951), generation=u32(0))
    )
}

fn main() -> Integer {
    scene = capture wrapped_batch_shape
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
            origin=vec3(2.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
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

    if not cpu_hits[0].hit or not cpu_hits[1].hit { return 1 }
    if not vgpu_hits[0].hit or not vgpu_hits[1].hit { return 2 }
    if cpu_hits[0].instance_id != cpu_hits[1].instance_id { return 3 }
    if cpu_hits[0].repeat_id == cpu_hits[1].repeat_id { return 4 }
    if vgpu_hits[0].instance_id != cpu_hits[0].instance_id { return 5 }
    if vgpu_hits[1].instance_id != cpu_hits[1].instance_id { return 6 }
    if vgpu_hits[0].repeat_id != cpu_hits[0].repeat_id { return 7 }
    if vgpu_hits[1].repeat_id != cpu_hits[1].repeat_id { return 8 }
    if abs(vgpu_hits[0].local_position.x - cpu_hits[0].local_position.x) > 0.01 { return 9 }
    if abs(vgpu_hits[0].local_position.y - cpu_hits[0].local_position.y) > 0.01 { return 10 }
    if abs(vgpu_hits[0].local_position.z - cpu_hits[0].local_position.z) > 0.01 { return 11 }
    if abs(vgpu_hits[1].local_position.x - cpu_hits[1].local_position.x) > 0.01 { return 12 }
    if abs(vgpu_hits[1].local_position.y - cpu_hits[1].local_position.y) > 0.01 { return 13 }
    if abs(vgpu_hits[1].local_position.z - cpu_hits[1].local_position.z) > 0.01 { return 14 }
    if abs(vgpu_hits[0].local_normal.x - cpu_hits[0].local_normal.x) > 0.01 { return 15 }
    if abs(vgpu_hits[0].local_normal.y - cpu_hits[0].local_normal.y) > 0.01 { return 16 }
    if abs(vgpu_hits[0].local_normal.z - cpu_hits[0].local_normal.z) > 0.01 { return 17 }
    if abs(vgpu_hits[1].local_normal.x - cpu_hits[1].local_normal.x) > 0.01 { return 18 }
    if abs(vgpu_hits[1].local_normal.y - cpu_hits[1].local_normal.y) > 0.01 { return 19 }
    if abs(vgpu_hits[1].local_normal.z - cpu_hits[1].local_normal.z) > 0.01 { return 20 }
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase9_virtual_gpu_trace_batch_preserves_identity_and_local_frame",
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
fn native_v2_phase8_region_domain_render_world_queries_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance phase8_coarse_shell(p: Vec3) -> F32 {
    translate = vec3(3.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance phase8_fine_shell(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

radiance field phase8_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    horizon = clamp(0.5 + direction.y * 0.5, 0.0, 1.0)
    glow = clamp(0.25 - abs(p.z - 0.6), 0.0, 0.25) * 4.0
    feature = clamp(f32(feature_id), 0.0, 1.0)
    return vec3(0.08, 0.12, 0.20) * (1.0 - horizon)
        + vec3(0.20, 0.34, 0.72) * horizon
        + vec3(0.04, 0.02, 0.10) * glow * feature
}

volume field phase8_volume(p: Vec3, surface_distance: F32) -> Medium {
    density = clamp(0.04 + clamp(0.18 - abs(surface_distance), 0.0, 0.18) * 0.45, 0.0, 0.16)
    return Medium(
        density=density,
        emission=vec3(0.02, 0.03, 0.06) * density + vec3(abs(p.x) * 0.0, 0.0, 0.0),
        anisotropy=0.08
    )
}

material phase8_surface(hit: Hit3) -> Surface {
    ridge = clamp(abs(hit.local_position.y) * 0.6 + abs(hit.local_normal.x) * 0.2, 0.0, 1.0)
    return Surface(
        albedo=vec3(0.16, 0.24, 0.62) + vec3(0.08, 0.04, 0.10) * ridge,
        roughness=0.18 + ridge * 0.16,
        metalness=0.10 + clamp(hit.local_normal.z, 0.0, 1.0) * 0.12,
        clearcoat=0.12 + clamp(hit.local_normal.y, 0.0, 1.0) * 0.10,
        clearcoat_roughness=0.08 + abs(hit.local_position.x) * 0.10,
        sheen=0.06 + abs(hit.local_normal.x) * 0.10,
        emissive=vec3(0.02, 0.01, 0.04) * clamp(hit.local_normal.z, 0.0, 1.0)
    )
}

shape phase8_coarse_shape {
    field = phase8_coarse_shell
    material = phase8_surface
    payload = Payload(
        entity_id=u32(810),
        material_id=u32(810),
        actor=ActorHandle(id=u32(810), generation=u32(0))
    )
}

shape phase8_fine_shape {
    field = phase8_fine_shell
    material = phase8_surface
    radiance = phase8_radiance
    volume = phase8_volume
    payload = Payload(
        entity_id=u32(811),
        material_id=u32(811),
        actor=ActorHandle(id=u32(811), generation=u32(0))
    )
}

region phase8_scene_region() {
    place coarse = phase8_coarse_shape
    place fine = phase8_fine_shape
}

domain phase8_coarse_domain(world: RegionCapture) {
    geometry_detail = 0
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

domain phase8_fine_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

render phase8_render_ppm(world: RegionCapture, camera: Camera) {
    domain = phase8_fine_domain(world = world)
    light = Light(
        position = camera.position + vec3(1.5, 1.5, 1.5),
        direction = normalize(vec3(-0.6, -0.7, -0.5)),
        intensity = vec3(1.0, 0.95, 0.90),
        range = 10.0
    )
    width = 4
    height = 4
    world_up = camera.up
    view_scale = 0.82
    fill_dir = normalize(vec3(-0.4, 0.5, 0.2))
}

fn main() -> Integer {
    world = capture phase8_scene_region
    coarse_domain = phase8_coarse_domain(world = world)
    fine_domain = phase8_fine_domain(world = world)
    probe = vec3(0.0, 0.0, 0.6)

    coarse_distance = distance_world(capture = world, domain = coarse_domain, point = probe)
    fine_distance = distance_world(capture = world, domain = fine_domain, point = probe)
    fine_normal = normal_world(capture = world, domain = fine_domain, point = probe)
    coarse_hit = trace_world(
        capture = world,
        domain = coarse_domain,
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
    fine_hit = trace_world(
        capture = world,
        domain = fine_domain,
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96
    )
    coarse_surface = surface_world(capture = world, domain = coarse_domain, hit = fine_hit)
    fine_surface = surface_world(capture = world, domain = fine_domain, hit = fine_hit)
    coarse_radiance = radiance_world(
        capture = world,
        domain = coarse_domain,
        point = fine_hit.position,
        direction = normalize(vec3(0.0, 1.0, 1.0))
    )
    fine_radiance = radiance_world(
        capture = world,
        domain = fine_domain,
        point = fine_hit.position,
        direction = normalize(vec3(0.0, 1.0, 1.0))
    )
    coarse_medium = medium_world(capture = world, domain = coarse_domain, point = fine_hit.position)
    fine_medium = medium_world(capture = world, domain = fine_domain, point = fine_hit.position)
    camera = Camera(
        position = vec3(0.0, 0.0, 3.0),
        forward = vec3(0.0, 0.0, -1.0),
        up = vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees = 48.0
    )
    ppm = phase8_render_ppm(world = world, camera = camera)

    if coarse_domain.scene_id != world.scene_id { return 1 }
    if fine_domain.scene_id != world.scene_id { return 2 }
    if coarse_domain.geometry_detail != 0 { return 3 }
    if fine_domain.geometry_detail != 1 { return 4 }
    if coarse_distance <= 2.0 { return 5 }
    if abs(fine_distance) > 0.02 { return 6 }
    if fine_normal.z < 0.9 { return 7 }
    if coarse_hit.hit { return 8 }
    if not fine_hit.hit { return 9 }
    if coarse_surface.albedo.x != 0.0 or coarse_surface.albedo.y != 0.0 or coarse_surface.albedo.z != 0.0 { return 10 }
    if fine_surface.albedo.z <= fine_surface.albedo.x { return 11 }
    if coarse_radiance.z != 0.0 { return 12 }
    if fine_radiance.z <= fine_radiance.x { return 13 }
    if coarse_medium.density != 0.0 { return 14 }
    if fine_medium.density <= 0.0 { return 15 }
    if ppm == "" { return 16 }

    __wr_print(ppm)
    return 0
}
"#;

    let output = compile_and_run_native_inline_source(
        source,
        "wr_v2_phase8_region_domain_render_world_queries_smoke",
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("P3\n4 4\n255\n"),
        "expected compiler-owned render ppm prefix, got:\n{}\nstderr={}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_phase10_wgsl_world_queries_and_render_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material scene_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.20, 0.30, 0.65),
        roughness=0.22,
        metalness=0.08,
        clearcoat=0.10,
        clearcoat_roughness=0.08,
        sheen=0.04,
        emissive=vec3(0.01, 0.01, 0.02)
    )
}

radiance field scene_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    horizon = clamp(0.5 + direction.y * 0.5, 0.0, 1.0)
    return vec3(0.05, 0.08, 0.16) * (1.0 - horizon) + vec3(0.18, 0.30, 0.70) * horizon
}

volume field scene_medium(p: Vec3, surface_distance: F32) -> Medium {
    density = clamp(0.05 + clamp(0.2 - abs(surface_distance), 0.0, 0.2) * 0.4, 0.0, 0.14)
    return Medium(
        density=density,
        emission=vec3(0.02, 0.03, 0.05) * density,
        anisotropy=0.1
    )
}

shape scene_shape {
    field = scene_field
    material = scene_surface
    radiance = scene_radiance
    volume = scene_medium
    payload = Payload(
        entity_id=u32(7),
        material_id=u32(8),
        actor=ActorHandle(id=u32(9), generation=u32(0))
    )
}

region scene_region() {
    place scene = scene_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

render render_ppm(world: RegionCapture, camera: Camera) {
    domain = scene_domain(world = world)
    light = Light(
        position = camera.position + vec3(1.0, 1.25, 1.0),
        direction = normalize(vec3(-0.5, -0.8, -0.4)),
        intensity = vec3(1.0, 0.95, 0.90),
        range = 8.0
    )
    width = 4
    height = 4
    world_up = camera.up
    view_scale = 0.82
    fill_dir = normalize(vec3(-0.4, 0.5, 0.2))
}

fn main() -> Integer {
    world = capture scene_region
    domain = scene_domain(world = world)
    probe = vec3(0.0, 0.0, 0.6)

    cpu_distance = distance_world(
        capture=world,
        domain=domain,
        point=probe,
        backend=dispatch_backend_cpu()
    )
    auto_distance = distance_world(capture=world, domain=domain, point=probe)
    cpu_normal = normal_world(
        capture=world,
        domain=domain,
        point=probe,
        backend=dispatch_backend_cpu()
    )
    auto_normal = normal_world(capture=world, domain=domain, point=probe)
    cpu_hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96,
        backend=dispatch_backend_cpu()
    )
    auto_hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    cpu_surface = surface_world(
        capture=world,
        domain=domain,
        hit=cpu_hit,
        backend=dispatch_backend_cpu()
    )
    auto_surface = surface_world(capture=world, domain=domain, hit=auto_hit)
    cpu_radiance = radiance_world(
        capture=world,
        domain=domain,
        point=auto_hit.position,
        direction=normalize(vec3(0.0, 1.0, 1.0)),
        backend=dispatch_backend_cpu()
    )
    auto_radiance = radiance_world(
        capture=world,
        domain=domain,
        point=auto_hit.position,
        direction=normalize(vec3(0.0, 1.0, 1.0))
    )
    cpu_medium = medium_world(
        capture=world,
        domain=domain,
        point=auto_hit.position,
        backend=dispatch_backend_cpu()
    )
    auto_medium = medium_world(capture=world, domain=domain, point=auto_hit.position)
    camera = Camera(
        position=vec3(0.0, 0.0, 3.0),
        forward=vec3(0.0, 0.0, -1.0),
        up=vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees=48.0
    )
    ppm = render_ppm(world=world, camera=camera)

    if abs(cpu_distance - auto_distance) > 0.01 { return 1 }
    if abs(cpu_normal.z - auto_normal.z) > 0.01 { return 2 }
    if cpu_hit.hit != auto_hit.hit { return 3 }
    if abs(cpu_hit.distance - auto_hit.distance) > 0.01 { return 4 }
    if cpu_hit.feature_id != auto_hit.feature_id { return 5 }
    if abs(cpu_surface.albedo.z - auto_surface.albedo.z) > 0.01 { return 6 }
    if abs(cpu_radiance.z - auto_radiance.z) > 0.01 { return 7 }
    if abs(cpu_medium.density - auto_medium.density) > 0.01 { return 8 }
    if ppm == "" { return 9 }

    __wr_print(ppm)
    return 0
}
"#;

    let output = compile_and_run_native_inline_source_with_backend(
        source,
        "wr_v2_phase10_wgsl_world_queries_and_render_smoke",
        DispatchBackend::Wgsl,
    );
    let expected = expected_int_exit(0);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        expected,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("P3\n4 4\n255\n"),
        "expected WGSL world/render ppm prefix, got:\n{}\nstderr={}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_v2_phase10_wgsl_preview_project_sampled_queries_match_cpu() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    let run_source = r#"
fn fail(code: Integer) -> Integer {
    print_line(text="{code}")
    return code
}

fn approx_vec3(a: Vec3, b: Vec3, tolerance: F32) -> Boolean {
    if abs(a.x - b.x) > tolerance { return false }
    if abs(a.y - b.y) > tolerance { return false }
    if abs(a.z - b.z) > tolerance { return false }
    return true
}

fn probe(world: RegionCapture, domain: SceneDomain, direction: Vec3, code_base: Integer) -> Integer {
    cpu_hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.1, 2.7),
        direction=direction,
        max_distance=12.0,
        min_step=0.02,
        hit_epsilon=0.0008,
        max_steps=96,
        backend=dispatch_backend_cpu()
    )
    auto_hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.1, 2.7),
        direction=direction,
        max_distance=12.0,
        min_step=0.02,
        hit_epsilon=0.0008,
        max_steps=96
    )
    if cpu_hit.hit != auto_hit.hit { return fail(code=code_base + 1) }
    if abs(cpu_hit.distance - auto_hit.distance) > 0.01 { return code_base + 2 }
    if approx_vec3(a=cpu_hit.position, b=auto_hit.position, tolerance=0.01) == false { return code_base + 3 }
    if approx_vec3(a=cpu_hit.normal, b=auto_hit.normal, tolerance=0.01) == false { return code_base + 4 }
    if cpu_hit.feature_id != auto_hit.feature_id { return code_base + 5 }
    if cpu_hit.root_shape_id != auto_hit.root_shape_id { return code_base + 6 }

    if cpu_hit.hit == false {
        miss_point = vec3(0.0, 0.1, 2.7) + direction * 4.0
        cpu_radiance = radiance_world(
            capture=world,
            domain=domain,
            point=miss_point,
            direction=direction,
            backend=dispatch_backend_cpu()
        )
        auto_radiance = radiance_world(
            capture=world,
            domain=domain,
            point=miss_point,
            direction=direction
        )
        if approx_vec3(a=cpu_radiance, b=auto_radiance, tolerance=0.01) == false { return code_base + 7 }
        cpu_medium = medium_world(
            capture=world,
            domain=domain,
            point=miss_point,
            backend=dispatch_backend_cpu()
        )
        auto_medium = medium_world(capture=world, domain=domain, point=miss_point)
        if abs(cpu_medium.density - auto_medium.density) > 0.01 { return code_base + 8 }
        if approx_vec3(a=cpu_medium.emission, b=auto_medium.emission, tolerance=0.01) == false { return code_base + 9 }
        return 0
    }

    cpu_surface = surface_world(
        capture=world,
        domain=domain,
        hit=cpu_hit,
        backend=dispatch_backend_cpu()
    )
    auto_surface = surface_world(capture=world, domain=domain, hit=auto_hit)
    if approx_vec3(a=cpu_surface.albedo, b=auto_surface.albedo, tolerance=0.01) == false { return code_base + 10 }
    if abs(cpu_surface.roughness - auto_surface.roughness) > 0.01 { return code_base + 11 }
    if abs(cpu_surface.metalness - auto_surface.metalness) > 0.01 { return code_base + 12 }
    if approx_vec3(a=cpu_surface.emissive, b=auto_surface.emissive, tolerance=0.01) == false { return code_base + 13 }

    cpu_radiance = radiance_world(
        capture=world,
        domain=domain,
        point=cpu_hit.position,
        direction=direction,
        backend=dispatch_backend_cpu()
    )
    auto_radiance = radiance_world(
        capture=world,
        domain=domain,
        point=cpu_hit.position,
        direction=direction
    )
    if approx_vec3(a=cpu_radiance, b=auto_radiance, tolerance=0.01) == false { return code_base + 14 }

    cpu_medium = medium_world(
        capture=world,
        domain=domain,
        point=cpu_hit.position,
        backend=dispatch_backend_cpu()
    )
    auto_medium = medium_world(capture=world, domain=domain, point=cpu_hit.position)
    if abs(cpu_medium.density - auto_medium.density) > 0.01 { return code_base + 15 }
    if approx_vec3(a=cpu_medium.emission, b=auto_medium.emission, tolerance=0.01) == false { return code_base + 16 }

    ao_a_point = cpu_hit.position + cpu_hit.normal * 0.06
    ao_b_point = cpu_hit.position + cpu_hit.normal * 0.14
    ao_c_point = cpu_hit.position + cpu_hit.normal * 0.28
    cpu_ao_a = distance_world(
        capture=world,
        domain=domain,
        point=ao_a_point,
        backend=dispatch_backend_cpu()
    )
    auto_ao_a = distance_world(capture=world, domain=domain, point=ao_a_point)
    if abs(cpu_ao_a - auto_ao_a) > 0.01 { return code_base + 17 }
    cpu_ao_b = distance_world(
        capture=world,
        domain=domain,
        point=ao_b_point,
        backend=dispatch_backend_cpu()
    )
    auto_ao_b = distance_world(capture=world, domain=domain, point=ao_b_point)
    if abs(cpu_ao_b - auto_ao_b) > 0.01 { return code_base + 18 }
    cpu_ao_c = distance_world(
        capture=world,
        domain=domain,
        point=ao_c_point,
        backend=dispatch_backend_cpu()
    )
    auto_ao_c = distance_world(capture=world, domain=domain, point=ao_c_point)
    if abs(cpu_ao_c - auto_ao_c) > 0.01 { return code_base + 19 }

    shadow_origin = cpu_hit.position + cpu_hit.normal * 0.01
    light_delta = vec3(2.4, 2.8, 2.4) - shadow_origin
    shadow_direction = normalize(light_delta)
    shadow_limit = min(length(light_delta), 12.0)
    cpu_shadow = trace_world(
        capture=world,
        domain=domain,
        origin=shadow_origin,
        direction=shadow_direction,
        max_distance=shadow_limit,
        min_step=0.02,
        hit_epsilon=0.0008,
        max_steps=96,
        backend=dispatch_backend_cpu()
    )
    auto_shadow = trace_world(
        capture=world,
        domain=domain,
        origin=shadow_origin,
        direction=shadow_direction,
        max_distance=shadow_limit,
        min_step=0.02,
        hit_epsilon=0.0008,
        max_steps=96
    )
    if cpu_shadow.hit != auto_shadow.hit { return code_base + 20 }
    if abs(cpu_shadow.distance - auto_shadow.distance) > 0.01 { return code_base + 21 }
    return 0
}

fn run() -> Integer {
    world = capture scene_region
    domain = scene_domain(world=world)

    probe_a = probe(
        world=world,
        domain=domain,
        direction=vec3(-0.405183, -0.375170, -0.833711),
        code_base=100
    )
    if probe_a != 0 { return probe_a }

    probe_b = probe(
        world=world,
        domain=domain,
        direction=vec3(-0.379642, -0.379642, -0.843649),
        code_base=200
    )
    if probe_b != 0 { return probe_b }

    probe_c = probe(
        world=world,
        domain=domain,
        direction=vec3(0.017994, -0.017994, -0.999676),
        code_base=300
    )
    if probe_c != 0 { return probe_c }

    probe_d = probe(
        world=world,
        domain=domain,
        direction=vec3(-0.498185, 0.498185, -0.709665),
        code_base=400
    )
    if probe_d != 0 { return probe_d }

    return 0
}
"#;

    let output = compile_and_run_native_project_with_replaced_run(
        "language/preview",
        run_source,
        "wr_v2_phase10_wgsl_preview_project_sampled_queries_match_cpu",
        DispatchBackend::Wgsl,
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
