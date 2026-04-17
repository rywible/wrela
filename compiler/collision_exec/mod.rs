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
