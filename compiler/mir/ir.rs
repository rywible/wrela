use crate::hir::{BinaryOp, Literal, Objective, PoolSize, UnaryOp};
use rowan::TextRange;
use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeTagId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
    pub type_tags: Vec<SmolStr>,
    pub classes: Vec<MirClassInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirClassInfo {
    pub name: SmolStr,
    pub id: TypeTagId,
    pub fields: Vec<SmolStr>,
    pub methods: Vec<MirMethodInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirMethodInfo {
    pub name: SmolStr,
    pub func: SmolStr,
    pub arity: usize,
    pub id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirFunction {
    pub name: SmolStr,
    pub params: Vec<LocalId>,
    pub locals: Vec<Local>,
    pub temps: Vec<Temp>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
    pub suspendable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    pub name: SmolStr,
    pub mutable: bool,
    pub ty: MirType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Temp {
    pub ty: MirType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub stmts: Vec<Stmt>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MirType {
    Unknown,
    Integer,
    Float,
    Boolean,
    String,
    Nil,
    Named(SmolStr),
    Result(Box<MirType>, Box<MirType>),
    Actor(Box<MirType>),
    Pending(Box<MirType>),
}

impl MirType {
    pub fn is_ref(&self) -> bool {
        match self {
            MirType::Integer | MirType::Float | MirType::Boolean | MirType::Nil => false,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Const(Literal),
    Local(LocalId),
    Temp(TempId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    Local(LocalId),
    Temp(TempId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallTarget {
    Function(SmolStr),
    Method {
        receiver: Value,
        method: SmolStr,
        method_id: Option<u32>,
    },
    GuardedInterface {
        fast_paths: Vec<(TypeTagId, SmolStr)>,
        fallback: SmolStr,
    },
    Indirect(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Sync,
    Actor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPartValue {
    Literal(SmolStr),
    Value(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocKind {
    LocalTemp,
    Escaping,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    Use(Value),
    Unary {
        op: UnaryOp,
        operand: Value,
    },
    Binary {
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
    },
    StrConcat {
        parts: Vec<Value>,
        alloc: AllocKind,
    },
    ResultOk {
        value: Value,
    },
    ResultErr {
        value: Value,
    },
    ResultIsOk {
        value: Value,
    },
    ResultUnwrap {
        value: Value,
    },
    ResultErrUnwrap {
        value: Value,
    },
    Crash {
        value: Value,
    },
    GetField {
        base: Value,
        field: SmolStr,
        slot: Option<u32>,
    },
    Call {
        kind: CallKind,
        target: CallTarget,
        args: Vec<Value>,
    },
    ClassInit {
        class_id: u32,
        fields: Vec<SmolStr>,
    },
    Spawn {
        target: Value,
        instance: Value,
        size: PoolSize,
        objective: Objective,
        config: SpawnConfig,
    },
    PoolNew {
        handles: Value,
        objective: Objective,
        min_size: i64,
        max_size: i64,
        weight: i64,
        queue_cap: i64,
    },
    BuildList {
        items: Vec<Value>,
        alloc: AllocKind,
    },
    BuildMap {
        items: Vec<(Value, Value)>,
        alloc: AllocKind,
    },
    StringInterp {
        parts: Vec<StringPartValue>,
        alloc: AllocKind,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnConfig {
    pub mailbox_cap: Option<i64>,
    pub enqueue_timeout_ms: Option<i64>,
    pub batch_limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Phi {
        place: Place,
        sources: Vec<(BlockId, Value)>,
        original: LocalId,
        span: TextRange,
    },
    Assign {
        place: Place,
        value: Rvalue,
        span: TextRange,
    },
    SetField {
        base: Value,
        field: SmolStr,
        slot: Option<u32>,
        value: Value,
        span: TextRange,
    },
    RcInc {
        value: Value,
        span: TextRange,
    },
    RcDec {
        value: Value,
        span: TextRange,
    },
    Await {
        dst: Place,
        pending: Value,
        span: TextRange,
    },
    Fire {
        pending: Value,
        span: TextRange,
    },
    IterInit {
        dst: Place,
        iterable: Value,
        span: TextRange,
    },
    IterNext {
        iter: Value,
        dst_value: Place,
        dst_done: Place,
        span: TextRange,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SwitchCase {
    Literal(Literal),
    Type(TypeTagId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Return {
        value: Option<Value>,
        span: TextRange,
    },
    Jump {
        target: BlockId,
        span: TextRange,
    },
    Branch {
        cond: Value,
        then_target: BlockId,
        else_target: BlockId,
        span: TextRange,
    },
    Switch {
        scrutinee: Value,
        cases: Vec<(SwitchCase, BlockId)>,
        default: BlockId,
        span: TextRange,
    },
    Unreachable {
        span: TextRange,
    },
}

#[cfg(test)]
mod tests {
    use super::MirType;

    #[test]
    fn integer_is_scalar_not_ref() {
        assert!(!MirType::Integer.is_ref());
        assert!(!MirType::Float.is_ref());
        assert!(!MirType::Boolean.is_ref());
        assert!(!MirType::Nil.is_ref());
        assert!(MirType::Unknown.is_ref());
    }
}
