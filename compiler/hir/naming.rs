#![allow(unused_assignments)]

use crate::hir::{
    Body, Class, Expr, Function, FunctionKind, FunctionRole, Idx, InterfaceMethodKind, MatchCase,
    Module, Pattern, Stmt, TypeRef,
};
use crate::hir::{Type, TypeInfo};
use miette::{Diagnostic, SourceSpan};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum NamingError {
    #[error("{kind} '{name}' must be ASCII snake_case")]
    #[diagnostic(
        code(lang::naming::snake_case_required),
        help("Use lowercase ASCII with underscores, like `load_user`.")
    )]
    SnakeCaseRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("{kind} '{name}' must be ASCII PascalCase")]
    #[diagnostic(
        code(lang::naming::pascal_case_required),
        help("Use ASCII PascalCase, like `UserSession`.")
    )]
    PascalCaseRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("{kind} '{name}' must start with a verb")]
    #[diagnostic(
        code(lang::naming::verb_led_required),
        help("Use a verb-led name, like `load_data`, `create_user`, or `try_to_fetch`.")
    )]
    VerbLedRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error(
        "top-level check '{name}' must contain `_is_` or `_has_` and cannot start with `is_`/`has_`"
    )]
    #[diagnostic(
        code(lang::naming::top_level_check_shape),
        help("Use a subject statement, like `account_is_active` or `request_has_token`.")
    )]
    TopLevelCheckName {
        name: SmolStr,
        #[label("rename this check")]
        span: SourceSpan,
    },

    #[error("class/interface check '{name}' must start with `is_` or `has_`")]
    #[diagnostic(
        code(lang::naming::member_check_prefix),
        help("Use `is_...` or `has_...` for class/interface checks.")
    )]
    MemberCheckPrefix {
        name: SmolStr,
        #[label("rename this check")]
        span: SourceSpan,
    },

    #[error("{kind} '{name}' must be noun-only (not verb-led)")]
    #[diagnostic(
        code(lang::naming::noun_only_required),
        help("Use a noun-like name, not an action.")
    )]
    NounOnlyRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("result-returning {kind} '{name}' must start with `try_to_`")]
    #[diagnostic(
        code(lang::naming::result_prefix_required),
        help("Rename to a `try_to_...` name.")
    )]
    ResultPrefixRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("factory {kind} '{name}' must start with `{required}`")]
    #[diagnostic(
        code(lang::naming::factory_prefix_required),
        help(
            "Use `create_...` for class factories and `try_to_create_...` for fallible factories."
        )
    )]
    FactoryPrefixRequired {
        kind: &'static str,
        name: SmolStr,
        required: &'static str,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("Result error type '{name}' must be PascalCase and end with `Error`")]
    #[diagnostic(
        code(lang::naming::result_error_type_shape),
        help(
            "Rename the error type to PascalCase ending in `Error` (for example `NetworkError`)."
        )
    )]
    ResultErrorTypeShape {
        name: SmolStr,
        #[label("rename this error type")]
        span: SourceSpan,
    },

    #[error("boolean {kind} '{name}' must start with `is_` or `has_`")]
    #[diagnostic(
        code(lang::naming::boolean_prefix_required),
        help("Boolean names must read like predicates (`is_...` / `has_...`).")
    )]
    BooleanPrefixRequired {
        kind: &'static str,
        name: SmolStr,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },

    #[error("check result '{name}' is stored then used once as an immediate condition")]
    #[diagnostic(
        code(lang::naming::inline_check_condition),
        help("Inline the check call directly in the `if`/`while`/`match` condition.")
    )]
    InlineCheckCondition {
        name: SmolStr,
        #[label("inline this check result")]
        span: SourceSpan,
    },

    #[error("module segment '{name}' must represent a resource/service noun")]
    #[diagnostic(
        code(lang::naming::module_semantic_required),
        help("Use noun-like module segments (resource/service), not action/predicate names.")
    )]
    ModuleSemanticRequired {
        name: SmolStr,
        #[label("rename this module segment")]
        span: SourceSpan,
    },

    #[error("collection {kind} '{name}' must be {expected_form}")]
    #[diagnostic(
        code(lang::naming::collection_plurality_required),
        help("Collections use plural names; loop binders over collections use singular names.")
    )]
    CollectionPluralityRequired {
        kind: &'static str,
        name: SmolStr,
        expected_form: &'static str,
        #[label("rename this {kind}")]
        span: SourceSpan,
    },
}

impl NamingError {
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            NamingError::SnakeCaseRequired { span, .. } => *span,
            NamingError::PascalCaseRequired { span, .. } => *span,
            NamingError::VerbLedRequired { span, .. } => *span,
            NamingError::TopLevelCheckName { span, .. } => *span,
            NamingError::MemberCheckPrefix { span, .. } => *span,
            NamingError::NounOnlyRequired { span, .. } => *span,
            NamingError::ResultPrefixRequired { span, .. } => *span,
            NamingError::FactoryPrefixRequired { span, .. } => *span,
            NamingError::ResultErrorTypeShape { span, .. } => *span,
            NamingError::BooleanPrefixRequired { span, .. } => *span,
            NamingError::InlineCheckCondition { span, .. } => *span,
            NamingError::ModuleSemanticRequired { span, .. } => *span,
            NamingError::CollectionPluralityRequired { span, .. } => *span,
        }
    }
}

pub fn check_module(module: &Module, type_info: &TypeInfo) -> Vec<NamingError> {
    let mut checker = Checker::new(module, type_info);
    checker.check_module();
    checker.errors
}

struct Checker<'a> {
    module: &'a Module,
    type_info: &'a TypeInfo,
    class_names: HashSet<SmolStr>,
    class_method_ids: HashSet<usize>,
    errors: Vec<NamingError>,
}

impl<'a> Checker<'a> {
    fn new(module: &'a Module, type_info: &'a TypeInfo) -> Self {
        let class_names = module
            .classes
            .iter()
            .map(|(_, class)| class.name.clone())
            .collect::<HashSet<_>>();
        let class_method_ids = build_class_method_ids(module);
        Self {
            module,
            type_info,
            class_names,
            class_method_ids,
            errors: Vec::new(),
        }
    }

    fn check_module(&mut self) {
        for (_, class) in self.module.classes.iter() {
            self.check_pascal("class", &class.name, class.name_span);
            self.check_fields(class);
        }

        for (_, interface) in self.module.interfaces.iter() {
            self.check_pascal("interface", &interface.name, interface.name_span);
            for method in &interface.methods {
                self.check_snake("interface method", &method.name, method.name_span);
                if method.kind == InterfaceMethodKind::Check
                    && !starts_with_is_or_has(method.name.as_str())
                {
                    self.errors.push(NamingError::MemberCheckPrefix {
                        name: method.name.clone(),
                        span: span_from_option(method.name_span),
                    });
                }
                self.check_params(None, &method.params, "parameter");
                if let Some(ret) = &method.ret_type {
                    self.check_result_error_type_names(ret);
                }
            }
        }

        for (_, en) in self.module.enums.iter() {
            self.check_pascal("enum", &en.name, en.name_span);
            for variant in &en.variants {
                self.check_pascal("enum variant", &variant.name, variant.name_span);
                self.check_params(None, &variant.params, "parameter");
            }
        }

        for use_stmt in &self.module.uses {
            for segment in use_stmt.module.split('/') {
                if segment.is_empty() {
                    continue;
                }
                if !is_ascii_snake(segment) {
                    self.errors.push(NamingError::SnakeCaseRequired {
                        kind: "module segment",
                        name: SmolStr::new(segment),
                        span: span_from_option(use_stmt.module_span),
                    });
                }
                if !module_segment_is_nounish(segment) {
                    self.errors.push(NamingError::ModuleSemanticRequired {
                        name: SmolStr::new(segment),
                        span: span_from_option(use_stmt.module_span),
                    });
                }
            }
        }

        for (func_id, func) in self.module.functions.iter() {
            self.check_function(func_id.into_raw(), func);
        }
    }

    fn check_fields(&mut self, class: &Class) {
        for field in &class.fields {
            self.check_snake("field", &field.name, field.name_span);
            if is_verb_led(field.name.as_str()) {
                self.errors.push(NamingError::NounOnlyRequired {
                    kind: "field",
                    name: field.name.clone(),
                    span: span_from_option(field.name_span),
                });
            }
            if let Some(ty) = &field.ty {
                if type_ref_is_collection_like(ty) {
                    self.check_plural_collection_name("field", &field.name, field.name_span);
                }
                self.check_result_error_type_names(ty);
            }
        }
    }

    fn check_function(&mut self, func_id: usize, func: &Function) {
        let is_class_scope = self.class_method_ids.contains(&func_id);
        let is_bypass = is_bypass_name(func.name.as_str());

        if !is_bypass {
            let kind = match func.role {
                FunctionRole::Field => "field",
                FunctionRole::Material => "material",
                FunctionRole::Radiance => "radiance field",
                FunctionRole::Volume => "volume field",
                _ => function_kind_label(func.kind),
            };
            self.check_snake(kind, &func.name, func.name_span);

            match func.role {
                FunctionRole::Field
                | FunctionRole::Material
                | FunctionRole::Radiance
                | FunctionRole::Volume => {
                    if is_verb_led(func.name.as_str()) {
                        self.errors.push(NamingError::NounOnlyRequired {
                            kind,
                            name: func.name.clone(),
                            span: span_from_option(func.name_span),
                        });
                    }
                }
                _ => match func.kind {
                    FunctionKind::Function | FunctionKind::Method | FunctionKind::Derived => {
                        if !is_verb_led(func.name.as_str()) {
                            self.errors.push(NamingError::VerbLedRequired {
                                kind,
                                name: func.name.clone(),
                                span: span_from_option(func.name_span),
                            });
                        }
                    }
                    FunctionKind::Check => {
                        let name = func.name.as_str();
                        let has_subject_predicate = name.contains("_is_") || name.contains("_has_");
                        if !has_subject_predicate || starts_with_is_or_has(name) {
                            self.errors.push(NamingError::TopLevelCheckName {
                                name: func.name.clone(),
                                span: span_from_option(func.name_span),
                            });
                        }
                    }
                    FunctionKind::CheckMethod => {
                        if !starts_with_is_or_has(func.name.as_str()) {
                            self.errors.push(NamingError::MemberCheckPrefix {
                                name: func.name.clone(),
                                span: span_from_option(func.name_span),
                            });
                        }
                    }
                },
            }
        }

        self.check_params(Some(func_id), &func.params, "parameter");
        if let Some(ret) = &func.ret_type {
            self.check_result_error_type_names(ret);
            if !matches!(
                func.role,
                FunctionRole::Field
                    | FunctionRole::Material
                    | FunctionRole::Radiance
                    | FunctionRole::Volume
            ) {
                self.check_return_name_rules(func, ret, is_class_scope);
            }
        }

        if let Some(body) = &func.body {
            let fn_types = self.type_info.functions.get(&func_id);
            self.check_body_locals(body, fn_types);
        }
    }

    fn check_params(
        &mut self,
        func_id: Option<usize>,
        params: &[crate::hir::Param],
        kind_label: &'static str,
    ) {
        for param in params {
            self.check_snake(kind_label, &param.name, param.name_span);

            let is_boolean = param.ty.as_ref().map(type_ref_is_boolean).unwrap_or(false)
                || func_id
                    .and_then(|id| self.type_info.functions.get(&id))
                    .and_then(|info| info.local_types.get(&param.name))
                    .is_some_and(type_is_boolean);
            if is_boolean && !starts_with_is_or_has(param.name.as_str()) {
                self.errors.push(NamingError::BooleanPrefixRequired {
                    kind: kind_label,
                    name: param.name.clone(),
                    span: span_from_option(param.name_span),
                });
            }

            if let Some(ty) = &param.ty {
                if type_ref_is_collection_like(ty) {
                    self.check_plural_collection_name(kind_label, &param.name, param.name_span);
                }
                self.check_result_error_type_names(ty);
            }
        }
    }

    fn check_return_name_rules(&mut self, func: &Function, ret: &TypeRef, is_class_scope: bool) {
        if is_bypass_name(func.name.as_str()) {
            return;
        }

        if let Some(factory_kind) = factory_return_kind(ret, &self.class_names)
            && func.kind == FunctionKind::Function
            && !is_class_scope
        {
            let required = if factory_kind {
                "try_to_create_"
            } else {
                "create_"
            };
            if !func.name.starts_with(required) {
                self.errors.push(NamingError::FactoryPrefixRequired {
                    kind: "function",
                    name: func.name.clone(),
                    required,
                    span: span_from_option(func.name_span),
                });
            }
            return;
        }

        if matches!(func.kind, FunctionKind::Function | FunctionKind::Method)
            && type_ref_is_result_like(ret)
            && !func.name.starts_with("try_to_")
        {
            self.errors.push(NamingError::ResultPrefixRequired {
                kind: function_kind_label(func.kind),
                name: func.name.clone(),
                span: span_from_option(func.name_span),
            });
        }
    }

    fn check_result_error_type_names(&mut self, ty: &TypeRef) {
        if ty.name == "Result" && ty.args.len() == 2 {
            let err_ty = &ty.args[1];
            let err_name = err_ty.name.as_str();
            if !is_ascii_pascal(err_name) || !err_name.ends_with("Error") {
                self.errors.push(NamingError::ResultErrorTypeShape {
                    name: err_ty.name.clone(),
                    span: span_from_option(err_ty.name_span),
                });
            }
        }

        for arg in &ty.args {
            self.check_result_error_type_names(arg);
        }
    }

    fn check_body_locals(&mut self, body: &Body, fn_types: Option<&crate::hir::FunctionTypeInfo>) {
        self.check_block(body, &body.root_stmts, fn_types);
    }

    fn check_block(
        &mut self,
        body: &Body,
        block: &[Idx<Stmt>],
        fn_types: Option<&crate::hir::FunctionTypeInfo>,
    ) {
        let usage_suffix = build_usage_counts(body, block);
        for (i, stmt_id) in block.iter().enumerate() {
            match &body.stmts[*stmt_id] {
                Stmt::Let { name, value, .. } => {
                    self.check_local_name(*stmt_id, name, fn_types, body);
                    self.check_check_result_inline(
                        *stmt_id,
                        i,
                        name,
                        *value,
                        body,
                        block,
                        &usage_suffix,
                        fn_types,
                    );
                }
                Stmt::For {
                    value_name,
                    key_name,
                    index_name,
                    iterable,
                    body: inner,
                    ..
                } => {
                    self.check_snake("local", value_name, Some(body.stmt_span(*stmt_id)));
                    if let Some(key_name) = key_name {
                        self.check_snake("local", key_name, Some(body.stmt_span(*stmt_id)));
                    }
                    if let Some(index_name) = index_name {
                        self.check_snake("local", index_name, Some(body.stmt_span(*stmt_id)));
                    }
                    let is_collection_iterable = fn_types
                        .and_then(|info| info.expr_types.get(&iterable.into_raw()))
                        .is_some_and(type_is_collection);
                    if is_collection_iterable {
                        self.check_singular_collection_binder_name(
                            "loop binder",
                            value_name,
                            Some(body.stmt_span(*stmt_id)),
                        );
                    }
                    self.check_block(body, inner, fn_types);
                }
                Stmt::Capture { name, .. } => {
                    self.check_local_name(*stmt_id, name, fn_types, body);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.check_block(body, then_branch, fn_types);
                    if let Some(branch) = else_branch {
                        self.check_block(body, branch, fn_types);
                    }
                }
                Stmt::Optimize { body: inner, .. } | Stmt::While { body: inner, .. } => {
                    self.check_block(body, inner, fn_types);
                }
                Stmt::Match {
                    cases, otherwise, ..
                } => {
                    for case in cases {
                        for label in &case.labels {
                            self.check_pattern(label, body.stmt_span(*stmt_id), fn_types);
                        }
                        self.check_block(body, &case.body, fn_types);
                    }
                    if let Some(branch) = otherwise {
                        self.check_block(body, branch, fn_types);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_local_name(
        &mut self,
        stmt_id: Idx<Stmt>,
        name: &SmolStr,
        fn_types: Option<&crate::hir::FunctionTypeInfo>,
        body: &Body,
    ) {
        self.check_snake("local", name, Some(body.stmt_span(stmt_id)));
        let is_boolean = fn_types
            .and_then(|info| info.local_types.get(name))
            .is_some_and(type_is_boolean);
        if is_boolean && !starts_with_is_or_has(name.as_str()) {
            self.errors.push(NamingError::BooleanPrefixRequired {
                kind: "local",
                name: name.clone(),
                span: span_from_range(body.stmt_span(stmt_id)),
            });
        }
        let is_collection = fn_types
            .and_then(|info| info.local_types.get(name))
            .is_some_and(type_is_collection);
        if is_collection {
            self.check_plural_collection_name("local", name, Some(body.stmt_span(stmt_id)));
        }
    }

    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        fallback_span: TextRange,
        fn_types: Option<&crate::hir::FunctionTypeInfo>,
    ) {
        match pattern {
            Pattern::Binding(name) => {
                // Match labels like `String:` are parsed as bindings but semantically act as
                // type-pattern labels. Treat PascalCase bindings as type labels, not locals.
                if is_ascii_pascal(name.as_str()) {
                    return;
                }
                self.check_snake("local", name, Some(fallback_span));
                let is_boolean = fn_types
                    .and_then(|info| info.local_types.get(name))
                    .is_some_and(type_is_boolean);
                if is_boolean && !starts_with_is_or_has(name.as_str()) {
                    self.errors.push(NamingError::BooleanPrefixRequired {
                        kind: "local",
                        name: name.clone(),
                        span: span_from_range(fallback_span),
                    });
                }
                let is_collection = fn_types
                    .and_then(|info| info.local_types.get(name))
                    .is_some_and(type_is_collection);
                if is_collection {
                    self.check_plural_collection_name("local", name, Some(fallback_span));
                }
            }
            Pattern::Path { args, .. } => {
                for arg in args {
                    self.check_pattern(arg, fallback_span, fn_types);
                }
            }
            Pattern::Struct { fields, .. } => {
                for (_name, value) in fields {
                    self.check_pattern(value, fallback_span, fn_types);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
        }
    }

    fn check_check_result_inline(
        &mut self,
        stmt_id: Idx<Stmt>,
        stmt_index: usize,
        name: &SmolStr,
        value: Idx<Expr>,
        body: &Body,
        block: &[Idx<Stmt>],
        usage_suffix: &HashMap<SmolStr, Vec<usize>>,
        fn_types: Option<&crate::hir::FunctionTypeInfo>,
    ) {
        let is_boolean_local = fn_types
            .and_then(|info| info.local_types.get(name))
            .is_some_and(type_is_boolean);
        if !is_boolean_local {
            return;
        }

        if !matches!(&body.exprs[value], Expr::Call { .. }) {
            return;
        }
        let Some(next_stmt) = block.get(stmt_index + 1).copied() else {
            return;
        };

        let condition_uses = match &body.stmts[next_stmt] {
            Stmt::If { condition, .. } | Stmt::While { condition, .. } => {
                count_name_in_expr(body, *condition, name)
            }
            Stmt::Match { subject, .. } => count_name_in_expr(body, *subject, name),
            _ => 0,
        };
        if condition_uses != 1 {
            return;
        }

        let total_uses_after = usage_suffix
            .get(name)
            .and_then(|counts| counts.get(stmt_index + 1))
            .copied()
            .unwrap_or(0);
        if total_uses_after == 1 {
            self.errors.push(NamingError::InlineCheckCondition {
                name: name.clone(),
                span: span_from_range(body.stmt_span(stmt_id)),
            });
        }
    }

    fn check_snake(&mut self, kind: &'static str, name: &SmolStr, span: Option<TextRange>) {
        if is_bypass_name(name.as_str()) {
            return;
        }
        if !is_ascii_snake(name.as_str()) {
            self.errors.push(NamingError::SnakeCaseRequired {
                kind,
                name: name.clone(),
                span: span_from_option(span),
            });
        }
    }

    fn check_pascal(&mut self, kind: &'static str, name: &SmolStr, span: Option<TextRange>) {
        if !is_ascii_pascal(name.as_str()) {
            self.errors.push(NamingError::PascalCaseRequired {
                kind,
                name: name.clone(),
                span: span_from_option(span),
            });
        }
    }

    fn check_plural_collection_name(
        &mut self,
        kind: &'static str,
        name: &SmolStr,
        span: Option<TextRange>,
    ) {
        if !is_plural_name(name.as_str()) {
            self.errors.push(NamingError::CollectionPluralityRequired {
                kind,
                name: name.clone(),
                expected_form: "plural",
                span: span_from_option(span),
            });
        }
    }

    fn check_singular_collection_binder_name(
        &mut self,
        kind: &'static str,
        name: &SmolStr,
        span: Option<TextRange>,
    ) {
        if is_plural_name(name.as_str()) {
            self.errors.push(NamingError::CollectionPluralityRequired {
                kind,
                name: name.clone(),
                expected_form: "singular",
                span: span_from_option(span),
            });
        }
    }
}

fn build_class_method_ids(module: &Module) -> HashSet<usize> {
    let mut out = HashSet::new();
    for (_, class) in module.classes.iter() {
        for method in &class.methods {
            out.insert(method.into_raw());
        }
    }
    out
}

fn build_usage_counts(body: &Body, block: &[Idx<Stmt>]) -> HashMap<SmolStr, Vec<usize>> {
    let mut names = HashSet::new();
    for stmt_id in block {
        collect_declared_names(body, *stmt_id, &mut names);
    }

    let mut out = HashMap::new();
    for name in names {
        let mut suffix = vec![0usize; block.len() + 1];
        for i in (0..block.len()).rev() {
            suffix[i] = suffix[i + 1] + count_name_in_stmt(body, block[i], &name);
        }
        out.insert(name, suffix);
    }
    out
}

fn collect_declared_names(body: &Body, stmt_id: Idx<Stmt>, out: &mut HashSet<SmolStr>) {
    match &body.stmts[stmt_id] {
        Stmt::Let { name, .. } | Stmt::Capture { name, .. } => {
            out.insert(name.clone());
        }
        Stmt::For {
            value_name,
            key_name,
            index_name,
            ..
        } => {
            out.insert(value_name.clone());
            if let Some(key_name) = key_name {
                out.insert(key_name.clone());
            }
            if let Some(index_name) = index_name {
                out.insert(index_name.clone());
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_declared_names(body, *stmt, out);
            }
            if let Some(branch) = else_branch {
                for stmt in branch {
                    collect_declared_names(body, *stmt, out);
                }
            }
        }
        Stmt::Optimize { body: inner, .. } | Stmt::While { body: inner, .. } => {
            for stmt in inner {
                collect_declared_names(body, *stmt, out);
            }
        }
        Stmt::Match {
            cases, otherwise, ..
        } => {
            for MatchCase {
                labels,
                body: inner,
                ..
            } in cases
            {
                for label in labels {
                    collect_pattern_bindings(label, out);
                }
                for stmt in inner {
                    collect_declared_names(body, *stmt, out);
                }
            }
            if let Some(branch) = otherwise {
                for stmt in branch {
                    collect_declared_names(body, *stmt, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_pattern_bindings(pattern: &Pattern, out: &mut HashSet<SmolStr>) {
    match pattern {
        Pattern::Binding(name) => {
            if is_ascii_pascal(name.as_str()) {
                return;
            }
            out.insert(name.clone());
        }
        Pattern::Path { args, .. } => {
            for arg in args {
                collect_pattern_bindings(arg, out);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (_name, value) in fields {
                collect_pattern_bindings(value, out);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

fn count_name_in_stmt(body: &Body, stmt_id: Idx<Stmt>, name: &SmolStr) -> usize {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) | Stmt::IgnoreResult { expr } | Stmt::Defer { expr } => {
            count_name_in_expr(body, *expr, name)
        }
        Stmt::Assert {
            expr,
            rhs,
            tolerance,
            ..
        } => {
            count_name_in_expr(body, *expr, name)
                + rhs
                    .map(|rhs| count_name_in_expr(body, rhs, name))
                    .unwrap_or(0)
                + tolerance
                    .map(|tolerance| count_name_in_expr(body, tolerance, name))
                    .unwrap_or(0)
        }
        Stmt::Require { condition, message } => {
            count_name_in_expr(body, *condition, name) + count_name_in_expr(body, *message, name)
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Capture { value, .. } => {
            count_name_in_expr(body, *value, name)
        }
        Stmt::Optimize { body: inner, .. } => inner
            .iter()
            .map(|stmt| count_name_in_stmt(body, *stmt, name))
            .sum(),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut total = count_name_in_expr(body, *condition, name)
                + then_branch
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>();
            if let Some(branch) = else_branch {
                total += branch
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>();
            }
            total
        }
        Stmt::For {
            iterable,
            body: inner,
            ..
        } => {
            count_name_in_expr(body, *iterable, name)
                + inner
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>()
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            let mut total = count_name_in_expr(body, *subject, name);
            for case in cases {
                if let Some(guard) = case.guard {
                    total += count_name_in_expr(body, guard, name);
                }
                total += case
                    .body
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>();
            }
            if let Some(branch) = otherwise {
                total += branch
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>();
            }
            total
        }
        Stmt::While {
            condition,
            body: inner,
        } => {
            count_name_in_expr(body, *condition, name)
                + inner
                    .iter()
                    .map(|stmt| count_name_in_stmt(body, *stmt, name))
                    .sum::<usize>()
        }
        Stmt::Return(expr) => expr
            .as_ref()
            .map(|expr| count_name_in_expr(body, *expr, name))
            .unwrap_or(0),
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => 0,
    }
}

fn count_name_in_expr(body: &Body, expr_id: Idx<Expr>, name: &SmolStr) -> usize {
    match &body.exprs[expr_id] {
        Expr::Variable(var) => usize::from(var == name),
        Expr::Literal(_) => 0,
        Expr::Detach { target, .. } => count_name_in_expr(body, *target, name),
        Expr::Binary { lhs, rhs, .. } => {
            count_name_in_expr(body, *lhs, name) + count_name_in_expr(body, *rhs, name)
        }
        Expr::Unary { expr, .. } => count_name_in_expr(body, *expr, name),
        Expr::TypeApply { callee, .. } => count_name_in_expr(body, *callee, name),
        Expr::Crash { expr } => count_name_in_expr(body, *expr, name),
        Expr::Call { callee, args, .. } => {
            let mut total = count_name_in_expr(body, *callee, name);
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. }
                    | crate::hir::Arg::Named { value, .. } => {
                        total += count_name_in_expr(body, *value, name);
                    }
                }
            }
            total
        }
        Expr::Member { object, .. } => count_name_in_expr(body, *object, name),
        Expr::Index { object, index, .. } => {
            count_name_in_expr(body, *object, name) + count_name_in_expr(body, *index, name)
        }
        Expr::List(items) => items
            .iter()
            .map(|item| count_name_in_expr(body, *item, name))
            .sum(),
        Expr::Map(items) => items
            .iter()
            .map(|(k, v)| count_name_in_expr(body, *k, name) + count_name_in_expr(body, *v, name))
            .sum(),
        Expr::StringInterp(parts) => parts
            .iter()
            .map(|part| match part {
                crate::hir::StringPart::Expr(expr) => count_name_in_expr(body, *expr, name),
                crate::hir::StringPart::Literal(_) => 0,
            })
            .sum(),
        Expr::Closure {
            params,
            body: closure_body,
        } => {
            // Don't count the name if it's shadowed by a closure param
            if params.iter().any(|p| &p.name == name) {
                0
            } else {
                count_name_in_expr(body, *closure_body, name)
            }
        }
    }
}

fn factory_return_kind(ty: &TypeRef, class_names: &HashSet<SmolStr>) -> Option<bool> {
    if class_names.contains(&ty.name) {
        return Some(false);
    }
    if ty.name == "Result" && ty.args.len() == 2 && class_names.contains(&ty.args[0].name) {
        return Some(true);
    }
    if ty.name == "Pending" && ty.args.len() == 1 {
        return factory_return_kind(&ty.args[0], class_names);
    }
    None
}

fn function_kind_label(kind: FunctionKind) -> &'static str {
    match kind {
        FunctionKind::Function => "function",
        FunctionKind::Method => "method",
        FunctionKind::Derived => "derived",
        FunctionKind::Check => "check",
        FunctionKind::CheckMethod => "check",
    }
}

fn starts_with_is_or_has(name: &str) -> bool {
    name.starts_with("is_") || name.starts_with("has_")
}

fn is_bypass_name(name: &str) -> bool {
    matches!(name, "main" | "__configure__")
}

fn is_ascii_snake(name: &str) -> bool {
    if name.is_empty() || !name.is_ascii() {
        return false;
    }
    if name.starts_with('_') || name.ends_with('_') || name.contains("__") {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_ascii_pascal(name: &str) -> bool {
    if name.is_empty() || !name.is_ascii() || name.contains('_') {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric())
}

fn is_verb_led(name: &str) -> bool {
    let head = name.split('_').next().unwrap_or(name);
    matches!(
        head,
        "abort"
            | "add"
            | "apply"
            | "begin"
            | "build"
            | "calculate"
            | "check"
            | "close"
            | "collect"
            | "compile"
            | "compute"
            | "commit"
            | "convert"
            | "create"
            | "debug"
            | "delete"
            | "deregister"
            | "decode"
            | "drop"
            | "emit"
            | "encode"
            | "ensure"
            | "error"
            | "fetch"
            | "find"
            | "generate"
            | "get"
            | "greet"
            | "handle"
            | "has"
            | "init"
            | "initialize"
            | "info"
            | "install"
            | "is"
            | "list"
            | "load"
            | "make"
            | "mark"
            | "map"
            | "merge"
            | "notify"
            | "open"
            | "pause"
            | "parse"
            | "print"
            | "prepare"
            | "process"
            | "put"
            | "push"
            | "queue"
            | "read"
            | "record"
            | "refill"
            | "register"
            | "rename"
            | "render"
            | "restore"
            | "resume"
            | "return"
            | "run"
            | "save"
            | "scan"
            | "schedule"
            | "send"
            | "set"
            | "show"
            | "sleep"
            | "snapshot"
            | "start"
            | "stop"
            | "store"
            | "sync"
            | "test"
            | "transform"
            | "try"
            | "unpark"
            | "update"
            | "validate"
            | "warn"
            | "write"
            | "arm"
            | "choose"
            | "clamp"
            | "digit"
            | "example"
            | "normalize"
            | "objective"
            | "round"
            | "count"
            | "size"
            | "strip"
            | "trim"
            | "bump"
    )
}

fn module_segment_is_nounish(segment: &str) -> bool {
    if starts_with_is_or_has(segment)
        || segment.starts_with("try_to_")
        || segment.starts_with("create_")
        || segment.starts_with("to_")
        || segment.starts_with("can_")
        || segment.starts_with("check_")
    {
        return false;
    }
    if is_verb_led(segment) && !MODULE_SEGMENT_NOUN_EXCEPTIONS.contains(&segment) {
        return false;
    }
    true
}

const MODULE_SEGMENT_NOUN_EXCEPTIONS: &[&str] = &[
    "parse", "runtime", "data", "host", "core", "env", "fs", "pool", "task", "log",
];

fn type_ref_is_result_like(ty: &TypeRef) -> bool {
    if ty.name == "Result" && ty.args.len() == 2 {
        return true;
    }
    if ty.name == "Pending" && ty.args.len() == 1 {
        return type_ref_is_result_like(&ty.args[0]);
    }
    false
}

fn type_ref_is_boolean(ty: &TypeRef) -> bool {
    ty.name == "Boolean" && ty.args.is_empty()
}

fn type_is_boolean(ty: &Type) -> bool {
    matches!(ty, Type::Boolean)
}

fn type_ref_is_collection_like(ty: &TypeRef) -> bool {
    if ty.name == "List" && ty.args.len() == 1 {
        return true;
    }
    if ty.name == "Map" && ty.args.len() == 2 {
        return true;
    }
    if ty.name == "Pending" && ty.args.len() == 1 {
        return type_ref_is_collection_like(&ty.args[0]);
    }
    false
}

fn type_is_collection(ty: &Type) -> bool {
    matches!(ty, Type::List(_) | Type::Map(_, _))
}

fn is_plural_name(name: &str) -> bool {
    let tail = name.rsplit('_').next().unwrap_or(name);
    if tail.is_empty() {
        return false;
    }
    if IRREGULAR_PLURALS.contains(&tail) {
        return true;
    }
    if SINGULAR_ENDS_WITH_S.contains(&tail) {
        return false;
    }
    if tail.ends_with("ies") && tail.len() > 3 {
        return true;
    }
    if tail.ends_with("ses") || tail.ends_with("xes") || tail.ends_with("zes") {
        return true;
    }
    tail.ends_with('s') && !tail.ends_with("ss")
}

const IRREGULAR_PLURALS: &[&str] = &[
    "children", "people", "men", "women", "teeth", "feet", "geese", "data", "indices",
];

const SINGULAR_ENDS_WITH_S: &[&str] = &[
    "status", "analysis", "basis", "thesis", "axis", "class", "glass", "bus",
];

fn span_from_option(span: Option<TextRange>) -> SourceSpan {
    span.map(span_from_range)
        .unwrap_or_else(|| SourceSpan::from((0usize, 0usize)))
}

fn span_from_range(range: TextRange) -> SourceSpan {
    let start: usize = range.start().into();
    let len: usize = range.len().into();
    SourceSpan::from((start, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::hir::typeck;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    fn naming_errors(source: &str) -> Vec<NamingError> {
        let node = parse(source);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        check_module(&module, &type_info)
    }

    #[test]
    fn enforces_function_snake_and_verb() {
        let errors = naming_errors("fn helperThing() -> Integer { return 1 }\n");
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, NamingError::SnakeCaseRequired { kind, name, .. } if *kind == "function" && name == "helperThing"))
        );
    }

    #[test]
    fn enforces_material_snake_and_noun_only() {
        let errors = naming_errors(
            "material render_surface(hit: Hit3) -> Surface {\n    return Surface(albedo=vec3(0.2, 0.4, 0.6), roughness=0.5, metalness=0.1, clearcoat=0.25, clearcoat_roughness=0.3, sheen=0.15, emissive=vec3(1.0, 0.0, 0.0))\n}\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::NounOnlyRequired { kind, name, .. }
                    if *kind == "material" && name == "render_surface"
            )
        }));
    }

    #[test]
    fn top_level_boolean_fn_no_longer_requires_check_shape() {
        let errors = naming_errors("fn is_ready(value: Integer) -> Boolean { return value > 0 }\n");
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, NamingError::TopLevelCheckName { .. }))
        );
    }

    #[test]
    fn enforces_result_prefix_and_error_type_suffix() {
        let errors = naming_errors(
            "fn fetch_user() -> Result[User, network_failure] { return error network_failure }\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::ResultPrefixRequired { name, .. } if name == "fetch_user"
            )
        }));
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::ResultErrorTypeShape { name, .. } if name == "network_failure"
            )
        }));
    }

    #[test]
    fn enforces_factory_prefixes() {
        let errors = naming_errors(
            "class User {\n    id: Integer\n}\n\n\
             fn build_user() -> User { return User(id=1) }\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::FactoryPrefixRequired { name, required, .. }
                    if name == "build_user" && *required == "create_"
            )
        }));
    }

    #[test]
    fn enforces_boolean_prefix_for_param_and_local() {
        let errors = naming_errors(
            "fn run_check(ready: Boolean) -> Integer {\n    flag = true\n    if flag { return 1 }\n    return 0\n}\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::BooleanPrefixRequired { kind, name, .. }
                    if *kind == "parameter" && name == "ready"
            )
        }));
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::BooleanPrefixRequired { kind, name, .. }
                    if *kind == "local" && name == "flag"
            )
        }));
    }

    #[test]
    fn flags_immediate_single_use_check_storage() {
        let errors = naming_errors(
            "fn account_is_ready(v: Integer) -> Boolean { return v > 0 }\n\n\
             fn run() -> Integer {\n    is_ready = account_is_ready(v=1)\n    if is_ready { return 1 }\n    return 0\n}\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(err, NamingError::InlineCheckCondition { name, .. } if name == "is_ready")
        }));
    }

    #[test]
    fn flags_immediate_single_use_check_storage_with_normal_call() {
        let errors = naming_errors(
            "fn account_is_ready(v: Integer) -> Boolean { return v > 0 }\n\n\
             fn run() -> Integer {\n    is_ready = account_is_ready(v=1)\n    if is_ready { return 1 }\n    return 0\n}\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(err, NamingError::InlineCheckCondition { name, .. } if name == "is_ready")
        }));
    }

    #[test]
    fn does_not_flag_non_boolean_local_used_in_condition() {
        let errors = naming_errors(
            "fn run() -> Integer {\n    seed = __wr_env_get(\"WRELA_MODEL_SEED\")\n    if seed == \"9\" { return 1 }\n    return 0\n}\n",
        );
        assert!(!errors.iter().any(|err| {
            matches!(err, NamingError::InlineCheckCondition { name, .. } if name == "seed")
        }));
    }

    #[test]
    fn bypass_allows_main_and_configure_names() {
        let errors = naming_errors(
            "fn main() -> Integer { return 0 }\n\n\
             class Logger {\n    fn __configure__() -> Nothing { return }\n}\n",
        );
        assert!(!errors
            .iter()
            .any(|err| matches!(err, NamingError::VerbLedRequired { name, .. } if name == "main" || name == "__configure__")));
    }

    #[test]
    fn enforces_module_segment_semantics() {
        let errors =
            naming_errors("use foo from create/users\n\nfn run() -> Integer { return 1 }\n");
        assert!(errors.iter().any(|err| {
            matches!(err, NamingError::ModuleSemanticRequired { name, .. } if name == "create")
        }));
    }

    #[test]
    fn enforces_collection_plurality_for_field_param_local_and_binder() {
        let errors = naming_errors(
            "class Bucket {\n    item: List[Integer]\n}\n\n\
             fn run(item: List[Integer]) -> Integer {\n    user = [1, 2, 3]\n    for items in user { return items }\n    return 0\n}\n",
        );
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::CollectionPluralityRequired { kind, name, expected_form, .. }
                    if *kind == "field" && name == "item" && *expected_form == "plural"
            )
        }));
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::CollectionPluralityRequired { kind, name, expected_form, .. }
                    if *kind == "parameter" && name == "item" && *expected_form == "plural"
            )
        }));
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::CollectionPluralityRequired { kind, name, expected_form, .. }
                    if *kind == "local" && name == "user" && *expected_form == "plural"
            )
        }));
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                NamingError::CollectionPluralityRequired { kind, name, expected_form, .. }
                    if *kind == "loop binder" && name == "items" && *expected_form == "singular"
            )
        }));
    }

    #[test]
    fn naming_pass_performance_budget_smoke() {
        use std::time::{Duration, Instant};

        let mut source = String::new();
        source.push_str("class User {\n    ids: List[Integer]\n}\n\n");
        for index in 0..300 {
            source.push_str(&format!(
                "fn create_user_{index}() -> User {{\n    return User(ids=[1, 2, 3])\n}}\n\n",
            ));
        }
        source.push_str("fn run() -> Integer {\n    users = [1, 2, 3]\n    for user in users {\n        if user > 100 { return user }\n    }\n    return 0\n}\n");

        let node = parse(&source);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let type_start = Instant::now();
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let type_elapsed = type_start.elapsed();

        let naming_start = Instant::now();
        let _naming_errors = check_module(&module, &type_info);
        let naming_elapsed = naming_start.elapsed();

        let budget = type_elapsed
            .saturating_mul(8)
            .saturating_add(Duration::from_millis(50));
        assert!(
            naming_elapsed <= budget,
            "naming pass budget exceeded: naming={:?}, typeck={:?}, budget={:?}",
            naming_elapsed,
            type_elapsed,
            budget
        );
    }
}
