use super::collision::{broadphase_pairs, detect_contacts};
use super::core::{
    ColliderV1, ContactManifoldV1, FixedMilli, PhysicsBodyStateV1, PhysicsBodyV1,
    fixed_step_integrate,
};
use super::destruction::BreakableJointV1;
use super::solver::solve_contacts;
use crate::entity::EntityRegistry;
use std::collections::HashMap;

pub struct PhysicsWorld {
    bodies: HashMap<u64, PhysicsBodyV1>,
    contacts: Vec<ContactManifoldV1>,
    breakable_joints: Vec<BreakableJointV1>,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            bodies: HashMap::new(),
            contacts: Vec::new(),
            breakable_joints: Vec::new(),
        }
    }

    pub fn create_body(
        &mut self,
        entity_id: u64,
        collider: ColliderV1,
        mass_milli: FixedMilli,
        x: FixedMilli,
        y: FixedMilli,
        z: FixedMilli,
    ) {
        let body = PhysicsBodyV1 {
            body_id: entity_id,
            state: PhysicsBodyStateV1::Active,
            position_milli: [x, y, z],
            velocity_milli_per_s: [0, 0, 0],
            mass_milli,
            collider,
        };
        self.bodies.insert(entity_id, body);
    }

    pub fn remove_body(&mut self, entity_id: u64) -> bool {
        self.breakable_joints
            .retain(|j| j.parent != entity_id && j.child != entity_id);
        self.bodies.remove(&entity_id).is_some()
    }

    pub fn set_velocity(
        &mut self,
        entity_id: u64,
        vx: FixedMilli,
        vy: FixedMilli,
        vz: FixedMilli,
    ) -> bool {
        match self.bodies.get_mut(&entity_id) {
            Some(body) => {
                body.velocity_milli_per_s = [vx, vy, vz];
                true
            }
            None => false,
        }
    }

    pub fn get_position(&self, entity_id: u64) -> Option<[FixedMilli; 3]> {
        self.bodies.get(&entity_id).map(|b| b.position_milli)
    }

    pub fn step(&mut self, dt_milli: u32, entity_registry: &mut EntityRegistry) {
        let mut body_vec: Vec<PhysicsBodyV1> = self.bodies.values().cloned().collect();

        fixed_step_integrate(&mut body_vec, dt_milli);

        let _pairs = broadphase_pairs(&body_vec);
        let contacts = detect_contacts(&body_vec);

        solve_contacts(&mut body_vec, &contacts, 500, 100);

        self.contacts = contacts;

        for body in &body_vec {
            entity_registry.set_position(
                body.body_id,
                body.position_milli[0] as i32,
                body.position_milli[1] as i32,
                body.position_milli[2] as i32,
            );
            self.bodies.insert(body.body_id, body.clone());
        }
    }

    pub fn query_contacts(&self, entity_id: u64) -> Vec<u64> {
        let mut result = Vec::new();
        for c in &self.contacts {
            if c.a == entity_id {
                result.push(c.b);
            } else if c.b == entity_id {
                result.push(c.a);
            }
        }
        result
    }

    pub fn add_breakable_joint(&mut self, parent: u64, child: u64, threshold: FixedMilli) {
        self.breakable_joints.push(BreakableJointV1 {
            parent,
            child,
            break_threshold_milli: threshold,
        });
    }

    pub fn raycast(
        &self,
        origin: [FixedMilli; 3],
        direction: [FixedMilli; 3],
        max_dist: FixedMilli,
    ) -> Option<(u64, FixedMilli)> {
        let mut best: Option<(u64, FixedMilli)> = None;

        for (&eid, body) in &self.bodies {
            let half = match &body.collider {
                ColliderV1::Sphere { radius_milli } => {
                    [*radius_milli, *radius_milli, *radius_milli]
                }
                ColliderV1::Aabb {
                    half_extent_x_milli,
                    half_extent_y_milli,
                    half_extent_z_milli,
                } => [
                    *half_extent_x_milli,
                    *half_extent_y_milli,
                    *half_extent_z_milli,
                ],
            };

            let t = ray_aabb_intersect(origin, direction, body.position_milli, half);
            if let Some(dist) = t {
                if dist >= 0 && dist <= max_dist {
                    if best.map_or(true, |(_, d)| dist < d) {
                        best = Some((eid, dist));
                    }
                }
            }
        }

        best
    }
}

/// Slab-method ray vs AABB intersection in fixed-point milli-units.
/// Returns Some(t) where t is the entry distance along the ray, or None.
fn ray_aabb_intersect(
    origin: [FixedMilli; 3],
    direction: [FixedMilli; 3],
    center: [FixedMilli; 3],
    half: [FixedMilli; 3],
) -> Option<FixedMilli> {
    let mut t_min = i64::MIN;
    let mut t_max = i64::MAX;

    for axis in 0..3 {
        let d = direction[axis];
        let o = origin[axis];
        let lo = center[axis] - half[axis];
        let hi = center[axis] + half[axis];

        if d == 0 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            // t = (boundary - origin) * 1000 / direction  (milli-scaled)
            let t1 = ((lo - o) * 1000) / d;
            let t2 = ((hi - o) * 1000) / d;
            let (t_near, t_far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            t_min = t_min.max(t_near);
            t_max = t_max.min(t_far);
            if t_min > t_max {
                return None;
            }
        }
    }

    Some(t_min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityRegistry;

    fn aabb_collider(hx: i64, hy: i64, hz: i64) -> ColliderV1 {
        ColliderV1::Aabb {
            half_extent_x_milli: hx,
            half_extent_y_milli: hy,
            half_extent_z_milli: hz,
        }
    }

    #[test]
    fn physics_world_create_and_step_integrates_velocity() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let eid = reg.spawn(1, 0, 0, 0);

        world.create_body(eid, aabb_collider(500, 500, 500), 1000, 0, 0, 0);
        world.set_velocity(eid, 2000, 0, 0);
        world.step(100, &mut reg);

        let pos = world.get_position(eid).unwrap();
        assert_eq!(pos[0], 200);
        assert_eq!(pos[1], 0);
        assert_eq!(pos[2], 0);
    }

    #[test]
    fn physics_world_collision_detection_between_overlapping_bodies() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(1, 0, 0, 0);
        let b = reg.spawn(1, 0, 0, 0);

        world.create_body(a, aabb_collider(1000, 1000, 1000), 1000, 0, 0, 0);
        world.create_body(b, aabb_collider(1000, 1000, 1000), 1000, 500, 0, 0);
        world.step(0, &mut reg);

        let contacts = world.query_contacts(a);
        assert!(contacts.contains(&b));
    }

    #[test]
    fn physics_world_query_contacts_returns_correct_entity_ids() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(1, 0, 0, 0);
        let b = reg.spawn(1, 0, 0, 0);
        let c = reg.spawn(1, 0, 0, 0);

        world.create_body(a, aabb_collider(1000, 1000, 1000), 1000, 0, 0, 0);
        world.create_body(b, aabb_collider(1000, 1000, 1000), 1000, 500, 0, 0);
        world.create_body(c, aabb_collider(1000, 1000, 1000), 1000, 50000, 0, 0);
        world.step(0, &mut reg);

        let contacts_a = world.query_contacts(a);
        assert!(contacts_a.contains(&b));
        assert!(!contacts_a.contains(&c));

        let contacts_c = world.query_contacts(c);
        assert!(contacts_c.is_empty());
    }

    #[test]
    fn physics_world_remove_body_cleans_up() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(1, 0, 0, 0);
        let b = reg.spawn(1, 0, 0, 0);

        world.create_body(a, aabb_collider(500, 500, 500), 1000, 0, 0, 0);
        world.create_body(b, aabb_collider(500, 500, 500), 1000, 100, 0, 0);
        world.add_breakable_joint(a, b, 1000);

        assert!(world.remove_body(a));
        assert!(!world.remove_body(a));
        assert!(world.get_position(a).is_none());
        assert!(world.breakable_joints.is_empty());
    }

    #[test]
    fn physics_world_raycast_hits_nearest_body() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let near = reg.spawn(1, 0, 0, 0);
        let far = reg.spawn(1, 0, 0, 0);

        world.create_body(near, aabb_collider(500, 500, 500), 1000, 3000, 0, 0);
        world.create_body(far, aabb_collider(500, 500, 500), 1000, 10000, 0, 0);

        let hit = world.raycast([0, 0, 0], [1000, 0, 0], 100_000);
        assert!(hit.is_some());
        let (eid, _dist) = hit.unwrap();
        assert_eq!(eid, near);
    }

    #[test]
    fn physics_world_raycast_misses_when_no_bodies_in_path() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let eid = reg.spawn(1, 0, 0, 0);

        world.create_body(eid, aabb_collider(500, 500, 500), 1000, 0, 5000, 0);

        let hit = world.raycast([0, 0, 0], [1000, 0, 0], 100_000);
        assert!(hit.is_none());
    }

    #[test]
    fn physics_world_add_breakable_joint_and_query() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(1, 0, 0, 0);
        let b = reg.spawn(1, 0, 0, 0);

        world.create_body(a, aabb_collider(500, 500, 500), 1000, 0, 0, 0);
        world.create_body(b, aabb_collider(500, 500, 500), 1000, 100, 0, 0);
        world.add_breakable_joint(a, b, 1000);

        assert_eq!(world.breakable_joints.len(), 1);
        assert_eq!(world.breakable_joints[0].parent, a);
        assert_eq!(world.breakable_joints[0].child, b);
        assert_eq!(world.breakable_joints[0].break_threshold_milli, 1000);
    }

    #[test]
    fn physics_world_step_syncs_positions_to_entity_registry() {
        let mut world = PhysicsWorld::new();
        let mut reg = EntityRegistry::new();
        let eid = reg.spawn(1, 0, 0, 0);

        world.create_body(eid, aabb_collider(500, 500, 500), 1000, 1000, 2000, 3000);
        world.set_velocity(eid, 10000, 0, 0);
        world.step(100, &mut reg);

        let entity_pos = reg.get_position(eid).unwrap();
        let physics_pos = world.get_position(eid).unwrap();
        assert_eq!(entity_pos[0], physics_pos[0] as i32);
        assert_eq!(entity_pos[1], physics_pos[1] as i32);
        assert_eq!(entity_pos[2], physics_pos[2] as i32);
    }

    #[test]
    fn physics_world_set_velocity_nonexistent_returns_false() {
        let mut world = PhysicsWorld::new();
        assert!(!world.set_velocity(999, 1, 2, 3));
    }

    #[test]
    fn physics_world_get_position_nonexistent_returns_none() {
        let world = PhysicsWorld::new();
        assert!(world.get_position(999).is_none());
    }
}
