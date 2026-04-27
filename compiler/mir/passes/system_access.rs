use crate::hir::{Function, FunctionRole, Module, TypeRef};
use crate::system_contract::{
    EventTypeId, SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use crate::system_plan::{SystemPlan, SystemPlanError, SystemProgram};
use smol_str::SmolStr;

pub fn summarize_system_access(function: &Function) -> SystemAccessSummary {
    let mut summary = SystemAccessSummary::default();
    if let Some(metadata) = function.system_metadata.as_ref() {
        for read in &metadata.reads {
            summary
                .reads
                .insert(SystemResourceId::Resource(read.clone()));
        }
        for write in &metadata.writes {
            summary
                .writes
                .insert(SystemResourceId::Resource(write.clone()));
        }
    }
    for param in &function.params {
        let Some(ty) = param.ty.as_ref() else {
            continue;
        };
        if ty.name == "InputFrame" {
            summary.reads.insert(SystemResourceId::InputFrame);
            continue;
        }
        if ty.name == "EventEmitter" {
            if let Some(event_type) = ty.args.first() {
                summary
                    .emits_events
                    .insert(EventTypeId::new(event_type.name.clone()));
            }
            continue;
        }
        let resource = SystemResourceId::Resource(type_resource_name(ty));
        if param.mutable {
            summary.writes.insert(resource);
        } else {
            summary.reads.insert(resource);
        }
    }
    summary
}

pub fn build_system_program_from_module(module: &Module) -> Result<SystemProgram, SystemPlanError> {
    let plans = module
        .functions
        .iter()
        .filter_map(|(idx, function)| {
            if function.role != FunctionRole::System {
                return None;
            }
            let phase = system_phase(function);
            let metadata = function.system_metadata.as_ref();
            let mut plan = SystemPlan::new(
                SystemId::new(function.name.clone()),
                SystemContractId::new(function.name.clone()),
                phase,
                summarize_system_access(function),
                idx.into_raw() as u32,
            );
            if let Some(metadata) = metadata {
                for before in &metadata.before {
                    plan = plan.runs_before(SystemId::new(before.clone()));
                }
                for after in &metadata.after {
                    plan = plan.runs_after(SystemId::new(after.clone()));
                }
            }
            Some(plan)
        })
        .collect::<Vec<_>>();
    SystemProgram::new(plans)
}

fn system_phase(function: &Function) -> SystemPhase {
    let value = function
        .system_metadata
        .as_ref()
        .and_then(|metadata| metadata.phase.as_deref().or(metadata.stage.as_deref()));
    match value {
        Some("pre_sim" | "input") => SystemPhase::PreSim,
        Some("sim" | "fixed") => SystemPhase::Sim,
        Some("post_sim" | "post_fixed" | "late") => SystemPhase::PostSim,
        _ => SystemPhase::Sim,
    }
}

fn type_resource_name(ty: &TypeRef) -> SmolStr {
    ty.name.clone()
}
