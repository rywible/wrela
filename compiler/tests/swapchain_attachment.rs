use wrela::engine_frame::{
    EngineFrameContext, EngineFrameError, EngineFrameScheduler, EngineGpuTimingPolicy,
    EngineJobAffinity, EngineMeasurementPolicy, EngineRuntimeSource, EngineSpanDomain,
    EngineSubsystemAdapter, EngineSubsystemDescriptor, EngineSubsystemKind, EngineSubsystemPlan,
    EngineSubsystemReport,
};
use wrela::presentation_exec::framegraph::PresentationFramegraph;

struct SwapchainReportingAdapter;

impl EngineSubsystemAdapter for SwapchainReportingAdapter {
    fn build(
        &mut self,
        builder: &mut wrela::engine_frame::EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "presentation.swapchain".to_string(),
            runs_after: Vec::new(),
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let [acquire_label, present_label] = PresentationFramegraph::swapchain_reporting_labels();
        let acquire = builder.add_synthetic_job(
            EngineSubsystemKind::Presentation,
            acquire_label,
            EngineJobAffinity::External,
            EngineSpanDomain::PresentWait,
            Vec::new(),
            false,
            1,
        );
        let present = builder.add_synthetic_job(
            EngineSubsystemKind::Presentation,
            present_label,
            EngineJobAffinity::External,
            EngineSpanDomain::PresentWait,
            vec![acquire],
            false,
            1,
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![acquire],
            vec![present],
            move |timeline, _ctx: &mut EngineFrameContext| {
                let elapsed = timeline
                    .spans
                    .iter()
                    .filter(|span| span.subsystem == EngineSubsystemKind::Presentation)
                    .map(|span| span.elapsed_micros())
                    .sum();
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: 1,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: elapsed,
                    self_reported_runtime_micros: None,
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    timestamped_pass_count: 0,
                    timing_readback_bytes: 0,
                    wait_time_micros: elapsed,
                    notes: vec!["presentation_framegraph_swapchain_observed".to_string()],
                })
            },
        ))
    }
}

#[test]
fn presentation_reporting_observes_swapchain_acquire_and_present() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut adapters: Vec<Box<dyn EngineSubsystemAdapter>> =
        vec![Box::new(SwapchainReportingAdapter)];

    let report = scheduler
        .run_frame("swapchain_reporting", 0, &mut adapters)
        .expect("engine frame");

    let labels = report
        .timeline_spans
        .iter()
        .map(|span| span.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"presentation.swapchain_acquire"));
    assert!(labels.contains(&"presentation.swapchain_present"));

    let present_span = report
        .timeline_spans
        .iter()
        .find(|span| span.label == "presentation.swapchain_present")
        .expect("present span");
    assert_eq!(present_span.subsystem, EngineSubsystemKind::Presentation);
    assert!(!present_span.queue_submission);

    let presentation = report
        .subsystem(EngineSubsystemKind::Presentation)
        .expect("presentation report");
    assert_eq!(presentation.hot_path_readback_bytes, 0);
    assert_eq!(presentation.timing_readback_bytes, 0);
}
