use crate::web::axum_bridge::{
    MESSAGE_TYPE_CORRECTION_V5, MESSAGE_TYPE_DELTA_V5, MESSAGE_TYPE_ERROR_V5,
    MESSAGE_TYPE_HELLO_V5, MESSAGE_TYPE_INPUT_BATCH_V5, MESSAGE_TYPE_PING_V5,
    MESSAGE_TYPE_SNAPSHOT_V5, OutboundFrame, PROTOCOL_V5_SUB_VERSION, PROTOCOL_V5_VERSION,
    ProtocolEnvelope, WebSocketFrame, read_http_handshake, read_websocket_frame,
    write_bootstrap_response, write_outbound_frame, write_websocket_upgrade_response,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::timeout;

const FIXED_SCALE: i32 = 1024;
const WORLD_WIDTH_FIXED: i32 = 800 * FIXED_SCALE;
const WORLD_HEIGHT_FIXED: i32 = 600 * FIXED_SCALE;
const PLAYER_SPEED_FIXED: i32 = 5376;
const COLLISION_RADIUS_FIXED: i32 = 28 * FIXED_SCALE;
const COLLISION_RADIUS_SQ_FIXED: i64 =
    (COLLISION_RADIUS_FIXED as i64) * (COLLISION_RADIUS_FIXED as i64);
const WORLD_WIDTH: f32 = WORLD_WIDTH_FIXED as f32 / FIXED_SCALE as f32;
const WORLD_HEIGHT: f32 = WORLD_HEIGHT_FIXED as f32 / FIXED_SCALE as f32;
const HASH_PRIME: u64 = 1_099_511_628_211;
const DEFAULT_PARTITION_ID: u64 = 0;
const MIN_AXIS_INPUT: f32 = -1.0;
const MAX_AXIS_INPUT: f32 = 1.0;
const MIN_INPUT_DT_MS: u32 = 1;
const MAX_INPUT_DT_MS: u32 = 100;
const MAX_INPUT_BATCH_INPUTS: usize = 128;
const PROTOCOL_IDENTITY: &str = "protocol-v5";
const PROTOCOL_METADATA_ARTIFACT: &str = "protocol-v5.json";
const MMO_RUNTIME_ROLE_ENV: &str = "WRELA_GAME_MMO_ROLE";
const MMO_RUNTIME_ROLE_ENV_ALIAS: &str = "WRELA_MMO_ROLE";

const DEFAULT_COLLECTIBLE_POSITIONS_FIXED: &[(i32, i32)] = &[
    (160 * FIXED_SCALE, 120 * FIXED_SCALE),
    (340 * FIXED_SCALE, 180 * FIXED_SCALE),
    (560 * FIXED_SCALE, 240 * FIXED_SCALE),
    (220 * FIXED_SCALE, 400 * FIXED_SCALE),
    (650 * FIXED_SCALE, 320 * FIXED_SCALE),
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MmoRuntimeRole {
    Gateway,
    Zone,
    World,
}

impl MmoRuntimeRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::Zone => "zone",
            Self::World => "world",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "gateway" => Some(Self::Gateway),
            "zone" => Some(Self::Zone),
            "world" => Some(Self::World),
            _ => None,
        }
    }
}

fn resolve_mmo_runtime_role_from_sources(
    configured_role: Option<&str>,
    env_role: Option<&str>,
    env_role_alias: Option<&str>,
) -> MmoRuntimeRole {
    configured_role
        .and_then(MmoRuntimeRole::parse)
        .or_else(|| env_role.and_then(MmoRuntimeRole::parse))
        .or_else(|| env_role_alias.and_then(MmoRuntimeRole::parse))
        .unwrap_or(MmoRuntimeRole::World)
}

fn resolve_mmo_runtime_role(configured_role: Option<&str>) -> MmoRuntimeRole {
    let env_role = std::env::var(MMO_RUNTIME_ROLE_ENV).ok();
    let env_role_alias = std::env::var(MMO_RUNTIME_ROLE_ENV_ALIAS).ok();
    resolve_mmo_runtime_role_from_sources(
        configured_role,
        env_role.as_deref(),
        env_role_alias.as_deref(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainAbiDescriptor {
    domain_source_hash: String,
    source_seed: u64,
    collectibles: Vec<(i32, i32)>,
}

impl DomainAbiDescriptor {
    fn from_static_root(static_root: &Path) -> Self {
        let descriptor_path = static_root.join("domain-abi.json");
        let Ok(bytes) = std::fs::read(&descriptor_path) else {
            return Self::default();
        };
        serde_json::from_slice::<Self>(&bytes)
            .map(Self::sanitize_collectibles)
            .unwrap_or_else(|_| Self::default())
    }

    fn sanitize_collectibles(mut self) -> Self {
        if self.collectibles.len() > u32::BITS as usize {
            self.collectibles.truncate(u32::BITS as usize);
        }
        self
    }
}

impl Default for DomainAbiDescriptor {
    fn default() -> Self {
        Self {
            domain_source_hash: "fallback-runtime-descriptor".to_string(),
            source_seed: 0xcbf2_9ce4_8422_2325,
            collectibles: DEFAULT_COLLECTIBLE_POSITIONS_FIXED.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct DomainState {
    tick: u64,
    player_x_fixed: i32,
    player_y_fixed: i32,
    score: u32,
    collected_mask: u32,
}

impl Default for DomainState {
    fn default() -> Self {
        Self {
            tick: 0,
            player_x_fixed: WORLD_WIDTH_FIXED / 2,
            player_y_fixed: WORLD_HEIGHT_FIXED / 2,
            score: 0,
            collected_mask: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentWorldRecord {
    state: DomainState,
    last_ack: u64,
}

#[derive(Debug, Clone)]
struct WorldAuthority {
    state: DomainState,
    last_ack: u64,
    persisted_path: PathBuf,
}

fn write_persistent_world_record(
    persisted_path: &Path,
    record: &PersistentWorldRecord,
) -> Result<(), String> {
    if let Some(parent) = persisted_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed creating world persistence directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("failed to serialize world persistence record: {error}"))?;
    std::fs::write(persisted_path, bytes).map_err(|error| {
        format!(
            "failed writing world persistence file {}: {error}",
            persisted_path.display()
        )
    })
}

impl WorldAuthority {
    fn new(persisted_path: PathBuf) -> Self {
        Self {
            state: DomainState::default(),
            last_ack: 0,
            persisted_path,
        }
    }

    fn load_or_default(persisted_path: PathBuf) -> Self {
        let Ok(bytes) = std::fs::read(&persisted_path) else {
            return Self::new(persisted_path);
        };
        let Ok(record) = serde_json::from_slice::<PersistentWorldRecord>(&bytes) else {
            return Self::new(persisted_path);
        };
        Self {
            state: record.state,
            last_ack: record.last_ack,
            persisted_path,
        }
    }

    fn persist(&self) -> Result<(), String> {
        write_persistent_world_record(
            self.persisted_path.as_path(),
            &PersistentWorldRecord {
                state: self.state,
                last_ack: self.last_ack,
            },
        )
    }
}

fn world_state_path_for_static_root(static_root: &Path) -> PathBuf {
    let canonical_static_root =
        std::fs::canonicalize(static_root).unwrap_or_else(|_| static_root.to_path_buf());
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in canonical_static_root.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(HASH_PRIME);
    }
    PathBuf::from(".wrela")
        .join("realtime-state")
        .join(format!("{hash:016x}"))
        .join("world-state-v1.json")
}

async fn persist_world_state_after_unlock(
    authority: &Arc<Mutex<WorldAuthority>>,
    persisted_path: PathBuf,
    state: DomainState,
    last_ack: u64,
) -> Result<(), String> {
    let mut record = PersistentWorldRecord { state, last_ack };
    loop {
        persist_world_record_in_background(persisted_path.clone(), record.clone()).await?;
        let next_record = {
            let world = authority.lock().await;
            if world.state == record.state && world.last_ack == record.last_ack {
                None
            } else {
                Some(PersistentWorldRecord {
                    state: world.state,
                    last_ack: world.last_ack,
                })
            }
        };
        if let Some(next_record) = next_record {
            record = next_record;
            continue;
        }
        return Ok(());
    }
}

async fn persist_world_record_in_background(
    persisted_path: PathBuf,
    record: PersistentWorldRecord,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        WorldAuthority {
            state: record.state,
            last_ack: record.last_ack,
            persisted_path,
        }
        .persist()
    })
    .await
    .map_err(|error| format!("world persistence worker join failure: {error}"))?
}

#[derive(Debug, Clone, Serialize)]
struct DomainSnapshot {
    tick: u64,
    player_x: f32,
    player_y: f32,
    score: u32,
    collected_mask: u32,
    hash: u64,
    anim_state_id: String,
    anim_phase_q16: i32,
    anim_event_markers: Vec<String>,
    anim_root_motion_q16: i32,
    anim_reconcile_seq: u64,
}

impl DomainSnapshot {
    fn from_state(state: DomainState, source_seed: u64) -> Self {
        let anim_state_id = if state.score == 0 {
            "traveller.idle"
        } else if state.score < 3 {
            "traveller.locomotion"
        } else {
            "traveller.attack"
        };
        let anim_phase_q16 = ((state.tick % 65_536) as i32) * 256;
        let anim_event_markers = if state.tick % 20 == 0 {
            vec!["hit_window_open".to_string()]
        } else if state.tick % 20 == 10 {
            vec!["hit_window_close".to_string()]
        } else {
            Vec::new()
        };
        let anim_root_motion_q16 = (state.player_x_fixed + state.player_y_fixed) / 2;
        Self {
            tick: state.tick,
            player_x: fixed_to_float(state.player_x_fixed),
            player_y: fixed_to_float(state.player_y_fixed),
            score: state.score,
            collected_mask: state.collected_mask,
            hash: hash_state(&state, source_seed),
            anim_state_id: anim_state_id.to_string(),
            anim_phase_q16,
            anim_event_markers,
            anim_root_motion_q16,
            anim_reconcile_seq: state.tick,
        }
    }
}

fn axis_to_fixed(axis: f32) -> i32 {
    (axis * FIXED_SCALE as f32).trunc() as i32
}

fn clamp_axis_input(axis: f32) -> f32 {
    if !axis.is_finite() {
        return 0.0;
    }
    axis.clamp(MIN_AXIS_INPUT, MAX_AXIS_INPUT)
}

fn clamp_input_dt_ms(dt_ms: u32) -> u32 {
    dt_ms.clamp(MIN_INPUT_DT_MS, MAX_INPUT_DT_MS)
}

fn fixed_to_float(value: i32) -> f32 {
    value as f32 / FIXED_SCALE as f32
}

fn apply_input(
    state: &mut DomainState,
    descriptor: &DomainAbiDescriptor,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
) {
    let dt = i64::from(clamp_input_dt_ms(dt_ms));
    let axis_x_fixed = i64::from(axis_to_fixed(clamp_axis_input(axis_x)));
    let axis_y_fixed = i64::from(axis_to_fixed(clamp_axis_input(axis_y)));
    let speed = i64::from(PLAYER_SPEED_FIXED);
    let denominator = i64::from(16 * FIXED_SCALE);
    let delta_x = ((axis_x_fixed * speed * dt) / denominator) as i32;
    let delta_y = ((axis_y_fixed * speed * dt) / denominator) as i32;
    state.player_x_fixed = (state.player_x_fixed + delta_x).clamp(0, WORLD_WIDTH_FIXED);
    state.player_y_fixed = (state.player_y_fixed + delta_y).clamp(0, WORLD_HEIGHT_FIXED);
    state.tick = state.tick.saturating_add(1);
    collect_collisions(state, descriptor);
}

fn force_divergence(state: &mut DomainState, x_offset_fixed: i32) {
    state.player_x_fixed = (state.player_x_fixed + x_offset_fixed).clamp(0, WORLD_WIDTH_FIXED);
}

fn collect_collisions(state: &mut DomainState, descriptor: &DomainAbiDescriptor) {
    for (idx, (x, y)) in descriptor.collectibles.iter().enumerate() {
        if idx >= u32::BITS as usize {
            break;
        }
        let mask = 1u32 << idx;
        if state.collected_mask & mask != 0 {
            continue;
        }
        let dx = i64::from(state.player_x_fixed - *x);
        let dy = i64::from(state.player_y_fixed - *y);
        if dx * dx + dy * dy < COLLISION_RADIUS_SQ_FIXED {
            state.collected_mask |= mask;
            state.score = state.score.saturating_add(1);
        }
    }
}

fn collectible_target_count(descriptor: &DomainAbiDescriptor) -> u32 {
    let total = descriptor.collectibles.len().min(u32::BITS as usize) as u32;
    if total == 0 { 0 } else { 1 }
}

fn is_winning_state(state: &DomainState, descriptor: &DomainAbiDescriptor) -> bool {
    let target = collectible_target_count(descriptor);
    target > 0 && state.score >= target
}

fn restart_state(state: &mut DomainState) {
    *state = DomainState::default();
}

fn hash_state(state: &DomainState, source_seed: u64) -> u64 {
    let mut hash = source_seed;
    hash ^= state.tick;
    hash = hash.wrapping_mul(HASH_PRIME);
    hash ^= u64::from(state.player_x_fixed as u32);
    hash = hash.wrapping_mul(HASH_PRIME);
    hash ^= u64::from(state.player_y_fixed as u32);
    hash = hash.wrapping_mul(HASH_PRIME);
    hash ^= u64::from(state.score);
    hash = hash.wrapping_mul(HASH_PRIME);
    hash ^= u64::from(state.collected_mask);
    hash = hash.wrapping_mul(HASH_PRIME);
    hash
}

#[derive(Debug, Clone)]
pub struct VerticalSliceServerConfig {
    pub bind_address: String,
    pub static_root: PathBuf,
    pub artifact_root: Option<PathBuf>,
    pub heartbeat_ms: u64,
    pub force_divergence_interval: u64,
}

impl Default for VerticalSliceServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8091".to_string(),
            static_root: PathBuf::from("."),
            artifact_root: None,
            heartbeat_ms: 5_000,
            force_divergence_interval: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct HelloPayload {
    protocol: &'static str,
    session_id: u64,
    role: &'static str,
    world_width: f32,
    world_height: f32,
    collectibles: Vec<(f32, f32)>,
    snapshot: DomainSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct CorrectionPayload {
    role: &'static str,
    ack: u64,
    forced_divergence: bool,
    rollback_ring_len: u32,
    snapshot: DomainSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct StateSnapshotPayload {
    role: &'static str,
    reason: &'static str,
    ack: u64,
    snapshot: DomainSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct StateDeltaPayload {
    role: &'static str,
    ack: u64,
    forced_divergence: bool,
    delta_kind: &'static str,
    delta: SnapshotDelta,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotDelta {
    tick: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collected_mask: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anim_state_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anim_phase_q16: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anim_event_markers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anim_root_motion_q16: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anim_reconcile_seq: Option<u64>,
}

fn compact_snapshot_delta(
    previous: Option<&DomainSnapshot>,
    current: &DomainSnapshot,
) -> SnapshotDelta {
    let player_x = previous
        .filter(|snapshot| snapshot.player_x.to_bits() == current.player_x.to_bits())
        .map(|_| None)
        .unwrap_or(Some(current.player_x));
    let player_y = previous
        .filter(|snapshot| snapshot.player_y.to_bits() == current.player_y.to_bits())
        .map(|_| None)
        .unwrap_or(Some(current.player_y));
    let score = previous
        .filter(|snapshot| snapshot.score == current.score)
        .map(|_| None)
        .unwrap_or(Some(current.score));
    let collected_mask = previous
        .filter(|snapshot| snapshot.collected_mask == current.collected_mask)
        .map(|_| None)
        .unwrap_or(Some(current.collected_mask));
    let hash = previous
        .filter(|snapshot| snapshot.hash == current.hash)
        .map(|_| None)
        .unwrap_or(Some(current.hash));
    let anim_state_id = previous
        .filter(|snapshot| snapshot.anim_state_id == current.anim_state_id)
        .map(|_| None)
        .unwrap_or(Some(current.anim_state_id.clone()));
    let anim_phase_q16 = previous
        .filter(|snapshot| snapshot.anim_phase_q16 == current.anim_phase_q16)
        .map(|_| None)
        .unwrap_or(Some(current.anim_phase_q16));
    let anim_event_markers = previous
        .filter(|snapshot| snapshot.anim_event_markers == current.anim_event_markers)
        .map(|_| None)
        .unwrap_or(Some(current.anim_event_markers.clone()));
    let anim_root_motion_q16 = previous
        .filter(|snapshot| snapshot.anim_root_motion_q16 == current.anim_root_motion_q16)
        .map(|_| None)
        .unwrap_or(Some(current.anim_root_motion_q16));
    let anim_reconcile_seq = previous
        .filter(|snapshot| snapshot.anim_reconcile_seq == current.anim_reconcile_seq)
        .map(|_| None)
        .unwrap_or(Some(current.anim_reconcile_seq));

    SnapshotDelta {
        tick: current.tick,
        player_x,
        player_y,
        score,
        collected_mask,
        hash,
        anim_state_id,
        anim_phase_q16,
        anim_event_markers,
        anim_root_motion_q16,
        anim_reconcile_seq,
    }
}

#[derive(Debug, Clone, Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<ErrorPayloadDetails>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ErrorPayloadDetails {
    InputBatchTooLarge {
        max_inputs: usize,
        actual_inputs: usize,
    },
    VersionMismatch {
        expected_protocol: &'static str,
        metadata_artifact: &'static str,
        expected_version: u16,
        expected_sub_version: u16,
        actual_version: u16,
        actual_sub_version: u16,
    },
    SubVersionMismatch {
        expected_protocol: &'static str,
        metadata_artifact: &'static str,
        expected_sub_version: u16,
        actual_sub_version: u16,
    },
    IdentityMismatch {
        expected_session_id: u64,
        actual_session_id: u64,
        expected_partition_id: u64,
        actual_partition_id: u64,
        expected_actor_id: u64,
        actual_actor_id: u64,
    },
}

impl ErrorPayload {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    fn with_details(
        code: &'static str,
        message: impl Into<String>,
        details: ErrorPayloadDetails,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details),
        }
    }
}

fn oversized_input_batch_error(actual_inputs: usize) -> ErrorPayload {
    ErrorPayload::with_details(
        "input_batch_too_large",
        format!("input batch has {actual_inputs} entries; max allowed is {MAX_INPUT_BATCH_INPUTS}"),
        ErrorPayloadDetails::InputBatchTooLarge {
            max_inputs: MAX_INPUT_BATCH_INPUTS,
            actual_inputs,
        },
    )
}

fn version_mismatch_error(actual_version: u16, actual_sub_version: u16) -> ErrorPayload {
    ErrorPayload::with_details(
        "version_mismatch",
        format!(
            "unsupported protocol version={} sub_version={}; expected protocol={} version={} sub_version={}. Upgrade client metadata artifact '{}'.",
            actual_version,
            actual_sub_version,
            PROTOCOL_IDENTITY,
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            PROTOCOL_METADATA_ARTIFACT
        ),
        ErrorPayloadDetails::VersionMismatch {
            expected_protocol: PROTOCOL_IDENTITY,
            metadata_artifact: PROTOCOL_METADATA_ARTIFACT,
            expected_version: PROTOCOL_V5_VERSION,
            expected_sub_version: PROTOCOL_V5_SUB_VERSION,
            actual_version,
            actual_sub_version,
        },
    )
}

fn sub_version_mismatch_error(actual_sub_version: u16) -> ErrorPayload {
    ErrorPayload::with_details(
        "sub_version_mismatch",
        format!(
            "unsupported protocol sub_version={}; expected protocol={} sub_version={}. Upgrade client metadata artifact '{}'.",
            actual_sub_version,
            PROTOCOL_IDENTITY,
            PROTOCOL_V5_SUB_VERSION,
            PROTOCOL_METADATA_ARTIFACT
        ),
        ErrorPayloadDetails::SubVersionMismatch {
            expected_protocol: PROTOCOL_IDENTITY,
            metadata_artifact: PROTOCOL_METADATA_ARTIFACT,
            expected_sub_version: PROTOCOL_V5_SUB_VERSION,
            actual_sub_version,
        },
    )
}

fn identity_mismatch_error(
    expected_session_id: u64,
    actual_session_id: u64,
    expected_partition_id: u64,
    actual_partition_id: u64,
    expected_actor_id: u64,
    actual_actor_id: u64,
) -> ErrorPayload {
    ErrorPayload::with_details(
        "identity_mismatch",
        format!(
            "envelope identity mismatch: expected session/partition/actor={expected_session_id}/{expected_partition_id}/{expected_actor_id}, got {actual_session_id}/{actual_partition_id}/{actual_actor_id}"
        ),
        ErrorPayloadDetails::IdentityMismatch {
            expected_session_id,
            actual_session_id,
            expected_partition_id,
            actual_partition_id,
            expected_actor_id,
            actual_actor_id,
        },
    )
}

fn validate_envelope_identity(
    envelope: &ProtocolEnvelope,
    expected_session_id: u64,
    expected_partition_id: u64,
    expected_actor_id: u64,
) -> Result<(), ErrorPayload> {
    if envelope.session_id != expected_session_id
        || envelope.partition_id != expected_partition_id
        || envelope.actor_id != expected_actor_id
    {
        return Err(identity_mismatch_error(
            expected_session_id,
            envelope.session_id,
            expected_partition_id,
            envelope.partition_id,
            expected_actor_id,
            envelope.actor_id,
        ));
    }
    Ok(())
}

fn validate_input_batch_size(inputs_len: usize) -> Result<(), ErrorPayload> {
    if inputs_len > MAX_INPUT_BATCH_INPUTS {
        return Err(oversized_input_batch_error(inputs_len));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct InputBatchPayload {
    #[serde(default)]
    inputs: Vec<ClientInputPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClientInputPayload {
    seq: u64,
    tick: u64,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
    #[serde(default)]
    collect_pressed: bool,
}

pub fn run_vertical_slice_server(config: VerticalSliceServerConfig) -> Result<(), String> {
    crate::tokio_runtime().block_on(run_vertical_slice_server_async(config))
}

async fn run_vertical_slice_server_async(config: VerticalSliceServerConfig) -> Result<(), String> {
    if !config.static_root.is_dir() {
        return Err(format!(
            "realtime vertical slice static root does not exist: {}",
            config.static_root.display()
        ));
    }
    if let Some(artifact_root) = &config.artifact_root {
        std::fs::create_dir_all(artifact_root).map_err(|error| {
            format!(
                "failed creating artifact directory {}: {error}",
                artifact_root.display()
            )
        })?;
    }

    let domain_lib_path =
        config
            .static_root
            .join("domain")
            .with_extension(if cfg!(target_os = "macos") {
                "dylib"
            } else {
                "so"
            });
    let domain_abi = if domain_lib_path.exists() {
        match unsafe { crate::domain_abi::DomainAbi::load(&domain_lib_path) } {
            Ok(abi) => {
                unsafe { (abi.init)() };
                eprintln!(
                    "[game_slice] domain library loaded from {}",
                    domain_lib_path.display()
                );
                Some(abi)
            }
            Err(e) => {
                eprintln!(
                    "[game_slice] domain library load failed: {e}, falling back to built-in logic"
                );
                None
            }
        }
    } else {
        None
    };
    let domain_abi = Arc::new(domain_abi);

    let world_state_path = world_state_path_for_static_root(config.static_root.as_path());
    let authority = Arc::new(Mutex::new(WorldAuthority::load_or_default(
        world_state_path,
    )));
    let mmo_role = resolve_mmo_runtime_role(None);

    let listener = TcpListener::bind(config.bind_address.as_str())
        .await
        .map_err(|error| format!("failed to bind realtime vertical slice server: {error}"))?;
    eprintln!(
        "wrela realtime vertical slice server listening on http://{} role={}",
        config.bind_address,
        mmo_role.as_str(),
    );

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if let Err(error) = signal {
                    eprintln!("realtime vertical slice server signal watcher error: {error}");
                }
                eprintln!("wrela realtime vertical slice server shutting down");
                break;
            }
            accept_result = listener.accept() => {
                let (stream, _) = accept_result
                    .map_err(|error| format!("failed accepting realtime vertical slice connection: {error}"))?;
                let task_config = config.clone();
                let task_authority = authority.clone();
                let task_domain_abi = domain_abi.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, task_config, task_authority, mmo_role, task_domain_abi).await {
                        eprintln!("realtime vertical slice session connection error: {error}");
                    }
                });
            }
        }
    }

    Ok(())
}

async fn handle_connection(
    mut stream: TcpStream,
    config: VerticalSliceServerConfig,
    authority: Arc<Mutex<WorldAuthority>>,
    mmo_role: MmoRuntimeRole,
    domain_abi: Arc<Option<crate::domain_abi::DomainAbi>>,
) -> Result<(), String> {
    let handshake = read_http_handshake(&mut stream).await?;
    if handshake.is_websocket_upgrade && handshake.path == "/ws" {
        let websocket_key = handshake
            .websocket_key
            .ok_or_else(|| "missing Sec-WebSocket-Key during websocket upgrade".to_string())?;
        write_websocket_upgrade_response(&mut stream, websocket_key.as_str()).await?;
        return run_websocket_session(stream, config, authority, mmo_role, domain_abi).await;
    }
    serve_static_resource(
        &mut stream,
        config.static_root.as_path(),
        handshake.path.as_str(),
    )
    .await
}

async fn serve_static_resource(
    stream: &mut TcpStream,
    static_root: &Path,
    request_path: &str,
) -> Result<(), String> {
    let path_without_query = request_path.split('?').next().unwrap_or("/");
    let normalized_path = if path_without_query == "/" {
        "index.html".to_string()
    } else {
        path_without_query.trim_start_matches('/').to_string()
    };

    if normalized_path.contains("..") || normalized_path.contains('\\') {
        return write_bootstrap_response(stream, "text/plain; charset=utf-8", b"invalid path")
            .await;
    }

    let requested = static_root.join(normalized_path.as_str());
    let asset_path = if requested.is_file() {
        requested
    } else {
        static_root.join("index.html")
    };
    let body = tokio::fs::read(&asset_path).await.map_err(|error| {
        format!(
            "failed to read static asset {}: {error}",
            asset_path.display()
        )
    })?;

    let content_type = content_type_for_path(asset_path.as_path());
    write_bootstrap_response(stream, content_type, body.as_slice()).await
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

async fn run_websocket_session(
    mut stream: TcpStream,
    config: VerticalSliceServerConfig,
    authority: Arc<Mutex<WorldAuthority>>,
    mmo_role: MmoRuntimeRole,
    domain_abi: Arc<Option<crate::domain_abi::DomainAbi>>,
) -> Result<(), String> {
    let session_id = rand::random::<u64>().max(1);
    let partition_id = DEFAULT_PARTITION_ID;
    let actor_id = session_id;
    let descriptor = DomainAbiDescriptor::from_static_root(config.static_root.as_path());
    let (mut state, mut last_ack) = {
        let world = authority.lock().await;
        (world.state, world.last_ack)
    };
    let mut server_seq = 1u64;
    let mut last_client_seq = 0u64;
    let mut rollback_ring: VecDeque<(u64, u64)> = VecDeque::with_capacity(128);

    let hello_payload = HelloPayload {
        protocol: PROTOCOL_IDENTITY,
        session_id,
        role: mmo_role.as_str(),
        world_width: WORLD_WIDTH,
        world_height: WORLD_HEIGHT,
        collectibles: descriptor
            .collectibles
            .iter()
            .map(|(x, y)| (fixed_to_float(*x), fixed_to_float(*y)))
            .collect(),
        snapshot: DomainSnapshot::from_state(state, descriptor.source_seed),
    };
    let hello_payload_json = serde_json::to_vec(&hello_payload)
        .map_err(|error| format!("failed to serialize hello payload: {error}"))?;
    let hello_envelope = ProtocolEnvelope::new(
        PROTOCOL_V5_VERSION,
        PROTOCOL_V5_SUB_VERSION,
        session_id,
        partition_id,
        actor_id,
        MESSAGE_TYPE_HELLO_V5,
        state.tick,
        server_seq,
        last_ack,
        hello_payload_json,
    );
    send_envelope(
        &mut stream,
        &hello_envelope,
        config.artifact_root.as_deref(),
        session_id,
        "tx",
    )
    .await?;
    server_seq = server_seq.saturating_add(1);

    let state_snapshot_payload = StateSnapshotPayload {
        role: mmo_role.as_str(),
        reason: "initial",
        ack: last_ack,
        snapshot: DomainSnapshot::from_state(state, descriptor.source_seed),
    };
    let state_snapshot = ProtocolEnvelope::new(
        PROTOCOL_V5_VERSION,
        PROTOCOL_V5_SUB_VERSION,
        session_id,
        partition_id,
        actor_id,
        MESSAGE_TYPE_SNAPSHOT_V5,
        state.tick,
        server_seq,
        last_ack,
        serde_json::to_vec(&state_snapshot_payload)
            .map_err(|error| format!("failed to serialize state snapshot payload: {error}"))?,
    );
    send_envelope(
        &mut stream,
        &state_snapshot,
        config.artifact_root.as_deref(),
        session_id,
        "tx",
    )
    .await?;
    server_seq = server_seq.saturating_add(1);

    loop {
        let frame_result = timeout(
            Duration::from_millis(config.heartbeat_ms.max(250)),
            read_websocket_frame(&mut stream),
        )
        .await;

        let maybe_frame = match frame_result {
            Ok(frame) => frame?,
            Err(_) => {
                let heartbeat_payload = serde_json::to_vec(&serde_json::json!({
                    "kind": "heartbeat",
                    "role": mmo_role.as_str(),
                }))
                .map_err(|error| format!("failed to serialize heartbeat payload: {error}"))?;
                let heartbeat = ProtocolEnvelope::new(
                    PROTOCOL_V5_VERSION,
                    PROTOCOL_V5_SUB_VERSION,
                    session_id,
                    partition_id,
                    actor_id,
                    MESSAGE_TYPE_PING_V5,
                    state.tick,
                    server_seq,
                    last_ack,
                    heartbeat_payload,
                );
                send_envelope(
                    &mut stream,
                    &heartbeat,
                    config.artifact_root.as_deref(),
                    session_id,
                    "tx",
                )
                .await?;
                server_seq = server_seq.saturating_add(1);
                continue;
            }
        };

        let Some(frame) = maybe_frame else {
            return Ok(());
        };

        match frame {
            WebSocketFrame::Ping(payload) => {
                let pong = OutboundFrame::Pong(payload);
                write_outbound_frame(&mut stream, &pong).await?;
            }
            WebSocketFrame::Pong(_) => {}
            WebSocketFrame::Close(payload) => {
                let close = OutboundFrame::Close(payload);
                let _ = write_outbound_frame(&mut stream, &close).await;
                return Ok(());
            }
            WebSocketFrame::Text(payload) => {
                let message = String::from_utf8_lossy(payload.as_slice()).to_string();
                let envelope = ProtocolEnvelope::new(
                    PROTOCOL_V5_VERSION,
                    PROTOCOL_V5_SUB_VERSION,
                    session_id,
                    partition_id,
                    actor_id,
                    MESSAGE_TYPE_ERROR_V5,
                    state.tick,
                    server_seq,
                    last_ack,
                    serde_json::to_vec(&ErrorPayload {
                        ..ErrorPayload::new(
                            "text_frame_unsupported",
                            format!("text frame is not supported: {message}"),
                        )
                    })
                    .map_err(|error| format!("failed to serialize error payload: {error}"))?,
                );
                send_envelope(
                    &mut stream,
                    &envelope,
                    config.artifact_root.as_deref(),
                    session_id,
                    "tx",
                )
                .await?;
                server_seq = server_seq.saturating_add(1);
            }
            WebSocketFrame::Binary(payload) => {
                let envelope = match ProtocolEnvelope::decode(payload.as_slice()) {
                    Ok(envelope) => envelope,
                    Err(error) => {
                        let err_envelope = ProtocolEnvelope::new(
                            PROTOCOL_V5_VERSION,
                            PROTOCOL_V5_SUB_VERSION,
                            session_id,
                            partition_id,
                            actor_id,
                            MESSAGE_TYPE_ERROR_V5,
                            state.tick,
                            server_seq,
                            last_ack,
                            serde_json::to_vec(&ErrorPayload::new("bad_envelope", error)).map_err(
                                |serde_error| {
                                    format!("failed to serialize bad envelope error: {serde_error}")
                                },
                            )?,
                        );
                        send_envelope(
                            &mut stream,
                            &err_envelope,
                            config.artifact_root.as_deref(),
                            session_id,
                            "tx",
                        )
                        .await?;
                        server_seq = server_seq.saturating_add(1);
                        continue;
                    }
                };
                log_envelope(
                    config.artifact_root.as_deref(),
                    session_id,
                    "rx",
                    &envelope,
                    "received",
                );
                if envelope.version != PROTOCOL_V5_VERSION {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&version_mismatch_error(
                            envelope.version,
                            envelope.sub_version,
                        ))
                        .map_err(|error| {
                            format!("failed to serialize version mismatch: {error}")
                        })?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }
                if envelope.sub_version != PROTOCOL_V5_SUB_VERSION {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&sub_version_mismatch_error(envelope.sub_version))
                            .map_err(|error| {
                                format!("failed to serialize sub_version mismatch: {error}")
                            })?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }
                if let Err(error_payload) =
                    validate_envelope_identity(&envelope, session_id, partition_id, actor_id)
                {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&error_payload).map_err(|error| {
                            format!("failed to serialize identity mismatch error: {error}")
                        })?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }

                if envelope.seq <= last_client_seq {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&ErrorPayload::new(
                            "seq_not_monotonic",
                            format!(
                                "incoming seq={} must be greater than last_seq={last_client_seq}",
                                envelope.seq
                            ),
                        ))
                        .map_err(|error| format!("failed to serialize seq error: {error}"))?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }
                last_client_seq = envelope.seq;

                if envelope.message_type == MESSAGE_TYPE_PING_V5 {
                    let pong = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_PING_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        envelope.payload,
                    );
                    send_envelope(
                        &mut stream,
                        &pong,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }

                if envelope.message_type != MESSAGE_TYPE_INPUT_BATCH_V5 {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&ErrorPayload::new(
                            "unsupported_message_type",
                            format!(
                                "unsupported message_type={} for realtime vertical slice",
                                envelope.message_type
                            ),
                        ))
                        .map_err(|error| {
                            format!("failed to serialize unsupported type error: {error}")
                        })?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }

                let batch =
                    serde_json::from_slice::<InputBatchPayload>(envelope.payload.as_slice())
                        .map_err(|error| {
                            format!("failed to decode input batch payload: {error}")
                        })?;
                if let Err(error_payload) = validate_input_batch_size(batch.inputs.len()) {
                    let err_envelope = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_ERROR_V5,
                        state.tick,
                        server_seq,
                        last_ack,
                        serde_json::to_vec(&error_payload).map_err(|error| {
                            format!("failed to serialize input batch size error: {error}")
                        })?,
                    );
                    send_envelope(
                        &mut stream,
                        &err_envelope,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                    continue;
                }
                for input in batch.inputs {
                    let mut world = authority.lock().await;
                    let previous_state = world.state;
                    let mut restarted = false;
                    if input.collect_pressed && is_winning_state(&world.state, &descriptor) {
                        restart_state(&mut world.state);
                        restarted = true;
                    } else if let Some(ref abi) = *domain_abi {
                        unsafe {
                            (abi.tick)(
                                input.axis_x as f64,
                                input.axis_y as f64,
                                input.dt_ms,
                                if input.collect_pressed { 1 } else { 0 },
                            );
                        }
                        world.state.tick = world.state.tick.saturating_add(1);
                    } else {
                        apply_input(
                            &mut world.state,
                            &descriptor,
                            input.axis_x,
                            input.axis_y,
                            input.dt_ms,
                        );
                    }
                    world.last_ack = world.last_ack.max(input.seq);
                    let mut forced_divergence = false;
                    if !restarted
                        && config.force_divergence_interval > 0
                        && world.last_ack > 0
                        && world.last_ack % config.force_divergence_interval == 0
                    {
                        force_divergence(&mut world.state, 10 * FIXED_SCALE);
                        forced_divergence = true;
                    }
                    let persisted_path = world.persisted_path.clone();
                    let persist_state = world.state;
                    let persist_last_ack = world.last_ack;
                    last_ack = world.last_ack;
                    state = world.state;
                    drop(world);
                    persist_world_state_after_unlock(
                        &authority,
                        persisted_path,
                        persist_state,
                        persist_last_ack,
                    )
                    .await?;

                    let previous_snapshot =
                        DomainSnapshot::from_state(previous_state, descriptor.source_seed);
                    let snapshot = DomainSnapshot::from_state(state, descriptor.source_seed);
                    rollback_ring.push_back((last_ack, snapshot.hash));
                    if rollback_ring.len() > 128 {
                        rollback_ring.pop_front();
                    }

                    let payload = CorrectionPayload {
                        role: mmo_role.as_str(),
                        ack: last_ack,
                        forced_divergence,
                        rollback_ring_len: rollback_ring.len() as u32,
                        snapshot: snapshot.clone(),
                    };
                    let state_delta_payload = StateDeltaPayload {
                        role: mmo_role.as_str(),
                        ack: last_ack,
                        forced_divergence,
                        delta_kind: if restarted {
                            "restart"
                        } else {
                            "authoritative"
                        },
                        delta: compact_snapshot_delta(Some(&previous_snapshot), &snapshot),
                    };
                    let state_delta_json =
                        serde_json::to_vec(&state_delta_payload).map_err(|error| {
                            format!("failed to serialize state delta payload: {error}")
                        })?;
                    let state_delta = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_DELTA_V5,
                        input.tick,
                        server_seq,
                        last_ack,
                        state_delta_json,
                    );
                    send_envelope(
                        &mut stream,
                        &state_delta,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);

                    let payload_json = serde_json::to_vec(&payload).map_err(|error| {
                        format!("failed to serialize correction payload: {error}")
                    })?;
                    let correction = ProtocolEnvelope::new(
                        PROTOCOL_V5_VERSION,
                        PROTOCOL_V5_SUB_VERSION,
                        session_id,
                        partition_id,
                        actor_id,
                        MESSAGE_TYPE_CORRECTION_V5,
                        input.tick,
                        server_seq,
                        last_ack,
                        payload_json,
                    );
                    send_envelope(
                        &mut stream,
                        &correction,
                        config.artifact_root.as_deref(),
                        session_id,
                        "tx",
                    )
                    .await?;
                    server_seq = server_seq.saturating_add(1);
                }
            }
        }
    }
}

async fn send_envelope(
    stream: &mut TcpStream,
    envelope: &ProtocolEnvelope,
    artifact_root: Option<&Path>,
    session_id: u64,
    direction: &str,
) -> Result<(), String> {
    let frame = OutboundFrame::Binary(envelope.encode());
    write_outbound_frame(stream, &frame).await?;
    log_envelope(artifact_root, session_id, direction, envelope, "sent");
    Ok(())
}

fn log_envelope(
    artifact_root: Option<&Path>,
    session_id: u64,
    direction: &str,
    envelope: &ProtocolEnvelope,
    note: &str,
) {
    let Some(root) = artifact_root else {
        return;
    };
    let log_path = root.join(format!("session-{session_id}.jsonl"));
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let event = serde_json::json!({
        "ts_unix_ms": unix_timestamp_ms(),
        "direction": direction,
        "note": note,
        "session_id": envelope.session_id,
        "version": envelope.version,
        "sub_version": envelope.sub_version,
        "partition_id": envelope.partition_id,
        "actor_id": envelope.actor_id,
        "message_type": envelope.message_type,
        "tick": envelope.tick,
        "seq": envelope.seq,
        "ack": envelope.ack,
        "payload_len": envelope.payload_len,
        "crc32": envelope.crc32,
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = std::io::Write::write_all(&mut file, event.to_string().as_bytes());
        let _ = std::io::Write::write_all(&mut file, b"\n");
    }
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        ClientInputPayload, CorrectionPayload, DEFAULT_PARTITION_ID, DomainAbiDescriptor,
        DomainSnapshot, DomainState, MAX_INPUT_BATCH_INPUTS, MESSAGE_TYPE_CORRECTION_V5,
        MESSAGE_TYPE_DELTA_V5, MESSAGE_TYPE_INPUT_BATCH_V5, MESSAGE_TYPE_SNAPSHOT_V5,
        MmoRuntimeRole, PROTOCOL_V5_SUB_VERSION, PROTOCOL_V5_VERSION, ProtocolEnvelope,
        StateDeltaPayload, StateSnapshotPayload, WORLD_HEIGHT_FIXED, WORLD_WIDTH_FIXED,
        WorldAuthority, apply_input, clamp_axis_input, clamp_input_dt_ms, collect_collisions,
        compact_snapshot_delta, is_winning_state, persist_world_state_after_unlock,
        resolve_mmo_runtime_role_from_sources, restart_state, sub_version_mismatch_error,
        validate_envelope_identity, validate_input_batch_size, version_mismatch_error,
    };
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn snapshot_fixture(
        tick: u64,
        player_x: f32,
        player_y: f32,
        score: u32,
        collected_mask: u32,
        hash: u64,
    ) -> DomainSnapshot {
        DomainSnapshot {
            tick,
            player_x,
            player_y,
            score,
            collected_mask,
            hash,
            anim_state_id: "traveller.locomotion".to_string(),
            anim_phase_q16: 4_096,
            anim_event_markers: vec!["hit_window_open".to_string()],
            anim_root_motion_q16: 12_288,
            anim_reconcile_seq: tick,
        }
    }

    #[test]
    fn sanitize_collectibles_truncates_to_mask_width() {
        let descriptor = DomainAbiDescriptor {
            domain_source_hash: "test".to_string(),
            source_seed: 1,
            collectibles: (0..64).map(|_| (0, 0)).collect(),
        };
        let sanitized = descriptor.sanitize_collectibles();
        assert_eq!(sanitized.collectibles.len(), u32::BITS as usize);
    }

    #[test]
    fn collect_collisions_caps_mask_to_u32_width() {
        let mut state = DomainState {
            tick: 0,
            player_x_fixed: WORLD_WIDTH_FIXED / 2,
            player_y_fixed: WORLD_HEIGHT_FIXED / 2,
            score: 0,
            collected_mask: 0,
        };
        let descriptor = DomainAbiDescriptor {
            domain_source_hash: "test".to_string(),
            source_seed: 1,
            collectibles: (0..64)
                .map(|_| (WORLD_WIDTH_FIXED / 2, WORLD_HEIGHT_FIXED / 2))
                .collect(),
        };

        collect_collisions(&mut state, &descriptor);

        assert_eq!(state.score, u32::BITS);
        assert_eq!(state.collected_mask, u32::MAX);
    }

    #[test]
    fn winning_state_and_restart_round_trip() {
        let descriptor = DomainAbiDescriptor {
            domain_source_hash: "test".to_string(),
            source_seed: 1,
            collectibles: vec![(10, 10), (20, 20)],
        };
        let mut state = DomainState {
            tick: 9,
            player_x_fixed: 33,
            player_y_fixed: 44,
            score: 2,
            collected_mask: 0b11,
        };
        assert!(is_winning_state(&state, &descriptor));

        restart_state(&mut state);
        assert_eq!(state, DomainState::default());
    }

    #[test]
    fn input_payload_collect_pressed_defaults_false_for_compatibility() {
        let payload: ClientInputPayload = serde_json::from_value(serde_json::json!({
            "seq": 4,
            "tick": 8,
            "axis_x": 0.0,
            "axis_y": 1.0,
            "dt_ms": 16
        }))
        .expect("parse payload");
        assert!(!payload.collect_pressed);
    }

    #[test]
    fn clamp_axis_input_enforces_hard_bounds() {
        assert_eq!(clamp_axis_input(2.5), 1.0);
        assert_eq!(clamp_axis_input(-3.25), -1.0);
        assert_eq!(clamp_axis_input(0.625), 0.625);
        assert_eq!(clamp_axis_input(f32::NAN), 0.0);
        assert_eq!(clamp_axis_input(f32::INFINITY), 0.0);
    }

    #[test]
    fn clamp_input_dt_ms_enforces_hard_bounds() {
        assert_eq!(clamp_input_dt_ms(0), 1);
        assert_eq!(clamp_input_dt_ms(1), 1);
        assert_eq!(clamp_input_dt_ms(42), 42);
        assert_eq!(clamp_input_dt_ms(5_000), 100);
    }

    #[test]
    fn resolve_mmo_runtime_role_accepts_configured_values() {
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(Some("gateway"), None, None),
            MmoRuntimeRole::Gateway
        );
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(Some("ZONE"), None, None),
            MmoRuntimeRole::Zone
        );
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(Some(" world "), None, None),
            MmoRuntimeRole::World
        );
    }

    #[test]
    fn resolve_mmo_runtime_role_uses_fallback_order_and_defaults_to_world() {
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(None, Some("zone"), Some("gateway")),
            MmoRuntimeRole::Zone
        );
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(None, Some("invalid"), Some("gateway")),
            MmoRuntimeRole::Gateway
        );
        assert_eq!(
            resolve_mmo_runtime_role_from_sources(Some("invalid"), Some("bad"), Some("???")),
            MmoRuntimeRole::World
        );
    }

    #[test]
    fn compact_snapshot_delta_omits_unchanged_fields() {
        let previous = snapshot_fixture(10, 12.5, 20.0, 3, 0b0110, 100);
        let mut current = previous.clone();
        current.tick = 11;
        current.player_y = 22.0;
        current.hash = 101;
        current.anim_phase_q16 = 4_352;
        current.anim_reconcile_seq = 11;

        let delta = compact_snapshot_delta(Some(&previous), &current);

        assert_eq!(delta.tick, 11);
        assert_eq!(delta.player_x, None);
        assert_eq!(delta.player_y, Some(22.0));
        assert_eq!(delta.score, None);
        assert_eq!(delta.collected_mask, None);
        assert_eq!(delta.hash, Some(101));
        assert_eq!(delta.anim_state_id, None);
        assert_eq!(delta.anim_phase_q16, Some(4_352));
        assert_eq!(delta.anim_event_markers, None);
        assert_eq!(delta.anim_root_motion_q16, None);
        assert_eq!(delta.anim_reconcile_seq, Some(11));
    }

    #[test]
    fn compact_snapshot_delta_includes_all_fields_without_baseline() {
        let mut current = snapshot_fixture(77, 101.5, 202.25, 5, 0b1010, 9_999);
        current.anim_state_id = "traveller.attack".to_string();
        current.anim_phase_q16 = 65_280;
        current.anim_event_markers = vec![
            "hit_window_open".to_string(),
            "hit_window_close".to_string(),
        ];
        current.anim_root_motion_q16 = 98_304;

        let delta = compact_snapshot_delta(None, &current);

        assert_eq!(delta.tick, 77);
        assert_eq!(delta.player_x, Some(101.5));
        assert_eq!(delta.player_y, Some(202.25));
        assert_eq!(delta.score, Some(5));
        assert_eq!(delta.collected_mask, Some(0b1010));
        assert_eq!(delta.hash, Some(9_999));
        assert_eq!(delta.anim_state_id, Some("traveller.attack".to_string()));
        assert_eq!(delta.anim_phase_q16, Some(65_280));
        assert_eq!(
            delta.anim_event_markers,
            Some(vec![
                "hit_window_open".to_string(),
                "hit_window_close".to_string()
            ])
        );
        assert_eq!(delta.anim_root_motion_q16, Some(98_304));
        assert_eq!(delta.anim_reconcile_seq, Some(77));
    }

    #[test]
    fn compact_snapshot_delta_with_no_changes_keeps_only_tick() {
        let snapshot = snapshot_fixture(12, 8.0, 9.0, 2, 0b11, 500);

        let delta = compact_snapshot_delta(Some(&snapshot), &snapshot);

        assert_eq!(delta.tick, 12);
        assert_eq!(delta.player_x, None);
        assert_eq!(delta.player_y, None);
        assert_eq!(delta.score, None);
        assert_eq!(delta.collected_mask, None);
        assert_eq!(delta.hash, None);
        assert_eq!(delta.anim_state_id, None);
        assert_eq!(delta.anim_phase_q16, None);
        assert_eq!(delta.anim_event_markers, None);
        assert_eq!(delta.anim_root_motion_q16, None);
        assert_eq!(delta.anim_reconcile_seq, None);
    }

    #[test]
    fn state_delta_payload_serialization_skips_unchanged_fields() {
        let previous = snapshot_fixture(5, 1.0, 2.0, 7, 0b1, 42);
        let mut current = previous.clone();
        current.tick = 6;
        current.player_y = 3.0;
        current.hash = 43;
        current.anim_state_id = "traveller.attack".to_string();
        current.anim_phase_q16 = 8_192;
        current.anim_event_markers = vec!["hit_window_close".to_string()];
        current.anim_root_motion_q16 = 16_384;
        current.anim_reconcile_seq = 6;
        let payload = StateDeltaPayload {
            role: MmoRuntimeRole::World.as_str(),
            ack: 88,
            forced_divergence: false,
            delta_kind: "authoritative",
            delta: compact_snapshot_delta(Some(&previous), &current),
        };

        let value = serde_json::to_value(&payload).expect("serialize state delta payload");
        assert_eq!(value["role"], serde_json::json!("world"));
        let delta_json = value["delta"].as_object().expect("delta object");
        assert_eq!(delta_json["tick"], serde_json::json!(6));
        assert!(delta_json.get("player_x").is_none());
        assert_eq!(delta_json["player_y"], serde_json::json!(3.0));
        assert!(delta_json.get("score").is_none());
        assert!(delta_json.get("collected_mask").is_none());
        assert_eq!(delta_json["hash"], serde_json::json!(43));
        assert_eq!(
            delta_json["anim_state_id"],
            serde_json::json!("traveller.attack")
        );
        assert_eq!(delta_json["anim_phase_q16"], serde_json::json!(8_192));
        assert_eq!(
            delta_json["anim_event_markers"],
            serde_json::json!(["hit_window_close"])
        );
        assert_eq!(
            delta_json["anim_root_motion_q16"],
            serde_json::json!(16_384)
        );
        assert_eq!(delta_json["anim_reconcile_seq"], serde_json::json!(6));
    }

    #[test]
    fn protocol_v5_animation_payload_roundtrip() {
        let mut previous = snapshot_fixture(120, 10.0, 20.0, 2, 0b1, 111);
        previous.anim_state_id = "traveller.idle".to_string();
        previous.anim_phase_q16 = 2_560;
        previous.anim_event_markers = vec!["hit_window_open".to_string()];
        previous.anim_root_motion_q16 = 9_216;
        previous.anim_reconcile_seq = 120;

        let mut current = previous.clone();
        current.tick = 121;
        current.player_x = 11.0;
        current.score = 3;
        current.hash = 112;
        current.anim_state_id = "traveller.attack".to_string();
        current.anim_phase_q16 = 2_816;
        current.anim_event_markers = vec![
            "hit_window_open".to_string(),
            "hit_window_close".to_string(),
        ];
        current.anim_root_motion_q16 = 9_728;
        current.anim_reconcile_seq = 121;

        let snapshot_payload = StateSnapshotPayload {
            role: MmoRuntimeRole::World.as_str(),
            reason: "authoritative",
            ack: 300,
            snapshot: current.clone(),
        };
        let snapshot_envelope = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            90,
            DEFAULT_PARTITION_ID,
            90,
            MESSAGE_TYPE_SNAPSHOT_V5,
            current.tick,
            10,
            300,
            serde_json::to_vec(&snapshot_payload).expect("serialize state snapshot payload"),
        );
        let snapshot_roundtrip = ProtocolEnvelope::decode(snapshot_envelope.encode().as_slice())
            .expect("decode state snapshot envelope");
        assert_eq!(snapshot_roundtrip.message_type, MESSAGE_TYPE_SNAPSHOT_V5);
        let snapshot_json: serde_json::Value =
            serde_json::from_slice(snapshot_roundtrip.payload.as_slice())
                .expect("decode state snapshot payload");
        assert_eq!(
            snapshot_json["snapshot"]["anim_state_id"],
            serde_json::json!("traveller.attack")
        );
        assert_eq!(
            snapshot_json["snapshot"]["anim_phase_q16"],
            serde_json::json!(2_816)
        );
        assert_eq!(
            snapshot_json["snapshot"]["anim_event_markers"],
            serde_json::json!(["hit_window_open", "hit_window_close"])
        );
        assert_eq!(
            snapshot_json["snapshot"]["anim_root_motion_q16"],
            serde_json::json!(9_728)
        );
        assert_eq!(
            snapshot_json["snapshot"]["anim_reconcile_seq"],
            serde_json::json!(121)
        );

        let state_delta_payload = StateDeltaPayload {
            role: MmoRuntimeRole::World.as_str(),
            ack: 300,
            forced_divergence: false,
            delta_kind: "authoritative",
            delta: compact_snapshot_delta(Some(&previous), &current),
        };
        let state_delta_envelope = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            90,
            DEFAULT_PARTITION_ID,
            90,
            MESSAGE_TYPE_DELTA_V5,
            current.tick,
            11,
            300,
            serde_json::to_vec(&state_delta_payload).expect("serialize state delta payload"),
        );
        let state_delta_roundtrip =
            ProtocolEnvelope::decode(state_delta_envelope.encode().as_slice())
                .expect("decode state delta envelope");
        assert_eq!(state_delta_roundtrip.message_type, MESSAGE_TYPE_DELTA_V5);
        let state_delta_json: serde_json::Value =
            serde_json::from_slice(state_delta_roundtrip.payload.as_slice())
                .expect("decode state delta payload");
        assert_eq!(
            state_delta_json["delta"]["anim_state_id"],
            serde_json::json!("traveller.attack")
        );
        assert_eq!(
            state_delta_json["delta"]["anim_phase_q16"],
            serde_json::json!(2_816)
        );
        assert_eq!(
            state_delta_json["delta"]["anim_event_markers"],
            serde_json::json!(["hit_window_open", "hit_window_close"])
        );
        assert_eq!(
            state_delta_json["delta"]["anim_root_motion_q16"],
            serde_json::json!(9_728)
        );
        assert_eq!(
            state_delta_json["delta"]["anim_reconcile_seq"],
            serde_json::json!(121)
        );

        let correction_payload = CorrectionPayload {
            role: MmoRuntimeRole::World.as_str(),
            ack: 300,
            forced_divergence: true,
            rollback_ring_len: 8,
            snapshot: current,
        };
        let correction_envelope = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            90,
            DEFAULT_PARTITION_ID,
            90,
            MESSAGE_TYPE_CORRECTION_V5,
            121,
            12,
            300,
            serde_json::to_vec(&correction_payload).expect("serialize correction payload"),
        );
        let correction_roundtrip =
            ProtocolEnvelope::decode(correction_envelope.encode().as_slice())
                .expect("decode correction envelope");
        assert_eq!(
            correction_roundtrip.message_type,
            MESSAGE_TYPE_CORRECTION_V5
        );
        let correction_json: serde_json::Value =
            serde_json::from_slice(correction_roundtrip.payload.as_slice())
                .expect("decode correction payload");
        assert_eq!(
            correction_json["snapshot"]["anim_state_id"],
            serde_json::json!("traveller.attack")
        );
        assert_eq!(
            correction_json["snapshot"]["anim_phase_q16"],
            serde_json::json!(2_816)
        );
        assert_eq!(
            correction_json["snapshot"]["anim_event_markers"],
            serde_json::json!(["hit_window_open", "hit_window_close"])
        );
        assert_eq!(
            correction_json["snapshot"]["anim_root_motion_q16"],
            serde_json::json!(9_728)
        );
        assert_eq!(
            correction_json["snapshot"]["anim_reconcile_seq"],
            serde_json::json!(121)
        );
    }

    #[test]
    fn apply_input_clamps_axis_and_dt_ms() {
        let descriptor = DomainAbiDescriptor {
            domain_source_hash: "test".to_string(),
            source_seed: 1,
            collectibles: vec![],
        };
        let mut hardened_state = DomainState::default();
        let mut expected_state = DomainState::default();

        apply_input(&mut hardened_state, &descriptor, 9.0, -9.0, 5_000);
        apply_input(&mut expected_state, &descriptor, 1.0, -1.0, 100);

        assert_eq!(hardened_state, expected_state);
    }

    #[test]
    fn validate_input_batch_size_rejects_oversized_batches_with_typed_details() {
        assert!(validate_input_batch_size(MAX_INPUT_BATCH_INPUTS).is_ok());
        let err = validate_input_batch_size(MAX_INPUT_BATCH_INPUTS + 1)
            .expect_err("oversized batch should be rejected");
        assert_eq!(err.code, "input_batch_too_large");
        let payload = serde_json::to_value(&err).expect("serialize error payload");
        assert_eq!(payload["details"]["type"], "input_batch_too_large");
        assert_eq!(
            payload["details"]["max_inputs"],
            serde_json::json!(MAX_INPUT_BATCH_INPUTS)
        );
        assert_eq!(
            payload["details"]["actual_inputs"],
            serde_json::json!(MAX_INPUT_BATCH_INPUTS + 1)
        );
    }

    #[test]
    fn sub_version_mismatch_error_is_typed() {
        let err = sub_version_mismatch_error(PROTOCOL_V5_SUB_VERSION + 1);
        assert_eq!(err.code, "sub_version_mismatch");
        let payload = serde_json::to_value(&err).expect("serialize error payload");
        assert_eq!(payload["details"]["type"], "sub_version_mismatch");
        assert_eq!(payload["details"]["expected_protocol"], "protocol-v5");
        assert_eq!(payload["details"]["metadata_artifact"], "protocol-v5.json");
        assert_eq!(
            payload["details"]["expected_sub_version"],
            serde_json::json!(PROTOCOL_V5_SUB_VERSION)
        );
        assert_eq!(
            payload["details"]["actual_sub_version"],
            serde_json::json!(PROTOCOL_V5_SUB_VERSION + 1)
        );
        assert!(err.message.contains("Upgrade client metadata artifact"));
    }

    #[test]
    fn version_mismatch_error_is_typed_and_actionable() {
        let err = version_mismatch_error(2, 0);
        assert_eq!(err.code, "version_mismatch");
        let payload = serde_json::to_value(&err).expect("serialize error payload");
        assert_eq!(payload["details"]["type"], "version_mismatch");
        assert_eq!(payload["details"]["expected_protocol"], "protocol-v5");
        assert_eq!(payload["details"]["metadata_artifact"], "protocol-v5.json");
        assert_eq!(
            payload["details"]["expected_version"],
            serde_json::json!(PROTOCOL_V5_VERSION)
        );
        assert_eq!(payload["details"]["actual_version"], serde_json::json!(2));
        assert_eq!(
            payload["details"]["actual_sub_version"],
            serde_json::json!(0)
        );
        assert!(err.message.contains("protocol-v5"));
        assert!(err.message.contains("protocol-v5.json"));
    }

    #[test]
    fn validate_envelope_identity_rejects_mismatch_with_typed_details() {
        let envelope = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            77,
            DEFAULT_PARTITION_ID + 1,
            88,
            MESSAGE_TYPE_INPUT_BATCH_V5,
            0,
            1,
            0,
            Vec::new(),
        );

        let err = validate_envelope_identity(&envelope, 100, DEFAULT_PARTITION_ID, 100)
            .expect_err("identity mismatch must be rejected");
        assert_eq!(err.code, "identity_mismatch");
        let payload = serde_json::to_value(&err).expect("serialize identity mismatch payload");
        assert_eq!(
            payload["details"]["type"],
            serde_json::json!("identity_mismatch")
        );
        assert_eq!(
            payload["details"]["expected_session_id"],
            serde_json::json!(100u64)
        );
        assert_eq!(
            payload["details"]["actual_session_id"],
            serde_json::json!(77u64)
        );
        assert_eq!(
            payload["details"]["expected_partition_id"],
            serde_json::json!(DEFAULT_PARTITION_ID)
        );
        assert_eq!(
            payload["details"]["actual_partition_id"],
            serde_json::json!(DEFAULT_PARTITION_ID + 1)
        );
        assert_eq!(
            payload["details"]["expected_actor_id"],
            serde_json::json!(100u64)
        );
        assert_eq!(
            payload["details"]["actual_actor_id"],
            serde_json::json!(88u64)
        );
    }

    #[test]
    fn validate_envelope_identity_accepts_matching_values() {
        let envelope = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            42,
            DEFAULT_PARTITION_ID,
            42,
            MESSAGE_TYPE_INPUT_BATCH_V5,
            0,
            1,
            0,
            Vec::new(),
        );

        validate_envelope_identity(&envelope, 42, DEFAULT_PARTITION_ID, 42)
            .expect("matching envelope identity should pass");
    }

    #[test]
    fn persist_world_state_after_unlock_converges_to_latest_authority_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let persisted_path = tmp.path().join("world-state.json");
        let authority = Arc::new(Mutex::new(WorldAuthority::new(persisted_path.clone())));

        crate::tokio_runtime().block_on(async {
            let latest_state = DomainState {
                tick: 9,
                player_x_fixed: 123,
                player_y_fixed: 456,
                score: 2,
                collected_mask: 0b11,
            };
            {
                let mut world = authority.lock().await;
                world.state = latest_state;
                world.last_ack = 99;
            }

            let stale_state = DomainState {
                tick: 1,
                player_x_fixed: 10,
                player_y_fixed: 20,
                score: 0,
                collected_mask: 0,
            };
            persist_world_state_after_unlock(&authority, persisted_path.clone(), stale_state, 1)
                .await
                .expect("persist world state");
        });

        let loaded = WorldAuthority::load_or_default(persisted_path);
        assert_eq!(loaded.state.tick, 9);
        assert_eq!(loaded.state.player_x_fixed, 123);
        assert_eq!(loaded.state.player_y_fixed, 456);
        assert_eq!(loaded.state.score, 2);
        assert_eq!(loaded.state.collected_mask, 0b11);
        assert_eq!(loaded.last_ack, 99);
    }

    #[test]
    fn world_authority_persistence_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let persisted_path = tmp.path().join("world-state.json");

        let mut authority = WorldAuthority::new(persisted_path.clone());
        authority.state.tick = 42;
        authority.state.score = 7;
        authority.last_ack = 88;
        authority.persist().expect("persist world");

        let loaded = WorldAuthority::load_or_default(persisted_path);
        assert_eq!(loaded.state.tick, 42);
        assert_eq!(loaded.state.score, 7);
        assert_eq!(loaded.last_ack, 88);
    }
}
