use crate::artifact_contract::{
    ArtifactSnapshotRelation, ArtifactUseKind, ArtifactUseSource, ArtifactValidityPredicate,
    ArtifactValidityRule, SemanticArtifactKind,
};
use crate::artifact_key::ArtifactPolicyDigestMode;
use crate::query_program_spine::{
    ObserverKind, QueryProgramSpine, SpineDependencyEdge, SpineEdgeKind, SpineNode,
    SpineNodeFamily, observer_kind_name,
};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpineAnalysisStatus {
    Valid,
    Partial,
    Invalid,
}

pub type SharedDiagnosticStatus = SpineAnalysisStatus;

pub fn spine_analysis_status_name(status: SpineAnalysisStatus) -> &'static str {
    match status {
        SpineAnalysisStatus::Valid => "valid",
        SpineAnalysisStatus::Partial => "partial",
        SpineAnalysisStatus::Invalid => "invalid",
    }
}

pub fn shared_diagnostic_status_name(status: SharedDiagnosticStatus) -> &'static str {
    spine_analysis_status_name(status)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpineBackend {
    Auto,
    Cpu,
    VirtualGpu,
    Wgsl,
    Other(SmolStr),
}

pub fn spine_backend_name(backend: &SpineBackend) -> SmolStr {
    match backend {
        SpineBackend::Auto => SmolStr::new("auto"),
        SpineBackend::Cpu => SmolStr::new("cpu"),
        SpineBackend::VirtualGpu => SmolStr::new("virtual_gpu"),
        SpineBackend::Wgsl => SmolStr::new("wgsl"),
        SpineBackend::Other(name) => name.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineDependencyAnalysis {
    pub status: SpineAnalysisStatus,
    pub node_count: usize,
    pub edge_count: usize,
    pub artifact_edge_count: usize,
    pub policy_edge_count: usize,
    pub output_edge_count: usize,
    pub roots: Vec<SmolStr>,
    pub leaves: Vec<SmolStr>,
    pub topological_order: Vec<SmolStr>,
    pub missing_node_edges: Vec<SpineMissingNodeEdge>,
    pub cycles: Vec<SpineCycle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineMissingNodeEdge {
    pub index: usize,
    pub from: SmolStr,
    pub to: SmolStr,
    pub kind: SpineEdgeKind,
    pub subject: Option<SmolStr>,
    pub missing_from: bool,
    pub missing_to: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedSpineIssue {
    MissingNodeEdge(SpineMissingNodeEdge),
    Cycle(SpineCycle),
    ArtifactContract(SpineArtifactContractValidation),
    ArtifactUse(SpineArtifactUseValidation),
    PolicyRequirement(SpinePolicyNodeSummary),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineCycle {
    pub nodes: Vec<SmolStr>,
    pub self_loop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineEdgeReference {
    pub from: SmolStr,
    pub to: SmolStr,
    pub kind: SpineEdgeKind,
    pub subject: Option<SmolStr>,
    pub required_validity: Option<ArtifactValidityRule>,
    pub lossy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineArtifactLifetimeValidation {
    pub status: SpineAnalysisStatus,
    pub contract_checks: Vec<SpineArtifactContractValidation>,
    pub use_checks: Vec<SpineArtifactUseValidation>,
    pub missing_store_nodes: Vec<SmolStr>,
    pub unexpected_store_nodes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineArtifactContractValidation {
    pub artifact_id: SmolStr,
    pub store_node_id: Option<SmolStr>,
    pub expected_validity: ArtifactValidityRule,
    pub observed_validity: Option<ArtifactValidityRule>,
    pub load_node_ids: Vec<SmolStr>,
    pub preserve_edges: Vec<SpineEdgeReference>,
    pub status: SpineAnalysisStatus,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineArtifactUseValidation {
    pub artifact_id: SmolStr,
    pub actor: SmolStr,
    pub kind: ArtifactUseKind,
    pub source: ArtifactUseSource,
    pub required_validity: Option<ArtifactValidityRule>,
    pub matched_edges: Vec<SpineEdgeReference>,
    pub status: SpineAnalysisStatus,
    pub notes: Vec<SmolStr>,
}

pub type SpineArtifactAccessSummary = SpineArtifactUseValidation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpinePolicyLegalitySummary {
    pub status: SpineAnalysisStatus,
    pub policy_nodes: Vec<SpinePolicyNodeSummary>,
    pub illegal_combinations: Vec<SmolStr>,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpinePolicyNodeSummary {
    pub node_id: SmolStr,
    pub label: SmolStr,
    pub observer_kind: ObserverKind,
    pub backend: Option<SpineBackend>,
    pub backend_preference: Option<SpineBackend>,
    pub required_guarantee: Option<SmolStr>,
    pub selected_method: Option<SmolStr>,
    pub supported_backends: Vec<SpineBackend>,
    pub authority_scope: Option<SmolStr>,
    pub binding_count: Option<u32>,
    pub status: SpineAnalysisStatus,
    pub notes: Vec<SmolStr>,
}

pub type SpinePolicyRequirementSummary = SpinePolicyNodeSummary;

pub fn analyze_dependency_graph(spine: &QueryProgramSpine) -> SpineDependencyAnalysis {
    let node_ids = spine
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let mut outgoing = adjacency_map(&node_ids);
    let mut incoming = adjacency_map(&node_ids);
    let mut missing_node_edges = Vec::new();
    let mut artifact_edge_count = 0usize;
    let mut policy_edge_count = 0usize;
    let mut output_edge_count = 0usize;

    for (index, edge) in spine.dependencies.iter().enumerate() {
        match edge.kind {
            SpineEdgeKind::ConsumesArtifact
            | SpineEdgeKind::ProducesArtifact
            | SpineEdgeKind::LoadsArtifact
            | SpineEdgeKind::StoresArtifact => artifact_edge_count += 1,
            SpineEdgeKind::RequiresPolicy => policy_edge_count += 1,
            SpineEdgeKind::FeedsOutput => output_edge_count += 1,
            SpineEdgeKind::ConsumesInput | SpineEdgeKind::ConsumesValue => {}
        }
        let missing_from = !node_ids.contains(&edge.from);
        let missing_to = !node_ids.contains(&edge.to);
        if missing_from || missing_to {
            missing_node_edges.push(SpineMissingNodeEdge {
                index,
                from: edge.from.clone(),
                to: edge.to.clone(),
                kind: edge.kind,
                subject: edge.subject.clone(),
                missing_from,
                missing_to,
            });
            continue;
        }

        outgoing
            .entry(edge.from.clone())
            .or_default()
            .insert(edge.to.clone());
        incoming
            .entry(edge.to.clone())
            .or_default()
            .insert(edge.from.clone());
    }

    let roots = node_ids
        .iter()
        .filter(|node_id| incoming.get(*node_id).map_or(true, BTreeSet::is_empty))
        .cloned()
        .collect::<Vec<_>>();
    let leaves = node_ids
        .iter()
        .filter(|node_id| outgoing.get(*node_id).map_or(true, BTreeSet::is_empty))
        .cloned()
        .collect::<Vec<_>>();

    let cycles = strongly_connected_components(&node_ids, &outgoing)
        .into_iter()
        .filter_map(|component| {
            let self_loop = component.len() == 1
                && outgoing
                    .get(component.first().expect("component is non-empty"))
                    .is_some_and(|neighbors| {
                        neighbors.contains(component.first().expect("component is non-empty"))
                    });
            if component.len() > 1 || self_loop {
                Some(SpineCycle {
                    nodes: component,
                    self_loop,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let topological_order = topological_order(&node_ids, &incoming, &outgoing);
    let has_unsupported_cycle = cycles
        .iter()
        .any(|cycle| !is_temporal_feedback_cycle(cycle, spine));
    let mut status = SpineAnalysisStatus::Valid;
    if !missing_node_edges.is_empty() || has_unsupported_cycle {
        status = SpineAnalysisStatus::Invalid;
    }

    SpineDependencyAnalysis {
        status,
        node_count: spine.nodes.len(),
        edge_count: spine.dependencies.len(),
        artifact_edge_count,
        policy_edge_count,
        output_edge_count,
        roots,
        leaves,
        topological_order,
        missing_node_edges,
        cycles,
    }
}

pub fn validate_artifact_lifetime(spine: &QueryProgramSpine) -> SpineArtifactLifetimeValidation {
    let node_by_id = spine
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let primitive_nodes = spine
        .nodes
        .iter()
        .filter(|node| node.family == SpineNodeFamily::PrimitiveInvocation)
        .collect::<Vec<_>>();
    let store_nodes = spine
        .nodes
        .iter()
        .filter(|node| node.family == SpineNodeFamily::ArtifactStore)
        .collect::<Vec<_>>();
    let load_nodes = spine
        .nodes
        .iter()
        .filter(|node| node.family == SpineNodeFamily::ArtifactLoad)
        .collect::<Vec<_>>();
    let mut store_nodes_by_artifact = BTreeMap::<SmolStr, &SpineNode>::new();
    let mut load_nodes_by_artifact = BTreeMap::<SmolStr, Vec<&SpineNode>>::new();
    let mut primitive_nodes_by_actor = BTreeMap::<SmolStr, Vec<&SpineNode>>::new();
    let mut load_nodes_by_actor_artifact = BTreeMap::<(SmolStr, SmolStr), Vec<&SpineNode>>::new();
    for node in primitive_nodes {
        if let Some(actor) = node_note_value(node, "pass_id=") {
            primitive_nodes_by_actor
                .entry(SmolStr::new(actor))
                .or_default()
                .push(node);
        }
    }
    for node in store_nodes {
        if let Some(artifact_id) = node.artifact_ids.first() {
            store_nodes_by_artifact.insert(artifact_id.clone(), node);
        }
    }
    for node in load_nodes {
        if let Some(artifact_id) = node.artifact_ids.first() {
            load_nodes_by_artifact
                .entry(artifact_id.clone())
                .or_default()
                .push(node);
            if let Some(actor) = node_note_value(node, "actor=") {
                load_nodes_by_actor_artifact
                    .entry((SmolStr::new(actor), artifact_id.clone()))
                    .or_default()
                    .push(node);
            }
        }
    }

    let mut preserve_edges_by_artifact = BTreeMap::<SmolStr, Vec<&SpineDependencyEdge>>::new();
    let mut load_edges_by_artifact = BTreeMap::<SmolStr, Vec<&SpineDependencyEdge>>::new();
    let mut produce_edges_by_artifact = BTreeMap::<SmolStr, Vec<&SpineDependencyEdge>>::new();
    for edge in &spine.dependencies {
        if let Some(subject) = edge.subject.as_ref() {
            match edge.kind {
                SpineEdgeKind::StoresArtifact => {
                    preserve_edges_by_artifact
                        .entry(subject.clone())
                        .or_default()
                        .push(edge);
                }
                SpineEdgeKind::LoadsArtifact => {
                    load_edges_by_artifact
                        .entry(subject.clone())
                        .or_default()
                        .push(edge);
                }
                SpineEdgeKind::ProducesArtifact => {
                    produce_edges_by_artifact
                        .entry(subject.clone())
                        .or_default()
                        .push(edge);
                }
                SpineEdgeKind::ConsumesArtifact => {}
                SpineEdgeKind::ConsumesInput
                | SpineEdgeKind::ConsumesValue
                | SpineEdgeKind::RequiresPolicy
                | SpineEdgeKind::FeedsOutput => {}
            }
        }
    }

    let mut contract_checks = Vec::new();
    let mut missing_store_nodes = Vec::new();
    let mut unexpected_store_nodes = Vec::new();

    for contract in &spine.semantic_artifacts {
        let store_node_id = store_nodes_by_artifact
            .get(&contract.id)
            .map(|node| node.id.clone());
        let observed_validity = store_node_id
            .as_ref()
            .and_then(|node_id| node_by_id.get(node_id))
            .and_then(|node| node.required_validity.clone());
        let load_node_ids = load_nodes_by_artifact
            .get(&contract.id)
            .map(|nodes| nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        let preserve_edges = preserve_edges_by_artifact
            .get(&contract.id)
            .map(|edges| {
                edges
                    .iter()
                    .map(|edge| SpineEdgeReference {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                        kind: edge.kind,
                        subject: edge.subject.clone(),
                        required_validity: edge.required_validity.clone(),
                        lossy: edge.lossy,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut notes = Vec::new();
        let mut status = SpineAnalysisStatus::Valid;
        match store_node_id.as_ref() {
            Some(node_id) => {
                if observed_validity.as_ref() != Some(&contract.validity) {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new(format!(
                        "store_validity_mismatch node={} expected={:?} observed={:?}",
                        node_id, contract.validity, observed_validity
                    )));
                }
            }
            None => {
                status = SpineAnalysisStatus::Invalid;
                missing_store_nodes.push(contract.id.clone());
                notes.push(SmolStr::new("missing_artifact_store_node"));
            }
        }
        if load_node_ids.is_empty() {
            notes.push(SmolStr::new("no_store_backed_load_nodes"));
        }
        if preserve_edges.is_empty()
            && contract.kind == crate::artifact_contract::SemanticArtifactKind::PresentationHistory
        {
            notes.push(SmolStr::new("no_preserve_edges_for_history_artifact"));
        }
        notes.extend(validate_semantic_contract(spine.observer_kind, contract));
        if notes.iter().any(|note| {
            note.starts_with("semantic_")
                || note.starts_with("presentation_")
                || note.starts_with("collision_")
        }) {
            status = SpineAnalysisStatus::Invalid;
        }

        contract_checks.push(SpineArtifactContractValidation {
            artifact_id: contract.id.clone(),
            store_node_id,
            expected_validity: contract.validity.clone(),
            observed_validity,
            load_node_ids,
            preserve_edges,
            status,
            notes,
        });
    }

    for (artifact_id, node) in &store_nodes_by_artifact {
        if !spine
            .semantic_artifacts
            .iter()
            .any(|contract| &contract.id == artifact_id)
        {
            unexpected_store_nodes.push(artifact_id.clone());
            contract_checks.push(SpineArtifactContractValidation {
                artifact_id: artifact_id.clone(),
                store_node_id: Some(node.id.clone()),
                expected_validity: node
                    .required_validity
                    .clone()
                    .unwrap_or(ArtifactValidityRule::Always),
                observed_validity: node.required_validity.clone(),
                load_node_ids: Vec::new(),
                preserve_edges: Vec::new(),
                status: SpineAnalysisStatus::Partial,
                notes: vec![SmolStr::new("store_node_has_no_matching_semantic_contract")],
            });
        }
    }

    let mut use_checks = Vec::new();
    for use_record in &spine.artifact_uses {
        let mut matched_edges = Vec::new();
        let mut notes = Vec::new();
        let mut status = SpineAnalysisStatus::Valid;
        let actor_nodes = primitive_nodes_by_actor
            .get(&use_record.actor)
            .cloned()
            .unwrap_or_default();
        let actor_node_ids = node_id_set(&actor_nodes);
        let artifact_node_ids = store_nodes_by_artifact
            .get(&use_record.artifact_id)
            .map(|node| BTreeSet::from([node.id.clone()]))
            .unwrap_or_default();
        if actor_node_ids.is_empty() {
            status = SpineAnalysisStatus::Invalid;
            notes.push(SmolStr::new("missing_actor_node"));
        }
        if artifact_node_ids.is_empty() {
            status = SpineAnalysisStatus::Invalid;
            notes.push(SmolStr::new("missing_artifact_store_node"));
        }
        match (use_record.kind, use_record.source) {
            (ArtifactUseKind::Load, ArtifactUseSource::Plan) => {
                if use_record
                    .required_validity
                    .as_ref()
                    .is_some_and(|validity| validity.is_explicit())
                {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("plan_load_must_not_require_explicit_validity"));
                }
                matched_edges.extend(collect_matching_edges(
                    &spine.dependencies,
                    &use_record.artifact_id,
                    SpineEdgeKind::ConsumesArtifact,
                    &artifact_node_ids,
                    &actor_node_ids,
                ));
                if matched_edges.is_empty() {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_plan_load_edge"));
                }
            }
            (ArtifactUseKind::Load, ArtifactUseSource::ArtifactStore) => {
                if !use_record
                    .required_validity
                    .as_ref()
                    .is_some_and(ArtifactValidityRule::is_explicit)
                {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("store_load_requires_explicit_validity"));
                }
                let load_nodes = load_nodes_by_actor_artifact
                    .get(&(use_record.actor.clone(), use_record.artifact_id.clone()))
                    .cloned()
                    .unwrap_or_default();
                let load_node_ids = node_id_set(&load_nodes);
                if let Some(contract) = spine
                    .semantic_artifacts
                    .iter()
                    .find(|contract| contract.id == use_record.artifact_id)
                {
                    if !contract.validity.is_explicit() {
                        status = SpineAnalysisStatus::Invalid;
                        notes.push(SmolStr::new("store_load_contract_must_be_explicit"));
                    }
                    match spine.observer_kind {
                        ObserverKind::Presentation
                            if contract.kind
                                != crate::artifact_contract::SemanticArtifactKind::PresentationHistory =>
                        {
                            status = SpineAnalysisStatus::Invalid;
                            notes.push(SmolStr::new(
                                "presentation_store_load_requires_history_artifact",
                            ));
                        }
                        ObserverKind::Collision
                            if !contract.compatibility.transition.requires_previous_snapshot =>
                        {
                            status = SpineAnalysisStatus::Invalid;
                            notes.push(SmolStr::new(
                                "collision_store_load_requires_transition_compatible_artifact",
                            ));
                        }
                        ObserverKind::Presentation | ObserverKind::Collision => {}
                    }
                }
                if load_node_ids.is_empty() {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_load_node"));
                }
                for node in &load_nodes {
                    if node.required_validity != use_record.required_validity {
                        status = SpineAnalysisStatus::Invalid;
                        notes.push(SmolStr::new("load_node_required_validity_mismatch"));
                    }
                }
                matched_edges.extend(collect_matching_edges(
                    &spine.dependencies,
                    &use_record.artifact_id,
                    SpineEdgeKind::LoadsArtifact,
                    &artifact_node_ids,
                    &load_node_ids,
                ));
                matched_edges.extend(collect_matching_edges(
                    &spine.dependencies,
                    &use_record.artifact_id,
                    SpineEdgeKind::ConsumesArtifact,
                    &load_node_ids,
                    &actor_node_ids,
                ));

                if matched_edges
                    .iter()
                    .filter(|edge| edge.kind == SpineEdgeKind::LoadsArtifact)
                    .count()
                    == 0
                {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_store_load_edge"));
                }
                if matched_edges
                    .iter()
                    .filter(|edge| edge.kind == SpineEdgeKind::ConsumesArtifact)
                    .count()
                    == 0
                {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_load_consume_edge"));
                }
                for edge in matched_edges
                    .iter()
                    .filter(|edge| edge.kind == SpineEdgeKind::LoadsArtifact)
                {
                    if edge.required_validity != use_record.required_validity {
                        status = SpineAnalysisStatus::Invalid;
                        notes.push(SmolStr::new("store_load_edge_required_validity_mismatch"));
                    }
                }
                for edge in matched_edges
                    .iter()
                    .filter(|edge| edge.kind == SpineEdgeKind::ConsumesArtifact)
                {
                    if edge.required_validity != use_record.required_validity {
                        status = SpineAnalysisStatus::Invalid;
                        notes.push(SmolStr::new("load_consume_edge_required_validity_mismatch"));
                    }
                }
            }
            (ArtifactUseKind::Produce, ArtifactUseSource::Plan) => {
                matched_edges.extend(collect_matching_edges(
                    &spine.dependencies,
                    &use_record.artifact_id,
                    SpineEdgeKind::ProducesArtifact,
                    &actor_node_ids,
                    &artifact_node_ids,
                ));
                if matched_edges.is_empty() {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_produce_edge"));
                }
            }
            (ArtifactUseKind::Preserve, ArtifactUseSource::Plan) => {
                if !use_record
                    .required_validity
                    .as_ref()
                    .is_some_and(ArtifactValidityRule::is_explicit)
                {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("preserve_requires_explicit_validity"));
                }
                if let Some(contract) = spine
                    .semantic_artifacts
                    .iter()
                    .find(|contract| contract.id == use_record.artifact_id)
                {
                    match spine.observer_kind {
                        ObserverKind::Presentation
                            if contract.kind
                                != crate::artifact_contract::SemanticArtifactKind::PresentationHistory =>
                        {
                            status = SpineAnalysisStatus::Invalid;
                            notes.push(SmolStr::new(
                                "presentation_preserve_requires_history_artifact",
                            ));
                        }
                        ObserverKind::Collision
                            if !contract.compatibility.transition.requires_previous_snapshot =>
                        {
                            status = SpineAnalysisStatus::Invalid;
                            notes.push(SmolStr::new(
                                "collision_preserve_requires_transition_compatible_artifact",
                            ));
                        }
                        ObserverKind::Presentation | ObserverKind::Collision => {}
                    }
                }
                matched_edges.extend(collect_matching_edges(
                    &spine.dependencies,
                    &use_record.artifact_id,
                    SpineEdgeKind::StoresArtifact,
                    &actor_node_ids,
                    &artifact_node_ids,
                ));
                for edge in &matched_edges {
                    if edge.required_validity != use_record.required_validity {
                        status = SpineAnalysisStatus::Invalid;
                        notes.push(SmolStr::new("preserve_edge_required_validity_mismatch"));
                    }
                }
                if matched_edges.is_empty() {
                    status = SpineAnalysisStatus::Invalid;
                    notes.push(SmolStr::new("missing_preserve_edge"));
                }
            }
            (ArtifactUseKind::Produce, ArtifactUseSource::ArtifactStore)
            | (ArtifactUseKind::Preserve, ArtifactUseSource::ArtifactStore) => {
                status = SpineAnalysisStatus::Invalid;
                notes.push(SmolStr::new("observer_inconsistent_store_sourced_write"));
            }
        }

        use_checks.push(SpineArtifactUseValidation {
            artifact_id: use_record.artifact_id.clone(),
            actor: use_record.actor.clone(),
            kind: use_record.kind,
            source: use_record.source,
            required_validity: use_record.required_validity.clone(),
            matched_edges,
            status,
            notes,
        });
    }

    let status = if contract_checks
        .iter()
        .any(|check| check.status == SpineAnalysisStatus::Invalid)
        || use_checks
            .iter()
            .any(|check| check.status == SpineAnalysisStatus::Invalid)
        || !missing_store_nodes.is_empty()
        || !unexpected_store_nodes.is_empty()
    {
        SpineAnalysisStatus::Invalid
    } else if contract_checks.is_empty() && use_checks.is_empty() {
        SpineAnalysisStatus::Partial
    } else if contract_checks
        .iter()
        .any(|check| check.status == SpineAnalysisStatus::Partial)
        || use_checks
            .iter()
            .any(|check| check.status == SpineAnalysisStatus::Partial)
    {
        SpineAnalysisStatus::Partial
    } else {
        SpineAnalysisStatus::Valid
    };

    SpineArtifactLifetimeValidation {
        status,
        contract_checks,
        use_checks,
        missing_store_nodes,
        unexpected_store_nodes,
    }
}

pub fn validate_artifact_lifetimes(spine: &QueryProgramSpine) -> SpineArtifactLifetimeValidation {
    validate_artifact_lifetime(spine)
}

pub fn summarize_policy_legality(spine: &QueryProgramSpine) -> SpinePolicyLegalitySummary {
    let policy_nodes = spine
        .nodes
        .iter()
        .filter(|node| node.family == SpineNodeFamily::PolicyRequirement)
        .map(|node| summarize_policy_node(spine.observer_kind, node))
        .collect::<Vec<_>>();

    let mut illegal_combinations = Vec::new();
    for node in &policy_nodes {
        if let Some(reason) = illegal_policy_reason(node) {
            illegal_combinations.push(SmolStr::new(format!("{}: {reason}", node.node_id)));
        }
    }

    let status = if policy_nodes
        .iter()
        .any(|node| node.status == SpineAnalysisStatus::Invalid)
        || !illegal_combinations.is_empty()
    {
        SpineAnalysisStatus::Invalid
    } else if policy_nodes.is_empty() {
        SpineAnalysisStatus::Partial
    } else if policy_nodes
        .iter()
        .any(|node| node.status == SpineAnalysisStatus::Partial)
    {
        SpineAnalysisStatus::Partial
    } else {
        SpineAnalysisStatus::Valid
    };

    SpinePolicyLegalitySummary {
        status,
        policy_nodes,
        illegal_combinations,
        notes: vec![SmolStr::new(format!(
            "observer_kind={}",
            observer_kind_name(spine.observer_kind)
        ))],
    }
}

fn summarize_policy_node(observer_kind: ObserverKind, node: &SpineNode) -> SpinePolicyNodeSummary {
    let mut backend = None;
    let mut backend_preference = None;
    let mut required_guarantee = None;
    let mut selected_method = None;
    let mut supported_backends = Vec::new();
    let mut authority_scope = None;
    let mut binding_count = None;
    let mut notes = Vec::new();

    for note in &node.notes {
        if let Some(value) = note.strip_prefix("backend=") {
            backend = Some(parse_backend(value));
        } else if let Some(value) = note.strip_prefix("backend_preference=") {
            backend_preference = Some(parse_backend(value));
        } else if let Some(value) = note.strip_prefix("default_backends=") {
            supported_backends.extend(parse_backend_list(value));
        } else if let Some(value) = note.strip_prefix("supported_backends=") {
            supported_backends.extend(parse_backend_list(value));
        } else if let Some(value) = note.strip_prefix("backends=") {
            supported_backends.extend(parse_backend_list(value));
        } else if let Some(value) = note.strip_prefix("authority_scope=") {
            authority_scope = Some(SmolStr::new(value));
        } else if let Some(value) = note.strip_prefix("binding_count=") {
            binding_count = value.parse::<u32>().ok();
        } else if let Some(value) = note.strip_prefix("required_guarantee=") {
            required_guarantee = Some(SmolStr::new(value));
        } else if let Some(value) = note.strip_prefix("selected_method=") {
            selected_method = Some(SmolStr::new(value));
        }
        notes.push(note.clone());
    }

    supported_backends.sort();
    supported_backends.dedup();

    let mut status = SpineAnalysisStatus::Valid;
    if backend.is_none() && supported_backends.is_empty() {
        status = SpineAnalysisStatus::Partial;
        notes.push(SmolStr::new("policy_backend_not_explicitly_declared"));
    }

    SpinePolicyNodeSummary {
        node_id: node.id.clone(),
        label: node.label.clone(),
        observer_kind,
        backend,
        backend_preference,
        required_guarantee,
        selected_method,
        supported_backends,
        authority_scope,
        binding_count,
        status,
        notes,
    }
}

fn illegal_policy_reason(node: &SpinePolicyNodeSummary) -> Option<String> {
    let backend = node.backend.as_ref()?;
    if !node.supported_backends.is_empty()
        && *backend != SpineBackend::Auto
        && !node
            .supported_backends
            .iter()
            .any(|supported| supported == backend)
    {
        return Some(format!(
            "backend={} is outside supported_backends={}",
            spine_backend_name(backend),
            node.supported_backends
                .iter()
                .map(spine_backend_name)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let exact_backend = matches!(backend, SpineBackend::VirtualGpu | SpineBackend::Wgsl);
    if !exact_backend {
        return None;
    }

    let exact_guarantee = node
        .required_guarantee
        .as_deref()
        .is_some_and(|value| value == "exact");
    let exact_method = node
        .selected_method
        .as_deref()
        .is_some_and(|value| value == "exact_oracle");
    if exact_guarantee || exact_method {
        Some(format!(
            "backend={} is incompatible with guarantee={} method={}",
            spine_backend_name(backend),
            node.required_guarantee.as_deref().unwrap_or("none"),
            node.selected_method.as_deref().unwrap_or("none"),
        ))
    } else {
        None
    }
}

fn parse_backend(value: &str) -> SpineBackend {
    match value.trim() {
        "auto" => SpineBackend::Auto,
        "cpu" => SpineBackend::Cpu,
        "virtual_gpu" | "vgpu" => SpineBackend::VirtualGpu,
        "wgsl" => SpineBackend::Wgsl,
        other => SpineBackend::Other(SmolStr::new(other)),
    }
}

fn parse_backend_list(value: &str) -> Vec<SpineBackend> {
    value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter(|entry| !entry.is_empty())
        .map(parse_backend)
        .collect()
}

fn is_temporal_feedback_cycle(cycle: &SpineCycle, spine: &QueryProgramSpine) -> bool {
    let cycle_node_ids = cycle.nodes.iter().cloned().collect::<BTreeSet<_>>();
    let explicit_artifacts = spine
        .semantic_artifacts
        .iter()
        .filter(|contract| contract.validity.is_explicit())
        .map(|contract| contract.id.clone())
        .collect::<BTreeSet<_>>();
    let mut load_nodes_by_artifact = BTreeMap::<SmolStr, BTreeSet<SmolStr>>::new();
    let mut store_nodes_by_artifact = BTreeMap::<SmolStr, BTreeSet<SmolStr>>::new();
    for node in spine
        .nodes
        .iter()
        .filter(|node| cycle_node_ids.contains(&node.id))
    {
        match node.family {
            SpineNodeFamily::ArtifactLoad => {
                let Some(artifact_id) = node.artifact_ids.first() else {
                    return false;
                };
                if explicit_artifacts.contains(artifact_id) {
                    load_nodes_by_artifact
                        .entry(artifact_id.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
            }
            SpineNodeFamily::ArtifactStore => {
                let Some(artifact_id) = node.artifact_ids.first() else {
                    return false;
                };
                if explicit_artifacts.contains(artifact_id) {
                    store_nodes_by_artifact
                        .entry(artifact_id.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
            }
            SpineNodeFamily::InputBinding
            | SpineNodeFamily::PrimitiveInvocation
            | SpineNodeFamily::PolicyRequirement
            | SpineNodeFamily::OutputBinding
            | SpineNodeFamily::ObservabilitySummary => {}
        }
    }

    let in_cycle_edges = spine
        .dependencies
        .iter()
        .filter(|edge| cycle_node_ids.contains(&edge.from) && cycle_node_ids.contains(&edge.to))
        .collect::<Vec<_>>();
    if in_cycle_edges.is_empty() {
        return false;
    }

    let temporal_feedback_edge_indexes = in_cycle_edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let artifact_id = edge.subject.as_ref()?;
            let load_nodes = load_nodes_by_artifact.get(artifact_id)?;
            let store_nodes = store_nodes_by_artifact.get(artifact_id)?;
            let is_temporal_feedback_edge = match edge.kind {
                SpineEdgeKind::LoadsArtifact => {
                    store_nodes.contains(&edge.from) && load_nodes.contains(&edge.to)
                }
                SpineEdgeKind::ConsumesArtifact => load_nodes.contains(&edge.from),
                SpineEdgeKind::StoresArtifact | SpineEdgeKind::ProducesArtifact => {
                    store_nodes.contains(&edge.to)
                }
                SpineEdgeKind::ConsumesInput
                | SpineEdgeKind::ConsumesValue
                | SpineEdgeKind::RequiresPolicy
                | SpineEdgeKind::FeedsOutput => false,
            };
            is_temporal_feedback_edge.then_some(index)
        })
        .collect::<BTreeSet<_>>();

    let has_temporal_feedback = load_nodes_by_artifact.keys().any(|artifact_id| {
        let Some(load_nodes) = load_nodes_by_artifact.get(artifact_id) else {
            return false;
        };
        let Some(store_nodes) = store_nodes_by_artifact.get(artifact_id) else {
            return false;
        };
        let has_load_edge = in_cycle_edges.iter().any(|edge| {
            edge.kind == SpineEdgeKind::LoadsArtifact
                && edge.subject.as_ref() == Some(artifact_id)
                && store_nodes.contains(&edge.from)
                && load_nodes.contains(&edge.to)
        });
        let has_consume_edge = in_cycle_edges.iter().any(|edge| {
            edge.kind == SpineEdgeKind::ConsumesArtifact
                && edge.subject.as_ref() == Some(artifact_id)
                && load_nodes.contains(&edge.from)
        });
        let has_store_edge = in_cycle_edges.iter().any(|edge| {
            matches!(
                edge.kind,
                SpineEdgeKind::StoresArtifact | SpineEdgeKind::ProducesArtifact
            ) && edge.subject.as_ref() == Some(artifact_id)
                && store_nodes.contains(&edge.to)
        });
        has_load_edge && has_consume_edge && has_store_edge
    });
    if !has_temporal_feedback {
        return false;
    }

    let residual_outgoing = cycle_node_ids
        .iter()
        .map(|node_id| (node_id.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut residual_outgoing = residual_outgoing;
    for (index, edge) in in_cycle_edges.iter().enumerate() {
        if temporal_feedback_edge_indexes.contains(&index) {
            continue;
        }
        residual_outgoing
            .entry(edge.from.clone())
            .or_default()
            .insert(edge.to.clone());
    }

    let residual_has_cycle = strongly_connected_components(&cycle_node_ids, &residual_outgoing)
        .into_iter()
        .any(|component| {
            component.len() > 1
                || component.first().is_some_and(|node_id| {
                    residual_outgoing
                        .get(node_id)
                        .is_some_and(|neighbors| neighbors.contains(node_id))
                })
        });
    if residual_has_cycle {
        return false;
    }

    true
}

fn validate_semantic_contract(
    observer_kind: ObserverKind,
    contract: &crate::artifact_contract::SemanticArtifactContract,
) -> Vec<SmolStr> {
    let mut notes = Vec::new();
    match observer_kind {
        ObserverKind::Presentation => match contract.kind {
            SemanticArtifactKind::PresentationHistory => {
                if contract.compatibility.snapshot
                    != ArtifactSnapshotRelation::PreviousSnapshotEpoch
                {
                    notes.push(SmolStr::new(
                        "presentation_history_requires_previous_snapshot_scope",
                    ));
                }
                if !contract.compatibility.transition.requires_previous_snapshot {
                    notes.push(SmolStr::new(
                        "presentation_history_requires_previous_snapshot_transition",
                    ));
                }
                if contract.compatibility.policy.mode != ArtifactPolicyDigestMode::CompatibleRange {
                    notes.push(SmolStr::new(
                        "presentation_history_requires_compatible_range_policy",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
                ) {
                    notes.push(SmolStr::new(
                        "presentation_history_missing_previous_snapshot_predicate",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::LayoutSignatureMatches,
                ) {
                    notes.push(SmolStr::new(
                        "presentation_history_missing_layout_signature_predicate",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::HistoryCompatibilityMatches,
                ) {
                    notes.push(SmolStr::new(
                        "presentation_history_missing_history_compatibility_predicate",
                    ));
                }
                if !validity_contains_max_presentation_age(&contract.validity) {
                    notes.push(SmolStr::new(
                        "presentation_history_missing_max_frame_age_predicate",
                    ));
                }
                if let Some(compatibility) = contract.compatibility.transition.compatibility {
                    if !validity_contains_predicate(
                        &contract.validity,
                        &ArtifactValidityPredicate::CompatibleChange(compatibility),
                    ) {
                        notes.push(SmolStr::new(
                            "presentation_history_missing_compatible_change_predicate",
                        ));
                    }
                }
            }
            SemanticArtifactKind::PresentationAttachment => {
                if contract.compatibility.snapshot != ArtifactSnapshotRelation::ExactSnapshot {
                    notes.push(SmolStr::new(
                        "presentation_attachment_requires_exact_snapshot_scope",
                    ));
                }
                if contract.compatibility.transition.requires_previous_snapshot {
                    notes.push(SmolStr::new(
                        "presentation_attachment_must_not_require_previous_snapshot",
                    ));
                }
            }
            SemanticArtifactKind::Query => {}
        },
        ObserverKind::Collision => match collision_artifact_semantics(contract) {
            Some(CollisionArtifactSemantics::ExactSnapshot) => {
                if contract.compatibility.snapshot != ArtifactSnapshotRelation::ExactSnapshot {
                    notes.push(SmolStr::new(
                        "collision_exact_artifact_requires_exact_snapshot_scope",
                    ));
                }
                if contract.compatibility.transition.requires_previous_snapshot {
                    notes.push(SmolStr::new(
                        "collision_exact_artifact_must_not_require_previous_snapshot",
                    ));
                }
            }
            Some(CollisionArtifactSemantics::TransitionHistory) => {
                if contract.compatibility.snapshot
                    != ArtifactSnapshotRelation::PreviousSnapshotEpoch
                {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_requires_previous_snapshot_scope",
                    ));
                }
                if !contract.compatibility.transition.requires_previous_snapshot {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_requires_previous_snapshot",
                    ));
                }
                if contract.compatibility.policy.mode != ArtifactPolicyDigestMode::CompatibleRange {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_requires_compatible_range_policy",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
                ) {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_missing_previous_snapshot_predicate",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::HistoryCompatibilityMatches,
                ) {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_missing_history_compatibility_predicate",
                    ));
                }
                if !validity_contains_predicate(
                    &contract.validity,
                    &ArtifactValidityPredicate::EvidenceSummaryMatches,
                ) {
                    notes.push(SmolStr::new(
                        "collision_transition_artifact_missing_evidence_summary_predicate",
                    ));
                }
                if let Some(compatibility) = contract.compatibility.transition.compatibility {
                    if !validity_contains_predicate(
                        &contract.validity,
                        &ArtifactValidityPredicate::CompatibleChange(compatibility),
                    ) {
                        notes.push(SmolStr::new(
                            "collision_transition_artifact_missing_compatible_change_predicate",
                        ));
                    }
                }
            }
            None => {}
        },
    }
    notes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionArtifactSemantics {
    ExactSnapshot,
    TransitionHistory,
}

fn collision_artifact_semantics(
    contract: &crate::artifact_contract::SemanticArtifactContract,
) -> Option<CollisionArtifactSemantics> {
    match semantic_contract_field(contract, "artifact_kind") {
        Some("support_summary") | Some("broadphase_candidates") => {
            Some(CollisionArtifactSemantics::ExactSnapshot)
        }
        Some("witness_cache") | Some("continuation_seed") => {
            Some(CollisionArtifactSemantics::TransitionHistory)
        }
        _ if contract.compatibility.transition.requires_previous_snapshot
            || contract.compatibility.snapshot
                == ArtifactSnapshotRelation::PreviousSnapshotEpoch =>
        {
            Some(CollisionArtifactSemantics::TransitionHistory)
        }
        _ if contract.logical_schema.namespace == "collision" => {
            Some(CollisionArtifactSemantics::ExactSnapshot)
        }
        _ => None,
    }
}

fn semantic_contract_field<'a>(
    contract: &'a crate::artifact_contract::SemanticArtifactContract,
    field_name: &str,
) -> Option<&'a str> {
    contract
        .logical_schema
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .map(|field| field.value.as_str())
}

fn validity_contains_predicate(
    rule: &ArtifactValidityRule,
    predicate: &ArtifactValidityPredicate,
) -> bool {
    match rule {
        ArtifactValidityRule::Always => false,
        ArtifactValidityRule::All(rules) | ArtifactValidityRule::Any(rules) => rules
            .iter()
            .any(|rule| validity_contains_predicate(rule, predicate)),
        ArtifactValidityRule::Predicate(current) => current == predicate,
    }
}

fn validity_contains_max_presentation_age(rule: &ArtifactValidityRule) -> bool {
    match rule {
        ArtifactValidityRule::Always => false,
        ArtifactValidityRule::All(rules) | ArtifactValidityRule::Any(rules) => {
            rules.iter().any(validity_contains_max_presentation_age)
        }
        ArtifactValidityRule::Predicate(ArtifactValidityPredicate::MaxPresentationFrameAge(_)) => {
            true
        }
        ArtifactValidityRule::Predicate(_) => false,
    }
}

fn node_note_value<'a>(node: &'a SpineNode, prefix: &str) -> Option<&'a str> {
    node.notes.iter().find_map(|note| note.strip_prefix(prefix))
}

fn node_id_set(nodes: &[&SpineNode]) -> BTreeSet<SmolStr> {
    nodes.iter().map(|node| node.id.clone()).collect()
}

fn collect_matching_edges(
    edges: &[SpineDependencyEdge],
    subject: &SmolStr,
    kind: SpineEdgeKind,
    from_ids: &BTreeSet<SmolStr>,
    to_ids: &BTreeSet<SmolStr>,
) -> Vec<SpineEdgeReference> {
    edges
        .iter()
        .filter(|edge| {
            edge.kind == kind
                && edge.subject.as_ref() == Some(subject)
                && from_ids.contains(&edge.from)
                && to_ids.contains(&edge.to)
        })
        .map(|edge| SpineEdgeReference {
            from: edge.from.clone(),
            to: edge.to.clone(),
            kind: edge.kind,
            subject: edge.subject.clone(),
            required_validity: edge.required_validity.clone(),
            lossy: edge.lossy,
        })
        .collect()
}

fn adjacency_map(node_ids: &BTreeSet<SmolStr>) -> BTreeMap<SmolStr, BTreeSet<SmolStr>> {
    node_ids
        .iter()
        .map(|node_id| (node_id.clone(), BTreeSet::new()))
        .collect()
}

fn topological_order(
    node_ids: &BTreeSet<SmolStr>,
    incoming: &BTreeMap<SmolStr, BTreeSet<SmolStr>>,
    outgoing: &BTreeMap<SmolStr, BTreeSet<SmolStr>>,
) -> Vec<SmolStr> {
    let mut indegree = node_ids
        .iter()
        .map(|node_id| {
            let count = incoming
                .get(node_id)
                .map(|neighbors| neighbors.len())
                .unwrap_or(0);
            (node_id.clone(), count)
        })
        .collect::<BTreeMap<_, _>>();

    let mut ready = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(node_id, _)| node_id.clone())
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::new();
    let mut processed = BTreeSet::new();

    while let Some(node_id) = ready.iter().next().cloned() {
        ready.remove(&node_id);
        if !processed.insert(node_id.clone()) {
            continue;
        }
        ordered.push(node_id.clone());
        if let Some(neighbors) = outgoing.get(&node_id) {
            for neighbor in neighbors {
                if let Some(entry) = indegree.get_mut(neighbor) {
                    *entry = entry.saturating_sub(1);
                    if *entry == 0 {
                        ready.insert(neighbor.clone());
                    }
                }
            }
        }
    }

    for node_id in node_ids {
        if !processed.contains(node_id) {
            ordered.push(node_id.clone());
        }
    }

    ordered
}

fn strongly_connected_components(
    node_ids: &BTreeSet<SmolStr>,
    outgoing: &BTreeMap<SmolStr, BTreeSet<SmolStr>>,
) -> Vec<Vec<SmolStr>> {
    let mut visited = BTreeSet::new();
    let mut finish_order = Vec::new();
    for node_id in node_ids {
        if !visited.contains(node_id) {
            dfs_finish(node_id, outgoing, &mut visited, &mut finish_order);
        }
    }

    let mut reverse = adjacency_map(node_ids);
    for (from, neighbors) in outgoing {
        for neighbor in neighbors {
            reverse
                .entry(neighbor.clone())
                .or_default()
                .insert(from.clone());
        }
    }

    let mut components = Vec::new();
    let mut assigned = BTreeSet::new();
    for node_id in finish_order.into_iter().rev() {
        if assigned.contains(&node_id) {
            continue;
        }
        let mut component = Vec::new();
        dfs_collect(&node_id, &reverse, &mut assigned, &mut component);
        component.sort();
        components.push(component);
    }

    components.sort_by(|left, right| {
        left.first()
            .cmp(&right.first())
            .then(left.len().cmp(&right.len()))
    });
    components
}

fn dfs_finish(
    node_id: &SmolStr,
    outgoing: &BTreeMap<SmolStr, BTreeSet<SmolStr>>,
    visited: &mut BTreeSet<SmolStr>,
    finish_order: &mut Vec<SmolStr>,
) {
    if !visited.insert(node_id.clone()) {
        return;
    }
    if let Some(neighbors) = outgoing.get(node_id) {
        for neighbor in neighbors {
            dfs_finish(neighbor, outgoing, visited, finish_order);
        }
    }
    finish_order.push(node_id.clone());
}

fn dfs_collect(
    node_id: &SmolStr,
    incoming: &BTreeMap<SmolStr, BTreeSet<SmolStr>>,
    assigned: &mut BTreeSet<SmolStr>,
    component: &mut Vec<SmolStr>,
) {
    if !assigned.insert(node_id.clone()) {
        return;
    }
    component.push(node_id.clone());
    if let Some(neighbors) = incoming.get(node_id) {
        for neighbor in neighbors {
            dfs_collect(neighbor, incoming, assigned, component);
        }
    }
}
