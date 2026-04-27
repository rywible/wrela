use std::collections::BTreeMap;

use ciborium::value::Value;
use smol_str::SmolStr;
use wrela::engine_frame::{EngineResourceAccessMode, EngineResourceId, EngineSubsystemKind};
use wrela::persistence::{
    HeaderCompatibility, PersistenceError, PersistenceProject, PersistentHandle,
    SaveIncompatibility, SnapshotLedgerRecord, compare_header, load_snapshot_record,
    save_snapshot_record,
};
use wrela::query_exec::stable_region_snapshot_handle;

fn project() -> PersistenceProject {
    PersistenceProject {
        project_id: "save_demo".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::from([("region".into(), 10)]),
        archetype_schema_hashes: BTreeMap::from([("PlayerProgress".into(), 20)]),
    }
}

#[test]
fn persistence_round_trips_snapshot_record_and_load_plan() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(
        &snapshot,
        &project(),
        42,
        7,
        vec![SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"]),
            type_id: "PlayerProgress".into(),
            payload: Value::Map(vec![(
                Value::Text("level".into()),
                Value::Integer(7.into()),
            )]),
        }],
    )
    .expect("save");
    let (_loaded_snapshot, plan) = load_snapshot_record(record, &project()).expect("load");
    assert_eq!(plan.snapshot_epoch.0, 42);
    assert_eq!(
        plan.ledger[0].handle,
        PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"])
    );
    assert!(matches!(
        &plan.ledger[0].payload,
        Value::Map(fields)
            if fields.iter().any(|(name, value)| {
                matches!(name, Value::Text(name) if name == "level")
                    && matches!(value, Value::Integer(value) if i128::from(*value) == 7)
            })
    ));
    let load_ledger = plan.resource_ledger_for_load();
    assert!(load_ledger.accesses.iter().any(|access| {
        access.subsystem == EngineSubsystemKind::Save
            && access.mode == EngineResourceAccessMode::Read
            && matches!(access.resource, EngineResourceId::SaveRecord { .. })
    }));
    assert!(load_ledger.accesses.iter().any(|access| {
        access.subsystem == EngineSubsystemKind::Save
            && access.mode == EngineResourceAccessMode::Write
            && matches!(access.resource, EngineResourceId::WorldSnapshot { .. })
    }));
}

#[test]
fn persistence_version_bump_reports_generator_incompatibility() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 1, 1, Vec::new()).expect("save");
    let mut changed = project();
    changed
        .generator_compatibility_hashes
        .insert("region".into(), 11);
    let err = load_snapshot_record(record, &changed).expect_err("incompatible");
    assert!(matches!(
        err,
        PersistenceError::Incompatible(SaveIncompatibility::GeneratorDiverged {
            name,
            saved_hash: 10,
            running_hash: 11,
        }) if name == "region"
    ));
}

#[test]
fn persistence_payload_schema_detects_archetype_change() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 1, 1, Vec::new()).expect("save");
    let mut changed = project();
    changed
        .archetype_schema_hashes
        .insert("PlayerProgress".into(), 21);
    assert!(matches!(
        compare_header(&record.header, &changed),
        HeaderCompatibility::Incompatible {
            reason: SaveIncompatibility::ArchetypeSchemaChanged { .. }
        }
    ));
}

#[test]
fn persistent_handles_are_stable_across_sessions() {
    assert_eq!(
        PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"]),
        PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"])
    );
}
