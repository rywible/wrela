use crate::hir::{self, Expr};
use crate::mir::ir::Value;

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

#[derive(Clone, Copy)]
pub(crate) enum FieldQueryKind {
    Distance,
    Normal,
    Radiance,
    Medium,
}

pub(crate) struct FieldQuerySpec {
    pub(crate) kind: FieldQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) point: Option<hir::Idx<Expr>>,
    pub(crate) sample: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
pub(crate) enum ShapeQueryKind {
    Trace,
    Surface,
}

pub(crate) struct ShapeQuerySpec {
    pub(crate) kind: ShapeQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) ray: Option<hir::Idx<Expr>>,
    pub(crate) hit: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
pub(crate) enum WorldPointQueryKind {
    Distance,
    Normal,
    Radiance,
    Medium,
}

pub(crate) struct WorldPointQuerySpec {
    pub(crate) kind: WorldPointQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) domain: hir::Idx<Expr>,
    pub(crate) point: Option<hir::Idx<Expr>>,
    pub(crate) sample: Option<hir::Idx<Expr>>,
    pub(crate) backend: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
pub(crate) enum WorldShapeQueryKind {
    Trace,
    Surface,
}

pub(crate) struct WorldShapeQuerySpec {
    pub(crate) kind: WorldShapeQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) domain: hir::Idx<Expr>,
    pub(crate) ray: Option<hir::Idx<Expr>>,
    pub(crate) hit: Option<hir::Idx<Expr>>,
    pub(crate) backend: Option<hir::Idx<Expr>>,
}

#[derive(Clone, Copy)]
pub(crate) enum ShapeBatchQueryKind {
    Trace,
    Surface,
    Occluded,
}

pub(crate) struct ShapeBatchQuerySpec {
    pub(crate) kind: ShapeBatchQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) items: hir::Idx<Expr>,
    pub(crate) backend: hir::Idx<Expr>,
}

#[derive(Clone, Copy)]
pub(crate) enum FieldBatchQueryKind {
    Distance,
    Normal,
}

pub(crate) struct FieldBatchQuerySpec {
    pub(crate) kind: FieldBatchQueryKind,
    pub(crate) capture: hir::Idx<Expr>,
    pub(crate) items: hir::Idx<Expr>,
    pub(crate) backend: hir::Idx<Expr>,
}
