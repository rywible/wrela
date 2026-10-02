//! A minimal libtest-compatible runner for `harness = false` test targets, so each conformance
//! file, golden file or explanation is its own named test.
//!
//! It understands what `cargo test` and cargo-nextest pass: name filters, `--exact`, `--skip`,
//! `--list`, `--ignored`, `--include-ignored`. Other libtest flags (`--nocapture`,
//! `--test-threads`, `--color`, ...) are accepted and ignored. Cases run in order, one at a time.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::ExitCode;

type Body = Box<dyn FnOnce() -> Result<(), String>>;

/// One named test. It fails by returning `Err(report)` or by panicking.
pub struct Case {
    pub name: String,
    body: Body,
}

impl std::fmt::Debug for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Case")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Case {
    pub fn new(
        name: impl Into<String>,
        body: impl FnOnce() -> Result<(), String> + 'static,
    ) -> Self {
        Case {
            name: name.into(),
            body: Box::new(body),
        }
    }
}

#[derive(Default, Debug)]
struct Options {
    filters: Vec<String>,
    skips: Vec<String>,
    exact: bool,
    list: bool,
    ignored_only: bool,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        // libtest flags that take a value, when it's passed as a separate argument.
        const WITH_VALUE: &[&str] = &[
            "--test-threads",
            "--color",
            "--format",
            "--logfile",
            "-Z",
            "--shuffle-seed",
        ];
        let mut options = Options::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--exact" => options.exact = true,
                "--list" => options.list = true,
                "--ignored" => options.ignored_only = true,
                "--skip" => options.skips.extend(args.next()),
                flag if WITH_VALUE.contains(&flag) => {
                    args.next();
                }
                flag if flag.starts_with('-') => {}
                filter => options.filters.push(filter.to_string()),
            }
        }
        options
    }

    fn selects(&self, name: &str) -> bool {
        let matches = |pattern: &String| {
            if self.exact {
                name == pattern
            } else {
                name.contains(pattern.as_str())
            }
        };
        (self.filters.is_empty() || self.filters.iter().any(matches))
            && !self.skips.iter().any(matches)
    }
}

/// Runs the cases selected by the process's arguments and reports like libtest.
pub fn main(cases: Vec<Case>) -> ExitCode {
    run(cases, std::env::args().skip(1))
}

fn run(cases: Vec<Case>, args: impl Iterator<Item = String>) -> ExitCode {
    let options = Options::parse(args);
    let total = cases.len();
    // No case is ignored, so `--ignored` selects nothing.
    let selected: Vec<Case> = if options.ignored_only {
        Vec::new()
    } else {
        cases
            .into_iter()
            .filter(|case| options.selects(&case.name))
            .collect()
    };
    let filtered_out = total - selected.len();

    if options.list {
        for case in &selected {
            println!("{}: test", case.name);
        }
        return ExitCode::SUCCESS;
    }

    println!("\nrunning {} tests", selected.len());
    let mut failures: Vec<(String, String)> = Vec::new();
    for case in selected {
        let outcome = match catch_unwind(AssertUnwindSafe(case.body)) {
            Ok(result) => result,
            Err(payload) => Err(format!("panicked: {}", panic_message(payload.as_ref()))),
        };
        match outcome {
            Ok(()) => println!("test {} ... ok", case.name),
            Err(report) => {
                println!("test {} ... FAILED", case.name);
                failures.push((case.name, report));
            }
        }
    }

    let passed_count = total - filtered_out - failures.len();
    if !failures.is_empty() {
        println!("\nfailures:\n");
        for (name, report) in &failures {
            println!("---- {name} ----\n{}\n", report.trim_end());
        }
        println!("failures:");
        for (name, _) in &failures {
            println!("    {name}");
        }
    }
    let status = if failures.is_empty() { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {status}. {passed_count} passed; {} failed; 0 ignored; 0 measured; \
         {filtered_out} filtered out\n",
        failures.len()
    );
    // libtest's exit status for failed tests.
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(101)
    }
}

/// The message of a panic payload, when it's a string.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(a non-string panic payload)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Options {
        Options::parse(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn filters_skips_and_exact_match() {
        let o = options(&[
            "lexer",
            "--skip",
            "e0002",
            "--test-threads",
            "4",
            "--nocapture",
        ]);
        assert!(o.selects("conformance::lexer/e0001.wrela"));
        assert!(!o.selects("conformance::lexer/e0002.wrela"));
        assert!(!o.selects("golden::e0001"));
        let o = options(&["--exact", "golden::e0001"]);
        assert!(o.selects("golden::e0001") && !o.selects("golden::e0001-json"));
        assert!(options(&[]).selects("anything"));
    }
}
