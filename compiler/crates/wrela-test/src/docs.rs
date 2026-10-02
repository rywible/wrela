//! The docs check: every ```` ```wrela ```` block in `docs/` checks as its `//~` annotations say
//! (clean when it has none), unless its fence says ```` ```wrela imagined ````.
//!
//! A block the check can't see would escape it silently, so [`scan`] also rejects any line that
//! looks like a `wrela` fence but doesn't open a block it found (one in a blockquote, or indented
//! inside a list item), and any `wrela` info string it doesn't understand (`wrela,imagined`).

use std::collections::HashSet;

use crate::annotations::{self, Policy};
use crate::check_text;
use crate::markdown::fenced_blocks;

/// What one Markdown file holds, for the docs check.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Scan {
    /// The blocks to check, each as (`path:line`, content).
    pub checked: Vec<(String, String)>,
    /// The number of ```` ```wrela imagined ```` blocks.
    pub imagined: usize,
    /// Fences the check rejects, each as `path:line: problem`.
    pub problems: Vec<String>,
}

/// Sorts the `wrela` blocks of `text`, the Markdown file shown as `shown`.
pub fn scan(shown: &str, text: &str) -> Scan {
    let mut scan = Scan::default();
    let mut opened = HashSet::new();
    for block in fenced_blocks(text) {
        let name = format!("{shown}:{}", block.line);
        match block.tags().as_slice() {
            ["wrela"] => scan.checked.push((name, block.content)),
            ["wrela", "imagined"] => scan.imagined += 1,
            [first, ..] if first.starts_with("wrela") => scan.problems.push(format!(
                "{name}: unknown fence `{}`; use ```wrela or ```wrela imagined",
                block.info
            )),
            _ => continue,
        }
        opened.insert(block.line);
    }
    for (index, line) in text.lines().enumerate() {
        if looks_like_a_wrela_fence(line) && !opened.contains(&(index + 1)) {
            scan.problems.push(format!(
                "{shown}:{}: the docs check can't see this ```wrela block (it's in a blockquote, \
                 a list item or another block); move it to the top level",
                index + 1
            ));
        }
    }
    scan
}

/// Whether `line` looks like an opening `wrela` fence anywhere a Markdown reader might find one:
/// at any indentation, after any `>` blockquote markers.
fn looks_like_a_wrela_fence(line: &str) -> bool {
    let mut rest = line.trim_start();
    while let Some(inner) = rest.strip_prefix('>') {
        rest = inner.trim_start();
    }
    ["```", "~~~"].iter().any(|fence| {
        rest.strip_prefix(fence).is_some_and(|info| {
            let mark = &fence[..1];
            info.trim_start_matches(mark)
                .trim_start()
                .starts_with("wrela")
        })
    })
}

/// Checks one block against its annotations; `Err` is the report.
pub fn check_block(name: &str, content: &str) -> Result<(), String> {
    let expectations = annotations::parse(content, Policy::CleanByDefault)
        .map_err(|problems| format!("{name}: {}", problems.join("; ")))?;
    let (session, diagnostics) = check_text(name, content)?;
    annotations::compare(name, &expectations, &diagnostics, session.sources())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_blocks_are_checked() {
        let text =
            "# T\n\n```wrela\nfn f() { $ }\n```\n\n```wrela\nfn g() { $ } //~ ERROR E0001\n```\n";
        let scan = scan("t.md", text);
        assert_eq!(scan.problems, Vec::<String>::new());
        assert_eq!(scan.checked.len(), 2);
        let (name, content) = &scan.checked[0];
        assert_eq!(name, "t.md:3");
        let report = check_block(name, content).unwrap_err();
        assert!(report.contains("unexpected error[E0001]"), "{report}");
        let (name, content) = &scan.checked[1];
        assert_eq!(check_block(name, content), Ok(()));
    }

    #[test]
    fn imagined_blocks_are_counted_and_other_languages_ignored() {
        let text = "```wrela imagined\n$\n```\n~~~wrela imagined\n$\n~~~\n```rust\n$\n```\n";
        let scan = scan("t.md", text);
        assert_eq!((scan.imagined, scan.checked.len()), (2, 0));
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    }

    #[test]
    fn unknown_wrela_fences_are_rejected() {
        for fence in ["wrela,imagined", "wrela-imagined", "wrela bad", "wrelax"] {
            let scan = scan("t.md", &format!("```{fence}\n$\n```\n"));
            assert_eq!(scan.problems.len(), 1, "{fence}: {:?}", scan.problems);
            assert!(scan.problems[0].contains("unknown fence"), "{fence}");
            assert_eq!((scan.imagined, scan.checked.len()), (0, 0), "{fence}");
        }
    }

    #[test]
    fn fences_the_reader_cannot_see_are_rejected() {
        for text in [
            "> ```wrela\n> $\n> ```\n",
            "> > ~~~ wrela imagined\n> > $\n> > ~~~\n",
            "1. A step:\n\n    ```wrela\n    $\n    ```\n",
            "```markdown\n```wrela\n$\n```\n",
        ] {
            let scan = scan("t.md", text);
            assert_eq!(scan.problems.len(), 1, "{text:?}: {:?}", scan.problems);
            assert!(scan.problems[0].contains("can't see"), "{text:?}");
        }
    }
}
