#[allow(dead_code)]
mod support {
    pub mod cst_audit;
    pub mod fixtures;
}
use support::{cst_audit::audit, fixtures::ACCEPTED};
use wrela_frontend::{
    cst::{self, CstElement, CstKind, CstNode, CstRole},
    parse,
    source::ByteRange,
};
fn find_mut(node: &mut CstNode, kind: CstKind) -> &mut CstNode {
    if node.kind == kind {
        return node;
    }
    node.children
        .iter_mut()
        .find_map(|child| match &mut child.element {
            CstElement::Node(n) => find_optional(n, kind),
            _ => None,
        })
        .expect("fixture kind absent")
}
fn find_optional(node: &mut CstNode, kind: CstKind) -> Option<&mut CstNode> {
    if node.kind == kind {
        return Some(node);
    }
    node.children
        .iter_mut()
        .find_map(|child| match &mut child.element {
            CstElement::Node(n) => find_optional(n, kind),
            _ => None,
        })
}
#[test]
fn independent_audit_accepts_real_recursive_views_of_all_additional_fixtures() {
    for (name, source) in ACCEPTED {
        let parsed = parse(source.as_bytes());
        assert!(
            parsed.is_syntax_eligible(),
            "{name}: {:?}",
            parsed.diagnostics
        );
        audit(&parsed.cst(), &parsed.source).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}
#[test]
fn erased_children_markers_flattening_and_impossible_spans_are_rejected() {
    let parsed=parse(b"fn tick(mut values: Array<T>) -> Unit {\n let fixed = 1 + 2\n advance(mut values, dt)?\n return\n}\n");
    let original = parsed.cst();
    audit(&original, &parsed.source).unwrap();
    assert!(cst::validate(&original, &parsed.source).is_ok());
    let mut mutations = Vec::new();
    let mut erased = original.clone();
    find_mut(&mut erased, CstKind::CallExpression)
        .children
        .retain(|c| c.role != CstRole::Callee);
    mutations.push(("callee", erased));
    let mut marker = original.clone();
    find_mut(&mut marker, CstKind::Argument)
        .children
        .retain(|c| c.role != CstRole::Permission);
    mutations.push(("mut", marker));
    let mut question = original.clone();
    find_mut(&mut question, CstKind::PropagateExpression)
        .children
        .retain(|c| c.role != CstRole::Question);
    mutations.push(("question", question));
    let mut body = original.clone();
    find_mut(&mut body, CstKind::Function)
        .children
        .retain(|c| c.role != CstRole::Body);
    mutations.push(("body", body));
    let mut flat = original.clone();
    flat.children.clear();
    mutations.push(("flattened", flat));
    let mut bounds = original.clone();
    find_mut(&mut bounds, CstKind::Argument).range.end = parsed.source.len() + 1;
    mutations.push(("out of bounds", bounds));
    let mut inverted = original.clone();
    find_mut(&mut inverted, CstKind::Argument).range = ByteRange::new(9, 3);
    mutations.push(("inverted", inverted));
    let mut containment = original.clone();
    find_mut(&mut containment, CstKind::CallExpression).full_range = ByteRange::empty(0);
    mutations.push(("containment", containment));
    let mut nonexistent = original.clone();
    let target = find_mut(&mut nonexistent, CstKind::PropagateExpression);
    target
        .children
        .iter_mut()
        .find(|c| c.role == CstRole::Question)
        .unwrap()
        .element = CstElement::Token(wrela_frontend::token::TokenId(usize::MAX));
    mutations.push(("missing piece", nonexistent));
    let mut wrong_marker = original.clone();
    let fn_marker = parsed
        .source
        .pieces
        .iter()
        .position(|p| p.bytes == b"fn")
        .unwrap();
    find_mut(&mut wrong_marker, CstKind::LetStatement)
        .children
        .iter_mut()
        .find(|c| c.role == CstRole::Keyword)
        .unwrap()
        .element = CstElement::Token(wrela_frontend::token::TokenId(fn_marker));
    mutations.push(("wrong keyword token", wrong_marker));
    let mut swapped_roles = original.clone();
    let binary = find_mut(
        &mut swapped_roles,
        CstKind::BinaryExpression(wrela_frontend::syntax::BinaryOperator::Add),
    );
    for child in &mut binary.children {
        child.role = match child.role {
            CstRole::Left => CstRole::Right,
            CstRole::Right => CstRole::Left,
            role => role,
        };
    }
    mutations.push(("swapped operand roles", swapped_roles));
    for (name, corrupt) in mutations {
        assert!(
            audit(&corrupt, &parsed.source).is_err(),
            "independent audit accepted {name}"
        );
        assert!(
            cst::validate(&corrupt, &parsed.source).is_err(),
            "production gate accepted {name}"
        );
    }
}
#[test]
fn exported_structure_rejects_unknown_enum_kinds_and_roles_at_decoding_boundary() {
    let parsed = parse(b"fn f() -> Unit { return }");
    let value = serde_json::to_value(parsed.cst()).unwrap();
    let mut invalid_kind = value.clone();
    invalid_kind["kind"] = serde_json::json!("InventedSyntaxKind");
    assert!(serde_json::from_value::<CstNode>(invalid_kind).is_err());
    let mut invalid_role = value.clone();
    invalid_role["children"][0]["role"] = serde_json::json!("InventedChildRole");
    assert!(serde_json::from_value::<CstNode>(invalid_role).is_err());
    for range in [
        serde_json::json!({"start":true,"end":1}),
        serde_json::json!({"start":-1,"end":1}),
    ] {
        let mut invalid = value.clone();
        invalid["range"] = range;
        assert!(serde_json::from_value::<CstNode>(invalid).is_err());
    }
}
