use crate::physics::core::{ContactManifoldV1, PhysicsBodyV1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicsPersistenceRecordV1 {
    pub schema_version: u32,
    pub tick: u64,
    pub bodies: Vec<PhysicsBodyV1>,
    pub contacts: Vec<ContactManifoldV1>,
}

pub fn encode_record_json(record: &PhysicsPersistenceRecordV1) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(record).map_err(|error| format!("serialize physics record: {error}"))
}

pub fn decode_record_json(bytes: &[u8]) -> Result<PhysicsPersistenceRecordV1, String> {
    serde_json::from_slice(bytes).map_err(|error| format!("decode physics record: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{PhysicsPersistenceRecordV1, decode_record_json, encode_record_json};
    use crate::physics::core::{ColliderV1, PhysicsBodyStateV1, PhysicsBodyV1};

    #[test]
    fn serialization_roundtrip_preserves_record() {
        let record = PhysicsPersistenceRecordV1 {
            schema_version: 1,
            tick: 10,
            bodies: vec![PhysicsBodyV1 {
                body_id: 1,
                state: PhysicsBodyStateV1::Sleeping,
                position_milli: [1, 2, 3],
                velocity_milli_per_s: [4, 5, 6],
                mass_milli: 1000,
                collider: ColliderV1::Sphere { radius_milli: 500 },
            }],
            contacts: Vec::new(),
        };
        let bytes = encode_record_json(&record).expect("serialize");
        let decoded = decode_record_json(&bytes).expect("deserialize");
        assert_eq!(decoded, record);
    }
}
