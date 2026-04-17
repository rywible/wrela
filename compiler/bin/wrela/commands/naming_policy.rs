use super::*;

pub(crate) fn naming_policy_tier(error: &hir::naming::NamingError) -> &'static str {
    match error {
        hir::naming::NamingError::ResultPrefixRequired { .. }
        | hir::naming::NamingError::FactoryPrefixRequired { .. }
        | hir::naming::NamingError::ResultErrorTypeShape { .. }
        | hir::naming::NamingError::TopLevelCheckName { .. }
        | hir::naming::NamingError::MemberCheckPrefix { .. } => "strong",
        hir::naming::NamingError::SnakeCaseRequired { .. }
        | hir::naming::NamingError::PascalCaseRequired { .. }
        | hir::naming::NamingError::VerbLedRequired { .. }
        | hir::naming::NamingError::NounOnlyRequired { .. }
        | hir::naming::NamingError::BooleanPrefixRequired { .. }
        | hir::naming::NamingError::InlineCheckCondition { .. }
        | hir::naming::NamingError::ModuleSemanticRequired { .. }
        | hir::naming::NamingError::CollectionPluralityRequired { .. } => "style",
    }
}

pub(crate) fn naming_policy_severity(
    error: &hir::naming::NamingError,
    strict_naming: bool,
) -> DiagSeverity {
    let tier = naming_policy_tier(error);
    if strict_naming && (tier == "strong" || tier == "style") {
        DiagSeverity::Error
    } else {
        DiagSeverity::Warning
    }
}

pub(crate) fn project_naming_diagnostics(
    project: &hir::project::LoadedProject,
) -> Vec<(PathBuf, String, hir::naming::NamingError)> {
    let mut diagnostics = Vec::new();
    for source_module in &project.source_modules {
        let (_type_errors, type_info) = hir::typeck::check_module_with_info(&source_module.module);
        for err in hir::naming::check_module(&source_module.module, &type_info) {
            diagnostics.push((
                source_module.path.clone(),
                source_module.source.clone(),
                err,
            ));
        }
    }
    diagnostics
}
