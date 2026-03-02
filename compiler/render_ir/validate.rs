use crate::render_ir::types::{RenderGraphContractV5, RenderPassContractV5};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateResourceId {
        id: String,
    },
    DuplicateCapabilityId {
        id: String,
    },
    DuplicatePassName {
        name: String,
    },
    DuplicatePipelineId {
        id: String,
    },
    UnknownPipelineReference {
        pass_name: String,
        pipeline_id: String,
    },
    UnknownCapabilityReference {
        pipeline_id: String,
        capability_id: String,
    },
    UnknownResourceReference {
        pipeline_id: String,
        resource_id: String,
    },
    UnknownPassDependency {
        pass_name: String,
        dependency: String,
    },
    DependencyCycle {
        cycle: Vec<String>,
    },
    UndeclaredReadResource {
        pass_name: String,
        resource_id: String,
    },
    UndeclaredWriteResource {
        pass_name: String,
        resource_id: String,
    },
    RawHazard {
        resource_id: String,
        writer_pass: String,
        reader_pass: String,
    },
    WarHazard {
        resource_id: String,
        reader_pass: String,
        writer_pass: String,
    },
    WawHazard {
        resource_id: String,
        first_writer_pass: String,
        second_writer_pass: String,
    },
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::DuplicateResourceId { id } => {
                write!(f, "duplicate resource id: {id}")
            }
            ValidationError::DuplicateCapabilityId { id } => {
                write!(f, "duplicate capability id: {id}")
            }
            ValidationError::DuplicatePassName { name } => {
                write!(f, "duplicate pass name: {name}")
            }
            ValidationError::DuplicatePipelineId { id } => {
                write!(f, "duplicate pipeline id: {id}")
            }
            ValidationError::UnknownPipelineReference {
                pass_name,
                pipeline_id,
            } => {
                write!(
                    f,
                    "pass '{pass_name}' references missing pipeline '{pipeline_id}'"
                )
            }
            ValidationError::UnknownCapabilityReference {
                pipeline_id,
                capability_id,
            } => {
                write!(
                    f,
                    "pipeline '{pipeline_id}' references missing capability '{capability_id}'"
                )
            }
            ValidationError::UnknownResourceReference {
                pipeline_id,
                resource_id,
            } => {
                write!(
                    f,
                    "pipeline '{pipeline_id}' references missing resource '{resource_id}'"
                )
            }
            ValidationError::UnknownPassDependency {
                pass_name,
                dependency,
            } => {
                write!(
                    f,
                    "pass '{pass_name}' depends on missing pass '{dependency}'"
                )
            }
            ValidationError::DependencyCycle { cycle } => {
                write!(f, "pass dependency cycle detected: {}", cycle.join(" -> "))
            }
            ValidationError::UndeclaredReadResource {
                pass_name,
                resource_id,
            } => {
                write!(
                    f,
                    "pass '{pass_name}' reads undeclared resource '{resource_id}'"
                )
            }
            ValidationError::UndeclaredWriteResource {
                pass_name,
                resource_id,
            } => {
                write!(
                    f,
                    "pass '{pass_name}' writes undeclared resource '{resource_id}'"
                )
            }
            ValidationError::RawHazard {
                resource_id,
                writer_pass,
                reader_pass,
            } => {
                write!(
                    f,
                    "RAW hazard on resource '{resource_id}' between writer '{writer_pass}' and reader '{reader_pass}'"
                )
            }
            ValidationError::WarHazard {
                resource_id,
                reader_pass,
                writer_pass,
            } => {
                write!(
                    f,
                    "WAR hazard on resource '{resource_id}' between reader '{reader_pass}' and writer '{writer_pass}'"
                )
            }
            ValidationError::WawHazard {
                resource_id,
                first_writer_pass,
                second_writer_pass,
            } => {
                write!(
                    f,
                    "WAW hazard on resource '{resource_id}' between writers '{first_writer_pass}' and '{second_writer_pass}'"
                )
            }
        }
    }
}

impl Error for ValidationError {}

pub fn validate_render_graph_contract_v6(
    graph: &RenderGraphContractV5,
) -> Result<(), ValidationError> {
    let mut resource_ids = HashSet::<&str>::new();
    for resource in &graph.resources {
        if !resource_ids.insert(resource.id.as_str()) {
            return Err(ValidationError::DuplicateResourceId {
                id: resource.id.clone(),
            });
        }
    }

    let mut capability_ids = HashSet::<&str>::new();
    for capability in &graph.capabilities {
        if !capability_ids.insert(capability.id.as_str()) {
            return Err(ValidationError::DuplicateCapabilityId {
                id: capability.id.clone(),
            });
        }
    }

    let mut pipeline_ids = HashSet::<&str>::new();
    for pipeline in &graph.pipelines {
        if !pipeline_ids.insert(pipeline.id.as_str()) {
            return Err(ValidationError::DuplicatePipelineId {
                id: pipeline.id.clone(),
            });
        }
        if !capability_ids.contains(pipeline.capability_id.as_str()) {
            return Err(ValidationError::UnknownCapabilityReference {
                pipeline_id: pipeline.id.clone(),
                capability_id: pipeline.capability_id.clone(),
            });
        }
        for resource_id in &pipeline.resources {
            if !resource_ids.contains(resource_id.as_str()) {
                return Err(ValidationError::UnknownResourceReference {
                    pipeline_id: pipeline.id.clone(),
                    resource_id: resource_id.clone(),
                });
            }
        }
    }

    for capability in &graph.capabilities {
        if !pipeline_ids.contains(capability.pipeline_id.as_str()) {
            return Err(ValidationError::UnknownPipelineReference {
                pass_name: format!("capability:{}", capability.id),
                pipeline_id: capability.pipeline_id.clone(),
            });
        }
    }

    let mut pass_names = HashSet::<&str>::new();
    for pass in &graph.passes {
        if !pass_names.insert(pass.name.as_str()) {
            return Err(ValidationError::DuplicatePassName {
                name: pass.name.clone(),
            });
        }
    }

    for pass in &graph.passes {
        if !pipeline_ids.contains(pass.pipeline_id.as_str()) {
            return Err(ValidationError::UnknownPipelineReference {
                pass_name: pass.name.clone(),
                pipeline_id: pass.pipeline_id.clone(),
            });
        }
        for dependency in &pass.depends_on {
            if !pass_names.contains(dependency.as_str()) {
                return Err(ValidationError::UnknownPassDependency {
                    pass_name: pass.name.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        for resource_id in &pass.reads {
            if !resource_ids.contains(resource_id.as_str()) {
                return Err(ValidationError::UndeclaredReadResource {
                    pass_name: pass.name.clone(),
                    resource_id: resource_id.clone(),
                });
            }
        }
        for resource_id in &pass.writes {
            if !resource_ids.contains(resource_id.as_str()) {
                return Err(ValidationError::UndeclaredWriteResource {
                    pass_name: pass.name.clone(),
                    resource_id: resource_id.clone(),
                });
            }
        }
    }

    if let Some(cycle) = detect_cycle(graph) {
        return Err(ValidationError::DependencyCycle { cycle });
    }

    detect_basic_hazards(graph)
}

pub fn fingerprint_render_graph_contract_v6(
    graph: &RenderGraphContractV5,
) -> Result<String, ValidationError> {
    validate_render_graph_contract_v6(graph)?;

    let bytes = canonical_json_bytes(&normalized_graph(graph));
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    Ok(format!("{digest:x}"))
}

pub fn validate_render_graph_ir_v1(graph: &RenderGraphContractV5) -> Result<(), ValidationError> {
    validate_render_graph_contract_v6(graph)
}

pub fn fingerprint_render_graph_ir_v1(
    graph: &RenderGraphContractV5,
) -> Result<String, ValidationError> {
    fingerprint_render_graph_contract_v6(graph)
}

fn detect_basic_hazards(graph: &RenderGraphContractV5) -> Result<(), ValidationError> {
    if graph.passes.len() < 2 {
        return Ok(());
    }

    let pass_indices = graph
        .passes
        .iter()
        .enumerate()
        .map(|(idx, pass)| (pass.name.as_str(), idx))
        .collect::<HashMap<_, _>>();

    let mut dependency_closure = vec![vec![false; graph.passes.len()]; graph.passes.len()];
    for (pass_index, pass) in graph.passes.iter().enumerate() {
        let mut stack = pass
            .depends_on
            .iter()
            .filter_map(|name| pass_indices.get(name.as_str()).copied())
            .collect::<Vec<_>>();
        let mut seen = HashSet::<usize>::new();
        while let Some(dep_index) = stack.pop() {
            if !seen.insert(dep_index) {
                continue;
            }
            dependency_closure[pass_index][dep_index] = true;
            for transitive in &graph.passes[dep_index].depends_on {
                if let Some(next) = pass_indices.get(transitive.as_str()).copied() {
                    stack.push(next);
                }
            }
        }
    }

    for lhs_index in 0..graph.passes.len() {
        for rhs_index in (lhs_index + 1)..graph.passes.len() {
            let lhs = &graph.passes[lhs_index];
            let rhs = &graph.passes[rhs_index];
            let ordered = dependency_closure[lhs_index][rhs_index]
                || dependency_closure[rhs_index][lhs_index];
            if ordered {
                continue;
            }

            if let Some(resource_id) = first_intersection(&lhs.writes, &rhs.reads) {
                return Err(ValidationError::RawHazard {
                    resource_id,
                    writer_pass: lhs.name.clone(),
                    reader_pass: rhs.name.clone(),
                });
            }
            if let Some(resource_id) = first_intersection(&lhs.reads, &rhs.writes) {
                return Err(ValidationError::WarHazard {
                    resource_id,
                    reader_pass: lhs.name.clone(),
                    writer_pass: rhs.name.clone(),
                });
            }
            if let Some(resource_id) = first_intersection(&lhs.writes, &rhs.writes) {
                return Err(ValidationError::WawHazard {
                    resource_id,
                    first_writer_pass: lhs.name.clone(),
                    second_writer_pass: rhs.name.clone(),
                });
            }
        }
    }

    Ok(())
}

fn first_intersection(lhs: &[String], rhs: &[String]) -> Option<String> {
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }

    let rhs_set = rhs
        .iter()
        .map(|value| value.as_str())
        .collect::<HashSet<_>>();
    let mut shared = lhs
        .iter()
        .map(|value| value.as_str())
        .filter(|value| rhs_set.contains(*value))
        .map(str::to_string)
        .collect::<Vec<_>>();
    shared.sort();
    shared.into_iter().next()
}

fn detect_cycle(graph: &RenderGraphContractV5) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    fn dfs(
        current: &str,
        pass_by_name: &HashMap<String, &RenderPassContractV5>,
        states: &mut HashMap<String, VisitState>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if let Some(state) = states.get(current) {
            match state {
                VisitState::Visiting => {
                    if let Some(start) = stack.iter().position(|name| name == current) {
                        let mut cycle = stack[start..].to_vec();
                        cycle.push(current.to_string());
                        return Some(cycle);
                    }
                    return Some(vec![current.to_string(), current.to_string()]);
                }
                VisitState::Visited => return None,
            }
        }

        states.insert(current.to_string(), VisitState::Visiting);
        stack.push(current.to_string());

        let pass = pass_by_name
            .get(current)
            .expect("all dependencies were validated before cycle detection");
        for dependency in &pass.depends_on {
            if let Some(cycle) = dfs(dependency, pass_by_name, states, stack) {
                return Some(cycle);
            }
        }

        stack.pop();
        states.insert(current.to_string(), VisitState::Visited);
        None
    }

    let mut pass_by_name: HashMap<String, &RenderPassContractV5> = HashMap::new();
    for pass in &graph.passes {
        pass_by_name.insert(pass.name.clone(), pass);
    }

    let mut states: HashMap<String, VisitState> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();

    for pass in &graph.passes {
        if let Some(cycle) = dfs(pass.name.as_str(), &pass_by_name, &mut states, &mut stack) {
            return Some(cycle);
        }
    }

    None
}

fn normalized_graph(graph: &RenderGraphContractV5) -> RenderGraphContractV5 {
    let mut normalized = graph.clone();

    normalized.resources.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then(lhs.group.cmp(&rhs.group))
            .then(lhs.binding.cmp(&rhs.binding))
            .then(lhs.name.cmp(&rhs.name))
            .then(lhs.kind.cmp(&rhs.kind))
    });

    normalized.capabilities.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then(lhs.name.cmp(&rhs.name))
            .then(lhs.target.cmp(&rhs.target))
            .then(lhs.preset.cmp(&rhs.preset))
            .then(lhs.profile.cmp(&rhs.profile))
            .then(lhs.shader_mode.cmp(&rhs.shader_mode))
            .then(lhs.shader_ref.cmp(&rhs.shader_ref))
            .then(lhs.shader_module.cmp(&rhs.shader_module))
            .then(lhs.pipeline_id.cmp(&rhs.pipeline_id))
    });
    for capability in &mut normalized.capabilities {
        capability.override_tiers.sort();
    }

    normalized.pipelines.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then(lhs.label.cmp(&rhs.label))
            .then(lhs.shader_module.cmp(&rhs.shader_module))
            .then(lhs.vertex_entry.cmp(&rhs.vertex_entry))
            .then(lhs.fragment_entry.cmp(&rhs.fragment_entry))
            .then(lhs.capability_id.cmp(&rhs.capability_id))
    });
    for pipeline in &mut normalized.pipelines {
        pipeline.targets.sort();
        pipeline.targets.dedup();
        pipeline.resources.sort();
        pipeline.resources.dedup();
    }

    normalized.passes.sort_by(|lhs, rhs| {
        lhs.name
            .cmp(&rhs.name)
            .then(lhs.pipeline_id.cmp(&rhs.pipeline_id))
            .then(lhs.draw_phase.cmp(&rhs.draw_phase))
    });
    for pass in &mut normalized.passes {
        pass.depends_on.sort();
        pass.depends_on.dedup();
        pass.reads.sort();
        pass.reads.dedup();
        pass.writes.sort();
        pass.writes.dedup();
    }

    normalized
}

fn canonical_json_bytes(graph: &RenderGraphContractV5) -> Vec<u8> {
    let mut value =
        serde_json::to_value(graph).expect("RenderGraphContractV5 is always serializable");
    canonicalize_json(&mut value);
    serde_json::to_vec(&value).expect("canonical JSON serialization must succeed")
}

fn canonicalize_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, child) in &mut entries {
                canonicalize_json(child);
            }
            for (key, child) in entries {
                map.insert(key, child);
            }
        }
        Value::Array(items) => {
            for item in items {
                canonicalize_json(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ValidationError, fingerprint_render_graph_contract_v6, validate_render_graph_contract_v6,
    };
    use crate::render_ir::types::{
        RenderCapabilityContractV5, RenderGraphContractV5, RenderPassContractV5,
        RenderPipelineContractV5, RenderPipelineCullModeV5, RenderPipelineTargetV5,
        RenderPipelineTopologyV5, RenderResourceContractV5,
    };

    fn valid_graph() -> RenderGraphContractV5 {
        RenderGraphContractV5 {
            resources: vec![
                RenderResourceContractV5 {
                    id: "scene_globals".to_string(),
                    group: 0,
                    binding: 0,
                    name: "scene_globals".to_string(),
                    kind: "uniform-buffer".to_string(),
                },
                RenderResourceContractV5 {
                    id: "geometry_target".to_string(),
                    group: 9,
                    binding: 0,
                    name: "geometry_target".to_string(),
                    kind: "color-target".to_string(),
                },
            ],
            capabilities: vec![RenderCapabilityContractV5 {
                id: "geometry_capability".to_string(),
                name: "geometry".to_string(),
                target: "RenderNode".to_string(),
                preset: "strict-2d".to_string(),
                profile: "balanced".to_string(),
                shader_mode: "gpu".to_string(),
                shader_ref: Some("geometry_shader".to_string()),
                override_tiers: vec![0, 1, 2],
                shader_module: "geometry_shader".to_string(),
                pipeline_id: "geometry_pipeline".to_string(),
            }],
            pipelines: vec![RenderPipelineContractV5 {
                id: "geometry_pipeline".to_string(),
                label: "geometry".to_string(),
                shader_module: "geometry_shader".to_string(),
                vertex_entry: "vs_main".to_string(),
                fragment_entry: "fs_main".to_string(),
                topology: RenderPipelineTopologyV5::Triangles,
                cull_mode: RenderPipelineCullModeV5::None,
                targets: vec![RenderPipelineTargetV5::SurfaceColor],
                capability_id: "geometry_capability".to_string(),
                resources: vec!["scene_globals".to_string(), "geometry_target".to_string()],
            }],
            passes: vec![
                RenderPassContractV5 {
                    name: "gbuffer".to_string(),
                    pipeline_id: "geometry_pipeline".to_string(),
                    draw_phase: "opaque".to_string(),
                    depends_on: vec![],
                    reads: vec!["scene_globals".to_string()],
                    writes: vec!["geometry_target".to_string()],
                },
                RenderPassContractV5 {
                    name: "lighting".to_string(),
                    pipeline_id: "geometry_pipeline".to_string(),
                    draw_phase: "post".to_string(),
                    depends_on: vec!["gbuffer".to_string()],
                    reads: vec!["scene_globals".to_string(), "geometry_target".to_string()],
                    writes: vec!["geometry_target".to_string()],
                },
            ],
        }
    }

    #[test]
    fn validates_unique_pass_names() {
        let mut graph = valid_graph();
        graph.passes[1].name = "gbuffer".to_string();

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::DuplicatePassName {
                name: "gbuffer".to_string()
            })
        );
    }

    #[test]
    fn validates_unique_pipeline_ids() {
        let mut graph = valid_graph();
        graph.pipelines.push(graph.pipelines[0].clone());

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::DuplicatePipelineId {
                id: "geometry_pipeline".to_string()
            })
        );
    }

    #[test]
    fn validates_pass_pipeline_references() {
        let mut graph = valid_graph();
        graph.passes[1].pipeline_id = "missing_pipeline".to_string();

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::UnknownPipelineReference {
                pass_name: "lighting".to_string(),
                pipeline_id: "missing_pipeline".to_string(),
            })
        );
    }

    #[test]
    fn validates_pass_dependency_names_exist() {
        let mut graph = valid_graph();
        graph.passes[1].depends_on = vec!["not_a_real_pass".to_string()];

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::UnknownPassDependency {
                pass_name: "lighting".to_string(),
                dependency: "not_a_real_pass".to_string(),
            })
        );
    }

    #[test]
    fn validates_dependency_cycle_detection() {
        let mut graph = valid_graph();
        graph.passes[0].depends_on = vec!["lighting".to_string()];

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::DependencyCycle {
                cycle: vec![
                    "gbuffer".to_string(),
                    "lighting".to_string(),
                    "gbuffer".to_string(),
                ],
            })
        );
    }

    #[test]
    fn validates_undeclared_read_resources() {
        let mut graph = valid_graph();
        graph.passes[1].reads.push("missing_resource".to_string());

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::UndeclaredReadResource {
                pass_name: "lighting".to_string(),
                resource_id: "missing_resource".to_string(),
            })
        );
    }

    #[test]
    fn validates_undeclared_write_resources() {
        let mut graph = valid_graph();
        graph.passes[1].writes.push("missing_resource".to_string());

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::UndeclaredWriteResource {
                pass_name: "lighting".to_string(),
                resource_id: "missing_resource".to_string(),
            })
        );
    }

    #[test]
    fn detects_raw_hazards_for_unordered_passes() {
        let mut graph = valid_graph();
        graph.passes[1].depends_on.clear();

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::RawHazard {
                resource_id: "geometry_target".to_string(),
                writer_pass: "gbuffer".to_string(),
                reader_pass: "lighting".to_string(),
            })
        );
    }

    #[test]
    fn detects_war_hazards_for_unordered_passes() {
        let mut graph = valid_graph();
        graph.passes[0].writes.clear();
        graph.passes[0].reads.push("geometry_target".to_string());
        graph.passes[1].depends_on.clear();

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::WarHazard {
                resource_id: "geometry_target".to_string(),
                reader_pass: "gbuffer".to_string(),
                writer_pass: "lighting".to_string(),
            })
        );
    }

    #[test]
    fn detects_waw_hazards_for_unordered_passes() {
        let mut graph = valid_graph();
        graph.passes[0].reads.clear();
        graph.passes[1].reads.clear();
        graph.passes[1].depends_on.clear();

        assert_eq!(
            validate_render_graph_contract_v6(&graph),
            Err(ValidationError::WawHazard {
                resource_id: "geometry_target".to_string(),
                first_writer_pass: "gbuffer".to_string(),
                second_writer_pass: "lighting".to_string(),
            })
        );
    }

    #[test]
    fn fingerprint_is_deterministic_across_orderings() {
        let graph = valid_graph();
        let mut reordered = graph.clone();
        reordered.resources.reverse();
        reordered.passes.reverse();
        reordered.capabilities.reverse();
        reordered.pipelines.reverse();

        let first = fingerprint_render_graph_contract_v6(&graph).expect("graph should validate");
        let second =
            fingerprint_render_graph_contract_v6(&reordered).expect("graph should validate");

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn canonical_json_uses_backend_neutral_contract_tokens() {
        let graph = valid_graph();
        let payload = serde_json::to_string(&graph).expect("serialize graph");
        for leaked_token in ["triangle-list", "bgra8unorm", "wgpu::"] {
            assert!(
                !payload.contains(leaked_token),
                "contract leaked adapter token '{leaked_token}'"
            );
        }
        assert!(payload.contains("\"topology\":\"triangles\""));
        assert!(payload.contains("\"targets\":[\"surface-color\"]"));
    }
}
