use std::collections::BTreeMap;

use ciborium::value::Value;
use smol_str::SmolStr;
use wrela::persistence::{
    HeaderCompatibility, PersistenceProject, PersistentHandle, SaveIncompatibility,
    SnapshotLedgerRecord, compare_header, decompress_payload, save_snapshot_record,
};
use wrela::query_exec::stable_region_snapshot_handle;

fn project() -> PersistenceProject {
    PersistenceProject {
        project_id: "save_demo".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::from([("PlayerProgress".into(), 20)]),
    }
}

#[test]
fn ledger_payload_is_inspectable_cbor_value() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(
        &snapshot,
        &project(),
        1,
        1,
        vec![SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"]),
            type_id: "PlayerProgress".into(),
            payload: Value::Map(vec![
                (Value::Text("schema".into()), Value::Integer(2.into())),
                (Value::Text("xp".into()), Value::Integer(99.into())),
            ]),
        }],
    )
    .expect("save");
    let payload = decompress_payload(&record).expect("payload");

    assert!(matches!(
        &payload.ledger[0].payload,
        Value::Map(fields)
            if fields.iter().any(|(name, value)| {
                matches!(name, Value::Text(name) if name == "schema")
                    && matches!(value, Value::Integer(value) if i128::from(*value) == 2)
            })
    ));
}

#[test]
fn archetype_schema_change_emits_schema_changed() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 1, 1, Vec::new()).expect("save");
    let mut changed = project();
    changed
        .archetype_schema_hashes
        .insert("PlayerProgress".into(), 21);

    assert!(matches!(
        compare_header(&record.header, &changed),
        HeaderCompatibility::Incompatible {
            reason: SaveIncompatibility::ArchetypeSchemaChanged {
                name,
                saved_hash: 20,
                running_hash: 21,
            }
        } if name == "PlayerProgress"
    ));
}
