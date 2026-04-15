pub use super::{
    AccelerationChildSpan, AccelerationForest, AccelerationLeafPayload, AccelerationNode,
    AccelerationRejectionClass, AccelerationRejectionRecord, AccelerationReport,
};
use super::{
    acceleration_cache_kind_name, acceleration_observer_name, acceleration_rejection_class_name,
    cache_artifact_scope_name,
};
use crate::perf_target::PerfClosureFinding;
use crate::presentation_exec::PresentationFrameCostReport;
use std::fmt::{self, Display, Formatter};

fn sorted_nodes(forest: &AccelerationForest) -> Vec<&AccelerationNode> {
    let mut nodes = forest.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        left.stable_order
            .cmp(&right.stable_order)
            .then(left.id.cmp(&right.id))
    });
    nodes
}

pub fn render_forest_debug_dump(forest: &AccelerationForest) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "forest id={} kind={:?} version={} candidate_class={:?} roots={}\n",
        forest.contract.id,
        forest.contract.kind,
        forest.contract.forest_version,
        forest.contract.candidate_class,
        forest.contract.root_nodes.join(",")
    ));

    let mut caches = forest.caches.iter().collect::<Vec<_>>();
    caches.sort_by(|left, right| left.id.cmp(&right.id));
    for cache in caches {
        out.push_str(&format!(
            "cache id={} kind={} scope={} observer={} artifact_scope={} fallback={:?}\n",
            cache.id,
            acceleration_cache_kind_name(cache.kind),
            cache_artifact_scope_name(cache.scope),
            cache
                .observer
                .map(acceleration_observer_name)
                .unwrap_or("shared"),
            cache.artifact_scope,
            cache.fallback_expectation
        ));
    }

    for node in sorted_nodes(forest) {
        out.push_str(&format!(
            "node id={} order={} kind={:?} candidate_class={:?} child_span={} children={}\n",
            node.id,
            node.stable_order,
            node.kind,
            node.candidate_class,
            node.child_span
                .map(|span| format!("{}..{}", span.start, span.start + span.len))
                .unwrap_or_else(|| "none".to_string()),
            node.child_ids.join(",")
        ));
        if let Some(payload) = node.leaf_payload.as_ref() {
            out.push_str(&format!(
                "  leaf semantic_id={} feature_id={} instance_id={} repeat_id={}\n",
                payload.semantic_id,
                payload
                    .feature_id
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or("none"),
                payload
                    .instance_id
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or("none"),
                payload
                    .repeat_id
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or("none")
            ));
        }
        if let Some(support) = node.support.as_ref() {
            out.push_str(&format!(
                "  support kind={:?} semantics={} opaque_boundary={} coarse_prune={}\n",
                support.kind, support.semantics, support.opaque_boundary, support.can_coarse_prune
            ));
        }
        if !node.bounds.is_empty() {
            let bounds = node
                .bounds
                .iter()
                .map(|bound| format!("{:?}:{}", bound.kind, bound.summary))
                .collect::<Vec<_>>()
                .join(" | ");
            out.push_str(&format!("  bounds {bounds}\n"));
        }
        if !node.lineage.is_empty() {
            let lineage = node
                .lineage
                .iter()
                .map(|record| {
                    format!(
                        "{}@{}#{}",
                        record.semantic_id, record.source_path, record.stable_order
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            out.push_str(&format!("  lineage {lineage}\n"));
        }
        if !node.certificate_handles.is_empty() {
            let certificates = node
                .certificate_handles
                .iter()
                .map(|handle| format!("{:?}:{}", handle.kind, handle.handle))
                .collect::<Vec<_>>()
                .join(" | ");
            out.push_str(&format!("  certificates {certificates}\n"));
        }
        if !node.notes.is_empty() {
            out.push_str(&format!("  notes {}\n", node.notes.join(" | ")));
        }
    }

    if !forest.rejection_reasons.is_empty() {
        let mut rejection_reasons = forest.rejection_reasons.iter().collect::<Vec<_>>();
        rejection_reasons.sort_by(|left, right| {
            left.class
                .cmp(&right.class)
                .then(left.subject.cmp(&right.subject))
                .then(left.detail.cmp(&right.detail))
        });
        for rejection in rejection_reasons {
            out.push_str(&format!(
                "rejection class={} subject={} detail={}\n",
                acceleration_rejection_class_name(rejection.class),
                rejection.subject,
                rejection.detail
            ));
        }
    }

    let mut observer_usage = forest.observer_usage.iter().collect::<Vec<_>>();
    observer_usage.sort_by(|left, right| {
        left.observer
            .cmp(&right.observer)
            .then(left.contract_id.cmp(&right.contract_id))
    });
    for usage in observer_usage {
        let candidate_classes = usage
            .candidate_classes
            .iter()
            .map(|candidate| format!("{candidate:?}"))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(
            "usage observer={} contract={} caches={} candidates={} notes={}\n",
            acceleration_observer_name(usage.observer),
            usage.contract_id,
            usage.used_caches.join(","),
            candidate_classes,
            usage.notes.join(" | ")
        ));
    }

    out
}

pub fn render_report_debug_dump(report: &AccelerationReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "acceleration-report observer={}\n",
        acceleration_observer_name(report.observer)
    ));
    let mut forests = report.forests.iter().collect::<Vec<_>>();
    forests.sort_by(|left, right| left.contract.id.cmp(&right.contract.id));
    for note in &report.notes {
        out.push_str(&format!("note {}\n", note));
    }
    for forest in forests {
        out.push_str(&render_forest_debug_dump(forest));
    }
    out
}

pub fn explain_why_not_120_findings(
    report: &PresentationFrameCostReport,
) -> Vec<PerfClosureFinding> {
    let mut findings = Vec::new();

    if report.active_acceleration_artifacts.is_empty() || report.performance_gain_sources.is_empty()
    {
        findings.push(PerfClosureFinding {
            subsystem: "acceleration".to_string(),
            focus: "caches_unavailable_or_invalid".to_string(),
            summary: "cache-backed acceleration artifacts were not available in the sampled frame, so the engine still looks like it is paying for the slow path".to_string(),
            evidence: vec![
                format!(
                    "active_acceleration_artifacts={}",
                    if report.active_acceleration_artifacts.is_empty() {
                        "none".to_string()
                    } else {
                        report.active_acceleration_artifacts.join(",")
                    }
                ),
                format!(
                    "performance_gain_sources={}",
                    if report.performance_gain_sources.is_empty() {
                        "none".to_string()
                    } else {
                        report.performance_gain_sources.join(",")
                    }
                ),
                format!("cache_brick_visits={}", report.cache_brick_visits),
                format!("cache_brick_hits={}", report.cache_brick_hits),
                format!("cache_brick_misses={}", report.cache_brick_misses),
                format!("cache_interval_advances={}", report.cache_interval_advances),
            ],
            next_step:
                "confirm the acceleration artifact is being built, validated, and surfaced to the closure run before treating the frame as cache-accelerated".to_string(),
        });
    }

    let backend_is_wgsl = report.execution_policy.contains("backend=wgsl");
    if backend_is_wgsl
        && (report.average_trace_steps >= 8.0
            || report.candidate_count_after_pruning >= report.candidate_count_before_pruning)
    {
        findings.push(PerfClosureFinding {
            subsystem: "acceleration".to_string(),
            focus: "wgsl_linear_traversal".to_string(),
            summary: "the WGSL path still looks suspiciously dense, which usually means the acceleration spine is not being exercised yet".to_string(),
            evidence: vec![
                format!("execution_policy={}", report.execution_policy),
                format!("selected_workgroup_size={}", report.selected_workgroup_size),
                format!("average_trace_steps={:.3}", report.average_trace_steps),
                format!(
                    "candidate_count_before_pruning={}",
                    report.candidate_count_before_pruning
                ),
                format!(
                    "candidate_count_after_pruning={}",
                    report.candidate_count_after_pruning
                ),
                format!(
                    "support_prune_effectiveness={:.3}",
                    report.support_prune_effectiveness
                ),
            ],
            next_step:
                "check that WGSL traversal is using the shared acceleration forest instead of a linear scan through every candidate".to_string(),
        });
    }

    findings
}

impl Display for AccelerationNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "node id={} order={} kind={:?} candidate_class={:?}",
            self.id, self.stable_order, self.kind, self.candidate_class
        )
    }
}

impl Display for AccelerationForest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "forest id={} kind={:?} version={} candidate_class={:?}",
            self.contract.id,
            self.contract.kind,
            self.contract.forest_version,
            self.contract.candidate_class
        )
    }
}

impl Display for AccelerationReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&render_report_debug_dump(self))
    }
}
