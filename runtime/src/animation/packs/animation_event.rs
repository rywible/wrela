use crate::animation::world_events::AnimationWorldEvent;
use crate::web::axum_bridge::{
    MESSAGE_TYPE_DELTA_V5, PROTOCOL_V5_SUB_VERSION, PROTOCOL_V5_VERSION, ProtocolEnvelope,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationEventPacket {
    pub packet_kind: String,
    pub stream_id: String,
    pub events: Vec<AnimationWorldEvent>,
}

impl AnimationEventPacket {
    pub fn new(stream_id: impl Into<String>, events: Vec<AnimationWorldEvent>) -> Self {
        Self {
            packet_kind: "animation_event_v1".to_string(),
            stream_id: stream_id.into(),
            events,
        }
    }

    pub fn to_payload_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| format!("animation packet encode failed: {error}"))
    }

    pub fn from_payload_bytes(payload: &[u8]) -> Result<Self, String> {
        let packet = serde_json::from_slice::<Self>(payload)
            .map_err(|error| format!("animation packet decode failed: {error}"))?;
        if packet.packet_kind != "animation_event_v1" {
            return Err(format!(
                "unsupported animation packet kind '{}' (expected animation_event_v1)",
                packet.packet_kind
            ));
        }
        Ok(packet)
    }

    pub(crate) fn into_protocol_envelope(
        &self,
        session_id: u64,
        partition_id: u64,
        actor_id: u64,
        tick: u64,
        seq: u64,
        ack: u64,
    ) -> Result<ProtocolEnvelope, String> {
        let payload = self.to_payload_bytes()?;
        Ok(ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            session_id,
            partition_id,
            actor_id,
            MESSAGE_TYPE_DELTA_V5,
            tick,
            seq,
            ack,
            payload,
        ))
    }

    pub(crate) fn from_protocol_envelope(envelope: &ProtocolEnvelope) -> Result<Self, String> {
        if envelope.version != PROTOCOL_V5_VERSION
            || envelope.sub_version != PROTOCOL_V5_SUB_VERSION
        {
            return Err(format!(
                "unsupported protocol frame version={} sub_version={}",
                envelope.version, envelope.sub_version
            ));
        }
        if envelope.message_type != MESSAGE_TYPE_DELTA_V5 {
            return Err(format!(
                "unexpected message_type={} for animation packet (expected {})",
                envelope.message_type, MESSAGE_TYPE_DELTA_V5
            ));
        }
        Self::from_payload_bytes(envelope.payload.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use crate::animation::world_events::fracture_event_sequence;

    use super::AnimationEventPacket;

    #[test]
    fn packet_roundtrip_matches_protocol_v4_wire_contract() {
        let packet =
            AnimationEventPacket::new("zone-1", fracture_event_sequence(500, 90, 11, 77, 2));
        let envelope = packet
            .into_protocol_envelope(44, 3, 11, 90, 902, 900)
            .expect("packet should encode to envelope");
        let wire = envelope.encode();
        let decoded = crate::web::axum_bridge::ProtocolEnvelope::decode(&wire)
            .expect("encoded envelope should decode");
        let parsed = AnimationEventPacket::from_protocol_envelope(&decoded)
            .expect("decoded envelope should parse animation packet");
        assert_eq!(parsed, packet);
    }
}
