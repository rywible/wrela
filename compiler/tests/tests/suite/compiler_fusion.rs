//! A bounded dispatch-fusion prototype preserves every output on a hardware GPU.
//! The generator is an experiment, not a production compiler optimization.

use std::process::Command;
use wrela_host::Host;

#[test]
#[ignore = "needs a GPU"]
fn generated_dispatch_fusion_preserves_wrapping_results_and_frame_reset() {
    for stages in [3, 32] {
        check_fusion(stages);
    }
}

fn check_fusion(stages: u32) {
    const ELEMENTS: u32 = 129;
    let root = crate::scratch(&format!("compiler-fusion-{stages}"));
    let run = Command::new("python3")
        .arg(wrela_tests::repo_root().join("tools/compiler_fusion.py"))
        .arg("--output")
        .arg(&root)
        .args(["--elements", &ELEMENTS.to_string(), "--stages", &stages.to_string()])
        .output()
        .expect("run the fusion generator");
    assert!(run.status.success(), "generator: {}", String::from_utf8_lossy(&run.stderr));
    let expected: Vec<u8> = (0..ELEMENTS)
        .flat_map(|index| {
            let mut value = index.wrapping_mul(747796405) ^ (index >> 4) ^ 2891336453;
            for stage in 0..stages {
                value = value
                    .wrapping_mul(1664525 + stage * 2)
                    .wrapping_add(stage.wrapping_mul(1013904223).wrapping_add(17));
                value ^= value >> (stage % 15 + 1);
            }
            value.to_le_bytes()
        })
        .collect();
    assert_eq!(std::fs::read(root.join("expected.u32le")).expect("generator oracle"), expected);
    for variant in ["before", "after"] {
        let package = root.join(variant);
        let build = package.join("build");
        wrela_tests::must_build(&package, &build);
        let mut host = Host::load(&build).expect("hardware GPU host");
        for time in [0.0, 1.0 / 60.0] {
            host.run_frames(&[time], 16, 16).expect("run one frame");
            let buffers = host.buffers();
            assert_eq!(buffers.len(), 2, "{variant}: both owned buffers remain alive");
            let result =
                host.read_buffer(buffers[((stages - 1) % 2) as usize]).expect("read result");
            assert_eq!(result, expected, "{variant}, {stages} stages, at time {time}");
        }
    }
}
