use super::build_compile::{
    conservative_naming_fixes, project_record, resolve_path_from_owner_spans,
};
use super::fix_fmt::attach_expected_source_for_fixes;
use super::shared::{naming_policy_severity, naming_policy_tier};
use super::{
    AstNode, BTreeMap, BTreeSet, Command, CommandSpec, Deserialize, DiagFix, DiagRecord,
    DiagSeverity, DiagSpan, DiagStage, Duration, EXIT_CODEGEN, EXIT_OK, EXIT_PARSE,
    EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, HashMap, HashSet, Instant, Output, OutputFormat,
    ParsedCommandSpec, Path, PathBuf, Serialize, SmolStr, SourceSpan, SystemTime, UNIX_EPOCH,
    VecDeque, ast, cert_engine, dedupe_records, diag_emit, env, fs, hir, hir_lower, io, mir,
    mir_descriptor, parser, perf_engine, project_descriptor, replay_trace, suppress_cascades,
};

pub(crate) fn resolve_entry_path(path_arg: Option<&str>) -> Result<PathBuf, String> {
    let path = match path_arg {
        Some(path) => PathBuf::from(path),
        None => return Err("missing input path".to_string()),
    };
    if path.is_dir() {
        let entry = path.join("src").join("main.wr");
        if entry.exists() {
            return Ok(entry);
        }
        return Err(format!("no entry file found at {}", entry.display()));
    }
    if !path.exists() {
        return Err(format!("path not found: {}", path.display()));
    }
    Ok(path)
}

pub(crate) fn project_root_for_entry(entry_path: &Path) -> PathBuf {
    for ancestor in entry_path.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == "src")
            && let Some(parent) = ancestor.parent()
        {
            return parent.to_path_buf();
        }
    }
    entry_path.parent().unwrap_or(entry_path).to_path_buf()
}

pub(crate) fn integration_mode_entry_path_is_allowed(entry_path: &Path) -> bool {
    let workspace_root = project_root_for_entry(entry_path);
    let src_root = workspace_root.join("src");
    let relative = match entry_path.strip_prefix(&src_root) {
        Ok(path) => path,
        Err(_) => return false,
    };
    let relative_text = relative.to_string_lossy().replace('\\', "/");
    relative_text.starts_with("application/composition/")
        || relative_text.starts_with("infrastructure/integrations/")
}

pub(crate) fn compile_to_mir(
    entry_path: &Path,
    output_format: OutputFormat,
    emit_mir: bool,
    emit_mir_opt: bool,
    require_entrypoint: bool,
    enforce_naming: bool,
    strict_naming: bool,
    holes_only: bool,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<mir::ir::MirModule, i32> {
    let trace = std::env::var("WRELA_BUILD_TRACE").is_ok();
    let stage = |name: &str, start: &Instant| {
        if trace {
            eprintln!("build: {} ({:.2?})", name, start.elapsed());
        }
    };
    let start = Instant::now();
    if trace {
        eprintln!("build: start {:?}", entry_path);
    }
    let project = match hir::project::load_project_with_entrypoint(entry_path, require_entrypoint) {
        Ok(project) => project,
        Err(errors) => {
            let mut missing_run = false;
            let mut records = Vec::new();
            for err in errors {
                if err.message.contains("define 'fn run()'") {
                    missing_run = true;
                }
                let record = project_record(
                    err.kind,
                    DiagSeverity::Error,
                    err.message,
                    err.path.display().to_string(),
                    err.span,
                );
                records.push((record, err.source));
            }
            diag_emit::emit_deduped_records_with_sources(output_format, records);
            if missing_run && require_entrypoint && matches!(output_format, OutputFormat::Pretty) {
                eprintln!(
                    "note: add `fn run()` in your entry file to define the program entrypoint"
                );
            }
            return Err(EXIT_PARSE);
        }
    };
    let module = project.module.clone();
    let source = project.entry_source.clone();
    let source_name = entry_path.display().to_string();
    let mut source_by_path = project.module_sources.clone();
    let provenance = project.provenance.clone();
    source_by_path
        .entry(entry_path.to_path_buf())
        .or_insert_with(|| source.clone());
    for warn in project.warnings {
        let record = project_record(
            warn.kind,
            DiagSeverity::Warning,
            warn.message,
            warn.path.display().to_string(),
            warn.span,
        );
        diag_emit::emit_diag_record(output_format, &record, &warn.source);
    }
    stage("load_project", &start);

    let mut had_errors = false;
    let semantic = hir::semantic::check_module(&module);
    stage("semantic", &start);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    stage("typeck", &start);
    let hole_binding_types = hole_binding_type_lookup(&type_info);
    let mut records = Vec::new();
    for err in semantic.errors {
        let is_hole = matches!(err, hir::semantic::SemanticError::TypedHole { .. });
        if holes_only && !is_hole {
            continue;
        }
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        let source_for_path = source_by_path
            .get(std::path::Path::new(&path))
            .map(|s| s.as_str())
            .unwrap_or(source.as_str());
        let (semantic_data, mut semantic_fixes) =
            semantic_hole_payload_and_fixes(&err, &path, &hole_binding_types);
        attach_expected_source_for_fixes(&mut semantic_fixes, source_for_path);
        let mut record = DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Error,
            &err,
            path.clone(),
            err.primary_span(),
        );
        if semantic_data.is_some() {
            record = record.with_data(semantic_data);
        }
        if !semantic_fixes.is_empty() {
            record = record.with_fixes(semantic_fixes);
        }
        records.push(record);
        had_errors = true;
    }
    for warn in semantic.warnings {
        if holes_only {
            continue;
        }
        let path = resolve_path_from_owner_spans(warn.primary_span(), &provenance, &source_name);
        let record = DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Warning,
            &warn,
            path,
            warn.primary_span(),
        );
        records.push(record);
    }
    if holes_only {
        for record in suppress_cascades(dedupe_records(records)) {
            let source_for_record = source_by_path
                .get(std::path::Path::new(
                    &record
                        .labels
                        .first()
                        .map(|label| label.span.path.clone())
                        .unwrap_or_else(|| source_name.clone()),
                ))
                .cloned()
                .unwrap_or_else(|| source.clone());
            diag_emit::emit_diag_record(output_format, &record, &source_for_record);
        }
        if had_errors {
            return Err(EXIT_TYPE);
        }
        return Ok(mir::lower::lower_module_with_types_and_backend(
            &module,
            &type_info,
            query_backend,
        ));
    }

    for err in type_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        let source_for_type = source_by_path
            .get(std::path::Path::new(&path))
            .map(|s| s.as_str())
            .unwrap_or(source.as_str());
        let (type_data, mut type_fixes) = type_payload_and_fixes(&err, &path, source_for_type);
        attach_expected_source_for_fixes(&mut type_fixes, source_for_type);
        let mut record = DiagRecord::from_diagnostic(
            DiagStage::Type,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        );
        if type_data.is_some() {
            record = record.with_data(type_data);
        }
        if !type_fixes.is_empty() {
            record = record.with_fixes(type_fixes);
        }
        records.push(record);
        had_errors = true;
    }

    if enforce_naming {
        let naming_errors = hir::naming::check_module(&module, &type_info);
        stage("naming", &start);
        for err in naming_errors {
            let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
            let source_for_path = source_by_path
                .get(std::path::Path::new(&path))
                .map(|s| s.as_str())
                .unwrap_or(source.as_str());
            let mut fixes = conservative_naming_fixes(&err, &path);
            attach_expected_source_for_fixes(&mut fixes, source_for_path);
            let severity = naming_policy_severity(&err, strict_naming);
            let record = DiagRecord::from_diagnostic(
                DiagStage::Naming,
                severity,
                &err,
                path,
                err.primary_span(),
            )
            .with_fixes(fixes)
            .with_data(Some(serde_json::json!({
                "kind": "naming_policy",
                "tier": naming_policy_tier(&err)
            })));
            records.push(record);
            if matches!(severity, DiagSeverity::Error) {
                had_errors = true;
            }
        }
    }
    for record in suppress_cascades(dedupe_records(records)) {
        let source_for_record = source_by_path
            .get(std::path::Path::new(
                &record
                    .labels
                    .first()
                    .map(|label| label.span.path.clone())
                    .unwrap_or_else(|| source_name.clone()),
            ))
            .cloned()
            .unwrap_or_else(|| source.clone());
        diag_emit::emit_diag_record(output_format, &record, &source_for_record);
    }

    if had_errors {
        return Err(EXIT_TYPE);
    }

    let check_ir = hir::checkir::extract_module(&module);
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        let vector_eligible = check_ir
            .checks
            .iter()
            .filter(|check| check.supports_vector_lane)
            .count();
        eprintln!(
            "check-oracle: extracted={} skipped={} vector_eligible={}",
            check_ir.checks.len(),
            check_ir.skipped.len(),
            vector_eligible
        );
        for check in &check_ir.checks {
            eprintln!(
                "check-oracle-shape: name={} shape_id={} vector_lane={}",
                check.name, check.shape_id, check.supports_vector_lane
            );
        }
    }

    let mut mir_module =
        mir::lower::lower_module_with_types_and_backend(&module, &type_info, query_backend);
    stage("mir_lower", &start);
    if emit_mir {
        println!("{:#?}", mir_module);
    }
    let analysis = mir::analysis::analyze_module(&mir_module);
    let _ = mir::opt::inline_small_pure_functions(&mut mir_module, &analysis.call_graph);
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let rewrite_report =
        mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        eprintln!(
            "rewrite: mined={} admitted={} applied={} steps={} exhausted={}",
            rewrite_report.mined,
            rewrite_report.admitted,
            rewrite_report.applied,
            rewrite_report.steps,
            rewrite_report.budget_exhausted
        );
    }
    stage("mir_opt", &start);
    if emit_mir_opt {
        println!("{:#?}", mir_module);
    }
    for err in mir::validate::validate_module(&mir_module) {
        let record = DiagRecord::new(
            DiagStage::Mir,
            DiagSeverity::Error,
            err.message,
            source_name.clone(),
            SourceSpan::from((0usize, 0usize)),
        );
        diag_emit::emit_diag_record(output_format, &record, &source);
        had_errors = true;
    }

    if had_errors {
        return Err(EXIT_CODEGEN);
    }

    Ok(mir_module)
}

pub(crate) fn hole_binding_type_lookup(
    type_info: &hir::typeck::TypeInfo,
) -> HashMap<String, String> {
    let mut by_name = BTreeMap::<String, BTreeSet<String>>::new();
    for info in type_info.functions.values() {
        for (name, ty) in &info.local_types {
            by_name
                .entry(name.to_string())
                .or_default()
                .insert(format!("{ty:?}"));
        }
    }
    by_name
        .into_iter()
        .map(|(name, variants)| {
            let ty = variants
                .into_iter()
                .next()
                .unwrap_or_else(|| "Unknown".to_string());
            (name, ty)
        })
        .collect()
}

pub(crate) fn semantic_hole_payload_and_fixes(
    err: &hir::semantic::SemanticError,
    path: &str,
    binding_types: &HashMap<String, String>,
) -> (Option<serde_json::Value>, Vec<DiagFix>) {
    match err {
        hir::semantic::SemanticError::TypedHole {
            name,
            candidates,
            span,
        } => {
            let in_scope_bindings = candidates
                .iter()
                .map(|candidate| {
                    let candidate_name = candidate.to_string();
                    let candidate_type = binding_types
                        .get(candidate_name.as_str())
                        .cloned()
                        .unwrap_or_else(|| "Unknown".to_string());
                    serde_json::json!({
                        "name": candidate_name,
                        "type": candidate_type,
                    })
                })
                .collect::<Vec<_>>();
            let candidate_values = candidates
                .iter()
                .map(|candidate| serde_json::Value::String(candidate.to_string()))
                .collect::<Vec<_>>();
            let mut concrete_expected = BTreeSet::new();
            for binding in &in_scope_bindings {
                let Some(ty) = binding.get("type").and_then(|value| value.as_str()) else {
                    continue;
                };
                if ty != "Unknown" {
                    concrete_expected.insert(ty.to_string());
                }
            }
            let expected_type = if concrete_expected.len() == 1 {
                concrete_expected
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "Unknown".to_string())
            } else {
                "Unknown".to_string()
            };
            let fixes = pick_primary_hole_candidate(candidates)
                .into_iter()
                .map(|candidate| {
                    let candidate_name = candidate.as_str();
                    let preferred_binding = candidate_name
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_lowercase())
                        && candidate_name != "run"
                        && candidate_name != "main";
                    let (confidence, safety_tier, reason_code) = if preferred_binding {
                        (
                            0.97,
                            "safe".to_string(),
                            "typed_hole_primary_binding".to_string(),
                        )
                    } else {
                        (
                            0.9,
                            "review".to_string(),
                            "typed_hole_candidate".to_string(),
                        )
                    };
                    let padding = " ".repeat(span.len().saturating_sub(name.len()));
                    DiagFix {
                        replacement: format!("{padding}{candidate}"),
                        span: DiagSpan {
                            path: path.to_string(),
                            offset: span.offset(),
                            len: span.len(),
                        },
                        expected_source: None,
                        rationale: format!(
                            "Fill hole `{}` with in-scope binding `{}`.",
                            name, candidate
                        ),
                        confidence,
                        safety_tier,
                        reason_code,
                    }
                })
                .collect::<Vec<_>>();
            let hole_id = format!("{}:{}:{}:{}", path, span.offset(), span.len(), name);
            let ranked_candidates = in_scope_bindings
                .iter()
                .enumerate()
                .map(|(index, binding)| {
                    serde_json::json!({
                        "rank": index + 1,
                        "name": binding.get("name").cloned().unwrap_or(serde_json::Value::String("".to_string())),
                        "type": binding.get("type").cloned().unwrap_or(serde_json::Value::String("Unknown".to_string())),
                        "strategy": "lexicographic_binding_name"
                    })
                })
                .collect::<Vec<_>>();
            let code_actions = fixes
                .iter()
                .map(|fix| {
                    let applicability = if fix.safety_tier == "safe" {
                        "machine_applicable"
                    } else {
                        "maybe_correct"
                    };
                    serde_json::json!({
                        "kind": "fill_typed_hole",
                        "title": fix.rationale,
                        "replacement": fix.replacement.trim().to_string(),
                        "reason_code": fix.reason_code,
                        "applicability": applicability
                    })
                })
                .collect::<Vec<_>>();
            let payload = serde_json::json!({
                "kind": "typed_hole",
                "hole_name": name.to_string(),
                "hole_id": hole_id,
                "candidate_bindings": candidate_values,
                "expected_type": expected_type,
                "in_scope_bindings": in_scope_bindings,
                "ranking_strategy": "lexicographic_binding_name",
                "ranked_candidates": ranked_candidates,
                "code_actions": code_actions,
            });
            (Some(payload), fixes)
        }
        _ => (None, Vec::new()),
    }
}

pub(crate) fn type_payload_and_fixes(
    err: &hir::typeck::TypeError,
    path: &str,
    source_text: &str,
) -> (Option<serde_json::Value>, Vec<DiagFix>) {
    match err {
        hir::typeck::TypeError::NamedArgsRequired {
            span,
            param_names,
            arg_spans,
        } => {
            let payload = serde_json::json!({
                "kind": "named_args_required",
                "param_names": param_names.iter().map(|name| name.to_string()).collect::<Vec<_>>(),
                "hint": "Calls with multiple parameters should use named arguments."
            });
            let replacement =
                build_named_args_replacement(source_text, *span, param_names, arg_spans);
            let expected_source = source_text
                .get(span.offset()..span.offset().saturating_add(span.len()))
                .map(|text| text.to_string());
            let fixes = replacement
                .into_iter()
                .map(|(replacement, is_safe)| DiagFix {
                    replacement,
                    span: DiagSpan {
                        path: path.to_string(),
                        offset: span.offset(),
                        len: span.len(),
                    },
                    expected_source: expected_source.clone(),
                    rationale: "Rewrite positional arguments as named arguments.".to_string(),
                    confidence: if is_safe { 0.97 } else { 0.86 },
                    safety_tier: if is_safe {
                        "safe".to_string()
                    } else {
                        "review".to_string()
                    },
                    reason_code: "named_args_rewrite".to_string(),
                })
                .collect::<Vec<_>>();
            (Some(payload), fixes)
        }
        hir::typeck::TypeError::TryOutsideResult { span } => {
            let payload = serde_json::json!({
                "kind": "try_outside_result",
                "hint": "Use `?` only inside functions that return Result[...] or remove `?`."
            });
            let fixes = vec![DiagFix {
                replacement: String::new(),
                span: DiagSpan {
                    path: path.to_string(),
                    offset: span.offset(),
                    len: span.len(),
                },
                expected_source: None,
                rationale: "Remove `?` and handle the value/error explicitly in this function."
                    .to_string(),
                confidence: 0.7,
                safety_tier: "review".to_string(),
                reason_code: "remove_try_operator".to_string(),
            }];
            (Some(payload), fixes)
        }
        hir::typeck::TypeError::InvalidTryOperand { span } => {
            let payload = serde_json::json!({
                "kind": "invalid_try_operand",
                "hint": "`?` expects a Result on the left side."
            });
            let fixes = vec![DiagFix {
                replacement: String::new(),
                span: DiagSpan {
                    path: path.to_string(),
                    offset: span.offset(),
                    len: span.len(),
                },
                expected_source: None,
                rationale: "Remove `?` because this expression is not a Result value.".to_string(),
                confidence: 0.97,
                safety_tier: "safe".to_string(),
                reason_code: "remove_try_operator".to_string(),
            }];
            (Some(payload), fixes)
        }
        hir::typeck::TypeError::BoundaryMissingTypeArgs { name, span } => {
            let suggested = boundary_generic_replacement(name.as_str());
            let payload = serde_json::json!({
                "kind": "boundary_missing_type_args",
                "type_name": name.to_string(),
                "hint": "Declare explicit type arguments at module boundaries.",
                "suggested_replacement": suggested,
            });
            let span_start = span.offset();
            let span_end = span.offset().saturating_add(span.len());
            let span_matches_type_name = source_text
                .get(span_start..span_end)
                .is_some_and(|text| text == name.as_str());
            if !span_matches_type_name {
                return (Some(payload), Vec::new());
            }
            let fixes = suggested
                .into_iter()
                .map(|replacement| DiagFix {
                    replacement: replacement.to_string(),
                    span: DiagSpan {
                        path: path.to_string(),
                        offset: span.offset(),
                        len: span.len(),
                    },
                    expected_source: None,
                    rationale: format!(
                        "Add explicit type arguments for boundary generic `{}`.",
                        name
                    ),
                    confidence: 0.82,
                    safety_tier: "review".to_string(),
                    reason_code: "boundary_generic_rewrite".to_string(),
                })
                .collect::<Vec<_>>();
            (Some(payload), fixes)
        }
        hir::typeck::TypeError::EqualityRequiresEq { left, right, .. } => {
            let payload = serde_json::json!({
                "kind": "equality_requires_eq",
                "left_type": left,
                "right_type": right,
                "hint": "Use structurally comparable types; Actor and Pending values are not comparable."
            });
            (Some(payload), Vec::new())
        }
        _ => (None, Vec::new()),
    }
}

pub(crate) fn boundary_generic_replacement(name: &str) -> Option<&'static str> {
    match name {
        "List" => Some("List[Integer]"),
        "Map" => Some("Map[String, Integer]"),
        "Result" => Some("Result[Integer, RuntimeError]"),
        "Actor" => Some("Actor[Worker]"),
        "Pending" => Some("Pending[Integer]"),
        _ => None,
    }
}

pub(crate) fn build_named_args_replacement(
    source_text: &str,
    call_span: SourceSpan,
    param_names: &[smol_str::SmolStr],
    arg_spans: &[SourceSpan],
) -> Option<(String, bool)> {
    if param_names.is_empty() || arg_spans.is_empty() || param_names.len() != arg_spans.len() {
        return None;
    }
    let call_start = call_span.offset();
    let call_end = call_span.offset().saturating_add(call_span.len());
    let Some(call_text) = source_text.get(call_start..call_end) else {
        return None;
    };
    let open_idx = call_text.find('(')?;
    let close_idx = call_text.rfind(')')?;
    if open_idx >= close_idx {
        return None;
    }
    let call_prefix = call_text.get(..=open_idx)?;
    let call_suffix = call_text.get(close_idx..)?;

    let mut all_positional = true;
    let mut named_args = Vec::with_capacity(param_names.len());
    let mut cursor = call_start;
    for (index, (name, span)) in param_names.iter().zip(arg_spans.iter()).enumerate() {
        let start = span.offset();
        let end = span.offset().saturating_add(span.len());
        if start < call_start || end > call_end || start < cursor {
            return None;
        }
        let Some(prefix) = source_text.get(cursor..start) else {
            return None;
        };
        if index == 0 {
            if !prefix.trim_end().ends_with('(') {
                return None;
            }
        } else if !prefix.contains(',') {
            return None;
        }
        let Some(raw) = source_text.get(start..end) else {
            return None;
        };
        let value = raw.trim();
        if value.is_empty() {
            return None;
        }
        if value.starts_with('(')
            || value.ends_with(')')
            || value.starts_with(',')
            || value.ends_with(',')
        {
            return None;
        }
        let named_prefix = format!("{name}=");
        if value.starts_with(&named_prefix) {
            all_positional = false;
            return None;
        }
        named_args.push(format!("{name}={value}"));
        cursor = end;
    }
    let Some(suffix) = source_text.get(cursor..call_end) else {
        return None;
    };
    if !suffix.trim_start().starts_with(')') {
        return None;
    }
    let rewritten = format!("{call_prefix}{}{call_suffix}", named_args.join(", "));
    Some((rewritten, all_positional))
}

pub(crate) fn pick_primary_hole_candidate(
    candidates: &[smol_str::SmolStr],
) -> Option<&smol_str::SmolStr> {
    candidates.iter().max_by_key(|candidate| {
        let name = candidate.as_str();
        let first = name.chars().next();
        let mut score = 0i32;
        if first.is_some_and(|ch| ch.is_ascii_lowercase()) {
            score += 3;
        }
        if first.is_some_and(|ch| ch.is_ascii_uppercase()) {
            score -= 3;
        }
        if name == "run" || name == "main" {
            score -= 2;
        }
        score += (name.len().min(32)) as i32;
        score
    })
}

pub(crate) fn temp_exe_path() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    let name = format!("wrela_run_{}_{}", std::process::id(), nanos);
    env::temp_dir().join(name).to_string_lossy().to_string()
}
