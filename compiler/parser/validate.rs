use crate::diag::catalog::ValidationDiagKind;
use crate::parser::SyntaxKind;
use crate::parser::SyntaxNode;
use crate::parser::ast;
use crate::parser::ast::AstNode;
use miette::SourceSpan;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub kind: ValidationDiagKind,
    pub message: String,
    pub span: SourceSpan,
}

pub fn validate(root: &SyntaxNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for element in root.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind() == SyntaxKind::InvalidLiteral {
            errors.push(ValidationError {
                kind: ValidationDiagKind::InvalidLiteral,
                message: "invalid numeric literal".to_string(),
                span: span_for_token(&token),
            });
        }
    }
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::RegionDecl | SyntaxKind::DomainDecl | SyntaxKind::RenderDecl => {
                let (name_message, name_present, has_params) = match node.kind() {
                    SyntaxKind::RegionDecl => {
                        let region = ast::RegionDecl::cast(node.clone());
                        (
                            "region declaration requires a name",
                            region.as_ref().and_then(|decl| decl.name()).is_some(),
                            has_token(&node, SyntaxKind::LParen)
                                && has_token(&node, SyntaxKind::RParen),
                        )
                    }
                    SyntaxKind::DomainDecl => {
                        let domain = ast::DomainDecl::cast(node.clone());
                        (
                            "domain declaration requires a name",
                            domain.as_ref().and_then(|decl| decl.name()).is_some(),
                            has_token(&node, SyntaxKind::LParen)
                                && has_token(&node, SyntaxKind::RParen),
                        )
                    }
                    _ => {
                        let render = ast::RenderDecl::cast(node.clone());
                        (
                            "render declaration requires a name",
                            render.as_ref().and_then(|decl| decl.name()).is_some(),
                            has_token(&node, SyntaxKind::LParen)
                                && has_token(&node, SyntaxKind::RParen),
                        )
                    }
                };
                if !name_present {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: name_message.to_string(),
                        span: span_for_node(&node),
                    });
                }
                if !has_params {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "declaration requires an explicit parameter list".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            kind if is_function_like_definition(kind) => {
                if matches!(
                    kind,
                    SyntaxKind::FieldDecl
                        | SyntaxKind::RadianceDecl
                        | SyntaxKind::VolumeDecl
                        | SyntaxKind::MaterialDecl
                ) {
                    let (name_message, return_message, name_present, has_return_type) = match kind {
                        SyntaxKind::FieldDecl => {
                            let field = ast::FieldDecl::cast(node.clone());
                            let name_present =
                                field.as_ref().and_then(|field| field.name()).is_some();
                            let has_return_type =
                                field.as_ref().and_then(|field| field.ret_type()).is_some();
                            (
                                "field declaration requires a name",
                                "field requires an explicit return type",
                                name_present,
                                has_return_type,
                            )
                        }
                        SyntaxKind::RadianceDecl => {
                            let radiance = ast::RadianceDecl::cast(node.clone());
                            let name_present =
                                radiance.as_ref().and_then(|decl| decl.name()).is_some();
                            let has_return_type =
                                radiance.as_ref().and_then(|decl| decl.ret_type()).is_some();
                            (
                                "radiance field declaration requires a name",
                                "radiance field requires an explicit return type",
                                name_present,
                                has_return_type,
                            )
                        }
                        SyntaxKind::VolumeDecl => {
                            let volume = ast::VolumeDecl::cast(node.clone());
                            let name_present =
                                volume.as_ref().and_then(|decl| decl.name()).is_some();
                            let has_return_type =
                                volume.as_ref().and_then(|decl| decl.ret_type()).is_some();
                            (
                                "volume field declaration requires a name",
                                "volume field requires an explicit return type",
                                name_present,
                                has_return_type,
                            )
                        }
                        _ => {
                            let material = ast::MaterialDecl::cast(node.clone());
                            let name_present = material
                                .as_ref()
                                .and_then(|material| material.name())
                                .is_some();
                            let has_return_type = material
                                .as_ref()
                                .and_then(|material| material.ret_type())
                                .is_some();
                            (
                                "material declaration requires a name",
                                "material requires an explicit return type",
                                name_present,
                                has_return_type,
                            )
                        }
                    };
                    if !name_present {
                        errors.push(ValidationError {
                            kind: ValidationDiagKind::AstRule,
                            message: name_message.to_string(),
                            span: span_for_node(&node),
                        });
                    }
                    let has_arrow = has_token(&node, SyntaxKind::Arrow);
                    if !has_arrow || !has_return_type {
                        errors.push(ValidationError {
                            kind: ValidationDiagKind::AstRule,
                            message: return_message.to_string(),
                            span: span_for_node(&node),
                        });
                    }
                } else if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "function definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                } else {
                    let has_arrow = has_token(&node, SyntaxKind::Arrow);
                    let has_return_type = node
                        .children()
                        .any(|child| child.kind() == SyntaxKind::TypeRef);
                    if !has_arrow || !has_return_type {
                        errors.push(ValidationError {
                            kind: ValidationDiagKind::AstRule,
                            message: "function requires an explicit return type".to_string(),
                            span: span_for_node(&node),
                        });
                    }
                }
            }
            SyntaxKind::ClassDef
            | SyntaxKind::ResourceDef
            | SyntaxKind::EventDef
            | SyntaxKind::ValueDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "class definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let mut has_must = false;
                let mut has_is_a = false;
                let mut saw_class_item = false;
                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::IsAClause => {
                            if has_is_a {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "only one 'is a' clause is allowed".to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            if saw_class_item {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "'is a' must appear before other class items"
                                        .to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            has_is_a = true;
                        }
                        SyntaxKind::FieldDef
                        | SyntaxKind::MethodDef
                        | SyntaxKind::MustMethodDef
                        | SyntaxKind::PrivateBlock => {
                            saw_class_item = true;
                            if child.kind() == SyntaxKind::MustMethodDef {
                                has_must = true;
                            }
                        }
                        _ => {}
                    }
                }
                if has_must {
                    if has_is_a {
                        errors.push(ValidationError {
                            kind: ValidationDiagKind::AstRule,
                            message: "interfaces cannot declare 'is a'".to_string(),
                            span: span_for_node(&node),
                        });
                    }
                    for child in node.children() {
                        match child.kind() {
                            SyntaxKind::MustMethodDef | SyntaxKind::TypeParamList => {}
                            SyntaxKind::Ident => {}
                            SyntaxKind::FieldDef
                            | SyntaxKind::MethodDef
                            | SyntaxKind::PrivateBlock => {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "interfaces may only contain 'must' method signatures"
                                        .to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                if node.kind() == SyntaxKind::ValueDef {
                    for child in node.children() {
                        match child.kind() {
                            SyntaxKind::FieldDef => {
                                if let Some(field) = ast::FieldDef::cast(child.clone())
                                    && field.is_mutable()
                                {
                                    errors.push(ValidationError {
                                        kind: ValidationDiagKind::AstRule,
                                        message: "value fields cannot be mutable".to_string(),
                                        span: span_for_node(&child),
                                    });
                                }
                            }
                            SyntaxKind::MethodDef | SyntaxKind::MustMethodDef => {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "values may only contain fields".to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            SyntaxKind::IsAClause => {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "values cannot declare 'is a'".to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            SyntaxKind::PrivateBlock => {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "values cannot contain private blocks".to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            SyntaxKind::MethodDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "method definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node
                    .children()
                    .any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "method requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::MustMethodDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "interface method requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node
                    .children()
                    .any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "interface method requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::Param => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "parameter requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::FieldDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "field definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if let Some(field) = ast::FieldDef::cast(node.clone())
                    && let Some(default_expr) = field.default_expr()
                    && !field_default_is_allowed(default_expr)
                {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "field defaults must be literals, lists, or maps".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::TypeRef => {
                if !has_token(&node, SyntaxKind::Ident) && !has_token(&node, SyntaxKind::IntNumber)
                {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "type reference requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::PrivateBlock => {
                let parent_kind = node.parent().map(|p| p.kind());
                if !matches!(
                    parent_kind,
                    Some(
                        SyntaxKind::Root
                            | SyntaxKind::ClassDef
                            | SyntaxKind::ResourceDef
                            | SyntaxKind::EventDef
                    )
                ) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "private blocks are only valid at the top level or in classes"
                            .to_string(),
                        span: span_for_node(&node),
                    });
                } else if parent_kind == Some(SyntaxKind::Root) {
                    for stmt in node.children() {
                        if !matches!(
                            stmt.kind(),
                            SyntaxKind::ClassDef
                                | SyntaxKind::ResourceDef
                                | SyntaxKind::EventDef
                                | SyntaxKind::FuncDef
                                | SyntaxKind::KernelDef
                                | SyntaxKind::FieldDecl
                                | SyntaxKind::RadianceDecl
                                | SyntaxKind::VolumeDecl
                                | SyntaxKind::MaterialDecl
                                | SyntaxKind::RegionDecl
                                | SyntaxKind::DomainDecl
                                | SyntaxKind::RenderDecl
                                | SyntaxKind::SystemDef
                        ) {
                            errors.push(ValidationError {
                                kind: ValidationDiagKind::AstRule,
                                message: "private blocks at the top level may only \
contain functions, fields, radiance/volume fields, materials, regions, domains, renders, and classes"
                                    .to_string(),
                                span: span_for_node(&stmt),
                            });
                        }
                    }
                } else if matches!(
                    parent_kind,
                    Some(SyntaxKind::ClassDef | SyntaxKind::ResourceDef | SyntaxKind::EventDef)
                ) {
                    for child in node.children() {
                        if !matches!(
                            child.kind(),
                            SyntaxKind::FieldDef
                                | SyntaxKind::MethodDef
                                | SyntaxKind::MustMethodDef
                        ) {
                            errors.push(ValidationError {
                                kind: ValidationDiagKind::AstRule,
                                message: "private blocks in classes may only contain field \
definitions or methods"
                                    .to_string(),
                                span: span_for_node(&child),
                            });
                        }
                    }
                }
            }
            SyntaxKind::ReturnStmt => {
                if !is_in_function(&node) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "return is only valid inside functions".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::BreakStmt => {
                if !is_in_loop(&node) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "break is only valid inside loops".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::ContinueStmt => {
                if !is_in_loop(&node) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "continue is only valid inside loops".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::UseStmt => {
                if node.parent().map(|p| p.kind()) != Some(SyntaxKind::Root) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "use is only valid at the top level".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let mut in_module = false;
                let mut name_count = 0usize;
                let mut module_count = 0usize;
                let mut saw_glob = false;
                let mut saw_named = false;
                for token in node.children_with_tokens().filter_map(|it| it.into_token()) {
                    match token.kind() {
                        SyntaxKind::FromKw => in_module = true,
                        SyntaxKind::Ident => {
                            if in_module {
                                module_count += 1;
                            } else {
                                name_count += 1;
                                saw_named = true;
                            }
                        }
                        SyntaxKind::Star => {
                            if !in_module {
                                saw_glob = true;
                            }
                        }
                        _ => {}
                    }
                }
                if name_count == 0 && !saw_glob {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "use requires at least one name or '*'".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if module_count == 0 {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "use requires a module path".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if saw_glob && saw_named {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "use '*' cannot be combined with named imports".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            _ => {}
        }

        if node.kind() == SyntaxKind::PrefixExpr
            && (has_token(&node, SyntaxKind::DetachKw) || has_token(&node, SyntaxKind::SpawnKw))
        {
            validate_detach_tail(&node, &mut errors);
        }
    }
    errors
}

fn validate_detach_tail(node: &SyntaxNode, errors: &mut Vec<ValidationError>) {
    let mut iter = node.children_with_tokens().peekable();

    while let Some(child) = iter.next() {
        match child {
            rowan::NodeOrToken::Node(_) => {}
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::Star => {
                    let mut size_token = None;
                    for next in iter.by_ref() {
                        if let Some(tok) = next.into_token() {
                            if tok.kind().is_trivia() {
                                continue;
                            }
                            size_token = Some(tok);
                            break;
                        }
                    }
                    if let Some(tok) = size_token {
                        match tok.kind() {
                            SyntaxKind::IntNumber => {}
                            SyntaxKind::Ident => {
                                if tok.text() != "n" {
                                    errors.push(ValidationError {
                                        kind: ValidationDiagKind::AstRule,
                                        message: "pool size must be an integer literal or 'n'"
                                            .to_string(),
                                        span: span_for_token(&tok),
                                    });
                                }
                            }
                            _ => {
                                errors.push(ValidationError {
                                    kind: ValidationDiagKind::AstRule,
                                    message: "pool size must be an integer literal or 'n'"
                                        .to_string(),
                                    span: span_for_token(&tok),
                                });
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

fn field_default_is_allowed(expr: ast::Expr) -> bool {
    match expr {
        ast::Expr::Literal(_) => true,
        ast::Expr::List(list) => list.items().all(field_default_is_allowed),
        ast::Expr::Map(map) => {
            let mut items = map.items();
            while let Some(key) = items.next() {
                let Some(value) = items.next() else {
                    return false;
                };
                if !field_default_is_allowed(key) || !field_default_is_allowed(value) {
                    return false;
                }
            }
            true
        }
        ast::Expr::Paren(p) => {
            let expr = p.syntax().children().filter_map(ast::Expr::cast).next();
            expr.map(field_default_is_allowed).unwrap_or(false)
        }
        _ => false,
    }
}

fn is_in_function(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| is_function_like_body_container(ancestor.kind()))
}

fn is_in_loop(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| matches!(ancestor.kind(), SyntaxKind::WhileStmt | SyntaxKind::ForStmt))
}

fn span_for_node(node: &SyntaxNode) -> SourceSpan {
    let range = node.text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    SourceSpan::new(start.into(), end - start)
}

fn span_for_token(token: &rowan::SyntaxToken<crate::parser::WrelaLanguage>) -> SourceSpan {
    let range = token.text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();
    SourceSpan::new(start.into(), end - start)
}

fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .any(|token| token.kind() == kind)
}

fn is_function_like_definition(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FuncDef
            | SyntaxKind::KernelDef
            | SyntaxKind::SystemDef
            | SyntaxKind::FieldDecl
            | SyntaxKind::RadianceDecl
            | SyntaxKind::VolumeDecl
            | SyntaxKind::MaterialDecl
    )
}

fn is_function_like_body_container(kind: SyntaxKind) -> bool {
    is_function_like_definition(kind) || kind == SyntaxKind::MethodDef
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_self_expr_is_allowed() {
        let text = "\
fn f() -> Integer {
    return self.value
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_requires_default() {
        let text = "\
fn f(x: Integer) -> Integer {
    match x {
        1 { y = 1 }
        default { y = 2 }
    }
    return 0
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_break_continue_outside_loop() {
        let text = "break\ncontinue\n";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "break is only valid inside loops")
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message == "continue is only valid inside loops")
        );
    }

    #[test]
    fn test_break_continue_inside_loop() {
        let text = "\
while true {
    break
    continue
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_return_outside_function() {
        let text = "return 1\n";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "return is only valid inside functions")
        );
    }

    #[test]
    fn test_return_inside_function_is_allowed() {
        let text = "\
fn shade() -> String {
    return \"wgsl\"
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            !errors
                .iter()
                .any(|e| e.message == "return is only valid inside functions")
        );
    }

    #[test]
    fn test_return_inside_field_declaration_is_allowed() {
        let text = "\
field exact distance sphere(p: F32) -> F32 {
    return p
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            !errors
                .iter()
                .any(|e| e.message == "return is only valid inside functions")
        );
    }

    #[test]
    fn test_return_inside_material_declaration_is_allowed() {
        let text = "\
material surface(hit: Hit3) -> Surface {
    return hit
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            !errors
                .iter()
                .any(|e| e.message == "return is only valid inside functions")
        );
    }

    #[test]
    fn test_return_type_required() {
        let text = "\
fn f() {
    return 1
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "function requires an explicit return type")
        );
    }

    #[test]
    fn test_field_declaration_requires_name_and_return_type() {
        let text = "\
field exact distance (p: F32) {
    return p
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "field declaration requires a name"),
            "expected field name validation error, got: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message == "field requires an explicit return type"),
            "expected field return-type validation error, got: {errors:?}"
        );
    }

    #[test]
    fn test_material_declaration_requires_name_and_return_type() {
        let text = "\
material (hit: Hit3) {
    return hit
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "material declaration requires a name"),
            "expected material name validation error, got: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message == "material requires an explicit return type"),
            "expected material return-type validation error, got: {errors:?}"
        );
    }

    #[test]
    fn test_region_domain_and_render_declarations_require_names_and_parameter_lists() {
        let text = "\
region (band: I32) {
    place stairs = StairBand(index = band)
}
domain Combat {
    geometry_detail = coarse
}
render (world: Capture[StaircaseWorld]) {
    domain = Presentation(world = world, camera = camera)
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "region declaration requires a name"),
            "expected region name validation error, got: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message == "render declaration requires a name"),
            "expected render name validation error, got: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message == "declaration requires an explicit parameter list"),
            "expected parameter-list validation error, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_default_literals_ok() {
        let text = r#"
class Defaults {
    name: String = "ok"
    count: Integer = 3
    flags: List = [true, false]
    meta: Map = {"a": 1}
}
"#;
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_field_default_rejects_expr() {
        let text = r#"
class Bad {
    value: Integer = 1 + 2
}
"#;
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "field defaults must be literals, lists, or maps")
        );
    }

    #[test]
    fn test_private_block_top_level_restricts_members() {
        let text = "\
private {
    fn f() -> Integer {
        return 1
    }
    mutable x = 1
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("private blocks at the top level"))
        );
    }

    #[test]
    fn test_private_block_class_restricts_members() {
        let text = "\
class Foo {
    private {
        fn f() -> Integer {
            return 1
        }
    }
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_private_block_top_level_allows_kernel_functions() {
        let text = "\
private {
    kernel fn shade() -> Nothing {
        return nothing
    }
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_private_block_top_level_allows_material_declarations() {
        let text = "\
private {
    material surface(hit: Hit3) -> Surface {
        return hit
    }
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_private_block_top_level_allows_radiance_and_volume_declarations() {
        let text = "\
private {
    radiance field emit_sky(direction: Vec3) -> Vec3 {
        return direction
    }
    volume field accumulate_fog(p: Vec3) -> Medium {
        return Medium(density=0.1, emission=vec3(0.0, 0.0, 0.0), anisotropy=0.0)
    }
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_kernel_function_definition_validates_like_other_functions() {
        let text = "\
kernel fn shade() -> Integer {
    return 1
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_private_block_in_class_allows_fields() {
        let text = "\
class Foo {
    name: String
    private {
        secret: String
    }
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_use_requires_name_and_module() {
        let text = "use from core\n";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "use requires at least one name or '*'")
        );
    }

    #[test]
    fn test_missing_function_name() {
        let text = "\
fn () -> Integer {
    return 1
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "function definition requires a name")
        );
    }

    #[test]
    fn test_invalid_numeric_literal() {
        let text = "\
fn f() -> Integer {
    return 1e
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "invalid numeric literal")
        );
    }

    #[test]
    fn test_interface_must_method_signature_valid() {
        let text = "\
interface Predicate {
    must ready(value: Integer) -> Boolean
}
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty(), "{errors:?}");
    }
}
