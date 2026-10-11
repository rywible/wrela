use wrela_host::CpuHost;

/// Checked wide arithmetic and three-lane copies use scratch locals of several WASM types.
/// Their values must survive nested branches, loops and the scratch reuse in later statements.
#[test]
fn codegen_perf_scratch_reuse_keeps_nested_values_and_traps() {
    let dir = wrela_tests::scratch_root(env!("CARGO_TARGET_TMPDIR")).join("backend-perf-scratch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch package");
    std::fs::write(
        dir.join("main.wrela"),
        r#"
fn store(out: mut vec3, x: vec3) {
    out = x
}

pub fn run() -> u32 {
    var a = vec3(1.0, 2.0, 3.0)
    var n: u64 = 3
    var sum = 0.0
    for i in 0..8 {
        n = n * 3
        if i % 2 == 0 {
            var b = vec3(f32(n), a.y, a.z)
            store(mut b, b + vec3(1.0, 2.0, 3.0))
            sum += b.x + b.y + b.z
        } else {
            store(mut a, a + vec3(2.0, 3.0, 4.0))
            sum += a.x + a.y + a.z
        }
    }
    assert(n == 19683)
    assert(sum == 7580.0)
    assert(a == vec3(9.0, 14.0, 19.0))
    u32(n)
}

pub fn overflow(x: u64) -> u64 {
    x * 3
}

pub fn frame(time: f32, width: u32, height: u32) {}
"#,
    )
    .expect("write source");
    let built = wrela_tests::build(&dir).expect("build scratch reuse regression");
    let out = dir.join("build");
    built.write_to(&out).expect("write build");
    let mut host = CpuHost::load(&out).expect("load scratch reuse regression");
    assert_eq!(wrela_tests::one_u32(&mut host, "run", &[]), 19683);
    assert!(host.call_export("overflow", &[wrela_host::Value::I64(-1)]).is_err());
}
