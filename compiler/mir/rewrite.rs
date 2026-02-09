use crate::hir::BinaryOp;
use crate::hir::checkir::{CheckBinaryOp, CheckIrModule};
use crate::mir::ir::{MirModule, Rvalue, Stmt, Terminator, Value};
use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RewriteRule {
    AddZero,
    MulOne,
    BranchOnConst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteRulePack {
    pub rules: Vec<RewriteRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewriteBudget {
    pub max_steps: usize,
}

impl Default for RewriteBudget {
    fn default() -> Self {
        Self { max_steps: 50_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteEvidence {
    pub rule: RewriteRule,
    pub function: SmolStr,
    pub block: usize,
    pub stmt: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteReport {
    pub mined: usize,
    pub admitted: usize,
    pub applied: usize,
    pub steps: usize,
    pub budget_exhausted: bool,
    pub evidence: Vec<RewriteEvidence>,
}

pub fn mine_candidates(module: &MirModule, checkir: Option<&CheckIrModule>) -> Vec<RewriteRule> {
    let mut rules = BTreeSet::new();

    if let Some(checkir) = checkir {
        for check in &checkir.checks {
            if check.ops_used.contains(&CheckBinaryOp::Add)
                || check.ops_used.contains(&CheckBinaryOp::Sub)
            {
                rules.insert(RewriteRule::AddZero);
            }
            if check.ops_used.contains(&CheckBinaryOp::Mul) {
                rules.insert(RewriteRule::MulOne);
            }
            if check.ops_used.contains(&CheckBinaryOp::And)
                || check.ops_used.contains(&CheckBinaryOp::Or)
                || check.ops_used.contains(&CheckBinaryOp::Eq)
                || check.ops_used.contains(&CheckBinaryOp::Ne)
            {
                rules.insert(RewriteRule::BranchOnConst);
            }
        }
    }

    for func in &module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                match value {
                    Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs,
                        rhs,
                    } => {
                        if is_int_const(lhs, 0) || is_int_const(rhs, 0) {
                            rules.insert(RewriteRule::AddZero);
                        }
                    }
                    Rvalue::Binary {
                        op: BinaryOp::Mul,
                        lhs,
                        rhs,
                    } => {
                        if is_int_const(lhs, 1) || is_int_const(rhs, 1) {
                            rules.insert(RewriteRule::MulOne);
                        }
                    }
                    _ => {}
                }
            }

            if let Terminator::Branch { cond, .. } = &block.terminator {
                if matches!(cond, Value::Const(crate::hir::Literal::Boolean(_))) {
                    rules.insert(RewriteRule::BranchOnConst);
                }
            }
        }
    }

    rules.into_iter().collect()
}

pub fn admit_rulepack(candidates: &[RewriteRule], max_rules: usize) -> RewriteRulePack {
    let mut dedup = BTreeSet::new();
    for candidate in candidates {
        dedup.insert(*candidate);
    }
    let rules = dedup.into_iter().take(max_rules.max(1)).collect();
    RewriteRulePack { rules }
}

pub fn apply_rulepack(
    module: &mut MirModule,
    pack: &RewriteRulePack,
    budget: RewriteBudget,
) -> RewriteReport {
    let enabled: BTreeSet<RewriteRule> = pack.rules.iter().copied().collect();
    let mut report = RewriteReport {
        mined: pack.rules.len(),
        admitted: pack.rules.len(),
        applied: 0,
        steps: 0,
        budget_exhausted: false,
        evidence: Vec::new(),
    };

    for func in &mut module.functions {
        for (block_idx, block) in func.blocks.iter_mut().enumerate() {
            for (stmt_idx, stmt) in block.stmts.iter_mut().enumerate() {
                if report.steps >= budget.max_steps {
                    report.budget_exhausted = true;
                    return report;
                }
                report.steps += 1;

                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };

                if enabled.contains(&RewriteRule::AddZero) && rewrite_add_zero(value) {
                    report.applied += 1;
                    report.evidence.push(RewriteEvidence {
                        rule: RewriteRule::AddZero,
                        function: func.name.clone(),
                        block: block_idx,
                        stmt: stmt_idx,
                    });
                    continue;
                }

                if enabled.contains(&RewriteRule::MulOne) && rewrite_mul_one(value) {
                    report.applied += 1;
                    report.evidence.push(RewriteEvidence {
                        rule: RewriteRule::MulOne,
                        function: func.name.clone(),
                        block: block_idx,
                        stmt: stmt_idx,
                    });
                }
            }

            if report.steps >= budget.max_steps {
                report.budget_exhausted = true;
                return report;
            }
            report.steps += 1;

            if enabled.contains(&RewriteRule::BranchOnConst)
                && rewrite_branch_const(&mut block.terminator)
            {
                report.applied += 1;
                report.evidence.push(RewriteEvidence {
                    rule: RewriteRule::BranchOnConst,
                    function: func.name.clone(),
                    block: block_idx,
                    stmt: usize::MAX,
                });
            }
        }
    }

    report
}

pub fn mine_admit_and_apply(
    module: &mut MirModule,
    checkir: Option<&CheckIrModule>,
    budget: RewriteBudget,
    max_rules: usize,
) -> RewriteReport {
    let candidates = mine_candidates(module, checkir);
    let pack = admit_rulepack(&candidates, max_rules);
    let mut report = apply_rulepack(module, &pack, budget);
    report.mined = candidates.len();
    report.admitted = pack.rules.len();
    report
}

fn rewrite_add_zero(value: &mut Rvalue) -> bool {
    let Rvalue::Binary {
        op: BinaryOp::Add,
        lhs,
        rhs,
    } = value
    else {
        return false;
    };

    if is_int_const(lhs, 0) {
        *value = Rvalue::Use(rhs.clone());
        return true;
    }
    if is_int_const(rhs, 0) {
        *value = Rvalue::Use(lhs.clone());
        return true;
    }
    false
}

fn rewrite_mul_one(value: &mut Rvalue) -> bool {
    let Rvalue::Binary {
        op: BinaryOp::Mul,
        lhs,
        rhs,
    } = value
    else {
        return false;
    };

    if is_int_const(lhs, 1) {
        *value = Rvalue::Use(rhs.clone());
        return true;
    }
    if is_int_const(rhs, 1) {
        *value = Rvalue::Use(lhs.clone());
        return true;
    }
    false
}

fn rewrite_branch_const(term: &mut Terminator) -> bool {
    let Terminator::Branch {
        cond,
        then_target,
        else_target,
        span,
    } = term
    else {
        return false;
    };
    let Value::Const(crate::hir::Literal::Boolean(flag)) = cond else {
        return false;
    };
    let target = if *flag { *then_target } else { *else_target };
    *term = Terminator::Jump {
        target,
        span: *span,
    };
    true
}

fn is_int_const(value: &Value, n: i64) -> bool {
    matches!(value, Value::Const(crate::hir::Literal::Integer(value)) if *value == n)
}
