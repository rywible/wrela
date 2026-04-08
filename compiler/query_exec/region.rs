use crate::hir;
use crate::query_exec::ids::stable_region_scene_capture_id;
use smol_str::SmolStr;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionShapeLists {
    pub coarse: Vec<SmolStr>,
    pub fine: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionExecCase {
    pub region_name: SmolStr,
    pub scene_id: u32,
    pub shapes: Result<RegionShapeLists, SmolStr>,
}

impl RegionExecCase {
    pub fn shapes_for_detail(&self, detail: i32) -> Result<&[SmolStr], &str> {
        match &self.shapes {
            Ok(shapes) => {
                if detail > 0 {
                    Ok(&shapes.fine)
                } else {
                    Ok(&shapes.coarse)
                }
            }
            Err(message) => Err(message.as_str()),
        }
    }
}

fn region_item_matches_detail(
    domain_detail: hir::DomainGeometryDetail,
    item_detail: Option<hir::RegionDetailLevel>,
) -> bool {
    match item_detail {
        None => true,
        Some(hir::RegionDetailLevel::Coarse) => true,
        Some(hir::RegionDetailLevel::Fine) => {
            matches!(domain_detail, hir::DomainGeometryDetail::Fine)
        }
    }
}

fn resolve_region_shapes_for_detail(
    metadata: &hir::RegionMetadata,
    domain_detail: hir::DomainGeometryDetail,
) -> Result<Vec<SmolStr>, &'static str> {
    fn walk(
        items: &[hir::RegionItemMetadata],
        domain_detail: hir::DomainGeometryDetail,
        named: &mut HashMap<SmolStr, SmolStr>,
        ordered: &mut Vec<SmolStr>,
    ) -> Result<(), &'static str> {
        for item in items {
            match item {
                hir::RegionItemMetadata::Compose {
                    kind,
                    name,
                    shape,
                    detail,
                    ..
                } => {
                    if !region_item_matches_detail(domain_detail, *detail) {
                        continue;
                    }
                    match kind {
                        hir::RegionComposeKind::Place => {
                            if !named.contains_key(name) {
                                named.insert(name.clone(), shape.clone());
                                ordered.push(name.clone());
                            }
                        }
                        hir::RegionComposeKind::Replace => {
                            if !named.contains_key(name) {
                                ordered.push(name.clone());
                            }
                            named.insert(name.clone(), shape.clone());
                        }
                        hir::RegionComposeKind::Overlay => {
                            ordered.push(SmolStr::new(format!(
                                "__overlay_{}_{}",
                                name,
                                ordered.len()
                            )));
                            named
                                .insert(ordered.last().cloned().unwrap_or_default(), shape.clone());
                        }
                    }
                }
                hir::RegionItemMetadata::Scatter { .. } => {
                    return Err("scatter regions are not executable yet");
                }
                hir::RegionItemMetadata::Conditional { .. } => {
                    return Err("conditional regions are not executable yet");
                }
            }
        }
        Ok(())
    }

    let mut named = HashMap::new();
    let mut ordered = Vec::new();
    walk(&metadata.items, domain_detail, &mut named, &mut ordered)?;
    Ok(ordered
        .into_iter()
        .filter_map(|name| named.remove(&name))
        .collect())
}

pub fn executable_region_shape_lists(
    func: &hir::Function,
) -> Result<(Vec<SmolStr>, Vec<SmolStr>), &'static str> {
    let metadata = func.region.as_ref().ok_or("region metadata missing")?;
    let coarse = resolve_region_shapes_for_detail(metadata, hir::DomainGeometryDetail::Coarse)?;
    let fine = resolve_region_shapes_for_detail(metadata, hir::DomainGeometryDetail::Fine)?;
    Ok((coarse, fine))
}

pub fn build_region_exec_cases(module: &hir::Module) -> Vec<RegionExecCase> {
    module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, hir::FunctionRole::Region))
        .map(|(_, func)| {
            let shapes = executable_region_shape_lists(func)
                .map(|(coarse, fine)| RegionShapeLists { coarse, fine })
                .map_err(SmolStr::new);
            RegionExecCase {
                region_name: func.name.clone(),
                scene_id: stable_region_scene_capture_id(&func.name),
                shapes,
            }
        })
        .collect()
}

pub fn select_region_exec_case(cases: &[RegionExecCase], scene_id: u32) -> Option<&RegionExecCase> {
    cases.iter().find(|case| case.scene_id == scene_id)
}

pub fn world_domain_mismatch_message(query_name: &str) -> String {
    format!("{query_name} requires a domain derived from the same region capture")
}
