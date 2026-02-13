pub mod contract;
pub mod parity;

/// Marker for future v2 ownership boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipBoundary {
    Domain,
    Application,
    Infrastructure,
    Composition,
}

pub use contract::{ContractCaseResult, ContractReport, ContractScenario};
pub use parity::{
    ParityDiff, ParityDiffKind, ToolchainAdapter, V1Adapter, V2PlaceholderAdapter, compare_reports,
    write_parity_artifacts,
};
