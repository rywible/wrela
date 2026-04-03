use std::fs;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::hir::project::load_project;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::pir;

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

fn lower_pir_inline(source: &str, entry: &str) -> pir::PirModule {
    let module = lower_inline_module_from_source(source);
    lower_pir_module(module, entry)
}

fn lower_pir_module(module: hir::Module, entry: &str) -> pir::PirModule {
    let semantic = hir::semantic::check_module(&module);
    assert!(semantic.errors.is_empty(), "semantic errors: {:?}", semantic.errors);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    pir::lower_portable_entry_by_name(&module, &type_info, entry)
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"))
}

fn lower_pir_module_result(
    module: hir::Module,
    entry: &str,
) -> Result<pir::PirModule, Vec<pir::PirLowerError>> {
    let semantic = hir::semantic::check_module(&module);
    assert!(semantic.errors.is_empty(), "semantic errors: {:?}", semantic.errors);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    pir::lower_portable_entry_by_name(&module, &type_info, entry)
}

#[test]
fn lowers_only_reachable_portable_functions_for_entry() {
    let source = r#"
kernel fn helper(seed: I32) -> I32 {
    return seed + i32(1)
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed) * i32(2)
}

fn unused() -> I32 {
    return i32(99)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let mut names = module
        .functions
        .iter()
        .map(|function| function.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["helper".to_string(), "portable_entry".to_string()]);
}

#[test]
fn executes_scalar_kernel_entry_on_cpu() {
    let source = r#"
kernel fn helper(seed: I32) -> I32 {
    pair = Pair(x=seed, y=i32(5))
    return pair.x + pair.y
}

value Pair {
    x: I32
    y: I32
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed) * i32(2)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(22));
}

#[test]
fn executes_arrays_and_value_structs_on_cpu() {
    let source = r#"
value Pair {
    left: I32
    right: I32
}

kernel fn sum(values: Array[I32, 3]) -> I32 {
    return values[0] + values[1] + values[2]
}

kernel fn portable_entry(values: Array[I32, 3]) -> I32 {
    pair = Pair(left=i32(4), right=i32(6))
    return pair.left + pair.right + sum(values=values)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(
        &module,
        vec![pir::PirValue::Array(vec![
            pir::PirValue::I32(1),
            pir::PirValue::I32(2),
            pir::PirValue::I32(3),
        ])],
    )
    .expect("execute");
    assert_eq!(result, pir::PirValue::I32(16));
}

#[test]
fn reuses_runtime_vec_math_for_cpu_truth_execution() {
    let source = r#"
kernel fn portable_entry() -> F32 {
    direction = normalize(vec3(3.0, 0.0, 4.0))
    return dot(direction, vec3(0.6, 0.0, 0.8))
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    match result {
        pir::PirValue::F32(value) => assert!((value - 1.0).abs() < 0.0001, "value={value}"),
        other => panic!("expected F32 result, got {other:?}"),
    }
}

#[test]
fn lowers_project_module_through_portable_path_without_mir() {
    let source = r#"
value Pair {
    x: I32
    y: I32
}

kernel fn helper(pair: Pair) -> I32 {
    return pair.x + pair.y
}

kernel fn portable_entry(seed: I32) -> I32 {
    pair = Pair(x=seed, y=i32(4))
    return helper(pair=pair)
}

fn run() -> I32 {
    return portable_entry(seed=i32(8))
}
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(semantic.errors.is_empty(), "semantic errors: {:?}", semantic.errors);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let pir = pir::lower_portable_entry_by_name(&module, &type_info, "portable_entry")
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"));
    let result = pir::execute_entry(&pir, vec![pir::PirValue::I32(8)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(12));
}

#[test]
fn rejects_host_function_as_portable_entry() {
    let source = r#"
fn portable_entry(seed: I32) -> I32 {
    return seed + i32(1)
}
"#;

    let module = lower_inline_module_from_source(source);
    let errors = lower_pir_module_result(module, "portable_entry")
        .expect_err("host-lane entry should be rejected");
    assert_eq!(
        errors,
        vec![pir::PirLowerError::EntryNotPortable {
            name: "portable_entry".into(),
        }]
    );
}

#[test]
fn lowers_top_level_kernel_entry_even_when_method_shares_name() {
    let source = r#"
class Shadow {
    fn portable_entry(seed: I32) -> I32 {
        return seed + i32(99)
    }
}

kernel fn portable_entry(seed: I32) -> I32 {
    return seed + i32(1)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(7));
}

#[test]
fn lowers_top_level_kernel_helper_even_when_method_shares_name() {
    let source = r#"
class Shadow {
    fn helper(seed: I32) -> I32 {
        return seed + i32(99)
    }
}

kernel fn helper(seed: I32) -> I32 {
    return seed + i32(1)
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(7));
}

#[test]
fn prefers_top_level_portable_declarations_over_methods() {
    let source = r#"
class Shadow {
    private {
        fn portable_entry() -> I32 {
            return i32(100)
        }

        fn helper() -> I32 {
            return i32(200)
        }
    }
}

kernel fn helper() -> I32 {
    return i32(1)
}

kernel fn portable_entry() -> I32 {
    return helper()
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(semantic.errors.is_empty(), "semantic errors: {:?}", semantic.errors);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let pir = pir::lower_portable_entry_by_name(&module, &type_info, "portable_entry")
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"));
    let result = pir::execute_entry(&pir, Vec::new()).expect("execute");
    assert_eq!(result, pir::PirValue::I32(1));
}
