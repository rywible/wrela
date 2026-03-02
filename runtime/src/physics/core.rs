use serde::{Deserialize, Serialize};

pub type FixedMilli = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsBodyStateV1 {
    Static,
    Active,
    Sleeping,
    Baked,
    Harvested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColliderV1 {
    Sphere {
        radius_milli: FixedMilli,
    },
    Aabb {
        half_extent_x_milli: FixedMilli,
        half_extent_y_milli: FixedMilli,
        half_extent_z_milli: FixedMilli,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsBodyV1 {
    pub body_id: u64,
    pub state: PhysicsBodyStateV1,
    pub position_milli: [FixedMilli; 3],
    pub velocity_milli_per_s: [FixedMilli; 3],
    pub mass_milli: FixedMilli,
    pub collider: ColliderV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsIslandV1 {
    pub island_id: u64,
    pub body_ids: Vec<u64>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactManifoldV1 {
    pub a: u64,
    pub b: u64,
    pub normal_milli: [FixedMilli; 3],
    pub penetration_milli: FixedMilli,
}

pub fn fixed_step_integrate(bodies: &mut [PhysicsBodyV1], dt_ms: u32) {
    let dt_ms = dt_ms as i64;
    for body in bodies {
        if body.state != PhysicsBodyStateV1::Active {
            continue;
        }
        for axis in 0..3 {
            body.position_milli[axis] += (body.velocity_milli_per_s[axis] * dt_ms) / 1000;
        }
    }
}

pub fn physics_state_hash(bodies: &[PhysicsBodyV1], contacts: &[ContactManifoldV1]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut sorted = bodies.to_vec();
    sorted.sort_by_key(|body| body.body_id);
    for body in &sorted {
        hash = fnv_mix(hash, body.body_id);
        hash = fnv_mix(hash, body.mass_milli as u64);
        hash = fnv_mix(hash, body.state as u64);
        for value in body.position_milli {
            hash = fnv_mix(hash, value as u64);
        }
        for value in body.velocity_milli_per_s {
            hash = fnv_mix(hash, value as u64);
        }
    }

    let mut sorted_contacts = contacts.to_vec();
    sorted_contacts.sort_by_key(|c| (c.a.min(c.b), c.a.max(c.b)));
    for contact in &sorted_contacts {
        hash = fnv_mix(hash, contact.a);
        hash = fnv_mix(hash, contact.b);
        hash = fnv_mix(hash, contact.penetration_milli as u64);
        for component in contact.normal_milli {
            hash = fnv_mix(hash, component as u64);
        }
    }
    hash
}

fn fnv_mix(current: u64, value: u64) -> u64 {
    let mut hash = current;
    hash ^= value;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    hash
}

#[cfg(test)]
mod tests {
    use super::{
        ColliderV1, ContactManifoldV1, PhysicsBodyStateV1, PhysicsBodyV1, fixed_step_integrate,
        physics_state_hash,
    };

    fn active_body(id: u64, x: i64, y: i64, z: i64, vx: i64, vy: i64, vz: i64) -> PhysicsBodyV1 {
        PhysicsBodyV1 {
            body_id: id,
            state: PhysicsBodyStateV1::Active,
            position_milli: [x, y, z],
            velocity_milli_per_s: [vx, vy, vz],
            mass_milli: 1_000,
            collider: ColliderV1::Sphere { radius_milli: 500 },
        }
    }

    #[test]
    fn fixed_step_integration_updates_position_for_active_body() {
        let mut bodies = vec![active_body(1, 0, 0, 0, 2_000, -1_000, 500)];
        fixed_step_integrate(&mut bodies, 50);
        assert_eq!(bodies[0].position_milli, [100, -50, 25]);
    }

    #[test]
    fn deterministic_replay_hash_is_stable_for_same_state() {
        let bodies = vec![active_body(9, 10, 20, 30, 40, 50, 60)];
        let contacts = vec![ContactManifoldV1 {
            a: 9,
            b: 10,
            normal_milli: [0, 1000, 0],
            penetration_milli: 3,
        }];
        let a = physics_state_hash(&bodies, &contacts);
        let b = physics_state_hash(&bodies, &contacts);
        assert_eq!(a, b);
    }
}
