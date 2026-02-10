use crate::hir::BinaryOp;
use crate::hir::checkir::{CheckBinaryOp, CheckIrModule};
use crate::mir::ir::{MirFunction, MirModule, Rvalue, Stmt, Terminator, Value};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

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
    pub max_compile_cost: u32,
    pub max_rule_risk: u32,
    pub per_function_rewrite_cap: usize,
}

impl Default for RewriteBudget {
    fn default() -> Self {
        Self {
            max_steps: 50_000,
            max_compile_cost: 16,
            max_rule_risk: 8,
            per_function_rewrite_cap: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteRuleScore {
    pub rule: RewriteRule,
    pub expected_runtime_gain: u32,
    pub compile_cost: u32,
    pub risk: u32,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteAdmissionReason {
    pub rule: RewriteRule,
    pub reason: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteAdmission {
    pub pack: RewriteRulePack,
    pub rule_scores: Vec<RewriteRuleScore>,
    pub admission_reason: Vec<RewriteAdmissionReason>,
    pub ignored_by_budget: usize,
    pub ignored_by_risk: usize,
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
    pub rule_scores: Vec<RewriteRuleScore>,
    pub admission_reason: Vec<RewriteAdmissionReason>,
    pub ignored_by_budget: usize,
    pub ignored_by_risk: usize,
    pub oscillation_block_count: usize,
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

pub fn admit_rulepack_scored(
    candidates: &[RewriteRule],
    module: &MirModule,
    checkir: Option<&CheckIrModule>,
    budget: RewriteBudget,
    max_rules: usize,
) -> RewriteAdmission {
    let mut dedup = BTreeSet::new();
    for candidate in candidates {
        dedup.insert(*candidate);
    }

    let mut rule_scores = Vec::with_capacity(dedup.len());
    for rule in dedup {
        rule_scores.push(score_rule(rule, module, checkir));
    }
    rule_scores.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.rule.cmp(&right.rule))
    });

    let max_rules = max_rules.max(1);
    let mut compile_spend = 0u32;
    let mut admitted_rules = Vec::new();
    let mut admission_reason = Vec::with_capacity(rule_scores.len());
    let mut ignored_by_budget = 0usize;
    let mut ignored_by_risk = 0usize;

    for scored in &rule_scores {
        if scored.risk > budget.max_rule_risk {
            ignored_by_risk += 1;
            admission_reason.push(RewriteAdmissionReason {
                rule: scored.rule,
                reason: SmolStr::new("ignored_by_risk"),
            });
            continue;
        }
        if admitted_rules.len() >= max_rules {
            admission_reason.push(RewriteAdmissionReason {
                rule: scored.rule,
                reason: SmolStr::new("ignored_by_topn"),
            });
            continue;
        }
        let next_spend = compile_spend.saturating_add(scored.compile_cost);
        if next_spend > budget.max_compile_cost {
            ignored_by_budget += 1;
            admission_reason.push(RewriteAdmissionReason {
                rule: scored.rule,
                reason: SmolStr::new("ignored_by_budget"),
            });
            continue;
        }
        compile_spend = next_spend;
        admitted_rules.push(scored.rule);
        admission_reason.push(RewriteAdmissionReason {
            rule: scored.rule,
            reason: SmolStr::new("admitted"),
        });
    }

    RewriteAdmission {
        pack: RewriteRulePack {
            rules: admitted_rules,
        },
        rule_scores,
        admission_reason,
        ignored_by_budget,
        ignored_by_risk,
    }
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
        rule_scores: Vec::new(),
        admission_reason: Vec::new(),
        ignored_by_budget: 0,
        ignored_by_risk: 0,
        oscillation_block_count: 0,
        evidence: Vec::new(),
    };

    let mut rewrite_counts: BTreeMap<SmolStr, usize> = BTreeMap::new();
    let mut history_hashes: BTreeMap<SmolStr, BTreeSet<u64>> = BTreeMap::new();
    for func in &module.functions {
        rewrite_counts.insert(func.name.clone(), 0);
        let mut history = BTreeSet::new();
        history.insert(function_history_hash(func));
        history_hashes.insert(func.name.clone(), history);
    }

    for func_idx in 0..module.functions.len() {
        let func_name = module.functions[func_idx].name.clone();
        for block_idx in 0..module.functions[func_idx].blocks.len() {
            let stmt_len = module.functions[func_idx].blocks[block_idx].stmts.len();
            for stmt_idx in 0..stmt_len {
                if report.steps >= budget.max_steps {
                    report.budget_exhausted = true;
                    return report;
                }
                report.steps += 1;

                if rewrite_counts.get(&func_name).copied().unwrap_or(0)
                    >= budget.per_function_rewrite_cap
                {
                    continue;
                }

                let mut applied_rule = None;
                let mut revert_value = None;
                {
                    let stmt = &mut module.functions[func_idx].blocks[block_idx].stmts[stmt_idx];
                    let Stmt::Assign { value, .. } = stmt else {
                        continue;
                    };

                    if enabled.contains(&RewriteRule::AddZero) {
                        let before = value.clone();
                        if rewrite_add_zero(value) {
                            applied_rule = Some(RewriteRule::AddZero);
                            revert_value = Some(before);
                        }
                    }

                    if applied_rule.is_none() && enabled.contains(&RewriteRule::MulOne) {
                        let before = value.clone();
                        if rewrite_mul_one(value) {
                            applied_rule = Some(RewriteRule::MulOne);
                            revert_value = Some(before);
                        }
                    }
                }

                if let Some(rule) = applied_rule {
                    let next_hash = function_history_hash(&module.functions[func_idx]);
                    let seen = history_hashes
                        .get(&func_name)
                        .is_some_and(|seen| seen.contains(&next_hash));
                    if seen {
                        if let Some(before) = revert_value {
                            let stmt =
                                &mut module.functions[func_idx].blocks[block_idx].stmts[stmt_idx];
                            if let Stmt::Assign { value, .. } = stmt {
                                *value = before;
                            }
                        }
                        report.oscillation_block_count += 1;
                        continue;
                    }

                    if let Some(history) = history_hashes.get_mut(&func_name) {
                        history.insert(next_hash);
                    }
                    if let Some(count) = rewrite_counts.get_mut(&func_name) {
                        *count += 1;
                    }

                    report.applied += 1;
                    report.evidence.push(RewriteEvidence {
                        rule,
                        function: func_name.clone(),
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

            if rewrite_counts.get(&func_name).copied().unwrap_or(0)
                >= budget.per_function_rewrite_cap
            {
                continue;
            }

            let mut term_reverted = None;
            let mut term_applied = false;
            {
                let term = &mut module.functions[func_idx].blocks[block_idx].terminator;
                if enabled.contains(&RewriteRule::BranchOnConst) {
                    let before = term.clone();
                    if rewrite_branch_const(term) {
                        term_applied = true;
                        term_reverted = Some(before);
                    }
                }
            }

            if term_applied {
                let next_hash = function_history_hash(&module.functions[func_idx]);
                let seen = history_hashes
                    .get(&func_name)
                    .is_some_and(|seen| seen.contains(&next_hash));
                if seen {
                    if let Some(before) = term_reverted {
                        module.functions[func_idx].blocks[block_idx].terminator = before;
                    }
                    report.oscillation_block_count += 1;
                    continue;
                }

                if let Some(history) = history_hashes.get_mut(&func_name) {
                    history.insert(next_hash);
                }
                if let Some(count) = rewrite_counts.get_mut(&func_name) {
                    *count += 1;
                }

                report.applied += 1;
                report.evidence.push(RewriteEvidence {
                    rule: RewriteRule::BranchOnConst,
                    function: func_name.clone(),
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
    let admission = admit_rulepack_scored(&candidates, module, checkir, budget, max_rules);
    let mut report = apply_rulepack(module, &admission.pack, budget);
    report.mined = candidates.len();
    report.admitted = admission.pack.rules.len();
    report.rule_scores = admission.rule_scores;
    report.admission_reason = admission.admission_reason;
    report.ignored_by_budget = admission.ignored_by_budget;
    report.ignored_by_risk = admission.ignored_by_risk;
    report
}

fn score_rule(
    rule: RewriteRule,
    module: &MirModule,
    checkir: Option<&CheckIrModule>,
) -> RewriteRuleScore {
    let static_gain = match rule {
        RewriteRule::AddZero => 7,
        RewriteRule::MulOne => 9,
        RewriteRule::BranchOnConst => 11,
    };
    let compile_cost = match rule {
        RewriteRule::AddZero | RewriteRule::MulOne => 2,
        RewriteRule::BranchOnConst => 3,
    };
    let risk = match rule {
        RewriteRule::AddZero | RewriteRule::MulOne => 2,
        RewriteRule::BranchOnConst => 4,
    };

    let signal = rule_signal(module, rule).min(128) as u32;
    let check_bonus = checkir
        .map(|ir| {
            ir.checks
                .iter()
                .filter(|check| check_supports_rule(check, rule))
                .count() as u32
        })
        .unwrap_or(0)
        .min(16);

    let expected_runtime_gain = static_gain + signal + check_bonus;
    let score =
        (expected_runtime_gain as i64 * 1000) - (compile_cost as i64 * 120) - (risk as i64 * 80);

    RewriteRuleScore {
        rule,
        expected_runtime_gain,
        compile_cost,
        risk,
        score,
    }
}

fn rule_signal(module: &MirModule, rule: RewriteRule) -> usize {
    let mut signal = 0usize;
    for func in &module.functions {
        for block in &func.blocks {
            for stmt in &block.stmts {
                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                match (rule, value) {
                    (
                        RewriteRule::AddZero,
                        Rvalue::Binary {
                            op: BinaryOp::Add,
                            lhs,
                            rhs,
                        },
                    ) if is_int_const(lhs, 0) || is_int_const(rhs, 0) => {
                        signal = signal.saturating_add(1);
                    }
                    (
                        RewriteRule::MulOne,
                        Rvalue::Binary {
                            op: BinaryOp::Mul,
                            lhs,
                            rhs,
                        },
                    ) if is_int_const(lhs, 1) || is_int_const(rhs, 1) => {
                        signal = signal.saturating_add(1);
                    }
                    _ => {}
                }
            }
            if rule == RewriteRule::BranchOnConst
                && matches!(
                    block.terminator,
                    Terminator::Branch {
                        cond: Value::Const(crate::hir::Literal::Boolean(_)),
                        ..
                    }
                )
            {
                signal = signal.saturating_add(1);
            }
        }
    }
    signal
}

fn check_supports_rule(check: &crate::hir::checkir::CheckIrFunction, rule: RewriteRule) -> bool {
    match rule {
        RewriteRule::AddZero => {
            check.ops_used.contains(&CheckBinaryOp::Add)
                || check.ops_used.contains(&CheckBinaryOp::Sub)
        }
        RewriteRule::MulOne => check.ops_used.contains(&CheckBinaryOp::Mul),
        RewriteRule::BranchOnConst => {
            check.ops_used.contains(&CheckBinaryOp::And)
                || check.ops_used.contains(&CheckBinaryOp::Or)
                || check.ops_used.contains(&CheckBinaryOp::Eq)
                || check.ops_used.contains(&CheckBinaryOp::Ne)
        }
    }
}

fn function_history_hash(func: &MirFunction) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{func:?}").hash(&mut hasher);
    hasher.finish()
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
