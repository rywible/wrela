use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

struct ThinCoreSnapshot {
    abi_version: u32,
    intrinsics: BTreeSet<String>,
    runtime_exports: BTreeSet<String>,
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn parse_snapshot(path: &Path) -> ThinCoreSnapshot {
    let mut abi_version = None;
    let mut intrinsics = BTreeSet::new();
    let mut runtime_exports = BTreeSet::new();

    for raw in read(path).lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("abi_version=") {
            abi_version = value.parse::<u32>().ok();
            continue;
        }
        if let Some(value) = line.strip_prefix("intrinsic=") {
            intrinsics.insert(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("runtime_export=") {
            runtime_exports.insert(value.to_string());
        }
    }

    ThinCoreSnapshot {
        abi_version: abi_version.expect("missing abi_version in snapshot"),
        intrinsics,
        runtime_exports,
    }
}

fn extract_prefixed_symbols(source: &str, prefix: &str) -> BTreeSet<String> {
    let bytes = source.as_bytes();
    let mut out = BTreeSet::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let Some(found) = source[index..].find(prefix) else {
            break;
        };
        let start = index + found;
        let mut end = start + prefix.len();
        while end < bytes.len() {
            let ch = bytes[end] as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                end += 1;
            } else {
                break;
            }
        }
        out.insert(source[start..end].to_string());
        index = end;
    }
    out
}

fn extract_runtime_exports(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub extern \"C\" fn ") else {
            continue;
        };
        let Some(name) = rest.strip_prefix("wr_") else {
            continue;
        };
        let Some(end) = name.find('(') else {
            continue;
        };
        out.insert(format!("wr_{}", &name[..end]));
    }
    out
}

fn extract_abi_version(source: &str, marker: &str) -> u32 {
    for line in source.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix(marker) {
            let value = value.trim().trim_end_matches(';');
            return value.parse::<u32>().expect("invalid ABI value");
        }
    }
    panic!("missing ABI marker: {marker}");
}

#[test]
fn thin_core_snapshot_matches_compiler_and_runtime_surfaces() {
    let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshot = parse_snapshot(&compiler_dir.join("../spec/thin_core_snapshot.txt"));

    let semantic_intrinsics =
        extract_prefixed_symbols(&read(&compiler_dir.join("hir/semantic.rs")), "__wr_");
    let mir_intrinsics =
        extract_prefixed_symbols(&read(&compiler_dir.join("mir/lower.rs")), "__wr_");
    assert_eq!(
        semantic_intrinsics, mir_intrinsics,
        "semantic and MIR intrinsic lists diverged"
    );
    assert_eq!(
        snapshot.intrinsics, semantic_intrinsics,
        "intrinsic surface changed; update core/spec/thin_core_snapshot.txt intentionally"
    );

    let runtime_exports =
        extract_runtime_exports(&read(&compiler_dir.join("../runtime/src/lib.rs")));
    assert_eq!(
        snapshot.runtime_exports, runtime_exports,
        "runtime export surface changed; update core/spec/thin_core_snapshot.txt intentionally"
    );
}

#[test]
fn thin_core_abi_version_matches_snapshot() {
    let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let snapshot = parse_snapshot(&compiler_dir.join("../spec/thin_core_snapshot.txt"));

    let runtime_abi = extract_abi_version(
        &read(&compiler_dir.join("../runtime/src/diagnostics.rs")),
        "pub const RUNTIME_ABI_VERSION: u32 =",
    );
    let backend_abi = extract_abi_version(
        &read(&compiler_dir.join("backend/cranelift.rs")),
        "const RUNTIME_ABI_VERSION: i64 =",
    );

    assert_eq!(snapshot.abi_version, runtime_abi);
    assert_eq!(snapshot.abi_version, backend_abi);
}
