//! `wrela-service <build-dir> [--root NAME=DIR]... [--memory MIB] [--deadline MS] [-- ARGS...]`:
//! runs a build's server program (`server/game.wasm`) as a service (`wrela_service`), with the
//! directories it may use and its arguments.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use wrela_host::server::ServerBuild;

fn usage() -> ExitCode {
    eprintln!(
        "usage: wrela-service <build-dir> [--root NAME=DIR]... [--memory MIB] [--deadline MS] [-- ARGS...]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let (ours, args) = match all.iter().position(|a| a == "--") {
        Some(i) => (&all[..i], all[i + 1..].to_vec()),
        None => (&all[..], Vec::new()),
    };
    let Some(dir) = ours.first().map(PathBuf::from) else { return usage() };
    let mut roots = wrela_service::files::Roots::default();
    let mut limits = wrela_service::limits();
    let mut i = 1;
    while i < ours.len() {
        let Some(value) = ours.get(i + 1) else { return usage() };
        match ours[i].as_str() {
            "--root" => {
                let Some((name, path)) = value.split_once('=') else { return usage() };
                if let Err(e) = roots.add(name, &PathBuf::from(path)) {
                    eprintln!("wrela-service: {e}");
                    return ExitCode::from(2);
                }
            }
            "--memory" => match value.parse::<usize>() {
                Ok(mib) => limits.memory = mib << 20,
                Err(_) => return usage(),
            },
            "--deadline" => match value.parse::<u64>() {
                Ok(ms) => limits.deadline = Duration::from_millis(ms),
                Err(_) => return usage(),
            },
            _ => return usage(),
        }
        i += 2;
    }
    let build = match ServerBuild::load(&dir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("wrela-service: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = wrela_service::run(wrela_service::Config { build, roots, args, limits });
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
