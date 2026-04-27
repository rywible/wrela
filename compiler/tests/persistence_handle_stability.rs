use std::collections::BTreeMap;

use ciborium::value::Value;
use smol_str::SmolStr;
use wrela::persistence::{
    PersistenceProject, PersistentHandle, SnapshotLedgerRecord, load_snapshot_record,
    save_snapshot_record,
};
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::world_identity::SnapshotEpoch;

fn project() -> PersistenceProject {
    PersistenceProject {
        project_id: "save_demo".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::new(),
        archetype_schema_hashes: BTreeMap::new(),
    }
}

#[test]
fn persistent_handle_uses_stable_semantic_id_across_compatible_snapshots() {
    let handle = PersistentHandle::from_stable_semantic_parts(&[b"PlayerProgress", b"hero"]);
    let first_snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let later_snapshot = first_snapshot.with_epoch(SnapshotEpoch(first_snapshot.epoch().0 + 1));
    let first = save_snapshot_record(
        &first_snapshot,
        &project(),
        1,
        1,
        vec![SnapshotLedgerRecord {
            handle,
            type_id: "PlayerProgress".into(),
            payload: Value::Map(Vec::new()),
        }],
    )
    .expect("first save");
    let later = save_snapshot_record(
        &later_snapshot,
        &project(),
        2,
        2,
        vec![SnapshotLedgerRecord {
            handle,
            type_id: "PlayerProgress".into(),
            payload: Value::Map(Vec::new()),
        }],
    )
    .expect("later save");

    let (_, first_plan) = load_snapshot_record(first, &project()).expect("first load");
    let (_, later_plan) = load_snapshot_record(later, &project()).expect("later load");

    assert_eq!(first_plan.ledger[0].handle, later_plan.ledger[0].handle);
}
