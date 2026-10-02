//! wrela's test runners. Each `tests/*.rs` target is a `harness = false` binary built on
//! [`harness`], so every file it checks is a separately named test:
//!
//! - `conformance`: `compiler/tests/conformance/**/*.wrela` against their `//~` annotations.
//! - `golden`: `compiler/tests/golden/*.wrela` against the `wrela check` output next to them, as
//!   text (`.stderr`) and JSON (`.json`). `WRELA_BLESS=1` rewrites the expected files.
//! - `explain`: each code's `wrela bad` example reports exactly that code; `wrela fixed` is clean.
//! - `repo`: the ```` ```wrela ```` blocks in `docs/`, the imagined-block ratchet and the context
//!   budget.
//!
//! The `fuzz-smoke` binary, and a quick `cargo test` variant, live in [`fuzz`].

pub mod annotations;
pub mod fuzz;
pub mod harness;
pub mod markdown;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use wrela_diag::Diagnostic;
use wrela_driver::Session;

/// The repository root, found from this crate's location (`compiler/crates/wrela-test`).
pub fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .unwrap_or(manifest)
        .to_path_buf()
}

/// `path` relative to the repository root, with `/` separators, as tests name files.
pub fn repo_relative(path: &Path) -> String {
    let relative = path.strip_prefix(repo_root()).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Checks `text` as a file called `name` and returns the session (for its source map) and the
/// diagnostics. A query error is reported as a test failure.
pub fn check_text(name: &str, text: &str) -> Result<(Session, Arc<[Diagnostic]>), String> {
    let mut session = Session::new();
    let file = session.add_file(name, text).map_err(|e| e.to_string())?;
    let diagnostics = session
        .check(file)
        .map_err(|e| format!("{name}: query error: {e}"))?;
    Ok((session, diagnostics))
}

/// Every file under `dir` with extension `ext`, sorted.
pub fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, ext, out);
            } else if path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, ext, &mut out);
    out.sort();
    out
}

/// The fuzzing corpus: the test programs under `compiler/tests/`, the explanation examples and
/// the docs' code blocks (imagined syntax included: it's realistic input).
pub fn fuzz_corpus() -> Vec<String> {
    let root = repo_root();
    let mut corpus: Vec<String> = files_with_extension(&root.join("compiler/tests"), "wrela")
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    for code in wrela_diag::Code::all() {
        corpus.extend(
            markdown::fenced_blocks(code.explanation())
                .into_iter()
                .map(|b| b.content),
        );
    }
    for doc in files_with_extension(&root.join("docs"), "md") {
        let Ok(text) = std::fs::read_to_string(&doc) else {
            continue;
        };
        let blocks = markdown::fenced_blocks(&text);
        corpus.extend(
            blocks
                .into_iter()
                .filter(|b| b.info.starts_with("wrela"))
                .map(|b| b.content),
        );
    }
    corpus
}
