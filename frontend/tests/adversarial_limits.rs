use wrela_frontend::{diagnostic::DiagnosticCode, lexer::Limits, parse, parse_with_limits};

#[test]
fn long_recursive_shapes_fail_before_parser_tree_construction_and_keep_all_bytes() {
    let expressions = [
        format!("return {}x", "-".repeat(10_000)),
        format!("f{}", "()".repeat(10_000)),
        format!("if x {{}}{}", " else if x {}".repeat(10_000)),
        format!("f<{}T{}(x)", "Box<".repeat(10_000), ">".repeat(10_001)),
    ];
    for expression in expressions {
        let source = format!("fn f() -> Unit {{\n{expression}\nreturn\n}}\n// trailing 🚀\r\n  ");
        let parsed = parse(source.as_bytes());
        assert_eq!(parsed.source.render(), source.as_bytes());
        assert!(parsed.source.validate());
        assert!(!parsed.is_syntax_eligible());
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::Limit)
        );
        assert!(parsed.diagnostics.len() <= Limits::default().max_diagnostics);
        for diagnostic in &parsed.diagnostics {
            assert!(diagnostic.range.start <= diagnostic.range.end);
            assert!(diagnostic.range.end <= source.len());
            assert!(source.is_char_boundary(diagnostic.range.start));
            assert!(source.is_char_boundary(diagnostic.range.end));
        }
        assert!(parsed.admit().is_err());
    }
}

#[test]
fn structural_chain_budget_accepts_its_boundary_and_rejects_the_next_level() {
    let source = b"fn f() -> Unit { return ---x }\r\n// end";
    let accepted = parse_with_limits(
        source,
        Limits {
            max_depth: 5,
            ..Limits::default()
        },
    );
    assert!(accepted.is_syntax_eligible(), "{:?}", accepted.diagnostics);
    let rejected = parse_with_limits(
        source,
        Limits {
            max_depth: 4,
            ..Limits::default()
        },
    );
    assert_eq!(rejected.source.render(), source);
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::Limit)
    );
    assert!(rejected.admit().is_err());
}
