use crate::hir::*;
use crate::parser::ast::{self, AstNode};
use crate::parser::kind::SyntaxKind;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

pub fn lower(root: ast::Root) -> Module {
    let mut ctx = LoweringContext::default();
    ctx.lower_module(root)
}

pub fn lower_root_body(root: ast::Root) -> Option<Body> {
    let mut body_ctx = BodyLoweringContext::new();
    let mut has_stmt = false;
    for stmt in root.statements() {
        match stmt {
            ast::Stmt::FuncDef(_) | ast::Stmt::ClassDef(_) | ast::Stmt::UseStmt(_) => {}
            other => {
                let s = body_ctx.lower_stmt(other);
                body_ctx.body.root_stmts.push(s);
                has_stmt = true;
            }
        }
    }
    if has_stmt {
        Some(body_ctx.body)
    } else {
        None
    }
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
                ast::Stmt::ClassDef(c) => {
                    let class = self.lower_class(c);
                    self.module.classes.alloc(class);
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
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_class(&mut self, c: ast::ClassDef) -> Class {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for has_block in c.has_blocks() {
            for field in has_block.fields() {
                fields.push(self.lower_field(field));
            }
        }

        for method in c.methods() {
            let func = self.lower_method(method);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        Class {
            name,
            name_span,
            visibility,
            fields,
            methods,
        }
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
            params,
            ret_type,
            body: Some(body_ctx.body),
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
        }
    }

    fn lower_type_ref(&mut self, t: ast::TypeRef) -> TypeRef {
        TypeRef {
            name: t
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            name_span: t.name().map(|tok| tok.text_range()),
            args: t.args().into_iter().map(|arg| self.lower_type_ref(arg)).collect(),
        }
    }
}

struct BodyLoweringContext {
    body: Body,
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
                let mutable = has_token(v.syntax(), SyntaxKind::ChangingKw);
                let value = v
                    .value()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                match assign_op_for_node(v.syntax()) {
                    Some(AssignOp::Assign) | None => Stmt::Let {
                        name,
                        value,
                        mutable,
                        visibility,
                    },
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
                let condition = i
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Bool(false)), self.empty_span())
                    });
                let then_branch = self.lower_block(i.then_block());
                let else_branch = i.else_block().map(|b| self.lower_block(Some(b)));
                Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                }
            }
            ast::Stmt::WhileStmt(w) => {
                let condition = w
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Bool(false)), self.empty_span())
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
                            let labels = c.labels().filter_map(|e| self.lower_expr(e)).collect();
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
            for stmt in b.statements() {
                stmts.push(self.lower_stmt(stmt));
            }
        }
        stmts
    }

    fn lower_expr(&mut self, expr: ast::Expr) -> Option<Idx<Expr>> {
        // println!("Lowering expr: {:?}", expr.syntax().kind());
        let expr_span = expr.syntax().text_range();
        let hir_expr = match expr {
            ast::Expr::Literal(l) => {
                let token = first_non_trivia_token(l.syntax())?;
                let lit = match token.kind() {
                    SyntaxKind::IntNumber => Literal::Int(token.text().parse().unwrap_or(0)),
                    SyntaxKind::FloatNumber => Literal::Float(token.text().parse().unwrap_or(0.0)),
                    SyntaxKind::StringLiteral => {
                        Literal::String(SmolStr::new(token.text().trim_matches('"')))
                    }
                    SyntaxKind::TrueKw => Literal::Bool(true),
                    SyntaxKind::FalseKw => Literal::Bool(false),
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
                let expr = self.lower_expr(p.expr()?)?;
                let (op, op_span) = self.lower_unary_op(p.syntax())?;
                Expr::Unary { op, expr, op_span }
            }
            ast::Expr::Crash(c) => {
                let expr = self.lower_expr(c.expr()?)?;
                Expr::Crash { expr }
            }
            ast::Expr::Call(c) => {
                let callee = self.lower_expr(c.callee()?)?;
                let args = c.args().filter_map(|a| self.lower_arg(a)).collect();
                Expr::Call { callee, args }
            }
            ast::Expr::Member(m) => {
                let object = self.lower_expr(m.object()?)?;
                let member = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let member_span = m
                    .name()
                    .map(|t| t.text_range())
                    .unwrap_or(expr_span);
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
            ast::Expr::Its(_i) => Expr::Variable(SmolStr::new("it")),
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
                let name_span = n
                    .name()
                    .map(|t| t.text_range())
                    .unwrap_or(span);
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

    fn lower_binary_op(
        &self,
        node: &crate::parser::SyntaxNode,
    ) -> Option<(BinaryOp, TextRange)> {
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

    fn lower_unary_op(
        &self,
        node: &crate::parser::SyntaxNode,
    ) -> Option<(UnaryOp, TextRange)> {
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
                        parts.push(StringPart::Literal(SmolStr::new(text)));
                    }
                    SyntaxKind::StringPart => {
                        parts.push(StringPart::Literal(SmolStr::new(token.text())));
                    }
                    SyntaxKind::StringEnd => {
                        let text = token.text();
                        let text = text.strip_suffix('"').unwrap_or(text);
                        parts.push(StringPart::Literal(SmolStr::new(text)));
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
}

fn visibility_for_node_default(node: &crate::parser::SyntaxNode) -> Visibility {
    match visibility_for_node(node) {
        Some(visibility) => visibility,
        None => Visibility::Private,
    }
}

fn visibility_for_node(node: &crate::parser::SyntaxNode) -> Option<Visibility> {
    if has_token(node, SyntaxKind::PublicKw) {
        Some(Visibility::Public)
    } else if has_token(node, SyntaxKind::PrivateKw) {
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

fn parse_use_stmt(u: &ast::UseStmt) -> (Vec<UseName>, SmolStr, Option<TextRange>) {
    let mut names = Vec::new();
    let mut module_parts: Vec<String> = Vec::new();
    let mut in_module = false;
    let mut module_span: Option<TextRange> = None;

    for token in u.syntax().children_with_tokens().filter_map(|it| it.into_token()) {
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
        let input = "to add(a: Int, b: Int) -> Int:\n    return a + b";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[Idx::new(0)];
        assert_eq!(func.name, "add");
        assert_eq!(func.visibility, Visibility::Private);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[1].name, "b");

        let body = func.body.as_ref().unwrap();
        assert_eq!(body.root_stmts.len(), 1);
    }

    #[test]
    fn test_lower_type_args() {
        let input = "to f(x: Result[Int, Error]) -> List[String]:\n    return []";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let func = &module.functions[Idx::new(0)];
        let param_ty = func.params[0].ty.as_ref().unwrap();
        assert_eq!(param_ty.name, "Result");
        assert_eq!(param_ty.args.len(), 2);
        assert_eq!(param_ty.args[0].name, "Int");
        assert_eq!(param_ty.args[1].name, "Error");

        let ret_ty = func.ret_type.as_ref().unwrap();
        assert_eq!(ret_ty.name, "List");
        assert_eq!(ret_ty.args.len(), 1);
        assert_eq!(ret_ty.args[0].name, "String");
    }

    #[test]
    fn test_lower_for_match_use() {
        let input = "\
use:
    std,
    io
from core

to f():
    for i in [1, 2]:
        if i == 1:
            break
    match x:
        1: return it
        otherwise: return it
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

        assert!(matches!(
            &body.stmts[body.root_stmts[0]],
            Stmt::For { .. }
        ));
        assert!(matches!(
            &body.stmts[body.root_stmts[1]],
            Stmt::Match { .. }
        ));
    }

    #[test]
    fn test_lower_string_interp_and_ops() {
        let input = "\
to f():
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
to f():
    return await spawn Whale(name=\"moby\")
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
to f():
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

to f():
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
to f():
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
}

fn first_non_trivia_token(
    node: &crate::parser::SyntaxNode,
) -> Option<crate::parser::SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|token| !token.kind().is_trivia())
}
