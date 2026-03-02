use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEntity {
    pub id: u64,
    pub archetype_id: u32,
    pub position: [f32; 3],
    pub components: HashMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientEntityState {
    pub entities: HashMap<u64, ClientEntity>,
}

impl ClientEntityState {
    pub fn apply_snapshot(&mut self, data: &[u8]) -> Result<(), String> {
        let state: ClientEntityState = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        *self = state;
        Ok(())
    }

    pub fn get(&self, id: u64) -> Option<&ClientEntity> {
        self.entities.get(&id)
    }

    pub fn count(&self) -> usize {
        self.entities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_snapshot_roundtrip() {
        let mut state = ClientEntityState::default();
        let mut entities = HashMap::new();
        entities.insert(
            1,
            ClientEntity {
                id: 1,
                archetype_id: 10,
                position: [1.0, 2.0, 3.0],
                components: HashMap::new(),
            },
        );
        let src = ClientEntityState { entities };
        let json = serde_json::to_vec(&src).unwrap();
        state.apply_snapshot(&json).unwrap();
        assert_eq!(state.count(), 1);
        let e = state.get(1).unwrap();
        assert_eq!(e.archetype_id, 10);
        assert_eq!(e.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn apply_snapshot_invalid_json() {
        let mut state = ClientEntityState::default();
        assert!(state.apply_snapshot(b"bad").is_err());
    }

    #[test]
    fn default_is_empty() {
        let state = ClientEntityState::default();
        assert_eq!(state.count(), 0);
        assert!(state.get(1).is_none());
    }
}
