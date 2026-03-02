use crate::hir::project::ProjectProvenance;
use crate::hir::{
    AttributeAnnotation, BinaryOp, Body, Expr, Function, GpuFunctionSurface, Literal,
    MaterialDeclarationSurface, Module, RenderContract, RenderOverrideTierLevel,
    RenderShaderModeKind, RenderShaderModeSurface, Stmt, SurfaceDeclarationKind,
};
use crate::render_ir::types::{
    RenderPipelineCullModeV5, RenderPipelineTargetV5, RenderPipelineTopologyV5,
};
use crate::shader_compiler::lower::{
    apply_material_texture_policy_to_report_v1, build_material_variants_v1,
    compute_material_compile_report_v1, lower_material_to_ir_v1,
    validate_material_compile_report_v1,
};
use crate::shader_compiler::types::{
    MATERIAL_COMPILE_BUDGET_GATES_V1, MaterialCompileReport, MaterialIrV1, MaterialQualityTierV1,
};
use crate::shader_compiler::wgsl_codegen::generate_material_wgsl_v1;
use rowan::TextRange;
use serde::Serialize;
use serde_json::json;
use smol_str::SmolStr;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap};
use std::path::PathBuf;

const RENDER_SCHEMA_VERSION_V6: &str = "render-schema-v6";
const SHADER_BUNDLE_SCHEMA_VERSION_V6: &str = "shader-bundle-v6";
const RENDER_PROVENANCE_SCHEMA_VERSION_V2: &str = "render-provenance-v2";
const SHADER_PROVENANCE_SCHEMA_VERSION_V2: &str = "shader-provenance-v2";
const EXPANSION_TRACE_SCHEMA_VERSION_V1: &str = "render-expansion-trace-v1";
const MAX_MATERIAL_VARIANTS_PER_MATERIAL: usize = 32;
const MAX_MATERIAL_VARIANTS_PER_BUILD: usize = 2048;
const RAW_SHADER_UNSAFE_BUDGET_TAG: &str = "unsafe_raw_shader";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AnnotationProvenance {
    pub source_path: String,
    pub line: usize,
    pub column: usize,
    pub directive: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderScenePacketLayoutIr {
    pub world_width: u32,
    pub world_height: u32,
    pub max_instances: u32,
    pub instance_stride_f32: u32,
    pub fields: Vec<String>,
    pub pass_profile: String,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderBindGroupBindingIr {
    pub binding: u32,
    pub kind: String,
    pub name: String,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderBindGroupIr {
    pub id: String,
    pub bindings: Vec<RenderBindGroupBindingIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderPlanContractIr {
    pub id: String,
    pub name: String,
    pub resources: String,
    pub temporal: String,
    pub quality_tier: String,
    pub budget_tags: Vec<String>,
    pub target: String,
    pub preset: String,
    pub profile: String,
    pub shader_mode: String,
    pub shader_ref: Option<String>,
    pub override_tiers: Vec<u8>,
    pub shader_module: String,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderPipelineIr {
    pub id: String,
    pub label: String,
    pub shader_module: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub topology: RenderPipelineTopologyV5,
    pub cull_mode: RenderPipelineCullModeV5,
    pub targets: Vec<RenderPipelineTargetV5>,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderPassIr {
    pub name: String,
    pub pipeline: String,
    pub draw_phase: String,
    pub depends_on: Vec<String>,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderIr {
    pub render_plan: Vec<RenderPlanContractIr>,
    pub scene_packet_layout: RenderScenePacketLayoutIr,
    pub bind_groups: Vec<RenderBindGroupIr>,
    pub pipelines: Vec<RenderPipelineIr>,
    pub frame_graph: Vec<RenderPassIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ShaderModuleIr {
    pub id: String,
    pub source: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub material_compile_report: Option<MaterialCompileReport>,
    pub provenance: AnnotationProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ShaderIr {
    pub modules: Vec<ShaderModuleIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderShaderExtractionProvenance {
    pub source_files: Vec<String>,
    pub expansion_trace: Vec<AnnotationProvenance>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderShaderIr {
    pub render: RenderIr,
    pub shader: ShaderIr,
    pub provenance: RenderShaderExtractionProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderShaderIrError {
    #[error("source extraction error at {path}:{line}:{column}: {message}")]
    Source {
        path: String,
        line: usize,
        column: usize,
        message: String,
    },
    #[error("render/shader declaration validation failed: {message}")]
    Validation { message: String },
}

#[derive(Debug, Clone)]
pub struct RenderManifestContext {
    pub render_backend: String,
    pub app_mode: String,
    pub collectible_capacity: usize,
    pub entry_path: String,
    pub domain_source_hash: String,
}

#[derive(Debug, Clone)]
pub struct ShaderBundleManifestContext {
    pub render_manifest_path: String,
    pub entry_path: String,
    pub domain_source_hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedShaderModuleManifestEntry {
    pub id: String,
    pub path: String,
    pub entrypoints: Vec<String>,
    pub checksum: u32,
    pub source_path: String,
    pub provenance: AnnotationProvenance,
}

pub fn extract_render_shader_ir(
    module: &Module,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
) -> Result<RenderShaderIr, RenderShaderIrError> {
    reject_legacy_render_annotations(module, module_sources, project_provenance)?;

    let mut contracts = module
        .render_contracts
        .iter()
        .filter(|contract| matches!(contract.kind, SurfaceDeclarationKind::Render))
        .cloned()
        .collect::<Vec<_>>();
    if contracts.is_empty() {
        return Err(RenderShaderIrError::Validation {
            message: "missing required `render <Name> { ... }` declarations".to_string(),
        });
    }

    let mut expansion_trace = Vec::new();
    let material_declarations = module
        .material_declarations
        .iter()
        .map(|material| (material.name.to_string(), material.clone()))
        .collect::<HashMap<_, _>>();
    let mut shader_modules = extract_gpu_shader_modules(
        module,
        module_sources,
        project_provenance,
        &mut expansion_trace,
    )?;
    let mut shader_module_ids = shader_modules
        .iter()
        .map(|module| module.id.clone())
        .collect::<BTreeSet<_>>();

    contracts.sort_by(|lhs, rhs| {
        let lhs_key = render_contract_sort_key(lhs, project_provenance, module_sources);
        let rhs_key = render_contract_sort_key(rhs, project_provenance, module_sources);
        lhs_key.cmp(&rhs_key)
    });

    let mut render_plan = Vec::new();
    let mut pipelines = Vec::new();
    let mut frame_graph = Vec::new();
    let mut pipeline_ids = BTreeSet::new();
    let mut contract_ids = BTreeSet::new();
    let mut contract_names = BTreeSet::new();
    let mut total_material_variants = 0usize;

    for contract in contracts {
        if !contract_names.insert(contract.name.to_string()) {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "duplicate render contract name '{}' encountered while extracting render plan",
                    contract.name
                ),
            });
        }

        let provenance = render_contract_provenance(&contract, module_sources, project_provenance)?;
        expansion_trace.push(provenance.clone());

        let contract_id = normalize_identifier(contract.name.as_str());
        if !contract_ids.insert(contract_id.clone()) {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "render contract '{}' normalizes to duplicate id '{}'; rename contracts to unique identifiers",
                    contract.name, contract_id
                ),
            });
        }

        let (resources, temporal, quality_tier, budget_tags) =
            extract_render_v6_contract_clauses(&contract, &provenance)?;
        let target = resources.clone();
        let preset = quality_tier.clone();
        let profile = budget_tags.join("+");
        let shader_binding = resolve_shader_binding_for_contract(
            &contract,
            &contract_id,
            resources.as_str(),
            temporal.as_str(),
            quality_tier.as_str(),
            budget_tags.as_slice(),
            &material_declarations,
            &mut total_material_variants,
            &mut shader_modules,
            &mut shader_module_ids,
            &provenance,
            &mut expansion_trace,
        )?;
        let override_tiers = render_override_tiers(&contract);

        let pipeline_id = format!("{}_pipeline", contract_id);
        if !pipeline_ids.insert(pipeline_id.clone()) {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "render contract '{}' produced duplicate pipeline id '{}'; choose unique render names",
                    contract.name, pipeline_id
                ),
            });
        }

        let pipeline = RenderPipelineIr {
            id: pipeline_id.clone(),
            label: contract.name.to_string(),
            shader_module: shader_binding.shader_module_id.clone(),
            vertex_entry: shader_binding.vertex_entry.clone(),
            fragment_entry: shader_binding.fragment_entry.clone(),
            topology: RenderPipelineTopologyV5::Triangles,
            cull_mode: RenderPipelineCullModeV5::None,
            targets: vec![RenderPipelineTargetV5::SurfaceColor],
            provenance: provenance.clone(),
        };

        let phases = parse_contract_to_phases(
            temporal.as_str(),
            quality_tier.as_str(),
            budget_tags.as_slice(),
        );
        let mut previous_pass = None::<String>;
        for phase in phases {
            let pass_name = format!("{}_{}", pipeline_id, phase);
            let depends_on = previous_pass.iter().cloned().collect::<Vec<_>>();
            let pass_provenance = derived_provenance(
                &provenance,
                "render.frame_graph",
                format!(
                    "phase={} pipeline={} resources={} temporal={} quality_tier={} budget_tags={}",
                    phase,
                    pipeline_id,
                    resources,
                    temporal,
                    quality_tier,
                    budget_tags.join(",")
                )
                .as_str(),
            );
            frame_graph.push(RenderPassIr {
                name: pass_name.clone(),
                pipeline: pipeline_id.clone(),
                draw_phase: phase,
                depends_on,
                provenance: pass_provenance,
            });
            previous_pass = Some(pass_name);
        }

        expansion_trace.push(derived_provenance(
            &provenance,
            "render.expand",
            format!(
                "contract={} shader={} mode={} pipeline={}",
                contract.name,
                shader_binding.shader_module_id,
                shader_binding.shader_mode,
                pipeline_id
            )
            .as_str(),
        ));

        render_plan.push(RenderPlanContractIr {
            id: contract_id,
            name: contract.name.to_string(),
            resources,
            temporal: temporal.clone(),
            quality_tier,
            budget_tags,
            target,
            preset,
            profile,
            shader_mode: temporal,
            shader_ref: shader_binding.shader_ref,
            override_tiers,
            shader_module: shader_binding.shader_module_id,
            provenance,
        });
        pipelines.push(pipeline);
    }

    pipelines.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });
    let frame_graph = topologically_order_render_passes(
        frame_graph.as_slice(),
        "render contract frame graph extraction",
    )?;
    render_plan.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });

    expansion_trace.sort_by(compare_provenance);
    let source_files = collect_source_files(&expansion_trace);
    let baseline =
        expansion_trace
            .first()
            .cloned()
            .ok_or_else(|| RenderShaderIrError::Validation {
                message: "missing source provenance for render plan expansion".to_string(),
            })?;
    let pass_profile = render_plan
        .first()
        .map(|contract| contract.quality_tier.clone())
        .unwrap_or_else(|| "balanced".to_string());

    Ok(RenderShaderIr {
        render: RenderIr {
            render_plan,
            scene_packet_layout: strict_scene_packet_layout(&baseline, pass_profile.as_str()),
            bind_groups: strict_bind_groups(&baseline),
            pipelines,
            frame_graph,
        },
        shader: ShaderIr {
            modules: shader_modules,
        },
        provenance: RenderShaderExtractionProvenance {
            source_files,
            expansion_trace,
        },
    })
}

pub fn emit_render_manifest(
    ir: &RenderShaderIr,
    shader_module_paths: &HashMap<String, String>,
    context: &RenderManifestContext,
) -> Result<serde_json::Value, RenderShaderIrError> {
    let mut render_plan = ir.render.render_plan.clone();
    render_plan.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });
    let pipeline_selectors = build_pipeline_selector_map(&render_plan);
    let module_selectors = build_module_selector_map(&render_plan);
    let contract_sections = build_render_contract_sections_v6(ir, &pipeline_selectors);
    let gpu_scene_buffers = build_gpu_scene_buffer_contracts_v6(ir);

    let pipelines = ir
        .render
        .pipelines
        .iter()
        .map(|pipeline| {
            let selector = pipeline_selectors.get(pipeline.id.as_str());
            let node_target = selector.map(|(target, _)| target.as_str());
            let shader_mode = selector.map(|(_, mode)| mode.as_str());
            json!({
                "id": pipeline.id,
                "label": pipeline.label,
                "shader_module": pipeline.shader_module,
                "vertex_entry": pipeline.vertex_entry,
                "fragment_entry": pipeline.fragment_entry,
                "primitive": {
                    "topology": pipeline.topology,
                    "cull_mode": pipeline.cull_mode,
                },
                "targets": pipeline
                    .targets
                    .iter()
                    .map(|target| json!({"surface": target}))
                    .collect::<Vec<_>>(),
                "node_target": node_target,
                "shader_mode": shader_mode,
                "provenance": pipeline.provenance,
            })
        })
        .collect::<Vec<_>>();

    let mut bind_groups = ir.render.bind_groups.clone();
    bind_groups.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    for group in &mut bind_groups {
        group
            .bindings
            .sort_by(|lhs, rhs| lhs.binding.cmp(&rhs.binding));
    }

    let bind_groups = bind_groups
        .iter()
        .map(|group| {
            json!({
                "id": group.id,
                "bindings": group
                    .bindings
                    .iter()
                    .map(|binding| {
                        json!({
                            "binding": binding.binding,
                            "kind": binding.kind,
                            "name": binding.name,
                            "provenance": binding.provenance,
                        })
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();

    let mut shader_modules = Vec::with_capacity(ir.shader.modules.len());
    let mut sorted_modules = ir.shader.modules.clone();
    sorted_modules.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });
    for module in &sorted_modules {
        let Some(emitted_path) = shader_module_paths.get(module.id.as_str()) else {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "render manifest missing resolved shader path for module '{}'",
                    module.id
                ),
            });
        };
        let selector = module_selectors.get(module.id.as_str());
        let node_target = selector.and_then(|(target, _)| target.as_deref());
        let shader_mode = selector.and_then(|(_, mode)| mode.as_deref());
        shader_modules.push(json!({
            "id": module.id,
            "path": emitted_path,
            "node_target": node_target,
            "shader_mode": shader_mode,
            "material_compile_report": module.material_compile_report,
            "provenance": module.provenance,
        }));
    }

    let frame_graph = topologically_order_render_passes(
        ir.render.frame_graph.as_slice(),
        "render manifest emission",
    )?;

    let frame_graph = frame_graph
        .iter()
        .map(|pass| {
            let selector = pipeline_selectors.get(pass.pipeline.as_str());
            let node_target = selector.map(|(target, _)| target.as_str());
            let shader_mode = selector.map(|(_, mode)| mode.as_str());
            json!({
                "name": pass.name,
                "pipeline": pass.pipeline,
                "draw_phase": pass.draw_phase,
                "depends_on": pass.depends_on,
                "node_target": node_target,
                "shader_mode": shader_mode,
                "provenance": pass.provenance,
            })
        })
        .collect::<Vec<_>>();

    let mut trace = ir.provenance.expansion_trace.clone();
    trace.sort_by(compare_provenance);

    Ok(json!({
        "schema_version": RENDER_SCHEMA_VERSION_V6,
        "render_backend": context.render_backend,
        "app_mode": context.app_mode,
        "contracts": contract_sections,
        "render_plan": render_plan
            .iter()
            .map(|contract| {
                json!({
                    "id": contract.id,
                    "name": contract.name,
                    "resources": contract.resources,
                    "temporal": contract.temporal,
                    "quality_tier": contract.quality_tier,
                    "budget_tags": contract.budget_tags,
                    "target": contract.target,
                    "preset": contract.preset,
                    "profile": contract.profile,
                    "shader_mode": contract.shader_mode,
                    "shader_ref": contract.shader_ref,
                    "override_tiers": contract.override_tiers,
                    "shader_module": contract.shader_module,
                    "provenance": contract.provenance,
                })
            })
            .collect::<Vec<_>>(),
        "pipelines": pipelines,
        "bind_groups": bind_groups,
        "gpu_scene_buffers": gpu_scene_buffers,
        "scene_packet_layout": {
            "world_width": ir.render.scene_packet_layout.world_width,
            "world_height": ir.render.scene_packet_layout.world_height,
            "max_instances": ir.render.scene_packet_layout.max_instances,
            "instance_stride_f32": ir.render.scene_packet_layout.instance_stride_f32,
            "collectible_capacity": context.collectible_capacity,
            "pass_profile": ir.render.scene_packet_layout.pass_profile,
            "fields": ir.render.scene_packet_layout.fields,
            "provenance": ir.render.scene_packet_layout.provenance,
        },
        "shader_modules": shader_modules,
        "frame_graph": frame_graph,
        "provenance": {
            "schema_version": RENDER_PROVENANCE_SCHEMA_VERSION_V2,
            "entry_path": context.entry_path,
            "domain_source_hash": context.domain_source_hash,
            "source_files": ir.provenance.source_files,
            "expansion_trace": {
                "schema_version": EXPANSION_TRACE_SCHEMA_VERSION_V1,
                "records": trace,
            },
        }
    }))
}

pub fn emit_shader_bundle_manifest(
    ir: &RenderShaderIr,
    resolved_modules: &[ResolvedShaderModuleManifestEntry],
    context: &ShaderBundleManifestContext,
) -> serde_json::Value {
    let module_selectors = build_module_selector_map(&ir.render.render_plan);
    let pipeline_selectors = build_pipeline_selector_map(&ir.render.render_plan);
    let contract_sections = build_render_contract_sections_v6(ir, &pipeline_selectors);
    let mut sorted_modules = resolved_modules.to_vec();
    sorted_modules.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then(lhs.path.cmp(&rhs.path))
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });

    let modules = sorted_modules
        .iter()
        .map(|module| {
            let mut entrypoints = module.entrypoints.clone();
            entrypoints.sort();
            entrypoints.dedup();
            let selector = module_selectors.get(module.id.as_str());
            let node_target = selector.and_then(|(target, _)| target.as_deref());
            let shader_mode = selector.and_then(|(_, mode)| mode.as_deref());
            json!({
                "id": module.id,
                "path": module.path,
                "node_target": node_target,
                "shader_mode": shader_mode,
                "entrypoints": entrypoints,
                "checksum": module.checksum,
                "source_path": module.source_path,
                "provenance": module.provenance,
            })
        })
        .collect::<Vec<_>>();
    let prewarm_group_modules = sorted_modules
        .iter()
        .map(|module| module.id.clone())
        .collect::<Vec<_>>();
    let prewarm_groups = if prewarm_group_modules.is_empty() {
        Vec::new()
    } else {
        vec![json!({
            "id": "bootstrap-required",
            "required": true,
            "shader_modules": prewarm_group_modules,
        })]
    };

    let mut trace = ir.provenance.expansion_trace.clone();
    trace.sort_by(compare_provenance);

    json!({
        "schema_version": SHADER_BUNDLE_SCHEMA_VERSION_V6,
        "render_manifest": context.render_manifest_path,
        "contracts": contract_sections,
        "shader_modules": modules,
        "prewarm_groups": prewarm_groups,
        "provenance": {
            "schema_version": SHADER_PROVENANCE_SCHEMA_VERSION_V2,
            "entry_path": context.entry_path,
            "domain_source_hash": context.domain_source_hash,
            "source_files": ir.provenance.source_files,
            "expansion_trace": {
                "schema_version": EXPANSION_TRACE_SCHEMA_VERSION_V1,
                "records": trace,
            },
        }
    })
}

fn binding_resource_id(group_id: &str, binding: u32) -> String {
    format!("{group_id}:{binding}")
}

fn target_surface_label(target: RenderPipelineTargetV5) -> &'static str {
    match target {
        RenderPipelineTargetV5::SurfaceColor => "surface-color",
    }
}

fn target_resource_id(pipeline_id: &str, slot: usize, target: RenderPipelineTargetV5) -> String {
    format!(
        "target:{}:{}:{}",
        pipeline_id,
        slot,
        normalize_identifier(target_surface_label(target))
    )
}

fn build_gpu_scene_buffer_contracts_v6(ir: &RenderShaderIr) -> serde_json::Value {
    let instance_stride_bytes = ir
        .render
        .scene_packet_layout
        .instance_stride_f32
        .saturating_mul(4)
        .max(16);
    json!({
        "transforms": {
            "resource_id": "scene_transforms",
            "kind": "storage-buffer",
            "stride_bytes": 48
        },
        "bounds": {
            "resource_id": "scene_bounds",
            "kind": "storage-buffer",
            "stride_bytes": 32
        },
        "draw_records": {
            "resource_id": "scene_draw_records",
            "kind": "storage-buffer",
            "stride_bytes": instance_stride_bytes
        },
        "material_refs": {
            "resource_id": "scene_material_refs",
            "kind": "storage-buffer",
            "stride_bytes": 16
        },
        "hiz_occlusion": {
            "enabled": false,
            "tier": serde_json::Value::Null
        }
    })
}

fn build_render_contract_sections_v6(
    ir: &RenderShaderIr,
    pipeline_selectors: &HashMap<String, (String, String)>,
) -> serde_json::Value {
    let mut bind_groups = ir.render.bind_groups.clone();
    bind_groups.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    for group in &mut bind_groups {
        group
            .bindings
            .sort_by(|lhs, rhs| lhs.binding.cmp(&rhs.binding));
    }

    let mut resources = Vec::new();
    let mut bind_group_resource_ids = Vec::new();
    for (group_index, group) in bind_groups.iter().enumerate() {
        for binding in &group.bindings {
            let resource_id = binding_resource_id(group.id.as_str(), binding.binding);
            bind_group_resource_ids.push(resource_id.clone());
            resources.push(json!({
                "id": resource_id,
                "group": group_index as u32,
                "binding": binding.binding,
                "name": binding.name,
                "kind": binding.kind,
                "external": true,
                "provenance": binding.provenance,
            }));
        }
    }
    bind_group_resource_ids.sort();
    bind_group_resource_ids.dedup();

    let mut sorted_pipelines = ir.render.pipelines.clone();
    sorted_pipelines.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    let mut pipeline_target_resources = HashMap::<String, Vec<String>>::new();
    for pipeline in &sorted_pipelines {
        let target_ids = pipeline
            .targets
            .iter()
            .enumerate()
            .map(|(slot, target)| {
                let surface_label = target_surface_label(*target);
                let resource_id = target_resource_id(pipeline.id.as_str(), slot, *target);
                resources.push(json!({
                    "id": resource_id,
                    "group": 1024 + slot as u32,
                    "binding": slot as u32,
                    "name": format!("{} target {}", pipeline.id, surface_label),
                    "kind": "color-target",
                    "external": false,
                }));
                resource_id
            })
            .collect::<Vec<_>>();
        pipeline_target_resources.insert(pipeline.id.clone(), target_ids);
    }
    resources.sort_by(|lhs, rhs| {
        lhs.get("id")
            .and_then(|value| value.as_str())
            .cmp(&rhs.get("id").and_then(|value| value.as_str()))
    });

    let mut sorted_contracts = ir.render.render_plan.clone();
    sorted_contracts.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    let mut capability_ids_by_pipeline = HashMap::<String, String>::new();
    let capabilities = sorted_contracts
        .iter()
        .map(|contract| {
            let capability_id = format!("{}_capability", contract.id);
            let pipeline_id = format!("{}_pipeline", contract.id);
            capability_ids_by_pipeline.insert(pipeline_id.clone(), capability_id.clone());
            json!({
                "id": capability_id,
                "name": contract.name,
                "resources": contract.resources,
                "temporal": contract.temporal,
                "quality_tier": contract.quality_tier,
                "budget_tags": contract.budget_tags,
                "target": contract.target,
                "preset": contract.preset,
                "profile": contract.profile,
                "shader_mode": contract.shader_mode,
                "shader_ref": contract.shader_ref,
                "override_tiers": contract.override_tiers,
                "shader_module": contract.shader_module,
                "pipeline_id": pipeline_id,
                "provenance": contract.provenance,
            })
        })
        .collect::<Vec<_>>();

    let pipelines = sorted_pipelines
        .iter()
        .map(|pipeline| {
            let mut resource_ids = bind_group_resource_ids.clone();
            if let Some(targets) = pipeline_target_resources.get(pipeline.id.as_str()) {
                resource_ids.extend(targets.iter().cloned());
            }
            resource_ids.sort();
            resource_ids.dedup();

            let selector = pipeline_selectors.get(pipeline.id.as_str());
            let node_target = selector.map(|(target, _)| target.as_str());
            let shader_mode = selector.map(|(_, mode)| mode.as_str());

            json!({
                "id": pipeline.id,
                "label": pipeline.label,
                "shader_module": pipeline.shader_module,
                "vertex_entry": pipeline.vertex_entry,
                "fragment_entry": pipeline.fragment_entry,
                "topology": pipeline.topology,
                "cull_mode": pipeline.cull_mode,
                "targets": pipeline.targets,
                "capability_id": capability_ids_by_pipeline.get(pipeline.id.as_str()),
                "resources": resource_ids,
                "node_target": node_target,
                "shader_mode": shader_mode,
                "provenance": pipeline.provenance,
            })
        })
        .collect::<Vec<_>>();

    let pass_write_resources = ir
        .render
        .frame_graph
        .iter()
        .map(|pass| {
            (
                pass.name.clone(),
                pipeline_target_resources
                    .get(pass.pipeline.as_str())
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut passes_sorted = ir.render.frame_graph.clone();
    passes_sorted.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    let passes = passes_sorted
        .iter()
        .map(|pass| {
            let mut reads = bind_group_resource_ids.clone();
            for dependency in &pass.depends_on {
                if let Some(outputs) = pass_write_resources.get(dependency) {
                    reads.extend(outputs.iter().cloned());
                }
            }
            reads.sort();
            reads.dedup();
            let writes = pass_write_resources
                .get(pass.name.as_str())
                .cloned()
                .unwrap_or_default();

            let selector = pipeline_selectors.get(pass.pipeline.as_str());
            let node_target = selector.map(|(target, _)| target.as_str());
            let shader_mode = selector.map(|(_, mode)| mode.as_str());

            json!({
                "name": pass.name,
                "pipeline_id": pass.pipeline,
                "draw_phase": pass.draw_phase,
                "depends_on": pass.depends_on,
                "reads": reads,
                "writes": writes,
                "node_target": node_target,
                "shader_mode": shader_mode,
                "provenance": pass.provenance,
            })
        })
        .collect::<Vec<_>>();
    let default_profile = build_default_profile_contracts_v6(ir);

    json!({
        "schema_version": "render-contracts-v6",
        "resources": resources,
        "capabilities": capabilities,
        "pipelines": pipelines,
        "passes": passes,
        "default_profile": default_profile,
    })
}

fn build_default_profile_contracts_v6(ir: &RenderShaderIr) -> serde_json::Value {
    let mut contracts = ir.render.render_plan.clone();
    contracts.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    let Some(contract) = contracts.first() else {
        return json!({
            "lighting": {
                "pbr": {"enabled": true},
                "hdr": {"enabled": true},
                "tonemap": {"operator": "aces"},
                "clustered_lighting": {
                    "enabled": true,
                    "max_lights_per_cluster": 64,
                    "shadow": {
                        "enabled": true,
                        "cascade_count": 4,
                        "atlas_resolution": 2048
                    }
                }
            },
            "reflections": {
                "fallback_chain": ["planar", "ssr", "probe"],
                "planar_budget": {"max_planes": 2, "resolution_scale": 1.0},
                "ssr_budget": {"max_steps": 32, "max_rays_per_pixel": 1},
                "probe_budget": {"max_active_probes": 32, "update_ratio": 0.25}
            },
            "temporal": {
                "motion_vectors": {"enabled": true},
                "taa": {"enabled": true, "history_frames": 12},
                "upscaling": {"enabled": true, "mode": "temporal"},
                "reactive_mask": {"enabled": true},
                "disocclusion_mask": {"enabled": true},
                "dynamic_resolution_policy": {
                    "enabled": true,
                    "min_scale": 0.6,
                    "max_scale": 1.0,
                    "target_frame_time_ms": 16.7,
                    "scale_step": 0.05
                },
                "metrics": {
                    "window_frames": 120,
                    "report_interval_ms": 1000,
                    "max_jitter_ms": 0.75
                }
            }
        });
    };

    let quality = normalize_identifier(contract.quality_tier.as_str());
    let high_quality = matches!(quality.as_str(), "ultra" | "high" | "quality");
    let balanced_quality = matches!(quality.as_str(), "balanced" | "medium");
    let min_scale = if high_quality {
        0.7
    } else if balanced_quality {
        0.6
    } else {
        0.5
    };
    let max_lights = if high_quality {
        96
    } else if balanced_quality {
        64
    } else {
        32
    };
    let shadow_resolution = if high_quality { 2048 } else { 1024 };
    let taa_history = if high_quality { 16 } else { 10 };
    let temporal_mode = normalize_identifier(contract.temporal.as_str());
    let taa_enabled = !matches!(temporal_mode.as_str(), "off" | "none" | "disabled");
    let upscaler_mode = if taa_enabled { "temporal" } else { "none" };

    json!({
        "source_contract_id": contract.id,
        "lighting": {
            "pbr": {"enabled": true},
            "hdr": {"enabled": true},
            "tonemap": {"operator": if high_quality { "aces" } else { "reinhard" }},
            "clustered_lighting": {
                "enabled": true,
                "max_lights_per_cluster": max_lights,
                "shadow": {
                    "enabled": true,
                    "cascade_count": if high_quality { 4 } else { 2 },
                    "atlas_resolution": shadow_resolution
                }
            }
        },
        "reflections": {
            "fallback_chain": ["planar", "ssr", "probe"],
            "planar_budget": {
                "max_planes": if high_quality { 4 } else { 2 },
                "resolution_scale": if high_quality { 1.0 } else { 0.75 }
            },
            "ssr_budget": {
                "max_steps": if high_quality { 48 } else { 24 },
                "max_rays_per_pixel": 1
            },
            "probe_budget": {
                "max_active_probes": if high_quality { 64 } else { 24 },
                "update_ratio": if high_quality { 0.35 } else { 0.2 }
            }
        },
        "temporal": {
            "motion_vectors": {"enabled": true},
            "taa": {"enabled": taa_enabled, "history_frames": taa_history},
            "upscaling": {"enabled": taa_enabled, "mode": upscaler_mode},
            "reactive_mask": {"enabled": true},
            "disocclusion_mask": {"enabled": true},
            "dynamic_resolution_policy": {
                "enabled": true,
                "min_scale": min_scale,
                "max_scale": 1.0,
                "target_frame_time_ms": if high_quality { 16.7 } else { 15.5 },
                "scale_step": 0.05
            },
            "metrics": {
                "window_frames": if high_quality { 180 } else { 120 },
                "report_interval_ms": 1000,
                "max_jitter_ms": if high_quality { 0.65 } else { 0.85 }
            }
        }
    })
}

fn reject_legacy_render_annotations(
    module: &Module,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
) -> Result<(), RenderShaderIrError> {
    let mut function_ids = module
        .functions
        .iter()
        .map(|(func_id, _)| func_id)
        .collect::<Vec<_>>();
    function_ids.sort_by(|lhs, rhs| {
        let lhs_key = function_sort_key(*lhs, module, project_provenance);
        let rhs_key = function_sort_key(*rhs, module, project_provenance);
        lhs_key.cmp(&rhs_key)
    });

    let mut offenders = Vec::new();
    for func_id in function_ids {
        let func = &module.functions[func_id];
        for attr in &func.attributes {
            if !matches!(attr.name.as_str(), "shader" | "pipeline" | "pass") {
                continue;
            }
            let provenance = attribute_provenance(
                func_id.into_raw(),
                func,
                attr,
                module_sources,
                project_provenance,
            )?;
            offenders.push(provenance);
        }
    }

    if offenders.is_empty() {
        return Ok(());
    }

    offenders.sort_by(compare_provenance);
    let preview = offenders
        .iter()
        .take(6)
        .map(|entry| {
            format!(
                "{} at {}:{}:{}",
                entry.directive, entry.source_path, entry.line, entry.column
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if offenders.len() > 6 {
        format!(", +{} more", offenders.len() - 6)
    } else {
        String::new()
    };

    Err(RenderShaderIrError::Validation {
        message: format!(
            "legacy annotation-based render source is unsupported; migrate to `render <Name> {{ resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }}` and `gpu fn`. Offenders: {preview}{suffix}"
        ),
    })
}

fn build_pipeline_selector_map(
    contracts: &[RenderPlanContractIr],
) -> HashMap<String, (String, String)> {
    contracts
        .iter()
        .map(|contract| {
            (
                format!("{}_pipeline", contract.id),
                (contract.target.clone(), contract.shader_mode.clone()),
            )
        })
        .collect::<HashMap<_, _>>()
}

fn build_module_selector_map(
    contracts: &[RenderPlanContractIr],
) -> HashMap<String, (Option<String>, Option<String>)> {
    let mut selectors = HashMap::<String, BTreeSet<(String, String)>>::new();
    for contract in contracts {
        selectors
            .entry(contract.shader_module.clone())
            .or_default()
            .insert((contract.target.clone(), contract.shader_mode.clone()));
    }

    selectors
        .into_iter()
        .map(|(module_id, variants)| {
            if variants.len() == 1 {
                let mut it = variants.into_iter();
                let (target, mode) = it.next().expect("selector set should contain one variant");
                (module_id, (Some(target), Some(mode)))
            } else {
                (module_id, (None, None))
            }
        })
        .collect::<HashMap<_, _>>()
}

fn extract_gpu_shader_modules(
    module: &Module,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
    expansion_trace: &mut Vec<AnnotationProvenance>,
) -> Result<Vec<ShaderModuleIr>, RenderShaderIrError> {
    let mut gpu_functions = module.gpu_functions.clone();
    gpu_functions.sort_by(|lhs, rhs| {
        let lhs_key = gpu_function_sort_key(lhs, project_provenance, module_sources);
        let rhs_key = gpu_function_sort_key(rhs, project_provenance, module_sources);
        lhs_key.cmp(&rhs_key)
    });

    let mut modules = Vec::new();
    let mut module_ids = BTreeSet::new();
    for gpu in &gpu_functions {
        let id = gpu.name.to_string();
        if !module_ids.insert(id.clone()) {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "duplicate gpu shader function name '{}' encountered while extracting shader modules",
                    id
                ),
            });
        }

        if gpu
            .ret_type
            .as_ref()
            .is_none_or(|ty| ty.name.as_str() != "String")
        {
            return Err(RenderShaderIrError::Validation {
                message: format!("gpu shader function '{}' must return String", gpu.name),
            });
        }

        let provenance = gpu_function_provenance(gpu, module_sources, project_provenance)?;
        expansion_trace.push(provenance.clone());

        let source = extract_shader_source_from_gpu_function(gpu, id.as_str())?;
        let (vertex_entry, fragment_entry) =
            infer_shader_entrypoints(source.as_str(), id.as_str())?;
        modules.push(ShaderModuleIr {
            id,
            source,
            vertex_entry,
            fragment_entry,
            material_compile_report: None,
            provenance,
        });
    }

    modules.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then_with(|| compare_provenance(&lhs.provenance, &rhs.provenance))
    });
    Ok(modules)
}

#[derive(Debug, Clone)]
struct ResolvedShaderBinding {
    shader_module_id: String,
    vertex_entry: String,
    fragment_entry: String,
    shader_mode: String,
    shader_ref: Option<String>,
}

#[derive(Debug, Clone)]
enum SelectedShaderMode {
    Generated,
    Material { material: String },
    Gpu { function: String },
}

fn resolve_shader_binding_for_contract(
    contract: &RenderContract,
    contract_id: &str,
    resources: &str,
    temporal: &str,
    quality_tier: &str,
    budget_tags: &[String],
    material_declarations: &HashMap<String, MaterialDeclarationSurface>,
    total_material_variants: &mut usize,
    shader_modules: &mut Vec<ShaderModuleIr>,
    shader_module_ids: &mut BTreeSet<String>,
    provenance: &AnnotationProvenance,
    expansion_trace: &mut Vec<AnnotationProvenance>,
) -> Result<ResolvedShaderBinding, RenderShaderIrError> {
    let selected_mode = select_shader_mode_for_contract(contract, provenance)?;
    match selected_mode {
        SelectedShaderMode::Generated => {
            let shader_module_id = format!("{}_generated_shader", contract_id);
            if !shader_module_ids.insert(shader_module_id.clone()) {
                return Err(RenderShaderIrError::Validation {
                    message: format!(
                        "render contract '{}' generated shader module id '{}' collides with an existing module; rename the render contract [{}:{}:{}]",
                        contract.name,
                        shader_module_id,
                        provenance.source_path,
                        provenance.line,
                        provenance.column,
                    ),
                });
            }
            let generated_provenance = derived_provenance(
                provenance,
                "shader generated",
                format!(
                    "contract={} resources={} temporal={} quality_tier={} budget_tags={}",
                    contract.name,
                    resources,
                    temporal,
                    quality_tier,
                    budget_tags.join(",")
                )
                .as_str(),
            );
            expansion_trace.push(generated_provenance.clone());
            shader_modules.push(ShaderModuleIr {
                id: shader_module_id.clone(),
                source: generated_shader_source(
                    contract.name.as_str(),
                    resources,
                    temporal,
                    quality_tier,
                    budget_tags,
                ),
                vertex_entry: "vs_main".to_string(),
                fragment_entry: "fs_main".to_string(),
                material_compile_report: None,
                provenance: generated_provenance,
            });
            Ok(ResolvedShaderBinding {
                shader_module_id,
                vertex_entry: "vs_main".to_string(),
                fragment_entry: "fs_main".to_string(),
                shader_mode: "generated".to_string(),
                shader_ref: None,
            })
        }
        SelectedShaderMode::Material { material } => {
            let Some(material_declaration) = material_declarations.get(material.as_str()) else {
                let mut available = material_declarations
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>();
                available.sort();
                return Err(RenderShaderIrError::Validation {
                    message: format!(
                        "render contract '{}' references unknown in-source material declaration '{}' in `shader material`; available material declarations: {} [{}:{}:{}]. Declare the material in `.wr`.",
                        contract.name,
                        material,
                        if available.is_empty() {
                            "<none>".to_string()
                        } else {
                            available.join(", ")
                        },
                        provenance.source_path,
                        provenance.line,
                        provenance.column,
                    ),
                });
            };
            let material_id = normalize_identifier(material.as_str());
            let shader_module_id = format!(
                "{}_material_{}_shader",
                contract_id,
                if material_id.is_empty() {
                    "unnamed".to_string()
                } else {
                    material_id
                }
            );
            if !shader_module_ids.insert(shader_module_id.clone()) {
                return Err(RenderShaderIrError::Validation {
                    message: format!(
                        "render contract '{}' material shader module id '{}' collides with an existing module; rename the render contract or material reference [{}:{}:{}]",
                        contract.name,
                        shader_module_id,
                        provenance.source_path,
                        provenance.line,
                        provenance.column,
                    ),
                });
            }
            let material_provenance = derived_provenance(
                provenance,
                "shader material",
                format!(
                    "contract={} material={} resources={} temporal={} quality_tier={} budget_tags={}",
                    contract.name,
                    material,
                    resources,
                    temporal,
                    quality_tier,
                    budget_tags.join(",")
                )
                .as_str(),
            );
            expansion_trace.push(material_provenance.clone());
            let compiled_material = compile_material_shader(
                contract.name.as_str(),
                material_declaration,
                quality_tier,
            )?;
            validate_variant_cardinality(
                compiled_material.report.variant_count,
                MAX_MATERIAL_VARIANTS_PER_MATERIAL,
                format!(
                    "render contract '{}' material '{}'",
                    contract.name, material
                )
                .as_str(),
            )?;
            *total_material_variants += compiled_material.report.variant_count;
            validate_variant_cardinality(
                *total_material_variants,
                MAX_MATERIAL_VARIANTS_PER_BUILD,
                "render build total material variants",
            )?;
            shader_modules.push(ShaderModuleIr {
                id: shader_module_id.clone(),
                source: material_shader_source(
                    contract.name.as_str(),
                    material.as_str(),
                    resources,
                    temporal,
                    quality_tier,
                    budget_tags,
                    compiled_material.wgsl_source.as_str(),
                ),
                vertex_entry: "vs_main".to_string(),
                fragment_entry: "fs_main".to_string(),
                material_compile_report: Some(compiled_material.report),
                provenance: material_provenance,
            });
            Ok(ResolvedShaderBinding {
                shader_module_id,
                vertex_entry: "vs_main".to_string(),
                fragment_entry: "fs_main".to_string(),
                shader_mode: "material".to_string(),
                shader_ref: Some(material),
            })
        }
        SelectedShaderMode::Gpu { function } => {
            enforce_unsafe_raw_shader_opt_in(contract, budget_tags, provenance)?;
            let module = shader_modules
                .iter()
                .find(|module| module.id == function)
                .ok_or_else(|| {
                    let available = shader_modules
                        .iter()
                        .map(|module| module.id.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    RenderShaderIrError::Validation {
                        message: format!(
                            "render contract '{}' references unknown gpu shader function '{}' in `shader gpu`; available gpu functions: {} [{}:{}:{}]",
                            contract.name,
                            function,
                            if available.is_empty() {
                                "<none>".to_string()
                            } else {
                                available
                            },
                            provenance.source_path,
                            provenance.line,
                            provenance.column,
                        ),
                    }
                })?;
            lint_raw_gpu_shader_source(contract, function.as_str(), module, provenance)?;
            expansion_trace.push(derived_provenance(
                provenance,
                "shader gpu",
                format!("contract={} function={}", contract.name, function).as_str(),
            ));
            Ok(ResolvedShaderBinding {
                shader_module_id: module.id.clone(),
                vertex_entry: module.vertex_entry.clone(),
                fragment_entry: module.fragment_entry.clone(),
                shader_mode: "gpu".to_string(),
                shader_ref: Some(function),
            })
        }
    }
}

fn has_budget_tag(budget_tags: &[String], required_tag: &str) -> bool {
    let required = normalize_identifier(required_tag);
    budget_tags
        .iter()
        .map(|tag| normalize_identifier(tag.as_str()))
        .any(|tag| tag == required)
}

fn enforce_unsafe_raw_shader_opt_in(
    contract: &RenderContract,
    budget_tags: &[String],
    provenance: &AnnotationProvenance,
) -> Result<(), RenderShaderIrError> {
    if has_budget_tag(budget_tags, RAW_SHADER_UNSAFE_BUDGET_TAG) {
        return Ok(());
    }
    Err(RenderShaderIrError::Validation {
        message: format!(
            "render contract '{}' uses unsafe raw shader mode (`shader gpu ...`) without explicit opt-in budget tag `{}`; add `budget tags ..., {}` [{}:{}:{}]",
            contract.name,
            RAW_SHADER_UNSAFE_BUDGET_TAG,
            RAW_SHADER_UNSAFE_BUDGET_TAG,
            provenance.source_path,
            provenance.line,
            provenance.column
        ),
    })
}

fn lint_raw_gpu_shader_source(
    contract: &RenderContract,
    function: &str,
    module: &ShaderModuleIr,
    provenance: &AnnotationProvenance,
) -> Result<(), RenderShaderIrError> {
    let source = module.source.as_str();
    if !source.contains("@vertex") || !source.contains("@fragment") {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "raw shader lint failed for render contract '{}' (`shader gpu {}`): source must contain both `@vertex` and `@fragment` stage markers [{}:{}:{}]",
                contract.name, function, provenance.source_path, provenance.line, provenance.column
            ),
        });
    }
    if source.contains("@compute") {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "raw shader lint failed for render contract '{}' (`shader gpu {}`): raw surface shader path must not declare `@compute` entry points [{}:{}:{}]",
                contract.name, function, provenance.source_path, provenance.line, provenance.column
            ),
        });
    }
    Ok(())
}

fn select_shader_mode_for_contract(
    contract: &RenderContract,
    provenance: &AnnotationProvenance,
) -> Result<SelectedShaderMode, RenderShaderIrError> {
    let has_duplicate_modes = !contract.shader_modes.duplicate_modes.is_empty();
    let mut modes = Vec::<SelectedShaderMode>::new();
    let mut mode_labels = Vec::<String>::new();

    if contract.shader_modes.generated.is_some() {
        modes.push(SelectedShaderMode::Generated);
        mode_labels.push("generated".to_string());
    }
    if let Some(material) = contract.shader_modes.material.as_ref() {
        modes.push(SelectedShaderMode::Material {
            material: material.symbol.to_string(),
        });
        mode_labels.push(format!("material {}", material.symbol));
    }
    if let Some(gpu) = contract.shader_modes.gpu.as_ref() {
        modes.push(SelectedShaderMode::Gpu {
            function: gpu.symbol.to_string(),
        });
        mode_labels.push(format!("gpu {}", gpu.symbol));
    }
    for duplicate in &contract.shader_modes.duplicate_modes {
        mode_labels.push(render_shader_mode_surface_label(duplicate));
        match duplicate.kind {
            RenderShaderModeKind::Generated => modes.push(SelectedShaderMode::Generated),
            RenderShaderModeKind::Material => modes.push(SelectedShaderMode::Material {
                material: duplicate
                    .symbol
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            }),
            RenderShaderModeKind::Gpu => modes.push(SelectedShaderMode::Gpu {
                function: duplicate
                    .symbol
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            }),
        }
    }

    if modes.is_empty() && !has_duplicate_modes {
        return Ok(SelectedShaderMode::Generated);
    }
    if modes.len() > 1 || has_duplicate_modes {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' must declare exactly one shader mode (found: {}) [{}:{}:{}]",
                contract.name,
                mode_labels.join(", "),
                provenance.source_path,
                provenance.line,
                provenance.column,
            ),
        });
    }
    Ok(modes.remove(0))
}

fn render_shader_mode_surface_label(mode: &RenderShaderModeSurface) -> String {
    match mode.kind {
        RenderShaderModeKind::Generated => "generated".to_string(),
        RenderShaderModeKind::Material => format!(
            "material {}",
            mode.symbol.as_deref().unwrap_or("<missing-material>")
        ),
        RenderShaderModeKind::Gpu => {
            format!(
                "gpu {}",
                mode.symbol.as_deref().unwrap_or("<missing-gpu-fn>")
            )
        }
    }
}

fn generated_shader_source(
    contract: &str,
    resources: &str,
    temporal: &str,
    quality_tier: &str,
    budget_tags: &[String],
) -> String {
    use std::fmt::Write as _;

    let mut source = String::new();
    let _ = writeln!(
        source,
        "// generated shader for render contract: {contract}"
    );
    let _ = writeln!(source, "// resources: {resources}");
    let _ = writeln!(source, "// temporal: {temporal}");
    let _ = writeln!(source, "// quality_tier: {quality_tier}");
    let _ = writeln!(source, "// budget_tags: {}", budget_tags.join(","));
    source.push('\n');
    source.push_str("struct VsOut {\n");
    source.push_str("    @builtin(position) position: vec4<f32>,\n");
    source.push_str("};\n\n");
    source.push_str("@vertex\n");
    source.push_str("fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {\n");
    source.push_str("    var out: VsOut;\n");
    source.push_str("    var x = f32((vertex_index << 1u) & 2u);\n");
    source.push_str("    var y = f32(vertex_index & 2u);\n");
    source.push_str("    out.position = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);\n");
    source.push_str("    return out;\n");
    source.push_str("}\n\n");
    source.push_str("@fragment\n");
    source.push_str("fn fs_main() -> @location(0) vec4<f32> {\n");
    source.push_str("    return vec4<f32>(0.15, 0.35, 0.65, 1.0);\n");
    source.push_str("}\n");
    source
}

fn material_shader_source(
    contract: &str,
    material: &str,
    resources: &str,
    temporal: &str,
    quality_tier: &str,
    budget_tags: &[String],
    material_wgsl_body: &str,
) -> String {
    use std::fmt::Write as _;

    let mut source = String::new();
    let _ = writeln!(source, "// material shader for render contract: {contract}");
    let _ = writeln!(source, "// material ref: {material}");
    let _ = writeln!(source, "// resources: {resources}");
    let _ = writeln!(source, "// temporal: {temporal}");
    let _ = writeln!(source, "// quality_tier: {quality_tier}");
    let _ = writeln!(source, "// budget_tags: {}", budget_tags.join(","));
    source.push('\n');
    source.push_str(material_wgsl_body);
    source
}

#[derive(Debug, Clone)]
struct CompiledMaterialShader {
    wgsl_source: String,
    report: MaterialCompileReport,
}

fn quality_tier_to_material_tier(quality_tier: &str) -> MaterialQualityTierV1 {
    match normalize_identifier(normalize_render_clause_value(quality_tier).as_str()).as_str() {
        "ultra" | "high" | "quality" | "hero" => MaterialQualityTierV1::Hero,
        "medium" | "balanced" | "gameplay" => MaterialQualityTierV1::Gameplay,
        _ => MaterialQualityTierV1::Low,
    }
}

fn compile_material_shader(
    contract_name: &str,
    material: &MaterialDeclarationSurface,
    quality_tier: &str,
) -> Result<CompiledMaterialShader, RenderShaderIrError> {
    let material_ir: MaterialIrV1 = lower_material_to_ir_v1(material);
    let variants = build_material_variants_v1(&material_ir, MAX_MATERIAL_VARIANTS_PER_MATERIAL)
        .map_err(|error| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' material '{}' variant generation failed: {}",
                contract_name, material.name, error
            ),
        })?;
    let mut report = compute_material_compile_report_v1(&material_ir, variants.as_slice());
    apply_material_texture_policy_to_report_v1(material, &material_ir, &mut report).map_err(
        |error| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' material '{}' failed texture policy checks: {}",
                contract_name, material.name, error
            ),
        },
    )?;
    validate_material_compile_report_v1(&report, &MATERIAL_COMPILE_BUDGET_GATES_V1).map_err(
        |error| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' material '{}' failed compile budget gates ({:?}): {}",
                contract_name, material.name, MATERIAL_COMPILE_BUDGET_GATES_V1, error
            ),
        },
    )?;
    let active_tier = quality_tier_to_material_tier(quality_tier);
    let wgsl_source =
        generate_material_wgsl_v1(&material_ir, active_tier, variants.as_slice(), &report);
    Ok(CompiledMaterialShader {
        wgsl_source,
        report,
    })
}

fn validate_variant_cardinality(
    observed: usize,
    max_allowed: usize,
    scope: &str,
) -> Result<(), RenderShaderIrError> {
    if observed > max_allowed {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "shader variant cardinality exceeded for {scope}: observed {observed}, max {max_allowed}. Reduce material feature usage or raise explicit limits."
            ),
        });
    }
    Ok(())
}

fn render_override_tiers(contract: &RenderContract) -> Vec<u8> {
    let mut tiers = Vec::new();
    if matches!(
        contract.overrides.tier0.as_ref().map(|tier| tier.level),
        Some(RenderOverrideTierLevel::Tier0)
    ) {
        tiers.push(0);
    }
    if matches!(
        contract.overrides.tier1.as_ref().map(|tier| tier.level),
        Some(RenderOverrideTierLevel::Tier1)
    ) {
        tiers.push(1);
    }
    if matches!(
        contract.overrides.tier2.as_ref().map(|tier| tier.level),
        Some(RenderOverrideTierLevel::Tier2)
    ) {
        tiers.push(2);
    }
    tiers
}

fn extract_render_v6_contract_clauses(
    contract: &RenderContract,
    provenance: &AnnotationProvenance,
) -> Result<(String, String, String, Vec<String>), RenderShaderIrError> {
    let resources = contract
        .resources
        .as_ref()
        .map(|surface| normalize_render_clause_value(surface.value.as_str()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' is missing required `resources <AssetsDeclaration>` clause [{}:{}:{}]",
                contract.name, provenance.source_path, provenance.line, provenance.column
            ),
        })?;
    let temporal = contract
        .temporal
        .as_ref()
        .map(|surface| normalize_render_clause_value(surface.value.as_str()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' is missing required `temporal <mode>` clause [{}:{}:{}]",
                contract.name, provenance.source_path, provenance.line, provenance.column
            ),
        })?;
    let quality_tier = contract
        .quality_tier
        .as_ref()
        .map(|surface| normalize_render_clause_value(surface.value.as_str()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' is missing required `quality tier <tier>` clause [{}:{}:{}]",
                contract.name, provenance.source_path, provenance.line, provenance.column
            ),
        })?;
    let budget_tags = contract
        .budget_tags
        .as_ref()
        .map(|surface| {
            surface
                .tags
                .iter()
                .map(|tag| normalize_render_clause_value(tag.value.as_str()))
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|tags| !tags.is_empty())
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "render contract '{}' is missing required `budget tags <tag>[, <tag>...]` clause [{}:{}:{}]",
                contract.name, provenance.source_path, provenance.line, provenance.column
            ),
        })?;
    Ok((resources, temporal, quality_tier, budget_tags))
}

fn parse_contract_to_phases(
    temporal: &str,
    quality_tier: &str,
    budget_tags: &[String],
) -> Vec<String> {
    let quality = normalize_identifier(normalize_render_clause_value(quality_tier).as_str());
    let mut phases = match quality.as_str() {
        "ultra" | "high" | "quality" => vec![
            "depth".to_string(),
            "opaque".to_string(),
            "transparent".to_string(),
            "ui".to_string(),
            "post".to_string(),
            "composite".to_string(),
        ],
        "medium" | "balanced" => vec![
            "depth".to_string(),
            "opaque".to_string(),
            "transparent".to_string(),
            "ui".to_string(),
            "post".to_string(),
        ],
        "low" | "performance" => vec!["opaque".to_string(), "ui".to_string()],
        "ui" => vec!["ui".to_string()],
        _ => vec!["opaque".to_string()],
    };

    let temporal_mode = normalize_identifier(normalize_render_clause_value(temporal).as_str());
    if !matches!(temporal_mode.as_str(), "off" | "none" | "disabled") {
        phases.push("temporal_resolve".to_string());
    }
    if budget_tags
        .iter()
        .map(|tag| normalize_identifier(tag.as_str()))
        .any(|tag| tag == "reflection" || tag == "reflections")
    {
        phases.push("reflection".to_string());
    }

    if phases.is_empty() {
        phases.push("opaque".to_string());
    }

    let mut seen = BTreeSet::new();
    phases
        .into_iter()
        .filter(|phase| seen.insert(phase.clone()))
        .collect::<Vec<_>>()
}

fn extract_shader_source_from_gpu_function(
    gpu: &GpuFunctionSurface,
    shader_id: &str,
) -> Result<String, RenderShaderIrError> {
    let Some(body) = &gpu.body else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' must define a function body that returns WGSL source",
                shader_id
            ),
        });
    };

    if body.root_stmts.len() != 1 {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' must contain exactly one top-level return statement",
                shader_id
            ),
        });
    }

    let stmt = body.root_stmts[0];
    let Stmt::Return(Some(expr_id)) = &body.stmts[stmt] else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' must return WGSL source from its function body",
                shader_id
            ),
        });
    };

    match eval_compile_time_string(body, *expr_id, shader_id) {
        Ok(source) => Ok(source),
        Err(_) => compile_gpu_surface_shader_from_expr(body, *expr_id, shader_id),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuExprShape {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
}

fn merge_gpu_expr_shape(lhs: GpuExprShape, rhs: GpuExprShape) -> GpuExprShape {
    use GpuExprShape::*;
    match (lhs, rhs) {
        (Vec4, _) | (_, Vec4) => Vec4,
        (Vec3, _) | (_, Vec3) => Vec3,
        (Vec2, _) | (_, Vec2) => Vec2,
        _ => Scalar,
    }
}

fn compile_gpu_surface_shader_from_expr(
    body: &Body,
    expr_id: crate::hir::Idx<Expr>,
    shader_id: &str,
) -> Result<String, RenderShaderIrError> {
    let expression = lower_gpu_expr_to_wgsl(body, expr_id, shader_id)?;
    let shape = infer_gpu_expr_shape(body, expr_id);
    let color_line = match shape {
        GpuExprShape::Vec4 => "  let color = surface_value;\n".to_string(),
        GpuExprShape::Vec3 => "  let color = vec4<f32>(surface_value, 1.0);\n".to_string(),
        GpuExprShape::Vec2 => {
            "  let color = vec4<f32>(surface_value.x, surface_value.y, 0.0, 1.0);\n".to_string()
        }
        GpuExprShape::Scalar => {
            "  let color = vec4<f32>(f32(surface_value), f32(surface_value), f32(surface_value), 1.0);\n"
                .to_string()
        }
    };
    let mut source = String::new();
    source.push_str("// generated from gpu fn expression\n");
    source.push_str(&format!("// function: {shader_id}\n\n"));
    source.push_str("struct VsOut {\n");
    source.push_str("  @builtin(position) position: vec4<f32>,\n");
    source.push_str("};\n\n");
    source.push_str("@vertex\n");
    source.push_str("fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {\n");
    source.push_str("  var out: VsOut;\n");
    source.push_str("  let x = f32((vertex_index << 1u) & 2u);\n");
    source.push_str("  let y = f32(vertex_index & 2u);\n");
    source.push_str("  out.position = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);\n");
    source.push_str("  return out;\n");
    source.push_str("}\n\n");
    source.push_str("@fragment\n");
    source.push_str("fn fs_main() -> @location(0) vec4<f32> {\n");
    source.push_str(&format!("  let surface_value = {};\n", expression));
    source.push_str(color_line.as_str());
    source.push_str("  return clamp(color, vec4<f32>(0.0), vec4<f32>(1.0));\n");
    source.push_str("}\n");
    Ok(source)
}

fn eval_compile_time_string(
    body: &Body,
    expr_id: crate::hir::Idx<Expr>,
    shader_id: &str,
) -> Result<String, RenderShaderIrError> {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(value)) => Ok(value.to_string()),
        Expr::Binary { lhs, op, rhs, .. } if matches!(op, BinaryOp::Add) => {
            let lhs = eval_compile_time_string(body, *lhs, shader_id)?;
            let rhs = eval_compile_time_string(body, *rhs, shader_id)?;
            Ok(lhs + rhs.as_str())
        }
        Expr::StringInterp(parts) => {
            let mut out = String::new();
            for part in parts {
                match part {
                    crate::hir::StringPart::Literal(value) => out.push_str(value.as_str()),
                    crate::hir::StringPart::Expr(inner) => {
                        out.push_str(eval_compile_time_string(body, *inner, shader_id)?.as_str());
                    }
                }
            }
            Ok(out)
        }
        _ => Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' return expression must be a compile-time string literal (or literal concatenation)",
                shader_id
            ),
        }),
    }
}

fn infer_gpu_expr_shape(body: &Body, expr_id: crate::hir::Idx<Expr>) -> GpuExprShape {
    match &body.exprs[expr_id] {
        Expr::Literal(_) => GpuExprShape::Scalar,
        Expr::Variable(_) => GpuExprShape::Scalar,
        Expr::Binary { lhs, rhs, .. } => merge_gpu_expr_shape(
            infer_gpu_expr_shape(body, *lhs),
            infer_gpu_expr_shape(body, *rhs),
        ),
        Expr::Unary { expr, .. } => infer_gpu_expr_shape(body, *expr),
        Expr::Call { callee, .. } => {
            if let Some(name) = resolve_gpu_callee_name(body, *callee) {
                let normalized = normalize_identifier(name.as_str());
                if normalized == "vec4" {
                    return GpuExprShape::Vec4;
                }
                if normalized == "vec3" {
                    return GpuExprShape::Vec3;
                }
                if normalized == "vec2" {
                    return GpuExprShape::Vec2;
                }
            }
            GpuExprShape::Scalar
        }
        Expr::Member { member, .. } => {
            let len = member.len();
            if len >= 4 {
                GpuExprShape::Vec4
            } else if len == 3 {
                GpuExprShape::Vec3
            } else if len == 2 {
                GpuExprShape::Vec2
            } else {
                GpuExprShape::Scalar
            }
        }
        Expr::Index { .. } => GpuExprShape::Scalar,
        Expr::TypeApply { callee, .. } => infer_gpu_expr_shape(body, *callee),
        _ => GpuExprShape::Scalar,
    }
}

fn lower_gpu_expr_to_wgsl(
    body: &Body,
    expr_id: crate::hir::Idx<Expr>,
    shader_id: &str,
) -> Result<String, RenderShaderIrError> {
    match &body.exprs[expr_id] {
        Expr::Literal(literal) => lower_gpu_literal_to_wgsl(literal, shader_id),
        Expr::Variable(name) => Ok(name.to_string()),
        Expr::Binary { lhs, op, rhs, .. } => {
            let lhs = lower_gpu_expr_to_wgsl(body, *lhs, shader_id)?;
            let rhs = lower_gpu_expr_to_wgsl(body, *rhs, shader_id)?;
            let operator = wgsl_binary_operator(*op).ok_or_else(|| RenderShaderIrError::Validation {
                message: format!(
                    "gpu shader function '{}' expression lowering does not support binary operator {:?}",
                    shader_id, op
                ),
            })?;
            Ok(format!("({lhs} {operator} {rhs})"))
        }
        Expr::Unary { op, expr, .. } => {
            let value = lower_gpu_expr_to_wgsl(body, *expr, shader_id)?;
            let operator = match op {
                crate::hir::UnaryOp::Neg => "-",
                crate::hir::UnaryOp::Not => "!",
                _ => {
                    return Err(RenderShaderIrError::Validation {
                        message: format!(
                            "gpu shader function '{}' expression lowering does not support unary operator {:?}",
                            shader_id, op
                        ),
                    });
                }
            };
            Ok(format!("({operator}{value})"))
        }
        Expr::Call {
            callee,
            args,
            type_args: _,
        } => {
            let name = resolve_gpu_callee_name(body, *callee).ok_or_else(|| {
                RenderShaderIrError::Validation {
                    message: format!(
                        "gpu shader function '{}' expression lowering requires call callee to be a simple identifier or member path",
                        shader_id
                    ),
                }
            })?;
            let lowered_args = args
                .iter()
                .map(|arg| match arg {
                    crate::hir::Arg::Positional { value, .. } => {
                        lower_gpu_expr_to_wgsl(body, *value, shader_id)
                    }
                    crate::hir::Arg::Named { value, .. } => {
                        lower_gpu_expr_to_wgsl(body, *value, shader_id)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{}({})", name, lowered_args.join(", ")))
        }
        Expr::Member { object, member, .. } => {
            let object = lower_gpu_expr_to_wgsl(body, *object, shader_id)?;
            Ok(format!("{object}.{member}"))
        }
        Expr::Index { object, index, .. } => {
            let object = lower_gpu_expr_to_wgsl(body, *object, shader_id)?;
            let index = lower_gpu_expr_to_wgsl(body, *index, shader_id)?;
            Ok(format!("{object}[{index}]"))
        }
        Expr::TypeApply { callee, .. } => lower_gpu_expr_to_wgsl(body, *callee, shader_id),
        _ => Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' return expression must be a compile-time string literal or a WGSL-compatible expression tree",
                shader_id
            ),
        }),
    }
}

fn lower_gpu_literal_to_wgsl(
    literal: &Literal,
    shader_id: &str,
) -> Result<String, RenderShaderIrError> {
    match literal {
        Literal::Integer(value) => Ok(value.to_string()),
        Literal::Float(value) => Ok(format!("{value:?}")),
        Literal::Boolean(value) => Ok(value.to_string()),
        Literal::String(value) => Ok(format!("{value:?}")),
        Literal::Nil => Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' cannot lower `nothing`/`nil` in WGSL expression mode",
                shader_id
            ),
        }),
    }
}

fn resolve_gpu_callee_name(body: &Body, expr_id: crate::hir::Idx<Expr>) -> Option<String> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) => Some(name.to_string()),
        Expr::Member { object, member, .. } => {
            resolve_gpu_callee_name(body, *object).map(|prefix| format!("{prefix}.{member}"))
        }
        Expr::TypeApply { callee, .. } => resolve_gpu_callee_name(body, *callee),
        _ => None,
    }
}

fn wgsl_binary_operator(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("+"),
        BinaryOp::Sub => Some("-"),
        BinaryOp::Mul => Some("*"),
        BinaryOp::Div => Some("/"),
        BinaryOp::Mod => Some("%"),
        BinaryOp::Eq => Some("=="),
        BinaryOp::Ne => Some("!="),
        BinaryOp::Lt => Some("<"),
        BinaryOp::Gt => Some(">"),
        BinaryOp::Le => Some("<="),
        BinaryOp::Ge => Some(">="),
        BinaryOp::And => Some("&&"),
        BinaryOp::Or => Some("||"),
        BinaryOp::BitAnd => Some("&"),
        BinaryOp::BitOr => Some("|"),
        BinaryOp::BitXor => Some("^"),
        BinaryOp::Shl => Some("<<"),
        BinaryOp::Shr => Some(">>"),
        _ => None,
    }
}

fn infer_shader_entrypoints(
    source: &str,
    shader_id: &str,
) -> Result<(String, String), RenderShaderIrError> {
    let Some(vertex_entry) = infer_stage_entrypoint(source, "vertex") else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' WGSL source is missing a @vertex fn entrypoint",
                shader_id
            ),
        });
    };
    let Some(fragment_entry) = infer_stage_entrypoint(source, "fragment") else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "gpu shader function '{}' WGSL source is missing a @fragment fn entrypoint",
                shader_id
            ),
        });
    };
    Ok((vertex_entry, fragment_entry))
}

fn infer_stage_entrypoint(source: &str, stage: &str) -> Option<String> {
    let marker = format!("@{stage}");
    let lines = source.lines().collect::<Vec<_>>();
    for (index, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if !line.contains(marker.as_str()) {
            continue;
        }

        if let Some(name) = parse_fn_name(line) {
            return Some(name);
        }

        for lookahead in (index + 1)..lines.len().min(index + 4) {
            let next = lines[lookahead].trim();
            if next.starts_with('@') && !next.contains("fn") {
                break;
            }
            if let Some(name) = parse_fn_name(next) {
                return Some(name);
            }
        }
    }
    None
}

fn parse_fn_name(line: &str) -> Option<String> {
    let mut index = 0usize;
    let bytes = line.as_bytes();
    while index + 2 <= bytes.len() {
        let slice = &line[index..];
        let Some(offset) = slice.find("fn") else {
            return None;
        };
        let fn_index = index + offset;
        let before_ok = fn_index == 0
            || !line
                .as_bytes()
                .get(fn_index - 1)
                .is_some_and(u8::is_ascii_alphanumeric);
        let after = fn_index + 2;
        if !before_ok {
            index = after;
            continue;
        }

        let mut cursor = after;
        while let Some(ch) = line.as_bytes().get(cursor) {
            if ch.is_ascii_whitespace() {
                cursor += 1;
            } else {
                break;
            }
        }

        let start = cursor;
        if !line
            .as_bytes()
            .get(start)
            .is_some_and(|ch| ch.is_ascii_alphabetic() || *ch == b'_')
        {
            index = after;
            continue;
        }

        cursor += 1;
        while let Some(ch) = line.as_bytes().get(cursor) {
            if ch.is_ascii_alphanumeric() || *ch == b'_' {
                cursor += 1;
            } else {
                break;
            }
        }
        return Some(line[start..cursor].to_string());
    }
    None
}

fn topologically_order_render_passes(
    frame_graph: &[RenderPassIr],
    context: &str,
) -> Result<Vec<RenderPassIr>, RenderShaderIrError> {
    if frame_graph.is_empty() {
        return Ok(Vec::new());
    }

    let mut pass_indices = HashMap::<String, usize>::new();
    for (index, pass) in frame_graph.iter().enumerate() {
        if pass.name.trim().is_empty() {
            return Err(RenderShaderIrError::Validation {
                message: format!("{context}: render pass at index {index} has empty name"),
            });
        }
        if pass_indices.insert(pass.name.clone(), index).is_some() {
            return Err(RenderShaderIrError::Validation {
                message: format!(
                    "{context}: duplicate render pass name '{}' in frame_graph",
                    pass.name
                ),
            });
        }
    }

    let mut indegree = vec![0usize; frame_graph.len()];
    let mut dependents = vec![Vec::<usize>::new(); frame_graph.len()];

    for (pass_index, pass) in frame_graph.iter().enumerate() {
        for dependency in &pass.depends_on {
            let Some(&dependency_index) = pass_indices.get(dependency) else {
                return Err(RenderShaderIrError::Validation {
                    message: format!(
                        "{context}: render pass '{}' depends on missing pass '{}'",
                        pass.name, dependency
                    ),
                });
            };
            indegree[pass_index] = indegree[pass_index].saturating_add(1);
            dependents[dependency_index].push(pass_index);
        }
    }

    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(Reverse(index)))
        .collect::<BinaryHeap<_>>();

    let mut ordered_indices = Vec::with_capacity(frame_graph.len());
    while let Some(Reverse(index)) = ready.pop() {
        ordered_indices.push(index);

        for &dependent in &dependents[index] {
            indegree[dependent] = indegree[dependent].saturating_sub(1);
            if indegree[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }

    if ordered_indices.len() != frame_graph.len() {
        let mut stuck = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree > 0).then_some(frame_graph[index].name.clone()))
            .collect::<Vec<_>>();
        stuck.sort();
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "{context}: frame_graph contains a dependency cycle (stuck passes: {})",
                stuck.join(", ")
            ),
        });
    }

    Ok(ordered_indices
        .into_iter()
        .map(|index| frame_graph[index].clone())
        .collect::<Vec<_>>())
}

fn strict_scene_packet_layout(
    base: &AnnotationProvenance,
    pass_profile: &str,
) -> RenderScenePacketLayoutIr {
    RenderScenePacketLayoutIr {
        world_width: 800,
        world_height: 600,
        max_instances: 256,
        instance_stride_f32: 8,
        fields: vec![
            "position_xy".to_string(),
            "half_size_xy".to_string(),
            "color_rgba".to_string(),
        ],
        pass_profile: pass_profile.to_string(),
        provenance: derived_provenance(base, "render.scene_packet_layout", "render defaults"),
    }
}

fn strict_bind_groups(base: &AnnotationProvenance) -> Vec<RenderBindGroupIr> {
    vec![RenderBindGroupIr {
        id: "scene".to_string(),
        bindings: vec![
            RenderBindGroupBindingIr {
                binding: 0,
                kind: "uniform-buffer".to_string(),
                name: "scene_globals".to_string(),
                provenance: derived_provenance(base, "render.bind_group", "render defaults"),
            },
            RenderBindGroupBindingIr {
                binding: 1,
                kind: "read-only-storage-buffer".to_string(),
                name: "instance_buffer".to_string(),
                provenance: derived_provenance(base, "render.bind_group", "render defaults"),
            },
        ],
    }]
}

fn derived_provenance(
    base: &AnnotationProvenance,
    directive: &str,
    raw: &str,
) -> AnnotationProvenance {
    AnnotationProvenance {
        source_path: base.source_path.clone(),
        line: base.line,
        column: base.column,
        directive: directive.to_string(),
        raw: raw.to_string(),
    }
}

fn normalize_render_clause_value(raw: &str) -> String {
    raw.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn normalize_identifier(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "unnamed".to_string()
    } else {
        out
    }
}

fn collect_source_files(expansion_trace: &[AnnotationProvenance]) -> Vec<String> {
    let mut source_files = expansion_trace
        .iter()
        .map(|entry| entry.source_path.clone())
        .collect::<Vec<_>>();
    source_files.sort();
    source_files.dedup();
    source_files
}

fn function_sort_key(
    func_id: crate::hir::Idx<Function>,
    module: &Module,
    project_provenance: &ProjectProvenance,
) -> (String, usize, SmolStr) {
    let function = &module.functions[func_id];
    let path = project_provenance
        .function_owner_path_by_id
        .get(&func_id.into_raw())
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let span_start = project_provenance
        .function_owner_span_by_id
        .get(&func_id.into_raw())
        .map(|span| u32::from(span.start()) as usize)
        .unwrap_or(0);
    (path, span_start, function.name.clone())
}

fn render_contract_sort_key(
    contract: &RenderContract,
    project_provenance: &ProjectProvenance,
    module_sources: &HashMap<PathBuf, String>,
) -> (String, usize, SmolStr) {
    let path = project_provenance
        .render_contract_owner_path_by_name
        .get(&contract.name)
        .map(|path| path.display().to_string())
        .or_else(|| {
            find_owner_path_by_symbol(module_sources, format!("render {}", contract.name).as_str())
        })
        .unwrap_or_default();
    let span_start = project_provenance
        .render_contract_span_by_name
        .get(&contract.name)
        .map(|span| u32::from(span.start()) as usize)
        .unwrap_or_else(|| u32::from(contract.span.start()) as usize);
    (path, span_start, contract.name.clone())
}

fn gpu_function_sort_key(
    gpu: &GpuFunctionSurface,
    project_provenance: &ProjectProvenance,
    module_sources: &HashMap<PathBuf, String>,
) -> (String, usize, SmolStr) {
    let path = project_provenance
        .gpu_function_owner_path_by_name
        .get(&gpu.name)
        .map(|path| path.display().to_string())
        .or_else(|| {
            find_owner_path_by_symbol(module_sources, format!("gpu fn {}", gpu.name).as_str())
        })
        .unwrap_or_default();
    let span_start = project_provenance
        .gpu_function_span_by_name
        .get(&gpu.name)
        .map(|span| u32::from(span.start()) as usize)
        .unwrap_or_else(|| u32::from(gpu.span.start()) as usize);
    (path, span_start, gpu.name.clone())
}

fn find_owner_path_by_symbol(
    module_sources: &HashMap<PathBuf, String>,
    pattern: &str,
) -> Option<String> {
    let mut matches = module_sources
        .iter()
        .filter(|(_, source)| source.contains(pattern))
        .map(|(path, _)| path.display().to_string())
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next()
}

fn attribute_provenance(
    function_id: usize,
    func: &Function,
    attr: &AttributeAnnotation,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
) -> Result<AnnotationProvenance, RenderShaderIrError> {
    let Some(owner_path) = project_provenance
        .function_owner_path_by_id
        .get(&function_id)
    else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "missing source provenance for function '{}' while checking legacy render annotations",
                func.name
            ),
        });
    };
    let Some(source) = module_sources.get(owner_path) else {
        return Err(RenderShaderIrError::Validation {
            message: format!(
                "missing source text for '{}' while checking legacy render annotations",
                owner_path.display()
            ),
        });
    };

    let position_span = attr.name_span.unwrap_or(attr.span);
    let byte_offset = u32::from(position_span.start()) as usize;
    let (line, column) = line_col_from_offset(source, byte_offset);
    let raw = source_slice(source, attr.span)
        .map(|slice| slice.trim().to_string())
        .unwrap_or_else(|| format!("@{}", attr.name));

    Ok(AnnotationProvenance {
        source_path: owner_path.display().to_string(),
        line,
        column,
        directive: format!("@{}", attr.name),
        raw,
    })
}

fn render_contract_provenance(
    contract: &RenderContract,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
) -> Result<AnnotationProvenance, RenderShaderIrError> {
    let owner_path = project_provenance
        .render_contract_owner_path_by_name
        .get(&contract.name)
        .cloned()
        .or_else(|| {
            find_owner_path_by_symbol(module_sources, format!("render {}", contract.name).as_str())
                .map(PathBuf::from)
        })
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "missing source provenance for render contract '{}'",
                contract.name
            ),
        })?;
    let source =
        module_sources
            .get(&owner_path)
            .ok_or_else(|| RenderShaderIrError::Validation {
                message: format!(
                    "missing source text for render contract '{}' ({})",
                    contract.name,
                    owner_path.display()
                ),
            })?;

    let span = project_provenance
        .render_contract_span_by_name
        .get(&contract.name)
        .copied()
        .unwrap_or(contract.span);

    let byte_offset = u32::from(span.start()) as usize;
    let (line, column) = line_col_from_offset(source, byte_offset);
    let raw = source_slice(source, span)
        .map(|slice| slice.trim().to_string())
        .unwrap_or_else(|| format!("render {}", contract.name));

    Ok(AnnotationProvenance {
        source_path: owner_path.display().to_string(),
        line,
        column,
        directive: "render".to_string(),
        raw,
    })
}

fn gpu_function_provenance(
    gpu: &GpuFunctionSurface,
    module_sources: &HashMap<PathBuf, String>,
    project_provenance: &ProjectProvenance,
) -> Result<AnnotationProvenance, RenderShaderIrError> {
    let owner_path = project_provenance
        .gpu_function_owner_path_by_name
        .get(&gpu.name)
        .cloned()
        .or_else(|| {
            find_owner_path_by_symbol(module_sources, format!("gpu fn {}", gpu.name).as_str())
                .map(PathBuf::from)
        })
        .ok_or_else(|| RenderShaderIrError::Validation {
            message: format!(
                "missing source provenance for gpu shader function '{}'",
                gpu.name
            ),
        })?;
    let source =
        module_sources
            .get(&owner_path)
            .ok_or_else(|| RenderShaderIrError::Validation {
                message: format!(
                    "missing source text for gpu shader function '{}' ({})",
                    gpu.name,
                    owner_path.display()
                ),
            })?;

    let span = project_provenance
        .gpu_function_span_by_name
        .get(&gpu.name)
        .copied()
        .unwrap_or(gpu.span);
    let byte_offset = u32::from(span.start()) as usize;
    let (line, column) = line_col_from_offset(source, byte_offset);
    let raw = source_slice(source, span)
        .map(|slice| slice.trim().to_string())
        .unwrap_or_else(|| format!("gpu fn {}", gpu.name));

    Ok(AnnotationProvenance {
        source_path: owner_path.display().to_string(),
        line,
        column,
        directive: "gpu fn".to_string(),
        raw,
    })
}

fn line_col_from_offset(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    let limit = byte_offset.min(source.len());
    for byte in source.as_bytes().iter().take(limit) {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn source_slice(source: &str, span: TextRange) -> Option<&str> {
    let start = u32::from(span.start()) as usize;
    let end = u32::from(span.end()) as usize;
    if start > end || end > source.len() {
        return None;
    }
    source.get(start..end)
}

fn compare_provenance(
    lhs: &AnnotationProvenance,
    rhs: &AnnotationProvenance,
) -> std::cmp::Ordering {
    lhs.source_path
        .cmp(&rhs.source_path)
        .then(lhs.line.cmp(&rhs.line))
        .then(lhs.column.cmp(&rhs.column))
        .then(lhs.directive.cmp(&rhs.directive))
        .then(lhs.raw.cmp(&rhs.raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser;
    use crate::parser::ast::AstNode;

    fn fixture_hir(source: &str) -> (Module, HashMap<PathBuf, String>, ProjectProvenance) {
        let (node, errors) = parser::parse_with_errors(source);
        assert!(errors.is_empty(), "{errors:?}");
        let root = parser::ast::Root::cast(node).expect("expected root node");
        let module = lower(root);
        fixture_hir_from_module(module, source)
    }

    fn fixture_hir_allow_errors(
        source: &str,
    ) -> (Module, HashMap<PathBuf, String>, ProjectProvenance) {
        let (node, _errors) = parser::parse_with_errors(source);
        let root = parser::ast::Root::cast(node).expect("expected root node");
        let module = lower(root);
        fixture_hir_from_module(module, source)
    }

    fn fixture_hir_from_module(
        module: Module,
        source: &str,
    ) -> (Module, HashMap<PathBuf, String>, ProjectProvenance) {
        let source_path = PathBuf::from("/tmp/app/src/domain/render.wr");
        let mut module_sources = HashMap::new();
        module_sources.insert(source_path.clone(), source.to_string());

        let mut provenance = ProjectProvenance::default();
        for (func_id, func) in module.functions.iter() {
            provenance
                .function_owner_path_by_id
                .insert(func_id.into_raw(), source_path.clone());
            provenance
                .function_owner_path_by_name
                .insert(func.name.clone(), source_path.clone());
            provenance.function_owner_span_by_id.insert(
                func_id.into_raw(),
                func.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
            );
        }
        for material in &module.material_declarations {
            provenance
                .material_declaration_owner_path_by_name
                .insert(material.name.clone(), source_path.clone());
            provenance
                .material_declaration_span_by_name
                .insert(material.name.clone(), material.span);
        }
        for render in module
            .render_contracts
            .iter()
            .filter(|contract| matches!(contract.kind, SurfaceDeclarationKind::Render))
        {
            provenance
                .render_contract_owner_path_by_name
                .insert(render.name.clone(), source_path.clone());
            provenance
                .render_contract_span_by_name
                .insert(render.name.clone(), render.span);
        }
        for gpu in &module.gpu_functions {
            provenance
                .gpu_function_owner_path_by_name
                .insert(gpu.name.clone(), source_path.clone());
            provenance
                .gpu_function_span_by_name
                .insert(gpu.name.clone(), gpu.span);
        }

        (module, module_sources, provenance)
    }

    fn fixture_source() -> &'static str {
        r#"render sprite_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, lighting
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui, frame
}
"#
    }

    #[test]
    fn extracts_render_plan_and_frame_graph_with_expansion_trace() {
        let (module, sources, provenance) = fixture_hir(fixture_source());
        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir");
        assert_eq!(ir.render.render_plan.len(), 2);
        assert_eq!(ir.render.pipelines.len(), 2);
        assert_eq!(ir.render.frame_graph.len(), 13);
        assert_eq!(ir.shader.modules.len(), 2);
        assert!(
            ir.provenance
                .expansion_trace
                .iter()
                .any(|entry| entry.directive == "render"),
            "expansion trace should include render declarations"
        );
        assert!(
            ir.provenance
                .expansion_trace
                .iter()
                .any(|entry| entry.directive == "shader generated"),
            "expansion trace should include generated shader expansions"
        );
    }

    #[test]
    fn rejects_missing_required_render_declarations() {
        let source = r#"gpu fn sprite_lane_shader() -> String {
    return "@vertex\nfn vs_main() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_main() -> @location(0) vec4<f32>\n"
}
"#;
        let (module, sources, provenance) = fixture_hir(source);
        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected validation failure");
        assert!(
            matches!(
                &err,
                RenderShaderIrError::Validation { message }
                if message.contains("missing required `render")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn ignores_assets_and_mmo_for_required_render_contract_check() {
        let source = r#"assets UiAssets {
    manifest web_manifest
    streaming chunked
}

mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
    world earth_world
}
"#;
        let (module, sources, provenance) = fixture_hir(source);
        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected validation failure");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                if message.contains("missing required `render")
            ),
            "unexpected validation error: {err}"
        );
        let RenderShaderIrError::Validation { message } = err else {
            panic!("unexpected error: {err}");
        };
        assert!(
            !message.contains("target <NodeType>"),
            "assets/mmo declarations must not trigger legacy target errors: {message}"
        );
    }

    #[test]
    fn extracts_only_true_render_contracts_when_assets_and_mmo_are_present() {
        let source = r#"assets UiAssets {
    manifest web_manifest
    streaming chunked
}

mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
    world earth_world
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (module, sources, provenance) = fixture_hir(source);
        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir");
        assert_eq!(ir.render.render_plan.len(), 1);
        assert_eq!(ir.render.pipelines.len(), 1);
        assert_eq!(ir.render.render_plan[0].name, "ui_lane");
        assert_eq!(ir.render.render_plan[0].id, "ui_lane");
    }

    #[test]
    fn rejects_legacy_annotation_render_source() {
        let source = r#"@shader(stage=vertex, entry=vs_main)
fn sprite_lane_shader() -> String {
    return "@vertex\nfn vs_main() -> @builtin(position) vec4<f32>\n"
}
render sprite_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui, frame
}
"#;
        let (module, sources, provenance) = fixture_hir_allow_errors(source);
        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected hard cut legacy rejection");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                if message.contains("legacy annotation-based render source is unsupported")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn supports_generated_shader_mode_without_gpu_functions() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.generated = Some(render.span);

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir with generated mode");
        assert_eq!(ir.render.render_plan.len(), 1);
        assert_eq!(ir.render.render_plan[0].shader_mode, "stable");
        assert_eq!(ir.shader.modules.len(), 1);
        assert!(ir.shader.modules[0].id.contains("generated_shader"));
    }

    #[test]
    fn resolves_explicit_gpu_shader_mode_by_function_name() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, ui, unsafe_raw_shader
}

gpu fn first_shader() -> String {
    return "@vertex\nfn vs_first() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_first() -> @location(0) vec4<f32>\n"
}

gpu fn second_shader() -> String {
    return "@vertex\nfn vs_second() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_second() -> @location(0) vec4<f32>\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("second_shader"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir with explicit gpu mode");
        assert_eq!(ir.render.render_plan[0].shader_mode, "reproject");
        assert_eq!(
            ir.render.render_plan[0].shader_ref.as_deref(),
            Some("second_shader")
        );
        assert_eq!(ir.render.render_plan[0].shader_module, "second_shader");
    }

    #[test]
    fn gpu_fn_expression_codegen_roundtrip() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, unsafe_raw_shader
}

gpu fn raw_surface_shader() -> String {
    return (0.2 + 0.3) * 0.9
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("expression-return gpu fn should lower into WGSL");
        let shader = ir
            .shader
            .modules
            .iter()
            .find(|module| module.id == "raw_surface_shader")
            .expect("raw shader module");
        assert!(
            shader
                .source
                .contains("// generated from gpu fn expression"),
            "unexpected source: {}",
            shader.source
        );
        assert!(
            shader
                .source
                .contains("let surface_value = ((0.2 + 0.3) * 0.9);"),
            "unexpected source: {}",
            shader.source
        );
        assert!(shader.source.contains("@fragment"));
    }

    #[test]
    fn gpu_fn_expression_codegen_roundtrip_vec3_shape() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, unsafe_raw_shader
}

gpu fn raw_surface_shader() -> String {
    return vec3(0.2, 0.4, 0.8) + vec3(0.1, 0.05, 0.0)
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("vec3 expression-return gpu fn should lower into WGSL");
        let shader = ir
            .shader
            .modules
            .iter()
            .find(|module| module.id == "raw_surface_shader")
            .expect("raw shader module");
        assert!(
            shader
                .source
                .contains("let color = vec4<f32>(surface_value, 1.0);"),
            "unexpected source: {}",
            shader.source
        );
    }

    #[test]
    fn gpu_fn_type_error_diagnostic() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, unsafe_raw_shader
}

gpu fn raw_surface_shader() -> String {
    return [1, 2, 3]
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("non-lowerable expression must fail");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                    if message.contains("WGSL-compatible expression tree")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn unsafe_raw_shader_opt_in_still_required() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, ui
}

gpu fn raw_surface_shader() -> String {
    return "@vertex\n" + "fn vs_main() -> @builtin(position) vec4<f32>\n" + "@fragment\n" + "fn fs_main() -> @location(0) vec4<f32>\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("raw string concat mode must still require unsafe opt-in");
        assert!(
            matches!(err, RenderShaderIrError::Validation { ref message } if message.contains("unsafe_raw_shader")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_raw_gpu_shader_mode_without_unsafe_opt_in_tag() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, ui
}

gpu fn raw_surface_shader() -> String {
    return "@vertex\nfn vs_main() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_main() -> @location(0) vec4<f32>\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected unsafe raw shader opt-in failure");
        assert!(
            matches!(err, RenderShaderIrError::Validation { ref message } if message.contains("unsafe_raw_shader")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_raw_gpu_shader_mode_with_unsafe_opt_in_tag() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, unsafe_raw_shader
}

gpu fn raw_surface_shader() -> String {
    return "@vertex\nfn vs_main() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_main() -> @location(0) vec4<f32>\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("raw shader with unsafe opt-in should be accepted");
        assert_eq!(
            ir.render.render_plan[0].shader_ref.as_deref(),
            Some("raw_surface_shader")
        );
        assert_eq!(ir.render.render_plan[0].shader_module, "raw_surface_shader");
    }

    #[test]
    fn rejects_raw_gpu_shader_when_lint_fails() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags frame, unsafe_raw_shader
}

gpu fn raw_surface_shader() -> String {
    return "@vertex\nfn vs_main() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_main() -> @location(0) vec4<f32>\n// forbidden stage marker:\n@compute\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("raw_surface_shader"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected raw shader lint failure");
        assert!(
            matches!(err, RenderShaderIrError::Validation { ref message } if message.contains("raw shader lint failed") && message.contains("@compute")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn supports_material_shader_mode_with_material_reference() {
        let source = r#"material UiMaterial {
    surface_model pbr
    textures orm "ui_orm.ktx2"
    textures normal "ui_normal.ktx2"
    textures emissive "ui_emissive.ktx2"
    features clearcoat true
    render alpha blend
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("UiMaterial"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir with material mode");
        assert_eq!(ir.render.render_plan[0].shader_mode, "stable");
        assert_eq!(
            ir.render.render_plan[0].shader_ref.as_deref(),
            Some("UiMaterial")
        );
        assert_eq!(ir.shader.modules.len(), 1);
        assert_eq!(
            ir.shader.modules[0].id,
            "ui_lane_material_uimaterial_shader"
        );
        assert!(
            ir.shader.modules[0]
                .source
                .contains("material ref: UiMaterial")
        );
        assert!(
            ir.shader.modules[0]
                .source
                .contains("active_quality_tier: gameplay")
        );
        let report = ir.shader.modules[0]
            .material_compile_report
            .as_ref()
            .expect("material compile report should be present");
        assert_eq!(report.variant_count, 3);
        assert_eq!(report.quality_tier_costs.len(), 3);
        assert_eq!(report.runtime_texture_format, "ktx2");
        assert!(
            report
                .texture_bindings
                .iter()
                .any(|binding| binding.slot == "orm")
        );
        assert!(
            report
                .texture_lint_lines
                .iter()
                .any(|line| line.contains("includes required ORM texture slot"))
        );
    }

    #[test]
    fn material_shader_mode_emits_distinct_source_for_surface_and_alpha() {
        let source = r#"material PbrCarPaint {
    surface_model pbr
    textures orm "car_orm.ktx2"
    textures normal "car_normal.ktx2"
    textures emissive "car_emissive.ktx2"
    features clearcoat true
    features anisotropy true
    render alpha blend
}

material UiGlyph {
    surface_model unlit
    textures albedo "glyph_albedo.ktx2"
    render alpha mask
}

render lane_a {
    resources UiAssets
    temporal stable
    quality tier high
    budget tags ui
}

render lane_b {
    resources UiAssets
    temporal stable
    quality tier low
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render_a = module
            .render_contracts
            .iter_mut()
            .find(|contract| contract.name == "lane_a")
            .expect("lane_a render contract");
        render_a.shader_modes = crate::hir::RenderShaderModeSet::default();
        render_a.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("PbrCarPaint"),
            span: render_a.span,
        });
        let render_b = module
            .render_contracts
            .iter_mut()
            .find(|contract| contract.name == "lane_b")
            .expect("lane_b render contract");
        render_b.shader_modes = crate::hir::RenderShaderModeSet::default();
        render_b.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("UiGlyph"),
            span: render_b.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir with multiple material modes");
        assert_eq!(ir.shader.modules.len(), 2);
        let lane_a = ir
            .shader
            .modules
            .iter()
            .find(|module| module.id == "lane_a_material_pbrcarpaint_shader")
            .expect("lane_a shader module");
        let lane_b = ir
            .shader
            .modules
            .iter()
            .find(|module| module.id == "lane_b_material_uiglyph_shader")
            .expect("lane_b shader module");
        assert_ne!(lane_a.source, lane_b.source);
        assert!(lane_a.source.contains("active_quality_tier: hero"));
        assert!(lane_b.source.contains("active_quality_tier: low"));
        assert!(lane_b.source.contains("discard"));
    }

    #[test]
    fn material_compile_is_deterministic_for_identical_input() {
        let source = r#"material DeterministicPaint {
    surface_model pbr
    textures orm "paint_orm.ktx2"
    textures normal "paint_normal.ktx2"
    textures emissive "paint_emissive.ktx2"
    features clearcoat true
    features anisotropy true
    render alpha blend
}
"#;
        let (module, _sources, _provenance) = fixture_hir(source);
        let material = module
            .material_declarations
            .iter()
            .find(|entry| entry.name == "DeterministicPaint")
            .expect("material declaration");
        let compiled_a = compile_material_shader("determinism_lane", material, "high")
            .expect("material compile should succeed");
        let compiled_b = compile_material_shader("determinism_lane", material, "high")
            .expect("material compile should succeed");
        assert_eq!(compiled_a.report, compiled_b.report);
        assert_eq!(compiled_a.wgsl_source, compiled_b.wgsl_source);
    }

    #[test]
    fn material_shader_mode_fails_when_pbr_lacks_orm_slot() {
        let source = r#"material MissingOrm {
    surface_model pbr
    textures albedo "wood_albedo.ktx2"
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("MissingOrm"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("missing orm slot must fail closed");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                    if message.contains("requires texture slot 'orm'")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn material_shader_mode_reports_texture_extension_transcoding_to_ktx2() {
        let source = r#"material PhotoScannedRock {
    surface_model pbr
    textures albedo "rock_albedo.png"
    textures orm "rock_orm.exr"
    textures normal "rock_normal.tga"
    textures emissive "rock_emissive.jpg"
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("PhotoScannedRock"),
            span: render.span,
        });

        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extension policy should allow transcodeable sources");
        let report = ir.shader.modules[0]
            .material_compile_report
            .as_ref()
            .expect("material compile report should be present");
        assert_eq!(report.runtime_texture_format, "ktx2");
        assert!(
            report
                .texture_lint_lines
                .iter()
                .any(|line| line.contains("will compile to ktx2 runtime target"))
        );
        assert!(report.texture_bindings.iter().all(|binding| {
            binding
                .runtime_reference
                .to_ascii_lowercase()
                .ends_with(".ktx2")
        }));
    }

    #[test]
    fn material_shader_mode_rejects_unsupported_texture_extension() {
        let source = r#"material UnsupportedTextureExt {
    surface_model pbr
    textures albedo "asset_albedo.dds"
    textures orm "asset_orm.ktx2"
}

render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("UnsupportedTextureExt"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("unsupported texture extension must fail closed");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                    if message.contains("unsupported extension 'dds'")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn rejects_material_shader_mode_when_compile_budget_gate_is_exceeded() {
        let source = r#"material BudgetBomb {
    surface_model pbr
    textures albedo "bomb_albedo.ktx2"
    textures orm "bomb_orm.ktx2"
    textures normal "bomb_normal.ktx2"
    textures emissive "bomb_emissive.ktx2"
    textures detail_normal "bomb_detail_normal.ktx2"
    features clearcoat true
    features transmission true
    features anisotropy true
    features subsurface_lite true
    render alpha blend
}

render budget_lane {
    resources UiAssets
    temporal stable
    quality tier high
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("BudgetBomb"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected compile budget gate failure");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                    if message.contains("failed compile budget gates")
                        && (message.contains("texture fetch budget exceeded")
                            || message.contains("ALU budget exceeded"))
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn variant_cardinality_fail_closed_when_threshold_exceeded() {
        let err = validate_variant_cardinality(
            9,
            8,
            "test render contract 'ui_lane' material 'UiMaterial'",
        )
        .expect_err("expected variant cardinality error");
        assert!(
            matches!(err, RenderShaderIrError::Validation { ref message } if message.contains("variant cardinality exceeded")),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_material_shader_mode_without_in_source_material_declaration() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes = crate::hir::RenderShaderModeSet::default();
        render.shader_modes.material = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("UiMaterial"),
            span: render.span,
        });

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected missing material declaration validation error");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                    if message.contains("unknown in-source material declaration 'UiMaterial'")
                        && message.contains("available material declarations: <none>")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn rejects_multiple_shader_modes_per_render_contract() {
        let source = r#"render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags ui
}

gpu fn ui_lane_shader() -> String {
    return "@vertex\nfn vs_ui() -> @builtin(position) vec4<f32>\n@fragment\nfn fs_ui() -> @location(0) vec4<f32>\n"
}
"#;
        let (mut module, sources, provenance) = fixture_hir(source);
        let render = module
            .render_contracts
            .first_mut()
            .expect("expected render contract");
        render.shader_modes.gpu = Some(crate::hir::RenderShaderSymbolSurface {
            symbol: SmolStr::new("ui_lane_shader"),
            span: render.span,
        });
        render.shader_modes.generated = Some(render.span);

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected conflicting shader mode error");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                if message.contains("must declare exactly one shader mode")
            ),
            "unexpected validation error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_shader_mode_clauses_from_source_contract() {
        let source = r#"material UiMaterial {
    surface_model pbr
    textures albedo "ui_albedo.ktx2"
    textures normal "ui_normal.ktx2"
    textures orm "ui_orm.ktx2"
}
material UiMaterialFallback {
    surface_model pbr
    textures albedo "ui_fallback_albedo.ktx2"
    textures normal "ui_fallback_normal.ktx2"
    textures orm "ui_fallback_orm.ktx2"
}
render ui_lane {
    resources UiAssets
    temporal stable
    quality tier medium
    shader material UiMaterial
    shader material UiMaterialFallback
    budget tags ui
}
"#;
        let (module, sources, provenance) = fixture_hir(source);

        let err = extract_render_shader_ir(&module, &sources, &provenance)
            .expect_err("expected duplicate shader mode validation error");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                if message.contains("must declare exactly one shader mode")
                    && message.contains("material UiMaterial")
                    && message.contains("material UiMaterialFallback")
            ),
            "unexpected validation error: {err}"
        );
    }

    fn minimal_render_shader_ir_for_manifest_tests() -> RenderShaderIr {
        let provenance = fixture_provenance();
        RenderShaderIr {
            render: RenderIr {
                render_plan: vec![RenderPlanContractIr {
                    id: "sprite_lane".to_string(),
                    name: "sprite_lane".to_string(),
                    resources: "UiAssets".to_string(),
                    temporal: "stable".to_string(),
                    quality_tier: "medium".to_string(),
                    budget_tags: vec!["ui".to_string(), "frame".to_string()],
                    target: "UiAssets".to_string(),
                    preset: "medium".to_string(),
                    profile: "ui+frame".to_string(),
                    shader_mode: "stable".to_string(),
                    shader_ref: None,
                    override_tiers: vec![],
                    shader_module: "sprite_lane_shader".to_string(),
                    provenance: provenance.clone(),
                }],
                scene_packet_layout: strict_scene_packet_layout(&provenance, "medium"),
                bind_groups: strict_bind_groups(&provenance),
                pipelines: vec![RenderPipelineIr {
                    id: "sprite_lane_pipeline".to_string(),
                    label: "sprite_lane".to_string(),
                    shader_module: "sprite_lane_shader".to_string(),
                    vertex_entry: "vs_main".to_string(),
                    fragment_entry: "fs_main".to_string(),
                    topology: RenderPipelineTopologyV5::Triangles,
                    cull_mode: RenderPipelineCullModeV5::None,
                    targets: vec![RenderPipelineTargetV5::SurfaceColor],
                    provenance: provenance.clone(),
                }],
                frame_graph: vec![RenderPassIr {
                    name: "sprite_lane_pipeline_opaque".to_string(),
                    pipeline: "sprite_lane_pipeline".to_string(),
                    draw_phase: "opaque".to_string(),
                    depends_on: vec![],
                    provenance: provenance.clone(),
                }],
            },
            shader: ShaderIr {
                modules: vec![ShaderModuleIr {
                    id: "sprite_lane_shader".to_string(),
                    source: "@vertex\nfn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }\n@fragment\nfn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }".to_string(),
                    vertex_entry: "vs_main".to_string(),
                    fragment_entry: "fs_main".to_string(),
                    material_compile_report: None,
                    provenance: provenance.clone(),
                }],
            },
            provenance: RenderShaderExtractionProvenance {
                source_files: vec![provenance.source_path.clone()],
                expansion_trace: vec![provenance],
            },
        }
    }

    #[test]
    fn render_manifest_includes_v5_schema_and_contract_sections() {
        let ir = minimal_render_shader_ir_for_manifest_tests();
        let mut shader_paths = HashMap::new();
        shader_paths.insert(
            "sprite_lane_shader".to_string(),
            "sprite_lane_shader.wgsl".to_string(),
        );
        let manifest = emit_render_manifest(
            &ir,
            &shader_paths,
            &RenderManifestContext {
                render_backend: "webgpu".to_string(),
                app_mode: "game".to_string(),
                collectible_capacity: 9,
                entry_path: "/tmp/app/src/main.wr".to_string(),
                domain_source_hash: "abc123".to_string(),
            },
        )
        .expect("emit render manifest");

        assert_eq!(manifest["schema_version"], RENDER_SCHEMA_VERSION_V6);
        assert_eq!(
            manifest["frame_graph"].as_array().map(|it| it.len()),
            Some(1)
        );
        assert!(
            manifest["contracts"]["resources"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            manifest["contracts"]["capabilities"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            manifest["contracts"]["pipelines"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            manifest["contracts"]["passes"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert_eq!(
            manifest["gpu_scene_buffers"]["transforms"]["resource_id"],
            "scene_transforms"
        );
        assert_eq!(
            manifest["gpu_scene_buffers"]["draw_records"]["kind"],
            "storage-buffer"
        );
        assert_eq!(
            manifest["provenance"]["expansion_trace"]["schema_version"],
            EXPANSION_TRACE_SCHEMA_VERSION_V1
        );
        assert_eq!(manifest["render_plan"][0]["resources"], "UiAssets");
        assert_eq!(manifest["render_plan"][0]["temporal"], "stable");
        assert_eq!(manifest["render_plan"][0]["shader_mode"], "stable");
        assert_eq!(
            manifest["contracts"]["default_profile"]["lighting"]["pbr"]["enabled"],
            true
        );
        assert_eq!(
            manifest["contracts"]["default_profile"]["reflections"]["fallback_chain"][1],
            "ssr"
        );
        assert_eq!(
            manifest["contracts"]["default_profile"]["temporal"]["taa"]["enabled"],
            true
        );
        assert_eq!(
            manifest["pipelines"][0]["primitive"]["topology"],
            "triangles"
        );
        assert_eq!(manifest["pipelines"][0]["primitive"]["cull_mode"], "none");
        assert_eq!(
            manifest["pipelines"][0]["targets"][0]["surface"],
            "surface-color"
        );
        assert_eq!(manifest["shader_modules"][0]["id"], "sprite_lane_shader");

        let serialized = serde_json::to_string(&manifest).expect("serialize manifest");
        for leaked_token in ["triangle-list", "bgra8unorm", "wgpu::"] {
            assert!(
                !serialized.contains(leaked_token),
                "manifest leaked adapter token '{leaked_token}'"
            );
        }
    }

    #[test]
    fn render_manifest_rejects_missing_resolved_shader_paths() {
        let (module, sources, provenance) = fixture_hir(fixture_source());
        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir");
        let err = emit_render_manifest(
            &ir,
            &HashMap::new(),
            &RenderManifestContext {
                render_backend: "webgpu".to_string(),
                app_mode: "game".to_string(),
                collectible_capacity: 9,
                entry_path: "/tmp/app/src/main.wr".to_string(),
                domain_source_hash: "abc123".to_string(),
            },
        )
        .expect_err("expected missing path failure");
        assert!(
            matches!(
                err,
                RenderShaderIrError::Validation { ref message }
                if message.contains("missing resolved shader path")
            ),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn shader_bundle_manifest_includes_v5_schema_and_contract_sections() {
        let ir = minimal_render_shader_ir_for_manifest_tests();
        let module = &ir.shader.modules[0];
        let bundle = emit_shader_bundle_manifest(
            &ir,
            &[ResolvedShaderModuleManifestEntry {
                id: module.id.clone(),
                path: "sprite_lane_shader.wgsl".to_string(),
                entrypoints: vec![module.fragment_entry.clone(), module.vertex_entry.clone()],
                checksum: 1337,
                source_path: module.provenance.source_path.clone(),
                provenance: module.provenance.clone(),
            }],
            &ShaderBundleManifestContext {
                render_manifest_path: "/tmp/app/target/render-manifest.json".to_string(),
                entry_path: "/tmp/app/src/main.wr".to_string(),
                domain_source_hash: "abc123".to_string(),
            },
        );

        assert_eq!(bundle["schema_version"], SHADER_BUNDLE_SCHEMA_VERSION_V6);
        assert_eq!(bundle["shader_modules"][0]["checksum"], 1337);
        assert_eq!(bundle["shader_modules"][0]["entrypoints"][0], "fs_main");
        assert!(
            bundle["contracts"]["resources"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            bundle["contracts"]["passes"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert_eq!(
            bundle["provenance"]["expansion_trace"]["schema_version"],
            EXPANSION_TRACE_SCHEMA_VERSION_V1
        );
    }

    #[test]
    fn render_manifest_is_deterministic_with_unsorted_shader_path_map() {
        let (module, sources, provenance) = fixture_hir(fixture_source());
        let ir = extract_render_shader_ir(&module, &sources, &provenance)
            .expect("extract render/shader ir");

        let mut module_ids = ir
            .shader
            .modules
            .iter()
            .map(|module| module.id.clone())
            .collect::<Vec<_>>();
        module_ids.sort();
        assert_eq!(
            module_ids.len(),
            2,
            "fixture should emit exactly two modules"
        );

        let mut lhs_paths = HashMap::new();
        lhs_paths.insert(
            module_ids[0].clone(),
            format!("{}.wgsl", module_ids[0].clone()),
        );
        lhs_paths.insert(
            module_ids[1].clone(),
            format!("{}.wgsl", module_ids[1].clone()),
        );

        let mut rhs_paths = HashMap::new();
        rhs_paths.insert(
            module_ids[1].clone(),
            format!("{}.wgsl", module_ids[1].clone()),
        );
        rhs_paths.insert(
            module_ids[0].clone(),
            format!("{}.wgsl", module_ids[0].clone()),
        );

        let lhs = emit_render_manifest(
            &ir,
            &lhs_paths,
            &RenderManifestContext {
                render_backend: "webgpu".to_string(),
                app_mode: "game".to_string(),
                collectible_capacity: 9,
                entry_path: "/tmp/app/src/main.wr".to_string(),
                domain_source_hash: "abc123".to_string(),
            },
        )
        .expect("emit lhs manifest");

        let rhs = emit_render_manifest(
            &ir,
            &rhs_paths,
            &RenderManifestContext {
                render_backend: "webgpu".to_string(),
                app_mode: "game".to_string(),
                collectible_capacity: 9,
                entry_path: "/tmp/app/src/main.wr".to_string(),
                domain_source_hash: "abc123".to_string(),
            },
        )
        .expect("emit rhs manifest");

        assert_eq!(lhs, rhs);
    }

    fn fixture_provenance() -> AnnotationProvenance {
        AnnotationProvenance {
            source_path: "/tmp/app/src/domain/render.wr".to_string(),
            line: 1,
            column: 1,
            directive: "render".to_string(),
            raw: "render fixture".to_string(),
        }
    }

    #[test]
    fn topological_sort_reorders_unsorted_passes_into_dependency_order() {
        let provenance = fixture_provenance();
        let unsorted = vec![
            RenderPassIr {
                name: "post".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "post".to_string(),
                depends_on: vec!["ui".to_string()],
                provenance: provenance.clone(),
            },
            RenderPassIr {
                name: "depth".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "depth".to_string(),
                depends_on: vec![],
                provenance: provenance.clone(),
            },
            RenderPassIr {
                name: "ui".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "ui".to_string(),
                depends_on: vec!["opaque".to_string()],
                provenance: provenance.clone(),
            },
            RenderPassIr {
                name: "opaque".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "opaque".to_string(),
                depends_on: vec!["depth".to_string()],
                provenance,
            },
        ];

        let ordered = topologically_order_render_passes(
            unsorted.as_slice(),
            "test frame graph dependency ordering",
        )
        .expect("topological ordering should succeed");
        let names = ordered
            .iter()
            .map(|pass| pass.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["depth", "opaque", "ui", "post"]);
    }

    #[test]
    fn topological_sort_rejects_cycles() {
        let provenance = fixture_provenance();
        let cyclic = vec![
            RenderPassIr {
                name: "a".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "opaque".to_string(),
                depends_on: vec!["b".to_string()],
                provenance: provenance.clone(),
            },
            RenderPassIr {
                name: "b".to_string(),
                pipeline: "pipe".to_string(),
                draw_phase: "ui".to_string(),
                depends_on: vec!["a".to_string()],
                provenance,
            },
        ];

        let err = topologically_order_render_passes(cyclic.as_slice(), "test cycle detection")
            .expect_err("cycle should fail");
        assert!(
            matches!(err, RenderShaderIrError::Validation { ref message } if message.contains("dependency cycle")),
            "unexpected error: {err}"
        );
    }
}

// --- WS2: Shader variant/permutation support ---

use crate::hir::def::ShaderFunction;

/// A named feature flag that can be toggled to produce shader variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShaderFeature {
    pub name: SmolStr,
}

/// Describes the full set of shader functions and the feature flags that
/// produce shader permutations.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderPermutationContract {
    pub shaders: Vec<ShaderFunction>,
    pub features: Vec<ShaderFeature>,
}

/// A single compiled shader variant with its feature set recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderVariant {
    pub base_name: SmolStr,
    pub quality_tier: SmolStr,
    pub enabled_features: Vec<SmolStr>,
    pub variant_key: SmolStr,
}

const SHADER_VARIANT_MAX_CARDINALITY: usize = 512;
const SHADER_VARIANT_QUALITY_TIERS: [&str; 3] = ["hero", "gameplay", "low"];

impl ShaderPermutationContract {
    pub fn new() -> Self {
        Self {
            shaders: Vec::new(),
            features: Vec::new(),
        }
    }

    pub fn generate_variants(&self) -> Vec<ShaderVariant> {
        self.try_generate_variants(SHADER_VARIANT_MAX_CARDINALITY)
            .unwrap_or_else(|error| panic!("shader variant generation failed closed: {error}"))
    }

    pub fn try_generate_variants(&self, max_variants: usize) -> Result<Vec<ShaderVariant>, String> {
        let mut variants = Vec::new();
        let n = self.features.len();
        let permutation_count = 1usize << n;

        for shader in &self.shaders {
            for quality_tier in SHADER_VARIANT_QUALITY_TIERS {
                for mask in 0..permutation_count {
                    let mut enabled: Vec<SmolStr> = Vec::new();
                    for (bit, feature) in self.features.iter().enumerate() {
                        if mask & (1 << bit) != 0 {
                            enabled.push(feature.name.clone());
                        }
                    }
                    enabled.sort();
                    enabled.dedup();

                    let variant_key =
                        shader_variant_key(shader.name.as_str(), quality_tier, enabled.as_slice());

                    variants.push(ShaderVariant {
                        base_name: shader.name.clone(),
                        quality_tier: SmolStr::new(quality_tier),
                        enabled_features: enabled,
                        variant_key,
                    });

                    if variants.len() > max_variants {
                        return Err(format!(
                            "variant cardinality exceeded: generated {} variants, max {}",
                            variants.len(),
                            max_variants
                        ));
                    }
                }
            }
        }

        Ok(variants)
    }
}

fn shader_variant_key(
    base_name: &str,
    quality_tier: &str,
    enabled_features: &[SmolStr],
) -> SmolStr {
    let features = if enabled_features.is_empty() {
        "base".to_string()
    } else {
        enabled_features.join("+")
    };
    SmolStr::new(format!(
        "{base_name}::tier={quality_tier}::features={features}"
    ))
}

#[cfg(test)]
mod variant_tests {
    use super::{ShaderFeature, ShaderPermutationContract};
    use crate::hir::def::ShaderFunction;
    use smol_str::SmolStr;

    fn shader_fn(name: &str) -> ShaderFunction {
        ShaderFunction {
            name: SmolStr::new(name),
            name_span: None,
            params: Vec::new(),
            ret_type: None,
            body: None,
        }
    }

    #[test]
    fn variant_keys_are_deterministic() {
        let contract = ShaderPermutationContract {
            shaders: vec![shader_fn("lit_surface")],
            features: vec![
                ShaderFeature {
                    name: SmolStr::new("HAS_NORMAL"),
                },
                ShaderFeature {
                    name: SmolStr::new("CLEARCOAT"),
                },
            ],
        };
        let variants_a = contract
            .try_generate_variants(128)
            .expect("variant generation should succeed");
        let variants_b = contract
            .try_generate_variants(128)
            .expect("variant generation should succeed");
        assert_eq!(variants_a, variants_b);
        assert!(
            variants_a
                .iter()
                .all(|variant| variant.variant_key.contains("tier="))
        );
    }

    #[test]
    fn variant_generation_fails_when_threshold_is_exceeded() {
        let contract = ShaderPermutationContract {
            shaders: vec![shader_fn("lit_surface")],
            features: vec![
                ShaderFeature {
                    name: SmolStr::new("HAS_NORMAL"),
                },
                ShaderFeature {
                    name: SmolStr::new("HAS_EMISSIVE"),
                },
                ShaderFeature {
                    name: SmolStr::new("CLEARCOAT"),
                },
            ],
        };
        let err = contract
            .try_generate_variants(8)
            .expect_err("variant threshold should fail");
        assert!(
            err.contains("variant cardinality exceeded"),
            "unexpected error: {err}"
        );
    }
}
