use super::command_handlers::{build_compile, run_repro_artifact, test_eval_perf};
use super::contracts::{EXIT_USAGE, OutputFormat};
use std::path::Path;

use build_compile::{TestTarget, resolve_budget_policy_v1, resolve_test_target};
use test_eval_perf::{HttpCassetteMode, budget_jobs_timeout};

pub struct ReproCommandInput {
    pub path_arg: Option<String>,
    pub repro_artifact_path: String,
    pub test_record: bool,
    pub test_jobs: Option<usize>,
    pub test_timeout_ms: Option<u64>,
    pub output_format: OutputFormat,
}

pub fn run_repro_command(input: ReproCommandInput) -> i32 {
    let budget_policy = resolve_budget_policy_v1(input.test_jobs, input.test_timeout_ms);
    let (_, timeout) = budget_jobs_timeout(&budget_policy);
    let target = match resolve_test_target(input.path_arg.as_deref()) {
        Ok(target) => target,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };
    let workspace_root = match target {
        TestTarget::ProjectRoot(root) => root,
        TestTarget::SingleFile(path) => path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    };
    run_repro_artifact(
        &workspace_root,
        Path::new(&input.repro_artifact_path),
        timeout,
        input.output_format,
        if input.test_record {
            HttpCassetteMode::Record
        } else {
            HttpCassetteMode::Replay
        },
        &budget_policy,
    )
}
