pub mod eval;
pub mod ir;
pub mod lower;

pub use eval::{execute_entry, execute_function, PirExecError};
pub use ir::{
    PirBlock, PirCallTarget, PirExpr, PirFunction, PirIntrinsic, PirModule, PirParam,
    PirStructField, PirStructType, PirStructValue, PirStmt, PirType, PirValue,
};
pub use lower::{lower_portable_entry_by_name, lower_portable_function, PirLowerError};
