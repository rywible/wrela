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
    for element in root.descendants_with_tokens() {
        if let Some(token) = element.into_token()
            && (token.kind() == SyntaxKind::PublicKw || token.kind() == SyntaxKind::PrivateKw)
            && let Some(parent) = token.parent()
            && !is_allowed_visibility_parent(parent.kind())
        {
            errors.push(ValidationError {
                message: "visibility modifier is not valid here".to_string(),
                span: span_for_token(&token),
            });
        }
    }
    for node in root.descendants() {
        match node.kind() {
            SyntaxKind::ItExpr => {
                if !is_in_return(&node) {
                    errors.push(ValidationError {
                        message: "it is only valid in return statements".to_string(),
                        span: span_for_node(&node),
                    });
                }
            }
            SyntaxKind::VarAssign => {
                if has_token(&node, SyntaxKind::PublicKw)
                    && has_token(&node, SyntaxKind::ChangingKw)
                {
                    errors.push(ValidationError {
                        message: "public variables cannot be changing".to_string(),
                        span: span_for_node(&node),
                    });
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
    }
    errors
}

fn is_in_return(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| ancestor.kind() == SyntaxKind::ReturnStmt)
}

fn is_in_function(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| matches!(ancestor.kind(), SyntaxKind::FuncDef | SyntaxKind::MethodDef))
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

fn is_allowed_visibility_parent(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ClassDef
            | SyntaxKind::FuncDef
            | SyntaxKind::MethodDef
            | SyntaxKind::FieldDef
            | SyntaxKind::HasBlock
            | SyntaxKind::VarAssign
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_it_only_in_return() {
        let text = "\
it
to f():
    return it
";
        let root = parse(text);
        let errors = validate(&root);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "it is only valid in return statements");
    }

    #[test]
    fn test_it_inside_return_expr() {
        let text = "\
to f():
    return it + 1
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_public_changing_error() {
        let text = "public changing x = 1\n";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "public variables cannot be changing")
        );
    }

    #[test]
    fn test_match_requires_otherwise() {
        let text = "\
match x:
    1: return it
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
        let text = "return it\n";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "return is only valid inside functions")
        );
    }

    #[test]
    fn test_visibility_misuse() {
        let text = "\
public if true:
    break
";
        let root = parse(text);
        let errors = validate(&root);
        assert!(
            errors
                .iter()
                .any(|e| e.message == "visibility modifier is not valid here")
        );
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
}
