use crate::parser::ast;
use crate::parser::ast::AstNode;
use crate::parser::SyntaxKind;
use crate::parser::SyntaxNode;
use miette::SourceSpan;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub span: SourceSpan,
}

pub fn validate(root: &SyntaxNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let objectives = ["latency", "throughput", "conservation", "balance"];
    for element in root.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind() == SyntaxKind::InvalidLiteral {
            errors.push(ValidationError {
                message: "invalid numeric literal".to_string(),
                span: span_for_token(&token),
            });
        }
    }
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::FuncDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "function definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node.children().any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        message: "function requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::ClassDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
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
                                    message: "only one 'is a' clause is allowed".to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            if saw_class_item {
                                errors.push(ValidationError {
                                    message: "'is a' must appear before other class items"
                                        .to_string(),
                                    span: span_for_node(&child),
                                });
                            }
                            has_is_a = true;
                        }
                        SyntaxKind::HasBlock
                        | SyntaxKind::MethodDef
                        | SyntaxKind::MustMethodDef
                        | SyntaxKind::DeriveDef
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
                            message: "interfaces cannot declare 'is a'".to_string(),
                            span: span_for_node(&node),
                        });
                    }
                    for child in node.children() {
                        match child.kind() {
                            SyntaxKind::MustMethodDef | SyntaxKind::TypeParamList => {}
                            SyntaxKind::Ident => {}
                            SyntaxKind::HasBlock
                            | SyntaxKind::MethodDef
                            | SyntaxKind::DeriveDef
                            | SyntaxKind::PrivateBlock => {
                                errors.push(ValidationError {
                                    message:
                                        "interfaces may only contain 'must' method signatures"
                                            .to_string(),
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
                        message: "method definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node.children().any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        message: "method requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::MustMethodDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "interface method requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node.children().any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        message: "interface method requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::DeriveDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "derived definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                let has_arrow = has_token(&node, SyntaxKind::Arrow);
                let has_return_type = node.children().any(|child| child.kind() == SyntaxKind::TypeRef);
                if !has_arrow || !has_return_type {
                    errors.push(ValidationError {
                        message: "derived definition requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::Param => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "parameter requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::FieldDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "field definition requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if let Some(field) = ast::FieldDef::cast(node.clone()) {
                    if let Some(default_expr) = field.default_expr() {
                        if !field_default_is_allowed(default_expr) {
                            errors.push(ValidationError {
                                message: "field defaults must be literals, lists, or maps"
                                    .to_string(),
                                span: span_for_node(&node),
                            });
                        }
                    }
                }
            }
            SyntaxKind::TypeRef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        message: "type reference requires a name".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::ItExpr => {
                if !is_in_return(&node) {
                    errors.push(ValidationError {
                        message: "it is only valid in return statements".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::PrivateBlock => {
                let parent_kind = node.parent().map(|p| p.kind());
                if !matches!(
                    parent_kind,
                    Some(SyntaxKind::Root | SyntaxKind::ClassDef | SyntaxKind::HasBlock)
                ) {
                    errors.push(ValidationError {
                        message: "private blocks are only valid at the top level, in classes, \
or inside 'has' blocks"
                            .to_string(),
                        span: span_for_node(&node),
                    });
                } else if parent_kind == Some(SyntaxKind::Root) {
                    for child in node.children() {
                        if child.kind() == SyntaxKind::Block {
                            for stmt in child.children() {
                                if !matches!(
                                    stmt.kind(),
                                    SyntaxKind::ClassDef | SyntaxKind::FuncDef
                                ) {
                                    errors.push(ValidationError {
                                        message: "private blocks at the top level may only \
contain functions and classes"
                                            .to_string(),
                                        span: span_for_node(&stmt),
                                    });
                                }
                            }
                        }
                    }
                } else if parent_kind == Some(SyntaxKind::ClassDef) {
                    for child in node.children() {
                        if !matches!(
                            child.kind(),
                            SyntaxKind::HasBlock | SyntaxKind::MethodDef | SyntaxKind::DeriveDef
                        ) {
                            errors.push(ValidationError {
                                message: "private blocks in classes may only contain 'has' \
blocks, methods, or derives"
                                    .to_string(),
                                span: span_for_node(&child),
                            });
                        }
                    }
                } else if parent_kind == Some(SyntaxKind::HasBlock) {
                    let mut saw_block = false;
                    let mut saw_child = false;
                    for child in node.children() {
                        saw_child = true;
                        if child.kind() == SyntaxKind::Block {
                            saw_block = true;
                            for stmt in child.children() {
                                if stmt.kind() != SyntaxKind::FieldDef {
                                    errors.push(ValidationError {
                                        message: "private blocks inside 'has' may only contain \
field definitions"
                                            .to_string(),
                                        span: span_for_node(&stmt),
                                    });
                                }
                            }
                        } else if child.kind() != SyntaxKind::FieldDef {
                            errors.push(ValidationError {
                                message: "private blocks inside 'has' may only contain field \
definitions"
                                    .to_string(),
                                span: span_for_node(&child),
                            });
                        }
                    }
                    if !saw_block && !saw_child {
                        errors.push(ValidationError {
                            message: "private blocks inside 'has' may only contain field \
definitions"
                                .to_string(),
                            span: span_for_node(&node),
                        });
                    }
                }
            }
            SyntaxKind::MatchStmt => {
                if !node
                    .children()
                    .any(|child| child.kind() == SyntaxKind::OtherwiseCase)
                {
                    errors.push(ValidationError {
                        message: "match requires an otherwise case".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::OptimizeStmt => {
                let obj_token = node
                    .children_with_tokens()
                    .filter_map(|it| it.into_token())
                    .find(|it| it.kind() == SyntaxKind::Ident);
                if let Some(token) = obj_token {
                    if !objectives.contains(&token.text()) {
                        errors.push(ValidationError {
                            message: "invalid optimize objective".to_string(),
                            span: span_for_token(&token),
                        });
                    }
                }
            }
            SyntaxKind::ReturnStmt => {
                if !is_in_function(&node) {
                    errors.push(ValidationError {
                        message: "return is only valid inside functions".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::BreakStmt => {
                if !is_in_loop(&node) {
                    errors.push(ValidationError {
                        message: "break is only valid inside loops".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::ContinueStmt => {
                if !is_in_loop(&node) {
                    errors.push(ValidationError {
                        message: "continue is only valid inside loops".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::UseStmt => {
                if node.parent().map(|p| p.kind()) != Some(SyntaxKind::Root) {
                    errors.push(ValidationError {
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
                        message: "use requires at least one name or '*'".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if module_count == 0 {
                    errors.push(ValidationError {
                        message: "use requires a module path".to_string(),
                        span: span_for_node(&node),
                    });
                }
                if saw_glob && saw_named {
                    errors.push(ValidationError {
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
            validate_detach_tail(&node, &objectives, &mut errors);
        }
    }
    errors
}

fn validate_detach_tail(node: &SyntaxNode, objectives: &[&str], errors: &mut Vec<ValidationError>) {
    let mut iter = node.children_with_tokens().peekable();

    while let Some(child) = iter.next() {
        match child {
            rowan::NodeOrToken::Node(_) => {}
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::Star => {
                    let mut size_token = None;
                    while let Some(next) = iter.next() {
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
                                        message: "pool size must be an integer literal or 'n'"
                                            .to_string(),
                                        span: span_for_token(&tok),
                                    });
                                }
                            }
                            _ => {
                                errors.push(ValidationError {
                                    message: "pool size must be an integer literal or 'n'"
                                        .to_string(),
                                    span: span_for_token(&tok),
                                });
                            }
                        }
                    }
                }
                SyntaxKind::OptimizeKw => {
                    let mut obj_token = None;
                    while let Some(next) = iter.next() {
                        if let Some(tok) = next.into_token() {
                            if tok.kind().is_trivia() {
                                continue;
                            }
                            obj_token = Some(tok);
                            break;
                        }
                    }
                    if let Some(tok) = obj_token {
                        if tok.kind() == SyntaxKind::Ident && !objectives.contains(&tok.text()) {
                            errors.push(ValidationError {
                                message: "invalid optimize objective".to_string(),
                                span: span_for_token(&tok),
                            });
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

fn is_in_return(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| ancestor.kind() == SyntaxKind::ReturnStmt)
}

fn field_default_is_allowed(expr: ast::Expr) -> bool {
    match expr {
        ast::Expr::Literal(_) => true,
        ast::Expr::List(list) => list.items().all(field_default_is_allowed),
        ast::Expr::Map(map) => {
            let mut items = map.items();
            while let Some(key) = items.next() {
                let Some(value) = items.next() else { return false };
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
        .any(|ancestor| matches!(ancestor.kind(), SyntaxKind::FuncDef | SyntaxKind::MethodDef | SyntaxKind::DeriveDef))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_it_only_in_return() {
        let text = "\
it
to f() -> Int:
    return 1
";
        let root = parse(text);
        let errors = validate(&root);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "it is only valid in return statements");
    }

    #[test]
    fn test_it_inside_return_expr() {
        let text = "\
to f() -> Int:
    return 1
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_requires_otherwise() {
        let text = "\
match x:
    1: return 1
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "match requires an otherwise case")
        );
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
while true:
    break
    continue
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
    fn test_return_type_required() {
        let text = "\
to f():
    return 1
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
    fn test_field_default_literals_ok() {
        let text = r#"
A Defaults:
    has:
        name: String = "ok"
        count: Int = 3
        flags: List = [true, false]
        meta: Map = {"a": 1}
"#;
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_field_default_rejects_expr() {
        let text = r#"
A Bad:
    has:
        value: Int = 1 + 2
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
private:
    to f() -> Int:
        return 1
    mutable x = 1
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
A Foo:
    private:
        to f() -> Int:
            return 1
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("private blocks in classes"))
        );
    }

    #[test]
    fn test_private_block_in_has_allows_fields() {
        let text = "\
A Foo:
    has:
        name: String
        private:
            secret: String
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
to () -> Int:
    return 1
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
to f() -> Int:
    return 1e
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "invalid numeric literal")
        );
    }
}
