//! The `wrela` command, as a library so tests can run it in-process.
//!
//! Exit status: 0 when nothing is wrong, 1 when errors were reported, 2 when the command
//! couldn't run (bad usage, unreadable files, an internal error).

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};

use wrela_diag::{Code, Diagnostic, Severity};
use wrela_driver::Session;

pub const EXIT_OK: u8 = 0;
pub const EXIT_ERRORS: u8 = 1;
pub const EXIT_FAILURE: u8 = 2;

const USAGE: &str = "\
wrela: the wrela compiler

Usage:
  wrela check <path>... [--format text|json]
                       Check files; a directory means every .wrela file under it.
                       Text goes to stderr, JSON to stdout.
  wrela build <path>   Build a program (not available yet)
  wrela fmt <path>...  Format files (not available yet)
  wrela explain [<code>]
                       Explain a diagnostic code, or list every code
  wrela help           Show this message (also -h, --help)
  wrela --version      Show the version

Exit status: 0 no errors, 1 errors reported, 2 couldn't run (usage, I/O or internal error).
";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    Text,
    Json,
}

/// Runs `wrela` with `args` (without the program name). Relative paths resolve against `cwd`
/// but are shown as written.
pub fn run(args: &[OsString], cwd: &Path, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let mut cli = Cli {
        cwd,
        stdout,
        stderr,
    };
    match cli.dispatch(args) {
        Ok(code) => code,
        Err(Failure(message)) => {
            let _ = writeln!(cli.stderr, "error: {message}");
            EXIT_FAILURE
        }
    }
}

/// A reason the command couldn't run, reported as `error: ...` with exit status 2.
struct Failure(String);

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure(format!("{message}\n\n{USAGE}"))
}

struct Cli<'a> {
    cwd: &'a Path,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
}

impl Cli<'_> {
    fn dispatch(&mut self, args: &[OsString]) -> Result<u8, Failure> {
        let Some((command, rest)) = args.split_first() else {
            return Err(usage("no command given"));
        };
        match command.to_str() {
            Some("check") => self.check(rest),
            Some("build") => not_yet(
                "build",
                "with the back ends and the runtime (M1 slices S10–S15)",
            ),
            Some("fmt") => not_yet("fmt", "with the parser and formatter (M1 slice S3)"),
            Some("explain") => self.explain(rest),
            Some("help" | "-h" | "--help") => {
                self.out(USAGE);
                Ok(EXIT_OK)
            }
            Some("--version" | "-V") => {
                self.out(&format!("wrela {}\n", env!("CARGO_PKG_VERSION")));
                Ok(EXIT_OK)
            }
            _ => Err(usage(format_args!(
                "unknown command `{}`",
                command.to_string_lossy()
            ))),
        }
    }

    fn out(&mut self, text: &str) {
        let _ = self.stdout.write_all(text.as_bytes());
    }

    fn check(&mut self, args: &[OsString]) -> Result<u8, Failure> {
        let mut format = Format::Text;
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut options_done = false;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--") if !options_done => options_done = true,
                Some("--format") if !options_done => {
                    let value = args
                        .next()
                        .ok_or_else(|| usage("`--format` needs a value"))?;
                    format = parse_format(value)?;
                }
                Some(flag) if !options_done && flag.starts_with("--format=") => {
                    format = parse_format(OsStr::new(&flag["--format=".len()..]))?;
                }
                Some(flag) if !options_done && flag.starts_with('-') && flag != "-" => {
                    return Err(usage(format_args!("unknown option `{flag}`")));
                }
                _ => paths.push(PathBuf::from(arg)),
            }
        }
        if paths.is_empty() {
            return Err(usage("`wrela check` needs at least one path"));
        }

        let mut session = Session::new();
        for path in collect_files(&paths, self.cwd)? {
            let shown = path.display();
            let bytes = std::fs::read(self.cwd.join(&path))
                .map_err(|e| Failure(format!("couldn't read `{shown}`: {e}")))?;
            let text = String::from_utf8(bytes).map_err(|e| {
                let at = e.utf8_error().valid_up_to();
                Failure(format!(
                    "`{shown}` isn't UTF-8: the first invalid byte is at offset {at}"
                ))
            })?;
            session
                .add_file(shown.to_string(), text)
                .map_err(|e| Failure(e.to_string()))?;
        }
        let diagnostics = session
            .check_all()
            .map_err(|e| Failure(format!("internal compiler error: {e}")))?;

        match format {
            Format::Text => {
                let rendered = wrela_diag::render_all(&diagnostics, session.sources());
                let _ = self.stderr.write_all(rendered.as_bytes());
                if let Some(summary) = summary(&diagnostics) {
                    let _ = writeln!(self.stderr, "{summary}");
                }
            }
            Format::Json => self.out(&wrela_diag::to_json(&diagnostics, session.sources())),
        }
        Ok(if diagnostics.iter().any(Diagnostic::is_error) {
            EXIT_ERRORS
        } else {
            EXIT_OK
        })
    }

    fn explain(&mut self, args: &[OsString]) -> Result<u8, Failure> {
        match args {
            [] => {
                let list: String = Code::all()
                    .iter()
                    .map(|code| format!("{code}  {}\n", code.title()))
                    .collect();
                self.out(&list);
                Ok(EXIT_OK)
            }
            [id] => {
                let id = id.to_string_lossy();
                let code = Code::parse(&id).ok_or_else(|| {
                    Failure(format!(
                        "`{id}` isn't a wrela diagnostic code; `wrela explain` lists them all"
                    ))
                })?;
                self.out(code.explanation());
                Ok(EXIT_OK)
            }
            _ => Err(usage("`wrela explain` takes one code")),
        }
    }
}

fn not_yet(command: &str, when: &str) -> Result<u8, Failure> {
    Err(Failure(format!(
        "`wrela {command}` isn't available yet; it arrives {when}"
    )))
}

fn parse_format(value: &OsStr) -> Result<Format, Failure> {
    match value.to_str() {
        Some("text") => Ok(Format::Text),
        Some("json") => Ok(Format::Json),
        _ => Err(usage(format_args!(
            "unknown format `{}`; use `text` or `json`",
            value.to_string_lossy()
        ))),
    }
}

/// The files to check, in order: each file argument as given, and for each directory every
/// `.wrela` file under it (skipping hidden entries), sorted. Duplicates are dropped.
fn collect_files(paths: &[PathBuf], cwd: &Path) -> Result<Vec<PathBuf>, Failure> {
    let mut files = Vec::new();
    for path in paths {
        let metadata = std::fs::metadata(cwd.join(path))
            .map_err(|e| Failure(format!("couldn't read `{}`: {e}", path.display())))?;
        if metadata.is_dir() {
            let before = files.len();
            walk(path, cwd, &mut files)?;
            files[before..].sort();
            if files.len() == before {
                return Err(Failure(format!(
                    "no `.wrela` files under `{}`",
                    path.display()
                )));
            }
        } else {
            files.push(path.clone());
        }
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|file| seen.insert(file.clone()));
    Ok(files)
}

fn walk(dir: &Path, cwd: &Path, files: &mut Vec<PathBuf>) -> Result<(), Failure> {
    let failure = |path: &Path, e: std::io::Error| {
        Failure(format!("couldn't read `{}`: {e}", path.display()))
    };
    for entry in std::fs::read_dir(cwd.join(dir)).map_err(|e| failure(dir, e))? {
        let entry = entry.map_err(|e| failure(dir, e))?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = dir.join(&name);
        let file_type = entry.file_type().map_err(|e| failure(&path, e))?;
        if file_type.is_dir() {
            walk(&path, cwd, files)?;
        } else if path.extension() == Some(OsStr::new("wrela")) {
            files.push(path);
        }
    }
    Ok(())
}

/// `check failed: 2 errors and 1 warning`, or `None` when there's nothing to report.
fn summary(diagnostics: &[Diagnostic]) -> Option<String> {
    let count = |severity| {
        diagnostics
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    };
    let plural = |n: usize, noun: &str| format!("{n} {noun}{}", if n == 1 { "" } else { "s" });
    match (count(Severity::Error), count(Severity::Warning)) {
        (0, 0) => None,
        (0, w) => Some(format!("check found {}", plural(w, "warning"))),
        (e, 0) => Some(format!("check failed: {}", plural(e, "error"))),
        (e, w) => Some(format!(
            "check failed: {} and {}",
            plural(e, "error"),
            plural(w, "warning")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_in(cwd: &Path, args: &[&str]) -> (u8, String, String) {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, cwd, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// A scratch directory, removed when the test is done with it.
    struct TempDir(PathBuf);

    impl std::ops::Deref for TempDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("wrela-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    #[test]
    fn check_exit_codes_follow_the_diagnostics() {
        let dir = temp_dir("exit");
        std::fs::write(dir.join("ok.wrela"), "fn f() {}\n").unwrap();
        std::fs::write(dir.join("bad.wrela"), "fn f() { $ }\n").unwrap();
        let (code, out, err) = run_in(&dir, &["check", "ok.wrela"]);
        assert_eq!((code, out.as_str(), err.as_str()), (EXIT_OK, "", ""));
        let (code, _, err) = run_in(&dir, &["check", "bad.wrela"]);
        assert_eq!(code, EXIT_ERRORS);
        let header = "error[E0001]: unexpected character `$`\n --> bad.wrela:1:10\n";
        assert!(err.starts_with(header), "{err}");
        assert!(err.ends_with("check failed: 1 error\n"), "{err}");
        let (code, out, err) = run_in(&dir, &["check", "--format=json", "bad.wrela", "ok.wrela"]);
        assert_eq!((code, err.as_str()), (EXIT_ERRORS, ""));
        assert!(out.contains("\"code\": \"E0001\""), "{out}");
    }

    #[test]
    fn check_walks_directories_in_sorted_order() {
        let dir = temp_dir("walk");
        std::fs::create_dir_all(dir.join("src/b")).unwrap();
        std::fs::write(dir.join("src/b/z.wrela"), "$").unwrap();
        std::fs::write(dir.join("src/a.wrela"), "$").unwrap();
        std::fs::write(dir.join("src/notes.txt"), "$").unwrap();
        let (code, _, err) = run_in(&dir, &["check", "src"]);
        assert_eq!(code, EXIT_ERRORS);
        let a = err.find("src/a.wrela").unwrap();
        let z = err.find("src/b/z.wrela").unwrap();
        assert!(a < z && !err.contains("notes.txt"), "{err}");
        assert!(err.ends_with("check failed: 2 errors\n"), "{err}");
    }

    #[test]
    fn usage_and_io_problems_exit_2() {
        let dir = temp_dir("usage");
        std::fs::write(dir.join("latin1.wrela"), b"x \xE9").unwrap();
        for args in [
            &[][..],
            &["frobnicate"],
            &["check"],
            &["check", "--format", "xml", "a.wrela"],
            &["check", "--verbose", "a.wrela"],
            &["check", "missing.wrela"],
            &["explain", "E9999"],
            &["explain", "E0001", "E0002"],
        ] {
            let (code, _, err) = run_in(&dir, args);
            assert_eq!(code, EXIT_FAILURE, "{args:?}");
            assert!(err.starts_with("error: "), "{args:?}: {err}");
        }
        let (code, _, err) = run_in(&dir, &["check", "latin1.wrela"]);
        assert_eq!(code, EXIT_FAILURE);
        assert!(
            err.contains("isn't UTF-8: the first invalid byte is at offset 2"),
            "{err}"
        );
    }

    #[test]
    fn build_and_fmt_say_they_are_not_available_yet() {
        let dir = temp_dir("later");
        for command in ["build", "fmt"] {
            let (code, _, err) = run_in(&dir, &[command, "x"]);
            assert_eq!(code, EXIT_FAILURE);
            assert!(
                err.contains(&format!("`wrela {command}` isn't available yet")),
                "{err}"
            );
        }
    }

    #[test]
    fn explain_prints_a_code_or_the_list() {
        let dir = temp_dir("explain");
        let (code, out, _) = run_in(&dir, &["explain", "e0002"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("# E0002: "), "{out}");
        let (code, out, _) = run_in(&dir, &["explain"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("E0001  unexpected character\n"), "{out}");
    }

    #[test]
    fn help_and_version() {
        let dir = temp_dir("help");
        let (code, out, _) = run_in(&dir, &["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains("wrela check <path>..."));
        let (code, out, _) = run_in(&dir, &["--version"]);
        assert_eq!(
            (code, out.trim()),
            (EXIT_OK, concat!("wrela ", env!("CARGO_PKG_VERSION")))
        );
    }
}
