//! `wrela check` output for each `compiler/tests/golden/<name>.wrela`, compared with
//! `<name>.stderr` (the text rendering) and `<name>.json` (the JSON rendering, which also pins
//! the JSON shape). `WRELA_BLESS=1 cargo test -p wrela-test --test golden` rewrites them.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use wrela_test::harness::{self, Case};
use wrela_test::{files_with_extension, repo_relative, repo_root};

fn main() -> ExitCode {
    let root = repo_root();
    let bless = std::env::var_os("WRELA_BLESS").is_some_and(|v| v != "0" && !v.is_empty());
    let files = match files_with_extension(&root.join("compiler/tests/golden"), "wrela") {
        Ok(files) => files,
        Err(problem) => return harness::main(vec![Case::new("golden", || Err(problem))]),
    };
    let mut cases = Vec::new();
    for path in files {
        let name = repo_relative(&path);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (format, extension) in [("text", "stderr"), ("json", "json")] {
            let (root, name, path) = (root.clone(), name.clone(), path.clone());
            cases.push(Case::new(
                format!("golden::{stem}.{extension}"),
                move || {
                    let actual = run_check(&root, &name, format)?;
                    compare(&path.with_extension(extension), &actual, bless)
                },
            ));
        }
    }
    harness::main(cases)
}

/// Runs `wrela check --format <format> <name>` from the repo root and returns the output the
/// format writes to: stderr for text, stdout for JSON. The other stream must be empty.
fn run_check(root: &Path, name: &str, format: &str) -> Result<String, String> {
    let args: Vec<OsString> = ["check", "--format", format, name]
        .iter()
        .map(OsString::from)
        .collect();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let status = wrela_cli::run(&args, root, &mut stdout, &mut stderr);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr),
    );
    if status > wrela_cli::EXIT_ERRORS {
        return Err(format!(
            "`wrela check` couldn't run (status {status}):\n{stderr}"
        ));
    }
    let (wanted, other) = if format == "json" {
        (stdout, stderr)
    } else {
        (stderr, stdout)
    };
    if !other.is_empty() {
        return Err(format!("unexpected output on the other stream:\n{other}"));
    }
    Ok(wanted.into_owned())
}

fn compare(expected_path: &Path, actual: &str, bless: bool) -> Result<(), String> {
    let shown = repo_relative(expected_path);
    if bless {
        return std::fs::write(expected_path, actual).map_err(|e| format!("{shown}: {e}"));
    }
    let expected = std::fs::read_to_string(expected_path)
        .map_err(|e| format!("{shown}: {e} (WRELA_BLESS=1 creates it)"))?;
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "{shown} differs (WRELA_BLESS=1 rewrites it):\n{}",
        diff(&expected, actual)
    ))
}

/// A line diff that's enough to read: lines only in the expected file are `-`, new ones `+`.
fn diff(expected: &str, actual: &str) -> String {
    let (old, new): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    // Longest common subsequence table, fine for the small files goldens are.
    let mut lcs = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, String::new());
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            out.push_str(&format!(" {}\n", old[i]));
            (i, j) = (i + 1, j + 1);
        } else if j < new.len() && (i == old.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push_str(&format!("+{}\n", new[j]));
            j += 1;
        } else {
            out.push_str(&format!("-{}\n", old[i]));
            i += 1;
        }
    }
    out
}
