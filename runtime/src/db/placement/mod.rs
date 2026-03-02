pub mod failure_domains;
pub mod home;
pub mod policy;

pub use failure_domains::{FailureDomain, RegionTopology, build_region_topology};
pub use home::{
    PlacementHomeError, PlacementHomeStore, RelocationJob, RelocationPhase, ResidencyPolicy,
};
pub use policy::{
    PlacementPlan, PlacementPolicyError, PlacementProfile, ReplicaPlacement, plan_placement,
    survives_region_loss,
};
