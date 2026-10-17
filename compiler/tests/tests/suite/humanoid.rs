//! The humanoid's rig, motion, body, blade and duel regressions on small CPU fixtures.

use wrela_tests::repo_root;
/// The package's own tests (`wrela test examples/humanoid`: the rig's, the motion layer's, the
/// body's and its solver's, the blade's, the duel's, the mannequin's), on the CPU: each one its
/// files declare runs and passes (counted from the files).
#[test]
fn the_humanoids_own_tests_pass() {
    let dir = repo_root().join("examples/humanoid");
    let declared: usize = std::fs::read_dir(&dir)
        .expect("examples/humanoid/")
        .map(|e| e.expect("an entry of examples/humanoid/").path())
        .filter(|p| p.extension().is_some_and(|x| x == "wrela"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).expect("a humanoid file");
            text.lines().filter(|l| l.trim_start().starts_with("@test")).count()
        })
        .sum();
    assert!(declared > 0, "examples/humanoid/ declares no tests");
    assert_eq!(super::tests_pass(&dir), declared);
}
