//! Each registered code's explanation is checked by the compiler itself: every ```` ```wrela bad ````
//! example must report that code and nothing else, and every ```` ```wrela fixed ```` example must
//! check clean. This runs for every code, so a code whose phase isn't built yet can't be
//! registered with an example the compiler can't confirm.

use std::process::ExitCode;

use wrela_diag::Code;
use wrela_test::check_text;
use wrela_test::harness::{self, Case};
use wrela_test::markdown::fenced_blocks;

fn main() -> ExitCode {
    let cases = Code::all()
        .iter()
        .map(|&code| Case::new(format!("explain::{code}"), move || explain(code)))
        .collect();
    harness::main(cases)
}

fn explain(code: Code) -> Result<(), String> {
    let mut problems = Vec::new();
    let (mut bad, mut fixed) = (0, 0);
    for block in fenced_blocks(code.explanation()) {
        let tags = block.tags();
        let name = format!("explain/{code}.md:{}", block.line);
        let expect_bad = match tags.as_slice() {
            ["wrela", "bad"] => true,
            ["wrela", "fixed"] => false,
            ["wrela", ..] => {
                problems.push(format!(
                    "{name}: use ```wrela bad or ```wrela fixed, not {:?}",
                    block.info
                ));
                continue;
            }
            // Other languages (```text) are prose.
            _ => continue,
        };
        let (session, diagnostics) = check_text(&name, &block.content)?;
        let rendered = || wrela_diag::render_all(&diagnostics, session.sources());
        if expect_bad {
            bad += 1;
            if diagnostics.is_empty() || diagnostics.iter().any(|d| d.code != code) {
                problems.push(format!(
                    "{name}: the bad example must report only {code}, got:\n{}",
                    rendered()
                ));
            }
        } else {
            fixed += 1;
            if !diagnostics.is_empty() {
                problems.push(format!(
                    "{name}: the fixed example must check clean, got:\n{}",
                    rendered()
                ));
            }
        }
    }
    if bad == 0 || fixed == 0 {
        problems.push(format!(
            "explain/{code}.md needs a ```wrela bad and a ```wrela fixed example"
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}
