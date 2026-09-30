//! Recursive inspection view. Source pieces retain sole ownership of every byte.
//! Interior trivia remains on the source tape; non-root full ranges equal syntax ranges.
use crate::{
    source::{ByteRange, PieceKind, Source},
    syntax::*,
    token::{TokenId, TokenKind},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CstKind {
    Document,
    Function,
    IncompleteFunction,
    FunctionPrefix,
    MissingClose,
    Record,
    Enum,
    Parameter,
    Annotation,
    ResultType,
    WhereClause,
    Constraint,
    RecordField,
    EnumVariant,
    NamedType,
    TupleType,
    ArrayType,
    Path,
    Block,
    GenericParameters,
    Parameters,
    TypeArguments,
    Fields,
    Variants,
    PayloadTypes,
    MatchArms,
    Arguments,
    LetStatement,
    VarStatement,
    AssignmentStatement,
    ExpressionStatement,
    ReturnStatement,
    IfStatement,
    ElseBranch,
    ForStatement,
    EveryStatement,
    MatchStatement,
    MatchArm,
    PathPattern,
    WildcardPattern,
    NumberPattern,
    BooleanPattern,
    PatternPayload,
    PathExpression,
    NumberExpression,
    StringExpression,
    BooleanExpression,
    GroupExpression,
    TupleExpression,
    ArrayExpression,
    RecordExpression,
    LambdaExpression,
    UnaryExpression(UnaryOperator),
    BinaryExpression(BinaryOperator),
    CallExpression,
    FieldExpression,
    IndexExpression,
    PropagateExpression,
    Argument,
    FieldValue,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CstRole {
    Declaration,
    Prefix,
    Remainder,
    MissingClose,
    Statement,
    Keyword,
    Name,
    Permission,
    Generics,
    Parameters,
    Parameter,
    Annotation,
    Type,
    Result,
    Constraints,
    Constraint,
    Bound,
    Path,
    Segment,
    Open,
    Close,
    Separator,
    Colon,
    Arrow,
    Body,
    Field,
    Variant,
    Payload,
    Binding,
    InKeyword,
    Condition,
    Then,
    Else,
    Iterable,
    Scrutinee,
    Arms,
    Arm,
    Pattern,
    Value,
    Target,
    Equal,
    Expression,
    Operand,
    Operator,
    Left,
    Right,
    Callee,
    TypeArguments,
    Arguments,
    Argument,
    Receiver,
    Dot,
    Index,
    Question,
    Element,
    ErrorToken,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CstNode {
    pub kind: CstKind,
    pub range: ByteRange,
    pub full_range: ByteRange,
    pub children: Vec<CstChild>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CstChild {
    pub role: CstRole,
    pub element: CstElement,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CstElement {
    Node(Box<CstNode>),
    Token(TokenId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralError {
    pub kind: CstKind,
    pub range: ByteRange,
    pub message: String,
}

fn token(role: CstRole, id: TokenId) -> CstChild {
    CstChild {
        role,
        element: CstElement::Token(id),
    }
}
fn child(role: CstRole, node: CstNode) -> CstChild {
    CstChild {
        role,
        element: CstElement::Node(Box::new(node)),
    }
}
fn child_range(c: &CstChild, s: &Source) -> ByteRange {
    match &c.element {
        CstElement::Node(n) => n.range,
        CstElement::Token(id) => s.pieces.get(id.0).map_or(ByteRange::empty(0), |p| p.range),
    }
}
fn node(
    kind: CstKind,
    range: Option<ByteRange>,
    mut children: Vec<CstChild>,
    s: &Source,
) -> CstNode {
    children.sort_by_key(|c| {
        let r = child_range(c, s);
        (r.start, r.end)
    });
    let inferred = children
        .iter()
        .map(|c| child_range(c, s))
        .reduce(ByteRange::cover)
        .unwrap_or_default();
    let range = range.unwrap_or(inferred);
    CstNode {
        kind,
        range,
        full_range: range,
        children,
    }
}
fn list<T>(
    kind: CstKind,
    d: &Delimited<Separated<T>>,
    convert: impl Fn(&T) -> CstChild,
    s: &Source,
) -> CstNode {
    let mut cs = vec![token(CstRole::Open, d.open), token(CstRole::Close, d.close)];
    cs.extend(d.contents.items.iter().map(convert));
    cs.extend(
        d.contents
            .separators
            .iter()
            .map(|id| token(CstRole::Separator, *id)),
    );
    node(kind, None, cs, s)
}
fn path(p: &Path, s: &Source) -> CstNode {
    node(
        CstKind::Path,
        None,
        p.segments
            .iter()
            .map(|id| token(CstRole::Segment, *id))
            .chain(p.separators.iter().map(|id| token(CstRole::Separator, *id)))
            .collect(),
        s,
    )
}
fn annotation(a: &Annotation, s: &Source) -> CstNode {
    node(
        CstKind::Annotation,
        None,
        vec![
            token(CstRole::Colon, a.colon),
            child(CstRole::Type, ty(&a.ty, s)),
        ],
        s,
    )
}
fn result(r: &ResultType, s: &Source) -> CstNode {
    node(
        CstKind::ResultType,
        None,
        vec![
            token(CstRole::Arrow, r.arrow),
            child(CstRole::Type, ty(&r.ty, s)),
        ],
        s,
    )
}
fn generics(d: &Delimited<Separated<TokenId>>, s: &Source) -> CstNode {
    list(
        CstKind::GenericParameters,
        d,
        |id| token(CstRole::Parameter, *id),
        s,
    )
}
fn parameters(d: &Delimited<Separated<Node<Parameter>>>, s: &Source) -> CstNode {
    list(
        CstKind::Parameters,
        d,
        |p| {
            let mut cs = vec![token(CstRole::Name, p.kind.name)];
            if let Some(id) = p.kind.permission {
                cs.push(token(CstRole::Permission, id));
            }
            if let Some(a) = &p.kind.annotation {
                cs.push(child(CstRole::Annotation, annotation(a, s)));
            }
            child(
                CstRole::Parameter,
                node(CstKind::Parameter, Some(p.range), cs, s),
            )
        },
        s,
    )
}
fn where_clause(w: &WhereClause, s: &Source) -> CstNode {
    let mut cs = vec![token(CstRole::Keyword, w.keyword)];
    cs.extend(
        w.constraints
            .separators
            .iter()
            .map(|id| token(CstRole::Separator, *id)),
    );
    cs.extend(w.constraints.items.iter().map(|c| {
        let mut cs = vec![
            child(CstRole::Type, ty(&c.kind.parameter, s)),
            token(CstRole::Colon, c.kind.colon),
        ];
        cs.extend(
            c.kind
                .bounds
                .items
                .iter()
                .map(|t| child(CstRole::Bound, ty(t, s))),
        );
        cs.extend(
            c.kind
                .bounds
                .separators
                .iter()
                .map(|id| token(CstRole::Separator, *id)),
        );
        child(
            CstRole::Constraint,
            node(CstKind::Constraint, Some(c.range), cs, s),
        )
    }));
    node(CstKind::WhereClause, None, cs, s)
}
fn ty(t: &Type, s: &Source) -> CstNode {
    let (kind, cs) = match &t.kind {
        TypeKind::Named { path: p, arguments } => {
            let mut cs = vec![child(CstRole::Path, path(p, s))];
            if let Some(d) = arguments {
                cs.push(child(CstRole::TypeArguments, type_arguments(d, s)));
            }
            (CstKind::NamedType, cs)
        }
        TypeKind::Tuple(d) => (CstKind::TupleType, delimited_types(d, s)),
        TypeKind::Array(d) => (
            CstKind::ArrayType,
            vec![
                token(CstRole::Open, d.open),
                child(CstRole::Type, ty(&d.contents, s)),
                token(CstRole::Close, d.close),
            ],
        ),
    };
    node(kind, Some(t.range), cs, s)
}
fn delimited_types(d: &Delimited<Separated<Type>>, s: &Source) -> Vec<CstChild> {
    let mut cs = vec![token(CstRole::Open, d.open), token(CstRole::Close, d.close)];
    cs.extend(
        d.contents
            .items
            .iter()
            .map(|t| child(CstRole::Type, ty(t, s))),
    );
    cs.extend(
        d.contents
            .separators
            .iter()
            .map(|id| token(CstRole::Separator, *id)),
    );
    cs
}
fn type_arguments(d: &Delimited<Separated<Type>>, s: &Source) -> CstNode {
    node(CstKind::TypeArguments, None, delimited_types(d, s), s)
}
fn block(b: &Block, s: &Source) -> CstNode {
    let mut cs = vec![token(CstRole::Open, b.open), token(CstRole::Close, b.close)];
    cs.extend(
        b.statements
            .iter()
            .map(|st| child(CstRole::Statement, statement(st, s))),
    );
    cs.extend(b.separators.iter().map(|id| token(CstRole::Separator, *id)));
    node(CstKind::Block, Some(b.range), cs, s)
}
fn error(e: &ErrorSyntax, r: ByteRange, s: &Source) -> CstNode {
    node(
        CstKind::Error,
        Some(r),
        e.tokens
            .iter()
            .map(|id| token(CstRole::ErrorToken, *id))
            .collect(),
        s,
    )
}
fn declaration(d: &Node<Declaration>, s: &Source) -> CstNode {
    let (kind, cs) = match &d.kind {
        Declaration::Error(e) => return error(e, d.range, s),
        Declaration::IncompleteFunction(f) => {
            let p = &f.prefix.kind;
            let mut header = vec![
                token(CstRole::Keyword, p.keyword),
                token(CstRole::Name, p.name),
                child(CstRole::Parameters, parameters(&p.parameters, s)),
                child(CstRole::Result, result(&p.result, s)),
                token(CstRole::Open, p.open),
            ];
            if let Some(g) = &p.generics {
                header.push(child(CstRole::Generics, generics(g, s)));
            }
            if let Some(w) = &p.constraints {
                header.push(child(CstRole::Constraints, where_clause(w, s)));
            }
            let mut cs = vec![
                child(
                    CstRole::Prefix,
                    node(CstKind::FunctionPrefix, Some(f.prefix.range), header, s),
                ),
                child(
                    CstRole::MissingClose,
                    node(CstKind::MissingClose, Some(f.missing_close), vec![], s),
                ),
            ];
            cs.extend(
                f.statements
                    .iter()
                    .map(|st| child(CstRole::Statement, statement(st, s))),
            );
            cs.extend(f.separators.iter().map(|id| token(CstRole::Separator, *id)));
            let remainder_range = f
                .remainder
                .tokens
                .iter()
                .filter_map(|id| s.pieces.get(id.0).map(|p| p.range))
                .reduce(ByteRange::cover)
                .unwrap_or(f.missing_close);
            cs.push(child(
                CstRole::Remainder,
                error(&f.remainder, remainder_range, s),
            ));
            (CstKind::IncompleteFunction, cs)
        }
        Declaration::Function(f) => {
            let mut cs = vec![
                token(CstRole::Keyword, f.keyword),
                token(CstRole::Name, f.name),
                child(CstRole::Parameters, parameters(&f.parameters, s)),
                child(CstRole::Result, result(&f.result, s)),
                child(CstRole::Body, block(&f.body, s)),
            ];
            if let Some(g) = &f.generics {
                cs.push(child(CstRole::Generics, generics(g, s)));
            }
            if let Some(w) = &f.constraints {
                cs.push(child(CstRole::Constraints, where_clause(w, s)));
            }
            (CstKind::Function, cs)
        }
        Declaration::Record(r) => {
            let mut cs = vec![
                token(CstRole::Keyword, r.keyword),
                token(CstRole::Name, r.name),
                child(
                    CstRole::Body,
                    list(
                        CstKind::Fields,
                        &r.fields,
                        |f| {
                            child(
                                CstRole::Field,
                                node(
                                    CstKind::RecordField,
                                    Some(f.range),
                                    vec![
                                        token(CstRole::Name, f.kind.name),
                                        child(
                                            CstRole::Annotation,
                                            annotation(&f.kind.annotation, s),
                                        ),
                                    ],
                                    s,
                                ),
                            )
                        },
                        s,
                    ),
                ),
            ];
            if let Some(g) = &r.generics {
                cs.push(child(CstRole::Generics, generics(g, s)));
            }
            if let Some(w) = &r.constraints {
                cs.push(child(CstRole::Constraints, where_clause(w, s)));
            }
            (CstKind::Record, cs)
        }
        Declaration::Enum(e) => {
            let mut cs = vec![
                token(CstRole::Keyword, e.keyword),
                token(CstRole::Name, e.name),
                child(
                    CstRole::Body,
                    list(
                        CstKind::Variants,
                        &e.variants,
                        |v| {
                            let mut cs = vec![token(CstRole::Name, v.kind.name)];
                            if let Some(p) = &v.kind.payload {
                                cs.push(child(
                                    CstRole::Payload,
                                    node(CstKind::PayloadTypes, None, delimited_types(p, s), s),
                                ));
                            }
                            child(
                                CstRole::Variant,
                                node(CstKind::EnumVariant, Some(v.range), cs, s),
                            )
                        },
                        s,
                    ),
                ),
            ];
            if let Some(g) = &e.generics {
                cs.push(child(CstRole::Generics, generics(g, s)));
            }
            if let Some(w) = &e.constraints {
                cs.push(child(CstRole::Constraints, where_clause(w, s)));
            }
            (CstKind::Enum, cs)
        }
    };
    node(kind, Some(d.range), cs, s)
}
fn statement(st: &Statement, s: &Source) -> CstNode {
    let (kind, cs) = match &st.kind {
        StatementKind::Error(e) => return error(e, st.range, s),
        StatementKind::Local {
            binding,
            name,
            annotation: a,
            equal,
            value,
        } => {
            let (kind, id) = match binding {
                Binding::Let(id) => (CstKind::LetStatement, *id),
                Binding::Var(id) => (CstKind::VarStatement, *id),
            };
            let mut cs = vec![
                token(CstRole::Keyword, id),
                token(CstRole::Name, *name),
                token(CstRole::Equal, *equal),
                child(CstRole::Value, expr(value, s)),
            ];
            if let Some(a) = a {
                cs.push(child(CstRole::Annotation, annotation(a, s)));
            }
            (kind, cs)
        }
        StatementKind::Assign {
            target,
            equal,
            value,
        } => (
            CstKind::AssignmentStatement,
            vec![
                child(CstRole::Target, expr(target, s)),
                token(CstRole::Equal, *equal),
                child(CstRole::Value, expr(value, s)),
            ],
        ),
        StatementKind::Expression(e) => (
            CstKind::ExpressionStatement,
            vec![child(CstRole::Expression, expr(e, s))],
        ),
        StatementKind::Return { keyword, value } => {
            let mut cs = vec![token(CstRole::Keyword, *keyword)];
            if let Some(e) = value {
                cs.push(child(CstRole::Value, expr(e, s)));
            }
            (CstKind::ReturnStatement, cs)
        }
        StatementKind::If {
            keyword,
            condition,
            then_block,
            otherwise,
        } => {
            let mut cs = vec![
                token(CstRole::Keyword, *keyword),
                child(CstRole::Condition, expr(condition, s)),
                child(CstRole::Then, block(then_block, s)),
            ];
            if let Some(b) = otherwise {
                cs.push(child(
                    CstRole::Else,
                    node(
                        CstKind::ElseBranch,
                        None,
                        vec![
                            token(CstRole::Keyword, b.keyword),
                            child(
                                CstRole::Body,
                                match &b.body {
                                    ElseBody::Block(b) => block(b, s),
                                    ElseBody::If(st) => statement(st, s),
                                },
                            ),
                        ],
                        s,
                    ),
                ));
            }
            (CstKind::IfStatement, cs)
        }
        StatementKind::Iterate {
            iteration,
            binding,
            in_keyword,
            iterable,
            body,
        } => {
            let (kind, id) = match iteration {
                Iteration::For(id) => (CstKind::ForStatement, *id),
                Iteration::Every(id) => (CstKind::EveryStatement, *id),
            };
            (
                kind,
                vec![
                    token(CstRole::Keyword, id),
                    token(CstRole::Binding, *binding),
                    token(CstRole::InKeyword, *in_keyword),
                    child(CstRole::Iterable, expr(iterable, s)),
                    child(CstRole::Body, block(body, s)),
                ],
            )
        }
        StatementKind::Match {
            keyword,
            scrutinee,
            arms,
        } => (
            CstKind::MatchStatement,
            vec![
                token(CstRole::Keyword, *keyword),
                child(CstRole::Scrutinee, expr(scrutinee, s)),
                child(
                    CstRole::Arms,
                    list(
                        CstKind::MatchArms,
                        arms,
                        |a| {
                            child(
                                CstRole::Arm,
                                node(
                                    CstKind::MatchArm,
                                    Some(a.range),
                                    vec![
                                        child(CstRole::Pattern, pattern(&a.kind.pattern, s)),
                                        token(CstRole::Arrow, a.kind.arrow),
                                        child(CstRole::Body, block(&a.kind.body, s)),
                                    ],
                                    s,
                                ),
                            )
                        },
                        s,
                    ),
                ),
            ],
        ),
    };
    node(kind, Some(st.range), cs, s)
}
fn pattern(p: &Pattern, s: &Source) -> CstNode {
    let (kind, cs) = match &p.kind {
        PatternKind::Wildcard(id) => (CstKind::WildcardPattern, vec![token(CstRole::Value, *id)]),
        PatternKind::Number(id) => (CstKind::NumberPattern, vec![token(CstRole::Value, *id)]),
        PatternKind::Boolean(id) => (CstKind::BooleanPattern, vec![token(CstRole::Value, *id)]),
        PatternKind::Path { path: p, payload } => {
            let mut cs = vec![child(CstRole::Path, path(p, s))];
            if let Some(d) = payload {
                cs.push(child(
                    CstRole::Payload,
                    list(
                        CstKind::PatternPayload,
                        d,
                        |p| child(CstRole::Pattern, pattern(p, s)),
                        s,
                    ),
                ));
            }
            (CstKind::PathPattern, cs)
        }
    };
    node(kind, Some(p.range), cs, s)
}
fn expr(e: &Expr, s: &Source) -> CstNode {
    let (kind, cs) = match &e.kind {
        ExprKind::Path(p) => (
            CstKind::PathExpression,
            vec![child(CstRole::Path, path(p, s))],
        ),
        ExprKind::Number(id) => (CstKind::NumberExpression, vec![token(CstRole::Value, *id)]),
        ExprKind::String(id) => (CstKind::StringExpression, vec![token(CstRole::Value, *id)]),
        ExprKind::Boolean(id) => (CstKind::BooleanExpression, vec![token(CstRole::Value, *id)]),
        ExprKind::Group(d) => (
            CstKind::GroupExpression,
            vec![
                token(CstRole::Open, d.open),
                child(CstRole::Expression, expr(&d.contents, s)),
                token(CstRole::Close, d.close),
            ],
        ),
        ExprKind::Tuple(d) | ExprKind::Array(d) => {
            let n = list(
                if matches!(e.kind, ExprKind::Tuple(_)) {
                    CstKind::TupleExpression
                } else {
                    CstKind::ArrayExpression
                },
                d,
                |e| child(CstRole::Element, expr(e, s)),
                s,
            );
            (n.kind, n.children)
        }
        ExprKind::Record { path: p, fields } => (
            CstKind::RecordExpression,
            vec![
                child(CstRole::Path, path(p, s)),
                child(
                    CstRole::Body,
                    list(
                        CstKind::Fields,
                        fields,
                        |f| {
                            child(
                                CstRole::Field,
                                node(
                                    CstKind::FieldValue,
                                    Some(f.range),
                                    vec![
                                        token(CstRole::Name, f.kind.name),
                                        token(CstRole::Colon, f.kind.colon),
                                        child(CstRole::Value, expr(&f.kind.value, s)),
                                    ],
                                    s,
                                ),
                            )
                        },
                        s,
                    ),
                ),
            ],
        ),
        ExprKind::Lambda {
            keyword,
            parameters: p,
            result: r,
            body,
        } => {
            let mut cs = vec![
                token(CstRole::Keyword, *keyword),
                child(CstRole::Parameters, parameters(p, s)),
                child(CstRole::Body, block(body, s)),
            ];
            if let Some(r) = r {
                cs.push(child(CstRole::Result, result(r, s)));
            }
            (CstKind::LambdaExpression, cs)
        }
        ExprKind::Unary {
            operator,
            token: id,
            operand,
        } => (
            CstKind::UnaryExpression(*operator),
            vec![
                token(CstRole::Operator, *id),
                child(CstRole::Operand, expr(operand, s)),
            ],
        ),
        ExprKind::Binary {
            operator,
            token: id,
            left,
            right,
        } => (
            CstKind::BinaryExpression(*operator),
            vec![
                child(CstRole::Left, expr(left, s)),
                token(CstRole::Operator, *id),
                child(CstRole::Right, expr(right, s)),
            ],
        ),
        ExprKind::Call {
            callee,
            type_arguments: ta,
            arguments,
        } => {
            let mut cs = vec![
                child(CstRole::Callee, expr(callee, s)),
                child(
                    CstRole::Arguments,
                    list(
                        CstKind::Arguments,
                        arguments,
                        |a| {
                            let mut cs = vec![child(CstRole::Value, expr(&a.kind.value, s))];
                            if let Some(id) = a.kind.permission {
                                cs.push(token(CstRole::Permission, id));
                            }
                            child(
                                CstRole::Argument,
                                node(CstKind::Argument, Some(a.range), cs, s),
                            )
                        },
                        s,
                    ),
                ),
            ];
            if let Some(d) = ta {
                cs.push(child(CstRole::TypeArguments, type_arguments(d, s)));
            }
            (CstKind::CallExpression, cs)
        }
        ExprKind::Field {
            receiver,
            dot,
            name,
        } => (
            CstKind::FieldExpression,
            vec![
                child(CstRole::Receiver, expr(receiver, s)),
                token(CstRole::Dot, *dot),
                token(CstRole::Name, *name),
            ],
        ),
        ExprKind::Index { receiver, index } => (
            CstKind::IndexExpression,
            vec![
                child(CstRole::Receiver, expr(receiver, s)),
                token(CstRole::Open, index.open),
                child(CstRole::Index, expr(&index.contents, s)),
                token(CstRole::Close, index.close),
            ],
        ),
        ExprKind::Propagate { operand, question } => (
            CstKind::PropagateExpression,
            vec![
                child(CstRole::Operand, expr(operand, s)),
                token(CstRole::Question, *question),
            ],
        ),
    };
    node(kind, Some(e.range), cs, s)
}
/// Export retains token IDs even if the input model references an absent piece.
/// `validate` reports those references instead of silently dropping them.
pub fn export(document: &Document, source: &Source) -> CstNode {
    let cs = document
        .declarations
        .iter()
        .map(|d| child(CstRole::Declaration, declaration(d, source)))
        .chain(
            document
                .separators
                .iter()
                .map(|id| token(CstRole::Separator, *id)),
        )
        .collect();
    let mut root = node(CstKind::Document, Some(document.syntax_range), cs, source);
    root.full_range = document.range;
    root
}

#[derive(Clone)]
enum Expected {
    Tokens(Vec<TokenKind>),
    Nodes(Vec<CstKind>),
    Expression,
    Type,
    Statement,
    Declaration,
    Pattern,
}
struct Rule {
    role: CstRole,
    min: usize,
    max: usize,
    expected: Expected,
}
fn rule(role: CstRole, min: usize, max: usize, expected: Expected) -> Rule {
    Rule {
        role,
        min,
        max,
        expected,
    }
}
fn one(role: CstRole, expected: Expected) -> Rule {
    rule(role, 1, 1, expected)
}
fn optional(role: CstRole, expected: Expected) -> Rule {
    rule(role, 0, 1, expected)
}
fn many(role: CstRole, expected: Expected) -> Rule {
    rule(role, 0, usize::MAX, expected)
}
fn tokens(kinds: &[TokenKind]) -> Expected {
    Expected::Tokens(kinds.to_vec())
}
fn nodes(kinds: &[CstKind]) -> Expected {
    Expected::Nodes(kinds.to_vec())
}
fn rules(kind: CstKind) -> Vec<Rule> {
    use CstKind as K;
    use CstRole as R;
    use TokenKind as T;
    let name = || one(R::Name, tokens(&[T::Ident]));
    let body = || one(R::Body, nodes(&[K::Block]));
    let separator = || many(R::Separator, tokens(&[T::Comma]));
    let braces = || {
        vec![
            one(R::Open, tokens(&[T::LBrace])),
            one(R::Close, tokens(&[T::RBrace])),
        ]
    };
    let parens = || {
        vec![
            one(R::Open, tokens(&[T::LParen])),
            one(R::Close, tokens(&[T::RParen])),
        ]
    };
    let angles = || {
        vec![
            one(R::Open, tokens(&[T::TypeOpen, T::GenericOpen])),
            one(R::Close, tokens(&[T::TypeClose, T::GenericClose])),
        ]
    };
    match kind {
        K::MissingClose => vec![],
        K::IncompleteFunction => vec![
            one(R::Prefix, nodes(&[K::FunctionPrefix])),
            many(R::Statement, Expected::Statement),
            many(R::Separator, tokens(&[T::Semicolon, T::Newline])),
            one(R::Remainder, nodes(&[K::Error])),
            one(R::MissingClose, nodes(&[K::MissingClose])),
        ],
        K::FunctionPrefix => vec![
            one(R::Keyword, tokens(&[T::Fn])),
            name(),
            optional(R::Generics, nodes(&[K::GenericParameters])),
            one(R::Parameters, nodes(&[K::Parameters])),
            one(R::Result, nodes(&[K::ResultType])),
            optional(R::Constraints, nodes(&[K::WhereClause])),
            one(R::Open, tokens(&[T::LBrace])),
        ],
        K::Document => vec![
            many(R::Declaration, Expected::Declaration),
            many(R::Separator, tokens(&[T::Semicolon, T::Newline])),
        ],
        K::Function | K::Record | K::Enum => {
            let keyword = match kind {
                K::Function => T::Fn,
                K::Record => T::Record,
                _ => T::Enum,
            };
            let mut r = vec![
                one(R::Keyword, tokens(&[keyword])),
                name(),
                optional(R::Generics, nodes(&[K::GenericParameters])),
                optional(R::Constraints, nodes(&[K::WhereClause])),
            ];
            if kind == K::Function {
                r.extend([
                    one(R::Parameters, nodes(&[K::Parameters])),
                    one(R::Result, nodes(&[K::ResultType])),
                    body(),
                ]);
            } else {
                r.push(one(
                    R::Body,
                    nodes(&[if kind == K::Record {
                        K::Fields
                    } else {
                        K::Variants
                    }]),
                ));
            }
            r
        }
        K::Parameter => vec![
            name(),
            optional(R::Permission, tokens(&[T::Mut])),
            optional(R::Annotation, nodes(&[K::Annotation])),
        ],
        K::Annotation => vec![
            one(R::Colon, tokens(&[T::Colon])),
            one(R::Type, Expected::Type),
        ],
        K::ResultType => vec![
            one(R::Arrow, tokens(&[T::Arrow])),
            one(R::Type, Expected::Type),
        ],
        K::WhereClause => vec![
            one(R::Keyword, tokens(&[T::Where])),
            rule(R::Constraint, 1, usize::MAX, nodes(&[K::Constraint])),
            separator(),
        ],
        K::Constraint => vec![
            one(R::Type, Expected::Type),
            one(R::Colon, tokens(&[T::Colon])),
            rule(R::Bound, 1, usize::MAX, Expected::Type),
            many(R::Separator, tokens(&[T::Plus])),
        ],
        K::RecordField => vec![name(), one(R::Annotation, nodes(&[K::Annotation]))],
        K::EnumVariant => vec![name(), optional(R::Payload, nodes(&[K::PayloadTypes]))],
        K::NamedType => vec![
            one(R::Path, nodes(&[K::Path])),
            optional(R::TypeArguments, nodes(&[K::TypeArguments])),
        ],
        K::TupleType | K::PayloadTypes => {
            let mut r = parens();
            r.extend([many(R::Type, Expected::Type), separator()]);
            r
        }
        K::ArrayType => vec![
            one(R::Open, tokens(&[T::LBracket])),
            one(R::Type, Expected::Type),
            one(R::Close, tokens(&[T::RBracket])),
        ],
        K::Path => vec![
            rule(R::Segment, 1, usize::MAX, tokens(&[T::Ident])),
            many(R::Separator, tokens(&[T::PathSeparator])),
        ],
        K::Block => {
            let mut r = braces();
            r.extend([
                many(R::Statement, Expected::Statement),
                many(R::Separator, tokens(&[T::Semicolon, T::Newline])),
            ]);
            r
        }
        K::GenericParameters => {
            let mut r = angles();
            r.extend([many(R::Parameter, tokens(&[T::Ident])), separator()]);
            r
        }
        K::Parameters => {
            let mut r = parens();
            r.extend([many(R::Parameter, nodes(&[K::Parameter])), separator()]);
            r
        }
        K::TypeArguments => {
            let mut r = angles();
            r.extend([rule(R::Type, 1, usize::MAX, Expected::Type), separator()]);
            r
        }
        K::Fields | K::Variants | K::MatchArms => {
            let mut r = braces();
            r.push(match kind {
                K::Fields => many(R::Field, nodes(&[K::RecordField, K::FieldValue])),
                K::Variants => many(R::Variant, nodes(&[K::EnumVariant])),
                _ => many(R::Arm, nodes(&[K::MatchArm])),
            });
            r.push(separator());
            r
        }
        K::Arguments => {
            let mut r = parens();
            r.extend([many(R::Argument, nodes(&[K::Argument])), separator()]);
            r
        }
        K::LetStatement | K::VarStatement => vec![
            one(
                R::Keyword,
                tokens(&[if kind == K::LetStatement {
                    T::Let
                } else {
                    T::Var
                }]),
            ),
            name(),
            optional(R::Annotation, nodes(&[K::Annotation])),
            one(R::Equal, tokens(&[T::Equal])),
            one(R::Value, Expected::Expression),
        ],
        K::AssignmentStatement => vec![
            one(R::Target, Expected::Expression),
            one(R::Equal, tokens(&[T::Equal])),
            one(R::Value, Expected::Expression),
        ],
        K::ExpressionStatement => vec![one(R::Expression, Expected::Expression)],
        K::ReturnStatement => vec![
            one(R::Keyword, tokens(&[T::Return])),
            optional(R::Value, Expected::Expression),
        ],
        K::IfStatement => vec![
            one(R::Keyword, tokens(&[T::If])),
            one(R::Condition, Expected::Expression),
            one(R::Then, nodes(&[K::Block])),
            optional(R::Else, nodes(&[K::ElseBranch])),
        ],
        K::ElseBranch => vec![
            one(R::Keyword, tokens(&[T::Else])),
            one(R::Body, nodes(&[K::Block, K::IfStatement])),
        ],
        K::ForStatement | K::EveryStatement => vec![
            one(
                R::Keyword,
                tokens(&[if kind == K::ForStatement {
                    T::For
                } else {
                    T::Every
                }]),
            ),
            one(R::Binding, tokens(&[T::Ident])),
            one(R::InKeyword, tokens(&[T::In])),
            one(R::Iterable, Expected::Expression),
            body(),
        ],
        K::MatchStatement => vec![
            one(R::Keyword, tokens(&[T::Match])),
            one(R::Scrutinee, Expected::Expression),
            one(R::Arms, nodes(&[K::MatchArms])),
        ],
        K::MatchArm => vec![
            one(R::Pattern, Expected::Pattern),
            one(R::Arrow, tokens(&[T::FatArrow])),
            body(),
        ],
        K::PathPattern => vec![
            one(R::Path, nodes(&[K::Path])),
            optional(R::Payload, nodes(&[K::PatternPayload])),
        ],
        K::WildcardPattern => vec![one(R::Value, tokens(&[T::Ident]))],
        K::NumberPattern | K::NumberExpression => vec![one(R::Value, tokens(&[T::Number]))],
        K::BooleanPattern | K::BooleanExpression => {
            vec![one(R::Value, tokens(&[T::True, T::False]))]
        }
        K::PatternPayload => {
            let mut r = parens();
            r.extend([many(R::Pattern, Expected::Pattern), separator()]);
            r
        }
        K::PathExpression => vec![one(R::Path, nodes(&[K::Path]))],
        K::StringExpression => vec![one(R::Value, tokens(&[T::String]))],
        K::GroupExpression => {
            let mut r = parens();
            r.push(one(R::Expression, Expected::Expression));
            r
        }
        K::TupleExpression | K::ArrayExpression => {
            let mut r = if kind == K::TupleExpression {
                parens()
            } else {
                vec![
                    one(R::Open, tokens(&[T::LBracket])),
                    one(R::Close, tokens(&[T::RBracket])),
                ]
            };
            r.extend([many(R::Element, Expected::Expression), separator()]);
            r
        }
        K::RecordExpression => vec![
            one(R::Path, nodes(&[K::Path])),
            one(R::Body, nodes(&[K::Fields])),
        ],
        K::LambdaExpression => vec![
            one(R::Keyword, tokens(&[T::Fn])),
            one(R::Parameters, nodes(&[K::Parameters])),
            optional(R::Result, nodes(&[K::ResultType])),
            body(),
        ],
        K::UnaryExpression(op) => vec![
            one(
                R::Operator,
                tokens(&[match op {
                    UnaryOperator::Negate => T::Minus,
                    UnaryOperator::Not => T::Bang,
                }]),
            ),
            one(R::Operand, Expected::Expression),
        ],
        K::BinaryExpression(op) => vec![
            one(R::Left, Expected::Expression),
            one(
                R::Operator,
                tokens(&[match op {
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
                }]),
            ),
            one(R::Right, Expected::Expression),
        ],
        K::CallExpression => vec![
            one(R::Callee, Expected::Expression),
            optional(R::TypeArguments, nodes(&[K::TypeArguments])),
            one(R::Arguments, nodes(&[K::Arguments])),
        ],
        K::FieldExpression => vec![
            one(R::Receiver, Expected::Expression),
            one(R::Dot, tokens(&[T::Dot])),
            name(),
        ],
        K::IndexExpression => vec![
            one(R::Receiver, Expected::Expression),
            one(R::Open, tokens(&[T::LBracket])),
            one(R::Index, Expected::Expression),
            one(R::Close, tokens(&[T::RBracket])),
        ],
        K::PropagateExpression => vec![
            one(R::Operand, Expected::Expression),
            one(R::Question, tokens(&[T::Question])),
        ],
        K::Argument => vec![
            optional(R::Permission, tokens(&[T::Mut])),
            one(R::Value, Expected::Expression),
        ],
        K::FieldValue => vec![
            name(),
            one(R::Colon, tokens(&[T::Colon])),
            one(R::Value, Expected::Expression),
        ],
        K::Error => vec![many(R::ErrorToken, tokens(&[]))],
    }
}
fn accepts(expected: &Expected, element: &CstElement, source: &Source) -> bool {
    use CstKind as K;
    match (expected, element) {
        (Expected::Tokens(allowed), CstElement::Token(id)) => {
            source.pieces.get(id.0).is_some_and(|p| match p.kind {
                PieceKind::Token(k) => allowed.contains(&k),
                PieceKind::Trivia(crate::source::TriviaKind::Newline) => {
                    allowed.contains(&TokenKind::Newline)
                }
                PieceKind::Trivia(crate::source::TriviaKind::BlockComment) => {
                    allowed.contains(&TokenKind::Newline)
                        && p.bytes.iter().any(|b| matches!(b, b'\r' | b'\n'))
                }
                _ => false,
            })
        }
        (Expected::Nodes(allowed), CstElement::Node(n)) => allowed.contains(&n.kind),
        (Expected::Type, CstElement::Node(n)) => {
            matches!(n.kind, K::NamedType | K::TupleType | K::ArrayType)
        }
        (Expected::Declaration, CstElement::Node(n)) => matches!(
            n.kind,
            K::Function | K::Record | K::Enum | K::IncompleteFunction | K::Error
        ),
        (Expected::Statement, CstElement::Node(n)) => matches!(
            n.kind,
            K::LetStatement
                | K::VarStatement
                | K::AssignmentStatement
                | K::ExpressionStatement
                | K::ReturnStatement
                | K::IfStatement
                | K::ForStatement
                | K::EveryStatement
                | K::MatchStatement
                | K::Error
        ),
        (Expected::Pattern, CstElement::Node(n)) => matches!(
            n.kind,
            K::PathPattern | K::WildcardPattern | K::NumberPattern | K::BooleanPattern
        ),
        (Expected::Expression, CstElement::Node(n)) => matches!(
            n.kind,
            K::PathExpression
                | K::NumberExpression
                | K::StringExpression
                | K::BooleanExpression
                | K::GroupExpression
                | K::TupleExpression
                | K::ArrayExpression
                | K::RecordExpression
                | K::LambdaExpression
                | K::UnaryExpression(_)
                | K::BinaryExpression(_)
                | K::CallExpression
                | K::FieldExpression
                | K::IndexExpression
                | K::PropagateExpression
        ),
        _ => false,
    }
}
fn issue(n: &CstNode, message: impl Into<String>, errors: &mut Vec<StructuralError>) {
    errors.push(StructuralError {
        kind: n.kind,
        range: n.range,
        message: message.into(),
    });
}
fn direct_node(n: &CstNode, role: CstRole) -> Option<&CstNode> {
    n.children.iter().find_map(|c| {
        if c.role == role {
            if let CstElement::Node(n) = &c.element {
                Some(n.as_ref())
            } else {
                None
            }
        } else {
            None
        }
    })
}
fn validate_node(
    n: &CstNode,
    source: &Source,
    text: Option<&str>,
    ownership: &mut [usize],
    errors: &mut Vec<StructuralError>,
    is_root: bool,
) {
    if matches!(
        n.kind,
        CstKind::Error
            | CstKind::IncompleteFunction
            | CstKind::FunctionPrefix
            | CstKind::MissingClose
    ) {
        issue(n, "error or incomplete syntax cannot be admitted", errors);
    }
    for (label, r) in [("syntax", n.range), ("full", n.full_range)] {
        if r.start > r.end || r.end > source.len() {
            issue(n, format!("invalid {label} range"), errors);
        } else if text.is_some_and(|t| !t.is_char_boundary(r.start) || !t.is_char_boundary(r.end)) {
            issue(n, format!("{label} range splits a UTF-8 character"), errors);
        }
    }
    if !n.full_range.contains(n.range) {
        issue(n, "full range does not contain syntax range", errors);
    }
    if is_root {
        if n.kind != CstKind::Document || n.full_range != ByteRange::new(0, source.len()) {
            issue(
                n,
                "root must be Document with full physical source range",
                errors,
            );
        }
    } else if n.full_range != n.range {
        issue(
            n,
            "non-root full range must equal syntax range under this version's trivia policy",
            errors,
        );
    }
    let rules = rules(n.kind);
    for r in &rules {
        let count = n.children.iter().filter(|c| c.role == r.role).count();
        if count < r.min || count > r.max {
            issue(
                n,
                format!(
                    "{:?} requires {}..={} children, found {count}",
                    r.role, r.min, r.max
                ),
                errors,
            );
        }
    }
    let mut previous_end = None;
    let mut cover = None::<ByteRange>;
    for c in &n.children {
        match rules.iter().find(|r| r.role == c.role) {
            Some(r) if accepts(&r.expected, &c.element, source) => {}
            Some(_) => issue(
                n,
                format!("invalid element or token kind for {:?}", c.role),
                errors,
            ),
            None => issue(n, format!("unexpected child role {:?}", c.role), errors),
        }
        let r = child_range(c, source);
        if previous_end.is_some_and(|end| end > r.start) {
            issue(n, "children overlap or are out of source order", errors);
        }
        previous_end = Some(r.end);
        let containing = if is_root { n.full_range } else { n.range };
        if !containing.contains(r) {
            issue(n, "child is outside containing range", errors);
        }
        if !is_root || c.role == CstRole::Declaration {
            cover = Some(cover.map_or(r, |cover| cover.cover(r)));
        }
        match &c.element {
            CstElement::Token(id) => {
                if let Some(count) = ownership.get_mut(id.0) {
                    *count += 1;
                } else {
                    issue(n, format!("missing token piece {}", id.0), errors);
                }
            }
            CstElement::Node(child) => {
                if !n.full_range.contains(child.full_range) {
                    issue(n, "child full range is outside parent full range", errors);
                }
                validate_node(child, source, text, ownership, errors, false);
            }
        }
    }
    if let Some(cover) = cover {
        if n.range != cover {
            issue(
                n,
                "syntax range must exactly cover its syntax children",
                errors,
            );
        }
    } else if n.range.start != n.range.end {
        issue(n, "childless node must have an empty syntax range", errors);
    }
    validate_relationships(n, source, errors);
}
fn validate_relationships(n: &CstNode, source: &Source, errors: &mut Vec<StructuralError>) {
    use CstKind as K;
    use CstRole as R;
    let count = |role| n.children.iter().filter(|c| c.role == role).count();
    if n.kind == K::WildcardPattern {
        let valid = n
            .children
            .iter()
            .find(|c| c.role == R::Value)
            .is_some_and(|c| {
                if let CstElement::Token(id) = c.element {
                    source.pieces.get(id.0).is_some_and(|p| p.bytes == b"_")
                } else {
                    false
                }
            });
        if !valid {
            issue(
                n,
                "wildcard pattern must reference the underscore token",
                errors,
            );
        }
    }

    let list_role = match n.kind {
        K::Parameters | K::GenericParameters => Some(R::Parameter),
        K::TypeArguments | K::TupleType | K::PayloadTypes => Some(R::Type),
        K::Fields => Some(R::Field),
        K::Variants => Some(R::Variant),
        K::MatchArms => Some(R::Arm),
        K::Arguments => Some(R::Argument),
        K::TupleExpression | K::ArrayExpression => Some(R::Element),
        K::PatternPayload => Some(R::Pattern),
        K::WhereClause => Some(R::Constraint),
        _ => None,
    };
    if let Some(role) = list_role {
        let items = count(role);
        let separators = count(R::Separator);
        if separators < items.saturating_sub(1) || separators > items {
            issue(n, "list separator count disagrees with item count", errors);
        }
        let mut seen_item = false;
        let mut after_separator = false;
        for c in &n.children {
            if c.role == role {
                if seen_item && !after_separator {
                    issue(n, "list items require intervening separators", errors);
                }
                seen_item = true;
                after_separator = false;
            } else if c.role == R::Separator {
                if !seen_item || after_separator {
                    issue(n, "misplaced list separator", errors);
                }
                after_separator = true;
            }
        }
    }
    if n.kind == K::Path {
        if count(R::Separator) + 1 != count(R::Segment) {
            issue(n, "path requires a separator between every segment", errors);
        }
        let expected: Vec<_> = (0..n.children.len())
            .map(|i| if i % 2 == 0 { R::Segment } else { R::Separator })
            .collect();
        if !n.children.iter().map(|c| c.role).eq(expected) {
            issue(n, "path roles must alternate segment and separator", errors);
        }
    }
    if n.kind == K::Constraint && count(R::Separator) + 1 != count(R::Bound) {
        issue(n, "constraint bounds require plus separators", errors);
    }
    if let (Some(open), Some(close)) = (
        n.children.iter().find(|c| c.role == R::Open),
        n.children.iter().find(|c| c.role == R::Close),
    ) {
        if (n.children.first() != Some(open) || n.children.last() != Some(close))
            && !matches!(n.kind, K::IndexExpression)
        {
            issue(n, "delimiters must surround all contents", errors);
        }
        if let (CstElement::Token(a), CstElement::Token(b)) = (&open.element, &close.element)
            && let (Some(a), Some(b)) = (source.pieces.get(a.0), source.pieces.get(b.0))
            && matches!(
                (a.kind, b.kind),
                (
                    PieceKind::Token(TokenKind::TypeOpen),
                    PieceKind::Token(TokenKind::GenericClose)
                ) | (
                    PieceKind::Token(TokenKind::GenericOpen),
                    PieceKind::Token(TokenKind::TypeClose)
                )
            )
        {
            issue(n, "generic delimiter classifications disagree", errors);
        }
    }
    if n.kind == K::Function
        && let Some(parameters) = direct_node(n, R::Parameters)
    {
        for p in &parameters.children {
            if let CstElement::Node(p) = &p.element
                && p.kind == K::Parameter
                && direct_node(p, R::Annotation).is_none()
            {
                issue(
                    p,
                    "named function parameter requires a type annotation",
                    errors,
                );
            }
        }
    }
    if matches!(n.kind, K::Record | K::RecordExpression)
        && let Some(fields) = direct_node(n, R::Body)
    {
        let expected = if n.kind == K::Record {
            K::RecordField
        } else {
            K::FieldValue
        };
        for c in &fields.children {
            if c.role == R::Field
                && let CstElement::Node(field) = &c.element
                && field.kind != expected
            {
                issue(
                    field,
                    "field form disagrees with its enclosing record",
                    errors,
                );
            }
        }
    }
    // Source order alone cannot identify a role swap: require domain ordering too.
    let order: Option<&[R]> = match n.kind {
        K::Function => Some(&[
            R::Keyword,
            R::Name,
            R::Generics,
            R::Parameters,
            R::Result,
            R::Constraints,
            R::Body,
        ]),
        K::Record | K::Enum => Some(&[R::Keyword, R::Name, R::Generics, R::Constraints, R::Body]),
        K::Parameter => Some(&[R::Permission, R::Name, R::Annotation]),
        K::Annotation => Some(&[R::Colon, R::Type]),
        K::ResultType => Some(&[R::Arrow, R::Type]),
        K::LetStatement | K::VarStatement => {
            Some(&[R::Keyword, R::Name, R::Annotation, R::Equal, R::Value])
        }
        K::AssignmentStatement => Some(&[R::Target, R::Equal, R::Value]),
        K::ReturnStatement => Some(&[R::Keyword, R::Value]),
        K::IfStatement => Some(&[R::Keyword, R::Condition, R::Then, R::Else]),
        K::ElseBranch => Some(&[R::Keyword, R::Body]),
        K::ForStatement | K::EveryStatement => {
            Some(&[R::Keyword, R::Binding, R::InKeyword, R::Iterable, R::Body])
        }
        K::MatchStatement => Some(&[R::Keyword, R::Scrutinee, R::Arms]),
        K::MatchArm => Some(&[R::Pattern, R::Arrow, R::Body]),
        K::NamedType => Some(&[R::Path, R::TypeArguments]),
        K::PathPattern => Some(&[R::Path, R::Payload]),
        K::RecordExpression => Some(&[R::Path, R::Body]),
        K::UnaryExpression(_) => Some(&[R::Operator, R::Operand]),
        K::BinaryExpression(_) => Some(&[R::Left, R::Operator, R::Right]),
        K::CallExpression => Some(&[R::Callee, R::TypeArguments, R::Arguments]),
        K::FieldExpression => Some(&[R::Receiver, R::Dot, R::Name]),
        K::IndexExpression => Some(&[R::Receiver, R::Open, R::Index, R::Close]),
        K::PropagateExpression => Some(&[R::Operand, R::Question]),
        K::Argument => Some(&[R::Permission, R::Value]),
        K::FieldValue => Some(&[R::Name, R::Colon, R::Value]),
        K::RecordField => Some(&[R::Name, R::Annotation]),
        K::EnumVariant => Some(&[R::Name, R::Payload]),
        K::LambdaExpression => Some(&[R::Keyword, R::Parameters, R::Result, R::Body]),
        _ => None,
    };
    if let Some(order) = order {
        let indices: Vec<_> = n
            .children
            .iter()
            .filter_map(|c| order.iter().position(|r| *r == c.role))
            .collect();
        if indices.windows(2).any(|w| w[0] >= w[1]) {
            issue(n, "child roles violate syntax order", errors);
        }
    }
}
/// Structural admission gate. Checks typed roles, markers, positions and exact significant-token ownership.
pub fn validate(cst: &CstNode, source: &Source) -> Result<(), Vec<StructuralError>> {
    let mut errors = vec![];
    if !source.validate() {
        issue(
            cst,
            "source tape is not contiguous or piece lengths disagree",
            &mut errors,
        );
    }
    let bytes = source.render();
    let text = std::str::from_utf8(&bytes).ok();
    if text.is_none() {
        issue(cst, "invalid UTF-8 source cannot be admitted", &mut errors);
    }
    let mut ownership = vec![0; source.pieces.len()];
    validate_node(cst, source, text, &mut ownership, &mut errors, true);
    for (i, p) in source.pieces.iter().enumerate() {
        match p.kind {
            PieceKind::Token(TokenKind::Newline) | PieceKind::Trivia(_) => {
                if ownership[i] > 1 {
                    issue(
                        cst,
                        format!("source piece {i} is referenced more than once"),
                        &mut errors,
                    );
                }
            }
            PieceKind::Token(_) => {
                if ownership[i] != 1 {
                    issue(
                        cst,
                        format!(
                            "significant token piece {i} requires exactly one owner, found {}",
                            ownership[i]
                        ),
                        &mut errors,
                    );
                }
            }
            PieceKind::Invalid | PieceKind::Unparsed => issue(
                cst,
                format!("invalid or unparsed source piece {i} cannot be admitted"),
                &mut errors,
            ),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid(bytes: &[u8]) -> (Source, CstNode) {
        let parsed = crate::parse(bytes);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let tree = parsed.cst();
        assert_eq!(validate(&tree, &parsed.source), Ok(()));
        (parsed.source, tree)
    }
    fn find(n: &mut CstNode, kind: CstKind) -> Option<&mut CstNode> {
        if n.kind == kind {
            return Some(n);
        }
        for c in &mut n.children {
            if let CstElement::Node(child) = &mut c.element
                && let Some(found) = find(child, kind)
            {
                return Some(found);
            }
        }
        None
    }
    #[test]
    fn recursive_roles_markers_and_source_ranges() {
        let(source,tree)=valid(b"// lead\nfn update(mut value: Thing) -> Thing { var result = change(mut value)?; for entry in result { return entry }; return result } // tail\n");
        assert_eq!(tree.full_range, ByteRange::new(0, source.len()));
        assert!(tree.range.start > 0);
        assert!(tree.range.end < tree.full_range.end);
        assert!(
            tree.children
                .iter()
                .any(|c| matches!(&c.element,CstElement::Node(n) if n.kind==CstKind::Function))
        );
    }
    #[test]
    fn absent_callee_and_markers_are_rejected() {
        let (source, tree) =
            valid(b"fn update(value: Thing) -> Thing { return change(mut value)? }");
        for (kind, role) in [
            (CstKind::CallExpression, CstRole::Callee),
            (CstKind::Argument, CstRole::Permission),
            (CstKind::PropagateExpression, CstRole::Question),
        ] {
            let mut erased = tree.clone();
            find(&mut erased, kind)
                .unwrap()
                .children
                .retain(|c| c.role != role);
            assert!(
                validate(&erased, &source).is_err(),
                "erased {kind:?} {role:?}"
            );
        }
    }
    #[test]
    fn impossible_ranges_token_references_and_flattening_are_rejected() {
        let (source, tree) = valid(b"fn value() -> Thing { return 1 }");
        let mut broken = tree.clone();
        find(&mut broken, CstKind::Function).unwrap().range = ByteRange::new(5, 2);
        assert!(validate(&broken, &source).is_err());
        let mut broken = tree.clone();
        find(&mut broken, CstKind::Function).unwrap().children[0].element =
            CstElement::Token(TokenId(source.pieces.len()));
        assert!(validate(&broken, &source).is_err());
        let mut flat = tree.clone();
        flat.children = source
            .pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.kind, PieceKind::Token(_)))
            .map(|(i, _)| token(CstRole::Declaration, TokenId(i)))
            .collect();
        assert!(validate(&flat, &source).is_err());
    }
    #[test]
    fn token_kinds_and_operator_semantics_are_checked() {
        let (source, tree) = valid(b"fn value() -> Thing { return 1 + 2 }");
        let mut swapped = tree.clone();
        find(&mut swapped, CstKind::BinaryExpression(BinaryOperator::Add))
            .unwrap()
            .kind = CstKind::BinaryExpression(BinaryOperator::Multiply);
        assert!(validate(&swapped, &source).is_err());
        let mut swapped = tree.clone();
        let f = find(&mut swapped, CstKind::Function).unwrap();
        f.children.swap(0, 1);
        assert!(validate(&swapped, &source).is_err());
    }
    #[test]
    fn newline_bearing_comment_is_a_real_statement_separator() {
        let (source, tree) =
            valid(b"fn value() -> Thing { let x = 1 /* retained\n comment */ return x }");
        assert_eq!(
            source.render(),
            b"fn value() -> Thing { let x = 1 /* retained\n comment */ return x }"
        );
        assert_eq!(validate(&tree, &source), Ok(()));
    }
    #[test]
    fn wildcard_pattern_preserves_its_marker_and_rejects_other_names() {
        let (mut source, mut tree) =
            valid(b"fn check(value: Thing) -> Thing { match value { _ => { return value } } }");
        let wildcard = find(&mut tree, CstKind::WildcardPattern).unwrap();
        let CstElement::Token(id) = wildcard.children[0].element else {
            panic!("wildcard must be a token");
        };
        source.pieces[id.0].bytes = b"x".to_vec();
        assert!(source.validate());
        assert!(validate(&tree, &source).is_err());
    }
    #[test]
    fn typed_schema_rejects_unknown_kinds() {
        assert!(serde_json::from_str::<CstKind>("\"ImaginaryExpression\"").is_err());
    }
    #[test]
    fn trivia_only_document_uses_physical_full_range() {
        let (source, tree) = valid(b" // only trivia\r\n  ");
        assert_eq!(tree.full_range, ByteRange::new(0, source.len()));
        assert_eq!(tree.range, ByteRange::empty(source.len()));
    }
}
