use crate::physics::core::{ContactManifoldV1, PhysicsBodyStateV1, PhysicsBodyV1, PhysicsIslandV1};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn detect_islands(
    bodies: &[PhysicsBodyV1],
    contacts: &[ContactManifoldV1],
) -> Vec<PhysicsIslandV1> {
    let mut graph = BTreeMap::<u64, BTreeSet<u64>>::new();
    for body in bodies {
        graph.entry(body.body_id).or_default();
    }
    for contact in contacts {
        graph.entry(contact.a).or_default().insert(contact.b);
        graph.entry(contact.b).or_default().insert(contact.a);
    }

    let mut visited = BTreeSet::<u64>::new();
    let mut islands = Vec::<PhysicsIslandV1>::new();
    let mut island_id = 1u64;

    for body in bodies {
        if !visited.insert(body.body_id) {
            continue;
        }
        let mut queue = VecDeque::from([body.body_id]);
        let mut body_ids = Vec::<u64>::new();
        let mut is_active = false;

        while let Some(current) = queue.pop_front() {
            body_ids.push(current);
            let state = bodies
                .iter()
                .find(|entry| entry.body_id == current)
                .map(|entry| entry.state)
                .unwrap_or(PhysicsBodyStateV1::Static);
            if state == PhysicsBodyStateV1::Active {
                is_active = true;
            }
            for neighbor in graph.get(&current).into_iter().flatten() {
                if visited.insert(*neighbor) {
                    queue.push_back(*neighbor);
                }
            }
        }

        body_ids.sort_unstable();
        islands.push(PhysicsIslandV1 {
            island_id,
            body_ids,
            is_active,
        });
        island_id = island_id.saturating_add(1);
    }

    islands.sort_by_key(|island| island.body_ids.first().copied().unwrap_or(0));
    islands
}

pub fn transition_sleep_state(
    body: &mut PhysicsBodyV1,
    kinetic_energy_milli: i64,
    sleep_threshold_milli: i64,
) {
    if body.state == PhysicsBodyStateV1::Baked || body.state == PhysicsBodyStateV1::Harvested {
        return;
    }
    if kinetic_energy_milli <= sleep_threshold_milli {
        body.state = PhysicsBodyStateV1::Sleeping;
    } else {
        body.state = PhysicsBodyStateV1::Active;
    }
}

#[cfg(test)]
mod tests {
    use super::detect_islands;
    use crate::physics::core::{ColliderV1, ContactManifoldV1, PhysicsBodyStateV1, PhysicsBodyV1};

    fn body(id: u64, active: bool) -> PhysicsBodyV1 {
        PhysicsBodyV1 {
            body_id: id,
            state: if active {
                PhysicsBodyStateV1::Active
            } else {
                PhysicsBodyStateV1::Static
            },
            position_milli: [0, 0, 0],
            velocity_milli_per_s: [0, 0, 0],
            mass_milli: 1_000,
            collider: ColliderV1::Sphere { radius_milli: 500 },
        }
    }

    #[test]
    fn deterministic_partition_from_contact_graph() {
        let bodies = vec![body(1, true), body(2, false), body(3, true), body(4, false)];
        let contacts = vec![
            ContactManifoldV1 {
                a: 1,
                b: 2,
                normal_milli: [1000, 0, 0],
                penetration_milli: 1,
            },
            ContactManifoldV1 {
                a: 3,
                b: 4,
                normal_milli: [1000, 0, 0],
                penetration_milli: 1,
            },
        ];
        let islands = detect_islands(&bodies, &contacts);
        assert_eq!(islands.len(), 2);
        assert_eq!(islands[0].body_ids, vec![1, 2]);
        assert_eq!(islands[1].body_ids, vec![3, 4]);
        assert!(islands[0].is_active);
        assert!(islands[1].is_active);
    }
}
