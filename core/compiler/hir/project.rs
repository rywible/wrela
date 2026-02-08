use crate::hir::arena::Idx;
use crate::hir::lower::{lower, lower_root_body};
use crate::hir::{Body, Function, FunctionKind, Module, Stmt, UseName, UseNameKind, Visibility};
use crate::parser;
use crate::parser::ast::AstNode;
use miette::SourceSpan;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct LoadedProject {
    pub module: Module,
    pub entry_source: String,
    pub warnings: Vec<ProjectWarning>,
}

#[derive(Debug, Clone)]
pub struct ProjectError {
    pub path: PathBuf,
    pub source: String,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ProjectWarning {
    pub path: PathBuf,
    pub source: String,
    pub message: String,
    pub span: SourceSpan,
}

struct LoadedModule {
    name: SmolStr,
    path: PathBuf,
    source: String,
    module: Module,
    uses: Vec<UseSite>,
    root_body: Option<Body>,
}

#[derive(Clone)]
struct UseSite {
    module: SmolStr,
    names: Vec<UseName>,
    span: TextRange,
    module_span: Option<TextRange>,
}

#[derive(Copy, Clone)]
enum DefinitionKind {
    Function,
    Class,
}

struct ProjectLoader {
    root_dir: PathBuf,
    #[allow(dead_code)]
    tests_dir: Option<PathBuf>,
    modules: HashMap<SmolStr, LoadedModule>,
    errors: Vec<ProjectError>,
    warnings: Vec<ProjectWarning>,
}

fn build_trace() -> bool {
    std::env::var("WRELA_BUILD_TRACE").is_ok()
}

pub fn load_project(entry_path: &Path) -> Result<LoadedProject, Vec<ProjectError>> {
    load_project_with_entrypoint(entry_path, true)
}

pub fn load_project_with_entrypoint(
    entry_path: &Path,
    enforce_entrypoint: bool,
) -> Result<LoadedProject, Vec<ProjectError>> {
    if build_trace() {
        eprintln!("project: load_project {}", entry_path.display());
    }
    let root_dir = match find_src_root(entry_path) {
        Some(dir) => dir,
        None => match entry_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => {
                return Err(vec![ProjectError {
                    path: entry_path.to_path_buf(),
                    source: String::new(),
                    message: "entry file must have a parent directory".to_string(),
                    span: SourceSpan::from((0usize, 0usize)),
                }]);
            }
        },
    };
    load_project_with_roots(entry_path, &root_dir, None, enforce_entrypoint)
}

pub fn load_project_with_root(
    entry_path: &Path,
    root_dir: &Path,
) -> Result<LoadedProject, Vec<ProjectError>> {
    load_project_with_roots(entry_path, root_dir, None, true)
}

pub fn load_project_with_roots(
    entry_path: &Path,
    root_dir: &Path,
    tests_dir: Option<PathBuf>,
    enforce_entrypoint: bool,
) -> Result<LoadedProject, Vec<ProjectError>> {
    let mut loader = ProjectLoader {
        root_dir: root_dir.to_path_buf(),
        tests_dir,
        modules: HashMap::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    let entry_name = module_name_for_entry_path(entry_path, &loader.root_dir);

    loader.load_module(entry_name.clone(), entry_path.to_path_buf());
    if build_trace() {
        eprintln!("project: load_module done");
    }
    if enforce_entrypoint {
        loader.enforce_entrypoint(&entry_name);
    }
    loader.validate_uses();
    loader.detect_cycles();
    loader.analyze_imports();
    if !loader.errors.is_empty() {
        return Err(loader.errors);
    }

    let mut merged = Module {
        functions: Default::default(),
        classes: Default::default(),
        enums: Default::default(),
        interfaces: Default::default(),
        uses: Vec::new(),
    };

    let mut function_origins: HashMap<SmolStr, (SmolStr, Option<TextRange>, PathBuf, String)> =
        HashMap::new();
    let mut class_origins: HashMap<SmolStr, (SmolStr, Option<TextRange>, PathBuf, String)> =
        HashMap::new();
    let mut enum_origins: HashMap<SmolStr, (SmolStr, Option<TextRange>, PathBuf, String)> =
        HashMap::new();
    let mut interface_origins: HashMap<SmolStr, (SmolStr, Option<TextRange>, PathBuf, String)> =
        HashMap::new();

    for module in loader.modules.values() {
        let mut method_ids = HashSet::new();
        for (_, class) in module.module.classes.iter() {
            for method in &class.methods {
                method_ids.insert(*method);
            }
        }
        for (func_id, func) in module.module.functions.iter() {
            if method_ids.contains(&func_id) {
                continue;
            }
            if let Some((prev_mod, prev_span, prev_path, prev_src)) =
                function_origins.get(&func.name)
            {
                loader.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: format!(
                        "duplicate function '{}' (already defined in module '{}')",
                        func.name, prev_mod
                    ),
                    span: span_from_range(
                        func.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
                loader.errors.push(ProjectError {
                    path: prev_path.clone(),
                    source: prev_src.clone(),
                    message: format!(
                        "previous definition of '{}' in module '{}'",
                        func.name, prev_mod
                    ),
                    span: span_from_range(prev_span.unwrap_or_else(|| TextRange::empty(0.into()))),
                });
            } else {
                function_origins.insert(
                    func.name.clone(),
                    (
                        module.name.clone(),
                        func.name_span,
                        module.path.clone(),
                        module.source.clone(),
                    ),
                );
            }
        }
        for (_, class) in module.module.classes.iter() {
            if let Some((prev_mod, prev_span, prev_path, prev_src)) = class_origins.get(&class.name)
            {
                loader.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: format!(
                        "duplicate class '{}' (already defined in module '{}')",
                        class.name, prev_mod
                    ),
                    span: span_from_range(
                        class
                            .name_span
                            .unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
                loader.errors.push(ProjectError {
                    path: prev_path.clone(),
                    source: prev_src.clone(),
                    message: format!(
                        "previous definition of '{}' in module '{}'",
                        class.name, prev_mod
                    ),
                    span: span_from_range(prev_span.unwrap_or_else(|| TextRange::empty(0.into()))),
                });
            } else {
                class_origins.insert(
                    class.name.clone(),
                    (
                        module.name.clone(),
                        class.name_span,
                        module.path.clone(),
                        module.source.clone(),
                    ),
                );
            }
        }
        for (_, en) in module.module.enums.iter() {
            if let Some((prev_mod, prev_span, prev_path, prev_src)) = enum_origins.get(&en.name) {
                loader.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: format!(
                        "duplicate enum '{}' (already defined in module '{}')",
                        en.name, prev_mod
                    ),
                    span: span_from_range(
                        en.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
                loader.errors.push(ProjectError {
                    path: prev_path.clone(),
                    source: prev_src.clone(),
                    message: format!(
                        "previous definition of '{}' in module '{}'",
                        en.name, prev_mod
                    ),
                    span: span_from_range(prev_span.unwrap_or_else(|| TextRange::empty(0.into()))),
                });
            } else {
                enum_origins.insert(
                    en.name.clone(),
                    (
                        module.name.clone(),
                        en.name_span,
                        module.path.clone(),
                        module.source.clone(),
                    ),
                );
            }
        }
        for (_, interface) in module.module.interfaces.iter() {
            if let Some((prev_mod, prev_span, prev_path, prev_src)) =
                interface_origins.get(&interface.name)
            {
                loader.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: format!(
                        "duplicate interface '{}' (already defined in module '{}')",
                        interface.name, prev_mod
                    ),
                    span: span_from_range(
                        interface
                            .name_span
                            .unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
                loader.errors.push(ProjectError {
                    path: prev_path.clone(),
                    source: prev_src.clone(),
                    message: format!(
                        "previous definition of '{}' in module '{}'",
                        interface.name, prev_mod
                    ),
                    span: span_from_range(prev_span.unwrap_or_else(|| TextRange::empty(0.into()))),
                });
            } else {
                interface_origins.insert(
                    interface.name.clone(),
                    (
                        module.name.clone(),
                        interface.name_span,
                        module.path.clone(),
                        module.source.clone(),
                    ),
                );
            }
        }
    }

    if !loader.errors.is_empty() {
        return Err(loader.errors);
    }

    for module in loader.modules.values() {
        let mut func_map = HashMap::new();
        for (idx, func) in module.module.functions.iter() {
            let new_idx = merged.functions.alloc(func.clone());
            func_map.insert(idx, new_idx);
        }
        for (_, class) in module.module.classes.iter() {
            let mut new_class = class.clone();
            new_class.methods = new_class
                .methods
                .iter()
                .map(|idx| *func_map.get(idx).expect("missing function mapping"))
                .collect();
            merged.classes.alloc(new_class);
        }
        for (_, en) in module.module.enums.iter() {
            merged.enums.alloc(en.clone());
        }
        for (_, interface) in module.module.interfaces.iter() {
            merged.interfaces.alloc(interface.clone());
        }
    }

    let entry_source = loader
        .modules
        .get(&entry_name)
        .map(|m| m.source.clone())
        .unwrap_or_default();

    Ok(LoadedProject {
        module: merged,
        entry_source,
        warnings: loader.warnings,
    })
}

impl ProjectLoader {
    fn load_module(&mut self, name: SmolStr, path: PathBuf) {
        if self.modules.contains_key(&name) {
            return;
        }
        if build_trace() {
            eprintln!("project: reading {} ({})", name, path.display());
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(err) => {
                self.errors.push(ProjectError {
                    path: path.clone(),
                    source: String::new(),
                    message: format!("failed to read module '{}': {}", name, err),
                    span: SourceSpan::from((0usize, 0usize)),
                });
                return;
            }
        };

        if build_trace() {
            eprintln!("project: parse start {}", name);
        }
        let (node, parse_errors) = parser::parse_with_errors(&source);
        if build_trace() {
            eprintln!("project: parse done {}", name);
        }
        if !parse_errors.is_empty() {
            for err in parse_errors {
                self.errors.push(ProjectError {
                    path: path.clone(),
                    source: source.clone(),
                    message: err.message,
                    span: err.span,
                });
            }
            return;
        }

        if build_trace() {
            eprintln!("project: validate start {}", name);
        }
        let validation_errors = parser::validate::validate(&node);
        if build_trace() {
            eprintln!("project: validate done {}", name);
        }
        if !validation_errors.is_empty() {
            for err in validation_errors {
                self.errors.push(ProjectError {
                    path: path.clone(),
                    source: source.clone(),
                    message: err.message,
                    span: err.span,
                });
            }
            return;
        }

        let root = parser::ast::Root::cast(node).expect("expected root node");
        let root_body_root =
            parser::ast::Root::cast(root.syntax().clone()).expect("expected root node");
        let root_body = lower_root_body(root_body_root);
        let module = lower(root);
        let uses = collect_use_sites(&module);
        let name_for_uses = name.clone();
        self.modules.insert(
            name.clone(),
            LoadedModule {
                name: name.clone(),
                path: path.clone(),
                source,
                module,
                uses,
                root_body,
            },
        );

        let imported_modules = self
            .modules
            .get(&name_for_uses)
            .map(|m| m.uses.clone())
            .unwrap_or_default();
        for use_site in imported_modules {
            if let Some(module_path) = self.resolve_module_path(&use_site.module) {
                self.load_module(use_site.module.clone(), module_path);
            }
        }
    }

    fn enforce_entrypoint(&mut self, entry_name: &SmolStr) {
        let mut has_main = false;
        for module in self.modules.values() {
            if module
                .module
                .functions
                .iter()
                .any(|(_, func)| func.name == "main")
            {
                has_main = true;
                break;
            }
        }
        if has_main {
            for module in self.modules.values() {
                if let Some((_, func)) = module
                    .module
                    .functions
                    .iter()
                    .find(|(_, func)| func.name == "main")
                {
                    self.errors.push(ProjectError {
                        path: module.path.clone(),
                        source: module.source.clone(),
                        message: "function name 'main' is reserved (use 'run' as the entrypoint)"
                            .to_string(),
                        span: span_from_range(
                            func.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
                        ),
                    });
                    break;
                }
            }
            return;
        }

        for module in self.modules.values() {
            if &module.name == entry_name {
                continue;
            }
            if let Some((_, func)) = module
                .module
                .functions
                .iter()
                .find(|(_, func)| func.name == "run")
            {
                self.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: "only the entry module may define 'run'".to_string(),
                    span: span_from_range(
                        func.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
            }
        }

        for module in self.modules.values() {
            if let Some(body) = &module.root_body {
                if let Some(first) = body.root_stmts.first() {
                    self.errors.push(ProjectError {
                        path: module.path.clone(),
                        source: module.source.clone(),
                        message: "top-level executable statements are not allowed; only class/func/use are allowed at the top level"
                            .to_string(),
                        span: span_from_range(body.stmt_span(*first)),
                    });
                }
            }
        }

        let Some(entry_module) = self.modules.get_mut(entry_name) else {
            return;
        };

        let has_run = entry_module
            .module
            .functions
            .iter()
            .any(|(_, func)| func.name == "run");
        if !has_run {
            self.errors.push(ProjectError {
                path: entry_module.path.clone(),
                source: entry_module.source.clone(),
                message: "entry module must define 'to run() -> Type'".to_string(),
                span: SourceSpan::from((0usize, 0usize)),
            });
            return;
        }

        let wrapper = make_main_wrapper();
        entry_module.module.functions.alloc(wrapper);
    }

    fn detect_cycles(&mut self) {
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum VisitState {
            Visiting,
            Done,
        }

        fn dfs(
            name: &SmolStr,
            loader: &mut ProjectLoader,
            states: &mut HashMap<SmolStr, VisitState>,
            stack: &mut Vec<SmolStr>,
        ) {
            if matches!(states.get(name), Some(VisitState::Visiting)) {
                return;
            }
            if matches!(states.get(name), Some(VisitState::Done)) {
                return;
            }
            states.insert(name.clone(), VisitState::Visiting);
            stack.push(name.clone());

            let uses = match loader.modules.get(name) {
                Some(module) => module.uses.clone(),
                None => {
                    states.insert(name.clone(), VisitState::Done);
                    stack.pop();
                    return;
                }
            };

            for use_site in uses {
                let target = &use_site.module;
                if let Some(VisitState::Visiting) = states.get(target) {
                    let start = stack.iter().position(|m| m == target).unwrap_or(0);
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(target.clone());
                    let path = cycle
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    if let Some(origin) = loader.modules.get(name) {
                        loader.errors.push(ProjectError {
                            path: origin.path.clone(),
                            source: origin.source.clone(),
                            message: format!("circular module dependency detected: {}", path),
                            span: span_from_range(use_site.span),
                        });
                    }
                    continue;
                }
                if loader.modules.contains_key(target) {
                    dfs(target, loader, states, stack);
                }
            }

            stack.pop();
            states.insert(name.clone(), VisitState::Done);
        }

        let mut states: HashMap<SmolStr, VisitState> = HashMap::new();
        let mut stack = Vec::new();
        let module_names: Vec<SmolStr> = self.modules.keys().cloned().collect();
        for name in module_names {
            if !states.contains_key(&name) {
                dfs(&name, self, &mut states, &mut stack);
            }
        }
    }

    fn analyze_imports(&mut self) {
        let mut public_exports: HashMap<SmolStr, HashSet<SmolStr>> = HashMap::new();
        let mut local_defs: HashMap<SmolStr, HashSet<SmolStr>> = HashMap::new();

        for (name, module) in &self.modules {
            let mut exports = HashSet::new();
            let mut locals = HashSet::new();
            for (_, func) in module.module.functions.iter() {
                locals.insert(func.name.clone());
                if matches!(func.visibility, Visibility::Public) {
                    exports.insert(func.name.clone());
                }
            }
            for (_, class) in module.module.classes.iter() {
                locals.insert(class.name.clone());
                if matches!(class.visibility, Visibility::Public) {
                    exports.insert(class.name.clone());
                }
            }
            for (_, en) in module.module.enums.iter() {
                locals.insert(en.name.clone());
                if matches!(en.visibility, Visibility::Public) {
                    exports.insert(en.name.clone());
                }
            }
            for (_, interface) in module.module.interfaces.iter() {
                locals.insert(interface.name.clone());
                if matches!(interface.visibility, Visibility::Public) {
                    exports.insert(interface.name.clone());
                }
            }
            public_exports.insert(name.clone(), exports);
            local_defs.insert(name.clone(), locals);
        }

        let mut used_external: HashMap<SmolStr, HashMap<SmolStr, TextRange>> = HashMap::new();
        for (name, module) in &self.modules {
            used_external.insert(name.clone(), collect_used_external_names(&module.module));
        }

        let stdlib_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib");
        for module in self.modules.values() {
            let mut imported_names: HashMap<SmolStr, (SmolStr, TextRange)> = HashMap::new();
            let mut imported_set: HashSet<SmolStr> = HashSet::new();
            let mut glob_modules: HashSet<SmolStr> = HashSet::new();
            let used = used_external.get(&module.name).cloned().unwrap_or_default();
            let locals = local_defs.get(&module.name).cloned().unwrap_or_default();
            let skip_import_checks = module.name.as_str() == "core";
            let allow_any_type = module.path.starts_with(&stdlib_root);

            for use_site in &module.uses {
                let target = use_site.module.clone();
                let target_public = public_exports.get(&target).cloned().unwrap_or_default();
                let mut saw_glob = false;

                for name in &use_site.names {
                    match &name.kind {
                        UseNameKind::Glob => {
                            saw_glob = true;
                        }
                        UseNameKind::Name(item) => {
                            if locals.contains(item) {
                                self.errors.push(ProjectError {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!(
                                        "import '{}' conflicts with local definition",
                                        item
                                    ),
                                    span: span_from_range(name.span),
                                });
                                continue;
                            }
                            if let Some((prev, prev_span)) = imported_names.get(item) {
                                if prev != &target {
                                    self.errors.push(ProjectError {
                                        path: module.path.clone(),
                                        source: module.source.clone(),
                                        message: format!(
                                            "import '{}' conflicts with module '{}'",
                                            item, prev
                                        ),
                                        span: span_from_range(name.span),
                                    });
                                    self.errors.push(ProjectError {
                                        path: module.path.clone(),
                                        source: module.source.clone(),
                                        message: format!(
                                            "previous import of '{}' from '{}'",
                                            item, prev
                                        ),
                                        span: span_from_range(*prev_span),
                                    });
                                }
                                continue;
                            }
                            imported_names.insert(item.clone(), (target.clone(), name.span));
                            imported_set.insert(item.clone());
                            if !used.contains_key(item) {
                                self.warnings.push(ProjectWarning {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!("unused import '{}'", item),
                                    span: span_from_range(name.span),
                                });
                            }
                        }
                    }
                }

                if saw_glob {
                    if glob_modules.contains(&target) {
                        continue;
                    }
                    glob_modules.insert(target.clone());

                    for export in &target_public {
                        if locals.contains(export) {
                            self.errors.push(ProjectError {
                                path: module.path.clone(),
                                source: module.source.clone(),
                                message: format!(
                                    "glob import from '{}' conflicts with local definition '{}'",
                                    target, export
                                ),
                                span: span_from_range(use_site.span),
                            });
                            continue;
                        }
                        if let Some((prev, prev_span)) = imported_names.get(export) {
                            if prev != &target {
                                self.errors.push(ProjectError {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!(
                                        "glob import from '{}' conflicts with module '{}' for '{}'",
                                        target, prev, export
                                    ),
                                    span: span_from_range(use_site.span),
                                });
                                self.errors.push(ProjectError {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!(
                                        "previous import of '{}' from '{}'",
                                        export, prev
                                    ),
                                    span: span_from_range(*prev_span),
                                });
                                continue;
                            }
                        }
                        imported_names.insert(export.clone(), (target.clone(), use_site.span));
                        imported_set.insert(export.clone());
                    }

                    if !target_public.is_empty()
                        && !target_public.iter().any(|export| used.contains_key(export))
                    {
                        self.warnings.push(ProjectWarning {
                            path: module.path.clone(),
                            source: module.source.clone(),
                            message: format!("unused glob import from '{}'", target),
                            span: span_from_range(use_site.span),
                        });
                    }
                }
            }

            for (name, span) in &used {
                if skip_import_checks {
                    continue;
                }
                if name.as_str() == "Any" {
                    if allow_any_type {
                        continue;
                    }
                    self.errors.push(ProjectError {
                        path: module.path.clone(),
                        source: module.source.clone(),
                        message: "type 'Any' is reserved for stdlib".to_string(),
                        span: span_from_range(*span),
                    });
                    continue;
                }
                if locals.contains(name) || imported_set.contains(name) {
                    continue;
                }
                if is_builtin_value_name(name) || is_builtin_type_name(name) {
                    continue;
                }
                self.errors.push(ProjectError {
                    path: module.path.clone(),
                    source: module.source.clone(),
                    message: format!("use of '{}' requires an explicit import", name),
                    span: span_from_range(*span),
                });
            }
        }
    }

    fn resolve_module_path(&self, name: &SmolStr) -> Option<PathBuf> {
        if let Some(tests_root) = &self.tests_dir {
            if let Some(test_rel) = name.as_str().strip_prefix("tests/") {
                let mut rel = PathBuf::from(test_rel);
                let candidate_wr = tests_root.join(rel.with_extension("wr"));
                if candidate_wr.is_file() {
                    return Some(candidate_wr);
                }
                rel = PathBuf::from(test_rel);
                let candidate_sp = tests_root.join(rel.with_extension("sp"));
                if candidate_sp.is_file() {
                    return Some(candidate_sp);
                }
            }
        }
        let stdlib_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib");
        let canonical_name = canonical_stdlib_module(name);
        let mut rel = PathBuf::from(canonical_name.as_str());
        let candidate_wr = stdlib_root.join(rel.with_extension("wr"));
        if candidate_wr.is_file() {
            return Some(candidate_wr);
        }
        rel = PathBuf::from(canonical_name.as_str());
        let candidate_sp = stdlib_root.join(rel.with_extension("sp"));
        if candidate_sp.is_file() {
            return Some(candidate_sp);
        }
        let mut rel = PathBuf::from(name.as_str());
        let candidate_wr = self.root_dir.join(rel.with_extension("wr"));
        if candidate_wr.is_file() {
            return Some(candidate_wr);
        }
        rel = PathBuf::from(name.as_str());
        let candidate_sp = self.root_dir.join(rel.with_extension("sp"));
        if candidate_sp.is_file() {
            return Some(candidate_sp);
        }
        None
    }

    fn validate_uses(&mut self) {
        let mut exports: HashMap<SmolStr, HashMap<SmolStr, DefinitionKind>> = HashMap::new();
        let mut all_defs: HashMap<SmolStr, HashMap<SmolStr, DefinitionKind>> = HashMap::new();

        for (name, module) in &self.modules {
            let mut module_exports = HashMap::new();
            let mut module_all = HashMap::new();
            for (_, func) in module.module.functions.iter() {
                module_all.insert(func.name.clone(), DefinitionKind::Function);
                if matches!(func.visibility, Visibility::Public) {
                    module_exports.insert(func.name.clone(), DefinitionKind::Function);
                }
            }
            for (_, class) in module.module.classes.iter() {
                module_all.insert(class.name.clone(), DefinitionKind::Class);
                if matches!(class.visibility, Visibility::Public) {
                    module_exports.insert(class.name.clone(), DefinitionKind::Class);
                }
            }
            exports.insert(name.clone(), module_exports);
            all_defs.insert(name.clone(), module_all);
        }

        for module in self.modules.values() {
            for use_site in &module.uses {
                let Some(target_exports) = exports.get(&use_site.module) else {
                    let message = removed_core_stdlib_module_message(use_site.module.as_str())
                        .unwrap_or_else(|| format!("module '{}' not found", use_site.module));
                    self.errors.push(ProjectError {
                        path: module.path.clone(),
                        source: module.source.clone(),
                        message,
                        span: span_from_range(
                            use_site.module_span.unwrap_or_else(|| use_site.span),
                        ),
                    });
                    continue;
                };
                let target_all = all_defs.get(&use_site.module).cloned().unwrap_or_default();
                let mut saw_glob = false;
                for name in &use_site.names {
                    match &name.kind {
                        UseNameKind::Glob => {
                            saw_glob = true;
                        }
                        UseNameKind::Name(item) => {
                            if target_exports.contains_key(item) {
                                continue;
                            }
                            if target_all.contains_key(item) {
                                self.errors.push(ProjectError {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!(
                                        "cannot import private {} '{}' from module '{}'",
                                        def_kind(target_all[item]),
                                        item,
                                        use_site.module
                                    ),
                                    span: span_from_range(name.span),
                                });
                            } else {
                                self.errors.push(ProjectError {
                                    path: module.path.clone(),
                                    source: module.source.clone(),
                                    message: format!(
                                        "module '{}' has no item named '{}'",
                                        use_site.module, item
                                    ),
                                    span: span_from_range(name.span),
                                });
                            }
                        }
                    }
                }
                if saw_glob && target_exports.is_empty() {
                    self.warnings.push(ProjectWarning {
                        path: module.path.clone(),
                        source: module.source.clone(),
                        message: format!(
                            "module '{}' has no public items to import",
                            use_site.module
                        ),
                        span: span_from_range(use_site.span),
                    });
                }
            }
        }
    }
}

fn removed_core_stdlib_module_message(module_name: &str) -> Option<String> {
    let removed = matches!(
        module_name,
        "admin"
            | "auth"
            | "files"
            | "http"
            | "jobs"
            | "pubsub"
            | "rate_limit"
            | "rbac"
            | "realtime"
            | "schedule"
            | "search"
            | "storage"
    );
    if removed {
        Some(format!(
            "module '{}' was removed from core stdlib during thin-core reset",
            module_name
        ))
    } else {
        None
    }
}

fn canonical_stdlib_module_name(module_name: &str) -> Option<&'static str> {
    match module_name {
        "actor" => Some("runtime/actor"),
        "pool" => Some("runtime/pool"),
        "scheduler" => Some("runtime/scheduler"),
        "runtime" => Some("runtime/config"),
        "reactor" => Some("runtime/reactor"),
        "task" => Some("runtime/task"),
        "bytes" => Some("data/bytes"),
        "list" => Some("data/list"),
        "map" => Some("data/map"),
        "parse" => Some("data/parse"),
        "env" => Some("host/env"),
        "fs" => Some("host/fs"),
        "io" => Some("host/io"),
        "log" => Some("host/log"),
        "time" => Some("host/time"),
        _ => None,
    }
}

fn canonical_stdlib_module(module_name: &SmolStr) -> SmolStr {
    if let Some(canonical) = canonical_stdlib_module_name(module_name.as_str()) {
        SmolStr::new(canonical)
    } else {
        module_name.clone()
    }
}

fn collect_use_sites(module: &Module) -> Vec<UseSite> {
    module
        .uses
        .iter()
        .map(|use_stmt| UseSite {
            module: canonical_stdlib_module(&use_stmt.module),
            names: use_stmt.names.clone(),
            span: use_stmt.span,
            module_span: use_stmt.module_span,
        })
        .collect()
}

fn def_kind(kind: DefinitionKind) -> &'static str {
    match kind {
        DefinitionKind::Function => "function",
        DefinitionKind::Class => "class",
    }
}

fn make_main_wrapper() -> crate::hir::Function {
    let mut body = Body {
        exprs: crate::hir::Arena::new(),
        stmts: crate::hir::Arena::new(),
        root_stmts: Vec::new(),
        expr_spans: Vec::new(),
        stmt_spans: Vec::new(),
    };
    let var = body
        .exprs
        .alloc(crate::hir::Expr::Variable(SmolStr::new("run")));
    body.expr_spans.push(TextRange::empty(0.into()));
    let call = body.exprs.alloc(crate::hir::Expr::Call {
        callee: var,
        args: Vec::new(),
        type_args: Vec::new(),
    });
    body.expr_spans.push(TextRange::empty(0.into()));
    let stmt = body.stmts.alloc(Stmt::Return(Some(call)));
    body.stmt_spans.push(TextRange::empty(0.into()));
    body.root_stmts.push(stmt);

    crate::hir::Function {
        name: SmolStr::new("main"),
        name_span: None,
        visibility: Visibility::Private,
        kind: FunctionKind::Function,
        params: Vec::new(),
        ret_type: None,
        body: Some(body),
    }
}

fn find_src_root(entry_path: &Path) -> Option<PathBuf> {
    for ancestor in entry_path.ancestors() {
        if ancestor.file_name().and_then(|s| s.to_str()) == Some("src") {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn module_name_for_path(path: &Path, root: &Path) -> Option<SmolStr> {
    let mut relative = path.strip_prefix(root).ok()?.to_path_buf();
    relative.set_extension("");
    let mut parts = Vec::new();
    for comp in relative.components() {
        let piece = comp.as_os_str().to_string_lossy();
        if !piece.is_empty() {
            parts.push(piece.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(SmolStr::new(parts.join("/")))
    }
}

fn module_name_for_entry_path(path: &Path, root: &Path) -> SmolStr {
    if let Some(name) = module_name_for_path(path, root) {
        return name;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("entry");
    SmolStr::new(format!("__entry__{stem}"))
}

fn span_from_range(range: TextRange) -> SourceSpan {
    let start: usize = range.start().into();
    let len: usize = range.len().into();
    SourceSpan::from((start, len))
}

fn collect_used_external_names(module: &Module) -> HashMap<SmolStr, TextRange> {
    let mut used = HashMap::new();
    let mut method_ids = HashSet::new();
    let mut method_type_params: HashMap<Idx<Function>, HashSet<SmolStr>> = HashMap::new();
    for (_, class) in module.classes.iter() {
        let class_type_params: HashSet<SmolStr> = class.type_params.iter().cloned().collect();
        for method in &class.methods {
            method_ids.insert(*method);
            if !class_type_params.is_empty() {
                method_type_params.insert(*method, class_type_params.clone());
            }
        }
        for field in &class.fields {
            collect_type_names(field.ty.as_ref(), &mut used, &class_type_params);
        }
    }
    for (_, en) in module.enums.iter() {
        let enum_type_params: HashSet<SmolStr> = en.type_params.iter().cloned().collect();
        for variant in &en.variants {
            for param in &variant.params {
                collect_type_names(param.ty.as_ref(), &mut used, &enum_type_params);
            }
        }
    }
    for (_, interface) in module.interfaces.iter() {
        let interface_type_params: HashSet<SmolStr> =
            interface.type_params.iter().cloned().collect();
        for method in &interface.methods {
            for param in &method.params {
                collect_type_names(param.ty.as_ref(), &mut used, &interface_type_params);
            }
            collect_type_names(method.ret_type.as_ref(), &mut used, &interface_type_params);
        }
    }
    let empty_type_params: HashSet<SmolStr> = HashSet::new();
    for (idx, func) in module.functions.iter() {
        let type_params = method_type_params.get(&idx).unwrap_or(&empty_type_params);
        for param in &func.params {
            collect_type_names(param.ty.as_ref(), &mut used, type_params);
        }
        collect_type_names(func.ret_type.as_ref(), &mut used, type_params);
    }

    for (idx, func) in module.functions.iter() {
        if let Some(body) = &func.body {
            let mut scope = Scope::new();
            if method_ids.contains(&idx) {
                scope.insert(SmolStr::new("it"));
            }
            for param in &func.params {
                scope.insert(param.name.clone());
            }
            collect_used_in_block(body, &body.root_stmts, &mut scope, &mut used);
        }
    }
    used
}

fn collect_type_names(
    ty: Option<&crate::hir::TypeRef>,
    used: &mut HashMap<SmolStr, TextRange>,
    type_params: &HashSet<SmolStr>,
) {
    let Some(ty) = ty else { return };
    if !is_builtin_type_name(&ty.name) && !type_params.contains(&ty.name) {
        record_used(
            used,
            ty.name.clone(),
            ty.name_span.unwrap_or_else(|| TextRange::empty(0.into())),
        );
    }
    for arg in &ty.args {
        collect_type_names(Some(arg), used, type_params);
    }
}

fn is_builtin_type_name(name: &SmolStr) -> bool {
    matches!(
        name.as_str(),
        "Integer"
            | "Float"
            | "Number"
            | "Boolean"
            | "String"
            | "Nothing"
            | "Bytes"
            | "List"
            | "Map"
            | "Result"
            | "Actor"
            | "Pending"
            | "Error"
    )
}

fn record_used(used: &mut HashMap<SmolStr, TextRange>, name: SmolStr, span: TextRange) {
    let entry = used.entry(name).or_insert(span);
    if entry.is_empty() && !span.is_empty() {
        *entry = span;
    }
}

fn is_builtin_value_name(name: &SmolStr) -> bool {
    matches!(
        name.as_str(),
        "__wr_assert_err"
            | "__wr_print"
            | "__wr_list_push"
            | "__wr_map_new"
            | "__wr_runtime_cpu_count"
            | "__wr_reactor_new"
            | "__wr_reactor_drop"
            | "__wr_reactor_register"
            | "__wr_reactor_deregister"
            | "__wr_reactor_arm_timer"
            | "__wr_task_signal_new"
            | "__wr_task_signal_drop"
            | "__wr_task_unpark_one"
            | "__wr_task_unpark_all"
            | "__wr_task_epoch"
            | "__wr_atomic_i64_new"
            | "__wr_atomic_i64_drop"
            | "__wr_atomic_i64_load"
            | "__wr_atomic_i64_store"
            | "__wr_atomic_i64_fetch_add"
            | "__wr_pool_size"
            | "__wr_pool_rr"
            | "__wr_pool_queue_len"
            | "__wr_actor_mailbox_len"
            | "__wr_actor_pause"
            | "__wr_actor_resume"
            | "__wr_actor_pause_wait"
            | "__wr_metrics_get"
            | "__wr_metrics_dropped_paused_id"
            | "__wr_metrics_messages_dropped_id"
            | "__wr_clock_ns"
            | "__wr_sleep_ms"
            | "__wr_bytes_from_string"
            | "__wr_bytes_from_list"
            | "__wr_bytes_to_string"
            | "__wr_bytes_to_list"
            | "__wr_bytes_len"
            | "__wr_fs_read_bytes"
            | "__wr_fs_write_bytes"
            | "__wr_map_get"
            | "__wr_map_set"
            | "__wr_log"
            | "__wr_log_configure"
            | "__wr_env_get"
            | "__wr_env_set"
            | "__wr_runtime_configure"
            | "Pool"
            | "queue"
            | "drop"
            | "n"
    )
}

struct Scope {
    stack: Vec<HashSet<SmolStr>>,
}

impl Scope {
    fn new() -> Self {
        Self {
            stack: vec![HashSet::new()],
        }
    }

    fn enter(&mut self) {
        self.stack.push(HashSet::new());
    }

    fn exit(&mut self) {
        self.stack.pop();
    }

    fn insert(&mut self, name: SmolStr) {
        if let Some(scope) = self.stack.last_mut() {
            scope.insert(name);
        }
    }

    fn contains(&self, name: &SmolStr) -> bool {
        self.stack.iter().rev().any(|scope| scope.contains(name))
    }
}

fn collect_used_in_block(
    body: &Body,
    stmts: &[crate::hir::Idx<Stmt>],
    scope: &mut Scope,
    used: &mut HashMap<SmolStr, TextRange>,
) {
    for stmt_id in stmts {
        let stmt = &body.stmts[*stmt_id];
        match stmt {
            Stmt::Expr(expr) => collect_used_in_expr(body, *expr, scope, used),
            Stmt::Assert { expr, .. } => collect_used_in_expr(body, *expr, scope, used),
            Stmt::Require { condition, message } => {
                collect_used_in_expr(body, *condition, scope, used);
                collect_used_in_expr(body, *message, scope, used);
            }
            Stmt::Defer { expr } => collect_used_in_expr(body, *expr, scope, used),
            Stmt::IgnoreResult { expr } => collect_used_in_expr(body, *expr, scope, used),
            Stmt::Capture { name, value } => {
                collect_used_in_expr(body, *value, scope, used);
                scope.insert(name.clone());
            }
            Stmt::Let { name, value, .. } => {
                collect_used_in_expr(body, *value, scope, used);
                scope.insert(name.clone());
            }
            Stmt::Assign { name, value, .. } => {
                if !scope.contains(name) {
                    record_used(used, name.clone(), body.stmt_span(*stmt_id));
                }
                collect_used_in_expr(body, *value, scope, used);
            }
            Stmt::Optimize { body: opt_body, .. } => {
                scope.enter();
                collect_used_in_block(body, opt_body, scope, used);
                scope.exit();
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_used_in_expr(body, *condition, scope, used);
                scope.enter();
                collect_used_in_block(body, then_branch, scope, used);
                scope.exit();
                if let Some(branch) = else_branch {
                    scope.enter();
                    collect_used_in_block(body, branch, scope, used);
                    scope.exit();
                }
            }
            Stmt::For {
                name,
                iterable,
                body: loop_body,
            } => {
                collect_used_in_expr(body, *iterable, scope, used);
                scope.enter();
                scope.insert(name.clone());
                collect_used_in_block(body, loop_body, scope, used);
                scope.exit();
            }
            Stmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                collect_used_in_expr(body, *subject, scope, used);
                for case in cases {
                    scope.enter();
                    for label in &case.labels {
                        collect_pattern_bindings(label, scope);
                    }
                    collect_used_in_block(body, &case.body, scope, used);
                    scope.exit();
                }
                if let Some(otherwise_body) = otherwise {
                    scope.enter();
                    collect_used_in_block(body, otherwise_body, scope, used);
                    scope.exit();
                }
            }
            Stmt::While {
                condition,
                body: loop_body,
            } => {
                collect_used_in_expr(body, *condition, scope, used);
                scope.enter();
                collect_used_in_block(body, loop_body, scope, used);
                scope.exit();
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    collect_used_in_expr(body, *expr, scope, used);
                }
            }
            Stmt::Use { .. } => {}
            Stmt::Break | Stmt::Continue => {}
        }
    }
}

fn collect_used_in_expr(
    body: &Body,
    expr_id: crate::hir::Idx<crate::hir::Expr>,
    scope: &mut Scope,
    used: &mut HashMap<SmolStr, TextRange>,
) {
    use crate::hir::Expr;
    let expr = &body.exprs[expr_id];
    match expr {
        Expr::Literal(_) => {}
        Expr::Variable(name) => {
            if name != "it" && name != "its" && !scope.contains(name) {
                record_used(used, name.clone(), body.expr_span(expr_id));
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_used_in_expr(body, *lhs, scope, used);
            collect_used_in_expr(body, *rhs, scope, used);
        }
        Expr::Detach { target, .. } => {
            collect_used_in_expr(body, *target, scope, used);
        }
        Expr::Unary { expr, .. } => {
            collect_used_in_expr(body, *expr, scope, used);
        }
        Expr::TypeApply { callee, .. } => {
            collect_used_in_expr(body, *callee, scope, used);
        }
        Expr::Crash { expr } => {
            collect_used_in_expr(body, *expr, scope, used);
        }
        Expr::Call { callee, args, .. } => {
            collect_used_in_expr(body, *callee, scope, used);
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. }
                    | crate::hir::Arg::Named { value, .. } => {
                        collect_used_in_expr(body, *value, scope, used);
                    }
                }
            }
        }
        Expr::GivenCall { callee, args, .. } => {
            collect_used_in_expr(body, *callee, scope, used);
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. }
                    | crate::hir::Arg::Named { value, .. } => {
                        collect_used_in_expr(body, *value, scope, used);
                    }
                }
            }
        }
        Expr::Member { object, .. } => {
            collect_used_in_expr(body, *object, scope, used);
        }
        Expr::List(items) => {
            for item in items {
                collect_used_in_expr(body, *item, scope, used);
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                collect_used_in_expr(body, *key, scope, used);
                collect_used_in_expr(body, *value, scope, used);
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    collect_used_in_expr(body, *expr, scope, used);
                }
            }
        }
    }
}

fn collect_pattern_bindings(pattern: &crate::hir::Pattern, scope: &mut Scope) {
    match pattern {
        crate::hir::Pattern::Binding(name) => {
            scope.insert(name.clone());
        }
        crate::hir::Pattern::Path { args, .. } => {
            for arg in args {
                collect_pattern_bindings(arg, scope);
            }
        }
        crate::hir::Pattern::Wildcard | crate::hir::Pattern::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_temp(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn test_load_project_with_use() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        let mod_path = base.join("src").join("bar.wr");

        write_temp(
            &entry_path,
            "use foo from bar\n\nto run() -> Integer:\n    return f()\n\nto f() -> Integer:\n    return foo()\n",
        );
        write_temp(&mod_path, "to foo() -> Integer:\n    return 1\n");

        let project = load_project(&entry_path);
        assert!(project.is_ok());
        let project = project.unwrap();
        let mut found = false;
        for (_, func) in project.module.functions.iter() {
            if func.name == "foo" {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn test_load_project_outside_src() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_outside_src_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("bench").join("main.wr");
        let mod_path = base.join("bench").join("bar.wr");

        write_temp(
            &entry_path,
            "use foo from bar\n\nto run() -> Integer:\n    return foo()\n",
        );
        write_temp(&mod_path, "to foo() -> Integer:\n    return 1\n");

        let project = load_project(&entry_path);
        assert!(project.is_ok());
    }

    #[test]
    fn test_entry_top_level_is_error() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_entry_root_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        write_temp(&entry_path, "__wr_print(\"hi\")\n");

        let project = load_project(&entry_path);
        assert!(project.is_err());
    }

    #[test]
    fn test_non_entry_top_level_is_error() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_non_entry_root_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        let mod_path = base.join("src").join("bar.wr");

        write_temp(
            &entry_path,
            "use * from bar\n\nto run() -> Integer:\n    return 1\n",
        );
        write_temp(&mod_path, "__wr_print(\"hi\")\n");

        let project = load_project(&entry_path);
        assert!(project.is_err());
    }

    #[test]
    fn test_missing_import_is_error() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_missing_import_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        write_temp(&entry_path, "to run() -> Integer:\n    return foo()\n");

        let project = load_project(&entry_path);
        assert!(project.is_err());
    }

    #[test]
    fn test_entry_requires_run() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_requires_run_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        write_temp(&entry_path, "to f() -> Integer:\n    return 1\n");

        let project = load_project(&entry_path);
        assert!(project.is_err());
    }

    #[test]
    fn test_run_only_in_entry_module() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_run_only_entry_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        let mod_path = base.join("src").join("bar.wr");

        write_temp(
            &entry_path,
            "use * from bar\n\nto run() -> Integer:\n    return 1\n",
        );
        write_temp(&mod_path, "to run() -> Integer:\n    return 2\n");

        let project = load_project(&entry_path);
        assert!(project.is_err());
    }

    #[test]
    fn test_stdlib_flat_alias_table_maps_to_grouped_paths() {
        assert_eq!(canonical_stdlib_module_name("actor"), Some("runtime/actor"));
        assert_eq!(canonical_stdlib_module_name("runtime"), Some("runtime/config"));
        assert_eq!(canonical_stdlib_module_name("parse"), Some("data/parse"));
        assert_eq!(canonical_stdlib_module_name("env"), Some("host/env"));
        assert_eq!(canonical_stdlib_module_name("metrics"), None);
    }

    #[test]
    fn test_stdlib_flat_alias_imports_resolve_without_flat_files() {
        let base = std::env::temp_dir().join(format!(
            "wrela_project_stdlib_aliases_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry_path = base.join("src").join("main.wr");
        write_temp(
            &entry_path,
            "use parse_integer from parse\nuse get_environment_variable_or_default from env\n\nto run() -> Integer:\n    value = get_environment_variable_or_default(\"WRELA_ALIAS_TEST\", \"7\")\n    return parse_integer(value) otherwise 0\n",
        );

        let project = load_project(&entry_path);
        assert!(project.is_ok());
    }
}
