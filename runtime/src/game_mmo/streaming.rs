use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewQueryV1 {
    pub shard_id: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub quality_tier: String,
    pub nearby_player_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamTileRequestV1 {
    pub tile_id: String,
    pub priority: u32,
    pub tile_kind: String,
}

pub fn plan_stream_tiles(
    query: &ViewQueryV1,
    active_island_ids: &[u64],
) -> Vec<StreamTileRequestV1> {
    let mut requests = Vec::<StreamTileRequestV1>::new();
    let base_priority = if query.quality_tier == "hero" {
        100
    } else {
        60
    };
    requests.push(StreamTileRequestV1 {
        tile_id: format!(
            "{}:mesh:{}:{}",
            query.shard_id, query.position[0] as i32, query.position[2] as i32
        ),
        priority: base_priority,
        tile_kind: "mesh".to_string(),
    });
    requests.push(StreamTileRequestV1 {
        tile_id: format!("{}:material:{}", query.shard_id, query.quality_tier),
        priority: base_priority.saturating_sub(10),
        tile_kind: "texture".to_string(),
    });
    for island_id in active_island_ids {
        requests.push(StreamTileRequestV1 {
            tile_id: format!("{}:physics-island:{}", query.shard_id, island_id),
            priority: base_priority.saturating_add(20),
            tile_kind: "physics".to_string(),
        });
    }
    requests.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.tile_id.cmp(&right.tile_id))
    });
    requests
}

#[cfg(test)]
mod tests {
    use super::{ViewQueryV1, plan_stream_tiles};

    #[test]
    fn streaming_planner_is_deterministic() {
        let query = ViewQueryV1 {
            shard_id: "s-1".to_string(),
            position: [10.0, 0.0, 20.0],
            velocity: [1.0, 0.0, 0.0],
            quality_tier: "hero".to_string(),
            nearby_player_count: 12,
        };
        let a = plan_stream_tiles(&query, &[7, 8]);
        let b = plan_stream_tiles(&query, &[7, 8]);
        assert_eq!(a, b);
        assert!(a[0].tile_kind == "physics");
    }
}
