//! Owns perf closure verdict construction and the "why not 120" diagnostic lane.
//! Does not own benchmark execution, raw report collection, or CLI parsing.
//!
//! Key invariants:
//! - closure verdicts fail closed on violated/measured lanes instead of
//!   downgrading missing evidence into a harmless-looking success.
//! - scenario classification and bottleneck ranking must use the same typed
//!   scenario identity that report collection used.
//! - human-readable summaries are derived from the same lane status model that
//!   machine-readable verdicts consume.
//!
//! Primary entrypoints:
//! - `build_closure_report`
//! - `build_closure_verdict`
//! - `explain_collision_why_not_120_findings`
//!
//! Failure modes / common pitfalls:
//! - treating missing sampled lanes as "met" or "not applicable" when evidence
//!   collection failed makes roadmap closure dishonest.
//! - mixing scenario labels with scenario identity can rank the wrong bottleneck
//!   or attach findings to the wrong report.

use super::*;

fn state_advance_runtime_is_sampled(report: &EngineFrameBenchmarkReport) -> bool {
    report
        .subsystem_reports
        .iter()
        .find(|subsystem| subsystem.kind == wrela::engine_frame::EngineSubsystemKind::StateAdvance)
        .map(|subsystem| {
            !subsystem.notes.iter().any(|note| {
                note == "reserved-slot-unsampled" || note == "compatibility-slot-unsampled"
            })
        })
        .unwrap_or(report.state_advance_runtime_ns > 0)
}

fn collision_gpu_critical_path_uses_runtime_proxy(report: &EngineFrameBenchmarkReport) -> bool {
    report
        .subsystem_reports
        .iter()
        .find(|subsystem| subsystem.kind == wrela::engine_frame::EngineSubsystemKind::Collision)
        .map(|subsystem| {
            subsystem
                .notes
                .iter()
                .any(|note| note == "gpu_critical_path_proxy=runtime_ns")
        })
        .unwrap_or(false)
}

fn engine_frame_report_uses_timestamp_traffic(report: &EngineFrameBenchmarkReport) -> bool {
    report.timestamped_pass_count > 0
        || report.timing_readback_bytes > 0
        || report.measurement_policy.gpu_timing
            == wrela::engine_frame::EngineGpuTimingPolicy::Timestamped
        || report.subsystem_reports.iter().any(|subsystem| {
            subsystem.timestamped_pass_count > 0
                || subsystem.timing_readback_bytes > 0
                || subsystem.measurement_policy.gpu_timing
                    == wrela::engine_frame::EngineGpuTimingPolicy::Timestamped
        })
}

fn engine_frame_report_uses_hot_path_readback(report: &EngineFrameBenchmarkReport) -> bool {
    report.hot_path_readback_bytes > 0
        || report.measurement_policy.hot_path_readback_allowed
        || report.subsystem_reports.iter().any(|subsystem| {
            subsystem.hot_path_readback_bytes > 0
                || subsystem.measurement_policy.hot_path_readback_allowed
        })
}

fn engine_frame_report_uses_export_readback(report: &EngineFrameBenchmarkReport) -> bool {
    report.measurement_policy.export_readback_allowed
        || report
            .subsystem_reports
            .iter()
            .any(|subsystem| subsystem.measurement_policy.export_readback_allowed)
}

fn engine_frame_report_uses_non_authoritative_runtime(
    report: &EngineFrameBenchmarkReport,
) -> Vec<String> {
    let mut violations = Vec::new();
    if matches!(
        report.measurement_policy.runtime_source,
        wrela::engine_frame::EngineRuntimeSource::CompatibilityJoin
            | wrela::engine_frame::EngineRuntimeSource::ReservedSlotUnsampled
    ) {
        violations.push(format!(
            "scenario '{}' engine-frame report uses non-authoritative runtime_source={}",
            report.scenario_id,
            engine_runtime_source_name(report.measurement_policy.runtime_source)
        ));
    }
    for subsystem in &report.subsystem_reports {
        match (
            &subsystem.kind,
            subsystem.measurement_policy.runtime_source,
        ) {
            (
                wrela::engine_frame::EngineSubsystemKind::StateAdvance,
                wrela::engine_frame::EngineRuntimeSource::ReservedSlotUnsampled,
            ) => violations.push(format!(
                "scenario '{}' uses reserved state_advance instead of a kernel-owned transition",
                report.scenario_id
            )),
            (
                _,
                wrela::engine_frame::EngineRuntimeSource::CompatibilityJoin,
            ) => violations.push(format!(
                "scenario '{}' uses compatibility-joined subsystem '{}' in canonical engine-frame closure",
                report.scenario_id, subsystem.label
            )),
            (
                _,
                wrela::engine_frame::EngineRuntimeSource::ReservedSlotUnsampled,
            ) => violations.push(format!(
                "scenario '{}' uses reserved unsampled subsystem '{}' in canonical engine-frame closure",
                report.scenario_id, subsystem.label
            )),
            _ => {}
        }
    }
    violations
}

pub(super) fn frame_cost_total_ns(
    report: &wrela::presentation_exec::PresentationFrameCostReport,
) -> u128 {
    report
        .passes
        .iter()
        .map(|pass| pass.elapsed_micros)
        .sum::<u128>()
        * 1_000
}

pub(super) fn build_closure_report(
    profile: &PerfClosureProfile,
    manifest: Option<&BenchmarkManifest>,
    collision_samples: &[test_eval_perf::PerfSummary],
    presentation_reports: &[PresentationBenchmarkReport],
    presentation_report_errors: &[String],
    whole_frame_reports: &[WholeFrameBenchmarkReport],
    whole_frame_report_errors: &[String],
    engine_frame_reports: &[EngineFrameBenchmarkReport],
    engine_frame_report_errors: &[String],
    collision_reports: &[CollisionBenchmarkReport],
    collision_report_errors: &[String],
    perf_profile: PerfProfile,
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureReport {
    let sampled_suite = if matches!(perf_profile, PerfProfile::Closure1080p120) {
        manifest.map(|manifest| manifest.suite.as_str())
    } else {
        None
    };
    let mut report = PerfClosureReport::unsampled(profile.clone());
    apply_observed_wgsl_runtime_metadata(&mut report.profile, presentation_reports);
    report.cpu_oracle_profile = Some(match profile.execution_story {
        PerfClosureExecutionStory::CpuOracle => {
            PerfClosureProfile::canonical_1080p120_wgsl_resident()
        }
        PerfClosureExecutionStory::WgslResident => {
            PerfClosureProfile::canonical_1080p120_cpu_oracle()
        }
    });
    if sampled_suite.is_some_and(|suite| closure_suite_matches(suite, profile.frame.suite.as_str()))
    {
        report.frame = build_frame_closure_status(
            profile,
            sampled_suite,
            presentation_reports,
            presentation_report_errors,
            whole_frame_reports,
            whole_frame_report_errors,
            engine_frame_reports,
            engine_frame_report_errors,
            observed_warmup_runs,
            observed_measured_runs,
        );
    }
    if sampled_suite
        .is_some_and(|suite| closure_suite_matches(suite, profile.collision.suite.as_str()))
    {
        report.collision = build_collision_closure_status(
            profile,
            collision_samples,
            collision_reports,
            collision_report_errors,
            observed_warmup_runs,
            observed_measured_runs,
        );
        report.engine_frame = build_engine_frame_closure_status(
            profile,
            engine_frame_reports,
            engine_frame_report_errors,
            observed_warmup_runs,
            observed_measured_runs,
        );
    }
    report.verdict = build_closure_verdict(
        profile,
        &report.frame,
        &report.collision,
        &report.engine_frame,
        presentation_reports,
        collision_reports,
    );
    report
}

fn is_composite_engine_frame_suite(suite: &str) -> bool {
    suite.eq_ignore_ascii_case("engine_frame") || suite.eq_ignore_ascii_case("whole_frame")
}

fn closure_suite_matches(sampled_suite: &str, expected_suite: &str) -> bool {
    sampled_suite.eq_ignore_ascii_case(expected_suite)
        || (is_composite_engine_frame_suite(sampled_suite)
            && is_composite_engine_frame_suite(expected_suite))
}

pub(super) fn build_engine_frame_closure_status(
    profile: &PerfClosureProfile,
    engine_frame_reports: &[EngineFrameBenchmarkReport],
    engine_frame_report_errors: &[String],
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> wrela::perf_target::PerfClosureEngineFrameStatusReport {
    let mut report = wrela::perf_target::PerfClosureEngineFrameStatusReport::unsampled();
    let mut violations = engine_frame_report_errors.to_vec();
    if observed_warmup_runs != profile.warmup_runs as usize
        || observed_measured_runs != profile.measured_runs as usize
    {
        violations.push(format!(
            "observed run protocol warmup={} measured={} does not match canonical warmup={} measured={}",
            observed_warmup_runs,
            observed_measured_runs,
            profile.warmup_runs,
            profile.measured_runs
        ));
    }
    if engine_frame_reports.is_empty() {
        if !violations.is_empty() {
            report.status = PerfClosureLaneStatus::Violated;
            report.notes = violations;
        }
        return report;
    }

    report.notes.clear();
    report.status = if violations.is_empty() {
        PerfClosureLaneStatus::Sampled
    } else {
        PerfClosureLaneStatus::Violated
    };
    let frame_wall_times = engine_frame_reports
        .iter()
        .map(|report| ns_to_ms(report.frame_wall_time_ns))
        .collect::<Vec<_>>();
    let cpu_critical_paths = engine_frame_reports
        .iter()
        .map(|report| ns_to_ms(report.cpu_critical_path_ns))
        .collect::<Vec<_>>();
    let gpu_critical_paths = engine_frame_reports
        .iter()
        .filter_map(|report| report.gpu_critical_path_ns.map(ns_to_ms))
        .collect::<Vec<_>>();
    let presentation_times = engine_frame_reports
        .iter()
        .map(|report| ns_to_ms(report.presentation_runtime_ns))
        .collect::<Vec<_>>();
    let collision_times = engine_frame_reports
        .iter()
        .map(|report| ns_to_ms(report.collision_runtime_ns))
        .collect::<Vec<_>>();
    let state_advance_times = engine_frame_reports
        .iter()
        .filter(|report| state_advance_runtime_is_sampled(report))
        .map(|report| ns_to_ms(report.state_advance_runtime_ns))
        .collect::<Vec<_>>();
    let future_reserve = engine_frame_reports
        .iter()
        .map(|report| ns_to_ms(report.future_subsystem_reserve_ns))
        .collect::<Vec<_>>();
    report.frame_wall_time_median_ms = percentile_f32(&frame_wall_times, 0.50);
    report.frame_wall_time_p95_ms = percentile_f32(&frame_wall_times, 0.95);
    report.cpu_critical_path_median_ms = percentile_f32(&cpu_critical_paths, 0.50);
    report.gpu_critical_path_median_ms = percentile_f32(&gpu_critical_paths, 0.50);
    report.presentation_median_ms = percentile_f32(&presentation_times, 0.50);
    report.collision_median_ms = percentile_f32(&collision_times, 0.50);
    report.state_advance_median_ms = percentile_f32(&state_advance_times, 0.50);
    report.future_subsystem_reserve_ms = percentile_f32(&future_reserve, 0.50);
    report.queue_submit_count = Some(
        engine_frame_reports
            .iter()
            .map(|report| report.queue_submit_count)
            .max()
            .unwrap_or(0),
    );
    report.hot_path_readback_bytes = Some(
        engine_frame_reports
            .iter()
            .map(|report| report.hot_path_readback_bytes)
            .max()
            .unwrap_or(0),
    );
    report.scene_reupload_bytes = Some(
        engine_frame_reports
            .iter()
            .map(|report| report.scene_reupload_bytes)
            .max()
            .unwrap_or(0),
    );
    report.active_degradations = engine_frame_reports
        .iter()
        .flat_map(|report| report.active_degradations.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    report.violations = engine_frame_reports
        .iter()
        .flat_map(|report| report.violations.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    violations.extend(report.violations.iter().cloned());
    report.notes.push(format!(
        "engine-frame reports collected for {} scenario(s)",
        engine_frame_reports.len()
    ));
    if state_advance_times.is_empty() {
        report.notes.push(
            "state_advance runtime remains unsampled; reserve is accounted separately".to_string(),
        );
    }
    if !profile.timestamps_enabled {
        let timestamped_scenarios = engine_frame_reports
            .iter()
            .filter(|report| engine_frame_report_uses_timestamp_traffic(report))
            .map(|report| report.scenario_id.to_string())
            .collect::<Vec<_>>();
        if !timestamped_scenarios.is_empty() {
            violations.push(format!(
                "engine-frame timestamp traffic is disabled by the closure profile but observed in scenario(s): {}",
                timestamped_scenarios.join(", ")
            ));
        }
    }
    if profile.gpu_timestamps_required_if_supported
        && profile
            .enabled_optional_features
            .iter()
            .any(|feature| feature == "timestamp_query")
    {
        let missing_timestamp_scenarios = engine_frame_reports
            .iter()
            .filter(|report| {
                report.subsystem_reports.iter().any(|subsystem| {
                    let timestamp_required = match subsystem.kind {
                        wrela::engine_frame::EngineSubsystemKind::Presentation => true,
                        wrela::engine_frame::EngineSubsystemKind::Collision => {
                            subsystem.queue_submit_count > 0
                                || subsystem.gpu_critical_path_micros.is_some()
                                || !matches!(
                                    subsystem.measurement_policy.gpu_timing,
                                    wrela::engine_frame::EngineGpuTimingPolicy::Disabled
                                )
                        }
                        _ => false,
                    };
                    timestamp_required && subsystem.timestamped_pass_count == 0
                })
            })
            .map(|report| report.scenario_id.to_string())
            .collect::<Vec<_>>();
        if !missing_timestamp_scenarios.is_empty() {
            violations.push(format!(
                "engine-frame timestamp traffic was requested and the adapter reported timestamp_query support, but no timestamped presentation/collision pass was observed in scenario(s): {}",
                missing_timestamp_scenarios.join(", ")
            ));
        }
    }
    let hot_path_readback_scenarios = engine_frame_reports
        .iter()
        .filter(|report| engine_frame_report_uses_hot_path_readback(report))
        .map(|report| report.scenario_id.to_string())
        .collect::<Vec<_>>();
    if !hot_path_readback_scenarios.is_empty() && profile.max_hot_path_readback_bytes_per_frame == 0
    {
        violations.push(format!(
            "engine-frame hot-path readback is disabled by the closure profile but observed in scenario(s): {}",
            hot_path_readback_scenarios.join(", ")
        ));
    }
    let export_readback_scenarios = engine_frame_reports
        .iter()
        .filter(|report| engine_frame_report_uses_export_readback(report))
        .map(|report| report.scenario_id.to_string())
        .collect::<Vec<_>>();
    if !export_readback_scenarios.is_empty() {
        violations.push(format!(
            "engine-frame export/readback is disabled by the closure profile but observed in scenario(s): {}",
            export_readback_scenarios.join(", ")
        ));
    }
    if engine_frame_reports
        .iter()
        .any(collision_gpu_critical_path_uses_runtime_proxy)
    {
        report.notes.push(
            "collision gpu critical path uses runtime proxy until per-subsystem collision gpu timing exists"
                .to_string(),
        );
    }
    for authority_violation in engine_frame_reports
        .iter()
        .flat_map(engine_frame_report_uses_non_authoritative_runtime)
    {
        violations.push(authority_violation);
    }
    if let Some(frame_wall_time_median_ms) = report.frame_wall_time_median_ms
        && frame_wall_time_median_ms > profile.engine_frame_budget.frame_wall_time_median_ms
    {
        violations.push(format!(
            "engine frame median {:.2} ms exceeds budget {:.2} ms",
            frame_wall_time_median_ms, profile.engine_frame_budget.frame_wall_time_median_ms
        ));
    }
    if let Some(frame_wall_time_p95_ms) = report.frame_wall_time_p95_ms
        && frame_wall_time_p95_ms > profile.engine_frame_budget.frame_wall_time_p95_ms
    {
        violations.push(format!(
            "engine frame p95 {:.2} ms exceeds budget {:.2} ms",
            frame_wall_time_p95_ms, profile.engine_frame_budget.frame_wall_time_p95_ms
        ));
    }
    if let Some(presentation_median_ms) = report.presentation_median_ms
        && presentation_median_ms > profile.engine_frame_budget.presentation_median_ms
    {
        violations.push(format!(
            "engine-frame presentation median {:.2} ms exceeds budget {:.2} ms",
            presentation_median_ms, profile.engine_frame_budget.presentation_median_ms
        ));
    }
    if let Some(collision_median_ms) = report.collision_median_ms
        && collision_median_ms > profile.engine_frame_budget.collision_median_ms
    {
        violations.push(format!(
            "engine-frame collision median {:.2} ms exceeds budget {:.2} ms",
            collision_median_ms, profile.engine_frame_budget.collision_median_ms
        ));
    }
    if let Some(state_advance_median_ms) = report.state_advance_median_ms
        && state_advance_median_ms > profile.engine_frame_budget.state_advance_median_ms
    {
        violations.push(format!(
            "engine-frame state_advance median {:.2} ms exceeds budget {:.2} ms",
            state_advance_median_ms, profile.engine_frame_budget.state_advance_median_ms
        ));
    }
    if let Some(future_subsystem_reserve_ms) = report.future_subsystem_reserve_ms
        && future_subsystem_reserve_ms < profile.engine_frame_budget.future_subsystem_reserve_ms
    {
        violations.push(format!(
            "engine-frame reserve {:.2} ms is below the required future subsystem reserve {:.2} ms",
            future_subsystem_reserve_ms, profile.engine_frame_budget.future_subsystem_reserve_ms
        ));
    }
    if let Some(queue_submit_count) = report.queue_submit_count
        && queue_submit_count > profile.engine_frame_budget.max_queue_submit_count_per_frame
    {
        violations.push(format!(
            "engine-frame queue submit count {} exceeds budget {}",
            queue_submit_count, profile.engine_frame_budget.max_queue_submit_count_per_frame
        ));
    }
    if let Some(hot_path_readback_bytes) = report.hot_path_readback_bytes
        && hot_path_readback_bytes
            > profile
                .engine_frame_budget
                .max_hot_path_readback_bytes_per_frame
    {
        violations.push(format!(
            "engine-frame hot-path readback {} byte(s) exceeds budget {} byte(s)",
            hot_path_readback_bytes,
            profile
                .engine_frame_budget
                .max_hot_path_readback_bytes_per_frame
        ));
    }
    if violations.is_empty() {
        report.status = PerfClosureLaneStatus::Validated;
        report
            .notes
            .push("engine-frame closure met the canonical 1080p120 contract".to_string());
    } else {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.extend(violations);
    }
    report
}

pub(super) fn apply_observed_wgsl_runtime_metadata(
    profile: &mut PerfClosureProfile,
    presentation_reports: &[PresentationBenchmarkReport],
) {
    if !matches!(
        profile.execution_story,
        PerfClosureExecutionStory::WgslResident
    ) {
        return;
    }
    let wgsl_reports = presentation_reports
        .iter()
        .filter(|report| report.backend.eq_ignore_ascii_case("wgsl"))
        .collect::<Vec<_>>();
    if wgsl_reports.is_empty() {
        return;
    }
    if let Some(adapter_name) = wgsl_reports
        .iter()
        .filter_map(|report| report.observed_adapter_name.as_ref())
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
    {
        profile.adapter_name = adapter_name.to_string();
    }
    if let Some(requested_limits_profile) = wgsl_reports
        .iter()
        .map(|report| {
            report
                .frame_cost
                .gpu_runtime
                .requested_limits_profile
                .trim()
        })
        .find(|profile_name| !profile_name.is_empty())
    {
        profile.requested_limits_profile = requested_limits_profile.to_string();
    }
    profile.timestamps_enabled = wgsl_reports
        .iter()
        .any(|report| report.frame_cost.gpu_runtime.timestamps_supported);
    profile.enabled_optional_features = wgsl_reports
        .iter()
        .flat_map(|report| {
            report
                .frame_cost
                .gpu_runtime
                .enabled_optional_features
                .iter()
                .cloned()
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    profile.f16_enabled = profile
        .enabled_optional_features
        .iter()
        .any(|feature| feature == "shader_f16");
    profile.indirect_dispatch_enabled = profile
        .enabled_optional_features
        .iter()
        .any(|feature| feature == "indirect_dispatch");
}

pub(super) fn build_frame_closure_status(
    profile: &PerfClosureProfile,
    sampled_suite: Option<&str>,
    presentation_reports: &[PresentationBenchmarkReport],
    presentation_report_errors: &[String],
    whole_frame_reports: &[WholeFrameBenchmarkReport],
    whole_frame_report_errors: &[String],
    engine_frame_reports: &[EngineFrameBenchmarkReport],
    engine_frame_report_errors: &[String],
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureLaneStatusReport {
    let mut report = PerfClosureLaneStatusReport::unsampled(&profile.frame);
    report.status = PerfClosureLaneStatus::Sampled;
    report.notes.clear();
    let sampled_suite = sampled_suite.unwrap_or(profile.frame.suite.as_str());
    let whole_frame_suite = sampled_suite.eq_ignore_ascii_case("whole_frame");
    let engine_frame_suite = sampled_suite.eq_ignore_ascii_case("engine_frame");
    let composite_frame_suite = is_composite_engine_frame_suite(sampled_suite);
    let mut violations = presentation_report_errors
        .iter()
        .map(|error| format!("presentation report collection failed: {error}"))
        .collect::<Vec<_>>();
    if whole_frame_suite {
        violations.extend(
            whole_frame_report_errors
                .iter()
                .map(|error| format!("whole-frame report collection failed: {error}")),
        );
    }
    if engine_frame_suite {
        violations.extend(
            engine_frame_report_errors
                .iter()
                .map(|error| format!("engine-frame report collection failed: {error}")),
        );
    }
    if presentation_reports.is_empty() {
        if violations.is_empty() {
            report.notes.push(
                "frame closure suite ran without presentation frame-cost reports".to_string(),
            );
        } else {
            report.status = PerfClosureLaneStatus::Violated;
            report.notes.extend(violations);
        }
        return report;
    }

    let legal_degradations = profile
        .legal_degradations
        .iter()
        .map(|step| quality_degradation_step_name(*step))
        .collect::<std::collections::BTreeSet<_>>();
    let mut total_ms = Vec::new();
    let mut primary_ms = Vec::new();
    let mut scales = Vec::new();
    let mut reconstructed_output_detected = false;
    let mut output_width = None;
    let mut output_height = None;
    let mut observed_backends = std::collections::BTreeSet::new();
    let mut active_acceleration_artifacts = std::collections::BTreeSet::new();
    let mut active_degradations = std::collections::BTreeSet::new();
    let mut bottleneck_counts: HashMap<String, usize> = HashMap::new();
    let mut hot_path_readback_bytes = 0u64;
    let mut scene_reupload_bytes = 0u64;
    let mut cpu_screen_sample_allocations = 0u32;
    let mut attachment_cpu_bounce_count = 0u32;
    let mut queue_submit_count = 0u32;
    let mut primary_visibility_dispatch_count = 0u32;
    let mut timestamps_supported = false;
    let mut timestamped_pass_count = 0u32;
    let expected_backend = profile.backend.as_str();
    let whole_frame_by_scenario = whole_frame_reports
        .iter()
        .map(|report| (report.scenario_id.as_str(), report))
        .collect::<HashMap<_, _>>();

    if observed_warmup_runs != profile.warmup_runs as usize
        || observed_measured_runs != profile.measured_runs as usize
    {
        violations.push(format!(
            "observed run protocol warmup={} measured={} does not match canonical warmup={} measured={}",
            observed_warmup_runs,
            observed_measured_runs,
            profile.warmup_runs,
            profile.measured_runs
        ));
    }

    for sample in presentation_reports {
        observed_backends.insert(sample.backend.clone());
        if !sample.backend.eq_ignore_ascii_case(expected_backend) {
            violations.push(format!(
                "scenario '{}' reported backend '{}' instead of closure backend '{}'",
                sample.scenario_id, sample.backend, expected_backend
            ));
        }
        let measured_frame_costs =
            presentation_frame_history(&sample.frame_cost, &sample.frame_cost_history);
        let frame_costs = measured_frame_costs.iter().collect::<Vec<_>>();
        let composite_total_ms = if engine_frame_suite {
            engine_frame_reports
                .iter()
                .find(|report| report.scenario_id == sample.scenario_id)
                .map(|report| ns_to_ms(report.frame_wall_time_ns))
                .or_else(|| {
                    violations.push(format!(
                        "engine-frame timing missing for scenario '{}'",
                        sample.scenario_id
                    ));
                    None
                })
        } else if whole_frame_suite {
            whole_frame_by_scenario
                .get(sample.scenario_id.as_str())
                .map(|report| {
                    ns_to_ms(
                        report
                            .total_runtime_ns
                            .checked_div(u128::from(sample.frames_executed.max(1)))
                            .unwrap_or(report.total_runtime_ns),
                    )
                })
                .or_else(|| {
                    violations.push(format!(
                        "whole-frame timing missing for scenario '{}'",
                        sample.scenario_id
                    ));
                    None
                })
        } else {
            None
        };
        for frame_cost in frame_costs {
            total_ms.push(
                composite_total_ms.unwrap_or_else(|| ns_to_ms(frame_cost_total_ns(frame_cost))),
            );
            if let Some(primary_pass_ms) = primary_visibility_pass_ms(frame_cost) {
                primary_ms.push(primary_pass_ms);
            }
            scales.push(frame_cost.quality.internal_resolution_scale);
            reconstructed_output_detected |= frame_cost.quality.reconstructed_output;
            output_width = Some(frame_cost.output_width);
            output_height = Some(frame_cost.output_height);

            if frame_cost.output_width != profile.output_width
                || frame_cost.output_height != profile.output_height
            {
                violations.push(format!(
                    "scenario '{}' observed output {}x{} does not match closure target {}x{}",
                    sample.scenario_id,
                    frame_cost.output_width,
                    frame_cost.output_height,
                    profile.output_width,
                    profile.output_height
                ));
            }
            if frame_cost.quality.tier != "realtime_120" {
                violations.push(format!(
                    "scenario '{}' reported quality tier '{}' instead of realtime_120",
                    sample.scenario_id, frame_cost.quality.tier
                ));
            }
            if frame_cost.quality.internal_resolution_scale < profile.min_internal_resolution_scale
            {
                violations.push(format!(
                    "scenario '{}' observed internal scale {:.2} below floor {:.2}",
                    sample.scenario_id,
                    frame_cost.quality.internal_resolution_scale,
                    profile.min_internal_resolution_scale
                ));
            }
            for degradation in &frame_cost.quality.active_degradations {
                active_degradations.insert(degradation.clone());
                if !legal_degradations.contains(degradation.as_str()) {
                    violations.push(format!(
                        "scenario '{}' used undeclared degradation '{}'",
                        sample.scenario_id, degradation
                    ));
                }
            }
            for artifact in &frame_cost.active_acceleration_artifacts {
                active_acceleration_artifacts.insert(artifact.clone());
            }
            if let Some(bottleneck) = &frame_cost.bottleneck_pass {
                *bottleneck_counts.entry(bottleneck.clone()).or_insert(0) += 1;
            }

            let frame_timestamped_pass_count = frame_cost.gpu_runtime.timestamped_pass_count;
            let frame_hot_path_readback_bytes = hot_path_readback_bytes_without_timestamp_traffic(
                frame_cost.gpu_runtime.readback_bytes,
                frame_cost.gpu_runtime.timestamps_supported,
                frame_timestamped_pass_count,
            );
            hot_path_readback_bytes = hot_path_readback_bytes.max(frame_hot_path_readback_bytes);
            scene_reupload_bytes =
                scene_reupload_bytes.max(frame_cost.gpu_runtime.scene_reupload_bytes);
            cpu_screen_sample_allocations = cpu_screen_sample_allocations
                .max(frame_cost.gpu_runtime.cpu_screen_sample_allocations);
            attachment_cpu_bounce_count = attachment_cpu_bounce_count.max(
                frame_cost
                    .gpu_runtime
                    .attachment_decode_count
                    .saturating_add(frame_cost.gpu_runtime.attachment_encode_count),
            );
            queue_submit_count = queue_submit_count.max(frame_cost.gpu_runtime.queue_submit_count);
            primary_visibility_dispatch_count = primary_visibility_dispatch_count.max(
                frame_cost
                    .gpu_runtime
                    .primary_visibility_packet_fanout_count,
            );
            timestamps_supported |= frame_cost.gpu_runtime.timestamps_supported;
            timestamped_pass_count = timestamped_pass_count.max(frame_timestamped_pass_count);

            if frame_hot_path_readback_bytes > profile.max_hot_path_readback_bytes_per_frame {
                violations.push(format!(
                    "scenario '{}' still has {} byte(s) of hot-path readback after subtracting timestamp traffic; per-frame budget is {} byte(s)",
                    sample.scenario_id,
                    frame_hot_path_readback_bytes,
                    profile.max_hot_path_readback_bytes_per_frame
                ));
            }
            if frame_cost.gpu_runtime.scene_reupload_bytes
                > profile.max_scene_reupload_bytes_per_frame
            {
                violations.push(format!(
                    "scenario '{}' reuploaded {} byte(s) of resident scene data; per-frame budget is {} byte(s)",
                    sample.scenario_id,
                    frame_cost.gpu_runtime.scene_reupload_bytes,
                    profile.max_scene_reupload_bytes_per_frame
                ));
            }
            if frame_cost.gpu_runtime.cpu_screen_sample_allocations
                > profile.max_cpu_screen_sample_allocations_per_frame
            {
                violations.push(format!(
                    "scenario '{}' still allocates {} CPU screen sample(s); per-frame budget is {}",
                    sample.scenario_id,
                    frame_cost.gpu_runtime.cpu_screen_sample_allocations,
                    profile.max_cpu_screen_sample_allocations_per_frame
                ));
            }
            if frame_cost
                .gpu_runtime
                .attachment_decode_count
                .saturating_add(frame_cost.gpu_runtime.attachment_encode_count)
                > profile.max_attachment_cpu_bounce_count
            {
                violations.push(format!(
                    "scenario '{}' bounced attachments through CPU {} time(s) (decode={} encode={}); per-frame budget is {}",
                    sample.scenario_id,
                    frame_cost
                        .gpu_runtime
                        .attachment_decode_count
                        .saturating_add(frame_cost.gpu_runtime.attachment_encode_count),
                    frame_cost.gpu_runtime.attachment_decode_count,
                    frame_cost.gpu_runtime.attachment_encode_count,
                    profile.max_attachment_cpu_bounce_count
                ));
            }
            if frame_cost.gpu_runtime.queue_submit_count > profile.max_queue_submit_count_per_frame
            {
                violations.push(format!(
                    "scenario '{}' issued {} queue submit(s); per-frame budget is {}",
                    sample.scenario_id,
                    frame_cost.gpu_runtime.queue_submit_count,
                    profile.max_queue_submit_count_per_frame
                ));
            }
            if frame_cost
                .gpu_runtime
                .primary_visibility_packet_fanout_count
                > profile.max_dispatch_count_primary_visibility
            {
                violations.push(format!(
                    "scenario '{}' used {} primary visibility dispatch(es); per-frame budget is {}",
                    sample.scenario_id,
                    frame_cost
                        .gpu_runtime
                        .primary_visibility_packet_fanout_count,
                    profile.max_dispatch_count_primary_visibility
                ));
            }
            if profile.gpu_timestamps_required_if_supported
                && frame_cost.gpu_runtime.timestamps_supported
                && frame_cost.gpu_runtime.timestamped_pass_count == 0
            {
                violations.push(format!(
                    "scenario '{}' reported timestamps_supported=true but never recorded a timestamped pass",
                    sample.scenario_id
                ));
            }
            if composite_frame_suite {
                validate_whole_frame_presentation_contract(sample, frame_cost, &mut violations);
            }
        }
    }

    report.measured_output_width = output_width;
    report.measured_output_height = output_height;
    report.min_internal_resolution_scale_observed = scales.iter().copied().reduce(f32::min);
    report.max_internal_resolution_scale_observed = scales.iter().copied().reduce(f32::max);
    report.reconstructed_output_detected = Some(reconstructed_output_detected);
    report.active_acceleration_artifacts = active_acceleration_artifacts.into_iter().collect();
    report.active_degradations = active_degradations.into_iter().collect();
    report.total_frame_median_ms = percentile_f32(&total_ms, 0.50);
    report.total_frame_median_fps = report.total_frame_median_ms.and_then(fps_from_ms);
    report.total_frame_p95_ms = percentile_f32(&total_ms, 0.95);
    report.primary_visibility_median_ms = percentile_f32(&primary_ms, 0.50);
    report.primary_visibility_p95_ms = percentile_f32(&primary_ms, 0.95);
    report.dominant_bottleneck_pass = most_common_key(&bottleneck_counts);
    report.hot_path_readback_bytes = Some(hot_path_readback_bytes);
    report.scene_reupload_bytes = Some(scene_reupload_bytes);
    report.cpu_screen_sample_allocations = Some(cpu_screen_sample_allocations);
    report.attachment_cpu_bounce_count = Some(attachment_cpu_bounce_count);
    report.queue_submit_count = Some(queue_submit_count);
    report.primary_visibility_dispatch_count = Some(primary_visibility_dispatch_count);
    report.timestamps_supported = Some(timestamps_supported);
    report.timestamped_pass_count = Some(timestamped_pass_count);
    report.notes.push(format!(
        "presentation reports collected for {} scenario(s) spanning {} closure frame sample(s)",
        presentation_reports.len(),
        total_ms.len()
    ));
    if whole_frame_suite {
        report.notes.push(format!(
            "whole-frame reports collected for {} scenario(s)",
            whole_frame_reports.len()
        ));
    }
    if engine_frame_suite {
        report.notes.push(format!(
            "engine-frame reports collected for {} scenario(s)",
            engine_frame_reports.len()
        ));
    }
    if !observed_backends.is_empty() {
        report.notes.push(format!(
            "presentation backends observed: {}",
            observed_backends.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    let selected_workgroup_sizes = presentation_reports
        .iter()
        .map(|report| report.selected_workgroup_size)
        .collect::<std::collections::BTreeSet<_>>();
    if !selected_workgroup_sizes.is_empty() {
        report.notes.push(format!(
            "wgsl workgroup size selection observed: {}",
            selected_workgroup_sizes
                .iter()
                .map(|size| size.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(comparison) = presentation_reports
        .iter()
        .find_map(|report| report.wgsl_workgroup_comparison.as_ref())
    {
        report.notes.push(format!(
            "wgsl workgroup comparison selected={} candidates={}",
            comparison.selected_workgroup_size,
            format_workgroup_comparison(comparison)
        ));
    }
    report.notes.push(format!(
        "execution model observations hot_path_readback_bytes={} scene_reupload_bytes={} cpu_screen_sample_allocations={} attachment_cpu_bounce_count={} queue_submit_count={} primary_visibility_dispatch_count={} timestamps_supported={} timestamped_pass_count={}",
        hot_path_readback_bytes,
        scene_reupload_bytes,
        cpu_screen_sample_allocations,
        attachment_cpu_bounce_count,
        queue_submit_count,
        primary_visibility_dispatch_count,
        timestamps_supported,
        timestamped_pass_count
    ));

    if report.primary_visibility_median_ms.is_none() || report.primary_visibility_p95_ms.is_none() {
        violations.push(
            "primary_visibility pass timings were not present in the sampled reports".to_string(),
        );
    }
    if let Some(total_median_ms) = report.total_frame_median_ms
        && total_median_ms > profile.frame_budget.median_ms
    {
        violations.push(format!(
            "frame median {:.2} ms exceeds budget {:.2} ms",
            total_median_ms, profile.frame_budget.median_ms
        ));
    }
    if let Some(total_p95_ms) = report.total_frame_p95_ms
        && total_p95_ms > profile.frame_budget.p95_ms
    {
        violations.push(format!(
            "frame p95 {:.2} ms exceeds budget {:.2} ms",
            total_p95_ms, profile.frame_budget.p95_ms
        ));
    }
    if let Some(primary_median_ms) = report.primary_visibility_median_ms
        && primary_median_ms > profile.primary_visibility_budget.median_ms
    {
        violations.push(format!(
            "primary visibility median {:.2} ms exceeds budget {:.2} ms",
            primary_median_ms, profile.primary_visibility_budget.median_ms
        ));
    }
    if let Some(primary_p95_ms) = report.primary_visibility_p95_ms
        && primary_p95_ms > profile.primary_visibility_budget.p95_ms
    {
        violations.push(format!(
            "primary visibility p95 {:.2} ms exceeds budget {:.2} ms",
            primary_p95_ms, profile.primary_visibility_budget.p95_ms
        ));
    }

    if violations.is_empty() {
        report.status = PerfClosureLaneStatus::Validated;
        report
            .notes
            .push("frame closure met the canonical 1080p120 contract".to_string());
    } else {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.extend(violations);
    }
    report
}

pub(super) fn validate_whole_frame_presentation_contract(
    sample: &PresentationBenchmarkReport,
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    violations: &mut Vec<String>,
) {
    use test_eval_perf::WholeFrameClosureScenarioKind as WholeFrameScenarioKind;

    let clipmap_passes = frame_cost
        .passes
        .iter()
        .filter(|pass| pass.pass_kind == "view_distance_clipmap")
        .collect::<Vec<_>>();
    if clipmap_passes.is_empty() {
        violations.push(format!(
            "scenario '{}' produced no view_distance_clipmap report in the timed lane",
            sample.scenario_id
        ));
    }
    let clipmap_states = clipmap_passes
        .iter()
        .filter_map(|pass| pass.clipmap.as_ref())
        .collect::<Vec<_>>();
    if !clipmap_passes.is_empty() && clipmap_states.is_empty() {
        violations.push(format!(
            "scenario '{}' produced a view_distance_clipmap pass without typed clipmap metadata",
            sample.scenario_id
        ));
    }
    if clipmap_states.iter().any(|clipmap| {
        clipmap.fallback_reasons.contains(
            &wrela::acceleration::clipmap::ViewDistanceClipmapFallbackReason::TileCullingUnavailable,
        )
    }) {
        violations.push(format!(
            "scenario '{}' fell back to clipmap mode because tile culling was unavailable",
            sample.scenario_id
        ));
    }
    if clipmap_states.iter().any(|clipmap| {
        clipmap.status == wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Fallback
            && !clipmap.fallback_reasons.iter().all(|reason| {
                matches!(
                    reason,
                    wrela::acceleration::clipmap::ViewDistanceClipmapFallbackReason::SnapshotMismatch
                        | wrela::acceleration::clipmap::ViewDistanceClipmapFallbackReason::LayoutMismatch
                )
            })
    }) {
        violations.push(format!(
            "scenario '{}' reported a whole-frame clipmap fallback outside the allowed snapshot/layout mismatch reasons",
            sample.scenario_id
        ));
    }
    let Some(kind) = sample.scenario_id.whole_frame_closure_kind() else {
        violations.push(format!(
            "scenario '{}' is not a recognized canonical whole-frame closure scenario id",
            sample.scenario_id
        ));
        return;
    };

    match kind {
        WholeFrameScenarioKind::Dense
        | WholeFrameScenarioKind::Repetition
        | WholeFrameScenarioKind::CameraMotion => {
            if frame_cost.tile_cull_total_tiles == 0 || frame_cost.tile_cull_active_tiles == 0 {
                violations.push(format!(
                    "scenario '{}' reported no tile-cull activity in the timed WGSL path",
                    sample.scenario_id
                ));
            }
            if frame_cost.tile_candidate_reduction == 0 {
                violations.push(format!(
                    "scenario '{}' reported no candidate-table reduction in the timed WGSL path",
                    sample.scenario_id
                ));
            }
        }
        _ => {}
    }

    match kind {
        WholeFrameScenarioKind::Dense
        | WholeFrameScenarioKind::Repetition
        | WholeFrameScenarioKind::ThinStack
        | WholeFrameScenarioKind::Media => {
            if !clipmap_states.iter().any(|clipmap| {
                clipmap.status == wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Reused
            }) {
                violations.push(format!(
                    "scenario '{}' did not report clipmap status=reused after warmup",
                    sample.scenario_id
                ));
            }
        }
        WholeFrameScenarioKind::CameraMotion => {
            if !clipmap_states.iter().any(|clipmap| {
                clipmap.status
                    == wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Updated
            }) {
                violations.push(format!(
                    "scenario '{}' did not report clipmap status=updated after warmup",
                    sample.scenario_id
                ));
            }
        }
    }

    if matches!(
        kind,
        WholeFrameScenarioKind::Media | WholeFrameScenarioKind::CameraMotion
    ) {
        if frame_cost.participant_resolve_count == 0 {
            violations.push(format!(
                "scenario '{}' did not execute participants_resolve in the media whole-frame lane",
                sample.scenario_id
            ));
        }
        if !frame_cost.quality.media_enabled {
            violations.push(format!(
                "scenario '{}' disabled media in the canonical whole-frame lane",
                sample.scenario_id
            ));
        }
        if frame_cost.quality.radiance_mode != "full" {
            violations.push(format!(
                "scenario '{}' degraded radiance mode to '{}'",
                sample.scenario_id, frame_cost.quality.radiance_mode
            ));
        }
        if frame_cost.quality.half_res_participants {
            violations.push(format!(
                "scenario '{}' enabled half-resolution participants in the canonical whole-frame lane",
                sample.scenario_id
            ));
        }
        if !frame_cost
            .passes
            .iter()
            .any(|pass| pass.pass_kind == "participants_resolve")
        {
            violations.push(format!(
                "scenario '{}' produced no participants_resolve pass report",
                sample.scenario_id
            ));
        }
    }
}

pub(super) fn frame_execution_model_gate_findings(
    profile: &PerfClosureProfile,
    report: &PerfClosureLaneStatusReport,
    presentation_reports: &[PresentationBenchmarkReport],
) -> Vec<PerfClosureFinding> {
    let mut findings = Vec::new();
    if let Some(observed) = report.hot_path_readback_bytes
        && observed > profile.max_hot_path_readback_bytes_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "hot_path_readback_gate".to_string(),
            summary: "the resident frame still performs hot-path readback, so the closure is not purely GPU-resident yet".to_string(),
            evidence: vec![
                format!("hot_path_readback_bytes={observed}"),
                format!(
                    "max_hot_path_readback_bytes_per_frame={}",
                    profile.max_hot_path_readback_bytes_per_frame
                ),
            ],
            next_step:
                "move the timed path off CPU readback and leave only the explicit timestamp budget, if supported".to_string(),
        });
    }
    if let Some(observed) = report.scene_reupload_bytes
        && observed > profile.max_scene_reupload_bytes_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "scene_reupload_gate".to_string(),
            summary: "the resident scene cache is still being reuploaded during the timed lane".to_string(),
            evidence: vec![
                format!("scene_reupload_bytes={observed}"),
                format!(
                    "max_scene_reupload_bytes_per_frame={}",
                    profile.max_scene_reupload_bytes_per_frame
                ),
            ],
            next_step:
                "keep the scene and acceleration data resident across frames instead of rebuilding it in the measured loop".to_string(),
        });
    }
    if let Some(observed) = report.cpu_screen_sample_allocations
        && observed > profile.max_cpu_screen_sample_allocations_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "cpu_screen_sample_allocation_gate".to_string(),
            summary: "the frame is still allocating CPU screen samples before WGSL can do the real work".to_string(),
            evidence: vec![
                format!("cpu_screen_sample_allocations={observed}"),
                format!(
                    "max_cpu_screen_sample_allocations_per_frame={}",
                    profile.max_cpu_screen_sample_allocations_per_frame
                ),
            ],
            next_step:
                "move primary screen-sample setup onto the resident path or reuse the existing GPU-side allocation".to_string(),
        });
    }
    if let Some(observed) = report.attachment_cpu_bounce_count
        && observed > profile.max_attachment_cpu_bounce_count
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "attachment_cpu_bounce_gate".to_string(),
            summary: "attachments are still bouncing through CPU memory inside the timed WGSL lane".to_string(),
            evidence: vec![
                format!("attachment_cpu_bounce_count={observed}"),
                format!(
                    "max_attachment_cpu_bounce_count={}",
                    profile.max_attachment_cpu_bounce_count
                ),
            ],
            next_step:
                "keep attachment decode and encode work GPU-resident so the closure lane does not materialize CPU copies".to_string(),
        });
    }
    if let Some(observed) = report.queue_submit_count
        && observed > profile.max_queue_submit_count_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "queue_submit_gate".to_string(),
            summary: "the measured lane is still fragmenting the frame across too many queue submits".to_string(),
            evidence: vec![
                format!("queue_submit_count={observed}"),
                format!(
                    "max_queue_submit_count_per_frame={}",
                    profile.max_queue_submit_count_per_frame
                ),
            ],
            next_step:
                "batch the resident work into fewer submissions so the closure lane reflects the steady-state framegraph".to_string(),
        });
    }
    if let Some(observed) = report.primary_visibility_dispatch_count
        && observed > profile.max_dispatch_count_primary_visibility
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "primary_visibility_dispatch_gate".to_string(),
            summary:
                "primary visibility is still dispatching more work than the closure profile allows"
                    .to_string(),
            evidence: vec![
                format!("primary_visibility_dispatch_count={observed}"),
                format!(
                    "max_dispatch_count_primary_visibility={}",
                    profile.max_dispatch_count_primary_visibility
                ),
            ],
            next_step:
                "tighten the primary visibility dispatch plan before tuning the later shading work"
                    .to_string(),
        });
    }
    if profile.gpu_timestamps_required_if_supported {
        for sample in presentation_reports {
            let timestamps_supported = sample.frame_cost.gpu_runtime.timestamps_supported;
            let timestamped_pass_count = sample.frame_cost.gpu_runtime.timestamped_pass_count;
            if timestamps_supported && timestamped_pass_count == 0 {
                findings.push(PerfClosureFinding {
                    subsystem: "presentation".to_string(),
                    focus: "timestamp_requirement_gate".to_string(),
                    summary: "the GPU supports timestamps, but the timed lane is not actually using them".to_string(),
                    evidence: vec![
                        format!("scenario={}", sample.scenario_id),
                        "timestamps_supported=true".to_string(),
                        "timestamped_pass_count=0".to_string(),
                    ],
                    next_step:
                        "thread timestamp coverage through the resident path whenever the adapter supports it".to_string(),
                });
            }
        }
    }
    findings
}

pub(super) fn explain_engine_frame_why_not_120_findings(
    profile: &PerfClosureProfile,
    report: &wrela::perf_target::PerfClosureEngineFrameStatusReport,
) -> Vec<PerfClosureFinding> {
    let mut findings = Vec::new();
    if let Some(observed) = report.frame_wall_time_median_ms
        && observed > profile.engine_frame_budget.frame_wall_time_median_ms
    {
        findings.push(PerfClosureFinding {
            subsystem: "engine_frame".to_string(),
            focus: "frame_wall_time_budget".to_string(),
            summary: "the unified engine frame is still over the wall-time budget even after combining presentation and collision under one scheduler".to_string(),
            evidence: vec![
                format!("frame_wall_time_median_ms={observed:.2}"),
                format!(
                    "frame_wall_time_budget_ms={:.2}",
                    profile.engine_frame_budget.frame_wall_time_median_ms
                ),
            ],
            next_step:
                "treat the engine frame as the canonical throughput unit and keep shaving the dominant subsystem until the whole frame fits".to_string(),
        });
    }
    if let Some(observed) = report.presentation_median_ms
        && observed > profile.engine_frame_budget.presentation_median_ms
    {
        findings.push(PerfClosureFinding {
            subsystem: "presentation".to_string(),
            focus: "engine_frame_presentation_budget".to_string(),
            summary: "presentation still dominates the engine frame budget, so subsystem wins are not yet enough for the full-frame target".to_string(),
            evidence: vec![
                format!("presentation_median_ms={observed:.2}"),
                format!(
                    "presentation_budget_ms={:.2}",
                    profile.engine_frame_budget.presentation_median_ms
                ),
            ],
            next_step:
                "keep reducing the resident presentation critical path before tuning smaller contributors".to_string(),
        });
    }
    if let Some(observed) = report.collision_median_ms
        && observed > profile.engine_frame_budget.collision_median_ms
    {
        findings.push(PerfClosureFinding {
            subsystem: "collision".to_string(),
            focus: "engine_frame_collision_budget".to_string(),
            summary: "collision is still too expensive inside the full engine frame, so the scheduler cannot hit the representative throughput target".to_string(),
            evidence: vec![
                format!("collision_median_ms={observed:.2}"),
                format!(
                    "collision_budget_ms={:.2}",
                    profile.engine_frame_budget.collision_median_ms
                ),
            ],
            next_step:
                "improve collision batching and certification pressure until the representative collision slice fits its frame budget".to_string(),
        });
    }
    if let Some(observed) = report.state_advance_median_ms
        && observed > profile.engine_frame_budget.state_advance_median_ms
    {
        findings.push(PerfClosureFinding {
            subsystem: "state_advance".to_string(),
            focus: "state_advance_budget".to_string(),
            summary: "the reserved state-advance slot is already consuming more time than the frame budget allows for future subsystems".to_string(),
            evidence: vec![
                format!("state_advance_median_ms={observed:.2}"),
                format!(
                    "state_advance_budget_ms={:.2}",
                    profile.engine_frame_budget.state_advance_median_ms
                ),
            ],
            next_step:
                "keep the state-advance adapter minimal so future gameplay work still fits inside the reserved engine-frame slot".to_string(),
        });
    }
    if let Some(observed) = report.future_subsystem_reserve_ms
        && observed < profile.engine_frame_budget.future_subsystem_reserve_ms
    {
        findings.push(PerfClosureFinding {
            subsystem: "engine_frame".to_string(),
            focus: "future_subsystem_reserve".to_string(),
            summary: "the engine frame has consumed the reserve that was supposed to protect future subsystem work".to_string(),
            evidence: vec![
                format!("future_subsystem_reserve_ms={observed:.2}"),
                format!(
                    "required_future_subsystem_reserve_ms={:.2}",
                    profile.engine_frame_budget.future_subsystem_reserve_ms
                ),
            ],
            next_step:
                "pull budget back out of the current frame so future subsystems can be added without immediately breaking throughput".to_string(),
        });
    }
    if let Some(observed) = report.queue_submit_count
        && observed > profile.engine_frame_budget.max_queue_submit_count_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "engine_frame".to_string(),
            focus: "engine_frame_queue_submit_budget".to_string(),
            summary: "the engine frame is still fragmented across too many queue submissions".to_string(),
            evidence: vec![
                format!("queue_submit_count={observed}"),
                format!(
                    "max_queue_submit_count_per_frame={}",
                    profile.engine_frame_budget.max_queue_submit_count_per_frame
                ),
            ],
            next_step:
                "keep presentation and collision on the same steady-state submission story so throughput reflects one frame, not a stack of micro-passes".to_string(),
        });
    }
    if let Some(observed) = report.hot_path_readback_bytes
        && observed
            > profile
                .engine_frame_budget
                .max_hot_path_readback_bytes_per_frame
    {
        findings.push(PerfClosureFinding {
            subsystem: "engine_frame".to_string(),
            focus: "engine_frame_hot_path_readback_budget".to_string(),
            summary: "the engine frame still performs hot-path readback, so the representative closure lane is not actually GPU-resident end to end".to_string(),
            evidence: vec![
                format!("hot_path_readback_bytes={observed}"),
                format!(
                    "max_hot_path_readback_bytes_per_frame={}",
                    profile.engine_frame_budget.max_hot_path_readback_bytes_per_frame
                ),
            ],
            next_step:
                "leave result readback to debug and oracle paths, and keep the closure lane on metrics-only tickets".to_string(),
        });
    }
    findings
}

pub(super) fn build_collision_closure_status(
    profile: &PerfClosureProfile,
    samples: &[test_eval_perf::PerfSummary],
    collision_reports: &[CollisionBenchmarkReport],
    collision_report_errors: &[String],
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureLaneStatusReport {
    let mut report = PerfClosureLaneStatusReport::unsampled(&profile.collision);
    let mut violations = Vec::new();
    report.status = PerfClosureLaneStatus::Sampled;
    report.notes.clear();
    report.collision_baseline_id = Some(profile.collision_baseline.baseline_id.clone());
    if samples.is_empty() {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.push(format!(
            "collision closure sampled 0 measured perf run(s) under protocol '{}'",
            profile.collision.protocol_id
        ));
        report
            .notes
            .push("collision closure suite ran without sampled runtime summaries".to_string());
        return report;
    }
    let summary = test_eval_perf::aggregate_perf_samples(samples);
    report.collision_runtime_median_ms = Some(ns_to_ms(summary.runtime_p50_ns));
    report.collision_runtime_p95_ms = Some(ns_to_ms(summary.runtime_p95_ns));
    report.notes.push(format!(
        "collision closure sampled {} measured perf run(s) under protocol '{}'",
        samples.len(),
        profile.collision.protocol_id
    ));
    if collision_reports.is_empty() {
        violations.push("collision benchmark collection produced no benchmark reports".to_string());
    }
    if !collision_report_errors.is_empty() {
        violations.push(format!(
            "collision benchmark collection reported {} error(s)",
            collision_report_errors.len()
        ));
        violations.extend(
            collision_report_errors
                .iter()
                .map(|error| format!("collision benchmark error: {error}")),
        );
    }
    if observed_warmup_runs != profile.warmup_runs as usize
        || observed_measured_runs != profile.measured_runs as usize
    {
        violations.push(format!(
            "observed run protocol warmup={} measured={} does not match canonical warmup={} measured={}",
            observed_warmup_runs,
            observed_measured_runs,
            profile.warmup_runs,
            profile.measured_runs
        ));
    }
    let expected_backend = profile.backend.as_str();
    let observed_backends = collision_reports
        .iter()
        .map(|report| report.backend.as_str())
        .collect::<Vec<_>>();
    if !observed_backends.is_empty()
        && observed_backends
            .iter()
            .any(|backend| *backend != expected_backend)
    {
        violations.push(format!(
            "collision backends observed: {}",
            observed_backends.join(", ")
        ));
        for observed_backend in observed_backends {
            if observed_backend != expected_backend {
                violations.push(format!(
                    "collision report backend '{observed_backend}' does not match closure backend '{expected_backend}'"
                ));
            }
        }
    }
    match load_collision_baseline_summary(&profile.collision_baseline.baseline_id) {
        Ok(baseline) => {
            let failures = test_eval_perf::evaluate_perf_gate(
                &summary,
                &baseline,
                profile.collision_baseline.max_runtime_regression_pct as f64,
                &test_eval_perf::KpiThresholds {
                    check_fallback_max: None,
                    check_batch_min: None,
                    scheduler_p99_improve_min_pct: None,
                    rewrite_overhead_max_pct: None,
                    actor_throughput_improve_min_pct: None,
                    queue_age_p99_max_regress_pct: None,
                    starvation_violations_max: None,
                    scheduler_throughput_improve_min_pct: None,
                    scheduler_loop_p99_max_regress_pct: None,
                    scheduler_local_hit_min: None,
                },
            );
            let regression_pct = if baseline.runtime_p50_ns == 0 {
                0.0
            } else {
                ((summary.runtime_p50_ns as f64 - baseline.runtime_p50_ns as f64)
                    / baseline.runtime_p50_ns as f64
                    * 100.0) as f32
            };
            report.collision_runtime_regression_pct = Some(regression_pct);
            report.notes.push(format!(
                "collision non-regression compared against baseline '{}' from {}",
                profile.collision_baseline.baseline_id,
                collision_baseline_fixture_path(&profile.collision_baseline.baseline_id).display()
            ));
            violations.extend(failures);
        }
        Err(err) => violations.push(format!(
            "collision baseline '{}' unavailable: {}",
            profile.collision_baseline.baseline_id, err
        )),
    }
    if violations.is_empty() {
        report.status = PerfClosureLaneStatus::Validated;
        report.notes.push(format!(
            "collision closure met the canonical non-regression budget ({:.2}% max runtime regression)",
            profile.collision_baseline.max_runtime_regression_pct
        ));
    } else {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.extend(violations);
    }
    report
}

pub(super) fn build_closure_verdict(
    profile: &PerfClosureProfile,
    frame: &PerfClosureLaneStatusReport,
    collision: &PerfClosureLaneStatusReport,
    engine_frame: &wrela::perf_target::PerfClosureEngineFrameStatusReport,
    presentation_reports: &[PresentationBenchmarkReport],
    collision_reports: &[CollisionBenchmarkReport],
) -> PerfClosureVerdict {
    let mut findings = BTreeMap::<(String, String), PerfClosureFinding>::new();
    let frame_sampled = !matches!(frame.status, PerfClosureLaneStatus::NotSampled);
    let collision_sampled = !matches!(collision.status, PerfClosureLaneStatus::NotSampled);
    let engine_frame_sampled = !matches!(engine_frame.status, PerfClosureLaneStatus::NotSampled);
    let sampled = frame_sampled || collision_sampled || engine_frame_sampled;
    let canonical_engine_frame_suite = profile.frame.suite.eq_ignore_ascii_case("engine_frame");

    if frame_sampled {
        for finding in frame_execution_model_gate_findings(profile, frame, presentation_reports) {
            merge_closure_finding(&mut findings, finding);
        }
        for report in presentation_reports {
            for finding in explain_frame_why_not_120_findings(
                &report.frame_cost,
                frame.total_frame_median_ms,
                frame.primary_visibility_median_ms,
                profile.frame_budget.median_ms,
                profile.primary_visibility_budget.median_ms,
            ) {
                merge_closure_finding(&mut findings, finding);
            }
            for finding in explain_acceleration_why_not_120_findings(&report.frame_cost) {
                merge_closure_finding(&mut findings, finding);
            }
        }
    }
    if engine_frame_sampled {
        for finding in explain_engine_frame_why_not_120_findings(profile, engine_frame) {
            merge_closure_finding(&mut findings, finding);
        }
    }
    if collision_sampled {
        for report in collision_reports {
            for finding in explain_collision_why_not_120_findings(report) {
                merge_closure_finding(&mut findings, finding);
            }
        }
    }

    // Fail closed once any sampled lane violates the profile. `NotApplicable`
    // is reserved for runs that never exercised this closure story at all; it
    // must not become a softer label for collected-but-bad evidence.
    let status = if !sampled {
        PerfClosureVerdictStatus::NotApplicable
    } else if (matches!(frame.status, PerfClosureLaneStatus::Violated)
        && !canonical_engine_frame_suite)
        || matches!(collision.status, PerfClosureLaneStatus::Violated)
        || matches!(engine_frame.status, PerfClosureLaneStatus::Violated)
    {
        PerfClosureVerdictStatus::Failed
    } else {
        PerfClosureVerdictStatus::Met
    };
    let top_remaining_bottleneck = if matches!(status, PerfClosureVerdictStatus::Failed) {
        choose_top_remaining_bottleneck(profile, frame, collision, engine_frame, collision_reports)
    } else {
        None
    };
    let summary = match status {
        PerfClosureVerdictStatus::NotApplicable => {
            format!(
                "{} closure target was not exercised in this run",
                profile.execution_story.as_str()
            )
        }
        PerfClosureVerdictStatus::Met => format!(
            "{} closure target met for sampled lanes across {} presentation report(s) and {} collision report(s)",
            profile.execution_story.as_str(),
            presentation_reports.len(),
            collision_reports.len()
        ),
        PerfClosureVerdictStatus::Failed => format!(
            "{} closure target failed; top remaining bottleneck: {}",
            profile.execution_story.as_str(),
            top_remaining_bottleneck.as_deref().unwrap_or("unknown")
        ),
    };

    PerfClosureVerdict {
        status,
        summary,
        top_remaining_bottleneck,
        findings: findings.into_values().collect(),
    }
}

pub(super) fn choose_top_remaining_bottleneck(
    profile: &PerfClosureProfile,
    frame: &PerfClosureLaneStatusReport,
    collision: &PerfClosureLaneStatusReport,
    engine_frame: &wrela::perf_target::PerfClosureEngineFrameStatusReport,
    collision_reports: &[CollisionBenchmarkReport],
) -> Option<String> {
    if matches!(engine_frame.status, PerfClosureLaneStatus::Violated) {
        if let Some(observed) = engine_frame.hot_path_readback_bytes
            && observed
                > profile
                    .engine_frame_budget
                    .max_hot_path_readback_bytes_per_frame
        {
            return Some("engine_frame_hot_path_readback".to_string());
        }
        if let Some(observed) = engine_frame.queue_submit_count
            && observed > profile.engine_frame_budget.max_queue_submit_count_per_frame
        {
            return Some("engine_frame_queue_submit_count".to_string());
        }
        if let Some(observed) = engine_frame.future_subsystem_reserve_ms
            && observed < profile.engine_frame_budget.future_subsystem_reserve_ms
        {
            return Some("future_subsystem_reserve".to_string());
        }
        if let Some(observed) = engine_frame.collision_median_ms
            && observed > profile.engine_frame_budget.collision_median_ms
        {
            return Some("collision".to_string());
        }
        if let Some(observed) = engine_frame.presentation_median_ms
            && observed > profile.engine_frame_budget.presentation_median_ms
        {
            return frame
                .dominant_bottleneck_pass
                .clone()
                .or_else(|| Some("presentation".to_string()));
        }
        if let Some(observed) = engine_frame.state_advance_median_ms
            && observed > profile.engine_frame_budget.state_advance_median_ms
        {
            return Some("state_advance".to_string());
        }
        if engine_frame
            .violations
            .iter()
            .any(|violation| violation.contains("future_reserve"))
        {
            return Some("future_subsystem_reserve".to_string());
        }
        return Some("engine_frame".to_string());
    }

    if matches!(frame.status, PerfClosureLaneStatus::Violated) {
        if let Some(bottleneck) = frame.dominant_bottleneck_pass.clone() {
            return Some(bottleneck);
        }
        if let Some(primary_visibility_median_ms) = frame.primary_visibility_median_ms
            && primary_visibility_median_ms > profile.primary_visibility_budget.median_ms
        {
            return Some("primary_visibility".to_string());
        }
        if let Some(frame_median_ms) = frame.total_frame_median_ms
            && frame_median_ms > profile.frame_budget.median_ms
        {
            return Some("surface_or_shading".to_string());
        }
        return Some("presentation".to_string());
    }

    if matches!(collision.status, PerfClosureLaneStatus::Violated) {
        if collision_reports.iter().any(|report| {
            report.witness_reuse_rate < 0.5
                && (report.unavailable_count_total > 0 || report.rejected_count_total > 0)
        }) {
            return Some("collision_witness_reuse".to_string());
        }
        if collision_reports
            .iter()
            .any(|report| report.fallback_rate > 0.0)
        {
            return Some("collision_fallback".to_string());
        }
        if collision.collision_runtime_regression_pct.is_some() {
            return Some("collision_runtime_regression".to_string());
        }
    }

    None
}

pub(super) fn explain_collision_why_not_120_findings(
    report: &CollisionBenchmarkReport,
) -> Vec<PerfClosureFinding> {
    let mut findings = Vec::new();

    if report.witness_reuse_rate < 0.50
        || report.unavailable_count_total > 0
        || report.rejected_count_total > 0
    {
        findings.push(PerfClosureFinding {
            subsystem: "collision".to_string(),
            focus: "witness_reuse_invalid_or_unsupported".to_string(),
            summary: "collision witness reuse is still being rejected or treated as unavailable, so the lane is not living on the fast path yet".to_string(),
            evidence: vec![
                format!("witness_reuse_rate={:.2}", report.witness_reuse_rate),
                format!("available_count_total={}", report.available_count_total),
                format!("consumed_count_total={}", report.consumed_count_total),
                format!("rejected_count_total={}", report.rejected_count_total),
                format!("unavailable_count_total={}", report.unavailable_count_total),
            ],
            next_step:
                "revisit the witness reuse contract, then make the conservative fallback path explicit when reuse is not valid".to_string(),
        });
    }

    if report.fallback_rate > 0.0 {
        findings.push(PerfClosureFinding {
            subsystem: "collision".to_string(),
            focus: "fallback_rate".to_string(),
            summary: "the collision lane is still falling back on a noticeable fraction of queries".to_string(),
            evidence: vec![
                format!("fallback_rate={:.2}", report.fallback_rate),
                format!("average_candidate_count={:.2}", report.average_candidate_count),
                format!(
                    "average_rejected_candidate_count={:.2}",
                    report.average_rejected_candidate_count
                ),
                format!(
                    "average_pruned_node_count={:.2}",
                    report.average_pruned_node_count
                ),
            ],
            next_step:
                "reduce the reasons the collision plan is falling back, then remeasure against the canonical baseline".to_string(),
        });
    }

    for execution in &report.executions {
        if execution.query_count > 1
            && execution.dispatch_count > 0
            && execution.average_items_per_dispatch < execution.query_count as f32
            && execution.average_items_per_dispatch <= 8.0
        {
            findings.push(PerfClosureFinding {
                subsystem: "collision".to_string(),
                focus: "batch_dispatch_amortization".to_string(),
                summary: "high-volume collision work is still dispatching too few items per GPU batch, so throughput is getting buried in per-dispatch overhead".to_string(),
                evidence: vec![
                    format!("scenario={}", execution.name),
                    format!("query_count={}", execution.query_count),
                    format!("dispatch_count={}", execution.dispatch_count),
                    format!(
                        "average_items_per_dispatch={:.2}",
                        execution.average_items_per_dispatch
                    ),
                ],
                next_step:
                    "keep feeding typed collision batches into the WGSL hot path until dispatch count scales with chunks instead of individual queries".to_string(),
            });
        }
        if execution.hot_path_readback_bytes > 0 {
            findings.push(PerfClosureFinding {
                subsystem: "collision".to_string(),
                focus: "collision_hot_path_readback".to_string(),
                summary: "the WGSL collision lane still reads results back on the hot path instead of running in metrics-only closure mode".to_string(),
                evidence: vec![
                    format!("scenario={}", execution.name),
                    format!(
                        "hot_path_readback_bytes={}",
                        execution.hot_path_readback_bytes
                    ),
                ],
                next_step:
                    "leave collision result materialization to the explicit oracle/debug path and keep the closure lane on observability-only tickets".to_string(),
            });
        }
        if execution.cpu_certification_query_count > 0 {
            findings.push(PerfClosureFinding {
                subsystem: "collision".to_string(),
                focus: "cpu_certification_pressure".to_string(),
                summary: "collision correctness is still leaning on CPU certification work inside the timed lane, so the apparent GPU win is not the whole cost".to_string(),
                evidence: vec![
                    format!("scenario={}", execution.name),
                    format!(
                        "cpu_certification_query_count={}",
                        execution.cpu_certification_query_count
                    ),
                ],
                next_step:
                    "keep certification explicit, counted, and shrinking so more of the workload stays on the GPU without hiding CPU oracle cost".to_string(),
            });
        }
        if execution.candidate_table_overflow_fallback_count > 0 {
            findings.push(PerfClosureFinding {
                subsystem: "collision".to_string(),
                focus: "candidate_table_overflow_fallback".to_string(),
                summary: "the fixed-cap collision candidate table is overflowing and dropping back to the slower compatibility path".to_string(),
                evidence: vec![
                    format!("scenario={}", execution.name),
                    format!(
                        "candidate_table_overflow_fallback_count={}",
                        execution.candidate_table_overflow_fallback_count
                    ),
                ],
                next_step:
                    "tighten candidate pruning or raise the fixed-cap table so representative workloads stay on the compaction path".to_string(),
            });
        }
    }

    findings
}

pub(super) fn merge_closure_finding(
    findings: &mut BTreeMap<(String, String), PerfClosureFinding>,
    finding: PerfClosureFinding,
) {
    let key = (finding.subsystem.clone(), finding.focus.clone());
    if let Some(existing) = findings.get_mut(&key) {
        for evidence in finding.evidence {
            if !existing.evidence.contains(&evidence) {
                existing.evidence.push(evidence);
            }
        }
        if existing.summary.is_empty() {
            existing.summary = finding.summary;
        }
        if existing.next_step.is_empty() {
            existing.next_step = finding.next_step;
        }
    } else {
        findings.insert(key, finding);
    }
}

pub(super) fn collision_baseline_fixture_path(baseline_id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("benchmarks")
        .join("collision_perf")
        .join("baselines")
        .join(format!("{baseline_id}.json"))
}

pub(super) fn load_collision_baseline_summary(
    baseline_id: &str,
) -> Result<test_eval_perf::PerfSummary, String> {
    let path = collision_baseline_fixture_path(baseline_id);
    test_eval_perf::load_perf_baseline_summary(&path)
        .map_err(|err| format!("{}: {}", path.display(), err))
}

pub(super) fn primary_visibility_pass_ms(
    report: &wrela::presentation_exec::PresentationFrameCostReport,
) -> Option<f32> {
    report
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "primary_visibility")
        .map(|pass| pass.elapsed_micros as f32 / 1_000.0)
}

pub(super) fn most_common_key(counts: &HashMap<String, usize>) -> Option<String> {
    counts
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(key, _)| key.clone())
}

pub(super) fn ns_to_ms(value: u128) -> f32 {
    value as f32 / 1_000_000.0
}

pub(super) fn hot_path_readback_bytes_without_timestamp_traffic(
    readback_bytes: u64,
    timestamps_supported: bool,
    timestamped_pass_count: u32,
) -> u64 {
    if timestamps_supported {
        let timestamp_bytes = (timestamped_pass_count as u64).saturating_mul(16);
        readback_bytes.saturating_sub(timestamp_bytes)
    } else {
        readback_bytes
    }
}

pub(super) fn fps_from_frame_time_ns(
    total_frame_time_ns: u128,
    measured_frame_count: usize,
) -> f64 {
    if total_frame_time_ns == 0 || measured_frame_count == 0 {
        0.0
    } else {
        measured_frame_count as f64 / (total_frame_time_ns as f64 / 1_000_000_000.0)
    }
}

pub(super) fn fps_from_ms(frame_time_ms: f32) -> Option<f32> {
    (frame_time_ms.is_finite() && frame_time_ms > 0.0).then_some(1000.0 / frame_time_ms)
}

pub(super) fn percentile_f32(values: &[f32], quantile: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index =
        ((sorted.len().saturating_sub(1)) as f32 * quantile.clamp(0.0, 1.0)).round() as usize;
    sorted.get(index).copied()
}

pub(super) fn print_presentation_benchmark_reports(reports: &[PresentationBenchmarkReport]) {
    println!("presentation-benchmarks:");
    for report in reports {
        let effective_history =
            presentation_frame_history(&report.frame_cost, &report.frame_cost_history);
        let solver_counters = aggregate_presentation_solver_counters(&effective_history);
        let field_samples = if report.observability_sampled {
            report.field_samples.to_string()
        } else {
            format!("unsampled({})", report.observability_notes.join("|"))
        };
        println!(
            "presentation-scenario {} test={} backend={} query_trace_solver_mode={} selected_workgroup_size={} frames={} frame_time_ns={} fps={:.2} field_samples={} quality={} target_fps={} scale={:.2} scale_history={} reconstructed_output={} bottleneck_pass={} acceleration={} gain_sources={}",
            report.scenario_id,
            report.test_name,
            report.backend,
            report.query_trace_solver_mode,
            report.selected_workgroup_size,
            report.frames_executed,
            report.frame_time_ns,
            report.steady_state_fps,
            field_samples,
            report.quality_tier,
            report.target_fps,
            report.internal_resolution_scale,
            report
                .internal_resolution_history
                .iter()
                .map(|scale| format!("{scale:.2}"))
                .collect::<Vec<_>>()
                .join(","),
            report.reconstructed_output,
            report.bottleneck_pass.as_deref().unwrap_or("none"),
            report.active_acceleration_artifacts.join(","),
            report.performance_gain_sources.join(","),
        );
        if let Some(comparison) = &report.wgsl_workgroup_comparison {
            println!(
                "  wgsl workgroup comparison selected={} candidates={}",
                comparison.selected_workgroup_size,
                format_workgroup_comparison(comparison),
            );
        }
        if let Some(comparison) = &report.ab_comparison {
            println!(
                "  ab hybrid-vs-dense-only frame_time_ns_delta={} ({:.2}%) average_trace_steps_delta={:.3} field_samples_delta={} candidate_count_before_pruning_delta={} candidate_count_after_pruning_delta={} dense_only_frame_time_ns={} dense_only_average_trace_steps={:.3} dense_only_field_samples={} dense_only_candidate_count_before_pruning={} dense_only_candidate_count_after_pruning={}",
                comparison.frame_time_ns_delta_vs_dense_only,
                comparison.frame_time_ns_delta_vs_dense_only_pct,
                comparison.average_trace_steps_delta_vs_dense_only,
                comparison.field_samples_delta_vs_dense_only,
                comparison.candidate_count_before_pruning_delta_vs_dense_only,
                comparison.candidate_count_after_pruning_delta_vs_dense_only,
                comparison.dense_only_frame_time_ns,
                comparison.dense_only_average_trace_steps,
                comparison.dense_only_field_samples,
                comparison.dense_only_candidate_count_before_pruning,
                comparison.dense_only_candidate_count_after_pruning,
            );
        }
        if report.observability_sampled {
            println!(
                "  solver counters relaxed_attempts={} relaxed_no_root_advances={} relaxed_brackets={} relaxed_unresolved={} interval_attempts={} interval_no_root_advances={} interval_brackets={} interval_unresolved={} refinement_attempts={} refinement_failures={} repeat_attempts={} repeat_supported={} repeat_inapplicable={} repeat_unsupported={} repeat_unsupported_form={} repeat_unsupported_bounds={} repeat_cells_enumerated={}",
                solver_counters.solver_relaxed_attempts,
                solver_counters.solver_relaxed_no_root_advances,
                solver_counters.solver_relaxed_brackets,
                solver_counters.solver_relaxed_unresolved,
                solver_counters.solver_interval_attempts,
                solver_counters.solver_interval_no_root_advances,
                solver_counters.solver_interval_brackets,
                solver_counters.solver_interval_unresolved,
                solver_counters.solver_refinement_attempts,
                solver_counters.solver_refinement_failures,
                solver_counters.solver_repeat_attempts,
                solver_counters.solver_repeat_supported,
                solver_counters.solver_repeat_inapplicable,
                solver_counters.solver_repeat_unsupported,
                solver_counters.solver_repeat_unsupported_form,
                solver_counters.solver_repeat_unsupported_bounds,
                solver_counters.solver_repeat_cells_enumerated,
            );
        } else {
            println!(
                "  solver counters unsampled notes={}",
                report.observability_notes.join("|")
            );
        }
        for pass in &report.frame_cost.passes {
            println!(
                "presentation-pass {} {} kind={} items={} elapsed_us={} dispatches={} bytes_read={} bytes_written={} notes={}",
                report.scenario_id,
                pass.pass_id,
                pass.pass_kind,
                pass.work_items,
                pass.elapsed_micros,
                pass.dispatch_count,
                pass.attachment_bytes_read,
                pass.attachment_bytes_written,
                pass.notes.join("|"),
            );
        }
    }
}

pub(super) fn print_collision_benchmark_reports(
    reports: &[test_eval_perf::CollisionBenchmarkReport],
) {
    println!("collision-benchmarks:");
    for report in reports {
        println!(
            "collision-scenario suite={} backend={} command={} total_runtime_ns={} queries_per_sec={:.2} avg_candidates={:.2} avg_rejected_candidates={:.2} avg_pruned_nodes={:.2} avg_interval_subdivisions={:.2} avg_interval_refinements={:.2} avg_certificate_successes={:.2} witness_reuse_rate={:.2} fallback_rate={:.2}",
            report.suite,
            report.backend,
            report.command,
            report.total_runtime_ns,
            report.queries_per_sec,
            report.average_candidate_count,
            report.average_rejected_candidate_count,
            report.average_pruned_node_count,
            report.average_interval_subdivisions,
            report.average_interval_refinements,
            report.average_certificate_successes,
            report.witness_reuse_rate,
            report.fallback_rate,
        );
        for execution in &report.executions {
            println!(
                "  collision-execution {} plan={} contract={} query_count={} runtime_ns={} gpu_time_total_ns={} timestamps_supported={} timestamped_pass_count={} qps={:.2} candidate_count={} rejected_candidate_count={} pruned_node_count={} candidate_reduction_effectiveness={:.3} interval_subdivisions={} interval_refinements={} certificate_successes={} fallback_count={} interval_bracket={} contact_normal_provenance={} wgsl_dispatch_count={} wgsl_dispatch_items={} wgsl_selected_workgroup_size={} wgsl_resident_shared_snapshot_artifacts={} cpu_certification_query_count={} reuse_available={} reuse_consumed={} reuse_rejected={} reuse_unavailable={} witness_reuse_rate={:.2} fallback_rate={:.2}",
                execution.name,
                execution.plan_name,
                execution.contract_id,
                execution.query_count,
                execution.runtime_ns,
                execution.gpu_time_total_ns,
                execution.timestamps_supported,
                execution.timestamped_pass_count,
                execution.queries_per_sec,
                execution.broadphase_candidate_count,
                execution.broadphase_rejected_candidate_count,
                execution.broadphase_pruned_node_count,
                execution.candidate_reduction_effectiveness,
                execution.interval_subdivisions,
                execution.interval_refinements,
                execution.certificate_successes,
                execution.fallback_count,
                execution
                    .interval_bracket
                    .map(|bracket| format!("[{:.6}, {:.6}]", bracket[0], bracket[1]))
                    .unwrap_or_else(|| "none".to_string()),
                execution
                    .contact_normal_provenance
                    .as_deref()
                    .unwrap_or("none"),
                execution.wgsl_dispatch_count,
                execution.wgsl_dispatch_items,
                execution.wgsl_selected_workgroup_size,
                execution.wgsl_resident_shared_snapshot_artifacts,
                execution.cpu_certification_query_count,
                execution.available_count,
                execution.consumed_count,
                execution.rejected_count,
                execution.unavailable_count,
                execution.witness_reuse_rate,
                execution.fallback_rate,
            );
        }
    }
}

pub(super) fn print_whole_frame_benchmark_reports(reports: &[WholeFrameBenchmarkReport]) {
    println!("whole-frame-benchmarks:");
    for report in reports {
        println!(
            "whole-frame-scenario {} test={} presentation_frame_time_ns={} collision_runtime_ns={} total_runtime_ns={} fps={:.2} presentation_bottleneck_pass={} collision_fallback_rate={:.2} collision_witness_reuse_rate={:.2}",
            report.scenario_id,
            report.test_name,
            report.presentation_frame_time_ns,
            report.collision_runtime_ns,
            report.total_runtime_ns,
            report.steady_state_fps,
            report
                .presentation_bottleneck_pass
                .as_deref()
                .unwrap_or("none"),
            report.collision_fallback_rate,
            report.collision_witness_reuse_rate,
        );
    }
}

fn engine_runtime_source_name(source: wrela::engine_frame::EngineRuntimeSource) -> &'static str {
    match source {
        wrela::engine_frame::EngineRuntimeSource::TimelineSpans => "timeline_spans",
        wrela::engine_frame::EngineRuntimeSource::SelfReported => "self_reported",
        wrela::engine_frame::EngineRuntimeSource::CompatibilityJoin => "compatibility_join",
        wrela::engine_frame::EngineRuntimeSource::ReservedSlotUnsampled => {
            "reserved_slot_unsampled"
        }
    }
}

fn engine_gpu_timing_policy_name(
    policy: wrela::engine_frame::EngineGpuTimingPolicy,
) -> &'static str {
    match policy {
        wrela::engine_frame::EngineGpuTimingPolicy::Disabled => "disabled",
        wrela::engine_frame::EngineGpuTimingPolicy::Timestamped => "timestamped",
        wrela::engine_frame::EngineGpuTimingPolicy::RuntimeProxy => "runtime_proxy",
    }
}

fn engine_measurement_policy_summary(
    policy: &wrela::engine_frame::EngineMeasurementPolicy,
) -> String {
    format!(
        "runtime_source={} gpu_timing={} hot_path_readback_allowed={} export_readback_allowed={}",
        engine_runtime_source_name(policy.runtime_source),
        engine_gpu_timing_policy_name(policy.gpu_timing),
        policy.hot_path_readback_allowed,
        policy.export_readback_allowed
    )
}

fn format_optional_ns(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

pub(super) fn render_engine_frame_benchmark_reports(
    reports: &[EngineFrameBenchmarkReport],
) -> String {
    let mut rendered = String::new();
    writeln!(rendered, "engine-frame-benchmarks:").expect("write engine-frame header");
    for report in reports {
        writeln!(
            rendered,
            "engine-frame-scenario {} test={} frame_count={} frame_wall_time_ns={} cpu_critical_path_ns={} gpu_critical_path_ns={} fps={:.2} presentation_runtime_ns={} presentation_self_reported_runtime_ns={} presentation_orchestration_gap_ns={} collision_runtime_ns={} collision_self_reported_runtime_ns={} collision_orchestration_gap_ns={} state_advance_runtime_ns={} state_advance_self_reported_runtime_ns={} state_advance_orchestration_gap_ns={} future_subsystem_reserve_ns={} queue_submit_count={} hot_path_readback_bytes={} scene_reupload_bytes={} timestamped_pass_count={} timing_readback_bytes={} measurement_policy=\"{}\" active_degradations={} violations={}",
            report.scenario_id,
            report.test_name,
            report.frame_count,
            report.frame_wall_time_ns,
            report.cpu_critical_path_ns,
            format_optional_ns(report.gpu_critical_path_ns),
            report.steady_state_fps,
            report.presentation_runtime_ns,
            format_optional_ns(report.presentation_self_reported_runtime_ns),
            report.presentation_orchestration_gap_ns,
            report.collision_runtime_ns,
            format_optional_ns(report.collision_self_reported_runtime_ns),
            report.collision_orchestration_gap_ns,
            report.state_advance_runtime_ns,
            format_optional_ns(report.state_advance_self_reported_runtime_ns),
            report.state_advance_orchestration_gap_ns,
            report.future_subsystem_reserve_ns,
            report.queue_submit_count,
            report.hot_path_readback_bytes,
            report.scene_reupload_bytes,
            report.timestamped_pass_count,
            report.timing_readback_bytes,
            engine_measurement_policy_summary(&report.measurement_policy),
            report.active_degradations.join(","),
            report.violations.join(","),
        )
        .expect("write engine-frame benchmark report");
        for subsystem in &report.subsystem_reports {
            writeln!(
                rendered,
                "  engine-subsystem {} kind={:?} items={} cpu_critical_path_us={} gpu_critical_path_us={} executed_wall_time_us={} self_reported_runtime_us={} orchestration_gap_us={} queue_submits={} hot_path_readback_bytes={} scene_reupload_bytes={} timestamped_pass_count={} timing_readback_bytes={} measurement_policy=\"{}\" notes={}",
                subsystem.label,
                subsystem.kind,
                subsystem.work_items,
                subsystem.cpu_critical_path_micros,
                subsystem
                    .gpu_critical_path_micros
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                subsystem.executed_wall_time_micros,
                subsystem
                    .self_reported_runtime_micros
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                subsystem.orchestration_gap_micros,
                subsystem.queue_submit_count,
                subsystem.hot_path_readback_bytes,
                subsystem.scene_reupload_bytes,
                subsystem.timestamped_pass_count,
                subsystem.timing_readback_bytes,
                engine_measurement_policy_summary(&subsystem.measurement_policy),
                subsystem.notes.join("|"),
            )
            .expect("write engine-frame subsystem report");
        }
    }
    rendered
}

pub(super) fn print_engine_frame_benchmark_reports(reports: &[EngineFrameBenchmarkReport]) {
    print!("{}", render_engine_frame_benchmark_reports(reports));
}

pub(super) fn print_engine_frame_audit_report(
    presentation_reports: &[PresentationBenchmarkReport],
    whole_frame_reports: &[WholeFrameBenchmarkReport],
    engine_frame_reports: &[EngineFrameBenchmarkReport],
) {
    let mut presentation_by_scenario = HashMap::new();
    for report in presentation_reports {
        presentation_by_scenario.insert(report.scenario_id.clone(), report);
    }
    let mut whole_frame_by_scenario = HashMap::new();
    for report in whole_frame_reports {
        whole_frame_by_scenario.insert(report.scenario_id.clone(), report);
    }
    println!("engine-frame-audit:");
    for report in engine_frame_reports {
        let compatibility = whole_frame_by_scenario.get(&report.scenario_id);
        let presentation = presentation_by_scenario.get(&report.scenario_id);
        println!(
            "audit-scenario {} live_frame_wall_time_ns={} compatibility_total_runtime_ns={} compatibility_presentation_frame_time_ns={} compatibility_collision_runtime_ns={} presentation_debug_frame_time_ns={} presentation_live_executed_ns={} presentation_live_self_reported_ns={} presentation_live_gap_ns={} collision_live_executed_ns={} collision_live_self_reported_ns={} collision_live_gap_ns={} state_advance_live_executed_ns={} state_advance_live_self_reported_ns={} state_advance_live_gap_ns={} measurement_policy=\"{}\" timestamped_pass_count={} timing_readback_bytes={}",
            report.scenario_id,
            report.frame_wall_time_ns,
            compatibility
                .map(|value| value.total_runtime_ns.to_string())
                .unwrap_or_else(|| "none".to_string()),
            compatibility
                .map(|value| value.presentation_frame_time_ns.to_string())
                .unwrap_or_else(|| "none".to_string()),
            compatibility
                .map(|value| value.collision_runtime_ns.to_string())
                .unwrap_or_else(|| "none".to_string()),
            presentation
                .map(|value| value.frame_time_ns.to_string())
                .unwrap_or_else(|| "none".to_string()),
            report.presentation_runtime_ns,
            format_optional_ns(report.presentation_self_reported_runtime_ns),
            report.presentation_orchestration_gap_ns,
            report.collision_runtime_ns,
            format_optional_ns(report.collision_self_reported_runtime_ns),
            report.collision_orchestration_gap_ns,
            report.state_advance_runtime_ns,
            format_optional_ns(report.state_advance_self_reported_runtime_ns),
            report.state_advance_orchestration_gap_ns,
            engine_measurement_policy_summary(&report.measurement_policy),
            report.timestamped_pass_count,
            report.timing_readback_bytes,
        );
        for subsystem in &report.subsystem_reports {
            println!(
                "  audit-subsystem {} kind={:?} executed_wall_time_us={} self_reported_runtime_us={} orchestration_gap_us={} queue_submits={} hot_path_readback_bytes={} scene_reupload_bytes={} timestamped_pass_count={} timing_readback_bytes={} measurement_policy=\"{}\" notes={}",
                subsystem.label,
                subsystem.kind,
                subsystem.executed_wall_time_micros,
                subsystem
                    .self_reported_runtime_micros
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                subsystem.orchestration_gap_micros,
                subsystem.queue_submit_count,
                subsystem.hot_path_readback_bytes,
                subsystem.scene_reupload_bytes,
                subsystem.timestamped_pass_count,
                subsystem.timing_readback_bytes,
                engine_measurement_policy_summary(&subsystem.measurement_policy),
                subsystem.notes.join("|"),
            );
        }
    }
}

pub(super) fn print_closure_verdict_report(report: &PerfClosureReport, verbose: bool) {
    print!("{}", render_closure_verdict_report(report, verbose));
}

pub(super) fn render_closure_verdict_report(report: &PerfClosureReport, verbose: bool) -> String {
    let mut rendered = String::new();
    writeln!(
        rendered,
        "closure verdict: {}",
        closure_verdict_status_name(report.verdict.status)
    )
    .expect("write closure verdict");
    writeln!(
        rendered,
        "  profile: {} ({}, backend={}, adapter={}, requested_limits={}, timestamps={}, timestamps_required_if_supported={}, readback_max={}, scene_reupload_max={}, cpu_samples_max={}, attachment_bounce_max={}, queue_submits_max={}, primary_visibility_dispatch_max={}, f16={}, indirect_dispatch={}, warmup={})",
        report.profile.name,
        report.profile.execution_story.as_str(),
        report.profile.backend.as_str(),
        report.profile.adapter_name,
        report.profile.requested_limits_profile,
        report.profile.timestamps_enabled,
        report.profile.gpu_timestamps_required_if_supported,
        report.profile.max_hot_path_readback_bytes_per_frame,
        report.profile.max_scene_reupload_bytes_per_frame,
        report.profile.max_cpu_screen_sample_allocations_per_frame,
        report.profile.max_attachment_cpu_bounce_count,
        report.profile.max_queue_submit_count_per_frame,
        report.profile.max_dispatch_count_primary_visibility,
        report.profile.f16_enabled,
        report.profile.indirect_dispatch_enabled,
        report.profile.warmup_protocol,
    )
    .expect("write closure profile");
    if !report.profile.enabled_optional_features.is_empty() {
        writeln!(
            rendered,
            "  enabled optional features: {}",
            report.profile.enabled_optional_features.join(", ")
        )
        .expect("write optional features");
    }
    if let Some(cpu_oracle_profile) = report.cpu_oracle_profile.as_ref() {
        writeln!(
            rendered,
            "  cpu-oracle companion: {} ({}, backend={}, adapter={})",
            cpu_oracle_profile.name,
            cpu_oracle_profile.execution_story.as_str(),
            cpu_oracle_profile.backend.as_str(),
            cpu_oracle_profile.adapter_name
        )
        .expect("write cpu oracle profile");
    }
    writeln!(rendered, "  summary: {}", report.verdict.summary).expect("write closure summary");
    if let Some(bottleneck) = report.verdict.top_remaining_bottleneck.as_deref() {
        writeln!(rendered, "  top remaining bottleneck: {bottleneck}")
            .expect("write top bottleneck");
    }
    if let Some(frame_median_ms) = report.frame.total_frame_median_ms {
        if let Some(frame_median_fps) = report.frame.total_frame_median_fps {
            writeln!(
                rendered,
                "  frame median: {:.2} ms ({:.2} FPS)",
                frame_median_ms, frame_median_fps
            )
            .expect("write frame median fps");
        } else {
            writeln!(rendered, "  frame median: {:.2} ms", frame_median_ms)
                .expect("write frame median");
        }
    }
    if let Some(engine_frame_median_ms) = report.engine_frame.frame_wall_time_median_ms {
        if let Some(engine_frame_p95_ms) = report.engine_frame.frame_wall_time_p95_ms {
            writeln!(
                rendered,
                "  engine-frame median: {:.2} ms (p95 {:.2} ms)",
                engine_frame_median_ms, engine_frame_p95_ms
            )
            .expect("write engine-frame median");
        } else {
            writeln!(
                rendered,
                "  engine-frame median: {:.2} ms",
                engine_frame_median_ms
            )
            .expect("write engine-frame median");
        }
    }
    if verbose || matches!(report.verdict.status, PerfClosureVerdictStatus::Failed) {
        if let Some(queue_submit_count) = report.engine_frame.queue_submit_count {
            writeln!(
                rendered,
                "  engine-frame queue submits: {} (budget {})",
                queue_submit_count,
                report
                    .profile
                    .engine_frame_budget
                    .max_queue_submit_count_per_frame
            )
            .expect("write engine-frame queue submits");
        }
        if let Some(hot_path_readback_bytes) = report.engine_frame.hot_path_readback_bytes {
            writeln!(
                rendered,
                "  engine-frame hot-path readback: {} byte(s) (budget {} byte(s))",
                hot_path_readback_bytes,
                report
                    .profile
                    .engine_frame_budget
                    .max_hot_path_readback_bytes_per_frame
            )
            .expect("write engine-frame hot-path readback");
        }
    }
    if verbose || matches!(report.verdict.status, PerfClosureVerdictStatus::Failed) {
        writeln!(rendered, "why-not-120:").expect("write why-not heading");
        if report.verdict.findings.is_empty() {
            writeln!(
                rendered,
                "  - no specific subsystem finding was inferred from the sampled reports"
            )
            .expect("write empty findings");
        } else {
            for finding in &report.verdict.findings {
                writeln!(
                    rendered,
                    "  - subsystem={} focus={}",
                    finding.subsystem, finding.focus
                )
                .expect("write finding header");
                writeln!(rendered, "    summary: {}", finding.summary)
                    .expect("write finding summary");
                if !finding.evidence.is_empty() {
                    writeln!(rendered, "    evidence: {}", finding.evidence.join(" | "))
                        .expect("write finding evidence");
                }
                writeln!(rendered, "    next step: {}", finding.next_step)
                    .expect("write finding next step");
            }
        }
    }
    rendered
}

pub(super) fn closure_verdict_status_name(status: PerfClosureVerdictStatus) -> &'static str {
    match status {
        PerfClosureVerdictStatus::NotApplicable => "not_applicable",
        PerfClosureVerdictStatus::Met => "met",
        PerfClosureVerdictStatus::Failed => "failed",
    }
}

pub(super) fn format_workgroup_comparison(
    comparison: &PresentationWgslWorkgroupComparison,
) -> String {
    comparison
        .candidate_workgroup_sizes
        .iter()
        .zip(&comparison.candidate_frame_time_ns)
        .zip(&comparison.frame_time_ns_delta_vs_selected_pct)
        .map(|((workgroup_size, frame_time_ns), delta_pct)| {
            format!("{}:{}ns({:+.2}%)", workgroup_size, frame_time_ns, delta_pct)
        })
        .collect::<Vec<_>>()
        .join(" ")
}
