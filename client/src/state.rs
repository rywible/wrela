use crate::input::InputFrame;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Entity {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl Entity {
    pub fn new(x: f32, y: f32, size: f32) -> Self {
        Self { x, y, size }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldState {
    #[serde(default = "default_world_width")]
    pub world_width: f32,
    #[serde(default = "default_world_height")]
    pub world_height: f32,
    pub player: Entity,
    #[serde(default)]
    pub collectibles: Vec<Entity>,
}

impl Default for WorldState {
    fn default() -> Self {
        Self {
            world_width: default_world_width(),
            world_height: default_world_height(),
            player: Entity::new(120.0, 120.0, 26.0),
            collectibles: vec![
                Entity::new(230.0, 120.0, 16.0),
                Entity::new(380.0, 170.0, 16.0),
                Entity::new(510.0, 310.0, 16.0),
                Entity::new(250.0, 400.0, 16.0),
                Entity::new(620.0, 240.0, 16.0),
            ],
        }
    }
}

fn default_world_width() -> f32 {
    800.0
}

fn default_world_height() -> f32 {
    600.0
}

impl WorldState {
    pub fn set_bounds(&mut self, width: f32, height: f32) {
        if width > 1.0 {
            self.world_width = width;
        }
        if height > 1.0 {
            self.world_height = height;
        }
        self.clamp_player();
    }

    pub fn integrate_local_input(&mut self, input: &InputFrame, dt_seconds: f32) {
        let speed_px_per_second = 240.0;
        self.player.x += input.axis_x * speed_px_per_second * dt_seconds;
        self.player.y += input.axis_y * speed_px_per_second * dt_seconds;
        self.clamp_player();
        self.collect_pickups();
    }

    pub fn apply_snapshot_payload(&mut self, payload: &[u8]) -> Result<(), StateDecodeError> {
        let decoded = decode_world_payload(payload)?;
        *self = decoded;
        Ok(())
    }

    fn clamp_player(&mut self) {
        let half_size = self.player.size * 0.5;
        self.player.x = self
            .player
            .x
            .clamp(half_size, (self.world_width - half_size).max(half_size));
        self.player.y = self
            .player
            .y
            .clamp(half_size, (self.world_height - half_size).max(half_size));
    }

    fn collect_pickups(&mut self) {
        let player = self.player;
        self.collectibles.retain(|collectible| {
            let dx = collectible.x - player.x;
            let dy = collectible.y - player.y;
            let min_distance = (collectible.size + player.size) * 0.5;
            (dx * dx + dy * dy) > min_distance * min_distance
        });
    }
}

pub fn decode_world_payload(payload: &[u8]) -> Result<WorldState, StateDecodeError> {
    if payload.is_empty() {
        return Err(StateDecodeError::EmptyPayload);
    }

    if let Ok(state) = serde_json::from_slice::<WorldState>(payload) {
        return Ok(state);
    }

    decode_binary_world_payload(payload)
}

pub fn encode_binary_world_payload(state: &WorldState) -> Result<Vec<u8>, StateDecodeError> {
    let collectible_count = u16::try_from(state.collectibles.len())
        .map_err(|_| StateDecodeError::TooManyCollectibles(state.collectibles.len()))?;

    let mut out = Vec::with_capacity(22 + state.collectibles.len() * 12);
    out.extend_from_slice(&state.world_width.to_le_bytes());
    out.extend_from_slice(&state.world_height.to_le_bytes());
    out.extend_from_slice(&state.player.x.to_le_bytes());
    out.extend_from_slice(&state.player.y.to_le_bytes());
    out.extend_from_slice(&state.player.size.to_le_bytes());
    out.extend_from_slice(&collectible_count.to_le_bytes());
    for collectible in &state.collectibles {
        out.extend_from_slice(&collectible.x.to_le_bytes());
        out.extend_from_slice(&collectible.y.to_le_bytes());
        out.extend_from_slice(&collectible.size.to_le_bytes());
    }

    Ok(out)
}

fn decode_binary_world_payload(payload: &[u8]) -> Result<WorldState, StateDecodeError> {
    const HEADER_LEN: usize = 22;

    if payload.len() < HEADER_LEN {
        return Err(StateDecodeError::BinaryPayloadTooShort {
            actual: payload.len(),
            minimum: HEADER_LEN,
        });
    }

    let world_width = f32::from_le_bytes(payload[0..4].try_into().expect("world_width slice"));
    let world_height = f32::from_le_bytes(payload[4..8].try_into().expect("world_height slice"));
    let player_x = f32::from_le_bytes(payload[8..12].try_into().expect("player_x slice"));
    let player_y = f32::from_le_bytes(payload[12..16].try_into().expect("player_y slice"));
    let player_size = f32::from_le_bytes(payload[16..20].try_into().expect("player_size slice"));
    let collectible_count =
        u16::from_le_bytes(payload[20..22].try_into().expect("count slice")) as usize;

    let expected_len = HEADER_LEN + collectible_count * 12;
    if payload.len() != expected_len {
        return Err(StateDecodeError::BinaryLengthMismatch {
            expected: expected_len,
            actual: payload.len(),
        });
    }

    let mut collectibles = Vec::with_capacity(collectible_count);
    let mut offset = HEADER_LEN;
    for _ in 0..collectible_count {
        let x = f32::from_le_bytes(payload[offset..offset + 4].try_into().expect("x slice"));
        offset += 4;
        let y = f32::from_le_bytes(payload[offset..offset + 4].try_into().expect("y slice"));
        offset += 4;
        let size = f32::from_le_bytes(payload[offset..offset + 4].try_into().expect("size slice"));
        offset += 4;

        collectibles.push(Entity::new(x, y, size));
    }

    Ok(WorldState {
        world_width,
        world_height,
        player: Entity::new(player_x, player_y, player_size),
        collectibles,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDecodeError {
    EmptyPayload,
    TooManyCollectibles(usize),
    BinaryPayloadTooShort { actual: usize, minimum: usize },
    BinaryLengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for StateDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPayload => write!(f, "state payload was empty"),
            Self::TooManyCollectibles(count) => {
                write!(f, "too many collectibles for u16 count field: {count}")
            }
            Self::BinaryPayloadTooShort { actual, minimum } => write!(
                f,
                "binary state payload too short: got {actual}, need at least {minimum}"
            ),
            Self::BinaryLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "binary state payload length mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for StateDecodeError {}

#[cfg(test)]
mod tests {
    use super::{Entity, WorldState, decode_world_payload, encode_binary_world_payload};
    use crate::input::{InputButtons, InputSequencer};

    #[test]
    fn local_input_clamps_player_to_world_bounds() {
        let mut world = WorldState::default();
        world.set_bounds(100.0, 80.0);

        let mut buttons = InputButtons::default();
        buttons.set_key("ArrowRight", true);
        buttons.set_key("ArrowDown", true);
        let mut seq = InputSequencer::default();
        for tick in 0..120 {
            let frame = seq.next_frame(tick, &buttons);
            world.integrate_local_input(&frame, 1.0 / 60.0);
        }

        assert!(world.player.x <= world.world_width);
        assert!(world.player.y <= world.world_height);
    }

    #[test]
    fn binary_payload_round_trip() {
        let world = WorldState {
            world_width: 640.0,
            world_height: 360.0,
            player: Entity::new(100.0, 44.0, 24.0),
            collectibles: vec![Entity::new(10.0, 20.0, 8.0), Entity::new(90.0, 42.0, 8.0)],
        };

        let payload = encode_binary_world_payload(&world).expect("encode");
        let decoded = decode_world_payload(&payload).expect("decode");
        assert_eq!(decoded, world);
    }

    #[test]
    fn json_payload_is_supported() {
        let payload = br#"{
            "world_width": 320.0,
            "world_height": 180.0,
            "player": {"x": 12.0, "y": 13.0, "size": 20.0},
            "collectibles": [{"x": 1.0, "y": 2.0, "size": 3.0}]
        }"#;

        let decoded = decode_world_payload(payload).expect("decode");
        assert_eq!(decoded.world_width, 320.0);
        assert_eq!(decoded.collectibles.len(), 1);
    }

    #[test]
    fn pickup_is_consumed_when_player_overlaps() {
        let mut world = WorldState {
            world_width: 200.0,
            world_height: 200.0,
            player: Entity::new(50.0, 50.0, 20.0),
            collectibles: vec![Entity::new(55.0, 50.0, 10.0)],
        };

        let buttons = InputButtons::default();
        let mut seq = InputSequencer::default();
        let frame = seq.next_frame(1, &buttons);
        world.integrate_local_input(&frame, 1.0 / 60.0);

        assert!(world.collectibles.is_empty());
    }
}
