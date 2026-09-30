//! Independent checks on the public recursive view, not serialized AST internals.
use CstKind as K;
use CstRole::*;
use std::collections::BTreeSet;
use wrela_frontend::{
    cst::{CstChild, CstElement, CstKind, CstNode, CstRole},
    source::{ByteRange, PieceKind, Source},
    syntax::{BinaryOperator, UnaryOperator},
    token::TokenKind,
};

fn required(kind: CstKind) -> &'static [CstRole] {
    match kind {
        K::Document => &[],
        K::Function => &[Keyword, Name, Parameters, Result, Body],
        K::Record | K::Enum => &[Keyword, Name, Body],
        K::Parameter => &[Name],
        K::Annotation => &[Colon, Type],
        K::ResultType => &[Arrow, Type],
        K::WhereClause => &[Keyword, Constraint],
        K::Constraint => &[Type, Colon, Bound],
        K::RecordField => &[Name, Annotation],
        K::EnumVariant => &[Name],
        K::NamedType | K::PathExpression | K::PathPattern => &[Path],
        K::ArrayType => &[Open, Type, Close],
        K::Path => &[Segment],
        K::Block
        | K::GenericParameters
        | K::Parameters
        | K::TypeArguments
        | K::Fields
        | K::Variants
        | K::PayloadTypes
        | K::MatchArms
        | K::Arguments
        | K::TupleType
        | K::PatternPayload
        | K::TupleExpression
        | K::ArrayExpression => &[Open, Close],
        K::LetStatement | K::VarStatement => &[Keyword, Name, Equal, Value],
        K::AssignmentStatement => &[Target, Equal, Value],
        K::ExpressionStatement => &[Expression],
        K::ReturnStatement => &[Keyword],
        K::IfStatement => &[Keyword, Condition, Then],
        K::ElseBranch => &[Keyword, Body],
        K::ForStatement | K::EveryStatement => &[Keyword, Binding, InKeyword, Iterable, Body],
        K::MatchStatement => &[Keyword, Scrutinee, Arms],
        K::MatchArm => &[Pattern, Arrow, Body],
        K::WildcardPattern
        | K::NumberPattern
        | K::BooleanPattern
        | K::NumberExpression
        | K::StringExpression
        | K::BooleanExpression => &[Value],
        K::GroupExpression => &[Open, Expression, Close],
        K::RecordExpression => &[Path, Body],
        K::LambdaExpression => &[Keyword, Parameters, Body],
        K::UnaryExpression(_) => &[Operator, Operand],
        K::BinaryExpression(_) => &[Left, Operator, Right],
        K::CallExpression => &[Callee, Arguments],
        K::FieldExpression => &[Receiver, Dot, Name],
        K::IndexExpression => &[Receiver, Open, Index, Close],
        K::PropagateExpression => &[Operand, Question],
        K::Argument => &[Value],
        K::FieldValue => &[Name, Colon, Value],
        K::Error | K::IncompleteFunction | K::FunctionPrefix | K::MissingClose => &[],
    }
}
fn allowed(kind: CstKind) -> &'static [CstRole] {
    match kind {
        K::Document => &[Declaration, Separator],
        K::Function => &[
            Keyword,
            Name,
            Generics,
            Parameters,
            Result,
            Constraints,
            Body,
        ],
        K::Record | K::Enum => &[Keyword, Name, Generics, Constraints, Body],
        K::Parameter => &[Permission, Name, Annotation],
        K::Annotation => &[Colon, Type],
        K::ResultType => &[Arrow, Type],
        K::WhereClause => &[Keyword, Constraint, Separator],
        K::Constraint => &[Type, Colon, Bound, Separator],
        K::RecordField => &[Name, Annotation],
        K::EnumVariant => &[Name, Payload],
        K::NamedType => &[Path, TypeArguments],
        K::TupleType | K::PayloadTypes | K::TypeArguments => &[Open, Type, Separator, Close],
        K::ArrayType => &[Open, Type, Close],
        K::Path => &[Segment, Separator],
        K::Block => &[Open, Statement, Separator, Close],
        K::GenericParameters | K::Parameters => &[Open, Parameter, Separator, Close],
        K::Fields => &[Open, Field, Separator, Close],
        K::Variants => &[Open, Variant, Separator, Close],
        K::MatchArms => &[Open, Arm, Separator, Close],
        K::Arguments => &[Open, Argument, Separator, Close],
        K::LetStatement | K::VarStatement => &[Keyword, Name, Annotation, Equal, Value],
        K::AssignmentStatement => &[Target, Equal, Value],
        K::ExpressionStatement => &[Expression],
        K::ReturnStatement => &[Keyword, Value],
        K::IfStatement => &[Keyword, Condition, Then, Else],
        K::ElseBranch => &[Keyword, Body],
        K::ForStatement | K::EveryStatement => &[Keyword, Binding, InKeyword, Iterable, Body],
        K::MatchStatement => &[Keyword, Scrutinee, Arms],
        K::MatchArm => &[Pattern, Arrow, Body],
        K::PathPattern => &[Path, Payload],
        K::WildcardPattern
        | K::NumberPattern
        | K::BooleanPattern
        | K::NumberExpression
        | K::StringExpression
        | K::BooleanExpression => &[Value],
        K::PatternPayload => &[Open, Pattern, Separator, Close],
        K::PathExpression => &[Path],
        K::GroupExpression => &[Open, Expression, Close],
        K::TupleExpression | K::ArrayExpression => &[Open, Element, Separator, Close],
        K::RecordExpression => &[Path, Body],
        K::LambdaExpression => &[Keyword, Parameters, Result, Body],
        K::UnaryExpression(_) => &[Operator, Operand],
        K::BinaryExpression(_) => &[Left, Operator, Right],
        K::CallExpression => &[Callee, TypeArguments, Arguments],
        K::FieldExpression => &[Receiver, Dot, Name],
        K::IndexExpression => &[Receiver, Open, Index, Close],
        K::PropagateExpression => &[Operand, Question],
        K::Argument => &[Permission, Value],
        K::FieldValue => &[Name, Colon, Value],
        K::Error | K::IncompleteFunction | K::FunctionPrefix | K::MissingClose => &[ErrorToken],
    }
}
fn repeated(role: CstRole) -> bool {
    matches!(
        role,
        Declaration
            | Statement
            | Parameter
            | Constraint
            | Bound
            | Segment
            | Separator
            | Type
            | Field
            | Variant
            | Arm
            | Pattern
            | Argument
            | Element
            | ErrorToken
    )
}
fn contains(outer: ByteRange, inner: ByteRange) -> bool {
    outer.start <= inner.start && inner.start <= inner.end && inner.end <= outer.end
}
fn range_valid(range: ByteRange, source: &Source, text: Option<&str>) -> bool {
    range.start <= range.end
        && range.end <= source.len()
        && text.is_none_or(|s| s.is_char_boundary(range.start) && s.is_char_boundary(range.end))
}
fn leaf_range(child: &CstChild, source: &Source) -> std::result::Result<ByteRange, String> {
    match &child.element {
        CstElement::Node(n) => Ok(n.full_range),
        CstElement::Token(id) => source
            .pieces
            .get(id.0)
            .map(|p| p.range)
            .ok_or("missing piece reference".into()),
    }
}
fn token_contract(kind: CstKind, role: CstRole, token: TokenKind) -> bool {
    use TokenKind as T;
    match role {
        Keyword => {
            token
                == match kind {
                    K::Function | K::LambdaExpression => T::Fn,
                    K::Record => T::Record,
                    K::Enum => T::Enum,
                    K::WhereClause => T::Where,
                    K::LetStatement => T::Let,
                    K::VarStatement => T::Var,
                    K::ReturnStatement => T::Return,
                    K::IfStatement => T::If,
                    K::ElseBranch => T::Else,
                    K::ForStatement => T::For,
                    K::EveryStatement => T::Every,
                    K::MatchStatement => T::Match,
                    _ => return false,
                }
        }
        Name | Binding | Segment | Parameter => token == T::Ident,
        Permission => token == T::Mut,
        InKeyword => token == T::In,
        Colon => token == T::Colon,
        Equal => token == T::Equal,
        Question => token == T::Question,
        Dot => token == T::Dot,
        Arrow => {
            token
                == if kind == K::MatchArm {
                    T::FatArrow
                } else {
                    T::Arrow
                }
        }
        Open => match kind {
            K::Block | K::Fields | K::Variants | K::MatchArms => token == T::LBrace,
            K::ArrayType | K::ArrayExpression | K::IndexExpression => token == T::LBracket,
            K::GenericParameters | K::TypeArguments => {
                matches!(token, T::GenericOpen | T::TypeOpen)
            }
            _ => token == T::LParen,
        },
        Close => match kind {
            K::Block | K::Fields | K::Variants | K::MatchArms => token == T::RBrace,
            K::ArrayType | K::ArrayExpression | K::IndexExpression => token == T::RBracket,
            K::GenericParameters | K::TypeArguments => {
                matches!(token, T::GenericClose | T::TypeClose)
            }
            _ => token == T::RParen,
        },
        Separator => match kind {
            K::Path => token == T::PathSeparator,
            K::Constraint => token == T::Plus,
            K::Block | K::Document => matches!(token, T::Semicolon | T::Newline),
            _ => token == T::Comma,
        },
        Operator => {
            token
                == match kind {
                    K::UnaryExpression(UnaryOperator::Negate) => T::Minus,
                    K::UnaryExpression(UnaryOperator::Not) => T::Bang,
                    K::BinaryExpression(op) => match op {
                        BinaryOperator::Multiply => T::Star,
                        BinaryOperator::Divide => T::Slash,
                        BinaryOperator::Remainder => T::Percent,
                        BinaryOperator::Add => T::Plus,
                        BinaryOperator::Subtract => T::Minus,
                        BinaryOperator::Equal => T::EqualEqual,
                        BinaryOperator::NotEqual => T::BangEqual,
                        BinaryOperator::Less => T::Less,
                        BinaryOperator::LessEqual => T::LessEqual,
                        BinaryOperator::Greater => T::Greater,
                        BinaryOperator::GreaterEqual => T::GreaterEqual,
                        BinaryOperator::And => T::AndAnd,
                        BinaryOperator::Or => T::OrOr,
                    },
                    _ => return false,
                }
        }
        Value => match kind {
            K::WildcardPattern => token == T::Ident,
            K::NumberExpression | K::NumberPattern => token == T::Number,
            K::StringExpression => token == T::String,
            K::BooleanExpression | K::BooleanPattern => matches!(token, T::True | T::False),
            _ => false,
        },
        _ => false,
    }
}
fn visit(
    node: &CstNode,
    source: &Source,
    text: Option<&str>,
    seen: &mut BTreeSet<usize>,
) -> std::result::Result<(), String> {
    if matches!(
        node.kind,
        K::Error | K::IncompleteFunction | K::FunctionPrefix | K::MissingClose
    ) {
        return Err("valid CST contains error island".into());
    }
    if !range_valid(node.range, source, text)
        || !range_valid(node.full_range, source, text)
        || !contains(node.full_range, node.range)
    {
        return Err("impossible syntax range".into());
    }
    for role in required(node.kind) {
        if !node.children.iter().any(|c| c.role == *role) {
            return Err(format!("{:?} missing {role:?}", node.kind));
        }
    }
    for role in allowed(node.kind) {
        if !repeated(*role) && node.children.iter().filter(|c| c.role == *role).count() > 1 {
            return Err("duplicate singular child role".into());
        }
    }
    let expected_order: Option<&[CstRole]> = match node.kind {
        K::BinaryExpression(_) => Some(&[Left, Operator, Right]),
        K::UnaryExpression(_) => Some(&[Operator, Operand]),
        K::PropagateExpression => Some(&[Operand, Question]),
        _ => None,
    };
    if let Some(expected) = expected_order
        && node.children.iter().map(|c| c.role).collect::<Vec<_>>() != expected
    {
        return Err("expression child roles disagree with source order".into());
    }
    let mut previous = node.full_range.start;
    for child in &node.children {
        if !allowed(node.kind).contains(&child.role) {
            return Err("role does not belong to syntax kind".into());
        }
        let r = leaf_range(child, source)?;
        if !contains(node.full_range, r) || r.start < previous {
            return Err("broken child containment/order".into());
        }
        previous = r.end;
        match &child.element {
            CstElement::Node(n) => visit(n, source, text, seen)?,
            CstElement::Token(id) => {
                if !seen.insert(id.0) {
                    return Err("duplicated source token reference".into());
                }
                let piece = &source.pieces[id.0];
                if node.kind == K::WildcardPattern && piece.bytes != b"_" {
                    return Err("wildcard spelling erased".into());
                }
                let k = match piece.kind {
                    PieceKind::Token(k) => k,
                    PieceKind::Trivia(wrela_frontend::source::TriviaKind::Newline) => {
                        TokenKind::Newline
                    }
                    PieceKind::Trivia(wrela_frontend::source::TriviaKind::BlockComment)
                        if piece.bytes.iter().any(|b| matches!(*b, b'\r' | b'\n')) =>
                    {
                        TokenKind::Newline
                    }
                    _ => return Err("invalid piece used as syntax token".into()),
                };
                if !token_contract(node.kind, child.role, k) {
                    return Err(format!("{:?} {:?} references {k:?}", node.kind, child.role));
                }
            }
        }
    }
    Ok(())
}
pub fn audit(root: &CstNode, source: &Source) -> std::result::Result<(), String> {
    if root.kind != K::Document || root.full_range != ByteRange::new(0, source.len()) {
        return Err("invalid document full range".into());
    }
    let rendered: Vec<_> = source
        .pieces
        .iter()
        .flat_map(|p| p.bytes.iter().copied())
        .collect();
    let text = std::str::from_utf8(&rendered).ok();
    let mut seen = BTreeSet::new();
    visit(root, source, text, &mut seen)?;
    for (id, piece) in source.pieces.iter().enumerate() {
        if matches!(piece.kind, PieceKind::Token(_)) && !seen.contains(&id) {
            return Err(format!("erased token {id}"));
        }
    }
    Ok(())
}
