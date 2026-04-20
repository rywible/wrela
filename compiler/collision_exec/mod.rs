//! Owns collision-query execution backends and the seam between CPU-authoritative
//! semantics and optional GPU acceleration helpers.
//! Does not own collision planning, public collision contracts, or presentation
//! query orchestration.
//!
//! Key invariants:
//! - CPU execution remains the trusted semantic oracle for collision results.
//! - GPU helpers may accelerate execution, but they must not redefine witness or
//!   distance semantics.
//!
//! Primary entrypoints:
//! - `cpu::*`
//! - `gpu::*`
//!
//! Failure modes / common pitfalls:
//! - treating GPU helper output as authoritative without CPU parity checks can
//!   silently break collision truth.

pub mod cpu;
pub(crate) mod gpu;

pub use crate::collision_plan::{
    CollisionBatchExecutionReport, CollisionBatchItem, CollisionBatchResult,
    CollisionCandidateGroupingPolicy, CollisionCertificationPolicy, CollisionWorkloadBatch,
};
pub use cpu::{execute_batch_cpu, execute_batch_cpu_with_store};
pub use gpu::CollisionGpuBatchTicket;

use crate::collision_plan::CollisionExecError;
use crate::query_exec::QueryExecContext;

pub fn execute_batch(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    store: Option<&mut crate::collision_exec::cpu::CollisionArtifactStore>,
) -> Result<CollisionBatchResult, CollisionExecError> {
    match batch.plan.backend {
        crate::query_plan::DispatchBackend::Wgsl => gpu::execute_batch_gpu(batch, ctx, store),
        crate::query_plan::DispatchBackend::Cpu | crate::query_plan::DispatchBackend::Auto => {
            execute_batch_cpu(batch, ctx, store)
        }
        other => Err(CollisionExecError::UnsupportedBackend { backend: other }),
    }
}

pub fn execute_batch_metrics_only(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
) -> Result<CollisionBatchExecutionReport, CollisionExecError> {
    match batch.plan.backend {
        crate::query_plan::DispatchBackend::Wgsl => gpu::execute_batch_gpu_metrics_only(batch, ctx),
        crate::query_plan::DispatchBackend::Cpu | crate::query_plan::DispatchBackend::Auto => {
            Ok(execute_batch_cpu(batch, ctx, None)?.report)
        }
        other => Err(CollisionExecError::UnsupportedBackend { backend: other }),
    }
}
