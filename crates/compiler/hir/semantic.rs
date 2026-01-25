#![allow(unused_assignments)]

use crate::hir::{Arg, Body, Class, Expr, Function, Idx, MatchCase, Module, Stmt, UnaryOp};
use miette::{Diagnostic, SourceSpan};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum SemanticError {
    #[error("duplicate {kind} '{name}'")]
    #[diagnostic(
        code(lang::sem::duplicate_definition),
        help("Choose a different name or remove the earlier definition.")
    )]
    DuplicateDefinition {
        name: SmolStr,
        kind: &'static str,
        #[label("redefined here")]
        span: SourceSpan,
        #[label("previous definition here")]
        previous: Option<SourceSpan>,
    },

    #[error("undefined name '{name}'")]
    #[diagnostic(
        code(lang::sem::undefined_name),
        help("Declare it first or check for a typo.")
    )]
    UndefinedName {
        name: SmolStr,
        #[label("not found in scope")]
        span: SourceSpan,
    },

    #[error("cannot assign to immutable variable '{name}'")]
    #[diagnostic(
        code(lang::sem::immutable_assign),
        help("Add 'changing' to make this variable mutable.")
    )]
    ImmutableAssign {
        name: SmolStr,
        #[label("assignment here")]
        span: SourceSpan,
        #[label("defined here")]
        definition: Option<SourceSpan>,
    },

    #[error("cannot assign to {kind} '{name}'")]
    #[diagnostic(
        code(lang::sem::invalid_assign_target),
        help("Assign to a variable instead.")
    )]
    InvalidAssignTarget {
        name: SmolStr,
        kind: &'static str,
        #[label("assignment here")]
        span: SourceSpan,
        #[label("defined here")]
        definition: Option<SourceSpan>,
    },

    #[error("break outside of a loop")]
    #[diagnostic(
        code(lang::sem::break_outside_loop),
        help("Move this 'break' inside a loop.")
    )]
    BreakOutsideLoop {
        #[label("break here")]
        span: SourceSpan,
    },

    #[error("continue outside of a loop")]
    #[diagnostic(
        code(lang::sem::continue_outside_loop),
        help("Move this 'continue' inside a loop.")
    )]
    ContinueOutsideLoop {
        #[label("continue here")]
        span: SourceSpan,
    },

    #[error("fire is only valid as a standalone statement")]
    #[diagnostic(
        code(lang::sem::fire_in_expression),
        help("Use `fire` as its own statement.")
    )]
    FireInExpression {
        #[label("fire used here")]
        span: SourceSpan,
    },

    #[error("positional arguments cannot appear after named arguments")]
    #[diagnostic(
        code(lang::sem::positional_after_named),
        help("Move positional arguments before the first named argument.")
    )]
    PositionalAfterNamed {
        #[label("argument here")]
        span: SourceSpan,
    },

    #[error("duplicate named argument '{name}'")]
    #[diagnostic(
        code(lang::sem::duplicate_named_arg),
        help("Remove or rename the duplicate argument.")
    )]
    DuplicateNamedArg {
        name: SmolStr,
        #[label("duplicate here")]
        span: SourceSpan,
    },

    #[error("name '{name}' shadows an outer definition")]
    #[diagnostic(
        code(lang::sem::shadowed_name),
        help("Rename this binding to avoid shadowing.")
    )]
    ShadowedName {
        name: SmolStr,
        #[label("shadows this binding")]
        span: SourceSpan,
        #[label("previous definition here")]
        previous: Option<SourceSpan>,
    },

    #[error("it is only valid in return statements")]
    #[diagnostic(
        code(lang::sem::invalid_it_usage),
        help("Move this expression inside a return statement.")
    )]
    InvalidItUsage {
        #[label("it used here")]
        span: SourceSpan,
    },

    #[error("visibility modifier is not valid here")]
    #[diagnostic(
        code(lang::sem::visibility_misuse),
        help("Remove the visibility modifier.")
    )]
    VisibilityMisuse {
        #[label("visibility here")]
        span: SourceSpan,
    },

    #[error("public variables cannot be changing")]
    #[diagnostic(
        code(lang::sem::public_changing_variable),
        help("Remove 'changing' or make the variable private.")
    )]
    PublicChangingVariable {
        #[label("variable here")]
        span: SourceSpan,
    },

    #[error("match requires an otherwise case")]
    #[diagnostic(
        code(lang::sem::match_missing_otherwise),
        help("Add an `otherwise:` case to this match.")
    )]
    MatchMissingOtherwise {
        #[label("match here")]
        span: SourceSpan,
    },
}

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum SemanticWarning {
    #[error("method '{name}' conflicts with a field of the same name")]
    #[diagnostic(
        code(lang::sem::method_field_conflict),
        help("Rename either the method or the field.")
    )]
    MethodFieldNameConflict {
        name: SmolStr,
        #[label("conflict here")]
        span: SourceSpan,
    },

    #[error("unreachable code")]
    #[diagnostic(
        code(lang::sem::unreachable_code),
        help("Remove this code or move it before the terminating statement.")
    )]
    UnreachableCode {
        #[label("unreachable statement")]
        span: SourceSpan,
    },

    #[error("unused {kind} '{name}'")]
    #[diagnostic(
        code(lang::sem::unused_binding),
        help("Remove it or use it in this scope.")
    )]
    UnusedBinding {
        name: SmolStr,
        kind: &'static str,
        #[label("unused here")]
        span: SourceSpan,
    },
}

impl SemanticError {
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            SemanticError::DuplicateDefinition { span, .. } => *span,
            SemanticError::UndefinedName { span, .. } => *span,
            SemanticError::ImmutableAssign { span, .. } => *span,
            SemanticError::InvalidAssignTarget { span, .. } => *span,
            SemanticError::BreakOutsideLoop { span } => *span,
            SemanticError::ContinueOutsideLoop { span } => *span,
            SemanticError::InvalidItUsage { span } => *span,
            SemanticError::ShadowedName { span, .. } => *span,
            SemanticError::FireInExpression { span } => *span,
            SemanticError::VisibilityMisuse { span, .. } => *span,
            SemanticError::PublicChangingVariable { span } => *span,
            SemanticError::DuplicateNamedArg { span, .. } => *span,
            SemanticError::PositionalAfterNamed { span } => *span,
            SemanticError::MatchMissingOtherwise { span } => *span,
        }
    }
}

impl SemanticWarning {
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            SemanticWarning::MethodFieldNameConflict { span, .. } => *span,
            SemanticWarning::UnreachableCode { span } => *span,
            SemanticWarning::UnusedBinding { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum BindingKind {
    Function,
    Class,
    Method,
    Field,
    Param,
    Local,
    Use,
    LoopVar,
    Implicit,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    kind: BindingKind,
    span: Option<TextRange>,
    used: bool,
}

#[derive(Default)]
struct Scope {
    bindings: HashMap<SmolStr, Binding>,
}

struct Checker<'a> {
    module: &'a Module,
    errors: Vec<SemanticError>,
    warnings: Vec<SemanticWarning>,
    scopes: Vec<Scope>,
    loop_depth: usize,
    method_ids: HashSet<usize>,
    in_method: bool,
}

pub struct SemanticDiagnostics {
    pub errors: Vec<SemanticError>,
    pub warnings: Vec<SemanticWarning>,
}

pub fn check_module(module: &Module) -> SemanticDiagnostics {
    let mut checker = Checker::new(module);
    checker.check_module();
    SemanticDiagnostics {
        errors: checker.errors,
        warnings: checker.warnings,
    }
}

impl<'a> Checker<'a> {
    fn new(module: &'a Module) -> Self {
        let mut method_ids = HashSet::new();
        for class in module.classes.iter().map(|(_, c)| c) {
            for method in &class.methods {
                method_ids.insert(method.into_raw());
            }
        }

        Self {
            module,
            errors: Vec::new(),
            warnings: Vec::new(),
            scopes: vec![Scope::default()],
            loop_depth: 0,
            method_ids,
            in_method: false,
        }
    }

    fn check_module(&mut self) {
        for (name, kind) in builtin_bindings() {
            self.declare(
                name,
                Binding {
                    mutable: false,
                    kind,
                    span: None,
                    used: true,
                },
            );
        }

        for (idx, func) in self.module.functions.iter() {
            if self.method_ids.contains(&idx.into_raw()) {
                continue;
            }
            self.declare(
                func.name.clone(),
                Binding {
                    mutable: false,
                    kind: BindingKind::Function,
                    span: func.name_span,
                    used: true,
                },
            );
        }

        for (_idx, class) in self.module.classes.iter() {
            self.declare(
                class.name.clone(),
                Binding {
                    mutable: false,
                    kind: BindingKind::Class,
                    span: class.name_span,
                    used: true,
                },
            );
        }

        for (_idx, class) in self.module.classes.iter() {
            self.check_class(class);
        }

        for (idx, func) in self.module.functions.iter() {
            if self.method_ids.contains(&idx.into_raw()) {
                continue;
            }
            self.check_function(func, false);
        }
    }

    fn check_class(&mut self, class: &Class) {
        let mut field_names = HashMap::new();
        for field in &class.fields {
            if let Some(prev) = field_names.insert(field.name.clone(), field.name_span) {
                self.errors.push(SemanticError::DuplicateDefinition {
                    name: field.name.clone(),
                    kind: "field",
                    span: span_from_option(field.name_span),
                    previous: Some(span_from_option(prev)),
                });
            }
        }

        let mut method_names = HashMap::new();
        for method_id in &class.methods {
            let method = &self.module.functions[*method_id];
            if let Some(prev) = method_names.insert(method.name.clone(), method.name_span) {
                self.errors.push(SemanticError::DuplicateDefinition {
                    name: method.name.clone(),
                    kind: "method",
                    span: span_from_option(method.name_span),
                    previous: Some(span_from_option(prev)),
                });
            }
            if field_names.contains_key(&method.name) {
                self.warnings.push(SemanticWarning::MethodFieldNameConflict {
                    name: method.name.clone(),
                    span: span_from_option(method.name_span),
                });
            }
            self.check_function(method, true);
        }
    }

    fn check_function(&mut self, func: &Function, is_method: bool) {
        let prev_method = self.in_method;
        self.in_method = is_method;
        self.enter_scope();
        for param in &func.params {
            self.declare(
                param.name.clone(),
                Binding {
                    mutable: false,
                    kind: BindingKind::Param,
                    span: param.name_span,
                    used: false,
                },
            );
        }
        if let Some(body) = &func.body {
            self.check_block(body, &body.root_stmts);
        }
        self.exit_scope();
        self.in_method = prev_method;
    }

    fn check_stmt(&mut self, body: &Body, stmt_id: Idx<Stmt>) {
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::Expr(expr) => self.check_expr_with_ctx(body, *expr, false, true),
            Stmt::Let {
                name,
                value,
                mutable,
                visibility,
            } => {
                self.check_expr_with_ctx(body, *value, false, false);
                let span = body.stmt_span(stmt_id);
                if visibility.is_some() {
                    self.errors.push(SemanticError::VisibilityMisuse {
                        span: span_from_range(span),
                    });
                }
                if matches!(visibility, Some(crate::hir::Visibility::Public)) && *mutable {
                    self.errors.push(SemanticError::PublicChangingVariable {
                        span: span_from_range(span),
                    });
                }
                self.declare(
                    name.clone(),
                    Binding {
                        mutable: *mutable,
                        kind: BindingKind::Local,
                        span: Some(span),
                        used: false,
                    },
                );
            }
            Stmt::Assign { name, value, .. } => {
                self.check_expr_with_ctx(body, *value, false, false);
                let span = body.stmt_span(stmt_id);
                if let Stmt::Assign { visibility, .. } = stmt {
                    if visibility.is_some() {
                        self.errors.push(SemanticError::VisibilityMisuse {
                            span: span_from_range(span),
                        });
                    }
                    if matches!(visibility, Some(crate::hir::Visibility::Public)) {
                        self.errors.push(SemanticError::PublicChangingVariable {
                            span: span_from_range(span),
                        });
                    }
                }
                match self.resolve(name) {
                    Some(binding) => match binding.kind {
                        BindingKind::Local | BindingKind::LoopVar => {
                            if !binding.mutable {
                                self.errors.push(SemanticError::ImmutableAssign {
                                    name: name.clone(),
                                    span: span_from_range(span),
                                    definition: binding.span.map(span_from_range),
                                });
                            }
                        }
                        BindingKind::Param
                        | BindingKind::Function
                        | BindingKind::Class
                        | BindingKind::Method
                        | BindingKind::Field
                        | BindingKind::Use
                        | BindingKind::Implicit => {
                            self.errors.push(SemanticError::InvalidAssignTarget {
                                name: name.clone(),
                                kind: binding_kind_label(binding.kind),
                                span: span_from_range(span),
                                definition: binding.span.map(span_from_range),
                            });
                        }
                    },
                    None => {
                        self.errors.push(SemanticError::UndefinedName {
                            name: name.clone(),
                            span: span_from_range(span),
                        });
                    }
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr_with_ctx(body, *condition, false, false);
                self.enter_scope();
                for stmt in then_branch {
                    self.check_stmt(body, *stmt);
                }
                self.exit_scope();
                if let Some(branch) = else_branch {
                    self.enter_scope();
                    for stmt in branch {
                        self.check_stmt(body, *stmt);
                    }
                    self.exit_scope();
                }
            }
            Stmt::For {
                name,
                iterable,
                body: loop_body,
            } => {
                self.check_expr_with_ctx(body, *iterable, false, false);
                self.enter_scope();
                let span = body.stmt_span(stmt_id);
                self.declare(
                    name.clone(),
                    Binding {
                        mutable: false,
                        kind: BindingKind::LoopVar,
                        span: Some(span),
                        used: false,
                    },
                );
                self.loop_depth += 1;
                self.check_block(body, loop_body);
                self.loop_depth -= 1;
                self.exit_scope();
            }
            Stmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                self.check_expr_with_ctx(body, *subject, false, false);
                if otherwise.is_none() {
                    self.errors.push(SemanticError::MatchMissingOtherwise {
                        span: span_from_range(body.stmt_span(stmt_id)),
                    });
                }
                for case in cases {
                    self.check_match_case(body, case);
                }
                if let Some(branch) = otherwise {
                    self.enter_scope();
                    self.check_block(body, branch);
                    self.exit_scope();
                }
            }
            Stmt::Use { names, .. } => {
                for use_name in names {
                    if let Some(name) = use_name.name() {
                        self.declare(
                            name.clone(),
                            Binding {
                                mutable: false,
                                kind: BindingKind::Use,
                                span: Some(use_name.span),
                                used: false,
                            },
                        );
                    }
                }
            }
            Stmt::While { condition, body: loop_body } => {
                self.check_expr_with_ctx(body, *condition, false, false);
                self.enter_scope();
                self.loop_depth += 1;
                self.check_block(body, loop_body);
                self.loop_depth -= 1;
                self.exit_scope();
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.check_expr_with_ctx(body, *expr, true, false);
                }
            }
            Stmt::Break => {
                if self.loop_depth == 0 {
                    self.errors.push(SemanticError::BreakOutsideLoop {
                        span: span_from_range(body.stmt_span(stmt_id)),
                    });
                }
            }
            Stmt::Continue => {
                if self.loop_depth == 0 {
                    self.errors.push(SemanticError::ContinueOutsideLoop {
                        span: span_from_range(body.stmt_span(stmt_id)),
                    });
                }
            }
        }
    }

    fn check_match_case(&mut self, body: &Body, case: &MatchCase) {
        self.enter_scope();
        for label in &case.labels {
            self.check_expr_with_ctx(body, *label, false, false);
        }
        self.check_block(body, &case.body);
        self.exit_scope();
    }

    fn check_expr_with_ctx(
        &mut self,
        body: &Body,
        expr_id: Idx<Expr>,
        allow_it: bool,
        allow_fire: bool,
    ) {
        let expr = &body.exprs[expr_id];
        match expr {
            Expr::Literal(_) => {}
            Expr::Variable(name) => {
                if name == "it" {
                    if allow_it || self.in_method {
                        return;
                    }
                    self.errors.push(SemanticError::InvalidItUsage {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                    return;
                }
                let span = body.expr_span(expr_id);
                if self.resolve(name).is_none() {
                    self.errors.push(SemanticError::UndefinedName {
                        name: name.clone(),
                        span: span_from_range(span),
                    });
                } else {
                    self.mark_used(name);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_with_ctx(body, *lhs, allow_it, false);
                self.check_expr_with_ctx(body, *rhs, allow_it, false);
            }
            Expr::Unary { op, expr, .. } => {
                if matches!(op, UnaryOp::Fire) && !allow_fire {
                    self.errors.push(SemanticError::FireInExpression {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                }
                self.check_expr_with_ctx(body, *expr, allow_it, false);
            }
            Expr::Call { callee, args } => {
                self.check_expr_with_ctx(body, *callee, allow_it, false);
                let mut seen_named = false;
                let mut named_args = HashSet::new();
                for arg in args {
                    match arg {
                        Arg::Positional { value, span } => {
                            if seen_named {
                                self.errors.push(SemanticError::PositionalAfterNamed {
                                    span: span_from_range(*span),
                                });
                            }
                            self.check_expr_with_ctx(body, *value, allow_it, false);
                        }
                        Arg::Named {
                            name,
                            value,
                            span: _,
                            name_span,
                        } => {
                            if !named_args.insert(name.clone()) {
                                self.errors.push(SemanticError::DuplicateNamedArg {
                                    name: name.clone(),
                                    span: span_from_range(*name_span),
                                });
                            }
                            seen_named = true;
                            self.check_expr_with_ctx(body, *value, allow_it, false);
                        }
                    }
                }
            }
            Expr::Member { object, .. } => self.check_expr_with_ctx(body, *object, allow_it, false),
            Expr::List(items) => {
                for item in items {
                    self.check_expr_with_ctx(body, *item, allow_it, false);
                }
            }
            Expr::Map(items) => {
                for (key, value) in items {
                    self.check_expr_with_ctx(body, *key, allow_it, false);
                    self.check_expr_with_ctx(body, *value, allow_it, false);
                }
            }
            Expr::StringInterp(parts) => {
                for part in parts {
                    if let crate::hir::StringPart::Expr(expr) = part {
                        self.check_expr_with_ctx(body, *expr, allow_it, false);
                    }
                }
            }
            Expr::Crash { expr } => {
                self.check_expr_with_ctx(body, *expr, allow_it, false);
            }
        }
    }

    fn declare(&mut self, name: SmolStr, binding: Binding) {
        if should_check_shadowing(binding.kind) {
            if let Some(previous) = self.resolve_in_outer(&name) {
                self.errors.push(SemanticError::ShadowedName {
                    name: name.clone(),
                    span: span_from_option(binding.span),
                    previous: previous.span.map(span_from_range),
                });
            }
        }
        let scope = match self.scopes.last_mut() {
            Some(scope) => scope,
            None => return,
        };
        if let Some(previous) = scope.bindings.get(&name) {
            self.errors.push(SemanticError::DuplicateDefinition {
                name,
                kind: binding_kind_label(binding.kind),
                span: span_from_option(binding.span),
                previous: previous.span.map(span_from_range),
            });
        } else {
            scope.bindings.insert(name, binding);
        }
    }

    fn resolve(&self, name: &SmolStr) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn resolve_in_outer(&self, name: &SmolStr) -> Option<&Binding> {
        if self.scopes.len() <= 1 {
            return None;
        }
        for scope in self.scopes.iter().rev().skip(1) {
            if let Some(binding) = scope.bindings.get(name) {
                return Some(binding);
            }
        }
        None
    }

    fn enter_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn exit_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for (name, binding) in scope.bindings {
                if binding.used {
                    continue;
                }
                if matches!(binding.kind, BindingKind::Local | BindingKind::Use) {
                    self.warnings.push(SemanticWarning::UnusedBinding {
                        name,
                        kind: unused_kind_label(binding.kind),
                        span: span_from_option(binding.span),
                    });
                }
            }
        }
    }

    fn mark_used(&mut self, name: &SmolStr) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.bindings.get_mut(name) {
                binding.used = true;
                return;
            }
        }
    }

    fn check_block(&mut self, body: &Body, stmts: &[Idx<Stmt>]) {
        let mut terminated = false;
        for stmt in stmts {
            if terminated {
                self.warnings.push(SemanticWarning::UnreachableCode {
                    span: span_from_range(body.stmt_span(*stmt)),
                });
            }
            self.check_stmt(body, *stmt);
            if matches!(
                body.stmts[*stmt],
                Stmt::Return(_) | Stmt::Break | Stmt::Continue
            ) {
                terminated = true;
            }
        }
    }
}

fn span_from_option(range: Option<TextRange>) -> SourceSpan {
    range
        .map(span_from_range)
        .unwrap_or_else(|| SourceSpan::from((0usize, 0usize)))
}

fn span_from_range(range: TextRange) -> SourceSpan {
    let start: usize = range.start().into();
    let len: usize = range.len().into();
    SourceSpan::from((start, len))
}

fn binding_kind_label(kind: BindingKind) -> &'static str {
    match kind {
        BindingKind::Function => "function",
        BindingKind::Class => "class",
        BindingKind::Method => "method",
        BindingKind::Field => "field",
        BindingKind::Param => "parameter",
        BindingKind::Local => "variable",
        BindingKind::Use => "import",
        BindingKind::LoopVar => "loop variable",
        BindingKind::Implicit => "name",
    }
}

fn should_check_shadowing(kind: BindingKind) -> bool {
    matches!(
        kind,
        BindingKind::Local | BindingKind::LoopVar | BindingKind::Param | BindingKind::Use
    )
}

fn unused_kind_label(kind: BindingKind) -> &'static str {
    match kind {
        BindingKind::Use => "import",
        _ => "variable",
    }
}

fn builtin_bindings() -> Vec<(SmolStr, BindingKind)> {
    vec![
        (SmolStr::new("print"), BindingKind::Function),
        (SmolStr::new("parse_int"), BindingKind::Function),
        (SmolStr::new("parse_float"), BindingKind::Function),
        (SmolStr::new("read_file"), BindingKind::Function),
        (SmolStr::new("write_file"), BindingKind::Function),
        (SmolStr::new("nil"), BindingKind::Implicit),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser::ast::AstNode;
    use crate::parser::ast;
    use crate::parser::parse;

    #[test]
    fn test_undefined_name() {
        let input = "to f():\n    return x";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(matches!(
            diagnostics.errors.first(),
            Some(SemanticError::UndefinedName { name, .. }) if name == "x"
        ));
    }

    #[test]
    fn test_immutable_assign() {
        let input = "to f():\n    x = 1\n    x += 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::ImmutableAssign { name, .. } if name == "x")));
    }

    #[test]
    fn test_duplicate_local() {
        let input = "to f():\n    x = 1\n    x = 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors.iter().any(|err| matches!(
            err,
            SemanticError::DuplicateDefinition { name, .. } if name == "x"
        )));
    }

    #[test]
    fn test_break_outside_loop() {
        let input = "to f():\n    break";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::BreakOutsideLoop { .. })));
    }

    #[test]
    fn test_fire_in_expression() {
        let input = "to f():\n    return fire Whale().swim()";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::FireInExpression { .. })));
    }

    #[test]
    fn test_positional_after_named_arg() {
        let input = "to f():\n    foo(a=1, 2)";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::PositionalAfterNamed { .. })));
    }

    #[test]
    fn test_duplicate_named_arg() {
        let input = "to f():\n    foo(a=1, a=2)";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::DuplicateNamedArg { name, .. } if name == "a")));
    }

    #[test]
    fn test_invalid_assign_target() {
        let input = "to f(a: Int):\n    a += 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors.iter().any(|err| matches!(
            err,
            SemanticError::InvalidAssignTarget { name, .. } if name == "a"
        )));
    }

    #[test]
    fn test_duplicate_param() {
        let input = "to f(a: Int, a: Int):\n    return a";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors.iter().any(|err| matches!(
            err,
            SemanticError::DuplicateDefinition { name, .. } if name == "a"
        )));
    }

    #[test]
    fn test_shadowing_local() {
        let input = "to f():\n    x = 1\n    if true:\n        x = 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors.iter().any(|err| matches!(
            err,
            SemanticError::ShadowedName { name, .. } if name == "x"
        )));
    }

    #[test]
    fn test_it_outside_return() {
        let input = "to f():\n    it";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::InvalidItUsage { .. })));
    }

    #[test]
    fn test_match_missing_otherwise() {
        let input = "to f():\n    match x:\n        1: return it";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::MatchMissingOtherwise { .. })));
    }

    #[test]
    fn test_visibility_misuse() {
        let input = "to f():\n    public x = 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::VisibilityMisuse { .. })));
    }

    #[test]
    fn test_public_changing_variable() {
        let input = "to f():\n    public changing x = 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors
            .iter()
            .any(|err| matches!(err, SemanticError::PublicChangingVariable { .. })));
    }

    #[test]
    fn test_method_field_name_conflict() {
        let input = "\
A Whale:\n    has:\n        name: String\n    can name() -> String:\n        return \"x\"\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.warnings
            .iter()
            .any(|err| matches!(err, SemanticWarning::MethodFieldNameConflict { .. })));
    }

    #[test]
    fn test_unreachable_code() {
        let input = "to f():\n    return 1\n    x = 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.warnings
            .iter()
            .any(|err| matches!(err, SemanticWarning::UnreachableCode { .. })));
    }

    #[test]
    fn test_unused_local() {
        let input = "to f():\n    x = 1\n    return 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.warnings
            .iter()
            .any(|err| matches!(err, SemanticWarning::UnusedBinding { name, .. } if name == "x")));
    }
}
