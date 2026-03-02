use crate::physics::core::{ContactManifoldV1, PhysicsBodyV1};
use std::collections::BTreeMap;

pub fn solve_contacts(
    bodies: &mut [PhysicsBodyV1],
    contacts: &[ContactManifoldV1],
    restitution_milli: i64,
    friction_milli: i64,
) {
    let mut index_by_id = BTreeMap::<u64, usize>::new();
    for (index, body) in bodies.iter().enumerate() {
        index_by_id.insert(body.body_id, index);
    }

    for contact in contacts {
        let Some(&a_index) = index_by_id.get(&contact.a) else {
            continue;
        };
        let Some(&b_index) = index_by_id.get(&contact.b) else {
            continue;
        };
        let (a, b) = if a_index < b_index {
            let (left, right) = bodies.split_at_mut(b_index);
            (&mut left[a_index], &mut right[0])
        } else {
            let (left, right) = bodies.split_at_mut(a_index);
            (&mut right[0], &mut left[b_index])
        };

        // Position correction (Baumgarte-ish, fixed-point): resolve half penetration per body.
        for axis in 0..3 {
            let correction = (contact.normal_milli[axis] * contact.penetration_milli) / 2000;
            a.position_milli[axis] -= correction;
            b.position_milli[axis] += correction;
        }

        // Very small impulse model for deterministic baseline behavior.
        for axis in 0..3 {
            let impulse = (contact.normal_milli[axis] * restitution_milli) / 1000;
            a.velocity_milli_per_s[axis] -= impulse;
            b.velocity_milli_per_s[axis] += impulse;

            let friction = (a.velocity_milli_per_s[axis] * friction_milli) / 1000;
            a.velocity_milli_per_s[axis] -= friction;
            let friction_b = (b.velocity_milli_per_s[axis] * friction_milli) / 1000;
            b.velocity_milli_per_s[axis] -= friction_b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::solve_contacts;
    use crate::physics::core::{ColliderV1, ContactManifoldV1, PhysicsBodyStateV1, PhysicsBodyV1};

    fn body(id: u64, x: i64, vx: i64) -> PhysicsBodyV1 {
        PhysicsBodyV1 {
            body_id: id,
            state: PhysicsBodyStateV1::Active,
            position_milli: [x, 0, 0],
            velocity_milli_per_s: [vx, 0, 0],
            mass_milli: 1000,
            collider: ColliderV1::Sphere { radius_milli: 1000 },
        }
    }

    #[test]
    fn solver_applies_position_correction_and_impulse() {
        let mut bodies = vec![body(1, 0, 100), body(2, 500, -100)];
        let contacts = vec![ContactManifoldV1 {
            a: 1,
            b: 2,
            normal_milli: [1000, 0, 0],
            penetration_milli: 100,
        }];
        solve_contacts(&mut bodies, &contacts, 250, 50);
        assert!(bodies[0].position_milli[0] < 0);
        assert!(bodies[1].position_milli[0] > 500);
    }
}
