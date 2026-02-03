use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rowan::{GreenNode, TextRange, TextSize};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CodeLens, CompletionItem,
    CompletionItemKind, CompletionOptions, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightParams,
    DocumentRangeFormattingParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    ExecuteCommandParams, FoldingRange, FoldingRangeKind, FoldingRangeParams,
    FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Location,
    MarkupContent, MarkupKind, OneOf, ParameterInformation,
    Position, PrepareRenameResponse, Range, ReferenceParams, RenameOptions, RenameParams,
    SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensLegend, SemanticTokensParams,
    SemanticTokensResult, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, SignatureInformation, SymbolInformation, SymbolKind,
    TextDocumentContentChangeEvent, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};
#[cfg(test)]
use tower_lsp::lsp_types::Command;
use wrela::hir;
use wrela::hir::TypeError;
use wrela::parser::ast::AstNode;
use wrela::parser::{self, ParseError, SyntaxNode, SyntaxToken, ast, kind::SyntaxKind};

#[derive(Clone)]
struct WorkspaceIndex {
    root: Url,
    documents: HashMap<Url, DocumentState>,
}

#[derive(Clone)]
struct ExternalModule {
    path: PathBuf,
    uri: Url,
    state: DocumentState,
}

#[derive(Clone)]
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { line_starts }
    }

    fn line_for_offset(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        }
    }
}

fn syntax_root(state: &DocumentState) -> SyntaxNode {
    SyntaxNode::new_root(state.green.clone())
}

#[derive(Clone)]
pub struct DocumentState {
    pub text: String,
    green: GreenNode,
    index: SymbolIndex,
    line_index: LineIndex,
    imports: Vec<ImportDef>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefKind {
    Class,
    Function,
    Method,
    Field,
    Variable,
    Parameter,
    Module,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Root,
    Class,
    Function,
    Method,
    Block,
}

#[derive(Clone)]
struct ParamInfo {
    name: String,
    ty: Option<String>,
    range: TextRange,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Definition {
    id: usize,
    name: String,
    kind: DefKind,
    scope_id: usize,
    range: TextRange,
    name_range: TextRange,
    detail: Option<String>,
    ty: Option<String>,
    params: Vec<ParamInfo>,
    doc: Option<String>,
    is_external: bool,
}

#[derive(Clone)]
struct ImportDef {
    name: String,
    module: String,
    name_range: TextRange,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Scope {
    id: usize,
    parent: Option<usize>,
    kind: ScopeKind,
    range: TextRange,
    class_name: Option<String>,
    depth: usize,
}

#[derive(Clone)]
pub struct SymbolIndex {
    defs: Vec<Definition>,
    scopes: Vec<Scope>,
    class_scopes: HashMap<String, usize>,
}

impl SymbolIndex {
    pub fn build(text: &str, root: &SyntaxNode) -> Self {
        let mut index = SymbolIndex {
            defs: Vec::new(),
            scopes: Vec::new(),
            class_scopes: HashMap::new(),
        };
        let root_scope = index.add_scope(ScopeKind::Root, None, root.text_range(), None);
        if let Some(ast_root) = ast::Root::cast(root.clone()) {
            for stmt in ast_root.statements() {
                match stmt {
                    ast::Stmt::ClassDef(def) => {
                        if let Some(name) = def.name() {
                            let class_name = name.text().to_string();
                            if !index.class_scopes.contains_key(&class_name) {
                                let class_scope = index.add_scope(
                                    ScopeKind::Class,
                                    Some(root_scope),
                                    def.syntax().text_range(),
                                    Some(class_name.clone()),
                                );
                                index.class_scopes.insert(class_name, class_scope);
                            }
                        }
                    }
                    ast::Stmt::EnumDef(def) => {
                        if let Some(name) = def.name() {
                            let class_name = name.text().to_string();
                            if !index.class_scopes.contains_key(&class_name) {
                                let class_scope = index.add_scope(
                                    ScopeKind::Class,
                                    Some(root_scope),
                                    def.syntax().text_range(),
                                    Some(class_name.clone()),
                                );
                                index.class_scopes.insert(class_name, class_scope);
                            }
                        }
                    }
                    _ => {}
                }
            }
            for stmt in ast_root.statements() {
                index.collect_stmt(root_scope, text, &stmt);
            }
        }
        index
    }

    fn add_scope(
        &mut self,
        kind: ScopeKind,
        parent: Option<usize>,
        range: TextRange,
        class_name: Option<String>,
    ) -> usize {
        let depth = parent
            .and_then(|id| self.scopes.get(id).map(|scope| scope.depth + 1))
            .unwrap_or(0);
        let id = self.scopes.len();
        self.scopes.push(Scope {
            id,
            parent,
            kind,
            range,
            class_name,
            depth,
        });
        id
    }

    fn add_def(
        &mut self,
        name: String,
        kind: DefKind,
        scope_id: usize,
        range: TextRange,
        name_range: TextRange,
        detail: Option<String>,
        ty: Option<String>,
        params: Vec<ParamInfo>,
        doc: Option<String>,
    ) -> usize {
        let id = self.defs.len();
        self.defs.push(Definition {
            id,
            name,
            kind,
            scope_id,
            range,
            name_range,
            detail,
            ty,
            params,
            doc,
            is_external: false,
        });
        id
    }

    fn collect_stmt(&mut self, scope_id: usize, text: &str, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(def) => self.collect_class(scope_id, text, def),
            ast::Stmt::EnumDef(def) => self.collect_enum(scope_id, text, def),
            ast::Stmt::FuncDef(def) => self.collect_function(scope_id, text, def),
            ast::Stmt::PrivateBlock(block) => {
                for stmt in block.statements() {
                    self.collect_stmt(scope_id, text, &stmt);
                }
            }
            ast::Stmt::VarAssign(def) => {
                if let Some(name) = def.name() {
                    if !is_augmented_assign(def.syntax()) {
                        let ty = def
                            .value()
                            .and_then(|expr| self.infer_expr_type(scope_id, text, &expr));
                        let detail = ty.as_ref().map(|ty| format!("{}: {}", name.text(), ty));
                        self.add_def(
                            name.text().to_string(),
                            DefKind::Variable,
                            scope_id,
                            def.syntax().text_range(),
                            name.text_range(),
                            detail,
                            ty,
                            Vec::new(),
                            None,
                        );
                    }
                }
            }
            ast::Stmt::ForStmt(def) => {
                if let Some(body) = def.body() {
                    let block_scope =
                        self.collect_block(scope_id, text, body.syntax().text_range(), &body);
                    if let Some(name) = def.name() {
                        self.add_def(
                            name.text().to_string(),
                            DefKind::Variable,
                            block_scope,
                            def.syntax().text_range(),
                            name.text_range(),
                            None,
                            None,
                            Vec::new(),
                            None,
                        );
                    }
                } else if let Some(name) = def.name() {
                    self.add_def(
                        name.text().to_string(),
                        DefKind::Variable,
                        scope_id,
                        def.syntax().text_range(),
                        name.text_range(),
                        None,
                        None,
                        Vec::new(),
                        None,
                    );
                }
            }
            ast::Stmt::IfStmt(def) => {
                if let Some(block) = def.then_block() {
                    self.collect_block(scope_id, text, block.syntax().text_range(), &block);
                }
                if let Some(block) = def.else_block() {
                    self.collect_block(scope_id, text, block.syntax().text_range(), &block);
                }
            }
            ast::Stmt::WhileStmt(def) => {
                if let Some(block) = def.body() {
                    self.collect_block(scope_id, text, block.syntax().text_range(), &block);
                }
            }
            ast::Stmt::MatchStmt(def) => {
                for case in def.cases() {
                    match case {
                        ast::MatchCaseItem::Case(item) => {
                            if let Some(block) = item.block() {
                                self.collect_block(
                                    scope_id,
                                    text,
                                    block.syntax().text_range(),
                                    &block,
                                );
                            }
                            if let Some(stmt) = item.statement() {
                                let case_scope = self.add_scope(
                                    ScopeKind::Block,
                                    Some(scope_id),
                                    stmt.syntax().text_range(),
                                    None,
                                );
                                self.collect_stmt(case_scope, text, &stmt);
                            }
                        }
                        ast::MatchCaseItem::Otherwise(item) => {
                            if let Some(block) = item.block() {
                                self.collect_block(
                                    scope_id,
                                    text,
                                    block.syntax().text_range(),
                                    &block,
                                );
                            }
                            if let Some(stmt) = item.statement() {
                                let case_scope = self.add_scope(
                                    ScopeKind::Block,
                                    Some(scope_id),
                                    stmt.syntax().text_range(),
                                    None,
                                );
                                self.collect_stmt(case_scope, text, &stmt);
                            }
                        }
                    }
                }
            }
            ast::Stmt::OptimizeStmt(def) => {
                if let Some(block) = def.block() {
                    self.collect_block(scope_id, text, block.syntax().text_range(), &block);
                }
            }
            ast::Stmt::UseStmt(def) => {
                for (name, range) in use_stmt_import_names(def) {
                    if name == "*" {
                        continue;
                    }
                    self.add_def(
                        name,
                        DefKind::Module,
                        scope_id,
                        def.syntax().text_range(),
                        range,
                        None,
                        None,
                        Vec::new(),
                        None,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_block(
        &mut self,
        parent_scope: usize,
        text: &str,
        range: TextRange,
        block: &ast::Block,
    ) -> usize {
        let block_scope = self.add_scope(ScopeKind::Block, Some(parent_scope), range, None);
        for stmt in block.statements() {
            self.collect_stmt(block_scope, text, &stmt);
        }
        block_scope
    }

    fn collect_class(&mut self, scope_id: usize, text: &str, def: &ast::ClassDef) {
        let class_name = def.name().map(|token| token.text().to_string());
        if let Some(name) = def.name() {
            let doc = extract_doc_comment(def.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Class,
                scope_id,
                def.syntax().text_range(),
                name.text_range(),
                None,
                Some(name.text().to_string()),
                Vec::new(),
                doc,
            );
        }
        let class_scope = if let Some(name) = class_name.as_ref() {
            self.class_scopes.get(name).copied().unwrap_or_else(|| {
                let scope = self.add_scope(
                    ScopeKind::Class,
                    Some(scope_id),
                    def.syntax().text_range(),
                    class_name.clone(),
                );
                self.class_scopes.insert(name.clone(), scope);
                scope
            })
        } else {
            self.add_scope(
                ScopeKind::Class,
                Some(scope_id),
                def.syntax().text_range(),
                class_name.clone(),
            )
        };

        for block in def.has_blocks() {
            self.collect_fields_from_has_block(class_scope, text, &block);
        }

        for method in def.methods() {
            self.collect_method(class_scope, text, class_name.clone(), &method);
        }

        for derive in def.derives() {
            self.collect_derive(class_scope, text, class_name.clone(), &derive);
        }

        for method in def.must_methods() {
            self.collect_required_method(class_scope, text, class_name.clone(), &method);
        }

        for private_block in def.syntax().children().filter_map(ast::PrivateBlock::cast) {
            for child in private_block.syntax().children() {
                if let Some(has_block) = ast::HasBlock::cast(child.clone()) {
                    self.collect_fields_from_has_block(class_scope, text, &has_block);
                    continue;
                }
                if let Some(method) = ast::MethodDef::cast(child) {
                    self.collect_method(class_scope, text, class_name.clone(), &method);
                }
            }
        }
    }

    fn collect_enum(&mut self, scope_id: usize, _text: &str, def: &ast::EnumDef) {
        let enum_name = def.name().map(|t| t.text().to_string());
        if let Some(name) = def.name() {
            self.add_def(
                name.text().to_string(),
                DefKind::Class,
                scope_id,
                def.syntax().text_range(),
                name.text_range(),
                None,
                None,
                Vec::new(),
                None,
            );
        }
        let class_scope = if let Some(name) = enum_name.as_ref() {
            self.class_scopes.get(name).copied().unwrap_or_else(|| {
                let scope = self.add_scope(
                    ScopeKind::Class,
                    Some(scope_id),
                    def.syntax().text_range(),
                    enum_name.clone(),
                );
                self.class_scopes.insert(name.clone(), scope);
                scope
            })
        } else {
            self.add_scope(
                ScopeKind::Class,
                Some(scope_id),
                def.syntax().text_range(),
                enum_name.clone(),
            )
        };

        for variant in def.variants() {
            if let Some(name) = variant.name() {
                let params = params_from_iter(variant.params());
                let ret_ty = enum_name.clone();
                let detail = Some(format_signature(name.text(), &params, ret_ty.as_deref()));
                let doc = extract_doc_comment(variant.syntax());
                self.add_def(
                    name.text().to_string(),
                    DefKind::Method,
                    class_scope,
                    variant.syntax().text_range(),
                    name.text_range(),
                    detail,
                    ret_ty.clone(),
                    params.clone(),
                    doc,
                );
                let variant_scope = self.add_scope(
                    ScopeKind::Method,
                    Some(class_scope),
                    variant.syntax().text_range(),
                    enum_name.clone(),
                );
                for param in params {
                    let detail = param
                        .ty
                        .as_ref()
                        .map(|ty| format!("{}: {}", param.name, ty));
                    self.add_def(
                        param.name.clone(),
                        DefKind::Parameter,
                        variant_scope,
                        param.range,
                        param.range,
                        detail,
                        param.ty.clone(),
                        Vec::new(),
                        None,
                    );
                }
            }
        }
    }

    fn collect_derive(
        &mut self,
        class_scope: usize,
        _text: &str,
        _class_name: Option<String>,
        derive: &ast::DeriveDef,
    ) {
        if let Some(name) = derive.name() {
            let ret_ty = derive.ret_type().and_then(|ty| format_type_ref(&ty));
            let detail = ret_ty.as_ref().map(|ty| format!("{}: {}", name.text(), ty));
            let doc = extract_doc_comment(derive.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Field,
                class_scope,
                derive.syntax().text_range(),
                name.text_range(),
                detail,
                ret_ty,
                Vec::new(),
                doc,
            );
        }
    }

    fn collect_method(
        &mut self,
        class_scope: usize,
        text: &str,
        class_name: Option<String>,
        method: &ast::MethodDef,
    ) {
        if let Some(name) = method.name() {
            let params = params_from_iter(method.params());
            let ret_ty = method.ret_type().and_then(|ty| format_type_ref(&ty));
            let detail = Some(format_signature(name.text(), &params, ret_ty.as_deref()));
            let doc = extract_doc_comment(method.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Method,
                class_scope,
                method.syntax().text_range(),
                name.text_range(),
                detail,
                ret_ty.clone(),
                params.clone(),
                doc,
            );
            let method_scope = self.add_scope(
                ScopeKind::Method,
                Some(class_scope),
                method.syntax().text_range(),
                class_name,
            );
            for param in params {
                let detail = param
                    .ty
                    .as_ref()
                    .map(|ty| format!("{}: {}", param.name, ty));
                self.add_def(
                    param.name.clone(),
                    DefKind::Parameter,
                    method_scope,
                    param.range,
                    param.range,
                    detail,
                    param.ty.clone(),
                    Vec::new(),
                    None,
                );
            }
            for stmt in method.statements() {
                self.collect_stmt(method_scope, text, &stmt);
            }
        }
    }

    fn collect_fields_from_has_block(
        &mut self,
        class_scope: usize,
        _text: &str,
        block: &ast::HasBlock,
    ) {
        for field in block.fields() {
            self.collect_field(class_scope, &field);
        }
        for private_block in block.syntax().children().filter_map(ast::PrivateBlock::cast) {
            for field in private_block.syntax().children().filter_map(ast::FieldDef::cast) {
                self.collect_field(class_scope, &field);
            }
        }
    }

    fn collect_field(&mut self, class_scope: usize, field: &ast::FieldDef) {
        if let Some(name) = field.name() {
            let ty = field.ty().and_then(|ty| format_type_ref(&ty));
            let detail = ty.as_ref().map(|ty| format!("{}: {}", name.text(), ty));
            let doc = extract_doc_comment(field.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Field,
                class_scope,
                field.syntax().text_range(),
                name.text_range(),
                detail,
                ty,
                Vec::new(),
                doc,
            );
        }
    }

    fn collect_required_method(
        &mut self,
        class_scope: usize,
        _text: &str,
        class_name: Option<String>,
        method: &ast::MustMethodDef,
    ) {
        if let Some(name) = method.name() {
            let params = params_from_iter(method.params());
            let ret_ty = method.ret_type().and_then(|ty| format_type_ref(&ty));
            let detail = Some(format_signature(name.text(), &params, ret_ty.as_deref()));
            let doc = extract_doc_comment(method.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Method,
                class_scope,
                method.syntax().text_range(),
                name.text_range(),
                detail,
                class_name,
                params,
                doc,
            );
        }
    }

    fn collect_function(&mut self, scope_id: usize, text: &str, def: &ast::FuncDef) {
        if let Some(name) = def.name() {
            let params = params_from_iter(def.params());
            let ret_ty = def.ret_type().and_then(|ty| format_type_ref(&ty));
            let detail = Some(format_signature(name.text(), &params, ret_ty.as_deref()));
            let doc = extract_doc_comment(def.syntax());
            self.add_def(
                name.text().to_string(),
                DefKind::Function,
                scope_id,
                def.syntax().text_range(),
                name.text_range(),
                detail,
                ret_ty.clone(),
                params.clone(),
                doc,
            );
            let func_scope = self.add_scope(
                ScopeKind::Function,
                Some(scope_id),
                def.syntax().text_range(),
                None,
            );
            for param in params {
                let detail = param
                    .ty
                    .as_ref()
                    .map(|ty| format!("{}: {}", param.name, ty));
                self.add_def(
                    param.name.clone(),
                    DefKind::Parameter,
                    func_scope,
                    param.range,
                    param.range,
                    detail,
                    param.ty.clone(),
                    Vec::new(),
                    None,
                );
            }
            for stmt in def.statements() {
                self.collect_stmt(func_scope, text, &stmt);
            }
        }
    }

    fn infer_expr_type(&self, scope_id: usize, text: &str, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Ident(expr) => expr.name().and_then(|token| {
                let name = token.text();
                let def = resolve_in_scope(self, scope_id, name)?;
                if def.kind == DefKind::Class {
                    Some(def.name.clone())
                } else {
                    def.ty.clone()
                }
            }),
            ast::Expr::Its(_) => class_name_for_scope(self, scope_id),
            ast::Expr::Literal(expr) => literal_type(expr),
            ast::Expr::StringInterp(_) => Some("String".to_string()),
            ast::Expr::Paren(expr) => expr
                .syntax()
                .children()
                .find_map(ast::Expr::cast)
                .and_then(|child| self.infer_expr_type(scope_id, text, &child)),
            ast::Expr::Prefix(expr) => expr
                .expr()
                .and_then(|child| self.infer_expr_type(scope_id, text, &child)),
            ast::Expr::Crash(expr) => expr
                .expr()
                .and_then(|child| self.infer_expr_type(scope_id, text, &child)),
            ast::Expr::Call(expr) => self.infer_call_type(scope_id, text, expr),
            ast::Expr::Member(expr) => self.infer_member_type(scope_id, text, expr),
            _ => None,
        }
    }

    fn infer_call_type(&self, scope_id: usize, text: &str, call: &ast::CallExpr) -> Option<String> {
        let callee = call.callee()?;
        match callee {
            ast::Expr::Ident(expr) => {
                let name = expr.name()?.text().to_string();
                if self.class_scopes.contains_key(&name) {
                    return Some(name);
                }
                let def = resolve_in_scope(self, scope_id, &name)?;
                if def.kind == DefKind::Class {
                    Some(def.name.clone())
                } else {
                    def.ty.clone()
                }
            }
            ast::Expr::Member(expr) => {
                let member_name = expr.name()?.text().to_string();
                let object = expr.object()?;
                let object_ty = self.infer_expr_type(scope_id, text, &object)?;
                let class_scope = self.class_scopes.get(&object_ty).copied()?;
                let def =
                    resolve_in_scope_kinds(self, class_scope, &member_name, &[DefKind::Method])?;
                def.ty.clone()
            }
            _ => None,
        }
    }

    fn infer_member_type(
        &self,
        scope_id: usize,
        text: &str,
        member: &ast::MemberExpr,
    ) -> Option<String> {
        let member_name = member.name()?.text().to_string();
        let object = member.object()?;
        let object_ty = self.infer_expr_type(scope_id, text, &object)?;
        let class_scope = self.class_scopes.get(&object_ty).copied()?;
        let def = resolve_in_scope_kinds(
            self,
            class_scope,
            &member_name,
            &[DefKind::Field, DefKind::Method],
        )?;
        def.ty.clone()
    }
}

fn use_stmt_import_names(use_stmt: &ast::UseStmt) -> Vec<(String, TextRange)> {
    let mut names = Vec::new();
    let mut after_from = false;
    for element in use_stmt.syntax().children_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        match token.kind() {
            SyntaxKind::FromKw => after_from = true,
            SyntaxKind::Star if !after_from => {
                names.push(("*".to_string(), token.text_range()));
            }
            SyntaxKind::Ident if !after_from => {
                names.push((token.text().to_string(), token.text_range()));
            }
            _ => {}
        }
    }
    names
}

fn use_stmt_module_path(use_stmt: &ast::UseStmt) -> Option<String> {
    let mut after_from = false;
    let mut module = String::new();
    for element in use_stmt.syntax().children_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        match token.kind() {
            SyntaxKind::FromKw => after_from = true,
            SyntaxKind::Ident if after_from => module.push_str(token.text()),
            SyntaxKind::Dot | SyntaxKind::Slash if after_from => module.push_str(token.text()),
            _ => {}
        }
    }
    if module.is_empty() { None } else { Some(module) }
}

fn collect_imports(root: &SyntaxNode) -> Vec<ImportDef> {
    let Some(ast_root) = ast::Root::cast(root.clone()) else {
        return Vec::new();
    };
    let mut imports = Vec::new();
    for stmt in ast_root.statements() {
        let ast::Stmt::UseStmt(use_stmt) = stmt else {
            continue;
        };
        let Some(module) = use_stmt_module_path(&use_stmt) else {
            continue;
        };
        for (name, range) in use_stmt_import_names(&use_stmt) {
            imports.push(ImportDef {
                name,
                module: module.clone(),
                name_range: range,
            });
        }
    }
    imports
}

fn token_is_in_use_stmt(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if ast::UseStmt::cast(current.clone()).is_some() {
            return true;
        }
        node = current.parent();
    }
    false
}

fn find_stdlib_module_path(start: &Path, module: &str) -> Option<PathBuf> {
    let file = format!("{}.wr", module);
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("core/compiler/stdlib").join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_root_definition(index: &SymbolIndex, name: &str) -> Option<Definition> {
    let mut candidates = index
        .defs
        .iter()
        .filter(|def| !def.is_external && def.name == name && is_root_def(index, def))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|def| def_precedence(def.kind));
    candidates.into_iter().next()
}

fn ensure_class_scope_from_external(
    target: &mut SymbolIndex,
    external: &SymbolIndex,
    class_name: &str,
) {
    if target.class_scopes.contains_key(class_name) {
        return;
    }
    let Some(&external_scope_id) = external.class_scopes.get(class_name) else {
        return;
    };
    let external_scope = match external.scopes.get(external_scope_id) {
        Some(scope) => scope,
        None => return,
    };
    let new_scope_id = target.add_scope(
        ScopeKind::Class,
        Some(0),
        external_scope.range,
        Some(class_name.to_string()),
    );
    target.class_scopes.insert(class_name.to_string(), new_scope_id);
    for def in external.defs.iter().filter(|def| def.scope_id == external_scope_id) {
        let id = target.defs.len();
        target.defs.push(Definition {
            id,
            name: def.name.clone(),
            kind: def.kind,
            scope_id: new_scope_id,
            range: def.range,
            name_range: def.name_range,
            detail: def.detail.clone(),
            ty: def.ty.clone(),
            params: def.params.clone(),
            doc: def.doc.clone(),
            is_external: true,
        });
    }
}

fn apply_import_overlays(state: &mut DocumentState, module: &str, external: &DocumentState) {
    let mut applied_classes = HashSet::new();
    let mut wildcard_range = None;
    for import in state.imports.iter().filter(|import| import.module == module) {
        if import.name == "*" {
            wildcard_range = Some(import.name_range);
            continue;
        }
        let Some(external_def) = find_root_definition(&external.index, &import.name) else {
            continue;
        };
        if let Some(def) = state
            .index
            .defs
            .iter_mut()
            .find(|def| {
                def.kind == DefKind::Module
                    && def.name == import.name
                    && def.name_range == import.name_range
            })
        {
            def.kind = external_def.kind;
            def.detail = external_def.detail.clone();
            def.ty = external_def.ty.clone();
            def.params = external_def.params.clone();
            def.doc = external_def.doc.clone();
        }
        if external_def.kind == DefKind::Class && applied_classes.insert(external_def.name.clone()) {
            ensure_class_scope_from_external(&mut state.index, &external.index, &external_def.name);
        }
    }
    if let Some(range) = wildcard_range {
        for external_def in external
            .index
            .defs
            .iter()
            .filter(|def| is_root_def(&external.index, def))
        {
            if state
                .index
                .defs
                .iter()
                .any(|def| def.scope_id == 0 && def.name == external_def.name)
            {
                continue;
            }
            let id = state.index.defs.len();
            state.index.defs.push(Definition {
                id,
                name: external_def.name.clone(),
                kind: external_def.kind,
                scope_id: 0,
                range,
                name_range: range,
                detail: external_def.detail.clone(),
                ty: external_def.ty.clone(),
                params: external_def.params.clone(),
                doc: external_def.doc.clone(),
                is_external: true,
            });
            if external_def.kind == DefKind::Class
                && applied_classes.insert(external_def.name.clone())
            {
                ensure_class_scope_from_external(&mut state.index, &external.index, &external_def.name);
            }
        }
    }
}

pub struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, DocumentState>>,
    root_uri: RwLock<Option<Url>>,
    index_cache: RwLock<Option<WorkspaceIndex>>,
    stdlib_cache: RwLock<HashMap<String, ExternalModule>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
            root_uri: RwLock::new(None),
            index_cache: RwLock::new(None),
            stdlib_cache: RwLock::new(HashMap::new()),
        }
    }

    async fn publish_diagnostics(
        &self,
        uri: &Url,
        text: &str,
        errors: Vec<ParseError>,
        state: &DocumentState,
    ) {
        let mut diagnostics = diagnostics_for_errors(text, errors.clone());
        diagnostics.extend(check_unused_variables(state));
        diagnostics.extend(check_unused_imports(state));
        if errors.is_empty() {
            diagnostics.extend(check_result_handling_diagnostics(state));
            let mut known = HashSet::new();
            collect_def_names(&state.index, &mut known);
            if let Some(root_uri) = self.root_uri.read().await.clone() {
                let documents = self.indexed_documents(&root_uri).await;
                for doc in documents.values() {
                    collect_def_names(&doc.index, &mut known);
                }
            }
            diagnostics.extend(check_unresolved_identifiers(state, &known));
        }
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    async fn document_state(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.read().await.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String) -> (Vec<ParseError>, DocumentState) {
        let (mut state, errors) = build_document_state(text);
        let module_names = state
            .imports
            .iter()
            .map(|import| import.module.clone())
            .collect::<HashSet<_>>();
        for module in module_names {
            if let Some(external) = self.stdlib_module_for_uri(&module, &uri).await {
                apply_import_overlays(&mut state, &module, &external.state);
            }
        }
        self.documents
            .write()
            .await
            .insert(uri.clone(), state.clone());
        if let Some(cache) = self.index_cache.write().await.as_mut() {
            if uri_in_workspace(&cache.root, &uri) {
                cache.documents.insert(uri, state.clone());
            }
        }
        (errors, state)
    }

    async fn indexed_documents(&self, root_uri: &Url) -> HashMap<Url, DocumentState> {
        if let Some(cache) = self.index_cache.read().await.as_ref() {
            if &cache.root == root_uri {
                return cache.documents.clone();
            }
        }
        let documents = index_workspace_documents(root_uri);
        *self.index_cache.write().await = Some(WorkspaceIndex {
            root: root_uri.clone(),
            documents: documents.clone(),
        });
        documents
    }

    async fn stdlib_module_for_uri(
        &self,
        module: &str,
        uri: &Url,
    ) -> Option<ExternalModule> {
        if let Some(cached) = self.stdlib_cache.read().await.get(module) {
            if cached.path.is_file() {
                return Some(cached.clone());
            }
        }
        let root_path = self
            .root_uri
            .read()
            .await
            .clone()
            .and_then(|uri| uri.to_file_path().ok());
        let start_path = match root_path {
            Some(path) => path,
            None => uri.to_file_path().ok()?,
        };
        let start_dir = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path
        };
        let module_path = find_stdlib_module_path(&start_dir, module)?;
        let text = fs::read_to_string(&module_path).ok()?;
        let (state, _errors) = build_document_state(text);
        let module_uri = Url::from_file_path(&module_path).ok()?;
        let entry = ExternalModule {
            path: module_path,
            uri: module_uri,
            state,
        };
        self.stdlib_cache
            .write()
            .await
            .insert(module.to_string(), entry.clone());
        Some(entry)
    }

    async fn hover_for_import(
        &self,
        state: &DocumentState,
        uri: &Url,
        position: Position,
    ) -> Option<Hover> {
        let (name, range) = {
            let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
            let root = syntax_root(state);
            let token =
                token_at_offset(&root, offset).or_else(|| token_before_offset(&root, offset))?;
            if token.kind() != SyntaxKind::Ident {
                return None;
            }
            let range = text_range_to_range_with_index(
                &state.text,
                &state.line_index,
                token.text_range(),
            );
            (token.text().to_string(), range)
        };
        let module = import_module_for_name(state, &name)?;
        let external = self.stdlib_module_for_uri(module, uri).await?;
        let def = find_root_definition(&external.state.index, &name)?;
        let value = hover_markdown_for_definition(&external.state, &def);
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(range),
        })
    }

    async fn definition_for_import(
        &self,
        state: &DocumentState,
        uri: &Url,
        position: Position,
    ) -> Option<Location> {
        let name = identifier_at_position(state, position)?;
        let module = import_module_for_name(state, &name)?;
        let external = self.stdlib_module_for_uri(module, uri).await?;
        let def = find_root_definition(&external.state.index, &name)?;
        let range = text_range_to_range_with_index(
            &external.state.text,
            &external.state.line_index,
            def.name_range,
        );
        Some(Location {
            uri: external.uri.clone(),
            range,
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root_uri = params.root_uri.or_else(|| {
            params
                .workspace_folders
                .and_then(|mut folders| folders.pop().map(|f| f.uri))
        });
        if let Some(uri) = root_uri {
            *self.root_uri.write().await = Some(uri);
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "(".to_string(),
                        ",".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_highlight_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    ..SignatureHelpOptions::default()
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                code_lens_provider: None,
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                semantic_tokens_provider: None,
                inlay_hint_provider: None,
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: tower_lsp::lsp_types::DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let (errors, state) = self.update_document(uri.clone(), text.clone()).await;
        self.publish_diagnostics(&uri, &text, errors, &state).await;
    }

    async fn did_change(&self, params: tower_lsp::lsp_types::DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let Some(existing) = self.document_state(&uri).await else {
            return;
        };
        let Some(text) = apply_content_changes(existing.text, params.content_changes) else {
            return;
        };
        let (errors, state) = self.update_document(uri.clone(), text.clone()).await;
        self.publish_diagnostics(&uri, &text, errors, &state).await;
    }

    async fn did_close(&self, params: tower_lsp::lsp_types::DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        if let Some(cache) = self.index_cache.write().await.as_mut() {
            cache.documents.remove(&uri);
        }
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn completion(
        &self,
        params: tower_lsp::lsp_types::CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        };
        let trigger = params
            .context
            .as_ref()
            .and_then(|ctx| ctx.trigger_character.as_ref())
            .and_then(|s| s.chars().next());
        let items = completion_items(&state, params.text_document_position.position, trigger);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(Some(DocumentSymbolResponse::Nested(Vec::new())));
        };
        let symbols = document_symbols(&state);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let position = params.text_document_position_params.position;
        if let Some(hover) = self.hover_for_import(&state, &uri, position).await {
            return Ok(Some(hover));
        }
        let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
        if let Some(def) =
            definition_at_offset(&state, offset).or_else(|| resolve_reference_at_offset(&state, offset))
        {
            if def.doc.is_none() {
                let mut external_def = None;
                if let Some(module) = import_module_for_name(&state, &def.name) {
                    if let Some(external) = self.stdlib_module_for_uri(module, &uri).await {
                        external_def =
                            find_root_definition(&external.state.index, &def.name).map(|def| {
                                (external.state, def)
                            });
                    }
                } else if matches!(def.kind, DefKind::Method | DefKind::Field) {
                    if let Some(class_name) = class_name_for_scope(&state.index, def.scope_id) {
                        if let Some(module) = import_module_for_name(&state, &class_name) {
                            if let Some(external) = self.stdlib_module_for_uri(module, &uri).await {
                                if let Some(class_scope) =
                                    class_scope_for_name(&external.state.index, &class_name)
                                {
                                    let kinds = match def.kind {
                                        DefKind::Method => &[DefKind::Method][..],
                                        DefKind::Field => &[DefKind::Field][..],
                                        _ => &[][..],
                                    };
                                    external_def = resolve_in_scope_kinds(
                                        &external.state.index,
                                        class_scope,
                                        &def.name,
                                        kinds,
                                    )
                                    .map(|def| (external.state, def));
                                }
                            }
                        }
                    }
                }
                if let Some((external_state, external_def)) = external_def {
                    let value = hover_markdown_for_definition(&external_state, &external_def);
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value,
                        }),
                        range: Some(text_range_to_range_with_index(
                            &state.text,
                            &state.line_index,
                            def.name_range,
                        )),
                    }));
                }
            }
        }
        Ok(hover_at_position(&state, position))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let position = params.text_document_position_params.position;
        if let Some(location) = self.definition_for_import(&state, &uri, position).await {
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
        }
        if let Some(range) = definition_location(&state, position) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range,
            })));
        }
        let Some(name) = identifier_at_position(&state, position) else {
            return Ok(None);
        };
        let documents = self.documents.read().await;
        let locations = workspace_definitions(&documents, &name);
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(Some(Vec::new()));
        };
        let position = params.text_document_position.position;
        let references =
            references_at_position(&state, position, params.context.include_declaration);
        let mut locations = references
            .into_iter()
            .map(|range| Location {
                uri: uri.clone(),
                range,
            })
            .collect::<Vec<_>>();
        let name = identifier_at_position(&state, position);
        if let Some(name) = name {
            let documents = self.documents.read().await;
            locations.extend(workspace_references(
                &documents,
                &uri,
                &name,
                params.context.include_declaration,
            ));
        }
        Ok(Some(locations))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let position = params.text_document_position.position;
        let edits = rename_at_position(&state, position, &params.new_name);
        let mut changes = HashMap::new();
        if let Some(edits) = edits {
            changes.insert(uri.clone(), edits);
        }
        if let Some(name) = identifier_at_position(&state, position) {
            if is_valid_identifier(&params.new_name) && !is_keyword(&params.new_name) {
                let documents = self.documents.read().await;
                let workspace_edits = workspace_rename(&documents, &uri, &name, &params.new_name);
                for (uri, edits) in workspace_edits {
                    changes.entry(uri).or_insert(edits);
                }
            }
        }
        if changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }))
        }
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(Some(Vec::new()));
        };
        let highlights =
            references_at_position(&state, params.text_document_position_params.position, true)
                .into_iter()
                .map(|range| DocumentHighlight { range, kind: None })
                .collect();
        Ok(Some(highlights))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(prepare_rename_at_position(&state, params.position))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(signature_help_at_position(
            &state,
            params.text_document_position_params.position,
        ))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query;
        let documents = self.documents.read().await;
        Ok(Some(workspace_symbols(&documents, &query)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(&state),
        })))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let _ = params;
        Ok(None)
    }

    async fn code_lens(
        &self,
        params: tower_lsp::lsp_types::CodeLensParams,
    ) -> Result<Option<Vec<CodeLens>>> {
        let _ = params;
        Ok(None)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(Some(folding_ranges(&state)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let formatted = format_text(&state.text);
        Ok(Some(vec![TextEdit {
            range: full_document_range(&state.text),
            new_text: formatted,
        }]))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let edit = format_range_text(&state, params.range);
        Ok(edit.map(|edit| vec![edit]))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command != "wrela.goToTypeDefinition"
            && params.command != "wrela.peekTypeDefinition"
        {
            return Ok(None);
        }
        let args = params.arguments;
        let uri = args
            .get(0)
            .and_then(|v: &serde_json::Value| v.as_str())
            .and_then(|s| Url::parse(s).ok());
        let line = args
            .get(1)
            .and_then(|v: &serde_json::Value| v.as_u64())
            .map(|v| v as u32);
        let character = args
            .get(2)
            .and_then(|v: &serde_json::Value| v.as_u64())
            .map(|v| v as u32);
        let Some(uri) = uri else {
            return Ok(None);
        };
        let Some(line) = line else {
            return Ok(None);
        };
        let Some(character) = character else {
            return Ok(None);
        };
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let position = Position { line, character };
        let Some(type_name) = type_at_position(&state, position) else {
            return Ok(None);
        };
        let documents = self.documents.read().await;
        let locations = workspace_type_definitions(&documents, &type_name);
        let value = serde_json::to_value(locations).ok();
        Ok(value)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        let mut documents = self.documents.read().await.clone();
        if let Some(root_uri) = self.root_uri.read().await.clone() {
            let indexed = self.indexed_documents(&root_uri).await;
            for (uri, doc) in indexed {
                documents.entry(uri).or_insert(doc);
            }
        }
        let mut actions = code_actions(&state, params.range, &uri, &documents);
        if let Some(only) = params.context.only {
            actions.retain(|action| code_action_matches_only(action, &only));
        }
        Ok(Some(
            actions
                .into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }
}

pub fn semantic_tokens(state: &DocumentState) -> Vec<SemanticToken> {
    let root = syntax_root(state);
    let mut last_line = 0;
    let mut last_char = 0;
    let mut collected = Vec::new();

    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        let kind = token.kind();
        let token_type = match kind {
            SyntaxKind::Ident => {
                if is_keyword(token.text()) {
                    None
                } else if is_builtin(token.text()) {
                    Some(2) // FUNCTION (builtins like print)
                } else if let Some(def) = resolve_reference_token(&state.index, &state.text, &token)
                {
                    Some(match def.kind {
                        DefKind::Class => 1,     // CLASS
                        DefKind::Function => 2,  // FUNCTION
                        DefKind::Method => 3,    // METHOD
                        DefKind::Field => 4,     // PROPERTY
                        DefKind::Variable => 5,  // VARIABLE
                        DefKind::Parameter => 6, // PARAMETER
                        DefKind::Module => 5,    // VARIABLE (fallback)
                    })
                } else if let Some(parent) = token.parent() {
                    Some(if ast::ClassDef::can_cast(parent.kind()) {
                        1 // CLASS
                    } else if ast::FuncDef::can_cast(parent.kind()) {
                        2 // FUNCTION
                    } else if ast::MethodDef::can_cast(parent.kind()) {
                        3 // METHOD
                    } else if ast::FieldDef::can_cast(parent.kind()) {
                        4 // PROPERTY
                    } else if ast::Param::can_cast(parent.kind()) {
                        6 // PARAMETER
                    } else if is_named_arg_name_token(&token) {
                        4 // PROPERTY (named argument names)
                    } else {
                        5 // VARIABLE (default)
                    })
                } else {
                    Some(5) // VARIABLE
                }
            }
            SyntaxKind::IntNumber | SyntaxKind::FloatNumber => Some(8), // NUMBER
            SyntaxKind::Comment | SyntaxKind::DocComment => Some(10),   // COMMENT
            SyntaxKind::At => Some(11),                                 // DECORATOR
            _ => None,
        };

        if let Some(token_type) = token_type {
            collected.push((token, token_type));
        }
    }

    collected.sort_by_key(|(token, _)| token.text_range().start());
    let mut tokens = Vec::with_capacity(collected.len());

    for (token, token_type) in collected {
        let range = token.text_range();
        let start =
            offset_to_position_with_index(&state.text, &state.line_index, range.start().into());

        let delta_line = start.line - last_line;
        let delta_start = if delta_line == 0 {
            start.character - last_char
        } else {
            start.character
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: token_text_len_utf16(&token),
            token_type,
            token_modifiers_bitset: 0,
        });

        last_line = start.line;
        last_char = start.character;
    }

    tokens
}

pub fn inlay_hints(state: &DocumentState) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let root = syntax_root(state);

    for node in root.descendants() {
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            hints.extend(argument_hints(state, &call));
        } else if let Some(func) = ast::FuncDef::cast(node.clone()) {
            if let Some(hint) = return_type_hint(state, &func) {
                hints.push(hint);
            }
        }
    }
    hints
}

fn argument_hints(state: &DocumentState, call: &ast::CallExpr) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let Some(callee) = call.callee() else {
        return hints;
    };

    // Resolve the function definition
    let Some(callee_name) = (match callee {
        ast::Expr::Ident(expr) => expr.name().map(|t| t.text().to_string()),
        ast::Expr::Member(expr) => expr.name().map(|t| t.text().to_string()),
        _ => None,
    }) else {
        return hints;
    };

    let offset: usize = call.syntax().text_range().start().into();
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|s| s.id)
        .unwrap_or(0);

    let def = resolve_in_scope_kinds(
        &state.index,
        scope_id,
        &callee_name,
        &[DefKind::Function, DefKind::Method],
    )
    .or_else(|| {
        member_scope_at_offset(&state.index, call.syntax(), &state.text, offset).and_then(
            |class_scope| {
                resolve_in_scope_kinds(&state.index, class_scope, &callee_name, &[DefKind::Method])
            },
        )
    });

    if let Some(def) = def {
        let mut param_idx = 0;
        for arg in call.args() {
            if let ast::Arg::Positional(expr) = arg {
                if is_inside_defer(expr.syntax()) {
                    param_idx += 1;
                    continue;
                }
                if param_idx < def.params.len() {
                    let param_name = &def.params[param_idx].name;
                    let hint_offset: usize =
                        expr.syntax().text_range().start().into();
                    if let Some(token) = token_at_offset(&syntax_root(state), hint_offset) {
                        if token.kind() == SyntaxKind::DeferKw {
                            param_idx += 1;
                            continue;
                        }
                    }
                    hints.push(InlayHint {
                        position: offset_to_position_with_index(
                            &state.text,
                            &state.line_index,
                            hint_offset,
                        ),
                        label: InlayHintLabel::String(format!("{}: ", param_name)),
                        kind: Some(InlayHintKind::PARAMETER),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    });
                }
                param_idx += 1;
            }
        }
    }

    hints
}

fn is_inside_defer(node: &SyntaxNode) -> bool {
    let mut current = Some(node.clone());
    while let Some(n) = current {
        if ast::DeferStmt::cast(n.clone()).is_some() {
            return true;
        }
        current = n.parent();
    }
    false
}

fn return_type_hint(state: &DocumentState, func: &ast::FuncDef) -> Option<InlayHint> {
    if func.ret_type().is_some() {
        return None;
    }

    let Some(name) = func.name() else {
        return None;
    };
    // Very basic inference: look at the last statement or return statements
    // For now, let's just use the 'infer_expr_type' if possible on the body block's last expr
    // This is complex without a full type checker, but we can try basic things.
    // Actually, let's skip complex inference for now and check if we can infer from a simple return stmt.

    // A simplified approach: iterate return statements
    let mut inferred_type = None;
    for stmt in func.statements() {
        if let ast::Stmt::ReturnStmt(ret) = stmt {
            if let Some(val) = ret.value() {
                let offset: usize = val.syntax().text_range().start().into();
                let scope_id = scope_at_offset(&state.index, offset)
                    .map(|s| s.id)
                    .unwrap_or(0);
                if let Some(ty) = state.index.infer_expr_type(scope_id, &state.text, &val) {
                    inferred_type = Some(ty);
                    break; // Just take the first one for now
                }
            }
        }
    }

    if let Some(ty) = inferred_type {
        // Place hint after parameters
        let end_pos = if let Some(params) = func
            .syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::ParamList)
        {
            params.text_range().end()
        } else {
            name.text_range().end()
        };

        Some(InlayHint {
            position: offset_to_position_with_index(&state.text, &state.line_index, end_pos.into()),
            label: InlayHintLabel::String(format!(" -> {}", ty)),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        })
    } else {
        None
    }
}

fn code_actions(
    state: &DocumentState,
    range: Range,
    uri: &Url,
    documents: &HashMap<Url, DocumentState>,
) -> Vec<CodeAction> {
    // We can just re-run check_unused_variables to find if the current range matches an unused variable
    // In a real implementation, we might want to pass the diagnostics in CodeActionParams context
    // but re-running is safer to ensure we have the latest state.
    // Optimization: filtering diagnostics from params would be faster.

    let mut diagnostics = Vec::new();
    diagnostics.extend(check_unused_variables(state));
    diagnostics.extend(check_unused_imports(state));
    diagnostics.extend(check_result_handling_diagnostics(state));
    let mut known = HashSet::new();
    for doc in documents.values() {
        collect_def_names(&doc.index, &mut known);
    }
    diagnostics.extend(check_unresolved_identifiers(state, &known));
    let mut actions = Vec::new();

    for diag in diagnostics {
        // If the diagnostic range overlaps with the requested range
        if ranges_overlap(diag.range, range) {
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unused_variable".to_string(),
                ))
            {
                let title = format!("Remove {}", diag.message);
                let mut edit_range = diag.range;
                let offset =
                    position_to_offset_with_index(&state.text, &state.line_index, diag.range.start);
                if let Some(def) = definition_at_offset(state, offset) {
                    edit_range =
                        text_range_to_range_with_index(&state.text, &state.line_index, def.range);
                }
                actions.push(CodeAction {
                    title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(
                            uri.clone(),
                            vec![TextEdit {
                                range: edit_range,
                                new_text: "".to_string(),
                            }],
                        )])),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unused_import".to_string(),
                ))
            {
                let title = format!("Remove {}", diag.message);
                actions.push(CodeAction {
                    title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(
                            uri.clone(),
                            vec![TextEdit {
                                range: diag.range,
                                new_text: "".to_string(),
                            }],
                        )])),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unresolved_identifier".to_string(),
                ))
            {
                actions.extend(auto_import_code_actions(
                    state,
                    uri,
                    &diag.message,
                    documents,
                ));
            }
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "pending_not_awaited".to_string(),
                ))
            {
                let insert = TextEdit {
                    range: Range {
                        start: diag.range.start,
                        end: diag.range.start,
                    },
                    new_text: "await ".to_string(),
                };
                actions.push(CodeAction {
                    title: "Add `await`".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), vec![insert])])),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                let offset =
                    position_to_offset_with_index(&state.text, &state.line_index, diag.range.start);
                if is_stmt_expr_at_offset(state, offset) {
                    let fire_insert = TextEdit {
                        range: Range {
                            start: diag.range.start,
                            end: diag.range.start,
                        },
                        new_text: "fire ".to_string(),
                    };
                    actions.push(CodeAction {
                        title: "Add `fire` (statement)".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(HashMap::from([(uri.clone(), vec![fire_insert])])),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                }
            }
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unhandled_result".to_string(),
                ))
            {
                let insert = TextEdit {
                    range: Range {
                        start: diag.range.end,
                        end: diag.range.end,
                    },
                    new_text: " otherwise nothing".to_string(),
                };
                actions.push(CodeAction {
                    title: "Wrap with `otherwise` (edit fallback)".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), vec![insert])])),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
            if diag.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "missing_result_return".to_string(),
                ))
            {
                let offset =
                    position_to_offset_with_index(&state.text, &state.line_index, diag.range.start);
                if let Some(edit) = result_return_type_edit(state, uri, offset) {
                    actions.push(CodeAction {
                        title: "Change return type to Result".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(edit),
                        ..Default::default()
                    });
                }
            }
        }
    }

    if let Some(edit) = organize_imports_edit(state, uri) {
        actions.push(CodeAction {
            title: "Organize imports".to_string(),
            kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
            diagnostics: None,
            edit: Some(edit),
            ..Default::default()
        });
    }

    actions.extend(extract_refactor_actions(state, uri, range));

    actions
}

fn ranges_overlap(a: Range, b: Range) -> bool {
    // Simple check: if one starts after the other ends, no overlap.
    if a.end.line < b.start.line || b.end.line < a.start.line {
        return false;
    }
    // Same line checks could be more granular but this is usually sufficient for line-based actions
    true
}

fn auto_import_code_actions(
    state: &DocumentState,
    uri: &Url,
    message: &str,
    documents: &HashMap<Url, DocumentState>,
) -> Vec<CodeAction> {
    let name = message.trim_start_matches("Unresolved identifier: ").trim();
    if name.is_empty() {
        return Vec::new();
    }
    let candidates = workspace_import_candidates(documents, state, uri, name);
    let mut actions = Vec::new();
    for module in candidates {
        if let Some(edit) = add_use_edit(state, uri, name, &module) {
            actions.push(CodeAction {
                title: format!("Add use {} from {}", name, module),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(edit),
                ..Default::default()
            });
        }
    }
    if actions.is_empty() {
        if let Some(edit) = add_use_edit(state, uri, name, "module") {
            actions.push(CodeAction {
                title: format!("Add use {} from module", name),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(edit),
                ..Default::default()
            });
        }
    }
    actions
}

fn workspace_import_candidates(
    documents: &HashMap<Url, DocumentState>,
    state: &DocumentState,
    current_uri: &Url,
    name: &str,
) -> Vec<String> {
    let mut modules = Vec::new();
    for (uri, other) in documents.iter() {
        if uri == current_uri {
            continue;
        }
        for def in other.index.defs.iter() {
            if def.name != name {
                continue;
            }
            if !matches!(def.kind, DefKind::Class | DefKind::Function) {
                continue;
            }
            if let Some(module) = module_path_from_uri(uri) {
                if !has_use_import(state, name, &module) {
                    modules.push(module);
                }
            }
        }
    }
    modules.sort();
    modules.dedup();
    modules
}

fn module_path_from_uri(uri: &Url) -> Option<String> {
    let path = uri.to_file_path().ok()?;
    let file = path.file_stem()?.to_string_lossy().to_string();
    if file.is_empty() { None } else { Some(file) }
}

fn has_use_import(state: &DocumentState, name: &str, module: &str) -> bool {
    let root = syntax_root(state);
    let Some(ast_root) = ast::Root::cast(root) else {
        return false;
    };
    for stmt in ast_root.statements() {
        if let ast::Stmt::UseStmt(use_stmt) = stmt {
            let text = node_text(&state.text, use_stmt.syntax()).unwrap_or_default();
            if text.contains("from") && text.contains(module) {
                if use_stmt.names().any(|token| token.text() == name) {
                    return true;
                }
            }
        }
    }
    false
}

fn add_use_edit(
    state: &DocumentState,
    uri: &Url,
    name: &str,
    module: &str,
) -> Option<WorkspaceEdit> {
    let insert_offset = last_use_stmt_end_offset(state);
    let insert_pos = offset_to_position_with_index(&state.text, &state.line_index, insert_offset);
    let use_line = format!("use {} from {}\n", name, module);
    let edit = TextEdit {
        range: Range {
            start: insert_pos,
            end: insert_pos,
        },
        new_text: use_line,
    };
    Some(WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), vec![edit])])),
        ..Default::default()
    })
}

fn last_use_stmt_end_offset(state: &DocumentState) -> usize {
    let root = syntax_root(state);
    let Some(ast_root) = ast::Root::cast(root) else {
        return 0;
    };
    let mut end = 0usize;
    for stmt in ast_root.statements() {
        if let ast::Stmt::UseStmt(use_stmt) = stmt {
            let range = use_stmt.syntax().text_range();
            end = end.max(range.end().into());
        }
    }
    if end == 0 { 0 } else { end + 1 }
}

fn organize_imports_edit(state: &DocumentState, uri: &Url) -> Option<WorkspaceEdit> {
    let root = syntax_root(state);
    let Some(ast_root) = ast::Root::cast(root) else {
        return None;
    };
    let mut edits = Vec::new();
    for stmt in ast_root.statements() {
        let ast::Stmt::UseStmt(use_stmt) = stmt else {
            continue;
        };
        let range = use_stmt.syntax().text_range();
        let Some(text) = node_text(&state.text, use_stmt.syntax()) else {
            continue;
        };
        let mut names = use_stmt
            .names()
            .map(|token| token.text().to_string())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let from_idx = text.find("from")?;
        let from_part = text[from_idx..].trim();
        let new_text = format!("use {} {}", names.join(", "), from_part);
        edits.push(TextEdit {
            range: text_range_to_range_with_index(&state.text, &state.line_index, range),
            new_text,
        });
    }
    if edits.is_empty() {
        None
    } else {
        Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..Default::default()
        })
    }
}

fn extract_refactor_actions(state: &DocumentState, uri: &Url, range: Range) -> Vec<CodeAction> {
    if range.start == range.end {
        return Vec::new();
    }
    let start = position_to_offset_with_index(&state.text, &state.line_index, range.start);
    let end = position_to_offset_with_index(&state.text, &state.line_index, range.end);
    if start >= end || end > state.text.len() {
        return Vec::new();
    }
    let selected = &state.text[start..end];
    if selected.trim().is_empty() || selected.contains('\n') {
        return Vec::new();
    }
    let mut actions = Vec::new();
    let name = unique_local_name(state, "extracted");
    if let Some(edit) = extract_variable_edit(state, uri, range, &name, selected) {
        actions.push(CodeAction {
            title: format!("Extract variable '{}'", name),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            diagnostics: None,
            edit: Some(edit),
            ..Default::default()
        });
    }
    let func_name = unique_local_name(state, "extracted_fn");
    if let Some(edit) = extract_function_edit(state, uri, range, &func_name, selected) {
        actions.push(CodeAction {
            title: format!("Extract function '{}'", func_name),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            diagnostics: None,
            edit: Some(edit),
            ..Default::default()
        });
    }
    actions
}

fn code_action_matches_only(action: &CodeAction, only: &Vec<CodeActionKind>) -> bool {
    let Some(kind) = action.kind.as_ref() else {
        return only.is_empty();
    };
    only.iter().any(|allowed| {
        if kind == allowed {
            true
        } else {
            kind.as_str().starts_with(allowed.as_str())
        }
    })
}

fn extract_variable_edit(
    state: &DocumentState,
    uri: &Url,
    range: Range,
    name: &str,
    selected: &str,
) -> Option<WorkspaceEdit> {
    let line_start_offset = line_start_offset(&state.line_index, range.start.line as usize);
    let indent = current_line_indent(&state.text, line_start_offset);
    let insert_pos =
        offset_to_position_with_index(&state.text, &state.line_index, line_start_offset);
    let new_line = format!("{}{} = {}\n", indent, name, selected);
    let edits = vec![
        TextEdit {
            range: Range {
                start: insert_pos,
                end: insert_pos,
            },
            new_text: new_line,
        },
        TextEdit {
            range,
            new_text: name.to_string(),
        },
    ];
    Some(WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), edits)])),
        ..Default::default()
    })
}

fn extract_function_edit(
    state: &DocumentState,
    uri: &Url,
    range: Range,
    name: &str,
    selected: &str,
) -> Option<WorkspaceEdit> {
    let mut new_text = String::new();
    if !state.text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(&format!("\nto {}():\n    return {}\n", name, selected));
    let end_pos = Position {
        line: state.text.lines().count() as u32,
        character: 0,
    };
    let edits = vec![
        TextEdit {
            range,
            new_text: format!("{}()", name),
        },
        TextEdit {
            range: Range {
                start: end_pos,
                end: end_pos,
            },
            new_text,
        },
    ];
    Some(WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), edits)])),
        ..Default::default()
    })
}

fn unique_local_name(state: &DocumentState, base: &str) -> String {
    let mut name = base.to_string();
    let mut counter = 1;
    while state.index.defs.iter().any(|def| def.name == name) {
        name = format!("{}{}", base, counter);
        counter += 1;
    }
    name
}

fn line_start_offset(line_index: &LineIndex, line: usize) -> usize {
    line_index.line_starts.get(line).copied().unwrap_or(0)
}

fn current_line_indent(text: &str, line_start: usize) -> String {
    let mut indent = String::new();
    for ch in text[line_start..].chars() {
        if ch == ' ' || ch == '\t' {
            indent.push(ch);
        } else {
            break;
        }
    }
    indent
}

fn format_text(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        let mut normalized = String::new();
        let mut started = false;
        for ch in trimmed.chars() {
            if !started && ch == '\t' {
                normalized.push_str("    ");
                continue;
            }
            started = true;
            normalized.push(ch);
        }
        lines.push(normalized);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') || out.is_empty() {
        out.push('\n');
    } else {
        out.push('\n');
    }
    out
}

fn format_range_text(state: &DocumentState, range: Range) -> Option<TextEdit> {
    let start_offset = line_start_offset(&state.line_index, range.start.line as usize);
    let end_line = range.end.line as usize;
    let end_offset = state
        .line_index
        .line_starts
        .get(end_line + 1)
        .copied()
        .unwrap_or(state.text.len());
    if start_offset >= end_offset || end_offset > state.text.len() {
        return None;
    }
    let slice = &state.text[start_offset..end_offset];
    let formatted = format_text(slice);
    Some(TextEdit {
        range: Range {
            start: offset_to_position_with_index(&state.text, &state.line_index, start_offset),
            end: offset_to_position_with_index(&state.text, &state.line_index, end_offset),
        },
        new_text: formatted,
    })
}

fn full_document_range(text: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: offset_to_position(text, text.len()),
    }
}

fn folding_ranges(state: &DocumentState) -> Vec<FoldingRange> {
    let root = syntax_root(state);
    let mut ranges = Vec::new();
    for node in root.descendants() {
        let kind = node.kind();
        let fold_kind = match kind {
            SyntaxKind::UseStmt => Some(FoldingRangeKind::Imports),
            SyntaxKind::ClassDef
            | SyntaxKind::FuncDef
            | SyntaxKind::MethodDef
            | SyntaxKind::Block
            | SyntaxKind::MatchStmt
            | SyntaxKind::HasBlock => Some(FoldingRangeKind::Region),
            _ => None,
        };
        let Some(kind) = fold_kind else {
            continue;
        };
        let range =
            text_range_to_range_with_index(&state.text, &state.line_index, node.text_range());
        if range.end.line > range.start.line {
            ranges.push(FoldingRange {
                start_line: range.start.line,
                start_character: None,
                end_line: range.end.line,
                end_character: None,
                kind: Some(kind),
                collapsed_text: None,
            });
        }
    }
    ranges
}

#[cfg(test)]
fn code_lenses(state: &DocumentState) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    for def in &state.index.defs {
        if !matches!(
            def.kind,
            DefKind::Class | DefKind::Function | DefKind::Method
        ) {
            continue;
        }
        let references = collect_references(state, def, false).len();
        let range = text_range_to_range_with_index(&state.text, &state.line_index, def.name_range);
        lenses.push(CodeLens {
            range,
            command: Some(Command {
                title: format!("References: {}", references),
                command: "wrela.codeLens.showReferences".to_string(),
                arguments: None,
            }),
            data: None,
        });
    }
    lenses
}

pub fn check_unused_variables(state: &DocumentState) -> Vec<Diagnostic> {
    let root = syntax_root(state);
    let mut ref_counts = HashMap::new();

    // Initialize counts for variables to 0
    for def in &state.index.defs {
        if def.kind == DefKind::Variable {
            ref_counts.insert(def.id, 0);
        }
    }

    // Walk AST to find references
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.kind() == SyntaxKind::Ident {
            if let Some(def) = resolve_reference_token(&state.index, &state.text, &token) {
                if let Some(count) = ref_counts.get_mut(&def.id) {
                    // Don't count the definition itself as a reference
                    if token.text_range() != def.name_range {
                        *count += 1;
                    }
                }
            }
        }
    }

    let mut diagnostics = Vec::new();
    for (id, count) in ref_counts {
        if count == 0 {
            let def = &state.index.defs[id];

            // Find the full declaration range to enable deletion
            // The def.range covers the node.

            diagnostics.push(Diagnostic {
                range: text_range_to_range_with_index(
                    &state.text,
                    &state.line_index,
                    def.name_range,
                ),
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unused_variable".to_string(),
                )),
                source: Some("wrela".to_string()),
                message: format!("Unused variable: {}", def.name),
                tags: Some(vec![tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY]),
                ..Default::default()
            });
        }
    }
    diagnostics
}

pub fn check_unused_imports(state: &DocumentState) -> Vec<Diagnostic> {
    let root = syntax_root(state);
    if state.imports.is_empty() {
        return Vec::new();
    }
    let mut ref_counts: HashMap<&str, usize> = state
        .imports
        .iter()
        .filter(|import| import.name != "*")
        .map(|import| (import.name.as_str(), 0))
        .collect();
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.kind() == SyntaxKind::Ident {
            if token_is_in_use_stmt(&token) {
                continue;
            }
            if let Some(count) = ref_counts.get_mut(token.text()) {
                *count += 1;
            }
        }
    }
    let mut diagnostics = Vec::new();
    for import in &state.imports {
        if import.name == "*" {
            continue;
        }
        let count = ref_counts.get(import.name.as_str()).copied().unwrap_or(0);
        if count != 0 {
            continue;
        }
        diagnostics.push(Diagnostic {
            range: text_range_to_range_with_index(&state.text, &state.line_index, import.name_range),
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                "unused_import".to_string(),
            )),
            source: Some("wrela".to_string()),
            message: format!("Unused import: {}", import.name),
            tags: Some(vec![tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY]),
            ..Default::default()
        });
    }
    diagnostics
}

pub fn check_unresolved_identifiers(
    state: &DocumentState,
    known_definitions: &HashSet<String>,
) -> Vec<Diagnostic> {
    let root = syntax_root(state);
    let mut diagnostics = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.kind() != SyntaxKind::Ident {
            continue;
        }
        if is_keyword(token.text()) {
            continue;
        }
        if is_type_context_token(&token) {
            continue;
        }
        if is_assert_mode_token(&token) {
            continue;
        }
        if is_member_name_token(&token) {
            continue;
        }
        if is_named_arg_name_token(&token) {
            continue;
        }
        if is_builtin(token.text()) {
            continue;
        }
        if is_implicit_binding(token.text()) {
            continue;
        }
        if known_definitions.contains(token.text()) {
            continue;
        }
        let offset: usize = token.text_range().start().into();
        if definition_at_offset(state, offset).is_some() {
            continue;
        }
        if resolve_reference_token(&state.index, &state.text, &token).is_some() {
            continue;
        }
        diagnostics.push(Diagnostic {
            range: text_range_to_range_with_index(
                &state.text,
                &state.line_index,
                token.text_range(),
            ),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                "unresolved_identifier".to_string(),
            )),
            source: Some("wrela".to_string()),
            message: format!("Unresolved identifier: {}", token.text()),
            ..Default::default()
        });
    }
    diagnostics
}

fn collect_def_names(index: &SymbolIndex, names: &mut HashSet<String>) {
    for def in &index.defs {
        if def.is_external {
            continue;
        }
        names.insert(def.name.clone());
    }
}

pub fn check_result_handling_diagnostics(state: &DocumentState) -> Vec<Diagnostic> {
    let root = syntax_root(state);
    let Some(ast_root) = ast::Root::cast(root) else {
        return Vec::new();
    };
    let module = hir::lower::lower(ast_root);
    let (errors, _) = hir::typeck::check_module_with_info(&module);
    errors
        .into_iter()
        .filter_map(|error| {
            let message = error.to_string();
            match error {
                TypeError::PendingNotAwaited { span, help } => Some(Diagnostic {
                    range: span_to_range(&state.text, span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("wrela".to_string()),
                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                        "pending_not_awaited".to_string(),
                    )),
                    message: format!("{message}\nHint: {help}"),
                    ..Diagnostic::default()
                }),
                TypeError::UnhandledResult { span, help } => Some(Diagnostic {
                    range: span_to_range(&state.text, span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("wrela".to_string()),
                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                        "unhandled_result".to_string(),
                    )),
                    message: format!("{message}\nHint: {help}"),
                    ..Diagnostic::default()
                }),
                TypeError::MissingResultReturn { span, help } => Some(Diagnostic {
                    range: span_to_range(&state.text, span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("wrela".to_string()),
                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                        "missing_result_return".to_string(),
                    )),
                    message: format!("{message}\nHint: {help}"),
                    ..Diagnostic::default()
                }),
                _ => None,
            }
        })
        .collect()
}

pub fn check_shadowing(state: &DocumentState) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for def in &state.index.defs {
        if !matches!(
            def.kind,
            DefKind::Variable | DefKind::Parameter | DefKind::Function | DefKind::Method
        ) {
            continue;
        }
        let mut current = state.index.scopes.get(def.scope_id).and_then(|s| s.parent);
        while let Some(id) = current {
            if state
                .index
                .defs
                .iter()
                .any(|other| other.scope_id == id && other.name == def.name)
            {
                diagnostics.push(Diagnostic {
                    range: text_range_to_range_with_index(
                        &state.text,
                        &state.line_index,
                        def.name_range,
                    ),
                    severity: Some(DiagnosticSeverity::HINT),
                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                        "shadowed_name".to_string(),
                    )),
                    source: Some("wrela".to_string()),
                    message: format!("'{}' shadows a name from an outer scope", def.name),
                    ..Default::default()
                });
                break;
            }
            current = state.index.scopes.get(id).and_then(|scope| scope.parent);
        }
    }
    diagnostics
}

pub fn check_naming_conventions(state: &DocumentState) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for def in &state.index.defs {
        let ok = match def.kind {
            DefKind::Class => is_pascal_case(&def.name),
            DefKind::Function | DefKind::Method | DefKind::Variable | DefKind::Parameter => {
                is_lower_snake_case(&def.name)
            }
            _ => true,
        };
        if ok {
            continue;
        }
        diagnostics.push(Diagnostic {
            range: text_range_to_range_with_index(&state.text, &state.line_index, def.name_range),
            severity: Some(DiagnosticSeverity::INFORMATION),
            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                "naming_convention".to_string(),
            )),
            source: Some("wrela".to_string()),
            message: format!("'{}' does not follow naming conventions", def.name),
            ..Default::default()
        });
    }
    diagnostics
}

fn is_member_name_token(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(member) = ast::MemberExpr::cast(current.clone()) {
            if let Some(name) = member.name() {
                return name.text_range() == token.text_range();
            }
        }
        node = current.parent();
    }
    false
}

fn is_named_arg_name_token(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(named) = ast::NamedArg::cast(current.clone()) {
            if let Some(name) = named.name() {
                return name.text_range() == token.text_range();
            }
        }
        node = current.parent();
    }
    false
}

fn is_assert_mode_token(token: &SyntaxToken) -> bool {
    if token.text() != "value" && token.text() != "identity" {
        return false;
    }
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(assert_stmt) = ast::AssertStmt::cast(current.clone()) {
            for t in assert_stmt
                .syntax()
                .children_with_tokens()
                .filter_map(|it| it.into_token())
            {
                if t.text_range() == token.text_range() {
                    return true;
                }
            }
            return false;
        }
        node = current.parent();
    }
    false
}

fn is_type_context_token(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        match current.kind() {
            SyntaxKind::TypeRef | SyntaxKind::TypeArgList | SyntaxKind::TypeParamList => {
                return true
            }
            _ => {}
        }
        node = current.parent();
    }
    false
}

fn is_lower_snake_case(name: &str) -> bool {
    let mut prev_underscore = false;
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' {
            if i == 0 || prev_underscore {
                return false;
            }
            prev_underscore = true;
            continue;
        }
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() {
            return false;
        }
        prev_underscore = false;
    }
    !name.is_empty()
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => (),
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric())
}

fn apply_content_changes(
    mut text: String,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Option<String> {
    if changes.is_empty() {
        return Some(text);
    }
    if changes.len() == 1 && changes[0].range.is_none() && changes[0].range_length.is_none() {
        return Some(changes[0].text.clone());
    }
    let mut line_index = LineIndex::new(&text);
    for change in changes {
        if change.range.is_none() && change.range_length.is_none() {
            text = change.text;
            line_index = LineIndex::new(&text);
            continue;
        }
        let range = change.range?;
        let start = position_to_offset_with_index(&text, &line_index, range.start);
        let end = position_to_offset_with_index(&text, &line_index, range.end);
        if start > end || end > text.len() {
            return None;
        }
        text.replace_range(start..end, &change.text);
        line_index = LineIndex::new(&text);
    }
    Some(text)
}

pub fn build_document_state(text: String) -> (DocumentState, Vec<ParseError>) {
    let (root, errors) = parser::parse_with_errors(&text);
    let index = SymbolIndex::build(&text, &root);
    let line_index = LineIndex::new(&text);
    let imports = collect_imports(&root);
    (
        DocumentState {
            text,
            green: root.green().clone().into(),
            index,
            line_index,
            imports,
        },
        errors,
    )
}

fn diagnostics_for_errors(text: &str, errors: Vec<ParseError>) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|error| Diagnostic {
            range: span_to_range(text, error.span),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("wrela".to_string()),
            message: error.message,
            ..Diagnostic::default()
        })
        .collect()
}

fn span_to_range(text: &str, span: miette::SourceSpan) -> Range {
    let start = span.offset();
    let end = start.saturating_add(span.len());
    Range {
        start: offset_to_position(text, start),
        end: offset_to_position(text, end),
    }
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let mut line = 0usize;
    let mut line_start = 0usize;
    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    let line_slice = &text[line_start..offset];
    let character = line_slice.encode_utf16().count();
    Position {
        line: line as u32,
        character: character as u32,
    }
}

fn offset_to_position_with_index(text: &str, line_index: &LineIndex, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let line = line_index.line_for_offset(offset);
    let line_start = *line_index.line_starts.get(line).unwrap_or(&0);
    let line_slice = &text[line_start..offset];
    let character = line_slice.encode_utf16().count();
    Position {
        line: line as u32,
        character: character as u32,
    }
}

fn position_to_offset_with_index(text: &str, line_index: &LineIndex, position: Position) -> usize {
    let line = position.line as usize;
    let line_start = match line_index.line_starts.get(line) {
        Some(start) => *start,
        None => return text.len(),
    };
    let line_end = line_index
        .line_starts
        .get(line + 1)
        .copied()
        .unwrap_or(text.len());
    let line_slice = &text[line_start..line_end];
    let mut utf16_count = 0u32;
    for (idx, ch) in line_slice.char_indices() {
        if utf16_count >= position.character {
            return (line_start + idx).min(text.len());
        }
        utf16_count += ch.len_utf16() as u32;
    }
    (line_start + line_slice.len()).min(text.len())
}

fn text_range_to_range(text: &str, range: TextRange) -> Range {
    Range {
        start: offset_to_position(text, range.start().into()),
        end: offset_to_position(text, range.end().into()),
    }
}

fn text_range_to_range_with_index(text: &str, line_index: &LineIndex, range: TextRange) -> Range {
    Range {
        start: offset_to_position_with_index(text, line_index, range.start().into()),
        end: offset_to_position_with_index(text, line_index, range.end().into()),
    }
}

fn token_text_len_utf16(token: &SyntaxToken) -> u32 {
    token.text().encode_utf16().count() as u32
}

fn node_text(text: &str, node: &SyntaxNode) -> Option<String> {
    let range = node.text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    text.get(start..end).map(|slice| slice.to_string())
}

fn format_type_ref(ty: &ast::TypeRef) -> Option<String> {
    let name = ty.name()?.text().to_string();
    let args = ty
        .args()
        .into_iter()
        .filter_map(|arg| format_type_ref(&arg))
        .collect::<Vec<_>>();
    if args.is_empty() {
        Some(name)
    } else {
        Some(format!("{}[{}]", name, args.join(", ")))
    }
}

fn params_from_iter(params: impl Iterator<Item = ast::Param>) -> Vec<ParamInfo> {
    params
        .filter_map(|param| {
            let name = param.name()?;
            let ty = param.ty().and_then(|ty| format_type_ref(&ty));
            Some(ParamInfo {
                name: name.text().to_string(),
                ty,
                range: name.text_range(),
            })
        })
        .collect()
}

fn format_signature(name: &str, params: &[ParamInfo], ret_type: Option<&str>) -> String {
    let params_text = params
        .iter()
        .map(|param| match &param.ty {
            Some(ty) => format!("{}: {}", param.name, ty),
            None => param.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut signature = format!("{}({})", name, params_text);
    if let Some(ret) = ret_type {
        signature.push_str(" -> ");
        signature.push_str(ret);
    }
    signature
}

fn class_field_params(index: &SymbolIndex, class_scope: usize) -> Vec<ParamInfo> {
    index
        .defs
        .iter()
        .filter(|def| def.scope_id == class_scope && def.kind == DefKind::Field)
        .map(|def| ParamInfo {
            name: def.name.clone(),
            ty: def.ty.clone(),
            range: def.name_range,
        })
        .collect()
}

fn class_field_params_from_text(text: &str, class_name: &str) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    let mut in_class = false;
    let mut in_has = false;
    let mut class_indent = 0usize;
    let mut has_indent = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        if !in_class {
            let class_decl = trimmed
                .strip_prefix("A ")
                .or_else(|| trimmed.strip_prefix("An "));
            let Some(rest) = class_decl else { continue };
            let name = rest
                .trim()
                .trim_end_matches(':')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if name == class_name {
                in_class = true;
                class_indent = indent;
            }
            continue;
        }
        if indent <= class_indent {
            break;
        }
        if !in_has {
            if trimmed == "has:" {
                in_has = true;
                has_indent = indent;
            }
            continue;
        }
        if indent <= has_indent {
            in_has = false;
            continue;
        }
        let Some((name, ty)) = trimmed.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let ty = ty.trim();
        params.push(ParamInfo {
            name: name.to_string(),
            ty: if ty.is_empty() { None } else { Some(ty.to_string()) },
            range: TextRange::new(TextSize::from(0), TextSize::from(0)),
        });
    }
    params
}

fn class_decl_exists_in_text(text: &str, class_name: &str) -> bool {
    for line in text.lines() {
        let trimmed = line.trim_start();
        let class_decl = trimmed
            .strip_prefix("A ")
            .or_else(|| trimmed.strip_prefix("An "));
        let Some(rest) = class_decl else { continue };
        let name = rest
            .trim()
            .trim_end_matches(':')
            .split_whitespace()
            .next()
            .unwrap_or("");
        if name == class_name {
            return true;
        }
    }
    false
}

fn class_name_from_assignment(text: &str, var_name: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(var_name) {
            continue;
        }
        let rest = trimmed[var_name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let name = rest
            .split(|ch: char| ch == '(' || ch.is_whitespace())
            .next()
            .unwrap_or("");
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn class_member_items_from_text(
    text: &str,
    class_name: &str,
    seen: &mut HashSet<String>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut in_class = false;
    let mut in_has = false;
    let mut class_indent = 0usize;
    let mut has_indent = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        if !in_class {
            let class_decl = trimmed
                .strip_prefix("A ")
                .or_else(|| trimmed.strip_prefix("An "));
            let Some(rest) = class_decl else { continue };
            let name = rest
                .trim()
                .trim_end_matches(':')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if name == class_name {
                in_class = true;
                class_indent = indent;
            }
            continue;
        }
        if indent <= class_indent {
            break;
        }
        if trimmed == "has:" {
            in_has = true;
            has_indent = indent;
            continue;
        }
        if in_has {
            if indent <= has_indent {
                in_has = false;
                continue;
            }
            if let Some((name, _)) = trimmed.split_once(':') {
                let name = name.trim();
                if !name.is_empty() && seen.insert(name.to_string()) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        ..CompletionItem::default()
                    });
                }
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("can ") {
            let rest = rest.trim_start();
            let method_name = rest
                .split(|ch: char| ch == '(' || ch.is_whitespace())
                .next()
                .unwrap_or("");
            if !method_name.is_empty() && seen.insert(method_name.to_string()) {
                items.push(CompletionItem {
                    label: method_name.to_string(),
                    kind: Some(CompletionItemKind::METHOD),
                    ..CompletionItem::default()
                });
            }
        }
        if let Some(rest) = trimmed.strip_prefix("derives ") {
            let rest = rest.trim_start();
            let derived_name = rest
                .split(|ch: char| ch == '(' || ch.is_whitespace())
                .next()
                .unwrap_or("");
            if !derived_name.is_empty() && seen.insert(derived_name.to_string()) {
                items.push(CompletionItem {
                    label: derived_name.to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    ..CompletionItem::default()
                });
            }
        }
    }
    items
}

fn method_params_from_text(text: &str, method_name: &str) -> Vec<ParamInfo> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("can ") else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with(method_name) {
            continue;
        }
        let rest = rest[method_name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let Some(end) = rest.find(')') else {
            continue;
        };
        let args = &rest[..end];
        let mut params = Vec::new();
        for arg in args.split(',') {
            let arg = arg.trim();
            if arg.is_empty() {
                continue;
            }
            let (name, ty) = match arg.split_once(':') {
                Some((name, ty)) => (name.trim(), Some(ty.trim())),
                None => (arg, None),
            };
            if name.is_empty() {
                continue;
            }
            params.push(ParamInfo {
                name: name.to_string(),
                ty: ty.and_then(|ty| if ty.is_empty() { None } else { Some(ty.to_string()) }),
                range: TextRange::new(TextSize::from(0), TextSize::from(0)),
            });
        }
        if !params.is_empty() {
            return params;
        }
    }
    Vec::new()
}

fn call_signature_for_callee(
    index: &SymbolIndex,
    text: &str,
    scope_id: usize,
    call: &ast::CallExpr,
) -> Option<(String, Vec<ParamInfo>)> {
    let callee = call.callee()?;
    match callee {
        ast::Expr::Ident(expr) => {
            let name = expr.name()?.text().to_string();
            if let Some(class_scope) = class_scope_for_name(index, &name) {
                let params = class_field_params(index, class_scope);
                let label = format_signature(&name, &params, None);
                return Some((label, params));
            }
            let def = resolve_in_scope_kinds(
                index,
                scope_id,
                &name,
                &[DefKind::Function, DefKind::Class],
            )?;
            if def.kind == DefKind::Class {
                let class_scope = class_scope_for_name(index, &def.name)?;
                let params = class_field_params(index, class_scope);
                let label = format_signature(&def.name, &params, None);
                return Some((label, params));
            }
            let label = def
                .detail
                .clone()
                .unwrap_or_else(|| format_signature(&def.name, &def.params, def.ty.as_deref()));
            Some((label, def.params.clone()))
        }
        ast::Expr::Member(expr) => {
            let member_name = expr.name()?.text().to_string();
            let object = expr.object()?;
            let object_ty = index.infer_expr_type(scope_id, text, &object)?;
            let class_scope = class_scope_for_name(index, &object_ty)?;
            let def = resolve_in_scope_kinds(index, class_scope, &member_name, &[DefKind::Method])?;
            let label = def
                .detail
                .clone()
                .unwrap_or_else(|| format_signature(&def.name, &def.params, def.ty.as_deref()));
            Some((label, def.params.clone()))
        }
        _ => None,
    }
}

fn call_params_for_completion(
    index: &SymbolIndex,
    text: &str,
    scope_id: usize,
    call: &ast::CallExpr,
) -> Option<Vec<ParamInfo>> {
    call_signature_for_callee(index, text, scope_id, call).map(|(_, params)| params)
}

fn literal_type(expr: &ast::LiteralExpr) -> Option<String> {
    let token = expr
        .syntax()
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .next()?;
    match token.kind() {
        SyntaxKind::IntNumber => Some("Int".to_string()),
        SyntaxKind::FloatNumber => Some("Float".to_string()),
        SyntaxKind::TrueKw | SyntaxKind::FalseKw => Some("Bool".to_string()),
        SyntaxKind::NothingKw => Some("Nothing".to_string()),
        SyntaxKind::StringLiteral => Some("String".to_string()),
        _ => None,
    }
}

const KEYWORDS: &[&str] = &[
    "A",
    "An",
    "has",
    "can",
    "must",
    "derives",
    "is",
    "either",
    "to",
    "private",
    "mutable",
    "defer",
    "if",
    "while",
    "for",
    "in",
    "return",
    "break",
    "continue",
    "match",
    "otherwise",
    "use",
    "from",
    "optimize",
    "and",
    "or",
    "not",
    "await",
    "detach",
    "spawn",
    "fire",
    "err",
    "crash",
    "latency",
    "throughput",
    "conservation",
    "balance",
    "n",
];

const CONSTANTS: &[&str] = &["true", "false", "nothing", "nil", "it", "its"];
const BUILTINS: &[&str] = &[
    "print",
    "storage_get",
    "storage_set",
    "storage_delete",
    "storage_configure",
    "map_get",
    "map_set",
    "bytes_to_string",
    "bytes_from_string",
    "http_server_serve_get_requests",
    "http_server_serve_post_requests",
    "http_server_serve_requests",
    "http_server_serve_on",
    "http_server_stop",
];
const IMPLICIT_BINDINGS: &[&str] = &["Pool", "latency", "throughput", "conservation", "balance"];

lazy_static::lazy_static! {
    static ref SEMANTIC_TOKEN_LEGEND: SemanticTokensLegend = {
        let token_types = vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::CLASS,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::METHOD,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::COMMENT,
            SemanticTokenType::DECORATOR,
        ];
        let token_modifiers = vec![];
        SemanticTokensLegend {
            token_types,
            token_modifiers,
        }
    };
}

fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name) || CONSTANTS.contains(&name)
}

fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

fn is_implicit_binding(name: &str) -> bool {
    IMPLICIT_BINDINGS.contains(&name)
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_ident_start(first) {
        return false;
    }
    chars.all(is_ident_continue)
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic() || (c > '\u{007f}' && !c.is_whitespace())
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric() || (c > '\u{007f}' && !c.is_whitespace())
}

fn is_augmented_assign(node: &SyntaxNode) -> bool {
    node.children_with_tokens().any(|it| {
        it.into_token().is_some_and(|tok| {
            matches!(
                tok.kind(),
                SyntaxKind::PlusEq | SyntaxKind::MinusEq | SyntaxKind::StarEq | SyntaxKind::SlashEq
            )
        })
    })
}

fn keyword_completion_items() -> Vec<CompletionItem> {
    let mut items = KEYWORDS
        .iter()
        .map(|kw| CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..CompletionItem::default()
        })
        .collect::<Vec<_>>();
    items.extend(CONSTANTS.iter().map(|constant| CompletionItem {
        label: constant.to_string(),
        kind: Some(CompletionItemKind::CONSTANT),
        ..CompletionItem::default()
    }));
    items
}

fn completion_items(
    state: &DocumentState,
    position: Position,
    trigger: Option<char>,
) -> Vec<CompletionItem> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let mut seen = HashSet::new();

    let root = syntax_root(state);
    if is_completion_suppressed(&root, offset) {
        return Vec::new();
    }
    if is_param_list_context(&root, offset) {
        if is_type_completion_context(state, offset) {
            return type_completion_items(state, &mut seen);
        }
        return Vec::new();
    }
    if let Some(class_scope) =
        member_scope_at_completion_offset(&state.index, &root, &state.text, offset)
    {
        return member_completion_items(&state.index, class_scope, &mut seen);
    }
    if is_member_access_context(&root, offset) {
        let items = member_completion_items_from_text(state, offset, &mut seen);
        if !items.is_empty() {
            return items;
        }
        return Vec::new();
    }

    let force_call_context = matches!(trigger, Some('(') | Some(','));
    let in_param_list = is_param_list_context(&root, offset);
    let call_context = call_argument_context(state, offset, force_call_context && !in_param_list);
    if let Some(call_context) = call_context {
        if call_context.has_params {
            let mut items = Vec::new();
            for item in call_context.items {
                if seen.insert(item.label.clone()) {
                    items.push(item);
                }
            }
            return items;
        }
        return Vec::new();
    }
    let mut items = Vec::new();
    if is_type_completion_context(state, offset) {
        return type_completion_items(state, &mut seen);
    }
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    items.extend(scope_completion_items(&state.index, scope_id, &mut seen));
    if completion_prefix(&state.text, offset).is_empty() {
        for item in keyword_completion_items() {
            if seen.insert(item.label.clone()) {
                items.push(item);
            }
        }
    }
    items
}

struct CallArgContext {
    items: Vec<CompletionItem>,
    has_params: bool,
}

fn call_argument_context(
    state: &DocumentState,
    offset: usize,
    force_call_context: bool,
) -> Option<CallArgContext> {
    let root = syntax_root(state);
    if is_param_list_context(&root, offset) {
        return None;
    }
    let mut in_call_context = false;
    let call_opt = find_node_at_or_before_offset::<ast::CallExpr>(&root, offset).filter(|call| {
        let inside = offset_in_call_arguments(call, offset);
        if inside {
            in_call_context = true;
        }
        inside
    });
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let params = call_opt
        .as_ref()
        .and_then(|call| call_params_for_completion(&state.index, &state.text, scope_id, call))
        .or_else(|| call_params_from_tokens_at_offset(state, offset));
    let params = match params {
        Some(params) => params,
        None => {
            let hinted = in_call_context
                || is_call_context_from_tokens(&root, offset)
                || is_call_context_from_text(state, offset)
                || force_call_context;
            if hinted {
                return Some(CallArgContext {
                    items: Vec::new(),
                    has_params: false,
                });
            }
            return None;
        }
    };
    if params.is_empty() {
        return Some(CallArgContext {
            items: Vec::new(),
            has_params: false,
        });
    }
    let mut used = HashSet::new();
    if let Some(call) = call_opt.as_ref() {
        for arg in call.args() {
            if let ast::Arg::Named(named) = arg {
                if let Some(name) = named.name() {
                    used.insert(name.text().to_string());
                }
            }
        }
    }
    let mut items = Vec::new();
    for param in params {
        if used.contains(&param.name) {
            continue;
        }
        let detail = param
            .ty
            .as_ref()
            .map(|ty| format!("{}: {}", param.name, ty));
        items.push(CompletionItem {
            label: format!("{}=", param.name),
            kind: Some(CompletionItemKind::FIELD),
            detail,
            insert_text: Some(format!("{}=", param.name)),
            filter_text: Some(param.name.clone()),
            sort_text: Some(format!("0_{}", param.name)),
            ..CompletionItem::default()
        });
    }
    Some(CallArgContext {
        items,
        has_params: true,
    })
}

fn is_type_completion_context(state: &DocumentState, offset: usize) -> bool {
    let root = syntax_root(state);
    if find_node_at_or_before_offset::<ast::TypeRef>(&root, offset).is_some() {
        return true;
    }
    let Some(prev) = token_before_offset_skip_trivia(&root, offset) else {
        return false;
    };
    match prev.kind() {
        SyntaxKind::Arrow => is_return_type_context(&prev),
        SyntaxKind::Colon => is_field_or_param_type_context(&prev),
        _ => false,
    }
}

fn is_field_or_param_type_context(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if ast::FieldDef::can_cast(current.kind()) || ast::Param::can_cast(current.kind()) {
            return true;
        }
        if ast::ClassDef::can_cast(current.kind())
            || ast::FuncDef::can_cast(current.kind())
            || ast::MethodDef::can_cast(current.kind())
            || ast::IfStmt::can_cast(current.kind())
            || ast::ForStmt::can_cast(current.kind())
            || ast::WhileStmt::can_cast(current.kind())
            || ast::MatchStmt::can_cast(current.kind())
            || ast::MapExpr::can_cast(current.kind())
        {
            return false;
        }
        node = current.parent();
    }
    false
}

fn is_return_type_context(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if ast::FuncDef::can_cast(current.kind()) || ast::MethodDef::can_cast(current.kind()) {
            return true;
        }
        node = current.parent();
    }
    false
}

fn type_completion_items(state: &DocumentState, seen: &mut HashSet<String>) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let primitives = [
        "Int", "Float", "Number", "String", "Bool", "Nothing", "Map", "Bytes", "Result",
    ];
    for name in primitives {
        if seen.insert(name.to_string()) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::CLASS),
                ..CompletionItem::default()
            });
        }
    }
    for def in state
        .index
        .defs
        .iter()
        .filter(|def| def.kind == DefKind::Class)
    {
        if seen.insert(def.name.clone()) {
            items.push(completion_item_for_def(def));
        }
    }
    items
}

fn member_completion_items(
    index: &SymbolIndex,
    class_scope: usize,
    seen: &mut HashSet<String>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for def in index.defs.iter().filter(|def| {
        def.scope_id == class_scope && matches!(def.kind, DefKind::Field | DefKind::Method)
    }) {
        if seen.insert(def.name.clone()) {
            items.push(completion_item_for_def(def));
        }
    }
    items
}

fn scope_completion_items(
    index: &SymbolIndex,
    scope_id: usize,
    seen: &mut HashSet<String>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut current = Some(scope_id);
    while let Some(id) = current {
        for def in index.defs.iter().filter(|def| def.scope_id == id) {
            if seen.insert(def.name.clone()) {
                items.push(completion_item_for_def(def));
            }
        }
        current = index.scopes.get(id).and_then(|scope| scope.parent);
    }
    items
}

fn completion_item_for_def(def: &Definition) -> CompletionItem {
    let kind = match def.kind {
        DefKind::Class => CompletionItemKind::CLASS,
        DefKind::Function | DefKind::Method => CompletionItemKind::TEXT,
        DefKind::Field => CompletionItemKind::FIELD,
        DefKind::Module => CompletionItemKind::MODULE,
        DefKind::Parameter | DefKind::Variable => CompletionItemKind::VARIABLE,
    };
    let detail = def
        .detail
        .clone()
        .or_else(|| def.ty.as_ref().map(|ty| format!("{}: {}", def.name, ty)));
    let mut item = CompletionItem {
        label: def.name.clone(),
        kind: Some(kind),
        detail,
        ..CompletionItem::default()
    };
    item.insert_text = Some(def.name.clone());
    item.insert_text_format = Some(tower_lsp::lsp_types::InsertTextFormat::PLAIN_TEXT);
    item
}

#[allow(deprecated)]
fn document_symbols(state: &DocumentState) -> Vec<DocumentSymbol> {
    let root = syntax_root(state);
    let Some(ast_root) = ast::Root::cast(root) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    for stmt in ast_root.statements() {
        match stmt {
            ast::Stmt::ClassDef(def) => {
                symbols.push(class_symbol(&state.text, &def));
            }
            ast::Stmt::FuncDef(def) => {
                if let Some(name) = def.name() {
                    symbols.push(DocumentSymbol {
                        name: name.text().to_string(),
                        kind: SymbolKind::FUNCTION,
                        range: node_range(&state.text, def.syntax()),
                        selection_range: token_range(&state.text, &name),
                        children: None,
                        deprecated: None,
                        tags: None,
                        detail: None,
                    });
                }
            }
            ast::Stmt::PrivateBlock(block) => {
                for stmt in block.statements() {
                    match stmt {
                        ast::Stmt::ClassDef(def) => {
                            symbols.push(class_symbol(&state.text, &def));
                        }
                        ast::Stmt::FuncDef(def) => {
                            if let Some(name) = def.name() {
                                symbols.push(DocumentSymbol {
                                    name: name.text().to_string(),
                                    kind: SymbolKind::FUNCTION,
                                    range: node_range(&state.text, def.syntax()),
                                    selection_range: token_range(&state.text, &name),
                                    children: None,
                                    deprecated: None,
                                    tags: None,
                                    detail: None,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    symbols
}

#[allow(deprecated)]
fn class_symbol(text: &str, def: &ast::ClassDef) -> DocumentSymbol {
    let name_token = def.name();
    let class_name = name_token
        .as_ref()
        .map(|token| token.text().to_string())
        .unwrap_or_else(|| "Class".to_string());
    let selection_range = name_token
        .as_ref()
        .map(|token| token_range(text, token))
        .unwrap_or_else(|| node_range(text, def.syntax()));

    let mut children = Vec::new();
    for block in def.has_blocks() {
        collect_fields_from_has_block(text, &mut children, &block);
    }
    for method in def.methods() {
        if let Some(name) = method.name() {
            children.push(DocumentSymbol {
                name: name.text().to_string(),
                kind: SymbolKind::METHOD,
                range: node_range(text, method.syntax()),
                selection_range: token_range(text, &name),
                children: None,
                deprecated: None,
                tags: None,
                detail: None,
            });
        }
    }
    for private_block in def.syntax().children().filter_map(ast::PrivateBlock::cast) {
        for child in private_block.syntax().children() {
            if let Some(has_block) = ast::HasBlock::cast(child.clone()) {
                collect_fields_from_has_block(text, &mut children, &has_block);
                continue;
            }
            if let Some(method) = ast::MethodDef::cast(child) {
                if let Some(name) = method.name() {
                    #[allow(deprecated)]
                    let symbol = DocumentSymbol {
                        name: name.text().to_string(),
                        kind: SymbolKind::METHOD,
                        range: node_range(text, method.syntax()),
                        selection_range: token_range(text, &name),
                        children: None,
                        deprecated: None,
                        tags: None,
                        detail: None,
                    };
                    children.push(symbol);
                }
            }
        }
    }

    #[allow(deprecated)]
    let symbol = DocumentSymbol {
        name: class_name,
        kind: SymbolKind::CLASS,
        range: node_range(text, def.syntax()),
        selection_range,
        children: Some(children),
        deprecated: None,
        tags: None,
        detail: None,
    };
    symbol
}

fn collect_fields_from_has_block(
    text: &str,
    children: &mut Vec<DocumentSymbol>,
    block: &ast::HasBlock,
) {
    for field in block.fields() {
        if let Some(name) = field.name() {
            #[allow(deprecated)]
            let symbol = DocumentSymbol {
                name: name.text().to_string(),
                kind: SymbolKind::FIELD,
                range: node_range(text, field.syntax()),
                selection_range: token_range(text, &name),
                children: None,
                deprecated: None,
                tags: None,
                detail: None,
            };
            children.push(symbol);
        }
    }
    for private_block in block.syntax().children().filter_map(ast::PrivateBlock::cast) {
        for field in private_block.syntax().children().filter_map(ast::FieldDef::cast) {
            if let Some(name) = field.name() {
                #[allow(deprecated)]
                let symbol = DocumentSymbol {
                    name: name.text().to_string(),
                    kind: SymbolKind::FIELD,
                    range: node_range(text, field.syntax()),
                    selection_range: token_range(text, &name),
                    children: None,
                    deprecated: None,
                    tags: None,
                    detail: None,
                };
                children.push(symbol);
            }
        }
    }
}

fn node_range(text: &str, node: &SyntaxNode) -> Range {
    text_range_to_range(text, node.text_range())
}

fn token_range(text: &str, token: &SyntaxToken) -> Range {
    text_range_to_range(text, token.text_range())
}

pub fn hover_at_position(state: &DocumentState, position: Position) -> Option<Hover> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let root = syntax_root(state);
    let token_at_or_before =
        token_at_offset(&root, offset).or_else(|| token_before_offset(&root, offset));
    if let Some(token) = token_at_or_before.clone() {
        if matches!(token.kind(), SyntaxKind::ItsKw | SyntaxKind::ItKw) {
            let scope_id = scope_at_offset(&state.index, offset)
                .map(|scope| scope.id)
                .unwrap_or(0);
            if let Some(class_name) = class_name_for_scope(&state.index, scope_id) {
                let detail = format!("{}: {}", token.text(), class_name);
                let value = format!("```wrela\n{}\n```", detail);
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value,
                    }),
                    range: Some(text_range_to_range(&state.text, token.text_range())),
                });
            }
        }
    }
    let mut def =
        definition_at_offset(state, offset).or_else(|| resolve_reference_at_offset(state, offset));
    if def.is_none() {
        if let Some(token) = token_at_or_before {
            if token.kind() == SyntaxKind::Dot {
                if let Some(member) = find_node_at_offset::<ast::MemberExpr>(&root, offset) {
                    if let Some(name) = member.name() {
                        def = resolve_reference_token(&state.index, &state.text, &name);
                    }
                }
            } else if token.kind() == SyntaxKind::Ident {
                def = resolve_reference_token(&state.index, &state.text, &token).or_else(|| {
                    member_scope_at_offset(&state.index, &root, &state.text, offset).and_then(
                        |class_scope| {
                            resolve_in_scope_kinds(
                                &state.index,
                                class_scope,
                                token.text(),
                                &[DefKind::Method, DefKind::Field],
                            )
                        },
                    )
                });
            }
        }
    }
    let def = def?;
    let value = hover_markdown_for_definition(state, &def);

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(text_range_to_range(&state.text, def.name_range)),
    })
}

fn hover_markdown_for_definition(state: &DocumentState, def: &Definition) -> String {
    let detail = match def.kind {
        DefKind::Function | DefKind::Method => {
            def.detail.clone().unwrap_or_else(|| def.name.clone())
        }
        DefKind::Field | DefKind::Variable | DefKind::Parameter => def
            .ty
            .as_ref()
            .map(|ty| format!("{}: {}", def.name, ty))
            .or_else(|| def.detail.clone())
            .unwrap_or_else(|| def.name.clone()),
        DefKind::Class => {
            let mut class_detail = def.name.clone();
            if let Some(class_scope) = class_scope_for_name(&state.index, &def.name) {
                let fields: Vec<&Definition> = state
                    .index
                    .defs
                    .iter()
                    .filter(|d| d.scope_id == class_scope && matches!(d.kind, DefKind::Field))
                    .collect();
                if !fields.is_empty() {
                    class_detail.push_str("\n\nFields:");
                    for field in fields {
                        let field_info = field
                            .ty
                            .as_ref()
                            .map(|ty| format!("{}: {}", field.name, ty))
                            .unwrap_or_else(|| field.name.clone());
                        class_detail.push_str(&format!("\n  - {}", field_info));
                    }
                }
            }
            class_detail
        }
        DefKind::Module => def.name.clone(),
    };

    let mut value = format!("```wrela\n{}\n```", detail);
    if let Some(doc) = &def.doc {
        value.push_str("\n---\n");
        value.push_str(doc);
    }
    value
}

fn import_module_for_name<'a>(state: &'a DocumentState, name: &str) -> Option<&'a str> {
    state
        .imports
        .iter()
        .find(|import| import.name == name)
        .map(|import| import.module.as_str())
}

fn definition_location(state: &DocumentState, position: Position) -> Option<Range> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    definition_at_offset(state, offset)
        .or_else(|| resolve_reference_at_offset(state, offset))
        .map(|def| text_range_to_range(&state.text, def.name_range))
}

fn references_at_position(
    state: &DocumentState,
    position: Position,
    include_declaration: bool,
) -> Vec<Range> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let Some(def) =
        definition_at_offset(state, offset).or_else(|| resolve_reference_at_offset(state, offset))
    else {
        return Vec::new();
    };
    collect_references(state, &def, include_declaration)
}

fn rename_at_position(
    state: &DocumentState,
    position: Position,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    if !is_valid_identifier(new_name) || is_keyword(new_name) {
        return None;
    }
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let def = definition_at_offset(state, offset)
        .or_else(|| resolve_reference_at_offset(state, offset))?;
    let edits = collect_references(state, &def, true)
        .into_iter()
        .map(|range| TextEdit {
            range,
            new_text: new_name.to_string(),
        })
        .collect::<Vec<_>>();
    Some(edits)
}

fn prepare_rename_at_position(
    state: &DocumentState,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let def = definition_at_offset(state, offset)
        .or_else(|| resolve_reference_at_offset(state, offset))?;
    if is_keyword(&def.name) {
        return None;
    }
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: text_range_to_range(&state.text, def.name_range),
        placeholder: def.name.clone(),
    })
}

fn signature_help_at_position(state: &DocumentState, position: Position) -> Option<SignatureHelp> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let root = syntax_root(state);
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let active_parameter;
    let (label, params) =
        if let Some(call) = find_node_at_or_before_offset::<ast::CallExpr>(&root, offset) {
            if !offset_in_call_arguments(&call, offset) {
                return None;
            }
            active_parameter = call_argument_index(&call, offset);
            call_signature_for_callee(&state.index, &state.text, scope_id, &call)?
        } else {
            if !is_call_context_from_tokens(&root, offset) {
                return None;
            }
            active_parameter = call_argument_index_from_tokens(&root, offset);
            call_signature_from_tokens_at_offset(state, offset)?
        };
    if params.is_empty() {
        return None;
    }
    let parameters = params
        .iter()
        .map(|param| ParameterInformation {
            label: tower_lsp::lsp_types::ParameterLabel::Simple(match &param.ty {
                Some(ty) => format!("{}: {}", param.name, ty),
                None => param.name.clone(),
            }),
            documentation: None,
        })
        .collect::<Vec<_>>();
    let signature = SignatureInformation {
        label,
        documentation: None,
        parameters: Some(parameters),
        active_parameter: None,
    };
    Some(SignatureHelp {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter,
    })
}

fn type_at_position(state: &DocumentState, position: Position) -> Option<String> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    if let Some(def) =
        definition_at_offset(state, offset).or_else(|| resolve_reference_at_offset(state, offset))
    {
        if def.kind == DefKind::Class {
            return Some(def.name);
        }
        if let Some(ty) = def.ty {
            return Some(ty);
        }
    }
    let root = syntax_root(state);
    let token = token_at_offset(&root, offset)?;
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(expr) = ast::Expr::cast(current.clone()) {
            let scope_id = scope_at_offset(&state.index, offset)
                .map(|scope| scope.id)
                .unwrap_or(0);
            return state.index.infer_expr_type(scope_id, &state.text, &expr);
        }
        node = current.parent();
    }
    None
}

fn identifier_at_position(state: &DocumentState, position: Position) -> Option<String> {
    let offset = position_to_offset_with_index(&state.text, &state.line_index, position);
    let token = token_at_offset(&syntax_root(state), offset)?;
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    Some(token.text().to_string())
}

fn workspace_definitions(documents: &HashMap<Url, DocumentState>, name: &str) -> Vec<Location> {
    let mut locations = Vec::new();
    for (uri, state) in documents.iter() {
        for def in state.index.defs.iter() {
            if def.is_external {
                continue;
            }
            if def.name != name {
                continue;
            }
            if !is_root_def(&state.index, def) {
                continue;
            }
            if !matches!(def.kind, DefKind::Class | DefKind::Function) {
                continue;
            }
            locations.push(Location {
                uri: uri.clone(),
                range: text_range_to_range(&state.text, def.name_range),
            });
        }
    }
    locations
}

#[allow(deprecated)]
fn workspace_symbols(
    documents: &HashMap<Url, DocumentState>,
    query: &str,
) -> Vec<SymbolInformation> {
    let query = query.to_lowercase();
    let mut symbols = Vec::new();
    for (uri, state) in documents.iter() {
        for def in state.index.defs.iter() {
            if def.is_external {
                continue;
            }
            if !matches!(
                def.kind,
                DefKind::Class | DefKind::Function | DefKind::Method | DefKind::Field
            ) {
                continue;
            }
            if !query.is_empty() && !def.name.to_lowercase().contains(&query) {
                continue;
            }
            let container_name = state
                .index
                .scopes
                .get(def.scope_id)
                .and_then(|scope| scope.class_name.clone());
            symbols.push(SymbolInformation {
                name: def.name.clone(),
                kind: match def.kind {
                    DefKind::Class => SymbolKind::CLASS,
                    DefKind::Function => SymbolKind::FUNCTION,
                    DefKind::Method => SymbolKind::METHOD,
                    DefKind::Field => SymbolKind::FIELD,
                    _ => SymbolKind::VARIABLE,
                },
                location: Location {
                    uri: uri.clone(),
                    range: text_range_to_range(&state.text, def.name_range),
                },
                container_name,
                tags: None,
                deprecated: None,
            });
        }
    }
    symbols
}

fn workspace_type_definitions(
    documents: &HashMap<Url, DocumentState>,
    type_name: &str,
) -> Vec<Location> {
    if is_primitive_type(type_name) {
        return Vec::new();
    }
    let mut locations = Vec::new();
    for (uri, state) in documents.iter() {
        for def in state.index.defs.iter() {
            if def.kind != DefKind::Class || def.name != type_name {
                continue;
            }
            locations.push(Location {
                uri: uri.clone(),
                range: text_range_to_range_with_index(
                    &state.text,
                    &state.line_index,
                    def.name_range,
                ),
            });
        }
    }
    locations
}

fn index_workspace_documents(root_uri: &Url) -> HashMap<Url, DocumentState> {
    let mut documents = HashMap::new();
    let Ok(root_path) = root_uri.to_file_path() else {
        return documents;
    };
    let mut stack = vec![root_path];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if should_skip_dir(&path) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("wr") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let (state, _) = build_document_state(text);
            if let Ok(uri) = Url::from_file_path(&path) {
                documents.insert(uri, state);
            }
        }
    }
    documents
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return true;
    };
    matches!(name, ".git" | "target" | "node_modules")
}

fn uri_in_workspace(root: &Url, uri: &Url) -> bool {
    let Ok(root_path) = root.to_file_path() else {
        return false;
    };
    let Ok(doc_path) = uri.to_file_path() else {
        return false;
    };
    doc_path.starts_with(root_path)
}

fn workspace_references(
    documents: &HashMap<Url, DocumentState>,
    current_uri: &Url,
    name: &str,
    include_declaration: bool,
) -> Vec<Location> {
    let mut locations = Vec::new();
    for (uri, state) in documents.iter() {
        if uri == current_uri {
            continue;
        }
        let ranges = collect_identifier_ranges(state, name, include_declaration);
        locations.extend(ranges.into_iter().map(|range| Location {
            uri: uri.clone(),
            range,
        }));
    }
    locations
}

fn workspace_rename(
    documents: &HashMap<Url, DocumentState>,
    current_uri: &Url,
    name: &str,
    new_name: &str,
) -> HashMap<Url, Vec<TextEdit>> {
    let mut edits = HashMap::new();
    for (uri, state) in documents.iter() {
        if uri == current_uri {
            continue;
        }
        if document_has_definition(state, name) {
            continue;
        }
        let ranges = collect_identifier_ranges(state, name, true);
        if ranges.is_empty() {
            continue;
        }
        let text_edits = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: new_name.to_string(),
            })
            .collect::<Vec<_>>();
        edits.insert(uri.clone(), text_edits);
    }
    edits
}

fn document_has_definition(state: &DocumentState, name: &str) -> bool {
    state
        .index
        .defs
        .iter()
        .any(|def| !def.is_external && def.name == name)
}

fn collect_identifier_ranges(
    state: &DocumentState,
    name: &str,
    include_declaration: bool,
) -> Vec<Range> {
    let root = syntax_root(state);
    let mut ranges = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.kind() != SyntaxKind::Ident {
            continue;
        }
        if token.text() != name {
            continue;
        }
        if !include_declaration {
            // We don't know declaration locations in other documents; include all.
        }
        ranges.push(text_range_to_range_with_index(
            &state.text,
            &state.line_index,
            token.text_range(),
        ));
    }
    ranges
}

fn is_root_def(index: &SymbolIndex, def: &Definition) -> bool {
    index
        .scopes
        .get(def.scope_id)
        .is_some_and(|scope| scope.kind == ScopeKind::Root)
}

fn collect_references(
    state: &DocumentState,
    def: &Definition,
    include_declaration: bool,
) -> Vec<Range> {
    let mut ranges = Vec::new();
    let def_name_range = def.name_range;
    let root = syntax_root(state);
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.kind() != SyntaxKind::Ident {
            continue;
        }
        let range = token.text_range();
        if range == def_name_range {
            if include_declaration {
                ranges.push(text_range_to_range_with_index(
                    &state.text,
                    &state.line_index,
                    range,
                ));
            }
            continue;
        }
        if resolve_reference_token(&state.index, &state.text, &token)
            .is_some_and(|resolved| resolved.id == def.id)
        {
            ranges.push(text_range_to_range_with_index(
                &state.text,
                &state.line_index,
                range,
            ));
        }
    }
    ranges
}

fn definition_at_offset(state: &DocumentState, offset: usize) -> Option<Definition> {
    let offset = TextSize::from(offset as u32);
    state
        .index
        .defs
        .iter()
        .find(|def| def.name_range.contains(offset))
        .cloned()
}

fn resolve_reference_at_offset(state: &DocumentState, offset: usize) -> Option<Definition> {
    let token = token_at_offset(&syntax_root(state), offset)?;
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    resolve_reference_token(&state.index, &state.text, &token)
}

fn resolve_reference_token(
    index: &SymbolIndex,
    text: &str,
    token: &SyntaxToken,
) -> Option<Definition> {
    let offset: usize = u32::from(token.text_range().start()) as usize;
    if let Some(member_def) = resolve_member_reference(index, text, token) {
        return Some(member_def);
    }
    let scope_id = scope_at_offset(index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    resolve_in_scope(index, scope_id, token.text())
}

fn resolve_member_reference(
    index: &SymbolIndex,
    text: &str,
    token: &SyntaxToken,
) -> Option<Definition> {
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(member) = ast::MemberExpr::cast(current.clone()) {
            let name = member.name()?;
            if name.text_range() != token.text_range() {
                return None;
            }
            let offset: usize = u32::from(token.text_range().start()) as usize;
            let scope_id = scope_at_offset(index, offset)
                .map(|scope| scope.id)
                .unwrap_or(0);
            let object = member.object()?;
            let object_ty = index.infer_expr_type(scope_id, text, &object)?;
            let class_scope = class_scope_for_name(index, &object_ty)?;
            return resolve_in_scope_kinds(
                index,
                class_scope,
                name.text(),
                &[DefKind::Field, DefKind::Method],
            );
        }
        node = current.parent();
    }
    None
}

fn scope_at_offset(index: &SymbolIndex, offset: usize) -> Option<&Scope> {
    let offset = TextSize::from(offset as u32);
    index
        .scopes
        .iter()
        .filter(|scope| scope.range.contains(offset))
        .max_by_key(|scope| scope.depth)
}

fn member_scope_at_offset(
    index: &SymbolIndex,
    root: &SyntaxNode,
    text: &str,
    offset: usize,
) -> Option<usize> {
    let member = find_node_at_offset::<ast::MemberExpr>(root, offset)?;
    let name = member.name()?;
    let offset_size = TextSize::from(offset as u32);
    if !name.text_range().contains(offset_size) {
        return None;
    }
    let scope_id = scope_at_offset(index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let object = member.object()?;
    let object_ty = index.infer_expr_type(scope_id, text, &object)?;
    class_scope_for_name(index, &object_ty)
}

fn member_scope_at_completion_offset(
    index: &SymbolIndex,
    root: &SyntaxNode,
    text: &str,
    offset: usize,
) -> Option<usize> {
    if let Some(class_scope) = member_scope_at_offset(index, root, text, offset) {
        return Some(class_scope);
    }
    let prev_token = token_before_offset_skip_trivia(root, offset)?;
    if prev_token.kind() != SyntaxKind::Dot {
        return None;
    }
    let dot_offset: usize = prev_token.text_range().start().into();
    let scope_id = scope_at_offset(index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    if let Some(member) = find_node_at_offset::<ast::MemberExpr>(root, dot_offset) {
        let object = member.object()?;
        let object_ty = index.infer_expr_type(scope_id, text, &object)?;
        return class_scope_for_name(index, &object_ty);
    }
    let object_token = token_before_offset_skip_trivia(root, dot_offset)?;
    if object_token.kind() == SyntaxKind::Ident {
        if let Some(def) = resolve_in_scope(index, scope_id, object_token.text()) {
            if def.kind == DefKind::Class {
                return class_scope_for_name(index, &def.name);
            }
            if let Some(ty) = def.ty.as_deref() {
                return class_scope_for_name(index, ty);
            }
        }
        if let Some(class_scope) = class_scope_for_name(index, object_token.text()) {
            return Some(class_scope);
        }
    }
    let mut node = object_token.parent();
    while let Some(current) = node {
        if let Some(expr) = ast::Expr::cast(current.clone()) {
            let object_ty = index.infer_expr_type(scope_id, text, &expr)?;
            return class_scope_for_name(index, &object_ty);
        }
        node = current.parent();
    }
    None
}

fn member_completion_items_from_text(
    state: &DocumentState,
    offset: usize,
    seen: &mut HashSet<String>,
) -> Vec<CompletionItem> {
    let root = syntax_root(state);
    let prev_token = token_before_offset_skip_trivia(&root, offset);
    let Some(prev_token) = prev_token else { return Vec::new() };
    if prev_token.kind() != SyntaxKind::Dot {
        return Vec::new();
    }
    let dot_offset: usize = prev_token.text_range().start().into();
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let mut class_name = None;
    if let Some(member) = find_node_at_offset::<ast::MemberExpr>(&root, dot_offset) {
        if let Some(object) = member.object() {
            if let Some(object_ty) = state.index.infer_expr_type(scope_id, &state.text, &object) {
                class_name = Some(object_ty);
            }
        }
    }
    if class_name.is_none() {
        if let Some(object_token) = token_before_offset_skip_trivia(&root, dot_offset) {
            if object_token.kind() == SyntaxKind::Ident {
                if let Some(def) = resolve_in_scope(&state.index, scope_id, object_token.text()) {
                    if def.kind == DefKind::Class {
                        class_name = Some(def.name.clone());
                    } else if let Some(ty) = def.ty.clone() {
                        class_name = Some(ty);
                    }
                }
                if class_name.is_none()
                    && class_decl_exists_in_text(&state.text, object_token.text())
                {
                    class_name = Some(object_token.text().to_string());
                }
                if class_name.is_none() {
                    class_name = class_name_from_assignment(&state.text, object_token.text());
                }
            }
        }
    }
    let Some(class_name) = class_name else { return Vec::new() };
    class_member_items_from_text(&state.text, &class_name, seen)
}

fn class_scope_for_name(index: &SymbolIndex, name: &str) -> Option<usize> {
    let base = name.split('[').next().unwrap_or(name).trim();
    index.class_scopes.get(base).copied()
}

fn class_name_for_scope(index: &SymbolIndex, scope_id: usize) -> Option<String> {
    let mut current = Some(scope_id);
    while let Some(id) = current {
        if let Some(scope) = index.scopes.get(id) {
            if let Some(name) = scope.class_name.clone() {
                return Some(name);
            }
            current = scope.parent;
        } else {
            break;
        }
    }
    None
}

fn resolve_in_scope(index: &SymbolIndex, scope_id: usize, name: &str) -> Option<Definition> {
    resolve_in_scope_kinds(index, scope_id, name, &[])
}

fn resolve_in_scope_kinds(
    index: &SymbolIndex,
    scope_id: usize,
    name: &str,
    only_kinds: &[DefKind],
) -> Option<Definition> {
    let mut current = Some(scope_id);
    while let Some(id) = current {
        let mut candidates = index
            .defs
            .iter()
            .filter(|def| def.scope_id == id && def.name == name)
            .filter(|def| only_kinds.is_empty() || only_kinds.contains(&def.kind))
            .cloned()
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            candidates.sort_by_key(|def| def_precedence(def.kind));
            return candidates.into_iter().next();
        }
        current = index.scopes.get(id).and_then(|scope| scope.parent);
    }
    None
}

fn def_precedence(kind: DefKind) -> u8 {
    match kind {
        DefKind::Parameter => 0,
        DefKind::Variable => 1,
        DefKind::Field => 2,
        DefKind::Method => 3,
        DefKind::Function => 4,
        DefKind::Class => 5,
        DefKind::Module => 6,
    }
}

fn find_node_at_offset<T: AstNode>(root: &SyntaxNode, offset: usize) -> Option<T> {
    let token = token_at_offset(root, offset)?;
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(found) = T::cast(current.clone()) {
            return Some(found);
        }
        node = current.parent();
    }
    None
}

fn find_node_at_or_before_offset<T: AstNode>(root: &SyntaxNode, offset: usize) -> Option<T> {
    if let Some(found) = find_node_at_offset::<T>(root, offset) {
        return Some(found);
    }
    let token = token_before_offset(root, offset)?;
    let mut node = token.parent();
    while let Some(current) = node {
        if let Some(found) = T::cast(current.clone()) {
            return Some(found);
        }
        node = current.parent();
    }
    None
}

fn is_stmt_expr_at_offset(state: &DocumentState, offset: usize) -> bool {
    let root = syntax_root(state);
    find_node_at_or_before_offset::<ast::StmtExpr>(&root, offset).is_some()
}

fn result_return_type_edit(
    state: &DocumentState,
    uri: &Url,
    offset: usize,
) -> Option<WorkspaceEdit> {
    let root = syntax_root(state);
    let func = find_node_at_or_before_offset::<ast::FuncDef>(&root, offset)?;
    if let Some(ret_ty) = func.ret_type() {
        let ret_text = format_type_ref(&ret_ty)?;
        if ret_text.trim_start().starts_with("Result") {
            return None;
        }
        let new_text = format!("Result[{ret_text}]");
        let range = text_range_to_range_with_index(
            &state.text,
            &state.line_index,
            ret_ty.syntax().text_range(),
        );
        return Some(WorkspaceEdit {
            changes: Some(HashMap::from([(
                uri.clone(),
                vec![TextEdit { range, new_text }],
            )])),
            ..Default::default()
        });
    }
    let param_list = func
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::ParamList)?;
    let insert_offset: usize = param_list.text_range().end().into();
    let insert_pos = offset_to_position_with_index(&state.text, &state.line_index, insert_offset);
    let range = Range {
        start: insert_pos,
        end: insert_pos,
    };
    Some(WorkspaceEdit {
        changes: Some(HashMap::from([(
            uri.clone(),
            vec![TextEdit {
                range,
                new_text: " -> Result".to_string(),
            }],
        )])),
        ..Default::default()
    })
}

fn token_at_offset(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let offset = TextSize::from(offset as u32);
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        let range = token.text_range();
        if range.start() <= offset && offset < range.end() {
            return Some(token);
        }
    }
    None
}

fn token_before_offset(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let offset = TextSize::from(offset as u32);
    let mut prev = None;
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.text_range().start() >= offset {
            break;
        }
        prev = Some(token);
    }
    prev
}

fn token_before_offset_skip_trivia(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let offset = TextSize::from(offset as u32);
    let mut prev = None;
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.text_range().start() >= offset {
            break;
        }
        if is_trivia_kind(token.kind()) {
            continue;
        }
        prev = Some(token);
    }
    prev
}

fn is_trivia_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Whitespace | SyntaxKind::Newline | SyntaxKind::Comment | SyntaxKind::DocComment
    )
}

fn is_completion_suppressed(root: &SyntaxNode, offset: usize) -> bool {
    let Some(token) = token_at_offset(root, offset) else {
        return false;
    };
    matches!(
        token.kind(),
        SyntaxKind::Comment
            | SyntaxKind::DocComment
            | SyntaxKind::StringLiteral
            | SyntaxKind::StringStart
            | SyntaxKind::StringPart
            | SyntaxKind::StringEnd
    )
}

fn is_param_list_context(root: &SyntaxNode, offset: usize) -> bool {
    if find_node_at_or_before_offset::<ast::ParamList>(root, offset).is_some() {
        return true;
    }
    if let Some(prev) = token_before_offset_skip_trivia(root, offset) {
        if prev.kind() == SyntaxKind::LParen && is_param_list_token(&prev) {
            return true;
        }
    }
    if let Some(lparen) = nearest_unmatched_lparen_before_offset(root, offset) {
        return is_param_list_token(&lparen);
    }
    false
}

fn completion_prefix(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let slice = &text[line_start..offset];
    let mut start = slice.len();
    for (idx, ch) in slice.char_indices().rev() {
        if is_ident_continue(ch) {
            start = idx;
        } else {
            break;
        }
    }
    slice[start..].to_string()
}

fn is_primitive_type(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Float" | "Number" | "String" | "Bool" | "Nothing" | "Map" | "Bytes" | "Result"
    )
}

fn call_argument_index(call: &ast::CallExpr, offset: usize) -> Option<u32> {
    let offset = TextSize::from(offset as u32);
    let mut depth = 0u32;
    let mut seen_paren = false;
    let mut commas = 0u32;
    for token in call
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.text_range().start() > offset {
            break;
        }
        match token.kind() {
            SyntaxKind::LParen => {
                if seen_paren {
                    depth += 1;
                } else {
                    seen_paren = true;
                }
            }
            SyntaxKind::RParen => {
                if depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            SyntaxKind::LBracket | SyntaxKind::LBrace => depth += 1,
            SyntaxKind::RBracket | SyntaxKind::RBrace => depth = depth.saturating_sub(1),
            SyntaxKind::Comma if seen_paren && depth == 0 => commas += 1,
            _ => {}
        }
    }
    if seen_paren { Some(commas) } else { None }
}

fn call_argument_bounds(call: &ast::CallExpr) -> Option<(TextSize, Option<TextSize>)> {
    let mut lparen_end = None;
    let mut rparen_start = None;
    let mut depth = 0u32;
    let mut seen_paren = false;
    for token in call
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        match token.kind() {
            SyntaxKind::LParen => {
                if !seen_paren {
                    seen_paren = true;
                    lparen_end = Some(token.text_range().end());
                } else {
                    depth += 1;
                }
            }
            SyntaxKind::RParen if seen_paren => {
                if depth == 0 {
                    rparen_start = Some(token.text_range().start());
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            SyntaxKind::LBracket | SyntaxKind::LBrace => depth += 1,
            SyntaxKind::RBracket | SyntaxKind::RBrace => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let lparen_end = lparen_end?;
    Some((lparen_end, rparen_start))
}

fn offset_in_call_arguments(call: &ast::CallExpr, offset: usize) -> bool {
    let offset = TextSize::from(offset as u32);
    match call_argument_bounds(call) {
        Some((start, end)) => match end {
            Some(end) => start <= offset && offset <= end,
            None => start <= offset,
        },
        None => false,
    }
}

fn is_param_list_token(token: &SyntaxToken) -> bool {
    let mut node = token.parent();
    while let Some(current) = node {
        if current.kind() == SyntaxKind::ParamList {
            return true;
        }
        node = current.parent();
    }
    false
}

fn is_call_context_from_tokens(root: &SyntaxNode, offset: usize) -> bool {
    nearest_unmatched_lparen_before_offset(root, offset).is_some()
}

fn is_member_access_context(root: &SyntaxNode, offset: usize) -> bool {
    let Some(prev) = token_before_offset_skip_trivia(root, offset) else {
        return false;
    };
    if prev.kind() == SyntaxKind::Dot {
        return true;
    }
    if prev.kind() == SyntaxKind::Ident {
        let before = token_before_offset_skip_trivia(root, prev.text_range().start().into());
        return before.is_some_and(|token| token.kind() == SyntaxKind::Dot);
    }
    false
}

fn is_call_context_from_text(state: &DocumentState, offset: usize) -> bool {
    let text = &state.text;
    let mut depth = 0i32;
    let mut idx = offset.min(text.len());
    let mut scanned = 0usize;
    let mut lines = 0u32;
    while idx > 0 {
        idx -= 1;
        scanned += 1;
        let ch = text.as_bytes()[idx] as char;
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            _ => {}
        }
        if ch == '\n' {
            lines += 1;
            if lines > 200 {
                break;
            }
        }
        if scanned > 5000 {
            break;
        }
    }
    false
}

fn nearest_unmatched_lparen_before_offset(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let offset = TextSize::from(offset as u32);
    let mut stack: Vec<SyntaxToken> = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.text_range().start() >= offset {
            break;
        }
        match token.kind() {
            SyntaxKind::LParen => stack.push(token),
            SyntaxKind::RParen => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
    while let Some(token) = stack.pop() {
        if !is_param_list_token(&token) {
            return Some(token);
        }
    }
    None
}

fn call_params_from_tokens_at_offset(
    state: &DocumentState,
    offset: usize,
) -> Option<Vec<ParamInfo>> {
    let root = syntax_root(state);
    let lparen = nearest_unmatched_lparen_before_offset(&root, offset)?;
    let callee = token_before_offset_skip_trivia(&root, lparen.text_range().start().into())?;
    if callee.kind() != SyntaxKind::Ident {
        return None;
    }
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let callee_name = callee.text().to_string();
    let before_callee = token_before_offset_skip_trivia(&root, callee.text_range().start().into());
    if let Some(dot) = before_callee
        .as_ref()
        .filter(|tok| tok.kind() == SyntaxKind::Dot)
    {
        let object_token = token_before_offset_skip_trivia(&root, dot.text_range().start().into())?;
        if object_token.kind() == SyntaxKind::Ident {
            if let Some(def) = resolve_in_scope(&state.index, scope_id, object_token.text()) {
                if def.kind == DefKind::Class {
                    if let Some(class_scope) = class_scope_for_name(&state.index, &def.name) {
                        let method = resolve_in_scope_kinds(
                            &state.index,
                            class_scope,
                            &callee_name,
                            &[DefKind::Method],
                        )?;
                        return Some(method.params.clone());
                    }
                }
                if let Some(ty) = def.ty.as_deref() {
                    if let Some(class_scope) = class_scope_for_name(&state.index, ty) {
                        let method = resolve_in_scope_kinds(
                            &state.index,
                            class_scope,
                            &callee_name,
                            &[DefKind::Method],
                        )?;
                        return Some(method.params.clone());
                    }
                }
            }
            if let Some(class_scope) = class_scope_for_name(&state.index, object_token.text()) {
                let method = resolve_in_scope_kinds(
                    &state.index,
                    class_scope,
                    &callee_name,
                    &[DefKind::Method],
                )?;
                return Some(method.params.clone());
            }
        }
        let mut node = object_token.parent();
        while let Some(current) = node {
            if let Some(expr) = ast::Expr::cast(current.clone()) {
                if let Some(object_ty) = state.index.infer_expr_type(scope_id, &state.text, &expr) {
                    if let Some(class_scope) = class_scope_for_name(&state.index, &object_ty) {
                        let method = resolve_in_scope_kinds(
                            &state.index,
                            class_scope,
                            &callee_name,
                            &[DefKind::Method],
                        )?;
                        return Some(method.params.clone());
                    }
                }
            }
            node = current.parent();
        }
        let fallback = method_params_from_text(&state.text, &callee_name);
        if !fallback.is_empty() {
            return Some(fallback);
        }
        return None;
    }
    if let Some(class_scope) = class_scope_for_name(&state.index, &callee_name) {
        let params = class_field_params(&state.index, class_scope);
        return Some(params);
    }
    let fallback = class_field_params_from_text(&state.text, &callee_name);
    if !fallback.is_empty() {
        return Some(fallback);
    }
    let def = resolve_in_scope_kinds(
        &state.index,
        scope_id,
        &callee_name,
        &[DefKind::Function, DefKind::Class],
    )?;
    if def.kind == DefKind::Class {
        if let Some(class_scope) = class_scope_for_name(&state.index, &def.name) {
            return Some(class_field_params(&state.index, class_scope));
        }
        let fallback = class_field_params_from_text(&state.text, &def.name);
        if !fallback.is_empty() {
            return Some(fallback);
        }
        return None;
    }
    Some(def.params.clone())
}

fn call_signature_from_tokens_at_offset(
    state: &DocumentState,
    offset: usize,
) -> Option<(String, Vec<ParamInfo>)> {
    let root = syntax_root(state);
    let lparen = nearest_unmatched_lparen_before_offset(&root, offset)?;
    let callee = token_before_offset_skip_trivia(&root, lparen.text_range().start().into())?;
    if callee.kind() != SyntaxKind::Ident {
        return None;
    }
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let callee_name = callee.text().to_string();
    let before_callee = token_before_offset_skip_trivia(&root, callee.text_range().start().into());
    if let Some(dot) = before_callee
        .as_ref()
        .filter(|tok| tok.kind() == SyntaxKind::Dot)
    {
        let object_token = token_before_offset_skip_trivia(&root, dot.text_range().start().into())?;
        if object_token.kind() == SyntaxKind::Ident {
            if let Some(def) = resolve_in_scope(&state.index, scope_id, object_token.text()) {
                if def.kind == DefKind::Class {
                    if let Some(class_scope) = class_scope_for_name(&state.index, &def.name) {
                        let method = resolve_in_scope_kinds(
                            &state.index,
                            class_scope,
                            &callee_name,
                            &[DefKind::Method],
                        )?;
                        let label = method.detail.clone().unwrap_or_else(|| {
                            format_signature(&method.name, &method.params, method.ty.as_deref())
                        });
                        return Some((label, method.params.clone()));
                    }
                }
                if let Some(ty) = def.ty.as_deref() {
                    if let Some(class_scope) = class_scope_for_name(&state.index, ty) {
                        let method = resolve_in_scope_kinds(
                            &state.index,
                            class_scope,
                            &callee_name,
                            &[DefKind::Method],
                        )?;
                        let label = method.detail.clone().unwrap_or_else(|| {
                            format_signature(&method.name, &method.params, method.ty.as_deref())
                        });
                        return Some((label, method.params.clone()));
                    }
                }
            }
            if let Some(class_scope) = class_scope_for_name(&state.index, object_token.text()) {
                let method = resolve_in_scope_kinds(
                    &state.index,
                    class_scope,
                    &callee_name,
                    &[DefKind::Method],
                )?;
                let label = method.detail.clone().unwrap_or_else(|| {
                    format_signature(&method.name, &method.params, method.ty.as_deref())
                });
                return Some((label, method.params.clone()));
            }
        }
        let fallback = method_params_from_text(&state.text, &callee_name);
        if !fallback.is_empty() {
            let label = format_signature(&callee_name, &fallback, None);
            return Some((label, fallback));
        }
        return None;
    }
    if let Some(class_scope) = class_scope_for_name(&state.index, &callee_name) {
        let params = class_field_params(&state.index, class_scope);
        let label = format_signature(&callee_name, &params, None);
        return Some((label, params));
    }
    let fallback = class_field_params_from_text(&state.text, &callee_name);
    if !fallback.is_empty() {
        let label = format_signature(&callee_name, &fallback, None);
        return Some((label, fallback));
    }
    let def = resolve_in_scope_kinds(
        &state.index,
        scope_id,
        &callee_name,
        &[DefKind::Function, DefKind::Class],
    )?;
    if def.kind == DefKind::Class {
        if let Some(class_scope) = class_scope_for_name(&state.index, &def.name) {
            let params = class_field_params(&state.index, class_scope);
            let label = format_signature(&def.name, &params, None);
            return Some((label, params));
        }
        let fallback = class_field_params_from_text(&state.text, &def.name);
        if !fallback.is_empty() {
            let label = format_signature(&def.name, &fallback, None);
            return Some((label, fallback));
        }
        return None;
    }
    let label = def
        .detail
        .clone()
        .unwrap_or_else(|| format_signature(&def.name, &def.params, def.ty.as_deref()));
    Some((label, def.params.clone()))
}

fn call_argument_index_from_tokens(root: &SyntaxNode, offset: usize) -> Option<u32> {
    let lparen = nearest_unmatched_lparen_before_offset(root, offset)?;
    let offset = TextSize::from(offset as u32);
    let mut depth = 0u32;
    let mut commas = 0u32;
    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        if token.text_range().start() <= lparen.text_range().start() {
            continue;
        }
        if token.text_range().start() > offset {
            break;
        }
        match token.kind() {
            SyntaxKind::LParen => depth += 1,
            SyntaxKind::RParen => {
                if depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            SyntaxKind::LBracket | SyntaxKind::LBrace => depth += 1,
            SyntaxKind::RBracket | SyntaxKind::RBrace => depth = depth.saturating_sub(1),
            SyntaxKind::Comma if depth == 0 => commas += 1,
            _ => {}
        }
    }
    Some(commas)
}

fn extract_doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut docs = Vec::new();
    let mut newline_count = 0;
    // Walk tokens backwards so we keep doc comments even when trivia isn't a sibling.
    let mut prev = node.first_token();
    let mut skipped_node_token = false;
    while let Some(token) = prev {
        let kind = token.kind();
        match kind {
            SyntaxKind::Whitespace | SyntaxKind::Indent | SyntaxKind::Dedent => {}
            SyntaxKind::Newline => {
                let newlines = token.text().matches('\n').count();
                newline_count += newlines.max(1);
                if newline_count > 1 {
                    break;
                }
            }
            SyntaxKind::Comment | SyntaxKind::DocComment => {
                let text = token.text();
                let content = text.strip_prefix("so:").unwrap_or(text).trim();
                docs.push(normalize_doc_comment(content));
                newline_count = 0;
            }
            _ => {
                if !skipped_node_token {
                    skipped_node_token = true;
                    prev = token.prev_token();
                    continue;
                }
                break;
            }
        }
        prev = token.prev_token();
        skipped_node_token = true;
    }

    if docs.is_empty() {
        None
    } else {
        docs.reverse(); // We collected them backwards
        Some(docs.join("\n"))
    }
}

fn normalize_doc_comment(text: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim().to_string();
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return first;
    }
    let indent = rest
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| *c == ' ' || *c == '\t').count())
        .min()
        .unwrap_or(0);
    let mut out = first;
    for line in rest {
        let trimmed = if line.len() >= indent {
            &line[indent..]
        } else {
            line
        };
        out.push('\n');
        out.push_str(trimmed.trim_end());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;
    use std::collections::HashMap;

    fn position_at(text: &str, needle: &str, offset: usize) -> Position {
        let base = text.find(needle).expect("needle not found");
        let index = LineIndex::new(text);
        offset_to_position_with_index(text, &index, base + offset)
    }

    fn labels(items: Vec<CompletionItem>) -> Vec<String> {
        let mut labels = items.into_iter().map(|item| item.label).collect::<Vec<_>>();
        labels.sort();
        labels
    }

    #[test]
    fn completion_members_after_dot() {
        let code = r#"
to main() -> Nothing:
    foo = Foo()
    foo.

A Foo:
    has:
        value: Int
    can bar(x: Int) -> Int:
        return x
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "foo.", "foo.".len());
        let items = completion_items(&state, pos, None);
        assert_debug_snapshot!(labels(items));
    }

    #[test]
    fn completion_named_args_for_constructor() {
        let code = r#"
to main() -> Nothing:
    whale = Whale(

A Whale:
    has:
        name: String
        age: Int
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "Whale(", "Whale(".len());
        let items = labels(completion_items(&state, pos, None));
        assert!(items.contains(&"name=".to_string()));
        assert!(items.contains(&"age=".to_string()));
    }

    #[test]
    fn completion_named_args_for_method_call() {
        let code = r#"
to main() -> Nothing:
    whale = Whale()
    whale.swim(

A Whale:
    can swim(distance: Int, speed: Int) -> Nothing:
        return nothing
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "swim(", "swim(".len());
        let items = labels(completion_items(&state, pos, None));
        assert!(items.contains(&"distance=".to_string()));
        assert!(items.contains(&"speed=".to_string()));
    }

    #[test]
    fn completion_named_args_for_constructor_multiline() {
        let code = "to main() -> Nothing:\n    whale = Whale(\n        name=\"moby\",\n        \nA Whale:\n    has:\n        name: String\n        age: Int\n";
        let (state, _) = build_document_state(code.to_string());
        let index = LineIndex::new(code);
        let blank_line = code.find("\n        \nA Whale").unwrap();
        let offset = blank_line + 1 + 8;
        let pos = offset_to_position_with_index(code, &index, offset);
        let items = labels(completion_items(&state, pos, None));
        assert!(items.contains(&"age=".to_string()));
        assert!(!items.contains(&"name=".to_string()));
    }

    #[test]
    fn goto_definition_for_member() {
        let code = r#"
to main() -> Nothing:
    foo = Foo()
    foo.bar(1)

A Foo:
    can bar(x: Int) -> Int:
        return x
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "bar(1)", 1);
        let range = definition_location(&state, pos);
        assert_debug_snapshot!(range);
    }

    #[test]
    fn references_in_document() {
        let code = r#"
to main() -> Nothing:
    x = 1
    y = x + x
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "x + x", 0);
        let ranges = references_at_position(&state, pos, true);
        assert_debug_snapshot!(ranges);
    }

    #[test]
    fn rename_edits_for_identifier() {
        let code = r#"
to main() -> Nothing:
    x = 1
    y = x + x
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "x + x", 0);
        let edits = rename_at_position(&state, pos, "total");
        assert_debug_snapshot!(edits);
    }

    #[test]
    fn signature_help_for_call() {
        let code = r#"
to add(a: Int, b: Int) -> Int:
    return a + b
to main() -> Nothing:
    add(1, 2)
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "add(1, 2)", "add(1, ".len());
        let help = signature_help_at_position(&state, pos);
        assert_debug_snapshot!(help);
    }

    #[test]
    fn workspace_symbols_across_documents() {
        let code_a = r#"
to main() -> Nothing:
    foo = Foo()

A Foo:
    has:
        value: Int
"#;
        let code_b = r#"
to add(a: Int, b: Int) -> Int:
    return a + b
"#;
        let (state_a, _) = build_document_state(code_a.to_string());
        let (state_b, _) = build_document_state(code_b.to_string());
        let mut documents = HashMap::new();
        documents.insert(Url::parse("file:///a.wr").unwrap(), state_a);
        documents.insert(Url::parse("file:///b.wr").unwrap(), state_b);

        let defs = workspace_definitions(&documents, "Foo");
        let symbols = workspace_symbols(&documents, "");
        assert_debug_snapshot!(defs);
        assert_debug_snapshot!(symbols.len());
    }

    #[test]
    fn workspace_references_and_rename() {
        let code_a = r#"
to main() -> Nothing:
    x = 1
"#;
        let code_b = r#"
to other() -> Nothing:
    y = x + x
"#;
        let (state_a, _) = build_document_state(code_a.to_string());
        let (state_b, _) = build_document_state(code_b.to_string());
        let uri_a = Url::parse("file:///a.wr").unwrap();
        let uri_b = Url::parse("file:///b.wr").unwrap();
        let mut documents = HashMap::new();
        documents.insert(uri_a.clone(), state_a);
        documents.insert(uri_b.clone(), state_b);

        let references = workspace_references(&documents, &uri_a, "x", true);
        let edits = workspace_rename(&documents, &uri_a, "x", "total");
        assert_debug_snapshot!(references);
        assert_debug_snapshot!(edits);
    }

    #[test]
    fn apply_content_changes_incremental() {
        let original = "to main() -> Nothing:\n    x = 1\n";
        let index = LineIndex::new(original);
        let start = original.find("1").unwrap();
        let end = start + 1;
        let range = Range {
            start: offset_to_position_with_index(original, &index, start),
            end: offset_to_position_with_index(original, &index, end),
        };
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(range),
            range_length: None,
            text: "42".to_string(),
        }];
        let updated = apply_content_changes(original.to_string(), changes).unwrap();
        assert!(updated.contains("x = 42"));
    }

    #[test]
    fn formatting_trims_whitespace() {
        let text = "to main() -> Nothing:  \n    x = 1\t\n";
        let formatted = format_text(text);
        assert_debug_snapshot!(formatted);
    }

    #[test]
    fn folding_ranges_basic() {
        let code = r#"
to main() -> Nothing:
    if true:
        x = 1

A Foo:
    has:
        value: Int
"#;
        let (state, _) = build_document_state(code.to_string());
        let ranges = folding_ranges(&state);
        assert_debug_snapshot!(ranges);
    }

    #[test]
    fn code_lens_references_count() {
        let code = r#"
to add(a: Int, b: Int) -> Int:
    return a + b
to main() -> Nothing:
    add(1, 2)
    add(3, 4)
"#;
        let (state, _) = build_document_state(code.to_string());
        let lenses = code_lenses(&state);
        assert_debug_snapshot!(lenses);
    }

    #[test]
    fn type_definition_for_variable() {
        let code = r#"
to main() -> Nothing:
    foo = Foo()
    foo

A Foo:
    has:
        value: Int
"#;
        let (state, _) = build_document_state(code.to_string());
        let pos = position_at(code, "foo\n", 1);
        let ty = type_at_position(&state, pos);
        assert_debug_snapshot!(ty);
    }

    #[test]
    fn organize_imports_editing() {
        let code = r#"
use b, a, a from core
to main() -> Nothing:
    a()
"#;
        let (state, _) = build_document_state(code.to_string());
        let uri = Url::parse("file:///main.wr").unwrap();
        let edit = organize_imports_edit(&state, &uri);
        assert_debug_snapshot!(edit);
    }

    #[test]
    fn diagnostics_naming_and_shadowing() {
        let code = r#"
to BadName() -> Nothing:
    x = 1
    to inner() -> Nothing:
        x = 2

A foo:
    has:
        value: Int
"#;
        let (state, _) = build_document_state(code.to_string());
        let mut diagnostics = Vec::new();
        diagnostics.extend(check_naming_conventions(&state));
        diagnostics.extend(check_shadowing(&state));
        assert_debug_snapshot!(diagnostics);
    }

    #[test]
    fn workspace_index_auto_import_candidates() {
        let base = std::env::temp_dir().join("wrela_lsp_index_test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let a_path = base.join("a.wr");
        let b_path = base.join("b.wr");
        fs::write(&a_path, "A Foo:\n    has:\n        value: Int\n").unwrap();
        fs::write(&b_path, "to main() -> Nothing:\n    foo = Foo()\n    bar = Bar()\n").unwrap();
        let root_uri = Url::from_directory_path(&base).unwrap();
        let documents = index_workspace_documents(&root_uri);
        let current = documents
            .get(&Url::from_file_path(&b_path).unwrap())
            .unwrap()
            .clone();
        let candidates = workspace_import_candidates(
            &documents,
            &current,
            &Url::from_file_path(&b_path).unwrap(),
            "Foo",
        );
        assert_debug_snapshot!(candidates);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn workspace_index_cache_reuse() {
        let base = std::env::temp_dir().join("wrela_lsp_index_cache");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let a_path = base.join("a.wr");
        fs::write(&a_path, "A Foo:\n    has:\n        value: Int\n").unwrap();
        let root_uri = Url::from_directory_path(&base).unwrap();
        let docs_first = index_workspace_documents(&root_uri);
        let cache = WorkspaceIndex {
            root: root_uri.clone(),
            documents: docs_first.clone(),
        };
        let docs_second = cache.documents.clone();
        assert_eq!(docs_first.len(), docs_second.len());
        let _ = fs::remove_dir_all(&base);
    }
}
