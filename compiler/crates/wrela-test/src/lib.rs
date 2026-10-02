//! wrela's test runners. Each `tests/*.rs` target is a `harness = false` binary built on
//! [`harness`], so every file it checks is a separately named test:
//!
//! - `conformance`: `compiler/tests/conformance/**/*.wrela` against their `//~` annotations.
//! - `golden`: `compiler/tests/golden/*.wrela` against the `wrela check` output next to them, as
//!   text (`.stderr`) and JSON (`.json`). `WRELA_BLESS=1` rewrites the expected files.
//! - `explain`: each code's `wrela bad` example reports exactly that code; `wrela fixed` is clean.
//! - `repo`: the ```` ```wrela ```` blocks in `docs/` (see [`docs`]), the imagined-block ratchet
//!   and the context budget.
//!
//! A runner that finds no files fails: a moved directory must not turn into a green run of zero
//! tests. The `fuzz-smoke` binary, and a quick `cargo test` variant, live in [`fuzz`].

pub mod annotations;
pub mod docs;
pub mod fuzz;
pub mod harness;
pub mod markdown;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use wrela_diag::Diagnostic;
use wrela_driver::Session;

/// The repository root, found from this crate's location (`compiler/crates/wrela-test`).
///
/// # Panics
/// If that directory doesn't hold `Cargo.toml` and `CLAUDE.md`: the crate moved, and every runner
/// would otherwise look for its files in the wrong place.
pub fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.ancestors().nth(3).unwrap_or(manifest);
    assert!(
        root.join("Cargo.toml").is_file() && root.join("CLAUDE.md").is_file(),
        "{} isn't the repository root; update `repo_root` for the crate's new place",
        root.display()
    );
    root.to_path_buf()
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

/// Every file under `dir` with extension `ext`, sorted. A directory that can't be read is an
/// error, and so is finding no files: a runner must not pass by checking nothing.
pub fn files_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let error = |e: std::io::Error| format!("{}: {e}", repo_relative(dir));
        for entry in std::fs::read_dir(dir).map_err(error)? {
            let path = entry.map_err(error)?.path();
            if path.is_dir() {
                walk(&path, ext, out)?;
            } else if path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, ext, &mut out)?;
    if out.is_empty() {
        return Err(format!("no `.{ext}` files under {}", repo_relative(dir)));
    }
    out.sort();
    Ok(out)
}

/// The fuzzing corpus: the test programs under `compiler/tests/`, the explanation examples and
/// the docs' code blocks (imagined syntax included: it's realistic input).
pub fn fuzz_corpus() -> Result<Vec<String>, String> {
    let root = repo_root();
    let mut corpus = Vec::new();
    for path in files_with_extension(&root.join("compiler/tests"), "wrela")? {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        corpus.push(text);
    }
    for code in wrela_diag::Code::all() {
        corpus.extend(
            markdown::fenced_blocks(code.explanation())
                .into_iter()
                .map(|b| b.content),
        );
    }
    for doc in files_with_extension(&root.join("docs"), "md")? {
        let text = std::fs::read_to_string(&doc).map_err(|e| format!("{}: {e}", doc.display()))?;
        let blocks = markdown::fenced_blocks(&text);
        corpus.extend(
            blocks
                .into_iter()
                .filter(|b| b.info.starts_with("wrela"))
                .map(|b| b.content),
        );
    }
    Ok(corpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_no_files_is_an_error() {
        let root = repo_root();
        let missing = files_with_extension(&root.join("compiler/tests/no-such-dir"), "wrela");
        assert!(missing.unwrap_err().contains("compiler/tests/no-such-dir"));

        let empty = std::env::temp_dir().join(format!("wrela-test-empty-{}", std::process::id()));
        std::fs::create_dir_all(empty.join("sub")).unwrap();
        let found = files_with_extension(&empty, "wrela");
        let _ = std::fs::remove_dir_all(&empty);
        assert!(found.unwrap_err().starts_with("no `.wrela` files under"));

        let conformance = files_with_extension(&root.join("compiler/tests/conformance"), "wrela");
        assert!(!conformance.unwrap().is_empty());
    }
}
