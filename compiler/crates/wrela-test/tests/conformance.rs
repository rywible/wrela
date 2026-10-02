//! Every `compiler/tests/conformance/**/*.wrela` file, checked against its `//~` annotations
//! (see `wrela_test::annotations`).

use std::process::ExitCode;

use wrela_test::annotations::{self, Policy};
use wrela_test::harness::{self, Case};
use wrela_test::{check_text, files_with_extension, repo_relative, repo_root};

fn main() -> ExitCode {
    let dir = repo_root().join("compiler/tests/conformance");
    let cases = files_with_extension(&dir, "wrela")
        .into_iter()
        .map(|path| {
            let name = repo_relative(&path);
            let short = path
                .strip_prefix(&dir)
                .map(repo_relative)
                .unwrap_or_else(|_| name.clone());
            Case::new(format!("conformance::{short}"), move || {
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
                let expectations =
                    annotations::parse(&text, Policy::Explicit).map_err(|problems| {
                        let lines: Vec<String> =
                            problems.iter().map(|p| format!("{name}:{p}")).collect();
                        lines.join("\n")
                    })?;
                let (session, diagnostics) = check_text(&name, &text)?;
                annotations::compare(&name, &expectations, &diagnostics, session.sources())
            })
        })
        .collect();
    harness::main(cases)
}
