use crate::parser::kind::SyntaxKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    StartNode {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    AddToken,
    FinishNode,
    Placeholder,
}
