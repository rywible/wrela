use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_V2_BIN: &str = "/Users/ryanwible/projects/wrela/target/wrela_v2_build/main.wrela.bin";

pub fn try_run_cutover_launcher(raw_args: Vec<String>) -> Result<i32, String> {
    if env::var("WRELA_USE_V1_FALLBACK").ok().as_deref() == Some("1") {
        return try_run_v1_fallback(raw_args);
    }

    if !is_darwin_arm64_host() {
        return Err("error: m10 cutover is darwin-arm64 only".to_string());
    }

    try_run_v2(raw_args)
}

pub fn is_darwin_arm64_host() -> bool {
    cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
}

pub fn resolve_v2_bin_path() -> PathBuf {
    resolve_v2_bin_path_from_value(env::var("WRELA_V2_BIN").ok())
}

fn resolve_v2_bin_path_from_value(v2_bin_value: Option<String>) -> PathBuf {
    v2_bin_value
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_V2_BIN))
}

pub fn try_run_v2(raw_args: Vec<String>) -> Result<i32, String> {
    let v2_bin = resolve_v2_bin_path();
    try_run_v2_with_bin(raw_args, v2_bin)
}

fn try_run_v2_with_bin(raw_args: Vec<String>, v2_bin: PathBuf) -> Result<i32, String> {
    if !v2_bin.exists() {
        return Err(format!(
            "error: v2 toolchain artifact missing: {}",
            v2_bin.display()
        ));
    }

    run_delegated_process(&v2_bin, raw_args)
}

pub fn try_run_v1_fallback(raw_args: Vec<String>) -> Result<i32, String> {
    let v1_bin = resolve_v1_bin_path_from_value(env::var("WRELA_V1_BIN").ok())?;
    run_delegated_process(&v1_bin, raw_args)
}

fn resolve_v1_bin_path_from_value(v1_bin_value: Option<String>) -> Result<PathBuf, String> {
    v1_bin_value
        .map(PathBuf::from)
        .ok_or_else(|| "error: v1 fallback requested but WRELA_V1_BIN is unset".to_string())
}

fn run_delegated_process(binary: &Path, raw_args: Vec<String>) -> Result<i32, String> {
    let status = Command::new(binary)
        .env("WRELA_LAUNCHER_INTERNAL_RUST", "1")
        .args(raw_args)
        .status()
        .map_err(|error| format!("failed to execute {}: {error}", binary.display()))?;

    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_override_path_is_used() {
        assert_eq!(
            resolve_v2_bin_path_from_value(Some("/tmp/wrela-v2-override".to_string())),
            PathBuf::from("/tmp/wrela-v2-override")
        );
    }

    #[test]
    fn v2_missing_artifact_returns_deterministic_error() {
        let result = try_run_v2_with_bin(
            vec!["check".to_string(), "wrela-v2".to_string()],
            PathBuf::from("/tmp/wrela-v2-missing-artifact"),
        );
        match result {
            Ok(_) => panic!("expected missing artifact error"),
            Err(error_text) => {
                assert_eq!(
                    error_text,
                    "error: v2 toolchain artifact missing: /tmp/wrela-v2-missing-artifact"
                );
            }
        }
    }

    #[test]
    fn v1_fallback_requires_bin_env() {
        let result = resolve_v1_bin_path_from_value(None);
        match result {
            Ok(_) => panic!("expected v1 fallback env error"),
            Err(error_text) => {
                assert_eq!(
                    error_text,
                    "error: v1 fallback requested but WRELA_V1_BIN is unset"
                );
            }
        }
    }

    #[test]
    fn v1_fallback_delegates_exit_code() {
        let v1_bin = resolve_v1_bin_path_from_value(Some("/usr/bin/true".to_string()))
            .expect("expected resolved v1 path");
        let result = run_delegated_process(&v1_bin, vec!["check".to_string(), "wrela-v2".to_string()]);
        assert_eq!(result.expect("expected delegated result"), 0);
    }
}
