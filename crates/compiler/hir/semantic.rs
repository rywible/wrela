#![allow(unused_assignments)]

use crate::hir::{
    Arg, Body, Class, Expr, Function, Idx, Literal, MatchCase, Module, Objective, Stmt, UnaryOp,
};
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


    #[error("match requires an otherwise case")]
    #[diagnostic(
        code(lang::sem::match_missing_otherwise),
        help("Add an `otherwise:` case to this match.")
    )]
    MatchMissingOtherwise {
        #[label("match here")]
        span: SourceSpan,
    },
    #[error("detached pools require an optimization objective")]
    #[diagnostic(
        code(lang::sem::missing_objective),
        help(
            "Add `optimize <objective>:` in scope or inline `optimize <objective>` on the detach."
        )
    )]
    MissingObjective {
        #[label("detach here")]
        span: SourceSpan,
    },
    #[error("only one optimize declaration is allowed per scope")]
    #[diagnostic(
        code(lang::sem::duplicate_optimize),
        help("Remove the extra optimize block or move it into a nested scope.")
    )]
    DuplicateOptimize {
        #[label("optimize here")]
        span: SourceSpan,
    },
    #[error("invalid pool objective")]
    #[diagnostic(
        code(lang::sem::invalid_pool_objective),
        help("Use one of: latency, throughput, conservation, balance.")
    )]
    InvalidPoolObjective {
        #[label("objective here")]
        span: SourceSpan,
    },
    #[error("invalid pool size")]
    #[diagnostic(
        code(lang::sem::invalid_pool_size),
        help("Pool size must be an integer literal or `n`.")
    )]
    InvalidPoolSize {
        #[label("size here")]
        span: SourceSpan,
    },
    #[error("invalid pool batch limit")]
    #[diagnostic(
        code(lang::sem::invalid_pool_batch),
        help("Batch must be an integer literal.")
    )]
    InvalidPoolBatch {
        #[label("batch here")]
        span: SourceSpan,
    },
    #[error("invalid pool backpressure")]
    #[diagnostic(
        code(lang::sem::invalid_pool_backpressure),
        help("Backpressure must be `drop` or `queue(<int>)`.")
    )]
    InvalidPoolBackpressure {
        #[label("backpressure here")]
        span: SourceSpan,
    },
    #[error("invalid pool bound")]
    #[diagnostic(
        code(lang::sem::invalid_pool_bound),
        help("Pool bounds must be integer literals.")
    )]
    InvalidPoolBound {
        #[label("bound here")]
        span: SourceSpan,
    },
    #[error("invalid pool weight")]
    #[diagnostic(
        code(lang::sem::invalid_pool_weight),
        help("Pool weight must be an integer literal.")
    )]
    InvalidPoolWeight {
        #[label("weight here")]
        span: SourceSpan,
    },
    #[error("pool size greater than 1 requires a class constructor")]
    #[diagnostic(
        code(lang::sem::invalid_pool_target),
        help("Use a class name or class constructor call as the detach target.")
    )]
    InvalidPoolTarget {
        #[label("detach here")]
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
            SemanticError::DuplicateNamedArg { span, .. } => *span,
            SemanticError::PositionalAfterNamed { span } => *span,
            SemanticError::MatchMissingOtherwise { span } => *span,
            SemanticError::MissingObjective { span } => *span,
            SemanticError::DuplicateOptimize { span } => *span,
            SemanticError::InvalidPoolObjective { span } => *span,
            SemanticError::InvalidPoolSize { span } => *span,
            SemanticError::InvalidPoolBatch { span } => *span,
            SemanticError::InvalidPoolBackpressure { span } => *span,
            SemanticError::InvalidPoolBound { span } => *span,
            SemanticError::InvalidPoolWeight { span } => *span,
            SemanticError::InvalidPoolTarget { span } => *span,
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

struct Scope {
    bindings: HashMap<SmolStr, Binding>,
    optimize_seen: bool,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            bindings: HashMap::new(),
            optimize_seen: false,
        }
    }
}

struct Checker<'a> {
    module: &'a Module,
    errors: Vec<SemanticError>,
    warnings: Vec<SemanticWarning>,
    scopes: Vec<Scope>,
    objective_stack: Vec<Objective>,
    objective_required_by_fn: HashMap<usize, bool>,
    current_objective_required: bool,
    loop_depth: usize,
    method_ids: HashSet<usize>,
    class_names: HashSet<SmolStr>,
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
        let mut class_names = HashSet::new();
        for class in module.classes.iter().map(|(_, c)| c) {
            class_names.insert(class.name.clone());
            for method in &class.methods {
                method_ids.insert(method.into_raw());
            }
        }
        let objective_required_by_fn = compute_objective_requirements(module, &method_ids);

        Self {
            module,
            errors: Vec::new(),
            warnings: Vec::new(),
            scopes: vec![Scope::default()],
            objective_stack: Vec::new(),
            objective_required_by_fn,
            current_objective_required: false,
            loop_depth: 0,
            method_ids,
            class_names,
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
            self.check_function(idx, func, false);
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
                self.warnings
                    .push(SemanticWarning::MethodFieldNameConflict {
                        name: method.name.clone(),
                        span: span_from_option(method.name_span),
                    });
            }
            self.check_function(*method_id, method, true);
        }
    }

    fn check_function(&mut self, func_id: Idx<Function>, func: &Function, is_method: bool) {
        let prev_method = self.in_method;
        let prev_require_objective = self.current_objective_required;
        self.current_objective_required = self
            .objective_required_by_fn
            .get(&func_id.into_raw())
            .copied()
            .unwrap_or(false);
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
        self.current_objective_required = prev_require_objective;
    }

    fn check_stmt(&mut self, body: &Body, stmt_id: Idx<Stmt>) {
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::Expr(expr) => self.check_expr_with_ctx(body, *expr, false, true),
            Stmt::Assert { expr, .. } => {
                self.check_expr_with_ctx(body, *expr, false, true);
            }
            Stmt::Let {
                name,
                value,
                mutable,
                visibility,
            } => {
                self.check_expr_with_ctx(body, *value, false, false);
                let span = body.stmt_span(stmt_id);
                let _ = visibility;
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
                    let _ = visibility;
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
            Stmt::Optimize {
                objective,
                body: opt_body,
            } => {
                if let Some(scope) = self.scopes.last_mut() {
                    if scope.optimize_seen {
                        self.errors.push(SemanticError::DuplicateOptimize {
                            span: span_from_range(body.stmt_span(stmt_id)),
                        });
                    } else {
                        scope.optimize_seen = true;
                    }
                }
                self.enter_scope();
                self.objective_stack.push(*objective);
                self.check_block(body, opt_body);
                self.objective_stack.pop();
                self.exit_scope();
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
            Stmt::While {
                condition,
                body: loop_body,
            } => {
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
            Expr::Detach {
                target,
                objective,
                size,
            } => {
                self.check_expr_with_ctx(body, *target, allow_it, false);
                let pool_objective = self.pool_of_objective(body, *target);
                if self.current_objective_required
                    && objective.is_none()
                    && pool_objective.is_none()
                    && self.objective_stack.is_empty()
                {
                    self.errors.push(SemanticError::MissingObjective {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                }
                if (matches!(size, crate::hir::PoolSize::Fixed(count) if *count > 1)
                    || matches!(size, crate::hir::PoolSize::Auto))
                    && !self.is_class_constructor_target(body, *target)
                    && !self.pool_of_target(body, *target)
                {
                    self.errors.push(SemanticError::InvalidPoolTarget {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                }
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
                let is_pool_of = self.is_pool_of_call(body, *callee);
                if is_pool_of {
                    self.validate_pool_of_args(body, args);
                }
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
                            if !is_pool_of {
                                self.check_expr_with_ctx(body, *value, allow_it, false);
                            }
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

    fn is_pool_of_call(&self, body: &Body, callee: Idx<Expr>) -> bool {
        match &body.exprs[callee] {
            Expr::Member { object, member, .. } => {
                if member.as_str() != "of" {
                    return false;
                }
                matches!(&body.exprs[*object], Expr::Variable(name) if name.as_str() == "Pool")
            }
            _ => false,
        }
    }

    fn validate_pool_of_args(&mut self, body: &Body, args: &[Arg]) {
        for arg in args {
            if let Arg::Named { name, value, .. } = arg {
                match name.as_str() {
                    "size" => {
                        let ok = match &body.exprs[*value] {
                            Expr::Literal(Literal::Int(_)) => true,
                            Expr::Variable(var) => var.as_str() == "n",
                            _ => false,
                        };
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolSize {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    "objective" => {
                        let ok = match &body.exprs[*value] {
                            Expr::Variable(name) => Objective::from_str(name.as_str()).is_some(),
                            _ => false,
                        };
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolObjective {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    "batch" => {
                        let ok = matches!(&body.exprs[*value], Expr::Literal(Literal::Int(_)));
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolBatch {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    "min" | "max" => {
                        let ok = matches!(&body.exprs[*value], Expr::Literal(Literal::Int(_)));
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolBound {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    "weight" => {
                        let ok = matches!(&body.exprs[*value], Expr::Literal(Literal::Int(_)));
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolWeight {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    "backpressure" => {
                        let ok = match &body.exprs[*value] {
                            Expr::Variable(name) => name.as_str() == "drop",
                            Expr::Call { callee, args } => {
                                if let Expr::Variable(name) = &body.exprs[*callee] {
                                    if name.as_str() != "queue" || args.len() != 1 {
                                        false
                                    } else {
                                        let arg = match &args[0] {
                                            Arg::Positional { value, .. } => *value,
                                            Arg::Named { value, .. } => *value,
                                        };
                                        matches!(&body.exprs[arg], Expr::Literal(Literal::Int(_)))
                                    }
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };
                        if !ok {
                            self.errors.push(SemanticError::InvalidPoolBackpressure {
                                span: span_from_range(body.expr_span(*value)),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn pool_of_objective(&self, body: &Body, expr_id: Idx<Expr>) -> Option<Objective> {
        let Expr::Call { callee, args } = &body.exprs[expr_id] else {
            return None;
        };
        if !self.is_pool_of_call(body, *callee) {
            return None;
        }
        for arg in args {
            if let Arg::Named { name, value, .. } = arg {
                if name.as_str() == "objective" {
                    if let Expr::Variable(id) = &body.exprs[*value] {
                        if let Some(obj) = Objective::from_str(id.as_str()) {
                            return Some(obj);
                        }
                    }
                }
            }
        }
        None
    }

    fn is_class_constructor_target(&self, body: &Body, expr_id: Idx<Expr>) -> bool {
        match &body.exprs[expr_id] {
            Expr::Variable(name) => self.class_names.contains(name),
            Expr::Call { callee, .. } => match &body.exprs[*callee] {
                Expr::Variable(name) => self.class_names.contains(name),
                _ => false,
            },
            _ => false,
        }
    }

    fn pool_of_target(&self, body: &Body, expr_id: Idx<Expr>) -> bool {
        let Expr::Call { callee, .. } = &body.exprs[expr_id] else {
            return false;
        };
        self.is_pool_of_call(body, *callee)
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

fn compute_objective_requirements(
    module: &Module,
    method_ids: &HashSet<usize>,
) -> HashMap<usize, bool> {
    let mut function_ids = HashMap::new();
    let mut method_name_ids: HashMap<SmolStr, Vec<Idx<Function>>> = HashMap::new();
    for (idx, func) in module.functions.iter() {
        if method_ids.contains(&idx.into_raw()) {
            method_name_ids
                .entry(func.name.clone())
                .or_default()
                .push(idx);
        } else {
            function_ids.insert(func.name.clone(), idx);
        }
    }

    let mut direct_await: HashMap<usize, bool> = HashMap::new();
    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, func) in module.functions.iter() {
        let mut has_await = false;
        let mut callees = HashSet::new();
        if let Some(body) = &func.body {
            collect_calls_and_awaits(
                body,
                &body.root_stmts,
                &function_ids,
                &method_name_ids,
                &mut has_await,
                &mut callees,
            );
        }
        direct_await.insert(idx.into_raw(), has_await);
        graph.insert(
            idx.into_raw(),
            callees
                .into_iter()
                .map(|callee| callee.into_raw())
                .collect(),
        );
    }

    let mut memo = HashMap::new();
    let mut visiting = HashSet::new();
    for (idx, _func) in module.functions.iter() {
        let id = idx.into_raw();
        let _ = await_in_transitive_call_graph(id, &graph, &direct_await, &mut visiting, &mut memo);
    }
    memo
}

fn await_in_transitive_call_graph(
    func_id: usize,
    graph: &HashMap<usize, Vec<usize>>,
    direct_await: &HashMap<usize, bool>,
    visiting: &mut HashSet<usize>,
    memo: &mut HashMap<usize, bool>,
) -> bool {
    if let Some(val) = memo.get(&func_id) {
        return *val;
    }
    if visiting.contains(&func_id) {
        return *direct_await.get(&func_id).unwrap_or(&false);
    }
    visiting.insert(func_id);
    let mut has_await = *direct_await.get(&func_id).unwrap_or(&false);
    if !has_await {
        if let Some(callees) = graph.get(&func_id) {
            for callee in callees {
                if await_in_transitive_call_graph(*callee, graph, direct_await, visiting, memo) {
                    has_await = true;
                    break;
                }
            }
        }
    }
    visiting.remove(&func_id);
    memo.insert(func_id, has_await);
    has_await
}

fn collect_calls_and_awaits(
    body: &Body,
    root_stmts: &[Idx<Stmt>],
    function_ids: &HashMap<SmolStr, Idx<Function>>,
    method_name_ids: &HashMap<SmolStr, Vec<Idx<Function>>>,
    has_await: &mut bool,
    callees: &mut HashSet<Idx<Function>>,
) {
    for stmt in root_stmts {
        collect_stmt_calls_and_awaits(
            body,
            *stmt,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        );
    }
}

fn collect_stmt_calls_and_awaits(
    body: &Body,
    stmt_id: Idx<Stmt>,
    function_ids: &HashMap<SmolStr, Idx<Function>>,
    method_name_ids: &HashMap<SmolStr, Vec<Idx<Function>>>,
    has_await: &mut bool,
    callees: &mut HashSet<Idx<Function>>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => collect_expr_calls_and_awaits(
            body,
            *expr,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        ),
        Stmt::Assert { expr, .. } => {
            collect_expr_calls_and_awaits(
                body,
                *expr,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } => collect_expr_calls_and_awaits(
            body,
            *value,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        ),
        Stmt::Optimize { body: inner, .. } => {
            for stmt in inner {
                collect_stmt_calls_and_awaits(
                    body,
                    *stmt,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_expr_calls_and_awaits(
                body,
                *condition,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            for stmt in then_branch {
                collect_stmt_calls_and_awaits(
                    body,
                    *stmt,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
            if let Some(branch) = else_branch {
                for stmt in branch {
                    collect_stmt_calls_and_awaits(
                        body,
                        *stmt,
                        function_ids,
                        method_name_ids,
                        has_await,
                        callees,
                    );
                }
            }
        }
        Stmt::For {
            iterable,
            body: inner,
            ..
        } => {
            collect_expr_calls_and_awaits(
                body,
                *iterable,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            for stmt in inner {
                collect_stmt_calls_and_awaits(
                    body,
                    *stmt,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            collect_expr_calls_and_awaits(
                body,
                *subject,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            for case in cases {
                for label in &case.labels {
                    collect_expr_calls_and_awaits(
                        body,
                        *label,
                        function_ids,
                        method_name_ids,
                        has_await,
                        callees,
                    );
                }
                for stmt in &case.body {
                    collect_stmt_calls_and_awaits(
                        body,
                        *stmt,
                        function_ids,
                        method_name_ids,
                        has_await,
                        callees,
                    );
                }
            }
            if let Some(otherwise) = otherwise {
                for stmt in otherwise {
                    collect_stmt_calls_and_awaits(
                        body,
                        *stmt,
                        function_ids,
                        method_name_ids,
                        has_await,
                        callees,
                    );
                }
            }
        }
        Stmt::While {
            condition,
            body: inner,
        } => {
            collect_expr_calls_and_awaits(
                body,
                *condition,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            for stmt in inner {
                collect_stmt_calls_and_awaits(
                    body,
                    *stmt,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_expr_calls_and_awaits(
                    body,
                    *expr,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

fn collect_expr_calls_and_awaits(
    body: &Body,
    expr_id: Idx<Expr>,
    function_ids: &HashMap<SmolStr, Idx<Function>>,
    method_name_ids: &HashMap<SmolStr, Vec<Idx<Function>>>,
    has_await: &mut bool,
    callees: &mut HashSet<Idx<Function>>,
) {
    match &body.exprs[expr_id] {
        Expr::Literal(_) | Expr::Variable(_) => {}
        Expr::Detach { target, .. } => collect_expr_calls_and_awaits(
            body,
            *target,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        ),
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_calls_and_awaits(
                body,
                *lhs,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            collect_expr_calls_and_awaits(
                body,
                *rhs,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
        }
        Expr::Unary { op, expr, .. } => {
            if matches!(op, UnaryOp::Await) {
                *has_await = true;
            }
            collect_expr_calls_and_awaits(
                body,
                *expr,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
        }
        Expr::Crash { expr } => collect_expr_calls_and_awaits(
            body,
            *expr,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        ),
        Expr::Call { callee, args } => {
            match &body.exprs[*callee] {
                Expr::Variable(name) => {
                    if let Some(id) = function_ids.get(name) {
                        callees.insert(*id);
                    }
                }
                Expr::Member { member, .. } => {
                    if !matches!(&body.exprs[*callee], Expr::Member { object, member, .. }
                        if member.as_str() == "of"
                            && matches!(&body.exprs[*object], Expr::Variable(name) if name.as_str() == "Pool"))
                    {
                        if let Some(methods) = method_name_ids.get(member) {
                            for method in methods {
                                callees.insert(*method);
                            }
                        }
                    }
                }
                _ => {}
            }
            collect_expr_calls_and_awaits(
                body,
                *callee,
                function_ids,
                method_name_ids,
                has_await,
                callees,
            );
            for arg in args {
                let value = match arg {
                    Arg::Positional { value, .. } => value,
                    Arg::Named { value, .. } => value,
                };
                collect_expr_calls_and_awaits(
                    body,
                    *value,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Expr::Member { object, .. } => collect_expr_calls_and_awaits(
            body,
            *object,
            function_ids,
            method_name_ids,
            has_await,
            callees,
        ),
        Expr::List(items) => {
            for item in items {
                collect_expr_calls_and_awaits(
                    body,
                    *item,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                collect_expr_calls_and_awaits(
                    body,
                    *key,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
                collect_expr_calls_and_awaits(
                    body,
                    *value,
                    function_ids,
                    method_name_ids,
                    has_await,
                    callees,
                );
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    collect_expr_calls_and_awaits(
                        body,
                        *expr,
                        function_ids,
                        method_name_ids,
                        has_await,
                        callees,
                    );
                }
            }
        }
    }
}

fn builtin_bindings() -> Vec<(SmolStr, BindingKind)> {
    vec![
        (SmolStr::new("print"), BindingKind::Function),
        (SmolStr::new("parse_int"), BindingKind::Function),
        (SmolStr::new("parse_float"), BindingKind::Function),
        (SmolStr::new("read_file"), BindingKind::Function),
        (SmolStr::new("write_file"), BindingKind::Function),
        (SmolStr::new("bytes_from_string"), BindingKind::Function),
        (SmolStr::new("bytes_to_string"), BindingKind::Function),
        (SmolStr::new("bytes_len"), BindingKind::Function),
        (SmolStr::new("list_push"), BindingKind::Function),
        (SmolStr::new("map_new"), BindingKind::Function),
        (SmolStr::new("map_get"), BindingKind::Function),
        (SmolStr::new("map_set"), BindingKind::Function),
        (SmolStr::new("log"), BindingKind::Function),
        (SmolStr::new("env_get"), BindingKind::Function),
        (SmolStr::new("env_get_or"), BindingKind::Function),
        (SmolStr::new("env_get_as_bool"), BindingKind::Function),
        (SmolStr::new("env_get_as_int"), BindingKind::Function),
        (SmolStr::new("env_set"), BindingKind::Function),
        (SmolStr::new("env_load"), BindingKind::Function),
        (SmolStr::new("auth_create_user"), BindingKind::Function),
        (SmolStr::new("auth_verify_password"), BindingKind::Function),
        (SmolStr::new("auth_issue_jwt"), BindingKind::Function),
        (SmolStr::new("auth_verify_jwt"), BindingKind::Function),
        (SmolStr::new("auth_issue_email_token"), BindingKind::Function),
        (SmolStr::new("auth_verify_email_token"), BindingKind::Function),
        (SmolStr::new("auth_oauth_login"), BindingKind::Function),
        (SmolStr::new("rbac_create_role"), BindingKind::Function),
        (SmolStr::new("rbac_assign_role"), BindingKind::Function),
        (SmolStr::new("rbac_check"), BindingKind::Function),
        (SmolStr::new("rbac_permissions_for"), BindingKind::Function),
        (SmolStr::new("files_upload_stream"), BindingKind::Function),
        (SmolStr::new("files_signed_url"), BindingKind::Function),
        (SmolStr::new("files_metadata"), BindingKind::Function),
        (SmolStr::new("files_delete"), BindingKind::Function),
        (SmolStr::new("files_set_acl"), BindingKind::Function),
        (SmolStr::new("jobs_enqueue"), BindingKind::Function),
        (SmolStr::new("jobs_process"), BindingKind::Function),
        (SmolStr::new("jobs_dead_letter"), BindingKind::Function),
        (SmolStr::new("schedule_cron"), BindingKind::Function),
        (SmolStr::new("schedule_every"), BindingKind::Function),
        (SmolStr::new("schedule_at"), BindingKind::Function),
        (SmolStr::new("search_index"), BindingKind::Function),
        (SmolStr::new("search_remove"), BindingKind::Function),
        (SmolStr::new("search_query"), BindingKind::Function),
        (SmolStr::new("realtime_on_connect"), BindingKind::Function),
        (SmolStr::new("realtime_join"), BindingKind::Function),
        (SmolStr::new("realtime_leave"), BindingKind::Function),
        (SmolStr::new("realtime_broadcast"), BindingKind::Function),
        (SmolStr::new("realtime_send"), BindingKind::Function),
        (SmolStr::new("rate_check"), BindingKind::Function),
        (SmolStr::new("rate_ip"), BindingKind::Function),
        (SmolStr::new("admin_enable"), BindingKind::Function),
        (SmolStr::new("storage_get"), BindingKind::Function),
        (SmolStr::new("storage_get_with_version"), BindingKind::Function),
        (SmolStr::new("storage_scan"), BindingKind::Function),
        (SmolStr::new("storage_list_prefix"), BindingKind::Function),
        (SmolStr::new("storage_set"), BindingKind::Function),
        (SmolStr::new("storage_set_if_version"), BindingKind::Function),
        (SmolStr::new("storage_delete_if_version"), BindingKind::Function),
        (SmolStr::new("storage_delete"), BindingKind::Function),
        (SmolStr::new("storage_batch_set"), BindingKind::Function),
        (SmolStr::new("storage_configure"), BindingKind::Function),
        (SmolStr::new("pool_auto_size"), BindingKind::Function),
        (SmolStr::new("pool_size"), BindingKind::Function),
        (SmolStr::new("pool_rr"), BindingKind::Function),
        (SmolStr::new("pool_queue_len"), BindingKind::Function),
        (SmolStr::new("actor_mailbox_len"), BindingKind::Function),
        (SmolStr::new("actor_pause"), BindingKind::Function),
        (SmolStr::new("actor_resume"), BindingKind::Function),
        (SmolStr::new("actor_pause_wait"), BindingKind::Function),
        (SmolStr::new("metrics_get"), BindingKind::Function),
        (
            SmolStr::new("metrics_dropped_paused_id"),
            BindingKind::Function,
        ),
        (
            SmolStr::new("metrics_messages_dropped_id"),
            BindingKind::Function,
        ),
        (SmolStr::new("clock_ns"), BindingKind::Function),
        (SmolStr::new("sleep_ms"), BindingKind::Function),
        (
            SmolStr::new("http_server_serve_get_requests"),
            BindingKind::Function,
        ),
        (
            SmolStr::new("http_server_serve_post_requests"),
            BindingKind::Function,
        ),
        (
            SmolStr::new("http_server_serve_requests"),
            BindingKind::Function,
        ),
        (SmolStr::new("http_server_serve_on"), BindingKind::Function),
        (SmolStr::new("http_server_stop"), BindingKind::Function),
        (SmolStr::new("nil"), BindingKind::Implicit),
        (SmolStr::new("Pool"), BindingKind::Implicit),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_undefined_name() {
        let input = "to f() -> Int:\n    return x";
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
        let input = "to f() -> Nothing:\n    x = 1\n    x += 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics.errors.iter().any(
                |err| matches!(err, SemanticError::ImmutableAssign { name, .. } if name == "x")
            )
        );
    }

    #[test]
    fn test_duplicate_local() {
        let input = "to f() -> Nothing:\n    x = 1\n    x = 2";
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
        let input = "to f() -> Nothing:\n    break";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::BreakOutsideLoop { .. }))
        );
    }

    #[test]
    fn test_fire_in_expression() {
        let input = "to f() -> Nothing:\n    return fire Whale().swim()";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::FireInExpression { .. }))
        );
    }

    #[test]
    fn test_positional_after_named_arg() {
        let input = "to f() -> Nothing:\n    foo(a=1, 2)";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::PositionalAfterNamed { .. }))
        );
    }

    #[test]
    fn test_duplicate_named_arg() {
        let input = "to f() -> Nothing:\n    foo(a=1, a=2)";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(diagnostics.errors.iter().any(
            |err| matches!(err, SemanticError::DuplicateNamedArg { name, .. } if name == "a")
        ));
    }

    #[test]
    fn test_invalid_assign_target() {
        let input = "to f(a: Int) -> Nothing:\n    a += 1";
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
        let input = "to f(a: Int, a: Int) -> Int:\n    return a";
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
        let input = "to f() -> Nothing:\n    x = 1\n    if true:\n        x = 2";
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
        let input = "to f() -> Nothing:\n    it";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidItUsage { .. }))
        );
    }

    #[test]
    fn test_match_missing_otherwise() {
        let input = "to f() -> Int:\n    match x:\n        1: return it";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::MatchMissingOtherwise { .. }))
        );
    }

    #[test]
    fn test_method_field_name_conflict() {
        let input = "\
A Whale:\n    has:\n        name: String\n    can name() -> String:\n        return \"x\"\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .warnings
                .iter()
                .any(|err| matches!(err, SemanticWarning::MethodFieldNameConflict { .. }))
        );
    }

    #[test]
    fn test_unreachable_code() {
        let input = "to f() -> Int:\n    return 1\n    x = 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .warnings
                .iter()
                .any(|err| matches!(err, SemanticWarning::UnreachableCode { .. }))
        );
    }

    #[test]
    fn test_unused_local() {
        let input = "to f() -> Int:\n    x = 1\n    return 2";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics.warnings.iter().any(
                |err| matches!(err, SemanticWarning::UnusedBinding { name, .. } if name == "x")
            )
        );
    }

    #[test]
    fn test_missing_objective_ignored_without_await() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    whale = detach Whale() * 1
    return 1
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            !diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::MissingObjective { .. }))
        );
    }

    #[test]
    fn test_missing_objective_with_await_in_call_graph() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    return f()

to f() -> Int:
    await 1
    whale = detach Whale() * 1
    return 1
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::MissingObjective { .. }))
        );
    }

    #[test]
    fn test_duplicate_optimize_in_scope() {
        let input = r#"
to run() -> Int:
    optimize balance:
        x = 1
    optimize latency:
        y = 2
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::DuplicateOptimize { .. }))
        );
    }

    #[test]
    fn test_pool_of_objective_satisfies_requirement() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    return f()

to f() -> Int:
    await 1
    whale = detach Pool.of(Whale, objective=latency) * 1
    return 1
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            !diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::MissingObjective { .. }))
        );
    }

    #[test]
    fn test_pool_of_invalid_size() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    optimize balance:
        pool = Pool.of(Whale, size=foo)
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolSize { .. }))
        );
    }

    #[test]
    fn test_pool_of_batch_and_backpressure_valid() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    optimize balance:
        pool = Pool.of(Whale, size=1, objective=balance, batch=8, backpressure=queue(4))
        pool2 = Pool.of(Whale, size=1, backpressure=drop)
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics.errors.is_empty(),
            "errors: {:?}",
            diagnostics.errors
        );
    }

    #[test]
    fn test_pool_of_invalid_backpressure() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    optimize balance:
        pool = Pool.of(Whale, backpressure=queue(foo))
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolBackpressure { .. }))
        );
    }

    #[test]
    fn test_pool_of_invalid_bounds_and_weight() {
        let input = r#"
A Whale:
    has:
        value: Int

to run() -> Int:
    optimize balance:
        pool = Pool.of(Whale, min=foo, max=bar, weight=baz)
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolBound { .. }))
        );
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolWeight { .. }))
        );
    }

    #[test]
    fn test_invalid_pool_target_for_fixed_size() {
        let input = r#"
to run() -> Int:
    optimize balance:
        x = 1
        worker = detach x * 2
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolTarget { .. }))
        );
    }

    #[test]
    fn test_invalid_pool_target_for_auto_size() {
        let input = r#"
to run() -> Int:
    optimize balance:
        x = 1
        worker = detach x * n
    return 0
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diagnostics = check_module(&module);
        assert!(
            diagnostics
                .errors
                .iter()
                .any(|err| matches!(err, SemanticError::InvalidPoolTarget { .. }))
        );
    }
}
