//! The `wrela` binary. All the behavior is in the library, so tests run it in-process.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let code = wrela_cli::run(&args, &cwd, &mut stdout, &mut stderr);
    ExitCode::from(code)
}
