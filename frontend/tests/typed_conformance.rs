mod support {
    pub mod fixtures;
}
use support::fixtures::{ACCEPTED, RECOVERY, REJECTED};
use wrela_frontend::{ParsedDocument, parse, source::Source, syntax::*};

fn eligible(source: &str) -> ParsedDocument {
    let parsed = parse(source.as_bytes());
    assert!(
        parsed.diagnostics.is_empty(),
        "source {source:?}: {:?}",
        parsed.diagnostics
    );
    assert!(parsed.complete);
    assert!(parsed.is_syntax_eligible());
    parsed
}
fn function<'a>(parsed: &'a ParsedDocument, name: &str) -> &'a Function {
    parsed
        .syntax
        .declarations
        .iter()
        .find_map(|node| match &node.kind {
            Declaration::Function(f) if parsed.source.token_text(f.name) == Some(name) => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("function {name} did not survive"))
}
fn expression(statement: &Statement) -> &Expr {
    match &statement.kind {
        StatementKind::Expression(e) => e,
        _ => panic!("expected expression statement: {statement:?}"),
    }
}
/// Independently specified algebraic shapes, built from the public typed syntax.
fn shape(expression: &Expr, source: &Source) -> String {
    match &expression.kind {
        ExprKind::Path(path) => path
            .segments
            .iter()
            .map(|id| source.token_text(*id).unwrap())
            .collect::<Vec<_>>()
            .join("::"),
        ExprKind::Number(id) => source.token_text(*id).unwrap().into(),
        ExprKind::Group(d) => format!("group({})", shape(&d.contents, source)),
        ExprKind::Unary {
            operator, operand, ..
        } => format!("{operator:?}({})", shape(operand, source)),
        ExprKind::Binary {
            operator,
            left,
            right,
            ..
        } => format!(
            "{operator:?}({},{})",
            shape(left, source),
            shape(right, source)
        ),
        ExprKind::Call {
            callee, arguments, ..
        } => format!(
            "call({};{})",
            shape(callee, source),
            arguments
                .contents
                .items
                .iter()
                .map(|a| shape(&a.kind.value, source))
                .collect::<Vec<_>>()
                .join(",")
        ),
        ExprKind::Field { receiver, name, .. } => format!(
            "field({},{})",
            shape(receiver, source),
            source.token_text(*name).unwrap()
        ),
        ExprKind::Index { receiver, index } => format!(
            "index({},{})",
            shape(receiver, source),
            shape(&index.contents, source)
        ),
        ExprKind::Propagate { operand, .. } => format!("propagate({})", shape(operand, source)),
        other => panic!("shape oracle has no expectation for {other:?}"),
    }
}

#[test]
fn independently_authored_acceptance_and_boundary_cases() {
    for (name, source) in ACCEPTED {
        let _ = eligible(source);
        eprintln!("accepted {name}");
    }
    for (name, source) in REJECTED {
        let parsed = parse(source.as_bytes());
        assert!(
            !parsed.diagnostics.is_empty() || !parsed.complete || parsed.syntax.has_errors(),
            "unexpected eligible case {name}"
        );
        assert!(
            parsed.admit().is_err(),
            "invalid input reached later phases: {name}"
        );
    }
}

#[test]
fn independently_specified_precedence_association_and_postfix_shapes() {
    let cases = [
        ("a + b * c", "Add(a,Multiply(b,c))"),
        ("a * b + c", "Add(Multiply(a,b),c)"),
        ("a - b - c", "Subtract(Subtract(a,b),c)"),
        ("a / b % c", "Remainder(Divide(a,b),c)"),
        ("a || b && c", "Or(a,And(b,c))"),
        ("a && b == c", "And(a,Equal(b,c))"),
        ("!!-a", "Not(Not(Negate(a)))"),
        ("-a * b", "Multiply(Negate(a),b)"),
        ("(a + b) * c", "Multiply(group(Add(a,b)),c)"),
        (
            "f(x).field[0]?",
            "propagate(index(field(call(f;x),field),0))",
        ),
        ("a < b", "Less(a,b)"),
        ("(a < b) == c", "Equal(group(Less(a,b)),c)"),
    ];
    for (text, expected) in cases {
        let parsed = eligible(&format!("fn shape() -> Unit {{\n {text}\n}}\n"));
        let actual = shape(
            expression(&function(&parsed, "shape").body.statements[0]),
            &parsed.source,
        );
        assert_eq!(actual, expected, "expression {text}");
    }
}

#[test]
fn meaningful_permissions_binding_iteration_return_and_propagation_survive() {
    let parsed = eligible(
        "fn tick(mut values: Array<T>, dt: Number) -> Unit {\n let fixed = 1\n var writable = 2\n for a in values { return }\n every b in values { return b }\n advance(mut values, dt)?\n return\n}\n",
    );
    let f = function(&parsed, "tick");
    assert_eq!(
        parsed
            .source
            .token_text(f.parameters.contents.items[0].kind.permission.unwrap()),
        Some("mut")
    );
    assert!(f.parameters.contents.items[1].kind.permission.is_none());
    assert!(matches!(
        f.body.statements[0].kind,
        StatementKind::Local {
            binding: Binding::Let(_),
            ..
        }
    ));
    assert!(matches!(
        f.body.statements[1].kind,
        StatementKind::Local {
            binding: Binding::Var(_),
            ..
        }
    ));
    for (index, every, valued_return) in [(2, false, false), (3, true, true)] {
        let StatementKind::Iterate {
            iteration, body, ..
        } = &f.body.statements[index].kind
        else {
            panic!("iteration erased")
        };
        assert_eq!(matches!(iteration, Iteration::Every(_)), every);
        let StatementKind::Return { keyword, value } = &body.statements[0].kind else {
            panic!("return erased")
        };
        assert_eq!(parsed.source.token_text(*keyword), Some("return"));
        assert_eq!(value.is_some(), valued_return);
    }
    let ExprKind::Propagate { operand, question } = &expression(&f.body.statements[4]).kind else {
        panic!("propagation erased")
    };
    assert_eq!(parsed.source.token_text(*question), Some("?"));
    let ExprKind::Call {
        callee, arguments, ..
    } = &operand.kind
    else {
        panic!("callee erased")
    };
    assert_eq!(shape(callee, &parsed.source), "advance");
    assert_eq!(arguments.contents.items.len(), 2);
    assert_eq!(
        parsed
            .source
            .token_text(arguments.contents.items[0].kind.permission.unwrap()),
        Some("mut")
    );
    assert!(arguments.contents.items[1].kind.permission.is_none());
    assert_eq!(
        shape(&arguments.contents.items[0].kind.value, &parsed.source),
        "values"
    );
    assert_eq!(
        shape(&arguments.contents.items[1].kind.value, &parsed.source),
        "dt"
    );
    assert!(matches!(
        f.body.statements[5].kind,
        StatementKind::Return { value: None, .. }
    ));
}

#[test]
fn generic_children_payload_distinctions_constraints_and_lambda_boundaries_survive() {
    let parsed = eligible(
        "enum Shape { Empty, Explicit(), Pair(Number, Text), }\nfn run<T>(x: Array<Box<T>>) -> T where T: Numeric + Copy {\n (f\n<T>(x))\n run(fn(x) {\n first\n second\n return x\n })\n return x\n}\n",
    );
    let Declaration::Enum(enumeration) = &parsed.syntax.declarations[0].kind else {
        panic!("enum erased")
    };
    assert!(
        enumeration.variants.contents.items[0]
            .kind
            .payload
            .is_none()
    );
    assert_eq!(
        enumeration.variants.contents.items[1]
            .kind
            .payload
            .as_ref()
            .unwrap()
            .contents
            .items
            .len(),
        0
    );
    assert_eq!(
        enumeration.variants.contents.items[2]
            .kind
            .payload
            .as_ref()
            .unwrap()
            .contents
            .items
            .len(),
        2
    );
    let f = function(&parsed, "run");
    assert_eq!(f.generics.as_ref().unwrap().contents.items.len(), 1);
    let constraints = &f.constraints.as_ref().unwrap().constraints.items;
    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].kind.bounds.items.len(), 2);
    let ExprKind::Group(group) = &expression(&f.body.statements[0]).kind else {
        panic!("group erased")
    };
    let ExprKind::Call {
        type_arguments: Some(types),
        ..
    } = &group.contents.kind
    else {
        panic!("generic call erased")
    };
    assert_eq!(types.contents.items.len(), 1);
    let ExprKind::Call { arguments, .. } = &expression(&f.body.statements[1]).kind else {
        panic!("lambda argument erased")
    };
    let ExprKind::Lambda { body, .. } = &arguments.contents.items[0].kind.value.kind else {
        panic!("lambda erased")
    };
    assert_eq!(
        body.statements.len(),
        3,
        "lambda block must reset enclosing call continuation"
    );
    assert_eq!(
        shape(expression(&body.statements[0]), &parsed.source),
        "first"
    );
    assert_eq!(
        shape(expression(&body.statements[1]), &parsed.source),
        "second"
    );
}

#[test]
fn bounded_recovery_preserves_named_later_structure_and_blocks_admission() {
    for case in RECOVERY {
        let parsed = parse(case.source.as_bytes());
        assert!(
            parsed.diagnostics.len() >= case.minimum_diagnostics,
            "{}: {:?}",
            case.name,
            parsed.diagnostics
        );
        assert_eq!(
            parsed.source.render(),
            case.source.as_bytes(),
            "{}",
            case.name
        );
        if let Some(name) = case.later_function {
            let _ = function(&parsed, name);
        }
        if let Some(name) = case.later_binding {
            let bad = function(&parsed, "bad");
            assert!(
                bad.body.statements.iter().any(|s| match &s.kind {
                    StatementKind::Local { name: token, .. } =>
                        parsed.source.token_text(*token) == Some(name),
                    _ => false,
                }),
                "later binding lost in {}",
                case.name
            );
        }
        if case.physical_eof {
            assert!(parsed.diagnostics.iter().any(|d| d.range.start == case.source.len() && d.range.end == case.source.len()), "{} did not anchor physical EOF: {:?}", case.name, parsed.diagnostics);
        }
        assert!(
            parsed.admit().is_err(),
            "{} admitted recovery structure",
            case.name
        );
    }
}

#[test]
fn multiline_comment_separates_statements_without_erasing_its_bytes() {
    let source = "fn f() -> Unit { inspect(1) /* c\n d */ inspect(2)\nreturn }";
    let parsed = eligible(source);
    let statements = &function(&parsed, "f").body.statements;
    assert_eq!(statements.len(), 3);
    assert_eq!(
        shape(expression(&statements[0]), &parsed.source),
        "call(inspect;1)"
    );
    assert_eq!(
        shape(expression(&statements[1]), &parsed.source),
        "call(inspect;2)"
    );
    assert!(matches!(
        statements[2].kind,
        StatementKind::Return { value: None, .. }
    ));
    assert_eq!(
        parsed
            .source
            .pieces
            .iter()
            .flat_map(|p| p.bytes.iter().copied())
            .collect::<Vec<_>>(),
        source.as_bytes()
    );
}
