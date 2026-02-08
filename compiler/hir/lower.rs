use crate::hir::*;
use crate::parser::ast::{self, AstNode};
use crate::parser::kind::SyntaxKind;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;
use std::collections::HashSet;

pub fn lower(root: ast::Root) -> Module {
    let mut ctx = LoweringContext::default();
    ctx.lower_module(root)
}

pub fn lower_root_body(root: ast::Root) -> Option<Body> {
    let mut body_ctx = BodyLoweringContext::new();
    let mut has_stmt = false;
    for stmt in root.statements() {
        match stmt {
            ast::Stmt::FuncDef(_)
            | ast::Stmt::CheckDef(_)
            | ast::Stmt::ClassDef(_)
            | ast::Stmt::EnumDef(_)
            | ast::Stmt::UseStmt(_)
            | ast::Stmt::PrivateBlock(_) => {}
            other => {
                let s = body_ctx.lower_stmt(other);
                body_ctx.body.root_stmts.push(s);
                has_stmt = true;
            }
        }
    }
    if has_stmt { Some(body_ctx.body) } else { None }
}

#[derive(Default)]
struct LoweringContext {
    module: Module,
}

impl Module {
    fn new() -> Self {
        Self {
            functions: Arena::new(),
            classes: Arena::new(),
            enums: Arena::new(),
            interfaces: Arena::new(),
            uses: Vec::new(),
        }
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

impl LoweringContext {
    fn lower_module(&mut self, root: ast::Root) -> Module {
        for stmt in root.statements() {
            match stmt {
                ast::Stmt::FuncDef(f) => {
                    let func = self.lower_func(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::CheckDef(c) => {
                    let func = self.lower_check(c);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::ClassDef(c) => {
                    if self.class_is_interface(&c) {
                        let interface = self.lower_interface_from_class(c);
                        self.module.interfaces.alloc(interface);
                    } else {
                        let class = self.lower_class(c);
                        self.module.classes.alloc(class);
                    }
                }
                ast::Stmt::EnumDef(e) => {
                    let en = self.lower_enum(e);
                    self.module.enums.alloc(en);
                }
                ast::Stmt::PrivateBlock(block) => {
                    for stmt in block.statements() {
                        match stmt {
                            ast::Stmt::FuncDef(f) => {
                                let func = self.lower_func(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::CheckDef(c) => {
                                let func = self.lower_check(c);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::ClassDef(c) => {
                                if self.class_is_interface(&c) {
                                    let interface = self.lower_interface_from_class(c);
                                    self.module.interfaces.alloc(interface);
                                } else {
                                    let class = self.lower_class(c);
                                    self.module.classes.alloc(class);
                                }
                            }
                            ast::Stmt::EnumDef(e) => {
                                let en = self.lower_enum(e);
                                self.module.enums.alloc(en);
                            }
                            _ => {}
                        }
                    }
                }
                ast::Stmt::UseStmt(u) => {
                    let (names, module, module_span) = parse_use_stmt(&u);
                    self.module.uses.push(UseStmt {
                        names,
                        module,
                        module_span,
                        span: u.syntax().text_range(),
                    });
                }
                _ => {
                    // Top-level executable statements are rejected; entrypoint is `run`.
                }
            }
        }
        std::mem::take(&mut self.module)
    }

    fn lower_func(&mut self, f: ast::FuncDef) -> Function {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }

        Function {
            name,
            name_span,
            visibility,
            kind: FunctionKind::Function,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_check(&mut self, c: ast::CheckDef) -> Function {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let params = c.params().map(|p| self.lower_param(p)).collect();
        let ret_type = c.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in c.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }

        Function {
            name,
            name_span,
            visibility,
            kind: FunctionKind::Check,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_class(&mut self, c: ast::ClassDef) -> Class {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let type_params = c.type_params().map(|t| SmolStr::new(t.text())).collect();
        let implements = c
            .is_a()
            .map(|t| SmolStr::new(t.text()))
            .into_iter()
            .collect();
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for has_block in c.has_blocks() {
            self.push_fields_from_has_block(has_block, &mut fields);
        }

        for method in c.methods() {
            let func = self.lower_method(method);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        for check in c.checks() {
            let func = self.lower_check_method(check);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        for derive in c.derives() {
            let func = self.lower_derive(derive);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        for private_block in c.syntax().children().filter_map(ast::PrivateBlock::cast) {
            for child in private_block.syntax().children() {
                if let Some(has_block) = ast::HasBlock::cast(child.clone()) {
                    self.push_fields_from_has_block(has_block, &mut fields);
                    continue;
                }
                if let Some(method) = ast::MethodDef::cast(child.clone()) {
                    let func = self.lower_method(method);
                    let id = self.module.functions.alloc(func);
                    methods.push(id);
                    continue;
                }
                if let Some(check) = ast::CheckMethodDef::cast(child.clone()) {
                    let func = self.lower_check_method(check);
                    let id = self.module.functions.alloc(func);
                    methods.push(id);
                    continue;
                }
                if let Some(derive) = ast::DeriveDef::cast(child) {
                    let func = self.lower_derive(derive);
                    let id = self.module.functions.alloc(func);
                    methods.push(id);
                }
            }
        }

        Class {
            name,
            name_span,
            visibility,
            type_params,
            fields,
            methods,
            implements,
        }
    }

    fn lower_enum(&mut self, e: ast::EnumDef) -> Enum {
        let name = e.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = e.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(e.syntax());
        let type_params = e.type_params().map(|t| SmolStr::new(t.text())).collect();
        let mut variants = Vec::new();
        for variant in e.variants() {
            let v_name = variant
                .name()
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_default();
            let v_name_span = variant.name().map(|t| t.text_range());
            let params = variant.params().map(|p| self.lower_param(p)).collect();
            variants.push(EnumVariant {
                name: v_name,
                name_span: v_name_span,
                params,
            });
        }
        Enum {
            name,
            name_span,
            visibility,
            type_params,
            variants,
        }
    }

    fn lower_interface_from_class(&mut self, c: ast::ClassDef) -> Interface {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let type_params = c.type_params().map(|t| SmolStr::new(t.text())).collect();
        let mut methods = Vec::new();
        for method in c.must_methods() {
            let m_name = method
                .name()
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_default();
            let m_name_span = method.name().map(|t| t.text_range());
            let params = method.params().map(|p| self.lower_param(p)).collect();
            let ret_type = method.ret_type().map(|t| self.lower_type_ref(t));
            methods.push(InterfaceMethod {
                name: m_name,
                name_span: m_name_span,
                params,
                ret_type,
                kind: if method.is_check() {
                    InterfaceMethodKind::Check
                } else {
                    InterfaceMethodKind::Method
                },
            });
        }
        Interface {
            name,
            name_span,
            visibility,
            type_params,
            methods,
        }
    }

    fn class_is_interface(&self, c: &ast::ClassDef) -> bool {
        c.must_methods().next().is_some()
    }

    fn lower_method(&mut self, m: ast::MethodDef) -> Function {
        let name = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = m.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(m.syntax());
        let params = m.params().map(|p| self.lower_param(p)).collect();
        let ret_type = m.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in m.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }

        Function {
            name,
            name_span,
            visibility,
            kind: FunctionKind::Method,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_check_method(&mut self, m: ast::CheckMethodDef) -> Function {
        let name = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = m.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(m.syntax());
        let params = m.params().map(|p| self.lower_param(p)).collect();
        let ret_type = m.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in m.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }

        Function {
            name,
            name_span,
            visibility,
            kind: FunctionKind::CheckMethod,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_derive(&mut self, d: ast::DeriveDef) -> Function {
        let name = d.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = d.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(d.syntax());
        let params = d.params().map(|p| self.lower_param(p)).collect();
        let ret_type = d.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in d.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }

        Function {
            name,
            name_span,
            visibility,
            kind: FunctionKind::Derived,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn push_fields_from_has_block(&mut self, has_block: ast::HasBlock, fields: &mut Vec<Field>) {
        for field in has_block.fields() {
            fields.push(self.lower_field(field));
        }
        for private_block in has_block
            .syntax()
            .children()
            .filter_map(ast::PrivateBlock::cast)
        {
            for field in private_block
                .syntax()
                .children()
                .filter_map(ast::FieldDef::cast)
            {
                fields.push(self.lower_field(field));
            }
        }
    }

    fn lower_param(&mut self, p: ast::Param) -> Param {
        Param {
            name: p.name().map(|t| SmolStr::new(t.text())).unwrap_or_default(),
            name_span: p.name().map(|t| t.text_range()),
            ty: p.ty().map(|t| self.lower_type_ref(t)),
        }
    }

    fn lower_field(&mut self, f: ast::FieldDef) -> Field {
        Field {
            name: f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default(),
            name_span: f.name().map(|t| t.text_range()),
            visibility: visibility_for_node_default(f.syntax()),
            ty: f.ty().map(|t| self.lower_type_ref(t)),
            mutable: f.is_mutable(),
            default: f
                .default_expr()
                .and_then(|expr| self.lower_field_default(expr)),
        }
    }

    fn lower_field_default(&mut self, expr: ast::Expr) -> Option<FieldDefault> {
        match expr {
            ast::Expr::Literal(l) => {
                let token = first_non_trivia_token(l.syntax())?;
                let lit = match token.kind() {
                    SyntaxKind::IntNumber => Literal::Integer(token.text().parse().unwrap_or(0)),
                    SyntaxKind::FloatNumber => Literal::Float(token.text().parse().unwrap_or(0.0)),
                    SyntaxKind::StringLiteral => {
                        Literal::String(SmolStr::new(token.text().trim_matches('\"')))
                    }
                    SyntaxKind::TrueKw => Literal::Boolean(true),
                    SyntaxKind::FalseKw => Literal::Boolean(false),
                    SyntaxKind::NothingKw => Literal::Nil,
                    _ => return None,
                };
                Some(FieldDefault::Literal(lit))
            }
            ast::Expr::List(list) => {
                let mut items = Vec::new();
                for item in list.items() {
                    let lowered = self.lower_field_default(item)?;
                    items.push(lowered);
                }
                Some(FieldDefault::List(items))
            }
            ast::Expr::Map(map) => {
                let mut items = Vec::new();
                let mut iter = map.items();
                while let Some(key) = iter.next() {
                    let value = iter.next()?;
                    let key = self.lower_field_default(key)?;
                    let value = self.lower_field_default(value)?;
                    items.push((key, value));
                }
                Some(FieldDefault::Map(items))
            }
            ast::Expr::Paren(p) => {
                self.lower_field_default(p.syntax().children().filter_map(ast::Expr::cast).next()?)
            }
            _ => None,
        }
    }

    fn lower_type_ref(&mut self, t: ast::TypeRef) -> TypeRef {
        TypeRef {
            name: t
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            name_span: t.name().map(|tok| tok.text_range()),
            args: t
                .args()
                .into_iter()
                .map(|arg| self.lower_type_ref(arg))
                .collect(),
        }
    }
}

struct BodyLoweringContext {
    body: Body,
    scopes: Vec<HashSet<SmolStr>>,
}

impl BodyLoweringContext {
    fn new() -> Self {
        Self {
            body: Body {
                exprs: Arena::new(),
                stmts: Arena::new(),
                root_stmts: Vec::new(),
                expr_spans: Vec::new(),
                stmt_spans: Vec::new(),
            },
            scopes: vec![HashSet::new()],
        }
    }

    fn alloc_expr(&mut self, expr: Expr, span: TextRange) -> Idx<Expr> {
        let idx = self.body.exprs.alloc(expr);
        self.body.expr_spans.push(span);
        idx
    }

    fn alloc_stmt(&mut self, stmt: Stmt, span: TextRange) -> Idx<Stmt> {
        let idx = self.body.stmts.alloc(stmt);
        self.body.stmt_spans.push(span);
        idx
    }

    fn empty_span(&self) -> TextRange {
        TextRange::empty(TextSize::from(0))
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_name(&mut self, name: &SmolStr) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.clone());
        }
    }

    fn name_exists(&self, name: &SmolStr) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn lower_type_ref(&mut self, t: ast::TypeRef) -> TypeRef {
        TypeRef {
            name: t
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            name_span: t.name().map(|tok| tok.text_range()),
            args: t
                .args()
                .into_iter()
                .map(|arg| self.lower_type_ref(arg))
                .collect(),
        }
    }

    fn lower_stmt(&mut self, stmt: ast::Stmt) -> Idx<Stmt> {
        let stmt_span = stmt.syntax().text_range();
        let name_span = match &stmt {
            ast::Stmt::VarAssign(v) => v.name().map(|t| t.text_range()),
            _ => None,
        };
        let hir_stmt = match stmt {
            ast::Stmt::Expr(e) => {
                let expr = e.expr().and_then(|e| self.lower_expr(e));
                match expr {
                    Some(e) => Stmt::Expr(e),
                    None => Stmt::Break, // Error recovery or empty
                }
            }
            ast::Stmt::VarAssign(v) => {
                let name = v.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let visibility = visibility_for_node(v.syntax());
                let mutable = has_token(v.syntax(), SyntaxKind::MutableKw);
                let value = v
                    .value()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                match assign_op_for_node(v.syntax()) {
                    Some(AssignOp::Assign) | None => {
                        let exists = self.name_exists(&name);
                        if !mutable && exists {
                            Stmt::Assign {
                                name,
                                op: AssignOp::Assign,
                                value,
                                mutable: false,
                                visibility,
                            }
                        } else {
                            self.declare_name(&name);
                            Stmt::Let {
                                name,
                                value,
                                mutable,
                                visibility,
                            }
                        }
                    }
                    Some(op) => Stmt::Assign {
                        name,
                        op,
                        value,
                        mutable,
                        visibility,
                    },
                }
            }
            ast::Stmt::IfStmt(i) => {
                let nested = self.lower_if_stmt(i);
                return nested;
            }
            ast::Stmt::WhileStmt(w) => {
                let condition = w
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let body = self.lower_block(w.body());
                Stmt::While { condition, body }
            }
            ast::Stmt::ForStmt(f) => {
                let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let iterable = f
                    .iterable()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                let body = self.lower_block(f.body());
                Stmt::For {
                    name,
                    iterable,
                    body,
                }
            }
            ast::Stmt::MatchStmt(m) => {
                let subject = m
                    .subject()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                let mut cases = Vec::new();
                let mut otherwise = None;
                for case in m.cases() {
                    match case {
                        ast::MatchCaseItem::Case(c) => {
                            let labels = c.labels().map(|p| self.lower_pattern(p)).collect();
                            let body = if let Some(block) = c.block() {
                                self.lower_block(Some(block))
                            } else {
                                c.statement()
                                    .map(|s| vec![self.lower_stmt(s)])
                                    .unwrap_or_default()
                            };
                            cases.push(MatchCase { labels, body });
                        }
                        ast::MatchCaseItem::Otherwise(c) => {
                            let body = if let Some(block) = c.block() {
                                self.lower_block(Some(block))
                            } else {
                                c.statement()
                                    .map(|s| vec![self.lower_stmt(s)])
                                    .unwrap_or_default()
                            };
                            otherwise = Some(body);
                        }
                    }
                }
                Stmt::Match {
                    subject,
                    cases,
                    otherwise,
                }
            }
            ast::Stmt::IgnoreResultStmt(d) => {
                let expr = d
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::IgnoreResult { expr }
            }
            ast::Stmt::CaptureStmt(c) => {
                let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let value = c
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::Capture { name, value }
            }
            ast::Stmt::DeferStmt(d) => {
                let expr = d
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::Defer { expr }
            }
            ast::Stmt::OptimizeStmt(o) => {
                let objective = o
                    .objective()
                    .and_then(|t| Objective::from_str(t.text()))
                    .unwrap_or(Objective::Balance);
                let body = self.lower_block(o.block());
                Stmt::Optimize { objective, body }
            }
            ast::Stmt::AssertStmt(a) => {
                let expr = a
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let kind = match a.mode() {
                    ast::AssertMode::Value => crate::hir::AssertKind::Value,
                    ast::AssertMode::Identity => crate::hir::AssertKind::Identity,
                };
                Stmt::Assert { kind, expr }
            }
            ast::Stmt::RequireStmt(r) => {
                let condition = r
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let message = r
                    .message()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(
                            Expr::Literal(Literal::String(SmolStr::new(""))),
                            self.empty_span(),
                        )
                    });
                Stmt::Require { condition, message }
            }
            ast::Stmt::UseStmt(u) => {
                let (names, module, _module_span) = parse_use_stmt(&u);
                Stmt::Use { names, module }
            }
            ast::Stmt::ReturnStmt(r) => {
                let value = r.value().and_then(|e| self.lower_expr(e));
                Stmt::Return(value)
            }
            ast::Stmt::BreakStmt(_) => Stmt::Break,
            ast::Stmt::ContinueStmt(_) => Stmt::Continue,
            _ => Stmt::Break, // Error recovery or unsupported statement
        };
        let stmt_span = name_span.unwrap_or(stmt_span);
        self.alloc_stmt(hir_stmt, stmt_span)
    }

    fn lower_block(&mut self, block: Option<ast::Block>) -> Vec<Idx<Stmt>> {
        let mut stmts = Vec::new();
        if let Some(b) = block {
            self.enter_scope();
            for stmt in b.statements() {
                stmts.push(self.lower_stmt(stmt));
            }
            self.exit_scope();
        }
        stmts
    }

    fn lower_if_stmt(&mut self, i: ast::IfStmt) -> Idx<Stmt> {
        let condition = i
            .condition()
            .and_then(|e| self.lower_expr(e))
            .unwrap_or_else(|| {
                self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
            });
        let then_branch = self.lower_block(i.then_block());
        let else_branch = if let Some(block) = i.else_block() {
            Some(self.lower_block(Some(block)))
        } else if let Some(else_if) = i.else_if() {
            let stmt = self.lower_if_stmt(else_if);
            Some(vec![stmt])
        } else {
            None
        };
        let stmt = Stmt::If {
            condition,
            then_branch,
            else_branch,
        };
        let span = i.syntax().text_range();
        self.alloc_stmt(stmt, span)
    }

    fn lower_pattern(&mut self, pattern: ast::Pattern) -> Pattern {
        if let Some(token) = pattern.literals().next() {
            let lit = match token.kind() {
                SyntaxKind::StringLiteral => Literal::String(parse_string_literal(token.text())),
                SyntaxKind::IntNumber => Literal::Integer(parse_int_literal(token.text())),
                SyntaxKind::FloatNumber => Literal::Float(parse_float_literal(token.text())),
                SyntaxKind::TrueKw => Literal::Boolean(true),
                SyntaxKind::FalseKw => Literal::Boolean(false),
                SyntaxKind::NothingKw => Literal::Nil,
                _ => Literal::Nil,
            };
            return Pattern::Literal(lit);
        }

        let parts: Vec<SmolStr> = pattern
            .name_tokens()
            .map(|t| SmolStr::new(t.text()))
            .collect();
        let args: Vec<Pattern> = pattern.args().map(|p| self.lower_pattern(p)).collect();

        if parts.len() == 1 && args.is_empty() {
            let name = parts[0].clone();
            if name.as_str() == "_" {
                Pattern::Wildcard
            } else {
                Pattern::Binding(name)
            }
        } else {
            Pattern::Path { parts, args }
        }
    }

    fn lower_expr(&mut self, expr: ast::Expr) -> Option<Idx<Expr>> {
        // println!("Lowering expr: {:?}", expr.syntax().kind());
        let expr_span = expr.syntax().text_range();
        let hir_expr = match expr {
            ast::Expr::Literal(l) => {
                let token = first_non_trivia_token(l.syntax())?;
                let lit = match token.kind() {
                    SyntaxKind::IntNumber => Literal::Integer(parse_int_literal(token.text())),
                    SyntaxKind::FloatNumber => Literal::Float(parse_float_literal(token.text())),
                    SyntaxKind::StringLiteral => {
                        Literal::String(parse_string_literal(token.text()))
                    }
                    SyntaxKind::TrueKw => Literal::Boolean(true),
                    SyntaxKind::FalseKw => Literal::Boolean(false),
                    SyntaxKind::NothingKw => Literal::Nil,
                    _ => return None,
                };
                Expr::Literal(lit)
            }
            ast::Expr::Ident(i) => {
                let name = i.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                Expr::Variable(name)
            }
            ast::Expr::Bin(b) => {
                let lhs = self.lower_expr(b.lhs()?)?;
                let rhs = self.lower_expr(b.rhs()?)?;
                let (op, op_span) = self.lower_binary_op(b.syntax())?;
                Expr::Binary {
                    lhs,
                    op,
                    rhs,
                    op_span,
                }
            }
            ast::Expr::Prefix(p) => {
                let target = self.lower_expr(p.expr()?)?;
                let (op, op_span) = self.lower_unary_op(p.syntax())?;
                if matches!(op, UnaryOp::Spawn) {
                    let target_span = p
                        .expr()
                        .map(|e| e.syntax().text_range())
                        .unwrap_or(expr_span);
                    let (size, objective) = self.parse_detach_tail(p.syntax(), target_span);
                    Expr::Detach {
                        target,
                        size,
                        objective,
                    }
                } else {
                    Expr::Unary {
                        op,
                        expr: target,
                        op_span,
                    }
                }
            }
            ast::Expr::Crash(c) => {
                let expr = self.lower_expr(c.expr()?)?;
                Expr::Crash { expr }
            }
            ast::Expr::TypeApply(t) => {
                let callee = self.lower_expr(t.callee()?)?;
                let type_args = t
                    .args()
                    .into_iter()
                    .map(|arg| self.lower_type_ref(arg))
                    .collect();
                Expr::TypeApply { callee, type_args }
            }
            ast::Expr::Call(c) => {
                let mut type_args = Vec::new();
                let callee_expr = c.callee()?;
                let callee = if let ast::Expr::TypeApply(t) = callee_expr {
                    type_args = t
                        .args()
                        .into_iter()
                        .map(|arg| self.lower_type_ref(arg))
                        .collect();
                    self.lower_expr(t.callee()?)?
                } else {
                    self.lower_expr(callee_expr)?
                };
                let args = c.args().filter_map(|a| self.lower_arg(a)).collect();
                Expr::Call {
                    callee,
                    args,
                    type_args,
                }
            }
            ast::Expr::Given(g) => {
                let mut type_args = Vec::new();
                let callee_expr = g.callee()?;
                let callee = if let ast::Expr::TypeApply(t) = callee_expr {
                    type_args = t
                        .args()
                        .into_iter()
                        .map(|arg| self.lower_type_ref(arg))
                        .collect();
                    self.lower_expr(t.callee()?)?
                } else {
                    self.lower_expr(callee_expr)?
                };
                let args = g.args().filter_map(|a| self.lower_arg(a)).collect();
                Expr::GivenCall {
                    callee,
                    args,
                    type_args,
                }
            }
            ast::Expr::Member(m) => {
                let object = self.lower_expr(m.object()?)?;
                let member = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let member_span = m.name().map(|t| t.text_range()).unwrap_or(expr_span);
                Expr::Member {
                    object,
                    member,
                    member_span,
                }
            }
            ast::Expr::Paren(p) => {
                return self.lower_expr(p.syntax().children().filter_map(ast::Expr::cast).next()?);
            }
            ast::Expr::List(l) => {
                let items = l.items().filter_map(|e| self.lower_expr(e)).collect();
                Expr::List(items)
            }
            ast::Expr::Map(m) => {
                let mut items = Vec::new();
                let mut iter = m.items().filter_map(|e| self.lower_expr(e));
                while let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                    items.push((key, value));
                }
                Expr::Map(items)
            }
            ast::Expr::StringInterp(s) => {
                let parts = self.lower_string_interp(s);
                Expr::StringInterp(parts)
            }
            ast::Expr::It(_i) => Expr::Variable(SmolStr::new("it")),
            ast::Expr::Its(_i) => Expr::Variable(SmolStr::new("its")),
            // All expression variants are handled above.
        };
        Some(self.alloc_expr(hir_expr, expr_span))
    }

    fn lower_arg(&mut self, arg: ast::Arg) -> Option<Arg> {
        match arg {
            ast::Arg::Positional(e) => {
                let span = e.syntax().text_range();
                Some(Arg::Positional {
                    value: self.lower_expr(e)?,
                    span,
                })
            }
            ast::Arg::Named(n) => {
                let name = n.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let span = n.syntax().text_range();
                let name_span = n.name().map(|t| t.text_range()).unwrap_or(span);
                let value = self.lower_expr(n.value()?)?;
                Some(Arg::Named {
                    name,
                    value,
                    span,
                    name_span,
                })
            }
        }
    }

    fn lower_binary_op(&self, node: &crate::parser::SyntaxNode) -> Option<(BinaryOp, TextRange)> {
        let op_tok = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::Plus
                        | SyntaxKind::Minus
                        | SyntaxKind::Star
                        | SyntaxKind::Slash
                        | SyntaxKind::Percent
                        | SyntaxKind::EqEq
                        | SyntaxKind::BangEq
                        | SyntaxKind::Less
                        | SyntaxKind::LessEq
                        | SyntaxKind::Greater
                        | SyntaxKind::GreaterEq
                        | SyntaxKind::AndKw
                        | SyntaxKind::OrKw
                        | SyntaxKind::OtherwiseKw
                        | SyntaxKind::Ampersand
                        | SyntaxKind::Pipe
                        | SyntaxKind::Caret
                        | SyntaxKind::ShiftLeft
                        | SyntaxKind::ShiftRight
                        | SyntaxKind::Range
                        | SyntaxKind::Equals
                        | SyntaxKind::PlusEq
                        | SyntaxKind::MinusEq
                        | SyntaxKind::StarEq
                        | SyntaxKind::SlashEq
                )
            })?;

        let op = match op_tok.kind() {
            SyntaxKind::Plus => BinaryOp::Add,
            SyntaxKind::Minus => BinaryOp::Sub,
            SyntaxKind::Star => BinaryOp::Mul,
            SyntaxKind::Slash => BinaryOp::Div,
            SyntaxKind::Percent => BinaryOp::Mod,
            SyntaxKind::EqEq => BinaryOp::Eq,
            SyntaxKind::BangEq => BinaryOp::Ne,
            SyntaxKind::Less => BinaryOp::Lt,
            SyntaxKind::LessEq => BinaryOp::Le,
            SyntaxKind::Greater => BinaryOp::Gt,
            SyntaxKind::GreaterEq => BinaryOp::Ge,
            SyntaxKind::AndKw => BinaryOp::And,
            SyntaxKind::OrKw => BinaryOp::Or,
            SyntaxKind::OtherwiseKw => BinaryOp::Otherwise,
            SyntaxKind::Ampersand => BinaryOp::BitAnd,
            SyntaxKind::Pipe => BinaryOp::BitOr,
            SyntaxKind::Caret => BinaryOp::BitXor,
            SyntaxKind::ShiftLeft => BinaryOp::Shl,
            SyntaxKind::ShiftRight => BinaryOp::Shr,
            SyntaxKind::Range => BinaryOp::Range,
            SyntaxKind::Equals => BinaryOp::Assign,
            SyntaxKind::PlusEq => BinaryOp::AddAssign,
            SyntaxKind::MinusEq => BinaryOp::SubAssign,
            SyntaxKind::StarEq => BinaryOp::MulAssign,
            SyntaxKind::SlashEq => BinaryOp::DivAssign,
            _ => return None,
        };
        Some((op, op_tok.text_range()))
    }

    fn lower_unary_op(&self, node: &crate::parser::SyntaxNode) -> Option<(UnaryOp, TextRange)> {
        let op_tok = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::Minus
                        | SyntaxKind::NotKw
                        | SyntaxKind::BitwiseNot
                        | SyntaxKind::AwaitKw
                        | SyntaxKind::ResolveKw
                        | SyntaxKind::DetachKw
                        | SyntaxKind::SpawnKw
                        | SyntaxKind::FireKw
                        | SyntaxKind::ErrKw
                )
            })?;

        let op = match op_tok.kind() {
            SyntaxKind::Minus => UnaryOp::Neg,
            SyntaxKind::NotKw => UnaryOp::Not,
            SyntaxKind::BitwiseNot => UnaryOp::BitNot,
            SyntaxKind::AwaitKw => UnaryOp::Await,
            SyntaxKind::ResolveKw => UnaryOp::Resolve,
            SyntaxKind::DetachKw => UnaryOp::Spawn,
            SyntaxKind::SpawnKw => UnaryOp::Spawn,
            SyntaxKind::FireKw => UnaryOp::Fire,
            SyntaxKind::ErrKw => UnaryOp::Err,
            _ => return None,
        };
        Some((op, op_tok.text_range()))
    }

    fn lower_string_interp(&mut self, s: ast::StringInterp) -> Vec<StringPart> {
        let mut parts = Vec::new();
        for element in s.syntax().children_with_tokens() {
            if let Some(token) = element.clone().into_token() {
                match token.kind() {
                    SyntaxKind::StringStart => {
                        let text = token.text();
                        let text = text.strip_prefix('"').unwrap_or(text);
                        let text = text.strip_suffix('{').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    SyntaxKind::StringPart => {
                        let text = token.text();
                        let text = text.strip_suffix('{').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    SyntaxKind::StringEnd => {
                        let text = token.text();
                        let text = text.strip_suffix('"').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    _ => {}
                }
            } else if let Some(node) = element.into_node() {
                if let Some(expr) = ast::Expr::cast(node) {
                    if let Some(expr) = self.lower_expr(expr) {
                        parts.push(StringPart::Expr(expr));
                    }
                }
            }
        }
        parts
    }

    fn parse_detach_tail(
        &self,
        node: &crate::parser::SyntaxNode,
        target_span: TextRange,
    ) -> (PoolSize, Option<Objective>) {
        let mut after_target = false;
        let mut size = PoolSize::Fixed(1);
        let mut objective = None;
        let mut iter = node.children_with_tokens().peekable();

        while let Some(child) = iter.next() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    if n.text_range() == target_span {
                        after_target = true;
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    if !after_target {
                        continue;
                    }
                    match t.kind() {
                        SyntaxKind::Star => {
                            while let Some(next) = iter.next() {
                                if let Some(tok) = next.into_token() {
                                    if tok.kind().is_trivia() {
                                        continue;
                                    }
                                    match tok.kind() {
                                        SyntaxKind::IntNumber => {
                                            let parsed = tok.text().parse::<i64>().unwrap_or(1);
                                            size = PoolSize::Fixed(parsed);
                                        }
                                        SyntaxKind::Ident => {
                                            if tok.text() == "n" {
                                                size = PoolSize::Auto;
                                            }
                                        }
                                        _ => {}
                                    }
                                    break;
                                }
                            }
                        }
                        SyntaxKind::OptimizeKw => {
                            while let Some(next) = iter.next() {
                                if let Some(tok) = next.into_token() {
                                    if tok.kind().is_trivia() {
                                        continue;
                                    }
                                    if tok.kind() == SyntaxKind::Ident {
                                        objective = Objective::from_str(tok.text());
                                    }
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        (size, objective)
    }
}

fn visibility_for_node_default(node: &crate::parser::SyntaxNode) -> Visibility {
    match visibility_for_node(node) {
        Some(visibility) => visibility,
        None => Visibility::Public,
    }
}

fn visibility_for_node(node: &crate::parser::SyntaxNode) -> Option<Visibility> {
    if has_token(node, SyntaxKind::PrivateKw) || has_private_block_ancestor(node) {
        Some(Visibility::Private)
    } else {
        None
    }
}

fn has_token(node: &crate::parser::SyntaxNode, kind: SyntaxKind) -> bool {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .any(|token| token.kind() == kind)
}

fn has_private_block_ancestor(node: &crate::parser::SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::PrivateBlock {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn parse_use_stmt(u: &ast::UseStmt) -> (Vec<UseName>, SmolStr, Option<TextRange>) {
    let mut names = Vec::new();
    let mut module_parts: Vec<String> = Vec::new();
    let mut in_module = false;
    let mut module_span: Option<TextRange> = None;

    for token in u
        .syntax()
        .children_with_tokens()
        .filter_map(|it| it.into_token())
    {
        match token.kind() {
            SyntaxKind::FromKw => {
                in_module = true;
            }
            SyntaxKind::Ident => {
                if in_module {
                    module_span = Some(
                        module_span
                            .map(|span| span.cover(token.text_range()))
                            .unwrap_or_else(|| token.text_range()),
                    );
                    module_parts.push(token.text().to_string());
                } else {
                    names.push(UseName {
                        kind: UseNameKind::Name(SmolStr::new(token.text())),
                        span: token.text_range(),
                    });
                }
            }
            SyntaxKind::Star => {
                if !in_module {
                    names.push(UseName {
                        kind: UseNameKind::Glob,
                        span: token.text_range(),
                    });
                }
            }
            SyntaxKind::Slash | SyntaxKind::Dot => {
                // separators in module path
            }
            _ => {}
        }
    }

    let module = if module_parts.is_empty() {
        SmolStr::new("")
    } else {
        SmolStr::new(module_parts.join("/"))
    };
    (names, module, module_span)
}

fn assign_op_for_node(node: &crate::parser::SyntaxNode) -> Option<AssignOp> {
    let op_tok = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| {
            matches!(
                it.kind(),
                SyntaxKind::Equals
                    | SyntaxKind::PlusEq
                    | SyntaxKind::MinusEq
                    | SyntaxKind::StarEq
                    | SyntaxKind::SlashEq
            )
        })?;

    match op_tok.kind() {
        SyntaxKind::Equals => Some(AssignOp::Assign),
        SyntaxKind::PlusEq => Some(AssignOp::AddAssign),
        SyntaxKind::MinusEq => Some(AssignOp::SubAssign),
        SyntaxKind::StarEq => Some(AssignOp::MulAssign),
        SyntaxKind::SlashEq => Some(AssignOp::DivAssign),
        _ => None,
    }
}

fn parse_int_literal(text: &str) -> i64 {
    let cleaned = text.replace('_', "");
    if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).unwrap_or_default();
    }
    if let Some(bin) = cleaned
        .strip_prefix("0b")
        .or_else(|| cleaned.strip_prefix("0B"))
    {
        return i64::from_str_radix(bin, 2).unwrap_or_default();
    }
    if let Some(oct) = cleaned
        .strip_prefix("0o")
        .or_else(|| cleaned.strip_prefix("0O"))
    {
        return i64::from_str_radix(oct, 8).unwrap_or_default();
    }
    cleaned.parse::<i64>().unwrap_or_default()
}

fn parse_float_literal(text: &str) -> f64 {
    let cleaned = text.replace('_', "");
    cleaned.parse::<f64>().unwrap_or_default()
}

fn parse_string_literal(text: &str) -> SmolStr {
    let mut raw = text;
    if let Some(stripped) = raw.strip_prefix('"') {
        raw = stripped;
    }
    if let Some(stripped) = raw.strip_suffix('"') {
        raw = stripped;
    }
    parse_string_fragment(raw)
}

fn parse_string_fragment(text: &str) -> SmolStr {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            while matches!(chars.peek(), Some('\\')) {
                chars.next();
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    SmolStr::new(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_lower_minimal() {
        let node = parse("1");
        let root = ast::Root::cast(node).unwrap();
        let _module = lower(root);
    }

    #[test]
    fn test_lower_basic() {
        let input = "to add(a: Integer, b: Integer) -> Integer:\n    return a + b";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[Idx::new(0)];
        assert_eq!(func.name, "add");
        assert_eq!(func.visibility, Visibility::Public);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[1].name, "b");

        let body = func.body.as_ref().unwrap();
        assert_eq!(body.root_stmts.len(), 1);
    }

    #[test]
    fn test_lower_type_args() {
        let input = "to f(x: Result[Integer, Error]) -> List[String]:\n    return []";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let func = &module.functions[Idx::new(0)];
        let param_ty = func.params[0].ty.as_ref().unwrap();
        assert_eq!(param_ty.name, "Result");
        assert_eq!(param_ty.args.len(), 2);
        assert_eq!(param_ty.args[0].name, "Integer");
        assert_eq!(param_ty.args[1].name, "Error");

        let ret_ty = func.ret_type.as_ref().unwrap();
        assert_eq!(ret_ty.name, "List");
        assert_eq!(ret_ty.args.len(), 1);
        assert_eq!(ret_ty.args[0].name, "String");
    }

    #[test]
    fn test_lower_field_defaults() {
        let input = "\
A Defaults:
    has:
        name: String = \"ok\"
        count: Integer = 3
        flags: List = [true, false]
        meta: Map = {\"a\": 1}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let class = &module.classes[Idx::new(0)];
        assert_eq!(class.name, "Defaults");
        assert_eq!(class.fields.len(), 4);

        match class.fields[0].default.as_ref().unwrap() {
            FieldDefault::Literal(Literal::String(val)) => assert_eq!(val.as_str(), "ok"),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[1].default.as_ref().unwrap() {
            FieldDefault::Literal(Literal::Integer(val)) => assert_eq!(*val, 3),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[2].default.as_ref().unwrap() {
            FieldDefault::List(items) => assert_eq!(items.len(), 2),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[3].default.as_ref().unwrap() {
            FieldDefault::Map(items) => assert_eq!(items.len(), 1),
            other => panic!("unexpected default: {other:?}"),
        }
    }

    #[test]
    fn test_lower_for_match_use() {
        let input = "\
use:
    std,
    io
from core

to f() -> Integer:
    for i in [1, 2]:
        if i == 1:
            break
    match x:
        1: return 1
        otherwise: return 1
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.uses.len(), 1);
        let use_stmt = &module.uses[0];
        assert_eq!(
            use_stmt
                .names
                .iter()
                .filter_map(|name| name.name().cloned())
                .collect::<Vec<_>>(),
            vec![SmolStr::new("std"), SmolStr::new("io")]
        );
        assert_eq!(use_stmt.module, "core");

        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        assert_eq!(body.root_stmts.len(), 2);

        assert!(matches!(&body.stmts[body.root_stmts[0]], Stmt::For { .. }));
        assert!(matches!(
            &body.stmts[body.root_stmts[1]],
            Stmt::Match { .. }
        ));
    }

    #[test]
    fn test_lower_string_interp_and_ops() {
        let input = "\
to f() -> String:
    return \"hi {name}\" 
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Return(Some(expr)) = stmt else {
            panic!("Expected return with expr");
        };
        let Expr::StringInterp(parts) = &body.exprs[*expr] else {
            panic!("Expected string interpolation");
        };
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[0], StringPart::Literal(_)));
        assert!(matches!(parts[1], StringPart::Expr(_)));
        assert!(matches!(parts[2], StringPart::Literal(_)));
    }

    #[test]
    fn test_lower_unary_and_binary_ops() {
        let input = "\
to f() -> Result:
    return await detach Whale(name=\"moby\") * 1
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Return(Some(expr)) = stmt else {
            panic!("Expected return with expr");
        };
        let Expr::Unary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected unary expr");
        };
        assert_eq!(*op, UnaryOp::Await);
    }

    #[test]
    fn test_lower_range_op() {
        let input = "\
to f() -> Nothing:
    1...3
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt");
        };
        let Expr::Binary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::Range);
    }

    #[test]
    fn test_lower_map_member_and_named_args() {
        let input = "\
use std, io from core

to f() -> Map:
    foo(a=1, b=2)
    foo.bar
    return {a: 1, b: 2}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        assert_eq!(module.uses.len(), 1);
        let use_stmt = &module.uses[0];
        assert_eq!(
            use_stmt
                .names
                .iter()
                .filter_map(|name| name.name().cloned())
                .collect::<Vec<_>>(),
            vec![SmolStr::new("std"), SmolStr::new("io")]
        );
        assert_eq!(use_stmt.module, "core");

        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();

        let Stmt::Expr(call_expr) = &body.stmts[body.root_stmts[0]] else {
            panic!("Expected call expr stmt");
        };
        let Expr::Call { args, .. } = &body.exprs[*call_expr] else {
            panic!("Expected call expr");
        };
        assert_eq!(args.len(), 2, "args: {:?}", args);
        assert!(matches!(args[0], Arg::Named { .. }));
        assert!(matches!(args[1], Arg::Named { .. }));

        let Stmt::Expr(member_expr) = &body.stmts[body.root_stmts[1]] else {
            panic!("Expected member expr stmt");
        };
        let Expr::Member { member, .. } = &body.exprs[*member_expr] else {
            panic!("Expected member expr");
        };
        assert_eq!(member, "bar");

        let Stmt::Return(Some(ret_expr)) = &body.stmts[body.root_stmts[2]] else {
            panic!("Expected return stmt");
        };
        let Expr::Map(items) = &body.exprs[*ret_expr] else {
            panic!("Expected map expr");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_lower_bitwise_and_shift_ops() {
        let input = "\
to f() -> Nothing:
    1 << 2
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt, got: {:?}", stmt);
        };
        let Expr::Binary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::Shl);
    }

    #[test]
    fn test_lower_member_assign_expr() {
        let input = "\
A Counter:
    has:
        value: Integer
    can add(delta: Integer) -> Nothing:
        its.value += delta
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt, got: {:?}", stmt);
        };
        let Expr::Binary { lhs, op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::AddAssign);
        let Expr::Member { member, .. } = &body.exprs[*lhs] else {
            panic!("Expected member lhs");
        };
        assert_eq!(member, "value");
    }
}

fn first_non_trivia_token(node: &crate::parser::SyntaxNode) -> Option<crate::parser::SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|token| !token.kind().is_trivia())
}
