use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldEventV1 {
    pub event_id: u64,
    pub revision: u64,
    pub shard_id: String,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorldEventLogV1 {
    pub events: Vec<WorldEventV1>,
}

impl WorldEventLogV1 {
    pub fn append(&mut self, event: WorldEventV1) {
        self.events.push(event);
        self.events
            .sort_by_key(|entry| (entry.revision, entry.event_id));
    }

    pub fn slice_from_revision(&self, revision: u64) -> Vec<WorldEventV1> {
        self.events
            .iter()
            .filter(|event| event.revision >= revision)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{WorldEventLogV1, WorldEventV1};

    #[test]
    fn event_log_keeps_revision_order() {
        let mut log = WorldEventLogV1::default();
        log.append(WorldEventV1 {
            event_id: 2,
            revision: 2,
            shard_id: "s1".to_string(),
            kind: "edit".to_string(),
            payload: "b".to_string(),
        });
        log.append(WorldEventV1 {
            event_id: 1,
            revision: 1,
            shard_id: "s1".to_string(),
            kind: "edit".to_string(),
            payload: "a".to_string(),
        });
        assert_eq!(log.events[0].revision, 1);
        assert_eq!(log.events[1].revision, 2);
    }
}
