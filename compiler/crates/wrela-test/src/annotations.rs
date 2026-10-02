//! In-file expectations for conformance tests.
//!
//! - `//~ ERROR E0001` expects an error with that code whose primary span starts on this line;
//!   `//~^ ERROR E0001` on the line above, `//~^^` two above, and so on. `WARN` expects a warning.
//!   Text after the code must appear in the message: `//~ ERROR E0001 unexpected character`.
//! - `//@ check-pass` (a whole-line directive) expects no diagnostics at all.
//!
//! A file must say which it is: annotations, or `check-pass`.

use wrela_diag::{Code, Diagnostic, Severity, SourceMap};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Expected {
    /// 1-based line of the primary span's start.
    pub line: u32,
    pub severity: Severity,
    pub code: Code,
    /// Text the message must contain.
    pub message: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Expectations {
    pub check_pass: bool,
    pub expected: Vec<Expected>,
}

/// Whether a file with neither annotations nor `//@ check-pass` is accepted as expecting a clean
/// check. Conformance files must be explicit; documentation examples are clean by default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    Explicit,
    CleanByDefault,
}

/// Reads the expectations in `text`. Malformed annotations are errors, as `line: problem`.
pub fn parse(text: &str, policy: Policy) -> Result<Expectations, Vec<String>> {
    let mut expectations = Expectations::default();
    let mut errors = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if let Some(directive) = line.trim_start().strip_prefix("//@") {
            match directive.trim() {
                "check-pass" => expectations.check_pass = true,
                other => errors.push(format!("{number}: unknown directive `//@ {other}`")),
            }
            continue;
        }
        let Some(at) = line.find("//~") else { continue };
        match parse_annotation(&line[at + 3..], number) {
            Ok(expected) => expectations.expected.push(expected),
            Err(problem) => errors.push(format!("{number}: {problem}")),
        }
    }
    if expectations.check_pass && !expectations.expected.is_empty() {
        errors.push("`//@ check-pass` contradicts the `//~` annotations".to_string());
    }
    if policy == Policy::Explicit && !expectations.check_pass && expectations.expected.is_empty() {
        errors
            .push("no expectations: add `//~ ERROR <code>` annotations or `//@ check-pass`".into());
    }
    if errors.is_empty() {
        Ok(expectations)
    } else {
        Err(errors)
    }
}

fn parse_annotation(rest: &str, line: u32) -> Result<Expected, String> {
    let carets = rest.chars().take_while(|&c| c == '^').count();
    let mut words = rest[carets..].split_whitespace();
    let severity = match words.next() {
        Some("ERROR") => Severity::Error,
        Some("WARN") => Severity::Warning,
        other => {
            return Err(format!(
                "expected `ERROR` or `WARN` after `//~`, found {other:?}"
            ));
        }
    };
    let id = words
        .next()
        .ok_or("expected a diagnostic code, like `E0001`")?;
    let code = Code::parse(id).ok_or_else(|| format!("`{id}` isn't a registered code"))?;
    let message: Vec<&str> = words.collect();
    let target = u32::try_from(carets)
        .ok()
        .and_then(|up| line.checked_sub(up))
        .filter(|&l| l > 0);
    let line = target.ok_or("the `^`s point above the first line")?;
    Ok(Expected {
        line,
        severity,
        code,
        message: (!message.is_empty()).then(|| message.join(" ")),
    })
}

/// Compares what the check reported with what `text` expects, returning a report of every
/// unexpected and every missing diagnostic, each as `name:line: ...`.
pub fn compare(
    name: &str,
    expectations: &Expectations,
    actual: &[Diagnostic],
    sources: &SourceMap,
) -> Result<(), String> {
    let mut unmatched: Vec<&Expected> = expectations.expected.iter().collect();
    let mut problems = Vec::new();
    for diagnostic in actual {
        let span = diagnostic.primary.span;
        let line = sources
            .get(span.file())
            .map_or(0, |file| file.line_col(span.start()).line);
        let found = unmatched.iter().position(|e| {
            e.line == line
                && e.severity == diagnostic.severity
                && e.code == diagnostic.code
                && e.message
                    .as_ref()
                    .is_none_or(|m| diagnostic.message.contains(m.as_str()))
        });
        match found {
            Some(i) => {
                unmatched.remove(i);
            }
            None => problems.push((
                line,
                format!(
                    "unexpected {}[{}]: {}",
                    diagnostic.severity, diagnostic.code, diagnostic.message
                ),
            )),
        }
    }
    for e in unmatched {
        let kind = if e.severity == Severity::Error {
            "ERROR"
        } else {
            "WARN"
        };
        let message = e
            .message
            .as_ref()
            .map(|m| format!(" containing {m:?}"))
            .unwrap_or_default();
        problems.push((
            e.line,
            format!("expected {kind} {}{message}, not reported", e.code),
        ));
    }
    if problems.is_empty() {
        return Ok(());
    }
    problems.sort();
    let lines: Vec<String> = problems
        .into_iter()
        .map(|(line, problem)| format!("{name}:{line}: {problem}"))
        .collect();
    Err(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_annotations_and_directives() {
        let text = "a $ //~ ERROR E0001\n\n//~^^ ERROR E0001 unexpected\n";
        let parsed = parse(text, Policy::Explicit).unwrap();
        assert_eq!(parsed.expected.len(), 2);
        assert_eq!(parsed.expected[0].line, 1);
        assert_eq!(parsed.expected[1].line, 1);
        assert_eq!(parsed.expected[1].message.as_deref(), Some("unexpected"));
        assert!(
            parse("//@ check-pass\nx\n", Policy::Explicit)
                .unwrap()
                .check_pass
        );
    }

    #[test]
    fn rejects_malformed_or_missing_expectations() {
        let problems = |text| parse(text, Policy::Explicit).unwrap_err();
        assert!(problems("x\n")[0].contains("no expectations"));
        assert!(problems("//~^ ERROR E0001\n")[0].contains("above the first line"));
        assert!(problems("x //~ error E0001\n")[0].contains("expected `ERROR` or `WARN`"));
        assert!(problems("x //~ ERROR E9999\n")[0].contains("isn't a registered code"));
        assert!(problems("//@ run\n")[0].contains("unknown directive"));
        assert!(problems("//@ check-pass\n$ //~ ERROR E0001\n")[0].contains("contradicts"));
        assert!(parse("x\n", Policy::CleanByDefault).is_ok());
    }
}
