use crate::hir::{self, Expr};
use crate::mir::ir::Value;
use crate::query_contract::QueryContractId;

#[derive(Default)]
pub(crate) struct BatchQueryLoopInputs {
    pub(crate) point: Option<Value>,
    pub(crate) origin: Option<Value>,
    pub(crate) direction: Option<Value>,
    pub(crate) max_distance: Option<Value>,
    pub(crate) min_step: Option<Value>,
    pub(crate) hit_epsilon: Option<Value>,
    pub(crate) max_steps: Option<Value>,
    pub(crate) hit: Option<Value>,
}

#[derive(Default)]
pub(crate) struct BatchQueryExecutionState {
    pub(crate) candidate_strategy: Option<crate::query_plan::CandidateStrategy>,
    pub(crate) pruning_strategy: Option<crate::query_plan::PruningStrategy>,
}

pub(crate) struct ScalarQueryInvocationSpec {
    pub(crate) contract_id: QueryContractId,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) domain: Option<hir::Idx<Expr>>,
    pub(crate) item: hir::Idx<Expr>,
    pub(crate) backend: Option<hir::Idx<Expr>>,
}

pub(crate) struct BatchQueryInvocationSpec {
    pub(crate) contract_id: QueryContractId,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) domain: Option<hir::Idx<Expr>>,
    pub(crate) items: hir::Idx<Expr>,
    pub(crate) backend: hir::Idx<Expr>,
}
