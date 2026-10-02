//! Repository-level checks that keep the docs honest and small:
//! - every ```` ```wrela ```` block in `docs/` checks as its `//~` annotations say (clean when it
//!   has none), unless its fence says ```` ```wrela imagined ```` (see `wrela_test::docs`);
//! - the number of imagined blocks only falls;
//! - the default reading path for agents stays within a token budget.

use std::path::Path;
use std::process::ExitCode;

use wrela_test::harness::{self, Case};
use wrela_test::{docs, files_with_extension, repo_relative, repo_root};

/// The ratchet: imagined blocks in `docs/` are syntax ahead of the compiler. As features land,
/// blocks become checked and this number only falls; lower it when they do.
const MAX_IMAGINED: usize = 18;

/// The default reading path's budget, in estimated tokens (see `estimate_tokens`).
const BUDGET_TOKENS: usize = 5000;

/// The files `CLAUDE.md` tells agents to read before working, besides itself. They are the lines
/// of `CLAUDE.md` that say "read it before" or "read first"; `context_budget` fails if this list
/// and that text disagree.
const READ_FIRST: &[&str] = &["docs/vision.md"];

fn main() -> ExitCode {
    let root = repo_root();
    let mut cases = Vec::new();
    let mut imagined = 0;
    let files = match files_with_extension(&root.join("docs"), "md") {
        Ok(files) => files,
        Err(problem) => return harness::main(vec![Case::new("docs", || Err(problem))]),
    };
    for doc in files {
        let shown = repo_relative(&doc);
        let text = match std::fs::read_to_string(&doc) {
            Ok(text) => text,
            Err(e) => {
                let message = format!("{shown}: {e}");
                cases.push(Case::new(format!("docs::{shown}"), move || Err(message)));
                continue;
            }
        };
        let scan = docs::scan(&shown, &text);
        imagined += scan.imagined;
        if !scan.problems.is_empty() {
            let problems = scan.problems.join("\n");
            cases.push(Case::new(format!("docs::{shown}::fences"), move || {
                Err(problems)
            }));
        }
        for (name, content) in scan.checked {
            cases.push(Case::new(format!("docs::{name}"), move || {
                docs::check_block(&name, &content)
            }));
        }
    }
    cases.push(Case::new("docs::imagined-ratchet", move || {
        ratchet(imagined)
    }));
    cases.push(Case::new("repo::context-budget", move || {
        context_budget(&root)
    }));
    harness::main(cases)
}

fn ratchet(imagined: usize) -> Result<(), String> {
    if imagined > MAX_IMAGINED {
        return Err(format!(
            "docs/ has {imagined} ```wrela imagined blocks; the limit is {MAX_IMAGINED} and only falls. \
             Write the example in syntax the compiler checks, or keep it out of docs/."
        ));
    }
    if imagined < MAX_IMAGINED {
        return Err(format!(
            "docs/ has {imagined} ```wrela imagined blocks, fewer than the limit of {MAX_IMAGINED}: \
             lower MAX_IMAGINED in {} to {imagined} to lock in the progress.",
            file!()
        ));
    }
    Ok(())
}

/// Estimated tokens: ceil(chars / 4), with each run of spaces counted as one char.
///
/// Both parts are hypotheses, not calibrated against a real tokenizer: chars / 4 is the usual
/// rule of thumb for English prose, and collapsing runs of spaces assumes a tokenizer spends about
/// one token on a run (most of the runs here are the padding that aligns Markdown tables). The
/// collapsing matters: on 2026-10-02, CLAUDE.md + docs/vision.md came to ~4,807 with it and
/// ~5,066 without. Calibrate both against a tokenizer before tightening the budget.
fn estimate_tokens(text: &str) -> usize {
    let mut chars = 0usize;
    let mut previous_space = false;
    for c in text.chars() {
        let space = c == ' ';
        if !(space && previous_space) {
            chars += 1;
        }
        previous_space = space;
    }
    chars.div_ceil(4)
}

fn context_budget(root: &Path) -> Result<(), String> {
    let claude =
        std::fs::read_to_string(root.join("CLAUDE.md")).map_err(|e| format!("CLAUDE.md: {e}"))?;

    // Keep READ_FIRST in step with what CLAUDE.md actually says.
    let markers = [
        "read it before",
        "read first",
        "read before",
        "read this first",
    ];
    let mut mentioned: Vec<String> = Vec::new();
    for line in claude.lines() {
        let lower = line.to_lowercase();
        if markers.iter().any(|m| lower.contains(m)) {
            let paths = line
                .split('`')
                .skip(1)
                .step_by(2)
                .filter(|s| s.ends_with(".md"));
            mentioned.extend(paths.map(str::to_string));
        }
    }
    mentioned.sort();
    mentioned.dedup();
    let mut listed: Vec<String> = READ_FIRST.iter().map(|s| (*s).to_string()).collect();
    listed.sort();
    if mentioned != listed {
        return Err(format!(
            "CLAUDE.md says to read {mentioned:?} first, but READ_FIRST in {} lists {listed:?}",
            file!()
        ));
    }

    let mut total = estimate_tokens(&claude);
    let mut plain = claude.chars().count().div_ceil(4);
    let mut report = format!("CLAUDE.md: {total}");
    for path in READ_FIRST {
        let text = std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
        let tokens = estimate_tokens(&text);
        total += tokens;
        plain += text.chars().count().div_ceil(4);
        report.push_str(&format!(", {path}: {tokens}"));
    }
    if total > BUDGET_TOKENS {
        return Err(format!(
            "the default reading path is an estimated ~{total} tokens ({report}; ~{plain} by plain \
             chars / 4), over the budget of {BUDGET_TOKENS}. Cut CLAUDE.md or what it says to read \
             first; move detail into code, tests or `wrela explain`."
        ));
    }
    Ok(())
}
