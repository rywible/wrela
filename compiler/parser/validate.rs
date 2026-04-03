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
            SyntaxKind::FuncDef | SyntaxKind::KernelDef | SyntaxKind::SystemDef => {
                if !has_token(&node, SyntaxKind::Ident) {
                    errors.push(ValidationError {
                        kind: ValidationDiagKind::AstRule,
                        message: "function definition requires a name".to_string(),
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
                        message: "function requires an explicit return type".to_string(),
                        span: span_for_node(&node),
                    });
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
                    for child in node.children() {
                        if child.kind() == SyntaxKind::Block {
                            for stmt in child.children() {
                                if !matches!(
                                    stmt.kind(),
                                    SyntaxKind::ClassDef
                                        | SyntaxKind::ResourceDef
                                        | SyntaxKind::EventDef
                                        | SyntaxKind::FuncDef
                                        | SyntaxKind::KernelDef
                                        | SyntaxKind::SystemDef
                                ) {
                                    errors.push(ValidationError {
                                        kind: ValidationDiagKind::AstRule,
                                        message: "private blocks at the top level may only \
contain functions and classes"
                                            .to_string(),
                                        span: span_for_node(&stmt),
                                    });
                                }
                            }
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
    node.ancestors().any(|ancestor| {
        matches!(
            ancestor.kind(),
            SyntaxKind::FuncDef
                | SyntaxKind::KernelDef
                | SyntaxKind::SystemDef
                | SyntaxKind::MethodDef
        )
    })
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
