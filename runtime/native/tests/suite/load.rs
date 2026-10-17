//! Loading a build: what's rejected before the GPU is touched (no GPU needed).

use crate::common;

use wrela_host::{CpuBuild, Error, Host};

fn load_with_manifest(name: &str, edit: impl Fn(&str) -> String) -> Error {
    let dir = common::first_light_copy(name);
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest");
    std::fs::write(dir.join("manifest.json"), edit(&manifest)).expect("write");
    Host::load(&dir).err().expect("rejected")
}

#[test]
fn rejects_another_manifest_version() {
    let err = load_with_manifest("manifest-v1", |m| {
        m.replace(
            &format!("\"manifest_version\": {}", wrela_abi::manifest::VERSION),
            "\"manifest_version\": 1",
        )
    });
    assert!(matches!(&err, Error::Manifest(_)), "{err}");
    let want =
        format!("manifest version 1, but this host reads version {}", wrela_abi::manifest::VERSION);
    assert!(err.to_string().contains(&want), "{err}");
}

#[test]
fn rejects_another_stream_version() {
    let other = wrela_abi::stream::VERSION + 1;
    let err = load_with_manifest("stream-other", |m| {
        m.replace(
            &format!("\"stream_version\": {}", wrela_abi::stream::VERSION),
            &format!("\"stream_version\": {other}"),
        )
    });
    assert!(matches!(&err, Error::Manifest(_)), "{err}");
    assert!(err.to_string().contains(&format!("command stream version {other}")), "{err}");
}

#[test]
fn reports_missing_files() {
    let err = load_with_manifest("missing-shader", |m| m.replace("fill.wgsl", "nope.wgsl"));
    assert!(matches!(&err, Error::Io { path, .. } if path.ends_with("nope.wgsl")), "{err}");
    let err = Host::load(common::temp_dir("empty")).err().expect("rejected");
    assert!(matches!(&err, Error::Io { path, .. } if path.ends_with("manifest.json")), "{err}");
}

#[test]
fn cpu_frame_builds_need_no_emitted_shaders() {
    let dir = common::first_light_copy("cpu-without-shaders");
    std::fs::remove_file(dir.join("fill.wgsl")).expect("remove compute shader");
    std::fs::remove_file(dir.join("first-light.wgsl")).expect("remove render shader");
    assert!(matches!(CpuBuild::load_metered(&dir, 1_000_000), Err(Error::Io { .. })));
    let mut host = CpuBuild::load_metered_cpu(&dir, 1_000_000)
        .expect("CPU build loads")
        .start_with(1)
        .expect("init's dispatch is checked without running a shader");
    host.frame(0.0, 64, 48).expect("the frame's draw is checked without running a shader");
    assert_eq!(
        host.call_export("tri", &[wrela_host::Value::F32(0.25)]).unwrap(),
        [wrela_host::Value::F32(0.0)]
    );
}

#[test]
fn rejects_a_program_with_other_imports() {
    let wasm = wat::parse_str(
        r#"(module (import "env" "now" (func (result f64))) (import "wrela" "memory" (memory 1 16384 shared)) (export "memory" (memory 0))
             (func (export "frame") (param f32 i32 i32)))"#,
    )
    .expect("compiles");
    let dir = common::first_light_with("other-import", &wasm);
    let err = Host::load(&dir).err().expect("rejected");
    assert_eq!(
        err.to_string(),
        "invalid program: it imports `env.now`, which isn't one of the host's: `wrela.memory`, `wrela.submit`, `wrela.request_status`, `wrela.request_take`, `wrela.limit`, `wrela.audio`, `wrela.input`, `wrela.payload`, `wrela.tick`, `wrela.keep`, `wrela.kept`, `wrela.clock`, `wrela.print`, `wrela.phase`"
    );
}

#[test]
fn a_server_build_loaded_again_reads_its_compiled_code_and_runs_the_same() {
    use wrela_host::server::{Limits, Room, ServerBuild};
    // A server build that counts its calls in its memory's first word.
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
             (func (export "serve") (param f32)
               (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))))"#,
    )
    .expect("compiles");
    let dir = std::env::temp_dir().join(format!("wrela-server-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("server")).expect("a build directory");
    std::fs::write(dir.join("server/game.wasm"), &wasm).expect("the build");
    let calls = |b: &ServerBuild| {
        let mut room = Room::start(b, Limits::default(), None).expect("it starts");
        room.serve(0.0).expect("a call");
        room.serve(0.0).expect("another");
        room.memory_bytes()
    };
    let first = ServerBuild::load(&dir).expect("the first load compiles it");
    // Beside the build, or in the shared directory when `WRELA_NATIVE_CACHE` names one.
    let cache = std::env::var_os("WRELA_NATIVE_CACHE")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dir.join("server/.native"));
    let prefix = wrela_host::compiled_code_prefix(&wasm);
    let cached: Vec<_> = std::fs::read_dir(&cache)
        .expect("its code is kept")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&prefix) && n.ends_with("-server.cwasm"))
        .collect();
    assert_eq!(cached.len(), 1, "{cached:?}");
    let code = cache.join(&cached[0]);
    let written = std::fs::metadata(&code).and_then(|m| m.modified()).expect("when it was written");
    let again = ServerBuild::load(&dir).expect("the second load reads the code back");
    let still = std::fs::metadata(&code).and_then(|m| m.modified()).expect("when it was written");
    assert_eq!(written, still, "the second load reads the code, and writes none");
    assert_eq!(calls(&first), calls(&again));
    let _ = std::fs::remove_dir_all(&dir);
}
