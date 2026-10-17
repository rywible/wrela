//! The physical body's regressions: it walks, turns, runs and stops under its trained policy.

use wrela_tests::repo_root;

/// The package's own tests (`wrela test examples/physical`), on the CPU: each one its files
/// declare runs and passes (counted from the files).
#[test]
fn the_physical_bodys_own_tests_pass() {
    let dir = repo_root().join("examples/physical");
    let declared: usize = std::fs::read_dir(&dir)
        .expect("examples/physical/")
        .map(|e| e.expect("an entry of examples/physical/").path())
        .filter(|p| p.extension().is_some_and(|x| x == "wrela"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).expect("a physical file");
            text.lines().filter(|l| l.trim_start().starts_with("@test")).count()
        })
        .sum();
    assert!(declared > 0, "examples/physical/ declares no tests");
    assert_eq!(super::tests_pass(&dir), declared);
}
