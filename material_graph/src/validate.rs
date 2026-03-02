use crate::types::{DefaultProfileContractsV1, MaterialGraphIRV1, ReflectionFallbackMode};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};

pub fn validate_graph(graph: &MaterialGraphIRV1) -> Result<(), String> {
    let mut node_ids: HashSet<&str> = HashSet::with_capacity(graph.nodes.len());

    for node in &graph.nodes {
        if !node_ids.insert(node.id.as_str()) {
            return Err(format!("duplicate node id: {}", node.id));
        }
    }

    for edge in &graph.edges {
        if edge.from == edge.to {
            return Err(format!(
                "self-edge is not allowed: {} -> {}",
                edge.from, edge.to
            ));
        }
        if !node_ids.contains(edge.from.as_str()) {
            return Err(format!("edge references missing from-node: {}", edge.from));
        }
        if !node_ids.contains(edge.to.as_str()) {
            return Err(format!("edge references missing to-node: {}", edge.to));
        }
    }

    let mut in_degree: HashMap<String, usize> = HashMap::with_capacity(graph.nodes.len());
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::with_capacity(graph.nodes.len());

    for node in &graph.nodes {
        in_degree.insert(node.id.clone(), 0);
        adjacency.insert(node.id.clone(), Vec::new());
    }

    for edge in &graph.edges {
        adjacency
            .get_mut(&edge.from)
            .expect("from-node validated earlier")
            .push(edge.to.clone());
        *in_degree
            .get_mut(&edge.to)
            .expect("to-node validated earlier") += 1;
    }

    let mut queue = VecDeque::new();
    for (id, degree) in &in_degree {
        if *degree == 0 {
            queue.push_back(id.clone());
        }
    }

    let mut visited = 0usize;
    while let Some(node_id) = queue.pop_front() {
        visited += 1;

        if let Some(neighbors) = adjacency.get(&node_id) {
            for neighbor in neighbors {
                let degree = in_degree
                    .get_mut(neighbor)
                    .expect("neighbor should exist in in-degree map");
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    if visited != graph.nodes.len() {
        return Err("graph contains a cycle".to_string());
    }

    Ok(())
}

pub fn graph_fingerprint(graph: &MaterialGraphIRV1) -> Result<String, String> {
    validate_graph(graph)?;

    let serialized = serde_json::to_vec(graph)
        .map_err(|err| format!("failed to serialize graph for fingerprinting: {err}"))?;

    let mut hasher = Sha256::new();
    hasher.update(serialized);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn validate_default_profile_contracts(
    contracts: &DefaultProfileContractsV1,
) -> Result<(), String> {
    if contracts.profile.trim().is_empty() {
        return Err("default profile contracts must declare a non-empty profile id".to_string());
    }

    let lighting = &contracts.lighting;
    if !lighting.pbr_enabled {
        return Err("WL05 contract violation: lighting.pbr_enabled must be true".to_string());
    }
    if !lighting.hdr_enabled {
        return Err("WL05 contract violation: lighting.hdr_enabled must be true".to_string());
    }
    if !lighting.clustered_lighting.enabled {
        return Err(
            "WL05 contract violation: lighting.clustered_lighting.enabled must be true".to_string(),
        );
    }
    if lighting.clustered_lighting.max_lights_per_cluster == 0 {
        return Err(
            "WL05 contract violation: lighting.clustered_lighting.max_lights_per_cluster must be > 0"
                .to_string(),
        );
    }
    if !lighting.clustered_lighting.shadow.enabled {
        return Err(
            "WL05 contract violation: lighting.clustered_lighting.shadow.enabled must be true"
                .to_string(),
        );
    }
    if lighting.clustered_lighting.shadow.cascade_count == 0 {
        return Err(
            "WL05 contract violation: lighting.clustered_lighting.shadow.cascade_count must be > 0"
                .to_string(),
        );
    }
    if lighting.clustered_lighting.shadow.atlas_resolution == 0 {
        return Err(
            "WL05 contract violation: lighting.clustered_lighting.shadow.atlas_resolution must be > 0"
                .to_string(),
        );
    }

    let reflections = &contracts.reflections;
    if reflections.fallback_chain
        != vec![
            ReflectionFallbackMode::Planar,
            ReflectionFallbackMode::Ssr,
            ReflectionFallbackMode::Probe,
        ]
    {
        return Err(
            "WL06 contract violation: reflection fallback_chain must be exactly [planar, ssr, probe]"
                .to_string(),
        );
    }
    if reflections.planar_budget.max_planes == 0 {
        return Err(
            "WL06 contract violation: reflections.planar_budget.max_planes must be > 0".to_string(),
        );
    }
    if !(0.0..=1.0).contains(&reflections.planar_budget.resolution_scale)
        || reflections.planar_budget.resolution_scale == 0.0
    {
        return Err(
            "WL06 contract violation: reflections.planar_budget.resolution_scale must be in (0, 1]"
                .to_string(),
        );
    }
    if reflections.ssr_budget.max_steps == 0 {
        return Err(
            "WL06 contract violation: reflections.ssr_budget.max_steps must be > 0".to_string(),
        );
    }
    if reflections.ssr_budget.max_rays_per_pixel == 0 {
        return Err(
            "WL06 contract violation: reflections.ssr_budget.max_rays_per_pixel must be > 0"
                .to_string(),
        );
    }
    if reflections.probe_budget.max_active_probes == 0 {
        return Err(
            "WL06 contract violation: reflections.probe_budget.max_active_probes must be > 0"
                .to_string(),
        );
    }
    if !(0.0..=1.0).contains(&reflections.probe_budget.update_ratio) {
        return Err(
            "WL06 contract violation: reflections.probe_budget.update_ratio must be in [0, 1]"
                .to_string(),
        );
    }

    let temporal = &contracts.temporal;
    if !temporal.motion_vectors_enabled {
        return Err(
            "WL07 contract violation: temporal.motion_vectors_enabled must be true".to_string(),
        );
    }
    if !temporal.taa_enabled {
        return Err("WL07 contract violation: temporal.taa_enabled must be true".to_string());
    }
    if !temporal.temporal_upscaling_enabled {
        return Err(
            "WL07 contract violation: temporal.temporal_upscaling_enabled must be true".to_string(),
        );
    }
    if !temporal.reactive_mask_enabled {
        return Err(
            "WL07 contract violation: temporal.reactive_mask_enabled must be true".to_string(),
        );
    }
    if !temporal.disocclusion_mask_enabled {
        return Err(
            "WL07 contract violation: temporal.disocclusion_mask_enabled must be true".to_string(),
        );
    }
    let dynamic = &temporal.dynamic_resolution_policy;
    if !dynamic.enabled {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.enabled must be true"
                .to_string(),
        );
    }
    if dynamic.min_scale <= 0.0 || dynamic.min_scale > 1.0 {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.min_scale must be in (0, 1]"
                .to_string(),
        );
    }
    if dynamic.max_scale <= 0.0 || dynamic.max_scale > 1.0 {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.max_scale must be in (0, 1]"
                .to_string(),
        );
    }
    if dynamic.min_scale > dynamic.max_scale {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.min_scale must be <= max_scale"
                .to_string(),
        );
    }
    if dynamic.target_frame_time_ms <= 0.0 {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.target_frame_time_ms must be > 0"
                .to_string(),
        );
    }
    if dynamic.scale_step <= 0.0 {
        return Err(
            "WL07 contract violation: temporal.dynamic_resolution_policy.scale_step must be > 0"
                .to_string(),
        );
    }
    if temporal.metrics.window_frames == 0 {
        return Err(
            "WL07 contract violation: temporal.metrics.window_frames must be > 0".to_string(),
        );
    }
    if temporal.metrics.report_interval_ms == 0 {
        return Err(
            "WL07 contract violation: temporal.metrics.report_interval_ms must be > 0".to_string(),
        );
    }
    if temporal.metrics.max_jitter_ms < 0.0 {
        return Err(
            "WL07 contract violation: temporal.metrics.max_jitter_ms must be >= 0".to_string(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{graph_fingerprint, validate_default_profile_contracts, validate_graph};
    use crate::types::{
        ClusteredLightingContractV1, DefaultProfileContractsV1, DynamicResolutionPolicyV1,
        LightingContractV1, MaterialEdge, MaterialGraphIRV1, MaterialNode,
        ReflectionFallbackContractV1, ReflectionFallbackMode, ReflectionPlanarBudgetV1,
        ReflectionProbeBudgetV1, ReflectionSsrBudgetV1, ShadowContractV1, ShadowQualityTier,
        TemporalMetricsContractV1, TemporalStackContractV1, TemporalUpscalerMode, TonemapOperator,
    };
    use std::collections::BTreeMap;

    fn base_graph() -> MaterialGraphIRV1 {
        MaterialGraphIRV1 {
            schema_version: 1,
            kind: "material_graph".to_string(),
            graph_id: "graph_1".to_string(),
            nodes: vec![
                MaterialNode {
                    id: "a".to_string(),
                    kind: "constant".to_string(),
                    params: BTreeMap::new(),
                },
                MaterialNode {
                    id: "b".to_string(),
                    kind: "output".to_string(),
                    params: BTreeMap::new(),
                },
            ],
            edges: vec![MaterialEdge {
                from: "a".to_string(),
                to: "b".to_string(),
            }],
        }
    }

    fn base_default_profile_contracts() -> DefaultProfileContractsV1 {
        DefaultProfileContractsV1 {
            schema_version: 1,
            profile: "default".to_string(),
            lighting: LightingContractV1 {
                pbr_enabled: true,
                hdr_enabled: true,
                tonemap_operator: TonemapOperator::Aces,
                clustered_lighting: ClusteredLightingContractV1 {
                    enabled: true,
                    max_lights_per_cluster: 64,
                    shadow: ShadowContractV1 {
                        enabled: true,
                        cascade_count: 4,
                        atlas_resolution: 2048,
                        quality_tier: ShadowQualityTier::High,
                    },
                },
            },
            reflections: ReflectionFallbackContractV1 {
                fallback_chain: vec![
                    ReflectionFallbackMode::Planar,
                    ReflectionFallbackMode::Ssr,
                    ReflectionFallbackMode::Probe,
                ],
                planar_budget: ReflectionPlanarBudgetV1 {
                    max_planes: 2,
                    resolution_scale: 1.0,
                },
                ssr_budget: ReflectionSsrBudgetV1 {
                    max_steps: 32,
                    max_rays_per_pixel: 1,
                },
                probe_budget: ReflectionProbeBudgetV1 {
                    max_active_probes: 32,
                    update_ratio: 0.25,
                },
            },
            temporal: TemporalStackContractV1 {
                motion_vectors_enabled: true,
                taa_enabled: true,
                temporal_upscaling_enabled: true,
                temporal_upscaler_mode: TemporalUpscalerMode::Taa,
                reactive_mask_enabled: true,
                disocclusion_mask_enabled: true,
                dynamic_resolution_policy: DynamicResolutionPolicyV1 {
                    enabled: true,
                    min_scale: 0.6,
                    max_scale: 1.0,
                    target_frame_time_ms: 16.7,
                    scale_step: 0.05,
                },
                metrics: TemporalMetricsContractV1 {
                    window_frames: 120,
                    report_interval_ms: 1000,
                    max_jitter_ms: 0.75,
                },
            },
        }
    }

    #[test]
    fn detects_cycle() {
        let mut graph = base_graph();
        graph.edges.push(MaterialEdge {
            from: "b".to_string(),
            to: "a".to_string(),
        });

        let err = validate_graph(&graph).expect_err("expected cycle detection error");
        assert!(err.contains("cycle"), "unexpected error: {err}");
    }

    #[test]
    fn detects_missing_nodes() {
        let mut graph = base_graph();
        graph.edges.push(MaterialEdge {
            from: "a".to_string(),
            to: "missing".to_string(),
        });

        let err = validate_graph(&graph).expect_err("expected missing node error");
        assert!(err.contains("missing to-node"), "unexpected error: {err}");
    }

    #[test]
    fn detects_duplicate_nodes() {
        let mut graph = base_graph();
        graph.nodes.push(MaterialNode {
            id: "a".to_string(),
            kind: "constant".to_string(),
            params: BTreeMap::new(),
        });

        let err = validate_graph(&graph).expect_err("expected duplicate node id error");
        assert!(err.contains("duplicate node id"), "unexpected error: {err}");
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let mut graph_a = base_graph();
        let mut graph_b = base_graph();

        let mut params_a = BTreeMap::new();
        params_a.insert("roughness".to_string(), 0.5);
        params_a.insert("metallic".to_string(), 0.2);

        let mut params_b = BTreeMap::new();
        params_b.insert("metallic".to_string(), 0.2);
        params_b.insert("roughness".to_string(), 0.5);

        graph_a.nodes[0].params = params_a;
        graph_b.nodes[0].params = params_b;

        let fingerprint_a = graph_fingerprint(&graph_a).expect("fingerprint should succeed");
        let fingerprint_b = graph_fingerprint(&graph_b).expect("fingerprint should succeed");

        assert_eq!(fingerprint_a, fingerprint_b);
        assert_eq!(fingerprint_a.len(), 64);
    }

    #[test]
    fn wl05_rejects_missing_lighting_contract_components() {
        let mut contracts = base_default_profile_contracts();
        contracts.lighting.clustered_lighting.shadow.enabled = false;

        let err = validate_default_profile_contracts(&contracts)
            .expect_err("missing shadow contract should fail");
        assert!(err.contains("WL05"), "unexpected error: {err}");
    }

    #[test]
    fn wl06_rejects_incomplete_reflection_fallback_chain() {
        let mut contracts = base_default_profile_contracts();
        contracts.reflections.fallback_chain = vec![
            ReflectionFallbackMode::Planar,
            ReflectionFallbackMode::Probe,
        ];

        let err = validate_default_profile_contracts(&contracts)
            .expect_err("incomplete fallback chain should fail");
        assert!(err.contains("WL06"), "unexpected error: {err}");
    }

    #[test]
    fn wl07_rejects_invalid_temporal_dynamic_resolution_policy() {
        let mut contracts = base_default_profile_contracts();
        contracts.temporal.dynamic_resolution_policy.min_scale = 1.1;

        let err = validate_default_profile_contracts(&contracts)
            .expect_err("invalid dynamic resolution policy should fail");
        assert!(err.contains("WL07"), "unexpected error: {err}");
    }

    #[test]
    fn wl05_wl06_wl07_accept_complete_default_profile_contract() {
        let contracts = base_default_profile_contracts();
        validate_default_profile_contracts(&contracts).expect("complete contracts should pass");
    }
}
