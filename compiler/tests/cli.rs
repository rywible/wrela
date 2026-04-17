include!("cli/support.rs");

#[path = "cli/help.rs"]
mod help;

#[path = "cli/contracts.rs"]
mod contracts;

#[path = "cli/preview.rs"]
mod preview;

#[path = "cli/diagnostics.rs"]
mod diagnostics;

#[path = "cli/build.rs"]
mod build;

#[path = "cli/maintenance.rs"]
mod maintenance;

#[path = "cli/perf.rs"]
mod perf;

#[path = "cli/test_runner.rs"]
mod test_runner;

#[path = "cli/tooling.rs"]
mod tooling;

#[path = "cli/eval.rs"]
mod eval;
