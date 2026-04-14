pub use super::{
    AccelerationChildSpan, AccelerationForest, AccelerationLeafPayload, AccelerationNode,
    AccelerationRejectionClass, AccelerationRejectionRecord, AccelerationReport,
};
use super::{
    acceleration_cache_kind_name, acceleration_observer_name, acceleration_rejection_class_name,
    cache_artifact_scope_name,
};
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
