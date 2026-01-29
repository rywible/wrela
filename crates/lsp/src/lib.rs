use std::collections::{HashMap, HashSet};

use rowan::{TextRange, TextSize};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionItemKind,
    CompletionOptions, CompletionResponse, Diagnostic, DiagnosticSeverity, DocumentHighlight,
    DocumentHighlightParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, InlayHint,
    InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    Location, MarkupContent, MarkupKind, OneOf, ParameterInformation, Position,
    PrepareRenameResponse, Range, ReferenceParams, RenameOptions, RenameParams, SemanticToken,
    SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelp, SignatureHelpOptions,
    SignatureHelpParams, SignatureInformation, SymbolInformation, SymbolKind,
    TextDocumentContentChangeEvent, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use wrela::parser::ast::AstNode;
use wrela::parser::{self, ParseError, SyntaxNode, SyntaxToken, ast, kind::SyntaxKind};

#[derive(Clone)]
pub struct DocumentState {
    pub text: String,
    index: SymbolIndex,
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
        });
        id
    }

    fn collect_stmt(&mut self, scope_id: usize, text: &str, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(def) => self.collect_class(scope_id, text, def),
            ast::Stmt::FuncDef(def) => self.collect_function(scope_id, text, def),
            ast::Stmt::VarAssign(def) => {
                if let Some(name) = def.name() {
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
                for name in def.names() {
                    self.add_def(
                        name.text().to_string(),
                        DefKind::Module,
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
        let class_scope = self.add_scope(
            ScopeKind::Class,
            Some(scope_id),
            def.syntax().text_range(),
            class_name.clone(),
        );
        if let Some(name) = class_name.as_ref() {
            self.class_scopes.insert(name.clone(), class_scope);
        }

        for block in def.has_blocks() {
            for field in block.fields() {
                if let Some(name) = field.name() {
                    let ty = field
                        .ty()
                        .and_then(|ty| node_text(text, ty.syntax()))
                        .map(|ty| ty.to_string());
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
        }

        for method in def.methods() {
            if let Some(name) = method.name() {
                let params = params_from_iter(text, method.params());
                let ret_ty = method
                    .ret_type()
                    .and_then(|ty| node_text(text, ty.syntax()));
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
                    class_name.clone(),
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
    }

    fn collect_function(&mut self, scope_id: usize, text: &str, def: &ast::FuncDef) {
        if let Some(name) = def.name() {
            let params = params_from_iter(text, def.params());
            let ret_ty = def.ret_type().and_then(|ty| node_text(text, ty.syntax()));
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

pub struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, DocumentState>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }

    async fn publish_diagnostics(
        &self,
        uri: &Url,
        text: &str,
        errors: Vec<ParseError>,
        state: &DocumentState,
    ) {
        let mut diagnostics = diagnostics_for_errors(text, errors);
        diagnostics.extend(check_unused_variables(state));
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    async fn document_state(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.read().await.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String) -> (Vec<ParseError>, DocumentState) {
        let (state, errors) = build_document_state(text);
        self.documents.write().await.insert(uri, state.clone());
        (errors, state)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "(".to_string()]),
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
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SEMANTIC_TOKEN_LEGEND.clone(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                ))),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
        if let Some(change) = params.content_changes.into_iter().next() {
            if let Some(text) = full_text_change(change) {
                let (errors, state) = self.update_document(uri.clone(), text.clone()).await;
                self.publish_diagnostics(&uri, &text, errors, &state).await;
            }
        }
    }

    async fn did_close(&self, params: tower_lsp::lsp_types::DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
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
        let items = completion_items(&state, params.text_document_position.position);
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
        Ok(hover_at_position(
            &state,
            params.text_document_position_params.position,
        ))
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
        if let Some(range) = definition_location(&state, position) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range,
            })));
        }
        let Some(name) = identifier_at_position(&state.text, position) else {
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
        let name = identifier_at_position(&state.text, position);
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
        if let Some(name) = identifier_at_position(&state.text, position) {
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
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(Some(inlay_hints(&state)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(state) = self.document_state(&uri).await else {
            return Ok(None);
        };
        Ok(Some(
            code_actions(&state, params.range, &uri)
                .into_iter()
                .map(CodeActionOrCommand::CodeAction)
                .collect(),
        ))
    }
}

pub fn semantic_tokens(state: &DocumentState) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let root = parser::parse(&state.text);
    let mut last_line = 0;
    let mut last_char = 0;

    for token in root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
    {
        let kind = token.kind();
        let token_type = match kind {
            SyntaxKind::ClassKw | SyntaxKind::FuncDef => {
                // Handle keywords generally
                if is_keyword(token.text()) {
                    0 // KEYWORD
                } else {
                    continue;
                }
            }
            SyntaxKind::Ident => {
                if is_keyword(token.text()) {
                    0 // KEYWORD
                } else if let Some(def) =
                    resolve_reference_token(&state.index, &root, &state.text, &token)
                {
                    match def.kind {
                        DefKind::Class => 1,     // CLASS
                        DefKind::Function => 2,  // FUNCTION
                        DefKind::Method => 3,    // METHOD
                        DefKind::Field => 4,     // PROPERTY
                        DefKind::Variable => 5,  // VARIABLE
                        DefKind::Parameter => 6, // PARAMETER
                        DefKind::Module => 5,    // VARIABLE (fallback)
                    }
                } else {
                    // Try to guess based on context if not resolved
                    if let Some(parent) = token.parent() {
                        if ast::ClassDef::can_cast(parent.kind()) {
                            1 // CLASS
                        } else if ast::FuncDef::can_cast(parent.kind()) {
                            2 // FUNCTION
                        } else if ast::MethodDef::can_cast(parent.kind()) {
                            3 // METHOD
                        } else if ast::FieldDef::can_cast(parent.kind()) {
                            4 // PROPERTY
                        } else if ast::Param::can_cast(parent.kind()) {
                            6 // PARAMETER
                        } else {
                            5 // VARIABLE (default)
                        }
                    } else {
                        5 // VARIABLE
                    }
                }
            }
            SyntaxKind::StringLiteral
            | SyntaxKind::StringStart
            | SyntaxKind::StringPart
            | SyntaxKind::StringEnd => 7, // STRING
            SyntaxKind::IntNumber | SyntaxKind::FloatNumber => 8, // NUMBER
            k if is_operator(k) => 9,                             // OPERATOR
            SyntaxKind::Comment | SyntaxKind::DocComment => 10,   // COMMENT
            SyntaxKind::At => 11,                                 // DECORATOR
            _ => {
                if is_keyword(token.text()) {
                    0 // KEYWORD
                } else {
                    continue;
                }
            }
        };

        let range = token.text_range();
        let start = offset_to_position(&state.text, range.start().into());

        // Semantic tokens are delta-encoded
        let delta_line = start.line - last_line;
        let delta_start = if delta_line == 0 {
            start.character - last_char
        } else {
            start.character
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: (range.end() - range.start()).into(),
            token_type,
            token_modifiers_bitset: 0,
        });

        last_line = start.line;
        last_char = start.character;
    }
    tokens
}

fn is_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Plus
            | SyntaxKind::Minus
            | SyntaxKind::Star
            | SyntaxKind::Slash
            | SyntaxKind::Equals
            | SyntaxKind::EqEq
            | SyntaxKind::BangEq
            | SyntaxKind::Less
            | SyntaxKind::LessEq
            | SyntaxKind::Greater
            | SyntaxKind::GreaterEq
            | SyntaxKind::Arrow
            | SyntaxKind::Dot
            | SyntaxKind::Colon
            | SyntaxKind::Range
            | SyntaxKind::Comma
            | SyntaxKind::LParen
            | SyntaxKind::RParen
            | SyntaxKind::LBracket
            | SyntaxKind::RBracket
            | SyntaxKind::LBrace
            | SyntaxKind::RBrace
            | SyntaxKind::PlusEq
            | SyntaxKind::MinusEq
            | SyntaxKind::StarEq
            | SyntaxKind::SlashEq
            | SyntaxKind::Ampersand
            | SyntaxKind::Pipe
            | SyntaxKind::Caret
            | SyntaxKind::BitwiseNot
            | SyntaxKind::ShiftLeft
            | SyntaxKind::ShiftRight
    )
}

pub fn inlay_hints(state: &DocumentState) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let root = parser::parse(&state.text);

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
                if param_idx < def.params.len() {
                    let param_name = &def.params[param_idx].name;
                    hints.push(InlayHint {
                        position: offset_to_position(
                            &state.text,
                            expr.syntax().text_range().start().into(),
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
            position: offset_to_position(&state.text, end_pos.into()),
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

fn code_actions(state: &DocumentState, range: Range, uri: &Url) -> Vec<CodeAction> {
    // We can just re-run check_unused_variables to find if the current range matches an unused variable
    // In a real implementation, we might want to pass the diagnostics in CodeActionParams context
    // but re-running is safer to ensure we have the latest state.
    // Optimization: filtering diagnostics from params would be faster.

    let diagnostics = check_unused_variables(state);
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
                actions.push(CodeAction {
                    title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(
                            uri.clone(),
                            vec![TextEdit {
                                range: diag.range, // This is the full range from check_unused_variables
                                new_text: "".to_string(),
                            }],
                        )])),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }
        }
    }
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

pub fn check_unused_variables(state: &DocumentState) -> Vec<Diagnostic> {
    let root = parser::parse(&state.text);
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
            if let Some(def) = resolve_reference_token(&state.index, &root, &state.text, &token) {
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
                range: text_range_to_range(&state.text, def.range), // Use full range for easier deletion
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

fn full_text_change(change: TextDocumentContentChangeEvent) -> Option<String> {
    if change.range.is_none() && change.range_length.is_none() {
        Some(change.text)
    } else {
        None
    }
}

pub fn build_document_state(text: String) -> (DocumentState, Vec<ParseError>) {
    let (root, errors) = parser::parse_with_errors(&text);
    let index = SymbolIndex::build(&text, &root);
    (DocumentState { text, index }, errors)
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

fn position_to_offset(text: &str, position: Position) -> usize {
    let mut offset = 0usize;
    let mut current_line = 0u32;
    for line in text.split_inclusive('\n') {
        if current_line == position.line {
            let mut utf16_count = 0u32;
            for (idx, ch) in line.char_indices() {
                if utf16_count >= position.character {
                    offset += idx;
                    return offset.min(text.len());
                }
                utf16_count += ch.len_utf16() as u32;
            }
            offset += line.len();
            return offset.min(text.len());
        }
        offset += line.len();
        current_line += 1;
    }
    offset.min(text.len())
}

fn text_range_to_range(text: &str, range: TextRange) -> Range {
    Range {
        start: offset_to_position(text, range.start().into()),
        end: offset_to_position(text, range.end().into()),
    }
}

fn node_text(text: &str, node: &SyntaxNode) -> Option<String> {
    let range = node.text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    text.get(start..end).map(|slice| slice.to_string())
}

fn params_from_iter(text: &str, params: impl Iterator<Item = ast::Param>) -> Vec<ParamInfo> {
    params
        .filter_map(|param| {
            let name = param.name()?;
            let ty = param.ty().and_then(|ty| node_text(text, ty.syntax()));
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

fn literal_type(expr: &ast::LiteralExpr) -> Option<String> {
    for token in expr
        .syntax()
        .children_with_tokens()
        .filter_map(|it| it.into_token())
    {
        return match token.kind() {
            SyntaxKind::IntNumber => Some("Int".to_string()),
            SyntaxKind::FloatNumber => Some("Float".to_string()),
            SyntaxKind::TrueKw | SyntaxKind::FalseKw => Some("Bool".to_string()),
            SyntaxKind::NothingKw => Some("Nothing".to_string()),
            SyntaxKind::StringLiteral => Some("String".to_string()),
            _ => None,
        };
    }
    None
}

const KEYWORDS: &[&str] = &[
    "A",
    "An",
    "has",
    "can",
    "to",
    "public",
    "private",
    "changing",
    "if",
    "else",
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
];

const CONSTANTS: &[&str] = &["true", "false", "nothing", "it", "its"];

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

fn completion_items(state: &DocumentState, position: Position) -> Vec<CompletionItem> {
    let offset = position_to_offset(&state.text, position);
    let mut seen = HashSet::new();

    let root = parser::parse(&state.text);
    if let Some(class_scope) = member_scope_at_offset(&state.index, &root, &state.text, offset) {
        return member_completion_items(&state.index, class_scope, &mut seen);
    }

    let mut items = keyword_completion_items();
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    items.extend(scope_completion_items(&state.index, scope_id, &mut seen));
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
        DefKind::Function => CompletionItemKind::FUNCTION,
        DefKind::Method => CompletionItemKind::METHOD,
        DefKind::Field => CompletionItemKind::FIELD,
        DefKind::Module => CompletionItemKind::MODULE,
        DefKind::Parameter | DefKind::Variable => CompletionItemKind::VARIABLE,
    };
    let detail = def
        .detail
        .clone()
        .or_else(|| def.ty.as_ref().map(|ty| format!("{}: {}", def.name, ty)));
    CompletionItem {
        label: def.name.clone(),
        kind: Some(kind),
        detail,
        ..CompletionItem::default()
    }
}

#[allow(deprecated)]
fn document_symbols(state: &DocumentState) -> Vec<DocumentSymbol> {
    let root = parser::parse(&state.text);
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
        for field in block.fields() {
            if let Some(name) = field.name() {
                children.push(DocumentSymbol {
                    name: name.text().to_string(),
                    kind: SymbolKind::FIELD,
                    range: node_range(text, field.syntax()),
                    selection_range: token_range(text, &name),
                    children: None,
                    deprecated: None,
                    tags: None,
                    detail: None,
                });
            }
        }
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

    DocumentSymbol {
        name: class_name,
        kind: SymbolKind::CLASS,
        range: node_range(text, def.syntax()),
        selection_range,
        children: Some(children),
        deprecated: None,
        tags: None,
        detail: None,
    }
}

fn node_range(text: &str, node: &SyntaxNode) -> Range {
    text_range_to_range(text, node.text_range())
}

fn token_range(text: &str, token: &SyntaxToken) -> Range {
    text_range_to_range(text, token.text_range())
}

pub fn hover_at_position(state: &DocumentState, position: Position) -> Option<Hover> {
    let offset = position_to_offset(&state.text, position);
    let def =
        definition_at_offset(state, offset).or_else(|| resolve_reference_at_offset(state, offset));
    let def = def?;
    let _label = match def.kind {
        DefKind::Class => "class",
        DefKind::Function => "function",
        DefKind::Method => "method",
        DefKind::Field => "field",
        DefKind::Variable => "variable",
        DefKind::Parameter => "parameter",
        DefKind::Module => "module",
    };
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
        DefKind::Class | DefKind::Module => def.name.clone(),
    };

    // Add documentation if available
    let mut value = format!("```wrela\n{}\n```", detail);
    if let Some(doc) = &def.doc {
        value.push_str("\n---\n");
        value.push_str(doc);
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(text_range_to_range(&state.text, def.name_range)),
    })
}

fn definition_location(state: &DocumentState, position: Position) -> Option<Range> {
    let offset = position_to_offset(&state.text, position);
    definition_at_offset(state, offset)
        .or_else(|| resolve_reference_at_offset(state, offset))
        .map(|def| text_range_to_range(&state.text, def.name_range))
}

fn references_at_position(
    state: &DocumentState,
    position: Position,
    include_declaration: bool,
) -> Vec<Range> {
    let offset = position_to_offset(&state.text, position);
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
    let offset = position_to_offset(&state.text, position);
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
    let offset = position_to_offset(&state.text, position);
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
    let offset = position_to_offset(&state.text, position);
    let root = parser::parse(&state.text);
    let call = find_node_at_offset::<ast::CallExpr>(&root, offset)?;
    let callee_name = match call.callee()? {
        ast::Expr::Ident(expr) => expr.name().map(|token| token.text().to_string()),
        ast::Expr::Member(expr) => expr.name().map(|token| token.text().to_string()),
        _ => None,
    }?;
    let scope_id = scope_at_offset(&state.index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    let def = resolve_in_scope_kinds(
        &state.index,
        scope_id,
        &callee_name,
        &[DefKind::Function, DefKind::Method],
    )
    .or_else(|| {
        member_scope_at_offset(&state.index, &root, &state.text, offset).and_then(|class_scope| {
            resolve_in_scope_kinds(&state.index, class_scope, &callee_name, &[DefKind::Method])
        })
    })?;
    if def.params.is_empty() {
        return None;
    }
    let active_parameter = call_argument_index(&call, offset);
    let parameters = def
        .params
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
        label: def.detail.clone().unwrap_or_else(|| def.name.clone()),
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

fn identifier_at_position(text: &str, position: Position) -> Option<String> {
    let offset = position_to_offset(text, position);
    let root = parser::parse(text);
    let token = token_at_offset(&root, offset)?;
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    Some(token.text().to_string())
}

fn workspace_definitions(documents: &HashMap<Url, DocumentState>, name: &str) -> Vec<Location> {
    let mut locations = Vec::new();
    for (uri, state) in documents.iter() {
        for def in state.index.defs.iter() {
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
        if document_has_definition(state, name) {
            continue;
        }
        let ranges = collect_identifier_ranges(&state.text, name, include_declaration);
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
        let ranges = collect_identifier_ranges(&state.text, name, true);
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
    state.index.defs.iter().any(|def| def.name == name)
}

fn collect_identifier_ranges(text: &str, name: &str, include_declaration: bool) -> Vec<Range> {
    let root = parser::parse(text);
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
        ranges.push(text_range_to_range(text, token.text_range()));
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
    let root = parser::parse(&state.text);
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
                ranges.push(text_range_to_range(&state.text, range));
            }
            continue;
        }
        if resolve_reference_token(&state.index, &root, &state.text, &token)
            .is_some_and(|resolved| resolved.id == def.id)
        {
            ranges.push(text_range_to_range(&state.text, range));
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
    let root = parser::parse(&state.text);
    let token = token_at_offset(&root, offset)?;
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    resolve_reference_token(&state.index, &root, &state.text, &token)
}

fn resolve_reference_token(
    index: &SymbolIndex,
    root: &SyntaxNode,
    text: &str,
    token: &SyntaxToken,
) -> Option<Definition> {
    let offset: usize = u32::from(token.text_range().start()) as usize;
    if let Some(member_def) = resolve_in_class_members(index, root, text, offset, token.text()) {
        return Some(member_def);
    }
    let scope_id = scope_at_offset(index, offset)
        .map(|scope| scope.id)
        .unwrap_or(0);
    resolve_in_scope(index, scope_id, token.text())
}

fn resolve_in_class_members(
    index: &SymbolIndex,
    root: &SyntaxNode,
    text: &str,
    offset: usize,
    name: &str,
) -> Option<Definition> {
    let Some(class_scope) = member_scope_at_offset(index, root, text, offset) else {
        return None;
    };
    resolve_in_scope_kinds(index, class_scope, name, &[DefKind::Field, DefKind::Method])
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
    Some(commas)
}

fn extract_doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut docs = Vec::new();
    // Look at previous siblings for comments
    let mut prev = node.prev_sibling_or_token();
    while let Some(element) = prev {
        match element.kind() {
            SyntaxKind::Whitespace => {
                // Skip whitespace but if we see too many newlines, stop?
                // For now just skip whitespace
            }
            SyntaxKind::DocComment => {
                // element is moved by into_token, so clone it for that check,
                // but we need element for prev_sibling_or_token later.
                // Actually, we can just use element.as_token() if available or just check kind
                // Since we already checked kind == DocComment, it SHOULD be a token.
                // Let's use as_token() if it exists or clone.
                if let Some(token) = element.as_token() {
                    let text = token.text();
                    // Remove '///' and trim
                    let content = if text.starts_with("///") {
                        text[3..].trim()
                    } else {
                        text.trim()
                    };
                    docs.push(content.to_string());
                }
            }
            _ => break, // Stop at anything else
        }
        prev = element.prev_sibling_or_token();
    }

    if docs.is_empty() {
        None
    } else {
        docs.reverse(); // We collected them backwards
        Some(docs.join("\n"))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
