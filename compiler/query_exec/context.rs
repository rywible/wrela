use crate::hir;
use crate::hir::typeck::TypeInfo;
use crate::query_exec::ids::{
    stable_field_scene_capture_id, stable_region_scene_capture_id, stable_shape_capture_id,
    stable_shape_scene_capture_id,
};
use crate::query_exec::region::{RegionExecCase, build_region_exec_cases};
use crate::scene_ir;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct QueryExecContext {
    pub module: hir::Module,
    pub type_info: TypeInfo,
    pub scene: scene_ir::SceneIrModule,
    pub functions_by_name: HashMap<SmolStr, hir::Function>,
    pub field_graphs: BTreeMap<SmolStr, hir::FieldGraph>,
    pub field_bodies: BTreeMap<SmolStr, hir::Body>,
    pub field_metadata: BTreeMap<SmolStr, hir::FieldMetadata>,
    pub shape_graphs: BTreeMap<SmolStr, hir::ShapeGraph>,
    pub value_class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    pub fields_by_name: HashMap<SmolStr, hir::Function>,
    pub regions_by_name: HashMap<SmolStr, hir::Function>,
    pub region_cases: Vec<RegionExecCase>,
    pub field_names: HashSet<SmolStr>,
    pub shape_names: HashSet<SmolStr>,
}

impl QueryExecContext {
    pub fn compile(module: &hir::Module, type_info: &TypeInfo) -> Self {
        let scene = scene_ir::lower_module(module);
        let functions_by_name = module
            .functions
            .iter()
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let field_graphs = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_bodies = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.body
                    .as_ref()
                    .map(|body| (func.name.clone(), body.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_metadata = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field
                    .as_ref()
                    .map(|metadata| (func.name.clone(), metadata.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let shape_graphs = module
            .shapes
            .iter()
            .filter_map(|(_, shape)| {
                shape
                    .graph
                    .as_ref()
                    .map(|graph| (shape.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let value_class_fields = module
            .classes
            .iter()
            .filter(|(_, class)| class.role == hir::ClassRole::Value)
            .map(|(_, class)| {
                (
                    class.name.clone(),
                    class
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        let fields_by_name = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Field))
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let regions_by_name = module
            .functions
            .iter()
            .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Region))
            .map(|(_, func)| (func.name.clone(), func.clone()))
            .collect::<HashMap<_, _>>();
        let region_cases = build_region_exec_cases(module);
        let field_names = fields_by_name.keys().cloned().collect::<HashSet<_>>();
        let shape_names = module
            .shapes
            .iter()
            .map(|(_, shape)| shape.name.clone())
            .collect::<HashSet<_>>();

        Self {
            module: module.clone(),
            type_info: type_info.clone(),
            scene,
            functions_by_name,
            field_graphs,
            field_bodies,
            field_metadata,
            shape_graphs,
            value_class_fields,
            fields_by_name,
            regions_by_name,
            region_cases,
            field_names,
            shape_names,
        }
    }

    pub fn field_scene_id(&self, name: &SmolStr) -> u32 {
        stable_field_scene_capture_id(name)
    }

    pub fn shape_scene_id(&self, name: &SmolStr) -> u32 {
        stable_shape_scene_capture_id(name)
    }

    pub fn shape_root_feature_id(&self, name: &SmolStr) -> u32 {
        stable_shape_capture_id(name)
    }

    pub fn region_scene_id(&self, name: &SmolStr) -> u32 {
        stable_region_scene_capture_id(name)
    }
}
