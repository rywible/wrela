use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type EntityId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRecord {
    pub id: EntityId,
    pub archetype_id: u32,
    pub position: [i32; 3],
    pub rotation: [i16; 4],
    pub components: HashMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityRegistry {
    next_id: EntityId,
    entities: HashMap<EntityId, EntityRecord>,
    #[serde(skip)]
    archetype_index: HashMap<u32, Vec<EntityId>>,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entities: HashMap::new(),
            archetype_index: HashMap::new(),
        }
    }

    pub fn spawn(&mut self, archetype_id: u32, x: i32, y: i32, z: i32) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        let record = EntityRecord {
            id,
            archetype_id,
            position: [x, y, z],
            rotation: [0, 0, 0, 1],
            components: HashMap::new(),
        };
        self.entities.insert(id, record);
        self.archetype_index
            .entry(archetype_id)
            .or_default()
            .push(id);
        id
    }

    pub fn despawn(&mut self, entity_id: EntityId) -> bool {
        let Some(record) = self.entities.remove(&entity_id) else {
            return false;
        };
        if let Some(ids) = self.archetype_index.get_mut(&record.archetype_id) {
            ids.retain(|&id| id != entity_id);
            if ids.is_empty() {
                self.archetype_index.remove(&record.archetype_id);
            }
        }
        true
    }

    pub fn set_position(&mut self, entity_id: EntityId, x: i32, y: i32, z: i32) -> bool {
        match self.entities.get_mut(&entity_id) {
            Some(record) => {
                record.position = [x, y, z];
                true
            }
            None => false,
        }
    }

    pub fn get_position(&self, entity_id: EntityId) -> Option<[i32; 3]> {
        self.entities.get(&entity_id).map(|r| r.position)
    }

    pub fn set_component(&mut self, entity_id: EntityId, key: &str, value: i64) -> bool {
        match self.entities.get_mut(&entity_id) {
            Some(record) => {
                record.components.insert(key.to_string(), value);
                true
            }
            None => false,
        }
    }

    pub fn get_component(&self, entity_id: EntityId, key: &str) -> Option<i64> {
        self.entities
            .get(&entity_id)
            .and_then(|r| r.components.get(key).copied())
    }

    pub fn query_by_archetype(&self, archetype_id: u32) -> Vec<EntityId> {
        self.archetype_index
            .get(&archetype_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn query_in_radius(&self, center: [i32; 3], radius: i32) -> Vec<EntityId> {
        let r2 = (radius as i64) * (radius as i64);
        self.entities
            .values()
            .filter(|rec| {
                let dx = (rec.position[0] as i64) - (center[0] as i64);
                let dy = (rec.position[1] as i64) - (center[1] as i64);
                let dz = (rec.position[2] as i64) - (center[2] as i64);
                dx * dx + dy * dy + dz * dz <= r2
            })
            .map(|rec| rec.id)
            .collect()
    }

    pub fn count(&self) -> usize {
        self.entities.len()
    }

    pub fn snapshot_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("EntityRegistry serialization should not fail")
    }

    pub fn apply_snapshot_json(&mut self, data: &[u8]) -> Result<(), String> {
        let mut loaded: EntityRegistry = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        let max_id = loaded.entities.keys().copied().max().unwrap_or(0);
        if loaded.next_id <= max_id {
            loaded.next_id = max_id + 1;
        }
        loaded.rebuild_archetype_index();
        *self = loaded;
        Ok(())
    }

    fn rebuild_archetype_index(&mut self) {
        self.archetype_index.clear();
        for (&id, record) in &self.entities {
            self.archetype_index
                .entry(record.archetype_id)
                .or_default()
                .push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_returns_monotonically_increasing_ids() {
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(1, 0, 0, 0);
        let b = reg.spawn(1, 0, 0, 0);
        let c = reg.spawn(2, 0, 0, 0);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(c, 3);
    }

    #[test]
    fn despawn_removes_entity() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 10, 20, 30);
        assert!(reg.get_position(id).is_some());
        assert!(reg.despawn(id));
        assert!(reg.get_position(id).is_none());
    }

    #[test]
    fn despawn_nonexistent_returns_false() {
        let mut reg = EntityRegistry::new();
        assert!(!reg.despawn(999));
    }

    #[test]
    fn set_get_position_roundtrip() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 0, 0, 0);
        assert!(reg.set_position(id, 100, 200, 300));
        assert_eq!(reg.get_position(id), Some([100, 200, 300]));
    }

    #[test]
    fn set_position_nonexistent_returns_false() {
        let mut reg = EntityRegistry::new();
        assert!(!reg.set_position(999, 1, 2, 3));
    }

    #[test]
    fn set_get_component_roundtrip() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 0, 0, 0);
        assert!(reg.set_component(id, "health", 100));
        assert_eq!(reg.get_component(id, "health"), Some(100));
        assert_eq!(reg.get_component(id, "missing"), None);
    }

    #[test]
    fn set_component_nonexistent_returns_false() {
        let mut reg = EntityRegistry::new();
        assert!(!reg.set_component(999, "x", 1));
    }

    #[test]
    fn get_component_nonexistent_returns_none() {
        let reg = EntityRegistry::new();
        assert_eq!(reg.get_component(999, "x"), None);
    }

    #[test]
    fn query_by_archetype_returns_correct_entities() {
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(10, 0, 0, 0);
        let b = reg.spawn(10, 0, 0, 0);
        let _c = reg.spawn(20, 0, 0, 0);
        let mut result = reg.query_by_archetype(10);
        result.sort();
        assert_eq!(result, vec![a, b]);
        assert_eq!(reg.query_by_archetype(99), Vec::<EntityId>::new());
    }

    #[test]
    fn query_in_radius_returns_entities_within_distance() {
        let mut reg = EntityRegistry::new();
        let near = reg.spawn(1, 1, 1, 1);
        let _far = reg.spawn(1, 1000, 1000, 1000);
        let result = reg.query_in_radius([0, 0, 0], 10);
        assert_eq!(result, vec![near]);
    }

    #[test]
    fn query_in_radius_includes_boundary() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 3, 4, 0);
        let result = reg.query_in_radius([0, 0, 0], 5);
        assert!(result.contains(&id));
    }

    #[test]
    fn spawn_1000_entities_verify_count() {
        let mut reg = EntityRegistry::new();
        for i in 0..1000 {
            reg.spawn(i % 10, 0, 0, 0);
        }
        assert_eq!(reg.count(), 1000);
    }

    #[test]
    fn despawn_removes_from_archetype_index() {
        let mut reg = EntityRegistry::new();
        let a = reg.spawn(5, 0, 0, 0);
        let b = reg.spawn(5, 0, 0, 0);
        reg.despawn(a);
        let result = reg.query_by_archetype(5);
        assert_eq!(result, vec![b]);
    }

    #[test]
    fn snapshot_json_apply_roundtrip() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 10, 20, 30);
        reg.set_component(id, "hp", 42);
        reg.spawn(2, 40, 50, 60);

        let snap = reg.snapshot_json();

        let mut reg2 = EntityRegistry::new();
        reg2.apply_snapshot_json(&snap).unwrap();

        assert_eq!(reg2.count(), 2);
        assert_eq!(reg2.get_position(id), Some([10, 20, 30]));
        assert_eq!(reg2.get_component(id, "hp"), Some(42));
        assert_eq!(reg2.query_by_archetype(1), vec![id]);
    }

    #[test]
    fn apply_snapshot_invalid_json_returns_error() {
        let mut reg = EntityRegistry::new();
        assert!(reg.apply_snapshot_json(b"not json").is_err());
    }

    #[test]
    fn component_overwrite() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, 0, 0, 0);
        reg.set_component(id, "score", 10);
        reg.set_component(id, "score", 20);
        assert_eq!(reg.get_component(id, "score"), Some(20));
    }

    #[test]
    fn position_after_spawn_matches_args() {
        let mut reg = EntityRegistry::new();
        let id = reg.spawn(1, -5, 100, 0);
        assert_eq!(reg.get_position(id), Some([-5, 100, 0]));
    }
}
