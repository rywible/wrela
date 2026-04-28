//! `wrela live` — headless engine-frame host (RFC 0011 Phase 63 / C2).

use super::*;
use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, LiveEngineHost, LiveProjectConfig,
};
use wrela::hir::{self, FunctionRole};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, IdentityTransitionEvent, IdentityTransitionKind,
    StateAdvanceResult, WorldTransitionRecord,
};
use wrela::world_identity::WorldSnapshotHandle;

pub(crate) struct CliProjectStateAdvanceExecutor {
    project_label: SmolStr,
    function_count: usize,
    system_count: usize,
}

impl CliProjectStateAdvanceExecutor {
    pub(crate) fn from_project(
        project_label: SmolStr,
        project: &hir::project::LoadedProject,
    ) -> Self {
        let mut function_count = 0usize;
        let mut system_count = 0usize;
        for (_, function) in project.module.functions.iter() {
            function_count += 1;
            if function.role == FunctionRole::System {
                system_count += 1;
            }
        }
        Self {
            project_label,
            function_count,
            system_count,
        }
    }
}

impl wrela::engine_frame::EngineStateAdvanceExecutor for CliProjectStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(wrela::world_identity::SnapshotEpoch(
            previous.epoch().0.saturating_add(1),
        ));
        let event = IdentityTransitionEvent::new(
            IdentityTransitionKind::Preserved,
            self.project_label.clone(),
            Some(format!("{}:{}", previous.capture_name(), previous.epoch().0).into()),
            Some(format!("{}:{}", next.capture_name(), next.epoch().0).into()),
            format!(
                "project-backed live transition; functions={} systems={} inputs={}",
                self.function_count,
                self.system_count,
                input.inputs.inputs.len()
            ),
        );
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                vec![event],
            ),
            ChangeSummary::new(ChangeClass::Behavior, "project-backed live transition"),
        ))
    }
}

pub(crate) fn execute_live_command(args: LiveCommandArgs) {
    if args.options.frames == Some(0) {
        eprintln!("error: --frames must be greater than zero");
        std::process::exit(EXIT_USAGE);
    }

    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };

    if !entry_path.is_file() && !entry_path.is_dir() {
        eprintln!("error: entry path `{}` not found", entry_path.display());
        std::process::exit(EXIT_USAGE);
    }

    // RFC 0011 C2: actually load the project so the live host has a project
    // identity to scope its snapshot handle, and so a malformed entry point
    // is reported up front instead of being silently ignored.
    let loaded_project = match hir::project::load_project_with_entrypoint(&entry_path, false) {
        Ok(project) => project,
        Err(errors) => {
            for err in errors {
                eprintln!("error: {err:?}");
            }
            std::process::exit(EXIT_USAGE);
        }
    };

    let project_label = entry_path
        .file_stem()
        .and_then(|os| os.to_str())
        .map(SmolStr::new)
        .unwrap_or_else(|| SmolStr::new("live_cli_project"));

    if !args.options.headless {
        launch_reference_host(&entry_path, args.options.frames);
    }

    let scenario_id = format!("live_cli:{}", entry_path.display());
    let snapshot: WorldSnapshotHandle = stable_region_snapshot_handle(&project_label);
    let runtime = EngineFrameRuntime::new(Box::new(CliProjectStateAdvanceExecutor::from_project(
        project_label.clone(),
        &loaded_project,
    )));
    let config = LiveProjectConfig {
        scenario_id,
        default_query_requests: Vec::new(),
        simulation_hz_override: None,
    };
    let policy = EngineFrameRuntimePolicy::live();
    // The headless live lane is a deterministic 120 Hz structural gate: there
    // is no real swapchain capability probe here, so use the RFC 0011 stretch
    // cadence instead of a 60 Hz FIFO fallback estimate.
    let sim_hz = 120.0;
    let mut host = LiveEngineHost::new_headless(runtime, config, policy, snapshot, sim_hz);
    let step = 1.0 / sim_hz;

    match args.output_format {
        OutputFormat::Json => {
            // RFC 0011 M8: track previous wall stamp so we can refuse to emit
            // a degenerate (zero / non-advancing) clock, even from the
            // deterministic headless host.
            let mut last_wall_nanos: Option<u64> = None;
            // RFC 0011 M3: when --enforce-latency-budget is on, accumulate
            // Closure findings are the canonical RFC 0011 gate. The flag name
            // remains latency-flavored for CLI compatibility, but any
            // structured live finding should fail the lane.
            let mut closure_findings: u32 = 0;
            let mut latency_samples_nanos: Vec<u64> = Vec::new();
            for _ in 0..args.options.frames.unwrap_or(1) {
                let tick = match host.advance(step) {
                    Ok(t) => t,
                    Err(err) => {
                        eprintln!("error: engine frame failed: {err}");
                        std::process::exit(EXIT_USAGE);
                    }
                };
                for output in tick.outputs {
                    let wall = output.report.identity.wall_clock;
                    if wall == 0 {
                        eprintln!(
                            "error: synth clock produced a zero wall stamp at frame {}",
                            output.report.frame_index
                        );
                        std::process::exit(EXIT_USAGE);
                    }
                    if let Some(prev) = last_wall_nanos
                        && wall <= prev
                    {
                        eprintln!(
                            "error: synth clock did not advance: previous={prev} current={wall}"
                        );
                        std::process::exit(EXIT_USAGE);
                    }
                    last_wall_nanos = Some(wall);
                    if args.options.enforce_latency_budget {
                        latency_samples_nanos.push(output.report.latency.total_estimate_nanos);
                        for finding in &output.report.closure_findings {
                            eprintln!(
                                "error: closure finding at frame {} [{}:{}]: {}",
                                output.report.frame_index,
                                finding.subsystem,
                                finding.focus,
                                finding.summary
                            );
                            closure_findings = closure_findings.saturating_add(1);
                        }
                    }
                    match serde_json::to_string(&output.report) {
                        Ok(line) => println!("{line}"),
                        Err(err) => {
                            eprintln!("error: serialize EngineFrameReport: {err}");
                            std::process::exit(EXIT_USAGE);
                        }
                    }
                }
            }
            if args.options.enforce_latency_budget {
                latency_samples_nanos.sort_unstable();
                let p50 = percentile_nanos(&latency_samples_nanos, 0.50);
                let p95 = percentile_nanos(&latency_samples_nanos, 0.95);
                let p99 = percentile_nanos(&latency_samples_nanos, 0.99);
                eprintln!(
                    "live latency: samples={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3}",
                    latency_samples_nanos.len(),
                    nanos_to_millis(p50),
                    nanos_to_millis(p95),
                    nanos_to_millis(p99)
                );
            }
            if args.options.enforce_latency_budget && closure_findings > 0 {
                eprintln!(
                    "error: --enforce-latency-budget failed: {closure_findings} closure findings"
                );
                std::process::exit(EXIT_USAGE);
            }
        }
        OutputFormat::Pretty | OutputFormat::Sarif => {
            eprintln!("error: wrela live --headless supports --json only");
            std::process::exit(EXIT_USAGE);
        }
    }

    std::process::exit(EXIT_OK);
}

pub(crate) fn execute_perf_latency_command(mut args: PerfLatencyCommandArgs) {
    args.options.enforce_latency_budget = true;
    if args.options.frames.is_none() {
        args.options.frames = Some(600);
    }
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    launch_reference_host_perf_latency(&entry_path, args.options.frames);
}

fn percentile_nanos(sorted_samples: &[u64], percentile: f64) -> u64 {
    if sorted_samples.is_empty() {
        return 0;
    }
    let clamped = percentile.clamp(0.0, 1.0);
    let idx = ((sorted_samples.len() - 1) as f64 * clamped).round() as usize;
    sorted_samples[idx]
}

fn nanos_to_millis(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000.0
}

fn launch_reference_host(entry_path: &std::path::Path, frames: Option<u32>) -> ! {
    let mut command = std::process::Command::new("cargo");
    command.args(["run", "-p", "wrela_reference_host", "--"]);
    command.env("WRELA_REFERENCE_HOST_PROJECT", entry_path.as_os_str());
    if let Some(frames) = frames {
        command.env("WRELA_REFERENCE_HOST_FRAMES", frames.to_string());
    }
    if std::env::var_os("WRELA_TEST_OFFSCREEN").is_some() {
        command.env("WRELA_TEST_OFFSCREEN", "1");
        command.env("WRELA_REFERENCE_HOST_HEADLESS", "1");
    }
    let status = match command.status() {
        Ok(status) => status,
        Err(err) => {
            eprintln!("error: launch reference host: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    std::process::exit(status.code().unwrap_or(EXIT_USAGE));
}

fn launch_reference_host_perf_latency(entry_path: &std::path::Path, frames: Option<u32>) -> ! {
    let mut command = std::process::Command::new("cargo");
    command.args(["run", "-p", "wrela_reference_host", "--"]);
    command.env("WRELA_REFERENCE_HOST_PROJECT", entry_path.as_os_str());
    command.env("WRELA_REFERENCE_HOST_RENDERED_LATENCY", "1");
    command.env("WRELA_REFERENCE_HOST_ENFORCE_LATENCY", "1");
    if let Some(frames) = frames {
        command.env("WRELA_REFERENCE_HOST_FRAMES", frames.to_string());
    }
    let status = match command.status() {
        Ok(status) => status,
        Err(err) => {
            eprintln!("error: failed to launch reference host: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    match status.code() {
        Some(code) => std::process::exit(code),
        None => std::process::exit(EXIT_USAGE),
    }
}
