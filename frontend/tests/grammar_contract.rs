use wrela_frontend::{
    diagnostic::DiagnosticCode,
    lexer::{Limits, lex},
    parse, parse_with_limits,
    source::{ByteRange, PieceKind},
    syntax::{Declaration, TypeKind},
    token::TokenKind,
};

#[test]
fn shared_array_rule_reaches_generic_selection() {
    let scanned = lex(b"f<[T]>(x)", Limits::default());
    assert!(scanned.diagnostics.is_empty());
    let open = scanned
        .source
        .pieces
        .iter()
        .find(|piece| piece.bytes == b"<")
        .unwrap();
    assert_eq!(open.kind, PieceKind::Token(TokenKind::GenericOpen));
}

#[test]
fn shared_array_rule_reaches_typed_parsing() {
    let document = parse(b"fn f(x:[T])->Unit{return}");
    assert!(document.is_syntax_eligible(), "{:?}", document.diagnostics);
    let Declaration::Function(function) = &document.syntax.declarations[0].kind else {
        panic!("expected a typed function declaration");
    };
    assert_eq!(function.parameters.contents.items.len(), 1);
    let annotation = function.parameters.contents.items[0]
        .kind
        .annotation
        .as_ref()
        .unwrap();
    let TypeKind::Array(array) = &annotation.ty.kind else {
        panic!("expected an array parameter type");
    };
    let TypeKind::Named {
        path,
        arguments: None,
    } = &array.contents.kind
    else {
        panic!("expected a named array element type");
    };
    assert_eq!(path.segments.len(), 1);
    assert_eq!(document.source.token_text(path.segments[0]), Some("T"));
}

#[test]
fn authored_arrow_policy_controls_exported_type_punctuation() {
    let scanned = lex(b"fn f()->Box<T>{return}", Limits::default());
    assert!(scanned.diagnostics.is_empty());
    let open = scanned
        .source
        .pieces
        .iter()
        .find(|piece| piece.bytes == b"<")
        .unwrap();
    let close = scanned
        .source
        .pieces
        .iter()
        .find(|piece| piece.bytes == b">")
        .unwrap();
    assert_eq!(open.kind, PieceKind::Token(TokenKind::TypeOpen));
    assert_eq!(close.kind, PieceKind::Token(TokenKind::TypeClose));
}

#[test]
fn authored_arrow_policy_controls_typed_function_result() {
    let document = parse(b"fn f()->Box<T>{return}");
    assert!(document.is_syntax_eligible(), "{:?}", document.diagnostics);
    let Declaration::Function(function) = &document.syntax.declarations[0].kind else {
        panic!("expected a typed function declaration");
    };
    let TypeKind::Named {
        path,
        arguments: Some(arguments),
    } = &function.result.ty.kind
    else {
        panic!("expected a parameterized named result type");
    };
    assert_eq!(path.segments.len(), 1);
    assert_eq!(document.source.token_text(path.segments[0]), Some("Box"));
    assert_eq!(arguments.contents.items.len(), 1);
    let TypeKind::Named {
        path,
        arguments: None,
    } = &arguments.contents.items[0].kind
    else {
        panic!("expected a named result type argument");
    };
    assert_eq!(path.segments.len(), 1);
    assert_eq!(document.source.token_text(path.segments[0]), Some("T"));
}

#[test]
fn nested_type_forms_agree_in_declarations_results_and_generic_calls() {
    for ty in [
        "Ns::Item<Box<(T,[U])>>",
        "[(T, [U], Ns::V<X>)]",
        "()",
        "(T,)",
        "(T,U,)",
        "Box<T,>",
        "(T)",
        "[T]",
    ] {
        let text = format!("fn f(x: {ty}) -> {ty} {{ return g<{ty}>(x) }}");
        let document = parse(text.as_bytes());
        assert!(
            document.is_syntax_eligible(),
            "{ty}: {:?}",
            document.diagnostics
        );
        assert_eq!(document.source.render(), text.as_bytes());
        assert!(document.admit().is_ok());
    }
    for ty in ["Box<>", "[T,U]", "(T U)", "fn(T)->U"] {
        let text = format!("fn f(x: {ty}) -> Unit {{ return }}");
        let document = parse(text.as_bytes());
        assert!(
            !document.is_syntax_eligible(),
            "accepted malformed type {ty}"
        );
        assert_eq!(document.source.render(), text.as_bytes());
        assert!(document.admit().is_err());
    }
}

#[test]
fn malformed_candidates_stop_recognition_before_the_remaining_input() {
    for (candidate, first_sufficient_budget) in
        [("f<[T U U U U U]>(x)", 4), ("f<Box<T,,U U U U>>(x)", 5)]
    {
        let text = format!("fn f() -> Unit {{ return {candidate} }}");
        for budget in [first_sufficient_budget - 1, first_sufficient_budget] {
            let document = parse_with_limits(
                text.as_bytes(),
                Limits {
                    max_generic_tokens: budget,
                    ..Limits::default()
                },
            );
            assert_eq!(document.source.render(), text.as_bytes());
            assert_eq!(
                document
                    .diagnostics
                    .iter()
                    .any(|d| d.code == DiagnosticCode::Limit),
                budget < first_sufficient_budget,
                "{candidate}, budget {budget}: {:?}",
                document.diagnostics
            );
        }
    }
    // `fn` cannot begin a type. No candidate token budget should be spent.
    let text = b"fn f() -> Unit { return f<fn T T T T T T>(x) }";
    let document = parse_with_limits(
        text,
        Limits {
            max_generic_tokens: 1,
            ..Limits::default()
        },
    );
    assert!(
        !document
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::Limit)
    );
    assert_eq!(document.source.render(), text);
}

#[test]
fn non_call_angles_preserve_the_primary_error_and_later_recovery() {
    let text = b"fn f() -> Unit { return f<T>\n(x) }";
    let document = parse(text);
    let diagnostic = document
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::Syntax)
        .unwrap();
    assert_eq!(diagnostic.range, ByteRange::new(27, 28));
    assert!(
        diagnostic.message.starts_with("unexpected Greater;"),
        "{diagnostic:?}"
    );
    assert_eq!(document.diagnostics.len(), 1);
    assert_eq!(document.source.render(), text);
    assert_eq!(document.syntax.declarations.len(), 1);
    for text in ["(f\n<T>(x))", "(f<T>\n(x))"] {
        let text = format!("fn f() -> Unit {{ return {text} }}");
        assert!(parse(text.as_bytes()).is_syntax_eligible(), "{text}");
    }
}

#[test]
fn rejected_declaration_prefixes_preserve_depth_budgets_and_recovery() {
    for (text, depth, primary) in [
        (
            "fn f() -> Unit { return x: Box<T> + z }",
            3,
            ByteRange::new(25, 26),
        ),
        (
            "fn f() -> Unit { return x -> Box<T> + z }",
            3,
            ByteRange::new(26, 28),
        ),
        (
            "fn f() -> Unit { broken where Box<T> }",
            2,
            ByteRange::new(24, 29),
        ),
        ("record R { x BAD: Box<T> }", 1, ByteRange::new(13, 16)),
    ] {
        let document = parse_with_limits(
            text.as_bytes(),
            Limits {
                max_depth: depth,
                ..Limits::default()
            },
        );
        assert_eq!(document.source.render(), text.as_bytes());
        assert_eq!(
            document.diagnostics.len(),
            1,
            "{text}: {:?}",
            document.diagnostics
        );
        assert_eq!(
            document.diagnostics[0].code,
            DiagnosticCode::Syntax,
            "{text}"
        );
        assert_eq!(document.diagnostics[0].range, primary, "{text}");
        assert!(!document.is_syntax_eligible());
    }
}

#[test]
fn malformed_token_envelopes_remain_single_owned_errors() {
    for text in ["0x7", "1_2", "1e+", "123abc", "ab\u{200d}cd"] {
        let scanned = lex(text.as_bytes(), Limits::default());
        assert_eq!(scanned.source.render(), text.as_bytes());
        assert_eq!(scanned.source.pieces.len(), 1, "{text}");
        assert!(scanned.tokens.is_empty(), "{text}");
        assert_eq!(scanned.diagnostics.len(), 1, "{text}");
        assert_eq!(scanned.diagnostics[0].code, DiagnosticCode::Lexical);
    }
}
