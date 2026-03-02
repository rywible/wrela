fn collect_safe_fixes(
    entry_path: &Path,
    output_format: OutputFormat,
    include_review: bool,
    diagnostic_scope: &DiagnosticScope,
) -> Result<Vec<DiagFix>, i32> {
    let project = match hir::project::load_project_with_entrypoint(entry_path, false) {
        Ok(project) => project,
        Err(errors) => {
            let mut records = Vec::new();
            let mut had_relevant_error = false;
            for err in errors {
                if !diagnostic_scope.allows_path(&err.path) {
                    continue;
                }
                let record = project_record(
                    err.kind,
                    DiagSeverity::Error,
                    err.message,
                    err.path.display().to_string(),
                    err.span,
                );
                records.push((record, err.source));
                had_relevant_error = true;
            }
            if !had_relevant_error {
                return Ok(Vec::new());
            }
            diag_emit::emit_deduped_records_with_sources(output_format, records);
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
        if !diagnostic_scope.allows_path(&warn.path) {
            continue;
        }
        let record = project_record(
            warn.kind,
            DiagSeverity::Warning,
            warn.message,
            warn.path.display().to_string(),
            warn.span,
        );
        diag_emit::emit_diag_record(output_format, &record, &warn.source);
    }

    let mut records = Vec::new();
    let mut fixes = Vec::new();
    let semantic = hir::semantic::check_module(&module);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    let hole_binding_types = hole_binding_type_lookup(&type_info);
    for err in semantic.errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        if !diagnostic_scope.allows_path_str(&path) {
            continue;
        }
        let source_for_path = source_by_path
            .get(std::path::Path::new(&path))
            .map(|s| s.as_str())
            .unwrap_or(source.as_str());
        let (semantic_data, mut semantic_fixes) =
            semantic_hole_payload_and_fixes(&err, &path, &hole_binding_types);
        attach_expected_source_for_fixes(&mut semantic_fixes, source_for_path);
        fixes.extend(semantic_fixes.clone());
        let mut record = DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        );
        if semantic_data.is_some() {
            record = record.with_data(semantic_data);
        }
        if !semantic_fixes.is_empty() {
            record = record.with_fixes(semantic_fixes);
        }
        records.push(record);
    }
    for warn in semantic.warnings {
        let path = resolve_path_from_owner_spans(warn.primary_span(), &provenance, &source_name);
        if !diagnostic_scope.allows_path_str(&path) {
            continue;
        }
        let record = DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Warning,
            &warn,
            path,
            warn.primary_span(),
        );
        records.push(record);
    }

    for err in type_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        if !diagnostic_scope.allows_path_str(&path) {
            continue;
        }
        let source_for_type = source_by_path
            .get(std::path::Path::new(&path))
            .map(|s| s.as_str())
            .unwrap_or(source.as_str());
        let (_type_data, type_fixes) = type_payload_and_fixes(&err, &path, source_for_type);
        fixes.extend(type_fixes);
        let record = DiagRecord::from_diagnostic(
            DiagStage::Type,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        );
        records.push(record);
    }

    let naming_errors = hir::naming::check_module(&module, &type_info);
    for err in naming_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        if !diagnostic_scope.allows_path_str(&path) {
            continue;
        }
        let source_for_path = source_by_path
            .get(std::path::Path::new(&path))
            .map(|s| s.as_str())
            .unwrap_or(source.as_str());
        let mut err_fixes = conservative_naming_fixes(&err, &path);
        attach_expected_source_for_fixes(&mut err_fixes, source_for_path);
        fixes.extend(err_fixes.clone());
        let severity = naming_policy_severity(&err, false);
        let record = DiagRecord::from_diagnostic(
            DiagStage::Naming,
            severity,
            &err,
            path,
            err.primary_span(),
        )
        .with_fixes(err_fixes)
        .with_data(Some(serde_json::json!({
            "kind": "naming_policy",
            "tier": naming_policy_tier(&err)
        })));
        records.push(record);
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

    let fixes = wrela::diag::fixit::normalize_and_filter_fixes(fixes)
        .into_iter()
        .filter(|fix| match fix.safety_tier.as_str() {
            "safe" => true,
            "review" => include_review,
            _ => false,
        })
        .collect::<Vec<_>>();
    Ok(attach_expected_source_fragments(fixes, &source_by_path))
}

fn attach_expected_source_fragments(
    mut fixes: Vec<DiagFix>,
    source_by_path: &HashMap<PathBuf, String>,
) -> Vec<DiagFix> {
    for fix in &mut fixes {
        if fix.expected_source.is_some() {
            continue;
        }
        let Some(source) = source_by_path.get(std::path::Path::new(&fix.span.path)) else {
            continue;
        };
        attach_expected_source_for_fix(fix, source);
    }
    fixes
}

fn attach_expected_source_for_fix(fix: &mut DiagFix, source: &str) {
    if fix.expected_source.is_some() {
        return;
    }
    let start = fix.span.offset;
    let end = start.saturating_add(fix.span.len);
    if end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return;
    }
    fix.expected_source = source.get(start..end).map(ToString::to_string);
}

fn attach_expected_source_for_fixes(fixes: &mut [DiagFix], source: &str) {
    for fix in fixes {
        attach_expected_source_for_fix(fix, source);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FixApplyReport {
    applied: usize,
    touched_files: usize,
    touched_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FixApplyError {
    message: String,
    applied: usize,
    touched_files: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct FixSummary {
    attempted: usize,
    applied: usize,
    skipped: usize,
    errors: usize,
    touched_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FixSummaryEvent {
    event: &'static str,
    summary: FixSummary,
}

fn emit_fix_summary(output_format: OutputFormat, summary: FixSummary) {
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    println!(
        "{}",
        serde_json::to_string(&FixSummaryEvent {
            event: "fix_summary",
            summary,
        })
        .unwrap_or_else(|_| "{}".to_string())
    );
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct FmtSummary {
    iterations: usize,
    attempted: usize,
    applied: usize,
    touched_files: usize,
    targets_scanned: usize,
    failed_targets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FmtSummaryEvent {
    event: &'static str,
    summary: FmtSummary,
}

fn emit_fmt_summary(output_format: OutputFormat, summary: FmtSummary) {
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    println!(
        "{}",
        serde_json::to_string(&FmtSummaryEvent {
            event: "fmt_summary",
            summary,
        })
        .unwrap_or_else(|_| "{}".to_string())
    );
}

fn run_format_loop(
    entry_path: &Path,
    output_format: OutputFormat,
    include_review: bool,
    workspace_diagnostics: bool,
) -> Result<FmtSummary, i32> {
    const MAX_PASSES: usize = 12;
    let mut summary = FmtSummary::default();
    let mut touched_paths: BTreeSet<String> = BTreeSet::new();
    let diagnostic_scope = DiagnosticScope::from_entrypoint(entry_path, workspace_diagnostics);

    for _ in 0..MAX_PASSES {
        let fixes = collect_safe_fixes(entry_path, output_format, include_review, &diagnostic_scope)?;
        if fixes.is_empty() {
            break;
        }
        summary.iterations = summary.iterations.saturating_add(1);
        summary.attempted = summary.attempted.saturating_add(fixes.len());
        for fix in &fixes {
            touched_paths.insert(fix.span.path.clone());
        }
        match apply_source_fixes(&fixes) {
            Ok(report) => {
                summary.applied = summary.applied.saturating_add(report.applied);
                for path in &report.touched_paths {
                    if let Err(err) = normalize_formatted_wr_file(path) {
                        eprintln!("fmt normalize error: {err}");
                        return Err(EXIT_CODEGEN);
                    }
                }
                if report.applied == 0 {
                    break;
                }
            }
            Err(err) => {
                eprintln!("fmt apply error: {}", err.message);
                return Err(EXIT_CODEGEN);
            }
        }
    }

    summary.touched_files = touched_paths.len();
    Ok(summary)
}

fn resolve_format_targets(path_arg: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let input = path_arg.unwrap_or(".");
    let raw = PathBuf::from(input);
    if raw.is_dir() {
        let mut targets = collect_wr_files_for_format(&raw)?;
        if targets.is_empty() {
            return Err(format!("no .wr files found under {}", raw.display()));
        }
        targets.sort();
        targets.dedup();
        return Ok(targets);
    }
    let entry = resolve_entry_path(Some(input))?;
    Ok(vec![entry])
}

fn collect_wr_files_for_format(root: &Path) -> Result<Vec<PathBuf>, String> {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    let language_dir = root.join("language");
    let apps_dir = root.join("apps");
    let is_workspace_repo = root.join("compiler").is_dir() && root.join("runtime").is_dir();
    let mut scan_roots = Vec::new();
    if src_dir.is_dir() {
        scan_roots.push(src_dir);
    }
    if tests_dir.is_dir() {
        scan_roots.push(tests_dir);
    }
    if is_workspace_repo {
        if language_dir.is_dir() {
            scan_roots.push(language_dir);
        }
        if apps_dir.is_dir() {
            scan_roots.push(apps_dir);
        }
    }
    let mut files = Vec::new();
    for dir in &scan_roots {
        collect_wr_files_recursive(dir, &mut files)?;
    }
    if files.is_empty() {
        collect_wr_files_recursive(root, &mut files)?;
    }
    Ok(files)
}

fn collect_wr_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?;
    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if matches!(name, ".git" | "target" | "node_modules") {
                continue;
            }
            collect_wr_files_recursive(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("wr") {
            out.push(path);
        }
    }
    Ok(())
}

fn apply_source_fixes(fixes: &[DiagFix]) -> Result<FixApplyReport, FixApplyError> {
    let mut fixes_by_path: BTreeMap<&str, Vec<&DiagFix>> = BTreeMap::new();
    for fix in fixes {
        fixes_by_path
            .entry(fix.span.path.as_str())
            .or_default()
            .push(fix);
    }

    let mut report = FixApplyReport::default();
    for (path, path_fixes) in fixes_by_path {
        let mut sorted = path_fixes;
        sorted.sort_by_key(|fix| (fix.span.offset, fix.span.len));
        for pair in sorted.windows(2) {
            let prev = pair[0];
            let next = pair[1];
            let prev_end = prev.span.offset.saturating_add(prev.span.len);
            if next.span.offset < prev_end {
                return Err(FixApplyError {
                    message: format!(
                        "overlapping fixes in {} at {} and {}",
                        path, prev.span.offset, next.span.offset
                    ),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            }
        }

        let mut source = fs::read_to_string(path).map_err(|err| FixApplyError {
            message: format!("failed to read source file {path}: {err}"),
            applied: report.applied,
            touched_files: report.touched_files,
        })?;
        for fix in sorted.iter().rev() {
            let start = fix.span.offset;
            let end = fix.span.offset.saturating_add(fix.span.len);
            if end > source.len() {
                return Err(FixApplyError {
                    message: format!("fix range out of bounds in {} at {}..{}", path, start, end),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            }
            if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
                return Err(FixApplyError {
                    message: format!(
                        "fix range is not on UTF-8 boundaries in {} at {}..{}",
                        path, start, end
                    ),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            }
            let Some(expected_source) = &fix.expected_source else {
                return Err(FixApplyError {
                    message: format!(
                        "missing expected source for fix in {} at {}..{}",
                        path, start, end
                    ),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            };
            let Some(current_source) = source.get(start..end) else {
                return Err(FixApplyError {
                    message: format!(
                        "fix range is not valid UTF-8 slice in {} at {}..{}",
                        path, start, end
                    ),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            };
            if current_source != expected_source {
                return Err(FixApplyError {
                    message: format!(
                        "source span mismatch in {} at {}..{} (expected {:?}, found {:?})",
                        path, start, end, expected_source, current_source
                    ),
                    applied: report.applied,
                    touched_files: report.touched_files,
                });
            }
            source.replace_range(start..end, &fix.replacement);
            report.applied += 1;
        }
        fs::write(path, source).map_err(|err| FixApplyError {
            message: format!("failed to write source file {path}: {err}"),
            applied: report.applied,
            touched_files: report.touched_files,
        })?;
        report.touched_files += 1;
        report.touched_paths.push(path.to_string());
    }
    Ok(report)
}

fn normalize_formatted_wr_file(path: &str) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))?;
    let normalized = normalize_wr_blank_lines(&source);
    if normalized != source {
        fs::write(path, normalized).map_err(|err| format!("failed to write {path}: {err}"))?;
    }
    Ok(())
}

fn normalize_wr_blank_lines(source: &str) -> String {
    let ends_with_newline = source.ends_with('\n');
    let mut out = String::with_capacity(source.len());
    let mut blank_run = 0usize;
    for line in source.lines() {
        if line.trim().is_empty() {
            blank_run = blank_run.saturating_add(1);
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !ends_with_newline && out.ends_with('\n') {
        out.pop();
    }
    out
}

