//! Broad flat lists protect observable source order without imposing a speed quota.
use wrela_frontend::{parse, syntax::*};

#[test]
fn broad_statement_and_declaration_lists_preserve_all_markers_and_separators() {
    let mut text = String::from("\n;fn first() -> Unit {\n;");
    for index in 0..4096 {
        text.push_str(&format!("{index};\n"));
    }
    text.push_str("return\n}\n;fn second() -> Unit { return };\nenum Last { A, B(), }\n;");
    let parsed = parse(text.as_bytes());
    assert!(parsed.is_syntax_eligible(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.source.render(), text.as_bytes());
    assert_eq!(parsed.syntax.declarations.len(), 3);
    let Declaration::Function(first) = &parsed.syntax.declarations[0].kind else {
        panic!("first declaration changed kind");
    };
    assert_eq!(parsed.source.token_text(first.name), Some("first"));
    assert_eq!(first.body.statements.len(), 4097);
    for (index, statement) in first.body.statements[..4096].iter().enumerate() {
        let StatementKind::Expression(expression) = &statement.kind else {
            panic!("statement {index} changed kind");
        };
        let ExprKind::Number(token) = expression.kind else {
            panic!("statement {index} changed value");
        };
        assert_eq!(
            parsed.source.token_text(token),
            Some(index.to_string().as_str())
        );
    }
    assert!(matches!(
        first.body.statements[4096].kind,
        StatementKind::Return { value: None, .. }
    ));
    assert!(
        first
            .body
            .separators
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
    );
    assert!(
        parsed
            .syntax
            .separators
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
    );
    let Declaration::Function(second) = &parsed.syntax.declarations[1].kind else {
        panic!("second declaration changed kind");
    };
    assert_eq!(parsed.source.token_text(second.name), Some("second"));
    let Declaration::Enum(last) = &parsed.syntax.declarations[2].kind else {
        panic!("last declaration changed kind");
    };
    assert!(last.variants.contents.items[0].kind.payload.is_none());
    assert!(last.variants.contents.items[1].kind.payload.is_some());
    assert_eq!(last.variants.contents.separators.len(), 2);
}

#[test]
fn broad_argument_list_preserves_order_and_optional_trailing_comma() {
    for trailing in [false, true] {
        let arguments = (0..2048)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",\n");
        let text = format!(
            "fn arguments() -> Unit {{\n consume({arguments}{})?\n return\n}}",
            if trailing { ",\n" } else { "" }
        );
        let parsed = parse(text.as_bytes());
        assert!(parsed.is_syntax_eligible(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.source.render(), text.as_bytes());
        let Declaration::Function(function) = &parsed.syntax.declarations[0].kind else {
            panic!("function changed kind");
        };
        let StatementKind::Expression(expression) = &function.body.statements[0].kind else {
            panic!("expression changed kind");
        };
        let ExprKind::Propagate { operand, .. } = &expression.kind else {
            panic!("propagation marker lost");
        };
        let ExprKind::Call { arguments, .. } = &operand.kind else {
            panic!("call changed kind");
        };
        assert_eq!(arguments.contents.items.len(), 2048);
        assert_eq!(
            arguments.contents.separators.len(),
            if trailing { 2048 } else { 2047 }
        );
        assert!(
            arguments
                .contents
                .separators
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
        for (index, argument) in arguments.contents.items.iter().enumerate() {
            let ExprKind::Number(token) = argument.kind.value.kind else {
                panic!("argument {index} changed value");
            };
            assert_eq!(
                parsed.source.token_text(token),
                Some(index.to_string().as_str())
            );
        }
    }
}
