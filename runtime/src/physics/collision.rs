use crate::physics::core::{ColliderV1, ContactManifoldV1, PhysicsBodyV1};

fn axis_aligned_half_extents(collider: &ColliderV1) -> [i64; 3] {
    match collider {
        ColliderV1::Sphere { radius_milli } => [*radius_milli, *radius_milli, *radius_milli],
        ColliderV1::Aabb {
            half_extent_x_milli,
            half_extent_y_milli,
            half_extent_z_milli,
        } => [
            *half_extent_x_milli,
            *half_extent_y_milli,
            *half_extent_z_milli,
        ],
    }
}

fn overlaps(a: &PhysicsBodyV1, b: &PhysicsBodyV1) -> Option<ContactManifoldV1> {
    let a_extents = axis_aligned_half_extents(&a.collider);
    let b_extents = axis_aligned_half_extents(&b.collider);

    let dx = (a.position_milli[0] - b.position_milli[0]).abs();
    let dy = (a.position_milli[1] - b.position_milli[1]).abs();
    let dz = (a.position_milli[2] - b.position_milli[2]).abs();

    let overlap_x = a_extents[0] + b_extents[0] - dx;
    let overlap_y = a_extents[1] + b_extents[1] - dy;
    let overlap_z = a_extents[2] + b_extents[2] - dz;

    if overlap_x < 0 || overlap_y < 0 || overlap_z < 0 {
        return None;
    }

    let (penetration_milli, normal_milli) = if overlap_x <= overlap_y && overlap_x <= overlap_z {
        (
            overlap_x,
            if a.position_milli[0] <= b.position_milli[0] {
                [-1000, 0, 0]
            } else {
                [1000, 0, 0]
            },
        )
    } else if overlap_y <= overlap_z {
        (
            overlap_y,
            if a.position_milli[1] <= b.position_milli[1] {
                [0, -1000, 0]
            } else {
                [0, 1000, 0]
            },
        )
    } else {
        (
            overlap_z,
            if a.position_milli[2] <= b.position_milli[2] {
                [0, 0, -1000]
            } else {
                [0, 0, 1000]
            },
        )
    };

    Some(ContactManifoldV1 {
        a: a.body_id,
        b: b.body_id,
        normal_milli,
        penetration_milli,
    })
}

pub fn broadphase_pairs(bodies: &[PhysicsBodyV1]) -> Vec<(u64, u64)> {
    let mut pairs = Vec::new();
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            if overlaps(&bodies[i], &bodies[j]).is_some() {
                pairs.push((bodies[i].body_id, bodies[j].body_id));
            }
        }
    }
    pairs
}

pub fn detect_contacts(bodies: &[PhysicsBodyV1]) -> Vec<ContactManifoldV1> {
    let mut contacts = Vec::new();
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            if let Some(contact) = overlaps(&bodies[i], &bodies[j]) {
                contacts.push(contact);
            }
        }
    }
    contacts
}

#[cfg(test)]
mod tests {
    use super::{broadphase_pairs, detect_contacts};
    use crate::physics::core::{ColliderV1, PhysicsBodyStateV1, PhysicsBodyV1};

    fn sphere(id: u64, x: i64) -> PhysicsBodyV1 {
        PhysicsBodyV1 {
            body_id: id,
            state: PhysicsBodyStateV1::Active,
            position_milli: [x, 0, 0],
            velocity_milli_per_s: [0, 0, 0],
            mass_milli: 1000,
            collider: ColliderV1::Sphere { radius_milli: 1000 },
        }
    }

    #[test]
    fn collision_detection_finds_overlapping_pairs() {
        let bodies = vec![sphere(1, 0), sphere(2, 1500), sphere(3, 5000)];
        let pairs = broadphase_pairs(&bodies);
        assert_eq!(pairs, vec![(1, 2)]);
        let contacts = detect_contacts(&bodies);
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].a, 1);
        assert_eq!(contacts[0].b, 2);
    }
}
