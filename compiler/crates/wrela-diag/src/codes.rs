//! The registry of diagnostic codes.
//!
//! Every code is declared exactly once, in the `codes!` list below, with its default severity, a
//! short title and an explanation embedded from `explain/<CODE>.md`. A missing explanation file is
//! a compile error; `tests::every_explanation_file_is_registered` catches the reverse.
//!
//! Ranges: E0001–E0099 lexical · E0100–E0199 syntax · E0200–E0299 names and modules ·
//! E0300–E0499 types and traits · E0500–E0599 modes, moves and exclusivity · E0600–E0699 effects
//! and GPU rules · E0700–E0799 derived interpretations, IR and back-end limits · E0900–E0999 not in
//! tier 0 · W0001 and up: warnings.

use std::fmt;
use std::hash::{Hash, Hasher};

use crate::Severity;

/// What the registry knows about one code.
#[derive(Debug)]
pub struct CodeInfo {
    /// The code as written: `E0001`.
    pub id: &'static str,
    pub severity: Severity,
    /// A short lowercase summary, like a diagnostic message without the specifics.
    pub title: &'static str,
    /// The `wrela explain` text: the rule, a ```` ```wrela bad ```` example and a
    /// ```` ```wrela fixed ```` one.
    pub explanation: &'static str,
}

/// A registered diagnostic code. Only the registry can create one, so every code has an entry.
#[derive(Clone, Copy)]
pub struct Code(&'static CodeInfo);

impl Code {
    pub fn id(self) -> &'static str {
        self.0.id
    }

    pub fn info(self) -> &'static CodeInfo {
        self.0
    }

    pub fn default_severity(self) -> Severity {
        self.0.severity
    }

    pub fn title(self) -> &'static str {
        self.0.title
    }

    pub fn explanation(self) -> &'static str {
        self.0.explanation
    }

    /// Looks a code up by its id, ignoring ASCII case: `E0001` or `e0001`.
    pub fn parse(id: &str) -> Option<Code> {
        ALL.iter()
            .copied()
            .find(|code| code.id().eq_ignore_ascii_case(id))
    }

    /// Every registered code, in id order.
    pub fn all() -> &'static [Code] {
        ALL
    }
}

// Codes are compared by id, not by address: a `const` may be instantiated more than once.
impl PartialEq for Code {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for Code {}

impl Hash for Code {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl PartialOrd for Code {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Code {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(other.id())
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl serde::Serialize for Code {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.id())
    }
}

macro_rules! codes {
    ($( $(#[$doc:meta])* $name:ident = $id:literal, $severity:ident, $title:literal; )*) => {
        $(
            $(#[$doc])*
            #[doc = concat!("`", $id, "`: ", $title, ".")]
            pub const $name: Code = Code(&CodeInfo {
                id: $id,
                severity: Severity::$severity,
                title: $title,
                explanation: include_str!(concat!("../explain/", $id, ".md")),
            });
        )*

        const ALL: &[Code] = &[$($name),*];
    };
}

codes! {
    UNEXPECTED_CHARACTER = "E0001", Error, "unexpected character";
    BLOCK_COMMENT = "E0002", Error, "`/* */` comments aren't supported";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn ids_are_well_formed_unique_sorted_and_match_their_severity() {
        let mut seen = BTreeSet::new();
        let mut previous = "";
        for code in Code::all() {
            let id = code.id();
            let (prefix, digits) = id.split_at(1);
            assert!(
                digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()),
                "{id}"
            );
            let expected = match prefix {
                "E" => Severity::Error,
                "W" => Severity::Warning,
                _ => panic!("{id}: codes start with E or W"),
            };
            assert_eq!(code.default_severity(), expected, "{id}");
            assert!(seen.insert(id), "{id} is registered twice");
            assert!(previous < id, "{id}: keep the registry in id order");
            previous = id;
        }
    }

    #[test]
    fn titles_read_like_messages() {
        for code in Code::all() {
            let title = code.title();
            assert!(
                !title.is_empty() && !title.ends_with('.'),
                "{code}: {title:?}"
            );
            assert!(
                !title.starts_with(|c: char| c.is_ascii_uppercase()),
                "{code}: {title:?}"
            );
        }
    }

    #[test]
    fn every_explanation_has_a_heading_a_bad_and_a_fixed_example() {
        for code in Code::all() {
            let text = code.explanation();
            let heading = format!("# {}: {}\n", code.id(), code.title());
            assert!(
                text.starts_with(&heading),
                "{code}: explanation must start with {heading:?}"
            );
            assert!(
                text.contains("\n```wrela bad\n"),
                "{code}: no ```wrela bad block"
            );
            assert!(
                text.contains("\n```wrela fixed\n"),
                "{code}: no ```wrela fixed block"
            );
        }
    }

    #[test]
    fn every_explanation_file_is_registered() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("explain");
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            files.push(name);
        }
        files.sort();
        let registered: Vec<String> = Code::all()
            .iter()
            .map(|c| format!("{}.md", c.id()))
            .collect();
        assert_eq!(
            files, registered,
            "explain/ must hold exactly one file per registered code"
        );
    }

    #[test]
    fn parse_ignores_case_and_rejects_unknown_codes() {
        assert_eq!(Code::parse("e0001"), Some(UNEXPECTED_CHARACTER));
        assert_eq!(Code::parse("E0002"), Some(BLOCK_COMMENT));
        assert_eq!(Code::parse("E9999"), None);
        assert_eq!(Code::parse(""), None);
    }
}
