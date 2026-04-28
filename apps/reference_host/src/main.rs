//! Generic interactive reference host entry (RFC 0011 Phase 70).

fn main() {
    let frames = std::env::var("WRELA_REFERENCE_HOST_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    let project_path = std::env::var("WRELA_REFERENCE_HOST_PROJECT")
        .ok()
        .map(std::path::PathBuf::from);
    let enforce_latency = std::env::var("WRELA_REFERENCE_HOST_ENFORCE_LATENCY").is_ok();
    let save_path = std::env::var("WRELA_REFERENCE_HOST_SAVE_PATH")
        .ok()
        .map(std::path::PathBuf::from);
    let rendered_latency = std::env::var("WRELA_REFERENCE_HOST_RENDERED_LATENCY").is_ok();
    if rendered_latency {
        let Some(project_path) = project_path else {
            eprintln!(
                "error: WRELA_REFERENCE_HOST_RENDERED_LATENCY requires WRELA_REFERENCE_HOST_PROJECT"
            );
            std::process::exit(1);
        };
        match wrela_reference_host::run_rendered_input_to_pixel_for_project(
            project_path,
            frames.unwrap_or(1),
        ) {
            Ok(observation) => {
                eprintln!(
                    "reference-host rendered input-to-pixel: p99_ms={:.3} changed_pixel={}",
                    nanos_to_millis(observation.input_to_pixel_nanos),
                    observation.changed_pixel_linear_index
                );
                let budget_ms = latency_budget_ms();
                if enforce_latency && observation.input_to_pixel_nanos > millis_to_nanos(budget_ms)
                {
                    eprintln!(
                        "error: rendered input-to-pixel exceeded {:.3} ms budget",
                        budget_ms
                    );
                    std::process::exit(1);
                }
            }
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::var("WRELA_REFERENCE_HOST_HEADLESS").is_ok() {
        if let Some(save_path) = save_path {
            let Some(project_path) = project_path else {
                eprintln!(
                    "error: WRELA_REFERENCE_HOST_SAVE_PATH requires WRELA_REFERENCE_HOST_PROJECT"
                );
                std::process::exit(1);
            };
            match wrela_reference_host::run_headless_save_for_project(
                frames.unwrap_or(8),
                project_path,
            ) {
                Ok(record) => {
                    if let Err(err) = wrela::persistence::write_record(&save_path, &record) {
                        eprintln!("error: {err}");
                        std::process::exit(1);
                    }
                    println!(
                        "{{\"command\":\"save\",\"path\":\"{}\",\"project_id\":\"{}\",\"snapshot_epoch\":{},\"sim_tick\":{},\"ledger_records\":{}}}",
                        json_escape(&save_path.display().to_string()),
                        json_escape(&record.header.project_id),
                        record.header.snapshot_epoch,
                        record.header.sim_tick,
                        ledger_record_count(&record)
                    );
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(1);
                }
            }
            return;
        }
        let result = match project_path {
            Some(path) => {
                wrela_reference_host::run_headless_smoke_for_project(frames.unwrap_or(8), path)
            }
            None => wrela_reference_host::run_headless_smoke(frames.unwrap_or(8)),
        };
        match result {
            Ok(reports) => {
                if let Err(err) = report_latency_summary(&reports, enforce_latency) {
                    eprintln!("error: {err}");
                    std::process::exit(1);
                }
            }
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut config = wrela_reference_host::ReferenceHostConfig::default();
    config.frames = frames;
    config.project_path = project_path;
    if let Err(err) = wrela_reference_host::run_interactive(config) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn ledger_record_count(record: &wrela::persistence::SnapshotSaveRecord) -> usize {
    wrela::persistence::decompress_payload(record)
        .map(|payload| payload.ledger.len())
        .unwrap_or(0)
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn report_latency_summary(
    reports: &[wrela::engine_frame::EngineFrameReport],
    enforce_latency: bool,
) -> Result<(), String> {
    let mut samples = reports
        .iter()
        .map(|report| report.latency.total_estimate_nanos)
        .collect::<Vec<_>>();
    samples.sort_unstable();
    let p50 = percentile_nanos(&samples, 0.50);
    let p95 = percentile_nanos(&samples, 0.95);
    let p99 = percentile_nanos(&samples, 0.99);
    eprintln!(
        "reference-host latency: samples={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3}",
        samples.len(),
        nanos_to_millis(p50),
        nanos_to_millis(p95),
        nanos_to_millis(p99)
    );
    if enforce_latency {
        let Some(last) = reports.last() else {
            return Err("latency lane produced no frame reports".to_string());
        };
        let required = [
            wrela::engine_frame::EngineSubsystemKind::StateAdvance,
            wrela::engine_frame::EngineSubsystemKind::Input,
            wrela::engine_frame::EngineSubsystemKind::System,
            wrela::engine_frame::EngineSubsystemKind::Residency,
            wrela::engine_frame::EngineSubsystemKind::Physics,
            wrela::engine_frame::EngineSubsystemKind::Audio,
            wrela::engine_frame::EngineSubsystemKind::Presentation,
        ];
        for kind in required {
            if !last.subsystems.iter().any(|report| report.kind == kind) {
                return Err(format!("latency lane missing subsystem row `{kind:?}`"));
            }
        }
        if p99 == 0 {
            return Err("latency lane produced a zero p99 motion-to-photon estimate".to_string());
        }
        let budget_ms = std::env::var("WRELA_REFERENCE_HOST_LATENCY_BUDGET_MS")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(25.0);
        if nanos_to_millis(p99) > budget_ms {
            return Err(format!(
                "latency lane p99 {:.3} ms exceeds budget {:.3} ms",
                nanos_to_millis(p99),
                budget_ms
            ));
        }
        for report in reports {
            for finding in &report.closure_findings {
                return Err(format!(
                    "closure finding at frame {} [{}:{}]: {}",
                    report.frame_index, finding.subsystem, finding.focus, finding.summary
                ));
            }
        }
    }
    Ok(())
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

fn millis_to_nanos(millis: f64) -> u64 {
    (millis.max(0.0) * 1_000_000.0).round().min(u64::MAX as f64) as u64
}

fn latency_budget_ms() -> f64 {
    std::env::var("WRELA_REFERENCE_HOST_LATENCY_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(25.0)
}
