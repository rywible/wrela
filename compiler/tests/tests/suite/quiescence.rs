//! A closed loop guard preserves exact GPU buffers, including divergent lanes.

use wrela_host::Host;

#[test]
#[ignore = "needs a GPU"]
fn closed_guard_keeps_exact_buffers_and_adds_an_exit() {
    let (dir, _) = wrela_tests::page("compiler/tests/quiescence", "quiescence");
    let manifest = wrela_tests::manifest(&dir);
    let pipeline = manifest.pipelines.iter().find(|p| p.name == "check").expect("check kernel");
    let shader = std::fs::read_to_string(dir.join(&pipeline.shader)).expect("shader");
    assert_eq!(shader.matches("loop {").count(), 2, "two dynamic counted loops");
    assert_eq!(shader.matches("break;").count(), 3, "the outer loop gains a closed-guard exit");
    let mut host = Host::load(&dir).expect("hardware GPU host");
    for bound in [0, 1, 32] {
        host.frame(bound as f32, 16, 16).expect("run bounded loops");
        let buffers = host.buffers();
        assert_eq!(buffers.len(), 1, "one persistent output buffer");
        let got = host.read_buffer(buffers[0]).expect("read loop results");
        // Dyadic arithmetic makes this source-order oracle bit exact on CPU and GPU.
        let expected: Vec<u8> = (0..129u32)
            .flat_map(|lane| {
                let mut state = if lane % 7 == 6 { 1.0f32 } else { (lane % 5) as f32 * 0.0078125 };
                let mut sum = 0.0f32;
                for i in 0..bound {
                    if (lane + i) % 3 != 0 && state >= 0.015625 {
                        for _ in 0..(2 + lane % 3) {
                            sum += state * 0.25;
                        }
                        state *= 0.5;
                    }
                }
                [state, sum].into_iter().flat_map(f32::to_le_bytes)
            })
            .collect();
        assert_eq!(got, expected, "bound {bound}");
    }
}
