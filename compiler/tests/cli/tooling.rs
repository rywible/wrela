use super::*;

#[test]
fn cli_thin_core_bootstrap_matrix() {
    let dir = workspace_tempdir();
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();

    write_fixture_file(
        src.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    let entry = src.join("main.wr");
    write_fixture_file(
        tests.join("basic_test.wr"),
        r#"fn test_basic() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run check");
    assert!(
        check.status.success(),
        "check failed: code={:?}\nstdout={}\nstderr={}",
        check.status.code(),
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let bin = dir.path().join("thin_core_matrix_bin");
    let build = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(bin.as_os_str())
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "build failed: {:?}",
        build.status.code()
    );
    assert!(bin.exists());

    let run_status = Command::new(&bin).status().expect("run built binary");
    assert_eq!(run_status.code(), Some(0));

    let test = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run test");
    assert!(
        test.status.success(),
        "test failed: {:?}",
        test.status.code()
    );
}

#[test]
fn cli_naming_is_warning_by_default() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn helper() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with a verb"));
    assert!(stderr.contains("warning"));
}

#[test]
fn cli_strict_naming_promotes_to_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn helper() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--strict-naming")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with a verb"));
}

#[test]
fn cli_fix_rewrites_safe_naming_issue() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn helperThing() -> Integer {
    return 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(rewritten.contains("helper_thing"));
}

#[test]
fn cli_fix_json_emits_summary_counts() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn helperThing() -> Integer {
    return 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("skipped")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("errors")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("touched_files")),
        Some(&serde_json::json!(1))
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(rewritten.contains("helper_thing"));
}

#[test]
fn cli_fix_json_emits_zero_summary_when_no_safe_fixes_found() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3), "expected no-fix exit code");

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("skipped")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("errors")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("touched_files")),
        Some(&serde_json::json!(0))
    );
}

#[test]
fn cli_fix_rewrites_safe_try_operator_issue() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("return 1\n"),
        "expected ? removal: {rewritten}"
    );
    assert!(!rewritten.contains('?'), "expected ? removal: {rewritten}");
}

#[test]
fn cli_fix_json_counts_safe_try_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_rewrites_single_candidate_typed_hole() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("return value\n"),
        "expected typed hole fill: {rewritten}"
    );
    assert!(
        !rewritten.contains("_todo"),
        "expected typed hole fill: {rewritten}"
    );
}

#[test]
fn cli_fix_json_counts_safe_single_candidate_typed_hole_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_allow_review_fixes_applies_review_tier_hole_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_named_args_required_call() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add(a=1, b=2)"),
        "expected named args rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_rewrites_safe_named_args_required_without_review_flag() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add(a=1, b=2)"),
        "expected named args rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_rewrites_legacy_given_call_to_function_call_syntax() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn run() -> Integer {
    is_ok = is_positive given 3
    if is_ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_rewrites_given_call_in_return_without_whitespace_loss() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_one(value: Integer) -> Integer {
    return value + 1

}
fn run() -> Integer {
    return add_one given 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_json_counts_given_call_rewrite_as_safe_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn run() -> Integer {
    is_ok = is_positive given value=3
    if is_ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"code\":\"lang::parse::syntax_error\""),
        "expected parse syntax error payload: {stdout}"
    );
}

#[test]
fn cli_fix_prefers_named_arg_rewrite_over_given_style_for_multi_positional_calls() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn in_range(value: Integer, limit: Integer) -> Boolean {
    return value < limit

}
fn run() -> Integer {
    ok = in_range given 1, 10
    if ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_rewrites_result_otherwise_to_or_else() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn run() -> Integer {
    return try_to_parse_number("1") ?? 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3), "expected no-op fix result");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no safe non-overlapping fixes found"),
        "expected no-op fix message: {stderr}"
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_boundary_generic_type() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(values: List) -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected boundary generic rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_boundary_map_generic_type() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(meta: Map) -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("Map[String, Integer]"),
        "expected boundary map rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_applies_rewrites_and_emits_summary_json_smoke() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(values: List) -> Integer {
    total = add_values(1, 10)
    return total
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    let applied = summary
        .get("summary")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        applied >= 2,
        "expected at least two rewrites, got {applied}"
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected List[Integer]: {rewritten}"
    );
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical call rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_rewrites_legacy_result_fallback_syntax() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn run() -> Integer {
    return try_to_parse_number("1") ?? 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("?? 0"),
        "expected canonical result fallback operator: {rewritten}"
    );
    assert!(
        !rewritten.contains(" otherwise "),
        "expected legacy fallback operator to be rewritten: {rewritten}"
    );
}

#[test]
fn cli_fmt_directory_sweeps_src_and_tests_files() {
    let dir = workspace_tempdir();
    let src_main = dir.path().join("src").join("main.wr");
    let test_file = dir.path().join("tests").join("sample_test.wr");
    std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
    std::fs::create_dir_all(test_file.parent().expect("test parent")).expect("create tests");
    write_fixture_file(
        &src_main,
        r#"fn add_one(value: Integer) -> Integer {
    return value + 1

}
fn run() -> Integer {
    return add_one(value=1)
}
"#,
    )
    .expect("write src");
    write_fixture_file(
        &test_file,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn test_sample() -> Nothing {
    value = try_to_parse_number("1") ?? 0
    assert value value == 0
}
"#,
    )
    .expect("write test");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count == 0),
        "expected no failures in fmt summary: {summary}"
    );
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("targets_scanned"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 2),
        "expected at least two scanned targets in fmt summary: {summary}"
    );

    let rewritten_src = std::fs::read_to_string(&src_main).expect("read src");
    assert!(
        rewritten_src.contains("add_one(value=1)"),
        "expected src call rewrite: {rewritten_src}"
    );
    let rewritten_test = std::fs::read_to_string(&test_file).expect("read test");
    assert!(
        rewritten_test.contains("?? 0"),
        "expected test fallback rewrite: {rewritten_test}"
    );
}

#[test]
fn cli_fmt_converges_multi_arg_given_call_to_canonical_named_call() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run() -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical named call syntax: {rewritten}"
    );
    assert!(
        !rewritten.contains(" given "),
        "expected no legacy given call syntax after fmt: {rewritten}"
    );
}

#[test]
fn cli_fmt_second_run_is_zero_diff_smoke() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(meta: Map) -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run first fmt");
    assert!(
        first.status.success(),
        "first fmt failed: code={:?}\nstdout={}\nstderr={}",
        first.status.code(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run second fmt");
    assert!(
        second.status.success(),
        "second fmt failed: code={:?}\nstdout={}\nstderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&second.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("applied"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero applied rewrites on second fmt run: {summary}"
    );
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("touched_files"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero touched files on second fmt run: {summary}"
    );
}

#[test]
fn cli_fmt_second_run_is_zero_diff() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(meta: Map) -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run first fmt");
    assert!(
        first.status.success(),
        "first fmt failed: code={:?}\nstdout={}\nstderr={}",
        first.status.code(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run second fmt");
    assert!(
        second.status.success(),
        "second fmt failed: code={:?}\nstdout={}\nstderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&second.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("applied"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero applied rewrites on second fmt run: {summary}"
    );
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("touched_files"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero touched files on second fmt run: {summary}"
    );
}

#[test]
fn cli_fmt_applies_rewrites_and_emits_summary_json() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(values: List) -> Integer {
    total = add_values(1, 10)
    return total
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    let applied = summary
        .get("summary")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(applied >= 2, "expected >=2 rewrites, got {applied}");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected no failed targets: {summary}"
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected boundary generic rewrite: {rewritten}"
    );
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical call rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_directory_continues_after_file_failure_and_reports_summary() {
    let dir = workspace_tempdir();
    let src_main = dir.path().join("src").join("main.wr");
    let broken_test = dir.path().join("tests").join("broken_test.wr");
    std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
    std::fs::create_dir_all(broken_test.parent().expect("tests parent")).expect("create tests");
    write_fixture_file(
        &src_main,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run() -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write src");
    write_fixture_file(
        &broken_test,
        r#"fn test_broken() -> Nothing {
    value = 1 +
    return
}
"#,
    )
    .expect("write broken");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "expected non-zero exit due to broken target"
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 1),
        "expected failed target count in fmt summary: {summary}"
    );
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("targets_scanned"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 2),
        "expected scanned target count in fmt summary: {summary}"
    );

    let rewritten_src = std::fs::read_to_string(&src_main).expect("read src");
    assert!(
        rewritten_src.contains("add_values(value=1, extra=10)"),
        "expected successful rewrites on healthy target despite one failure: {rewritten_src}"
    );
}

#[test]
fn cli_naming_bypass_allows_main_and_configure() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"class Logger {
    fn __configure__() -> Nothing {
        return

    }
}
fn main() -> Integer {
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path, body: &str) {
    write_fixture_file(path, body).expect("write script");
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

#[cfg(unix)]
fn setup_matrix_stubs(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let cargo_stub = root.join("cargo-stub.sh");
    let wrlea_stub = root.join("wrela-stub.sh");
    write_executable(
        &cargo_stub,
        r#"#!/bin/sh
set -eu
echo "cargo:$*" >> "$WRELA_MATRIX_STUB_LOG"
if [ "${WRELA_MATRIX_FAIL_STEP:-}" = "cargo" ]; then
  exit 9
fi
exit 0
"#,
    );
    write_executable(
        &wrlea_stub,
        r#"#!/bin/sh
set -eu
echo "wrela:$*" >> "$WRELA_MATRIX_STUB_LOG"
cmd="${1:-}"
if [ "${WRELA_MATRIX_FAIL_STEP:-}" = "$cmd" ]; then
  exit 7
fi
if [ "$cmd" = "perf" ]; then
  baseline=""
  for arg in "$@"; do
    case "$arg" in
      --baseline-out=*)
        baseline="${arg#--baseline-out=}"
        ;;
    esac
  done
  if [ -n "$baseline" ]; then
    mkdir -p "$(dirname "$baseline")"
    printf '{"sample_count":1,"compile_throughput_tests_per_sec":1.0,"runtime_p50_ns":1,"runtime_p95_ns":1,"runtime_p99_ns":1,"allocs_per_request":0.0,"rc_inc":0,"rc_dec":0,"rc_ops_total":0,"dispatch_hit_ratio":1.0,"check_fallback_rate":0.1,"avg_check_batch_size":8.0,"check_oracle_eval_ns_p50":50,"check_oracle_eval_ns_p95":90,"effect_annihilation_rewrite_count":2,"scheduler_dispatch_p99_ns":1000,"scheduler_starvation_violations":0,"rewrite_compile_overhead_pct":3.0,"rewrite_applied_count":12,"metrics":{"messages_sent":0,"messages_dropped":0,"pending_resolved":0,"pending_dropped":0,"mailbox_high_water":0,"rc_inc":0,"rc_dec":0,"alloc_list":0,"alloc_map":0,"alloc_string":0,"alloc_bytes":0,"alloc_result":0,"alloc_pending":0,"mailbox_enqueue_ok":0,"mailbox_enqueue_fail":0,"mailbox_dequeue":0,"sched_dispatched":0,"sched_skipped_no_credit":0,"sched_profile_switch":0,"sched_starvation_violation":0,"sched_cross_shard_migration":0,"abi_typed_lane":0,"abi_boxed_lane":0}}' > "$baseline"
  fi
fi
exit 0
"#,
    );
    (cargo_stub, wrlea_stub)
}

#[cfg(unix)]
#[test]
fn cli_matrix_writes_evidence_bundle() {
    let dir = workspace_tempdir();
    let log_path = dir.path().join("matrix-stub.log");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .output()
        .expect("run matrix");
    assert!(
        output.status.success(),
        "matrix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    assert!(latest.exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle json");
    assert_eq!(json.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(json.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(
        json.get("steps")
            .and_then(|v| v.as_array())
            .map(|steps| steps.len()),
        Some(3)
    );
    assert!(
        json.get("perf_summary")
            .and_then(|v| v.as_object())
            .is_some()
    );
    assert!(
        json.get("check_lane_kpis")
            .and_then(|v| v.as_object())
            .is_some()
    );
    let baseline = json
        .get("perf_baseline_path")
        .and_then(|v| v.as_str())
        .expect("baseline path");
    assert!(std::path::Path::new(baseline).exists());

    let invocations = std::fs::read_to_string(log_path).expect("read invocation log");
    assert!(invocations.contains("cargo:test --workspace"));
    assert!(invocations.contains("wrela:test language/spec --lane=fast"));
    assert!(invocations.contains("wrela:perf --runs=1"));
}

#[cfg(unix)]
#[test]
fn cli_matrix_forwards_perf_gate_flags() {
    let dir = workspace_tempdir();
    let log_path = dir.path().join("matrix-stub.log");
    let gate = dir.path().join("gate-baseline.json");
    write_fixture_file(&gate, r#"{}"#).expect("write gate");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .arg(format!("--perf-gate={}", gate.display()))
        .arg("--perf-max-regression-pct=12.5")
        .arg("--kpi-check-fallback-max=0.20")
        .arg("--kpi-check-batch-min=6")
        .arg("--kpi-scheduler-p99-improve-min-pct=10")
        .arg("--kpi-rewrite-overhead-max-pct=5")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .output()
        .expect("run matrix");
    assert!(output.status.success());
    let invocations = std::fs::read_to_string(log_path).expect("read invocation log");
    assert!(invocations.contains(&format!("--perf-gate={}", gate.display())));
    assert!(invocations.contains("--perf-max-regression-pct=12.5"));
    assert!(invocations.contains("--kpi-check-fallback-max=0.2"));
    assert!(invocations.contains("--kpi-check-batch-min=6"));
    assert!(invocations.contains("--kpi-scheduler-p99-improve-min-pct=10"));
    assert!(invocations.contains("--kpi-rewrite-overhead-max-pct=5"));

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle");
    assert_eq!(
        json.get("kpi_thresholds")
            .and_then(|v| v.get("check_fallback_max"))
            .and_then(|v| v.as_f64()),
        Some(0.2)
    );
}

#[cfg(unix)]
#[test]
fn cli_matrix_stops_on_failed_step_and_persists_evidence() {
    let dir = workspace_tempdir();
    let log_path = dir.path().join("matrix-stub.log");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .env("WRELA_MATRIX_FAIL_STEP", "test")
        .output()
        .expect("run matrix");
    assert!(!output.status.success());

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    assert!(latest.exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle json");
    assert_eq!(json.get("success").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        json.get("steps")
            .and_then(|v| v.as_array())
            .map(|steps| steps.len()),
        Some(2)
    );
}

#[test]
fn benchmark_manifest_scenarios_resolve_via_discovery() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let manifests = [
        "benchmarks/micro/bench.toml",
        "benchmarks/field_engine/bench.toml",
        "benchmarks/realtime_presentation/bench.toml",
        "benchmarks/collision_perf/bench.toml",
        "benchmarks/collision_perf/1080p120_closure.toml",
        "benchmarks/engine_frame/bench.toml",
        "benchmarks/engine_frame/1080p120_closure.toml",
        "benchmarks/whole_frame/bench.toml",
        "benchmarks/whole_frame/1080p120_closure.toml",
    ];

    for manifest_rel in manifests {
        let manifest_path = repo_root.join(manifest_rel);
        let bench_root = manifest_path.parent().expect("benchmark root");
        let raw_manifest =
            std::fs::read_to_string(&manifest_path).expect("read benchmark manifest text");
        let manifest: toml::Value =
            toml::from_str(&raw_manifest).expect("parse benchmark manifest");
        let scenarios = manifest
            .get("scenarios")
            .and_then(|value| value.as_array())
            .expect("manifest scenarios array");

        let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
            .arg("test")
            .arg(bench_root)
            .arg("--list")
            .arg("--error-format=json")
            .output()
            .expect("run wrela test --list");
        assert!(
            output.status.success(),
            "failed to list tests for {}:\nstdout:\n{}\nstderr:\n{}",
            manifest_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let payload: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid test list json");
        let discovered: HashSet<String> = payload
            .get("tests")
            .and_then(|value| value.as_array())
            .expect("test list array")
            .iter()
            .filter_map(|entry| {
                entry
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|name| name.to_string())
            })
            .collect();

        for scenario in scenarios {
            let test_name = scenario
                .get("test_name")
                .and_then(|value| value.as_str())
                .expect("scenario test_name");
            let scenario_id = scenario
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("<missing-id>");
            assert!(
                discovered.contains(test_name),
                "manifest scenario `{}` in {} references unknown discovery test `{}`",
                scenario_id,
                manifest_path.display(),
                test_name
            );
            if manifest_rel == "benchmarks/realtime_presentation/bench.toml" {
                let presentation = scenario
                    .get("presentation")
                    .and_then(|value| value.as_table())
                    .expect("realtime presentation metadata");
                assert!(
                    presentation
                        .get("entry")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("view")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("region")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("width")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
                assert!(
                    presentation
                        .get("height")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
                assert!(
                    presentation
                        .get("frames")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
            } else if manifest_rel.starts_with("benchmarks/collision_perf/") {
                let collision = scenario
                    .get("collision")
                    .and_then(|value| value.as_table())
                    .expect("collision perf metadata");
                assert!(
                    collision
                        .get("entry")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    collision
                        .get("region")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    collision
                        .get("domain")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    collision
                        .get("workload")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
            } else if manifest_rel.starts_with("benchmarks/whole_frame/")
                || manifest_rel.starts_with("benchmarks/engine_frame/")
            {
                let presentation = scenario
                    .get("presentation")
                    .and_then(|value| value.as_table())
                    .expect("whole frame presentation metadata");
                let collision = scenario
                    .get("collision")
                    .and_then(|value| value.as_table())
                    .expect("whole frame collision metadata");
                assert!(
                    presentation
                        .get("entry")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("view")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    collision
                        .get("entry")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    collision
                        .get("workload")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
            }
        }
    }
}
