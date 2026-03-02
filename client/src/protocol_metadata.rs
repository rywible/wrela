use crate::protocol::MessageTypeV5;
use serde::Deserialize;
use std::collections::BTreeMap;

const PROTOCOL_METADATA_IDENTIFIER: &str = "protocol-v5";
const EXPECTED_PROTOCOL_ENVELOPE_FIELDS: [(&str, &str); 11] = [
    ("version", "u16"),
    ("sub_version", "u16"),
    ("session_id", "u64"),
    ("partition_id", "u64"),
    ("actor_id", "u64"),
    ("message_type", "u16"),
    ("tick", "u64"),
    ("seq", "u64"),
    ("ack", "u64"),
    ("payload_len", "u32"),
    ("crc32", "u32"),
];
const EXPECTED_PROTOCOL_MESSAGE_TYPES: [(&str, u16); 9] = [
    ("HELLO_V5", MessageTypeV5::HelloV5 as u16),
    ("AUTH_OK_V5", MessageTypeV5::AuthOkV5 as u16),
    ("INPUT_BATCH_V5", MessageTypeV5::InputBatchV5 as u16),
    ("SNAPSHOT_V5", MessageTypeV5::SnapshotV5 as u16),
    ("DELTA_V5", MessageTypeV5::DeltaV5 as u16),
    ("CORRECTION_V5", MessageTypeV5::CorrectionV5 as u16),
    ("RESUME_V5", MessageTypeV5::ResumeV5 as u16),
    ("PING_V5", MessageTypeV5::PingV5 as u16),
    ("ERROR_V5", MessageTypeV5::ErrorV5 as u16),
];

#[derive(Debug, Clone, Deserialize)]
struct ProtocolContractPayload {
    protocol: String,
    #[serde(default)]
    envelope: BTreeMap<String, String>,
    #[serde(default)]
    message_types: BTreeMap<String, u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolContract {
    pub envelope: BTreeMap<String, String>,
    pub message_types: BTreeMap<String, u16>,
}

pub fn validate_protocol_envelope_fields(
    envelope: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, expected_type) in EXPECTED_PROTOCOL_ENVELOPE_FIELDS {
        let Some(actual_type) = envelope.get(name) else {
            return Err(format!("protocol metadata missing envelope field '{name}'"));
        };
        if actual_type != expected_type {
            return Err(format!(
                "protocol metadata mismatch for envelope field '{name}': expected {expected_type}, got {actual_type}"
            ));
        }
    }
    Ok(())
}

pub fn validate_protocol_message_types(
    message_types: &BTreeMap<String, u16>,
) -> Result<(), String> {
    for (name, expected_value) in EXPECTED_PROTOCOL_MESSAGE_TYPES {
        let Some(actual_value) = message_types.get(name) else {
            return Err(format!("protocol metadata missing message type '{name}'"));
        };
        if *actual_value != expected_value {
            return Err(format!(
                "protocol metadata mismatch for '{name}': expected {expected_value}, got {actual_value}"
            ));
        }
    }
    Ok(())
}

pub fn parse_protocol_contract(payload_text: &str) -> Result<ProtocolContract, String> {
    let payload: ProtocolContractPayload = serde_json::from_str(payload_text)
        .map_err(|error| format!("invalid protocol metadata JSON: {error}"))?;

    if payload.protocol != PROTOCOL_METADATA_IDENTIFIER {
        return Err(format!(
            "unsupported protocol metadata identifier '{}': expected '{PROTOCOL_METADATA_IDENTIFIER}'",
            payload.protocol
        ));
    }
    validate_protocol_envelope_fields(&payload.envelope)?;
    validate_protocol_message_types(&payload.message_types)?;

    Ok(ProtocolContract {
        envelope: payload.envelope,
        message_types: payload.message_types,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_protocol_contract;

    #[test]
    fn parse_protocol_contract_accepts_v3_metadata() {
        let payload = r#"{
            "protocol":"protocol-v5",
            "envelope":{
                "version":"u16",
                "sub_version":"u16",
                "session_id":"u64",
                "partition_id":"u64",
                "actor_id":"u64",
                "message_type":"u16",
                "tick":"u64",
                "seq":"u64",
                "ack":"u64",
                "payload_len":"u32",
                "crc32":"u32"
            },
            "message_types":{
                "HELLO_V5":1,
                "AUTH_OK_V5":2,
                "INPUT_BATCH_V5":3,
                "SNAPSHOT_V5":4,
                "DELTA_V5":5,
                "CORRECTION_V5":6,
                "RESUME_V5":7,
                "PING_V5":8,
                "ERROR_V5":9
            }
        }"#;

        let parsed = parse_protocol_contract(payload).expect("protocol metadata should parse");
        assert_eq!(parsed.message_types.len(), 9);
        assert_eq!(parsed.message_types.get("HELLO_V5"), Some(&1));
        assert_eq!(parsed.message_types.get("ERROR_V5"), Some(&9));
        assert_eq!(parsed.envelope.get("version"), Some(&"u16".to_string()));
    }

    #[test]
    fn parse_protocol_contract_rejects_wrong_protocol_name() {
        let payload = r#"{
            "protocol":"protocol-v1",
            "envelope":{
                "version":"u16",
                "sub_version":"u16",
                "session_id":"u64",
                "partition_id":"u64",
                "actor_id":"u64",
                "message_type":"u16",
                "tick":"u64",
                "seq":"u64",
                "ack":"u64",
                "payload_len":"u32",
                "crc32":"u32"
            },
            "message_types":{
                "HELLO_V5":1,
                "AUTH_OK_V5":2,
                "INPUT_BATCH_V5":3,
                "SNAPSHOT_V5":4,
                "DELTA_V5":5,
                "CORRECTION_V5":6,
                "RESUME_V5":7,
                "PING_V5":8,
                "ERROR_V5":9
            }
        }"#;

        let error =
            parse_protocol_contract(payload).expect_err("wrong protocol identifier must fail");
        assert!(error.contains("expected 'protocol-v5'"));
    }

    #[test]
    fn parse_protocol_contract_rejects_missing_required_envelope_field() {
        let payload = r#"{
            "protocol":"protocol-v5",
            "envelope":{
                "version":"u16",
                "sub_version":"u16",
                "session_id":"u64",
                "partition_id":"u64",
                "actor_id":"u64",
                "message_type":"u16",
                "tick":"u64",
                "seq":"u64",
                "ack":"u64",
                "payload_len":"u32"
            },
            "message_types":{
                "HELLO_V5":1,
                "AUTH_OK_V5":2,
                "INPUT_BATCH_V5":3,
                "SNAPSHOT_V5":4,
                "DELTA_V5":5,
                "CORRECTION_V5":6,
                "RESUME_V5":7,
                "PING_V5":8,
                "ERROR_V5":9
            }
        }"#;

        let error = parse_protocol_contract(payload).expect_err("missing envelope field must fail");
        assert!(error.contains("missing envelope field 'crc32'"));
    }

    #[test]
    fn parse_protocol_contract_rejects_mismatched_message_type_value() {
        let payload = r#"{
            "protocol":"protocol-v5",
            "envelope":{
                "version":"u16",
                "sub_version":"u16",
                "session_id":"u64",
                "partition_id":"u64",
                "actor_id":"u64",
                "message_type":"u16",
                "tick":"u64",
                "seq":"u64",
                "ack":"u64",
                "payload_len":"u32",
                "crc32":"u32"
            },
            "message_types":{
                "HELLO_V5":99,
                "AUTH_OK_V5":2,
                "INPUT_BATCH_V5":3,
                "SNAPSHOT_V5":4,
                "DELTA_V5":5,
                "CORRECTION_V5":6,
                "RESUME_V5":7,
                "PING_V5":8,
                "ERROR_V5":9
            }
        }"#;

        let error = parse_protocol_contract(payload).expect_err("mismatched value must fail");
        assert!(error.contains("mismatch for 'HELLO_V5'"));
    }

    #[test]
    fn parse_protocol_contract_rejects_missing_required_message_type() {
        let payload = r#"{
            "protocol":"protocol-v5",
            "envelope":{
                "version":"u16",
                "sub_version":"u16",
                "session_id":"u64",
                "partition_id":"u64",
                "actor_id":"u64",
                "message_type":"u16",
                "tick":"u64",
                "seq":"u64",
                "ack":"u64",
                "payload_len":"u32",
                "crc32":"u32"
            },
            "message_types":{
                "HELLO_V5":1,
                "AUTH_OK_V5":2,
                "INPUT_BATCH_V5":3,
                "SNAPSHOT_V5":4,
                "DELTA_V5":5,
                "CORRECTION_V5":6,
                "RESUME_V5":7,
                "PING_V5":8
            }
        }"#;

        let error = parse_protocol_contract(payload).expect_err("missing message type must fail");
        assert!(error.contains("missing message type 'ERROR_V5'"));
    }
}
