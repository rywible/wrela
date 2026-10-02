//! wrela's mid-level IR: typed, monomorphic, with structured control flow.
//!
//! It sits between the checked program and the back ends: naga (and from it WGSL) for the GPU,
//! WASM for the CPU, and the reference interpreter that is the oracle for both. The derived
//! interpretations (gradient, interval) are transforms from IR to IR. The IR is shaped for all of
//! tier 0, though first light uses only a little of it.
//!
//! - [`Module`] and everything in it: [`module`].
//! - [`Types`], the interning type arena, and the `GpuData` layout rule: [`types`].
//! - [`FunctionBuilder`] for building functions by hand or from a lowering.
//! - [`validate`]: the invariants every producer must keep and every consumer may assume. A
//!   failure is a compiler bug, not a user error.
//! - `Display` for [`Module`]: a deterministic text form for golden tests.

mod builder;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
pub mod module;
mod print;
pub mod types;
mod validate;

pub use builder::FunctionBuilder;
pub use module::{
    Arg, AtomicOp, BinaryOp, Block, Builtin, Const, EntryParam, EntryPoint, Export, Expr, Function,
    Import, Interpolation, Interpretation, Io, IoBinding, Local, MathOp, Module, Param, ParamMode,
    Pipeline, PipelineKind, Place, PlaceRoot, Projection, Record, Resource, ResourceKind, Stage,
    Stmt, UnaryOp,
};
pub use types::{
    EnumType, GpuLayout, LayoutError, Scalar, StructField, StructType, Type, Types, Variant,
    VectorSize,
};
pub use validate::{ValidationError, validate};

macro_rules! ids {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
            pub struct $name(pub u32);

            impl $name {
                pub fn index(self) -> usize {
                    self.0 as usize
                }
            }
        )*
    };
}

ids! {
    /// A type in a module's [`Types`].
    TypeId;
    /// A function in [`Module::functions`].
    FuncId;
    /// A value in one function: its type is `Function::values[id]`.
    ValueId;
    /// A local in one function's [`Function::locals`].
    LocalId;
    /// A host function in [`Module::imports`].
    ImportId;
    /// A GPU entry point in [`Module::entry_points`].
    EntryId;
    /// A pipeline in [`Module::pipelines`]; also its id in the manifest and in `DRAW`.
    PipelineId;
}

#[cfg(test)]
mod tests;
