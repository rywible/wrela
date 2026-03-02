#![allow(dead_code)]

use crate::protocol::MessageTypeV5;
use serde::Deserialize;

fn default_role_string() -> String {
    "world".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPayload {
    pub tick: u64,
    pub player_x: f32,
    pub player_y: f32,
    pub score: u32,
    pub collected_mask: u32,
    #[serde(default)]
    pub hash: Option<u64>,
    #[serde(default)]
    pub anim_state_id: Option<String>,
    #[serde(default)]
    pub anim_phase_q16: Option<i32>,
    #[serde(default)]
    pub anim_event_markers: Vec<String>,
    #[serde(default)]
    pub anim_root_motion_q16: Option<i32>,
    #[serde(default)]
    pub anim_reconcile_seq: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StateDeltaPayload {
    #[serde(default, alias = "t")]
    pub tick: Option<u64>,
    #[serde(default, alias = "x")]
    pub player_x: Option<f32>,
    #[serde(default, alias = "y")]
    pub player_y: Option<f32>,
    #[serde(default, alias = "s")]
    pub score: Option<u32>,
    #[serde(default, alias = "m")]
    pub collected_mask: Option<u32>,
    #[serde(default)]
    pub hash: Option<u64>,
    #[serde(default)]
    pub anim_state_id: Option<String>,
    #[serde(default)]
    pub anim_phase_q16: Option<i32>,
    #[serde(default)]
    pub anim_event_markers: Option<Vec<String>>,
    #[serde(default)]
    pub anim_root_motion_q16: Option<i32>,
    #[serde(default)]
    pub anim_reconcile_seq: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloPayload {
    #[serde(default)]
    pub protocol: Option<serde_json::Value>,
    #[serde(default)]
    pub session_id: Option<u64>,
    #[serde(default = "default_role_string")]
    pub role: String,
    #[serde(default)]
    pub world_width: f32,
    #[serde(default)]
    pub world_height: f32,
    #[serde(default)]
    pub collectibles: Vec<(f32, f32)>,
    #[serde(default)]
    pub snapshot: Option<SnapshotPayload>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ServerStatePayload {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub ack: Option<u64>,
    #[serde(default)]
    pub forced_divergence: bool,
    #[serde(default)]
    pub rollback_ring_len: Option<u32>,
    #[serde(default)]
    pub delta_kind: Option<String>,
    #[serde(default)]
    pub snapshot: Option<SnapshotPayload>,
    #[serde(default)]
    pub delta: Option<StateDeltaPayload>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnimationAuthorityState {
    pub anim_state_id: Option<String>,
    pub anim_phase_q16: i32,
    pub anim_event_markers: Vec<String>,
    pub anim_root_motion_q16: i32,
    pub anim_reconcile_seq: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ErrorPayload {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug)]
pub enum ParsedAuthorityPayload {
    Hello(HelloPayload),
    State {
        payload: ServerStatePayload,
        count_correction: bool,
    },
    Error(ErrorPayload),
    Ignore,
}

pub fn parse_authority_payload(
    message_type: MessageTypeV5,
    payload: &[u8],
) -> Result<ParsedAuthorityPayload, String> {
    match message_type {
        MessageTypeV5::HelloV5 => serde_json::from_slice::<HelloPayload>(payload)
            .map(ParsedAuthorityPayload::Hello)
            .map_err(|error| format!("invalid hello payload: {error}")),
        MessageTypeV5::SnapshotV5 => parse_state_payload_with_requirements(payload, true, false)
            .map(|payload| ParsedAuthorityPayload::State {
                payload,
                count_correction: false,
            })
            .map_err(|error| format!("invalid snapshot payload: {error}")),
        MessageTypeV5::DeltaV5 => parse_state_payload_with_requirements(payload, false, true)
            .map(|payload| ParsedAuthorityPayload::State {
                payload,
                count_correction: false,
            })
            .map_err(|error| format!("invalid delta payload: {error}")),
        MessageTypeV5::CorrectionV5 => parse_correction_state_payload(payload)
            .map(|payload| ParsedAuthorityPayload::State {
                payload,
                count_correction: true,
            })
            .map_err(|error| format!("invalid correction payload: {error}")),
        MessageTypeV5::ErrorV5 => serde_json::from_slice::<ErrorPayload>(payload)
            .map(ParsedAuthorityPayload::Error)
            .map_err(|error| format!("invalid error payload: {error}")),
        _ => Ok(ParsedAuthorityPayload::Ignore),
    }
}

fn parse_state_payload_with_requirements(
    payload: &[u8],
    require_snapshot: bool,
    require_delta: bool,
) -> Result<ServerStatePayload, String> {
    let parsed: ServerStatePayload =
        serde_json::from_slice(payload).map_err(|error| error.to_string())?;
    let has_snapshot = parsed.snapshot.is_some();
    let has_delta = parsed.delta.is_some();
    if require_snapshot && !has_snapshot {
        return Err("missing required 'snapshot' object".to_string());
    }
    if require_delta && !has_delta {
        return Err("missing required 'delta' object".to_string());
    }
    if require_snapshot && has_delta {
        return Err("unexpected 'delta' object for snapshot payload".to_string());
    }
    if require_delta && has_snapshot {
        return Err("unexpected 'snapshot' object for delta/correction payload".to_string());
    }
    if !has_snapshot && !has_delta {
        return Err("state payload must include 'snapshot' or 'delta'".to_string());
    }
    Ok(parsed)
}

fn parse_correction_state_payload(payload: &[u8]) -> Result<ServerStatePayload, String> {
    let parsed: ServerStatePayload =
        serde_json::from_slice(payload).map_err(|error| error.to_string())?;
    let has_snapshot = parsed.snapshot.is_some();
    let has_delta = parsed.delta.is_some();
    if has_snapshot == has_delta {
        return Err(
            "correction payload must include exactly one of 'snapshot' or 'delta'".to_string(),
        );
    }
    Ok(parsed)
}

pub fn reconcile_animation_from_server(
    authority: &ServerStatePayload,
    state: &mut AnimationAuthorityState,
) -> Result<(), String> {
    let mut next_seq = state.anim_reconcile_seq;
    let mut next_state_id = state.anim_state_id.clone();
    let mut next_phase_q16 = state.anim_phase_q16;
    let mut next_event_markers = state.anim_event_markers.clone();
    let mut next_root_motion_q16 = state.anim_root_motion_q16;
    let mut applied = false;

    if let Some(snapshot) = authority.snapshot.as_ref() {
        if let Some(seq) = snapshot.anim_reconcile_seq {
            if seq < next_seq {
                return Err(format!(
                    "animation reconcile sequence regressed: incoming={seq} last={next_seq}"
                ));
            }
            next_seq = seq;
        }
        if let Some(anim_state_id) = snapshot.anim_state_id.as_ref() {
            next_state_id = Some(anim_state_id.clone());
        }
        if let Some(phase) = snapshot.anim_phase_q16 {
            next_phase_q16 = phase;
        }
        if !snapshot.anim_event_markers.is_empty() {
            next_event_markers = snapshot.anim_event_markers.clone();
        }
        if let Some(root_motion) = snapshot.anim_root_motion_q16 {
            next_root_motion_q16 = root_motion;
        }
        applied = true;
    }

    if let Some(delta) = authority.delta.as_ref() {
        if let Some(seq) = delta.anim_reconcile_seq {
            if seq < next_seq {
                return Err(format!(
                    "animation reconcile sequence regressed: incoming={seq} last={next_seq}"
                ));
            }
            next_seq = seq;
        }
        if let Some(anim_state_id) = delta.anim_state_id.as_ref() {
            next_state_id = Some(anim_state_id.clone());
        }
        if let Some(phase) = delta.anim_phase_q16 {
            next_phase_q16 = phase;
        }
        if let Some(markers) = delta.anim_event_markers.as_ref() {
            next_event_markers = markers.clone();
        }
        if let Some(root_motion) = delta.anim_root_motion_q16 {
            next_root_motion_q16 = root_motion;
        }
        applied = true;
    }

    if !applied {
        return Ok(());
    }

    state.anim_reconcile_seq = next_seq;
    state.anim_state_id = next_state_id;
    state.anim_phase_q16 = next_phase_q16;
    state.anim_event_markers = next_event_markers;
    state.anim_root_motion_q16 = next_root_motion_q16;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationAuthorityState, HelloPayload, MessageTypeV5, ServerStatePayload,
        parse_authority_payload, reconcile_animation_from_server,
    };

    #[test]
    fn parse_server_state_payload_accepts_compact_delta_aliases() {
        let payload = r#"{
            "ack": 42,
            "forced_divergence": true,
            "delta": {
                "t": 120,
                "x": 245.5,
                "y": 188.25,
                "s": 3,
                "m": 5,
                "hash": 99
            }
        }"#;

        let parsed: ServerStatePayload =
            serde_json::from_str(payload).expect("compact delta payload should parse");
        assert_eq!(parsed.ack, Some(42));
        assert!(parsed.forced_divergence);

        let delta = parsed.delta.expect("delta should be present");
        assert_eq!(delta.tick, Some(120));
        assert_eq!(delta.player_x, Some(245.5));
        assert_eq!(delta.player_y, Some(188.25));
        assert_eq!(delta.score, Some(3));
        assert_eq!(delta.collected_mask, Some(5));
        assert_eq!(delta.hash, Some(99));
    }

    #[test]
    fn parse_snapshot_payload_accepts_hash_and_runtime_metadata_fields() {
        let payload = br#"{
          "role":"world",
          "reason":"initial",
          "ack":1,
          "rollback_ring_len":0,
          "snapshot":{
            "tick":1,
            "player_x":100.0,
            "player_y":200.0,
            "score":0,
            "collected_mask":0,
            "hash":12345,
            "anim_state_id":"traveller.idle",
            "anim_phase_q16":4096,
            "anim_event_markers":["foot_l","foot_r"],
            "anim_root_motion_q16":512,
            "anim_reconcile_seq":9
          }
        }"#;
        let parsed = parse_authority_payload(MessageTypeV5::SnapshotV5, payload)
            .expect("snapshot payload should parse");
        let super::ParsedAuthorityPayload::State { payload, .. } = parsed else {
            panic!("expected state payload");
        };
        assert_eq!(payload.reason.as_deref(), Some("initial"));
        assert_eq!(payload.rollback_ring_len, Some(0));
        let snapshot = payload.snapshot.expect("snapshot should exist");
        assert_eq!(snapshot.hash, Some(12345));
        assert_eq!(snapshot.anim_state_id.as_deref(), Some("traveller.idle"));
        assert_eq!(snapshot.anim_phase_q16, Some(4096));
        assert_eq!(
            snapshot.anim_event_markers,
            vec!["foot_l".to_string(), "foot_r".to_string()]
        );
        assert_eq!(snapshot.anim_root_motion_q16, Some(512));
        assert_eq!(snapshot.anim_reconcile_seq, Some(9));
    }

    #[test]
    fn malformed_authority_state_payloads_fail_closed_by_type() {
        let malformed = br#"{"ack":"bad"}"#;
        let snapshot_error = parse_authority_payload(MessageTypeV5::SnapshotV5, malformed)
            .expect_err("snapshot payload should reject");
        assert!(snapshot_error.contains("invalid snapshot payload"));

        let delta_error = parse_authority_payload(MessageTypeV5::DeltaV5, malformed)
            .expect_err("delta payload should reject");
        assert!(delta_error.contains("invalid delta payload"));

        let correction_error = parse_authority_payload(MessageTypeV5::CorrectionV5, malformed)
            .expect_err("correction payload should reject");
        assert!(correction_error.contains("invalid correction payload"));
    }

    #[test]
    fn state_payloads_require_message_specific_state_objects() {
        let missing_state = br#"{"ack":1}"#;
        let snapshot_error = parse_authority_payload(MessageTypeV5::SnapshotV5, missing_state)
            .expect_err("snapshot payload should reject missing snapshot");
        assert!(snapshot_error.contains("missing required 'snapshot'"));

        let delta_error = parse_authority_payload(MessageTypeV5::DeltaV5, missing_state)
            .expect_err("delta payload should reject missing delta");
        assert!(delta_error.contains("missing required 'delta'"));
    }

    #[test]
    fn state_payloads_reject_mixed_snapshot_and_delta_by_message_type() {
        let mixed = br#"{
          "snapshot": {"tick": 1, "player_x": 0.0, "player_y": 0.0, "score": 0, "collected_mask": 0},
          "delta": {"t": 1, "x": 0.0, "y": 0.0, "s": 0, "m": 0}
        }"#;
        let snapshot_error = parse_authority_payload(MessageTypeV5::SnapshotV5, mixed)
            .expect_err("snapshot payload should reject unexpected delta");
        assert!(snapshot_error.contains("unexpected 'delta'"));
        let delta_error = parse_authority_payload(MessageTypeV5::DeltaV5, mixed)
            .expect_err("delta payload should reject unexpected snapshot");
        assert!(delta_error.contains("unexpected 'snapshot'"));

        let correction_error = parse_authority_payload(MessageTypeV5::CorrectionV5, mixed)
            .expect_err("correction payload should reject mixed snapshot/delta");
        assert!(correction_error.contains("exactly one of 'snapshot' or 'delta'"));
    }

    #[test]
    fn correction_payload_accepts_snapshot_shape_when_delta_missing() {
        let snapshot_correction = br#"{
          "ack": 7,
          "forced_divergence": true,
          "snapshot": {"tick": 7, "player_x": 3.0, "player_y": 4.0, "score": 5, "collected_mask": 6}
        }"#;

        let parsed = parse_authority_payload(MessageTypeV5::CorrectionV5, snapshot_correction)
            .expect("correction snapshot payload should parse");
        let super::ParsedAuthorityPayload::State {
            payload,
            count_correction,
        } = parsed
        else {
            panic!("expected correction state payload");
        };
        assert!(count_correction);
        assert!(payload.snapshot.is_some());
        assert!(payload.delta.is_none());
    }

    #[test]
    fn deny_unknown_fields_fail_closed_for_state_payloads() {
        let unknown_field_payload = br#"{
          "snapshot": {"tick": 1, "player_x": 0.0, "player_y": 0.0, "score": 0, "collected_mask": 0},
          "unexpected": 42
        }"#;
        let error = parse_authority_payload(MessageTypeV5::SnapshotV5, unknown_field_payload)
            .expect_err("unknown fields should reject");
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn hello_payload_accepts_protocol_field() {
        let payload = br#"{
          "protocol": {"version": 3, "sub_version": 1},
          "session_id": 42,
          "role": "authority",
          "world_width": 1024.0,
          "world_height": 768.0,
          "collectibles": []
        }"#;
        let parsed: HelloPayload =
            serde_json::from_slice(payload).expect("hello payload with protocol should parse");
        assert_eq!(parsed.role, "authority");
        assert!(parsed.protocol.is_some());
        assert_eq!(parsed.session_id, Some(42));
    }

    #[test]
    fn animation_reconcile_from_correction() {
        let payload = br#"{
          "ack": 7,
          "forced_divergence": false,
          "delta": {
            "t": 201,
            "anim_state_id": "traveller.attack_2",
            "anim_phase_q16": 32768,
            "anim_event_markers": ["hit_start", "hit_end"],
            "anim_root_motion_q16": 1024,
            "anim_reconcile_seq": 44
          }
        }"#;
        let parsed = parse_authority_payload(MessageTypeV5::CorrectionV5, payload)
            .expect("correction payload should parse");
        let super::ParsedAuthorityPayload::State { payload, .. } = parsed else {
            panic!("expected correction state payload");
        };

        let mut state = AnimationAuthorityState::default();
        reconcile_animation_from_server(&payload, &mut state)
            .expect("reconciliation should apply correction fields");

        assert_eq!(state.anim_state_id.as_deref(), Some("traveller.attack_2"));
        assert_eq!(state.anim_phase_q16, 32768);
        assert_eq!(
            state.anim_event_markers,
            vec!["hit_start".to_string(), "hit_end".to_string()]
        );
        assert_eq!(state.anim_root_motion_q16, 1024);
        assert_eq!(state.anim_reconcile_seq, 44);
    }

    #[test]
    fn malformed_json_is_fail_closed_for_all_wire_message_types() {
        let malformed = br#"{"ack":1"#;

        let hello_error = parse_authority_payload(MessageTypeV5::HelloV5, malformed)
            .expect_err("hello payload should reject malformed json");
        assert!(hello_error.contains("invalid hello payload"));

        let snapshot_error = parse_authority_payload(MessageTypeV5::SnapshotV5, malformed)
            .expect_err("snapshot payload should reject malformed json");
        assert!(snapshot_error.contains("invalid snapshot payload"));

        let delta_error = parse_authority_payload(MessageTypeV5::DeltaV5, malformed)
            .expect_err("delta payload should reject malformed json");
        assert!(delta_error.contains("invalid delta payload"));

        let correction_error = parse_authority_payload(MessageTypeV5::CorrectionV5, malformed)
            .expect_err("correction payload should reject malformed json");
        assert!(correction_error.contains("invalid correction payload"));

        let error_payload_error = parse_authority_payload(MessageTypeV5::ErrorV5, malformed)
            .expect_err("error payload should reject malformed json");
        assert!(error_payload_error.contains("invalid error payload"));
    }

    #[test]
    fn deny_unknown_fields_fail_closed_for_hello_payloads() {
        let payload = br#"{
          "protocol": {"version": 3},
          "session_id": 7,
          "role": "authority",
          "world_width": 1280.0,
          "world_height": 720.0,
          "collectibles": [],
          "unexpected_root_field": true
        }"#;
        let error = parse_authority_payload(MessageTypeV5::HelloV5, payload)
            .expect_err("hello payload with unknown root field should reject");
        assert!(error.contains("invalid hello payload"));
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn deny_unknown_fields_fail_closed_for_nested_snapshot_and_delta_objects() {
        let snapshot_payload = br#"{
          "snapshot": {
            "tick": 1,
            "player_x": 10.0,
            "player_y": 20.0,
            "score": 2,
            "collected_mask": 1,
            "unexpected_nested": 123
          }
        }"#;
        let snapshot_error = parse_authority_payload(MessageTypeV5::SnapshotV5, snapshot_payload)
            .expect_err("unknown nested snapshot field should reject");
        assert!(snapshot_error.contains("invalid snapshot payload"));
        assert!(snapshot_error.contains("unknown field"));

        let delta_payload = br#"{
          "delta": {
            "t": 5,
            "x": 1.0,
            "y": 2.0,
            "s": 3,
            "m": 4,
            "unexpected_nested": 7
          }
        }"#;
        let delta_error = parse_authority_payload(MessageTypeV5::DeltaV5, delta_payload)
            .expect_err("unknown nested delta field should reject");
        assert!(delta_error.contains("invalid delta payload"));
        assert!(delta_error.contains("unknown field"));
    }

    #[test]
    fn correction_payload_type_mismatch_is_fail_closed() {
        let wrong_types = br#"{
          "ack":"not-a-number",
          "delta":{
            "t":"bad-tick",
            "x":"bad-x"
          }
        }"#;
        let error = parse_authority_payload(MessageTypeV5::CorrectionV5, wrong_types)
            .expect_err("wrong primitive types should reject");
        assert!(error.contains("invalid correction payload"));
    }
}
