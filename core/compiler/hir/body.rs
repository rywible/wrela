use crate::hir::arena::{Arena, Idx};
use crate::hir::TypeRef;
use rowan::TextRange;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub root_stmts: Vec<Idx<Stmt>>,
    pub expr_spans: Vec<TextRange>,
    pub stmt_spans: Vec<TextRange>,
}

impl Body {
    pub fn expr_span(&self, idx: Idx<Expr>) -> TextRange {
        self.expr_spans
            .get(idx.into_raw())
            .cloned()
            .unwrap_or_else(|| TextRange::empty(0.into()))
    }

    pub fn stmt_span(&self, idx: Idx<Stmt>) -> TextRange {
        self.stmt_spans
            .get(idx.into_raw())
            .cloned()
            .unwrap_or_else(|| TextRange::empty(0.into()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Idx<Expr>),
    Assert {
        kind: AssertKind,
        expr: Idx<Expr>,
    },
    Require {
        condition: Idx<Expr>,
        message: Idx<Expr>,
    },
    Let {
        name: SmolStr,
        value: Idx<Expr>,
        mutable: bool,
        visibility: Option<crate::hir::Visibility>,
    },
    Assign {
        name: SmolStr,
        op: AssignOp,
        value: Idx<Expr>,
        mutable: bool,
        visibility: Option<crate::hir::Visibility>,
    },
    Optimize {
        objective: Objective,
        body: Vec<Idx<Stmt>>,
    },
    If {
        condition: Idx<Expr>,
        then_branch: Vec<Idx<Stmt>>,
        else_branch: Option<Vec<Idx<Stmt>>>,
    },
    For {
        name: SmolStr,
        iterable: Idx<Expr>,
        body: Vec<Idx<Stmt>>,
    },
    Match {
        subject: Idx<Expr>,
        cases: Vec<MatchCase>,
        otherwise: Option<Vec<Idx<Stmt>>>,
    },
    IgnoreResult {
        expr: Idx<Expr>,
    },
    Capture {
        name: SmolStr,
        value: Idx<Expr>,
    },
    Defer {
        expr: Idx<Expr>,
    },
    Use {
        names: Vec<UseName>,
        module: SmolStr,
    },
    While {
        condition: Idx<Expr>,
        body: Vec<Idx<Stmt>>,
    },
    Return(Option<Idx<Expr>>),
    Break,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertKind {
    Value,
    Identity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub labels: Vec<Pattern>,
    pub body: Vec<Idx<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Binding(SmolStr),
    Literal(Literal),
    Path {
        parts: Vec<SmolStr>,
        args: Vec<Pattern>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Variable(SmolStr),
    Detach {
        target: Idx<Expr>,
        size: PoolSize,
        objective: Option<Objective>,
    },
    Binary {
        lhs: Idx<Expr>,
        op: BinaryOp,
        rhs: Idx<Expr>,
        op_span: TextRange,
    },
    Unary {
        op: UnaryOp,
        expr: Idx<Expr>,
        op_span: TextRange,
    },
    TypeApply {
        callee: Idx<Expr>,
        type_args: Vec<TypeRef>,
    },
    Crash {
        expr: Idx<Expr>,
    },
    Call {
        callee: Idx<Expr>,
        args: Vec<Arg>,
        type_args: Vec<TypeRef>,
    },
    GivenCall {
        callee: Idx<Expr>,
        args: Vec<Arg>,
        type_args: Vec<TypeRef>,
    },
    Member {
        object: Idx<Expr>,
        member: SmolStr,
        member_span: TextRange,
    },
    List(Vec<Idx<Expr>>),
    Map(Vec<(Idx<Expr>, Idx<Expr>)>),
    StringInterp(Vec<StringPart>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(SmolStr),
    Boolean(bool),
    Nil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolSize {
    Fixed(i64),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objective {
    Latency,
    Throughput,
    Conservation,
    Balance,
}

impl Objective {
    pub fn from_str(name: &str) -> Option<Self> {
        match name {
            "latency" => Some(Objective::Latency),
            "throughput" => Some(Objective::Throughput),
            "conservation" => Some(Objective::Conservation),
            "balance" => Some(Objective::Balance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Otherwise,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Range,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    Await,
    Spawn,
    Fire,
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Positional {
        value: Idx<Expr>,
        span: TextRange,
    },
    Named {
        name: SmolStr,
        value: Idx<Expr>,
        span: TextRange,
        name_span: TextRange,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(SmolStr),
    Expr(Idx<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseNameKind {
    Name(SmolStr),
    Glob,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseName {
    pub kind: UseNameKind,
    pub span: TextRange,
}

impl UseName {
    pub fn name(&self) -> Option<&SmolStr> {
        match &self.kind {
            UseNameKind::Name(name) => Some(name),
            UseNameKind::Glob => None,
        }
    }
}
