use crc32fast::Hasher;
use std::fmt;

pub const PROTOCOL_V5: u16 = 5;
pub const PROTOCOL_V5_SUB_VERSION: u16 = 0;
pub const HEADER_LEN: usize = 62;

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageTypeV5 {
    HelloV5 = 1,
    AuthOkV5 = 2,
    InputBatchV5 = 3,
    SnapshotV5 = 4,
    DeltaV5 = 5,
    CorrectionV5 = 6,
    ResumeV5 = 7,
    PingV5 = 8,
    ErrorV5 = 9,
}

impl MessageTypeV5 {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

impl TryFrom<u16> for MessageTypeV5 {
    type Error = ProtocolError;

    fn try_from(value: u16) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::HelloV5),
            2 => Ok(Self::AuthOkV5),
            3 => Ok(Self::InputBatchV5),
            4 => Ok(Self::SnapshotV5),
            5 => Ok(Self::DeltaV5),
            6 => Ok(Self::CorrectionV5),
            7 => Ok(Self::ResumeV5),
            8 => Ok(Self::PingV5),
            9 => Ok(Self::ErrorV5),
            _ => Err(ProtocolError::UnknownMessageTypeV5(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub version: u16,
    pub sub_version: u16,
    pub partition_id: u64,
    pub session_id: u64,
    pub actor_id: u64,
    pub message_type: MessageTypeV5,
    pub tick: u64,
    pub seq: u64,
    pub ack: u64,
    pub payload: Vec<u8>,
}

impl Envelope {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let payload_len = u32::try_from(self.payload.len())
            .map_err(|_| ProtocolError::PayloadTooLarge(self.payload.len()))?;
        let crc32 = crc32(&self.payload);

        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.sub_version.to_be_bytes());
        out.extend_from_slice(&self.session_id.to_be_bytes());
        out.extend_from_slice(&self.partition_id.to_be_bytes());
        out.extend_from_slice(&self.actor_id.to_be_bytes());
        out.extend_from_slice(&self.message_type.as_u16().to_be_bytes());
        out.extend_from_slice(&self.tick.to_be_bytes());
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.ack.to_be_bytes());
        out.extend_from_slice(&payload_len.to_be_bytes());
        out.extend_from_slice(&crc32.to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < HEADER_LEN {
            return Err(ProtocolError::FrameTooShort {
                actual: bytes.len(),
                minimum: HEADER_LEN,
            });
        }

        let version = u16::from_be_bytes(bytes[0..2].try_into().expect("u16 header slice"));
        let sub_version = u16::from_be_bytes(bytes[2..4].try_into().expect("u16 header slice"));
        let session_id = u64::from_be_bytes(bytes[4..12].try_into().expect("u64 header slice"));
        let partition_id = u64::from_be_bytes(bytes[12..20].try_into().expect("u64 header slice"));
        let actor_id = u64::from_be_bytes(bytes[20..28].try_into().expect("u64 header slice"));
        let message_type_raw =
            u16::from_be_bytes(bytes[28..30].try_into().expect("u16 header slice"));
        let tick = u64::from_be_bytes(bytes[30..38].try_into().expect("u64 header slice"));
        let seq = u64::from_be_bytes(bytes[38..46].try_into().expect("u64 header slice"));
        let ack = u64::from_be_bytes(bytes[46..54].try_into().expect("u64 header slice"));
        let payload_len =
            u32::from_be_bytes(bytes[54..58].try_into().expect("u32 header slice")) as usize;
        let payload_crc = u32::from_be_bytes(bytes[58..62].try_into().expect("u32 header slice"));

        if bytes.len() != HEADER_LEN + payload_len {
            return Err(ProtocolError::PayloadLengthMismatch {
                expected_payload: payload_len,
                actual_payload: bytes.len().saturating_sub(HEADER_LEN),
            });
        }

        let payload = bytes[HEADER_LEN..].to_vec();
        let computed_crc = crc32(&payload);
        if computed_crc != payload_crc {
            return Err(ProtocolError::CrcMismatch {
                expected: payload_crc,
                actual: computed_crc,
            });
        }

        Ok(Self {
            version,
            sub_version,
            partition_id,
            session_id,
            actor_id,
            message_type: MessageTypeV5::try_from(message_type_raw)?,
            tick,
            seq,
            ack,
            payload,
        })
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    FrameTooShort {
        actual: usize,
        minimum: usize,
    },
    PayloadTooLarge(usize),
    PayloadLengthMismatch {
        expected_payload: usize,
        actual_payload: usize,
    },
    CrcMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownMessageTypeV5(u16),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooShort { actual, minimum } => {
                write!(
                    f,
                    "frame too short: got {actual} bytes, need at least {minimum}"
                )
            }
            Self::PayloadTooLarge(len) => {
                write!(f, "payload too large for u32 length field: {len}")
            }
            Self::PayloadLengthMismatch {
                expected_payload,
                actual_payload,
            } => write!(
                f,
                "payload length mismatch: expected {expected_payload}, got {actual_payload}"
            ),
            Self::CrcMismatch { expected, actual } => {
                write!(
                    f,
                    "payload crc mismatch: expected {expected:#010x}, got {actual:#010x}"
                )
            }
            Self::UnknownMessageTypeV5(raw) => write!(f, "unknown v5 message type: {raw}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::{
        Envelope, HEADER_LEN, MessageTypeV5, PROTOCOL_V5, PROTOCOL_V5_SUB_VERSION, ProtocolError,
    };

    #[test]
    fn envelope_round_trip_preserves_fields() {
        let original = Envelope {
            version: PROTOCOL_V5,
            sub_version: PROTOCOL_V5_SUB_VERSION,
            partition_id: 11,
            session_id: 42,
            actor_id: 99,
            message_type: MessageTypeV5::SnapshotV5,
            tick: 99,
            seq: 333,
            ack: 222,
            payload: b"state-bytes".to_vec(),
        };

        let encoded = original.encode().expect("encode");
        assert_eq!(encoded.len(), HEADER_LEN + original.payload.len());

        let decoded = Envelope::decode(&encoded).expect("decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_rejects_crc_mismatch() {
        let mut encoded = Envelope {
            version: PROTOCOL_V5,
            sub_version: PROTOCOL_V5_SUB_VERSION,
            partition_id: 5,
            session_id: 7,
            actor_id: 8,
            message_type: MessageTypeV5::HelloV5,
            tick: 1,
            seq: 1,
            ack: 0,
            payload: b"abc".to_vec(),
        }
        .encode()
        .expect("encode");

        let payload_start = HEADER_LEN;
        encoded[payload_start] ^= 0xFF;

        let err = Envelope::decode(&encoded).expect_err("crc mismatch expected");
        assert!(matches!(err, ProtocolError::CrcMismatch { .. }));
    }

    #[test]
    fn decode_rejects_length_mismatch() {
        let encoded = Envelope {
            version: PROTOCOL_V5,
            sub_version: PROTOCOL_V5_SUB_VERSION,
            partition_id: 3,
            session_id: 1,
            actor_id: 2,
            message_type: MessageTypeV5::DeltaV5,
            tick: 1,
            seq: 1,
            ack: 1,
            payload: vec![1, 2, 3, 4],
        }
        .encode()
        .expect("encode");

        let truncated = &encoded[..encoded.len() - 1];
        let err = Envelope::decode(truncated).expect_err("mismatch expected");
        assert!(matches!(err, ProtocolError::PayloadLengthMismatch { .. }));
    }

    #[test]
    fn envelope_header_uses_big_endian_network_order() {
        let encoded = Envelope {
            version: 0x1234,
            sub_version: 0x5678,
            session_id: 0x0102_0304_0506_0708,
            partition_id: 0x1112_1314_1516_1718,
            actor_id: 0x090A_0B0C_0D0E_0F10,
            message_type: MessageTypeV5::CorrectionV5,
            tick: 0x2122_2324_2526_2728,
            seq: 0x2122_2324_2526_2728,
            ack: 0x3132_3334_3536_3738,
            payload: vec![1, 2, 3],
        }
        .encode()
        .expect("encode");

        assert_eq!(&encoded[0..2], &[0x12, 0x34]);
        assert_eq!(&encoded[2..4], &[0x56, 0x78]);
        assert_eq!(&encoded[4..12], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&encoded[12..20], &[17, 18, 19, 20, 21, 22, 23, 24]);
        assert_eq!(&encoded[20..28], &[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(
            &encoded[28..30],
            &[0x00, MessageTypeV5::CorrectionV5.as_u16() as u8]
        );
        assert_eq!(&encoded[54..58], &[0, 0, 0, 3]);
    }

    #[test]
    fn decode_rejects_unknown_v3_message_type() {
        let mut encoded = Envelope {
            version: PROTOCOL_V5,
            sub_version: PROTOCOL_V5_SUB_VERSION,
            partition_id: 1,
            session_id: 11,
            actor_id: 22,
            message_type: MessageTypeV5::HelloV5,
            tick: 7,
            seq: 8,
            ack: 9,
            payload: vec![1, 2, 3],
        }
        .encode()
        .expect("encode");
        encoded[28..30].copy_from_slice(&0xFFFFu16.to_be_bytes());

        let err = Envelope::decode(&encoded).expect_err("unknown message type expected");
        assert!(matches!(err, ProtocolError::UnknownMessageTypeV5(0xFFFF)));
    }
}
