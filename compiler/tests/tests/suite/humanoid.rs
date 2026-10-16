//! The humanoid bake-off's rig, motion and figure regressions on small CPU fixtures.

use wrela_tests::repo_root;
/// The package's own tests (`wrela test examples/humanoid`: the rig's, the shared motion
/// layer's on both sides, the capture's stride, the mannequin's), on the CPU, so the gate runs
/// them.
#[test]
fn the_bake_offs_own_tests_pass() {
    assert_eq!(super::tests_pass(&repo_root().join("examples/humanoid")), 8);
}
