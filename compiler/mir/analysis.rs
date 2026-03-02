use crate::mir::ir::{CallTarget, MirFunction, MirModule, MirType, Rvalue, Stmt};
use smol_str::SmolStr;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    edges: HashMap<SmolStr, Vec<SmolStr>>,
    counts: HashMap<SmolStr, usize>,
}

impl CallGraph {
    pub fn edges(&self, func: &SmolStr) -> &[SmolStr] {
        self.edges.get(func).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn call_count(&self, func: &SmolStr) -> usize {
        self.counts.get(func).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FunctionTypes {
    pub locals: Vec<MirType>,
    pub temps: Vec<MirType>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeMap {
    functions: HashMap<SmolStr, FunctionTypes>,
}

impl TypeMap {
    pub fn function(&self, name: &SmolStr) -> Option<&FunctionTypes> {
        self.functions.get(name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleAnalysis {
    pub call_graph: CallGraph,
    pub type_map: TypeMap,
}

pub fn analyze_module(module: &MirModule) -> ModuleAnalysis {
    let mut call_graph = CallGraph::default();
    let mut type_map = TypeMap::default();

    for func in &module.functions {
        let types = FunctionTypes {
            locals: func.locals.iter().map(|local| local.ty.clone()).collect(),
            temps: func.temps.iter().map(|temp| temp.ty.clone()).collect(),
        };
        type_map.functions.insert(func.name.clone(), types);
        record_calls(func, &mut call_graph);
    }

    ModuleAnalysis {
        call_graph,
        type_map,
    }
}

fn record_calls(func: &MirFunction, graph: &mut CallGraph) {
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            let Rvalue::Call { target, .. } = value else {
                continue;
            };
            for name in call_target_names(target) {
                graph
                    .edges
                    .entry(func.name.clone())
                    .or_default()
                    .push(name.clone());
                *graph.counts.entry(name).or_insert(0) += 1;
            }
        }
    }
}

fn call_target_names(target: &CallTarget) -> Vec<SmolStr> {
    match target {
        CallTarget::Function(name) => vec![name.clone()],
        CallTarget::Method { method, .. } => vec![method.clone()],
        CallTarget::GuardedInterface {
            fast_paths,
            fallback,
        } => {
            let mut out = Vec::with_capacity(fast_paths.len() + 1);
            for (_tag, func) in fast_paths {
                out.push(func.clone());
            }
            out.push(fallback.clone());
            out
        }
        CallTarget::Indirect(_) => Vec::new(),
    }
}
