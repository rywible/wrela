//! Physics contract identifiers and authored body descriptors (RFC 0011 Phase 67).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicsBodyId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicsContractId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhysicsBodyClass {
    Dynamic,
    Kinematic,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhysicsContactShape {
    PointSphere,
    SphereSphere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhysicsWitnessKind {
    Overlap,
    Sweep,
    TimeOfImpact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsBodyDescriptor {
    pub id: PhysicsBodyId,
    pub class: PhysicsBodyClass,
    pub mass_kg: f32,
    pub inverse_mass: f32,
    pub radius: f32,
    pub ccd_threshold_per_substep: f32,
    pub friction_static: f32,
    pub friction_dynamic: f32,
    pub restitution: f32,
}

impl PhysicsBodyDescriptor {
    pub fn dynamic_sphere(id: PhysicsBodyId, mass_kg: f32, radius: f32) -> Self {
        Self {
            id,
            class: PhysicsBodyClass::Dynamic,
            mass_kg,
            inverse_mass: if mass_kg > 0.0 { 1.0 / mass_kg } else { 0.0 },
            radius,
            ccd_threshold_per_substep: radius,
            friction_static: 0.6,
            friction_dynamic: 0.45,
            restitution: 0.0,
        }
    }
}
