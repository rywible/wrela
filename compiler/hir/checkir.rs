use crate::hir::arena::Idx;
use crate::hir::{BinaryOp, Body, Expr, FunctionKind, Literal, Module, Stmt, UnaryOp};
use smol_str::SmolStr;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub struct CheckIrModule {
    pub checks: Vec<CheckIrFunction>,
    pub skipped: Vec<SkippedCheck>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkippedCheck {
    pub name: SmolStr,
    pub reason: SmolStr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckIrFunction {
    pub name: SmolStr,
    pub params: Vec<SmolStr>,
    pub dag: DecisionDag,
    pub ops_used: BTreeSet<CheckBinaryOp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionDag {
    pub nodes: Vec<DecisionNode>,
    pub root: NodeId,
}

pub type NodeId = usize;

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionNode {
    Const(CheckValue),
    Param(usize),
    Unary {
        op: CheckUnaryOp,
        input: NodeId,
    },
    Binary {
        op: CheckBinaryOp,
        lhs: NodeId,
        rhs: NodeId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckUnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckBinaryOp {
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchEvalResult {
    pub lane_width: usize,
    pub values: Vec<Option<bool>>,
}

pub fn extract_module(module: &Module) -> CheckIrModule {
    let mut class_by_method = HashMap::new();
    for (_class_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            class_by_method.insert(method_id.into_raw(), class.name.clone());
        }
    }

    let mut checks = Vec::new();
    let mut skipped = Vec::new();

    for (func_idx, func) in module.functions.iter() {
        if !matches!(func.kind, FunctionKind::Check | FunctionKind::CheckMethod) {
            continue;
        }

        let name = if matches!(func.kind, FunctionKind::CheckMethod) {
            if let Some(class_name) = class_by_method.get(&func_idx.into_raw()) {
                SmolStr::new(format!("{}.{}", class_name, func.name))
            } else {
                func.name.clone()
            }
        } else {
            func.name.clone()
        };

        let Some(body) = &func.body else {
            skipped.push(SkippedCheck {
                name,
                reason: SmolStr::new("missing body"),
            });
            continue;
        };
        let Some(ret_expr) = find_direct_return_expr(body) else {
            skipped.push(SkippedCheck {
                name,
                reason: SmolStr::new("no direct return expression"),
            });
            continue;
        };

        let mut builder = DagBuilder::new(body, &func.params);
        let Some(root) = builder.lower_expr(ret_expr) else {
            skipped.push(SkippedCheck {
                name,
                reason: SmolStr::new("unsupported check expression"),
            });
            continue;
        };

        checks.push(CheckIrFunction {
            name,
            params: func.params.iter().map(|p| p.name.clone()).collect(),
            dag: DecisionDag {
                nodes: builder.nodes,
                root,
            },
            ops_used: builder.ops_used,
        });
    }

    CheckIrModule { checks, skipped }
}

fn find_direct_return_expr(body: &Body) -> Option<Idx<Expr>> {
    for stmt_id in &body.root_stmts {
        if let Stmt::Return(Some(expr)) = &body.stmts[*stmt_id] {
            return Some(*expr);
        }
    }
    None
}

impl CheckIrFunction {
    pub fn eval_scalar_bool(&self, args: &[CheckValue]) -> Option<bool> {
        self.dag.eval_bool(args)
    }

    pub fn eval_batch_bool(&self, rows: &[Vec<CheckValue>]) -> BatchEvalResult {
        let lane_width = 8;
        let mut out = Vec::with_capacity(rows.len());
        for chunk in rows.chunks(lane_width) {
            for row in chunk {
                out.push(self.eval_scalar_bool(row));
            }
        }
        BatchEvalResult {
            lane_width,
            values: out,
        }
    }
}

impl DecisionDag {
    pub fn eval_bool(&self, args: &[CheckValue]) -> Option<bool> {
        let value = self.eval_value(args)?;
        match value {
            CheckValue::Boolean(value) => Some(value),
            _ => None,
        }
    }

    pub fn eval_value(&self, args: &[CheckValue]) -> Option<CheckValue> {
        let mut memo = vec![None; self.nodes.len()];
        self.eval_node(self.root, args, &mut memo)
    }

    fn eval_node(
        &self,
        id: NodeId,
        args: &[CheckValue],
        memo: &mut [Option<CheckValue>],
    ) -> Option<CheckValue> {
        if let Some(value) = memo.get(id).cloned().flatten() {
            return Some(value);
        }

        let node = self.nodes.get(id)?;
        let value = match node {
            DecisionNode::Const(value) => value.clone(),
            DecisionNode::Param(idx) => args.get(*idx)?.clone(),
            DecisionNode::Unary { op, input } => {
                let input = self.eval_node(*input, args, memo)?;
                eval_unary(*op, input)?
            }
            DecisionNode::Binary { op, lhs, rhs } => {
                let lhs = self.eval_node(*lhs, args, memo)?;
                let rhs = self.eval_node(*rhs, args, memo)?;
                eval_binary(*op, lhs, rhs)?
            }
        };

        if let Some(slot) = memo.get_mut(id) {
            *slot = Some(value.clone());
        }
        Some(value)
    }
}

fn eval_unary(op: CheckUnaryOp, input: CheckValue) -> Option<CheckValue> {
    match (op, input) {
        (CheckUnaryOp::Not, CheckValue::Boolean(value)) => Some(CheckValue::Boolean(!value)),
        (CheckUnaryOp::Neg, CheckValue::Integer(value)) => Some(CheckValue::Integer(-value)),
        (CheckUnaryOp::Neg, CheckValue::Float(value)) => Some(CheckValue::Float(-value)),
        _ => None,
    }
}

fn eval_binary(op: CheckBinaryOp, lhs: CheckValue, rhs: CheckValue) -> Option<CheckValue> {
    match (op, lhs, rhs) {
        (CheckBinaryOp::Add, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Integer(lhs + rhs))
        }
        (CheckBinaryOp::Sub, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Integer(lhs - rhs))
        }
        (CheckBinaryOp::Mul, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Integer(lhs * rhs))
        }
        (CheckBinaryOp::Div, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            if rhs == 0 {
                None
            } else {
                Some(CheckValue::Integer(lhs / rhs))
            }
        }
        (CheckBinaryOp::Mod, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            if rhs == 0 {
                None
            } else {
                Some(CheckValue::Integer(lhs % rhs))
            }
        }
        (CheckBinaryOp::Eq, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs == rhs))
        }
        (CheckBinaryOp::Ne, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs != rhs))
        }
        (CheckBinaryOp::Lt, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs < rhs))
        }
        (CheckBinaryOp::Gt, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs > rhs))
        }
        (CheckBinaryOp::Le, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs <= rhs))
        }
        (CheckBinaryOp::Ge, CheckValue::Integer(lhs), CheckValue::Integer(rhs)) => {
            Some(CheckValue::Boolean(lhs >= rhs))
        }
        (CheckBinaryOp::And, CheckValue::Boolean(lhs), CheckValue::Boolean(rhs)) => {
            Some(CheckValue::Boolean(lhs && rhs))
        }
        (CheckBinaryOp::Or, CheckValue::Boolean(lhs), CheckValue::Boolean(rhs)) => {
            Some(CheckValue::Boolean(lhs || rhs))
        }
        (CheckBinaryOp::Eq, CheckValue::Boolean(lhs), CheckValue::Boolean(rhs)) => {
            Some(CheckValue::Boolean(lhs == rhs))
        }
        (CheckBinaryOp::Ne, CheckValue::Boolean(lhs), CheckValue::Boolean(rhs)) => {
            Some(CheckValue::Boolean(lhs != rhs))
        }
        _ => None,
    }
}

struct DagBuilder<'a> {
    body: &'a Body,
    params: HashMap<SmolStr, usize>,
    nodes: Vec<DecisionNode>,
    ops_used: BTreeSet<CheckBinaryOp>,
}

impl<'a> DagBuilder<'a> {
    fn new(body: &'a Body, params: &[crate::hir::Param]) -> Self {
        let mut param_map = HashMap::new();
        for (idx, param) in params.iter().enumerate() {
            param_map.insert(param.name.clone(), idx);
        }
        Self {
            body,
            params: param_map,
            nodes: Vec::new(),
            ops_used: BTreeSet::new(),
        }
    }

    fn alloc(&mut self, node: DecisionNode) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }

    fn lower_expr(&mut self, expr_id: Idx<Expr>) -> Option<NodeId> {
        let expr = &self.body.exprs[expr_id];
        match expr {
            Expr::Literal(lit) => {
                let value = match lit {
                    Literal::Integer(value) => CheckValue::Integer(*value),
                    Literal::Float(value) => CheckValue::Float(*value),
                    Literal::Boolean(value) => CheckValue::Boolean(*value),
                    _ => return None,
                };
                Some(self.alloc(DecisionNode::Const(value)))
            }
            Expr::Variable(name) => {
                let idx = *self.params.get(name)?;
                Some(self.alloc(DecisionNode::Param(idx)))
            }
            Expr::Unary { op, expr, .. } => {
                let op = match op {
                    UnaryOp::Not => CheckUnaryOp::Not,
                    UnaryOp::Neg => CheckUnaryOp::Neg,
                    _ => return None,
                };
                let input = self.lower_expr(*expr)?;
                Some(self.alloc(DecisionNode::Unary { op, input }))
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                let op = match op {
                    BinaryOp::Add => CheckBinaryOp::Add,
                    BinaryOp::Sub => CheckBinaryOp::Sub,
                    BinaryOp::Mul => CheckBinaryOp::Mul,
                    BinaryOp::Div => CheckBinaryOp::Div,
                    BinaryOp::Mod => CheckBinaryOp::Mod,
                    BinaryOp::Eq => CheckBinaryOp::Eq,
                    BinaryOp::Ne => CheckBinaryOp::Ne,
                    BinaryOp::Lt => CheckBinaryOp::Lt,
                    BinaryOp::Gt => CheckBinaryOp::Gt,
                    BinaryOp::Le => CheckBinaryOp::Le,
                    BinaryOp::Ge => CheckBinaryOp::Ge,
                    BinaryOp::And => CheckBinaryOp::And,
                    BinaryOp::Or => CheckBinaryOp::Or,
                    _ => return None,
                };
                self.ops_used.insert(op);
                let lhs = self.lower_expr(*lhs)?;
                let rhs = self.lower_expr(*rhs)?;
                Some(self.alloc(DecisionNode::Binary { op, lhs, rhs }))
            }
            _ => None,
        }
    }
}
