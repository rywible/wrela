use std::{
    env, fs,
    io::{self, Read},
    process::ExitCode,
};
use wrela_frontend::{inspection::Inspection, parse};

fn run() -> Result<bool, String> {
    let mut arguments = env::args().skip(1);
    let mut json = false;
    let mut filename = None;
    for argument in arguments.by_ref() {
        match argument.as_str() {
            "--json" => json = true,
            "--help" | "-h" => {
                println!(
                    "Usage: wrela_frontend [--json] [SOURCE|-]\nInspect typed Wrela syntax. Read stdin when SOURCE is absent or '-'.\nExit 0: syntax eligible; 1: rejected source; 2: I/O/usage error."
                );
                return Ok(true);
            }
            _ if filename.is_none() => filename = Some(argument),
            _ => return Err("expected at most one source file".into()),
        }
    }
    let bytes = match filename.as_deref() {
        None | Some("-") => {
            let mut bytes = Vec::new();
            io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            bytes
        }
        Some(path) => fs::read(path).map_err(|error| format!("{path}: {error}"))?,
    };
    let document = parse(&bytes);
    if json {
        let inspection = Inspection::from(&document);
        println!(
            "{}",
            serde_json::to_string_pretty(&inspection).map_err(|error| error.to_string())?
        );
    } else {
        for diagnostic in &document.diagnostics {
            eprintln!(
                "{:?} {}..{}: {}",
                diagnostic.code, diagnostic.range.start, diagnostic.range.end, diagnostic.message
            );
        }
        println!(
            "{}: {} declaration(s), {} byte(s), {} diagnostic(s)",
            if document.is_syntax_eligible() {
                "syntax eligible"
            } else {
                "syntax rejected"
            },
            document.syntax.declarations.len(),
            document.source.len(),
            document.diagnostics.len()
        );
    }
    Ok(document.is_syntax_eligible())
}
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
