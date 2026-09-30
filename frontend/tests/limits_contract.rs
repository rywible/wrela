use wrela_frontend::{
    diagnostic::DiagnosticCode,
    lexer::{Limits, lex},
    parse_with_limits,
    source::PieceKind,
};
fn limits() -> Limits {
    Limits {
        max_source_bytes: 1024,
        max_depth: 16,
        max_generic_tokens: 64,
        max_diagnostics: 8,
    }
}
fn independent_render(source: &wrela_frontend::source::Source) -> Vec<u8> {
    source
        .pieces
        .iter()
        .flat_map(|p| p.bytes.iter().copied())
        .collect()
}
#[test]
fn source_byte_budget_is_inclusive_and_rejection_retains_every_byte() {
    let limits = Limits {
        max_source_bytes: 4,
        ..limits()
    };
    for size in [3, 4, 5] {
        let bytes = vec![b' '; size];
        let parsed = parse_with_limits(&bytes, limits);
        assert_eq!(independent_render(&parsed.source), bytes);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::Limit),
            size > 4
        );
        assert_eq!(parsed.admit().is_ok(), size <= 4);
    }
}
#[test]
fn delimiter_and_nested_comment_depth_budgets_are_inclusive() {
    let limits = Limits {
        max_depth: 3,
        ..limits()
    };
    for depth in [2, 3, 4] {
        for text in [
            format!("{}x{}", "(".repeat(depth), ")".repeat(depth)),
            format!("{}x{}", "/*".repeat(depth), "*/".repeat(depth)),
        ] {
            let lexed = lex(text.as_bytes(), limits);
            assert_eq!(independent_render(&lexed.source), text.as_bytes());
            assert_eq!(
                lexed
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Limit),
                depth > 3,
                "{text}"
            );
        }
    }
}
#[test]
fn prospective_generic_budget_fails_without_reinterpretation_or_source_loss() {
    let text = b"fn run() -> Unit { f<A>(x) }";
    for budget in [2, 3, 4] {
        let parsed = parse_with_limits(
            text,
            Limits {
                max_generic_tokens: budget,
                ..limits()
            },
        );
        assert_eq!(independent_render(&parsed.source), text);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::Limit),
            budget < 3,
            "budget {budget}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.admit().is_ok(), budget >= 3);
    }
    let lexed = lex(
        text,
        Limits {
            max_generic_tokens: 2,
            ..limits()
        },
    );
    assert!(
        lexed
            .source
            .pieces
            .iter()
            .any(|p| p.kind == PieceKind::Unparsed)
    );
}
#[test]
fn diagnostic_budget_truncates_deterministically_without_dropping_source() {
    let limits = Limits {
        max_diagnostics: 3,
        ..limits()
    };
    for count in [2, 3, 4] {
        let bytes = vec![b'@'; count];
        let first = lex(&bytes, limits);
        let second = lex(&bytes, limits);
        assert_eq!(first.diagnostics, second.diagnostics);
        assert_eq!(first.source, second.source);
        assert_eq!(independent_render(&first.source), bytes);
        assert_eq!(first.diagnostics.len(), count.min(3));
        assert_eq!(
            first
                .diagnostics
                .last()
                .is_some_and(|d| d.code == DiagnosticCode::Truncated),
            count > 3
        );
        assert!(parse_with_limits(&bytes, limits).admit().is_err());
    }
}
