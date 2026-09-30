//! Typed syntax. Leaves reference the source tape; they never own source bytes.
use crate::source::ByteRange;
use crate::token::TokenId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node<T> {
    pub range: ByteRange,
    pub kind: T,
}
impl<T> Node<T> {
    pub fn new(start: usize, end: usize, kind: T) -> Self {
        Self {
            range: ByteRange { start, end },
            kind,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delimited<T> {
    pub open: TokenId,
    pub contents: T,
    pub close: TokenId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Separated<T> {
    pub items: Vec<T>,
    pub separators: Vec<TokenId>,
}
impl<T> Default for Separated<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            separators: Vec::new(),
        }
    }
}
impl<T> Separated<T> {
    pub fn prepend(item: T, comma: Option<TokenId>, mut rest: Self) -> Self {
        rest.items.insert(0, item);
        if let Some(comma) = comma {
            rest.separators.insert(0, comma);
        }
        rest
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub range: ByteRange,
    pub syntax_range: ByteRange,
    pub declarations: Vec<Node<Declaration>>,
    pub separators: Vec<TokenId>,
}
impl Document {
    pub fn has_errors(&self) -> bool {
        self.declarations.iter().any(|d| d.kind.has_errors())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Declaration {
    Function(Function),
    Record(Record),
    Enum(Enum),
    IncompleteFunction(IncompleteFunction),
    Error(ErrorSyntax),
}
impl Declaration {
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Function(f) => f.body.has_errors(),
            Self::Error(_) | Self::IncompleteFunction(_) => true,
            _ => false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    pub keyword: TokenId,
    pub name: TokenId,
    pub generics: Option<Delimited<Separated<TokenId>>>,
    pub parameters: Delimited<Separated<Node<Parameter>>>,
    pub result: ResultType,
    pub constraints: Option<WhereClause>,
    pub body: Block,
}
/// A recognized declaration prefix whose closing body delimiter is missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionPrefix {
    pub keyword: TokenId,
    pub name: TokenId,
    pub generics: Option<Delimited<Separated<TokenId>>>,
    pub parameters: Delimited<Separated<Node<Parameter>>>,
    pub result: ResultType,
    pub constraints: Option<WhereClause>,
    pub open: TokenId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompleteFunction {
    pub prefix: Node<FunctionPrefix>,
    pub statements: Vec<Statement>,
    pub separators: Vec<TokenId>,
    pub remainder: ErrorSyntax,
    pub missing_close: ByteRange,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub permission: Option<TokenId>,
    pub name: TokenId,
    pub annotation: Option<Annotation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub colon: TokenId,
    pub ty: Type,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultType {
    pub arrow: TokenId,
    pub ty: Type,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhereClause {
    pub keyword: TokenId,
    pub constraints: Separated<Node<Constraint>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraint {
    pub parameter: Type,
    pub colon: TokenId,
    pub bounds: Separated<Type>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub keyword: TokenId,
    pub name: TokenId,
    pub generics: Option<Delimited<Separated<TokenId>>>,
    pub constraints: Option<WhereClause>,
    pub fields: Delimited<Separated<Node<RecordField>>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordField {
    pub name: TokenId,
    pub annotation: Annotation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enum {
    pub keyword: TokenId,
    pub name: TokenId,
    pub generics: Option<Delimited<Separated<TokenId>>>,
    pub constraints: Option<WhereClause>,
    pub variants: Delimited<Separated<Node<EnumVariant>>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: TokenId,
    pub payload: Option<Delimited<Separated<Type>>>,
}
pub type Type = Node<TypeKind>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    Named {
        path: Path,
        arguments: Option<Delimited<Separated<Type>>>,
    },
    Tuple(Delimited<Separated<Type>>),
    Array(Delimited<Box<Type>>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub segments: Vec<TokenId>,
    pub separators: Vec<TokenId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub range: ByteRange,
    pub open: TokenId,
    pub statements: Vec<Statement>,
    pub separators: Vec<TokenId>,
    pub close: TokenId,
}
impl Block {
    pub fn has_errors(&self) -> bool {
        self.statements.iter().any(|s| s.kind.has_errors())
    }
}
pub type Statement = Node<StatementKind>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatementKind {
    Local {
        binding: Binding,
        name: TokenId,
        annotation: Option<Annotation>,
        equal: TokenId,
        value: Expr,
    },
    Assign {
        target: Expr,
        equal: TokenId,
        value: Expr,
    },
    Expression(Expr),
    Return {
        keyword: TokenId,
        value: Option<Expr>,
    },
    If {
        keyword: TokenId,
        condition: Expr,
        then_block: Block,
        otherwise: Option<ElseBranch>,
    },
    Iterate {
        iteration: Iteration,
        binding: TokenId,
        in_keyword: TokenId,
        iterable: Expr,
        body: Block,
    },
    Match {
        keyword: TokenId,
        scrutinee: Expr,
        arms: Delimited<Separated<Node<MatchArm>>>,
    },
    Error(ErrorSyntax),
}
impl StatementKind {
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Error(_) => true,
            Self::If {
                condition,
                then_block,
                otherwise,
                ..
            } => {
                condition.kind.has_errors()
                    || then_block.has_errors()
                    || otherwise.as_ref().is_some_and(|b| match &b.body {
                        ElseBody::Block(b) => b.has_errors(),
                        ElseBody::If(s) => s.kind.has_errors(),
                    })
            }
            Self::Iterate { iterable, body, .. } => iterable.kind.has_errors() || body.has_errors(),
            Self::Match {
                scrutinee, arms, ..
            } => {
                scrutinee.kind.has_errors()
                    || arms.contents.items.iter().any(|a| a.kind.body.has_errors())
            }
            Self::Assign { target, value, .. } => {
                target.kind.has_errors() || value.kind.has_errors()
            }
            Self::Local { value, .. } | Self::Expression(value) => value.kind.has_errors(),
            Self::Return { value, .. } => value.as_ref().is_some_and(|e| e.kind.has_errors()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Binding {
    Let(TokenId),
    Var(TokenId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Iteration {
    For(TokenId),
    Every(TokenId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElseBranch {
    pub keyword: TokenId,
    pub body: ElseBody,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElseBody {
    Block(Block),
    If(Box<Statement>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub arrow: TokenId,
    pub body: Block,
}
pub type Pattern = Node<PatternKind>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternKind {
    Wildcard(TokenId),
    Path {
        path: Path,
        payload: Option<Delimited<Separated<Pattern>>>,
    },
    Number(TokenId),
    Boolean(TokenId),
}
pub type Expr = Node<ExprKind>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExprKind {
    Path(Path),
    Number(TokenId),
    String(TokenId),
    Boolean(TokenId),
    Group(Delimited<Box<Expr>>),
    Tuple(Delimited<Separated<Expr>>),
    Array(Delimited<Separated<Expr>>),
    Record {
        path: Path,
        fields: Delimited<Separated<Node<FieldValue>>>,
    },
    Lambda {
        keyword: TokenId,
        parameters: Delimited<Separated<Node<Parameter>>>,
        result: Option<ResultType>,
        body: Block,
    },
    Unary {
        operator: UnaryOperator,
        token: TokenId,
        operand: Box<Expr>,
    },
    Binary {
        operator: BinaryOperator,
        token: TokenId,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        type_arguments: Option<Delimited<Separated<Type>>>,
        arguments: Delimited<Separated<Node<Argument>>>,
    },
    Field {
        receiver: Box<Expr>,
        dot: TokenId,
        name: TokenId,
    },
    Index {
        receiver: Box<Expr>,
        index: Delimited<Box<Expr>>,
    },
    Propagate {
        operand: Box<Expr>,
        question: TokenId,
    },
}
impl ExprKind {
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Lambda { body, .. } => body.has_errors(),
            Self::Group(d) => d.contents.kind.has_errors(),
            Self::Tuple(d) | Self::Array(d) => d.contents.items.iter().any(|x| x.kind.has_errors()),
            Self::Record { fields, .. } => fields
                .contents
                .items
                .iter()
                .any(|f| f.kind.value.kind.has_errors()),
            Self::Unary { operand, .. } | Self::Propagate { operand, .. } => {
                operand.kind.has_errors()
            }
            Self::Binary { left, right, .. } => left.kind.has_errors() || right.kind.has_errors(),
            Self::Call {
                callee, arguments, ..
            } => {
                callee.kind.has_errors()
                    || arguments
                        .contents
                        .items
                        .iter()
                        .any(|a| a.kind.value.kind.has_errors())
            }
            Self::Field { receiver, .. } => receiver.kind.has_errors(),
            Self::Index { receiver, index } => {
                receiver.kind.has_errors() || index.contents.kind.has_errors()
            }
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Negate,
    Not,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Multiply,
    Divide,
    Remainder,
    Add,
    Subtract,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Argument {
    pub permission: Option<TokenId>,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValue {
    pub name: TokenId,
    pub colon: TokenId,
    pub value: Expr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSyntax {
    pub tokens: Vec<TokenId>,
    pub expected: Vec<String>,
}

pub fn binary(left: Expr, operator: (BinaryOperator, TokenId), right: Expr) -> Expr {
    Node::new(
        left.range.start,
        right.range.end,
        ExprKind::Binary {
            operator: operator.0,
            token: operator.1,
            left: Box::new(left),
            right: Box::new(right),
        },
    )
}
#[derive(Clone, Debug)]
pub enum Postfix {
    Call(Delimited<Separated<Node<Argument>>>),
    Field(TokenId, TokenId, usize),
    Index(Delimited<Box<Expr>>, usize),
    Propagate(TokenId, usize),
}
pub fn postfix(
    mut expression: Expr,
    tails: Vec<Postfix>,
    end_of: impl Fn(TokenId) -> usize,
) -> Expr {
    for tail in tails {
        let start = expression.range.start;
        expression = match tail {
            Postfix::Call(arguments) => Node::new(
                start,
                end_of(arguments.close),
                ExprKind::Call {
                    callee: Box::new(expression),
                    type_arguments: None,
                    arguments,
                },
            ),
            Postfix::Field(dot, name, end) => Node::new(
                start,
                end,
                ExprKind::Field {
                    receiver: Box::new(expression),
                    dot,
                    name,
                },
            ),
            Postfix::Index(index, end) => Node::new(
                start,
                end,
                ExprKind::Index {
                    receiver: Box::new(expression),
                    index,
                },
            ),
            Postfix::Propagate(question, end) => Node::new(
                start,
                end,
                ExprKind::Propagate {
                    operand: Box::new(expression),
                    question,
                },
            ),
        };
    }
    expression
}
