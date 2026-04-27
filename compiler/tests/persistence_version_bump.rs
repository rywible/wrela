use std::collections::BTreeMap;

use smol_str::SmolStr;
use wrela::persistence::{
    PersistenceError, PersistenceProject, SaveIncompatibility, load_snapshot_record,
    save_snapshot_record,
};
use wrela::query_exec::stable_region_snapshot_handle;

fn project() -> PersistenceProject {
    PersistenceProject {
        project_id: "save_demo".into(),
        wrela_version: "test".into(),
        engine_compatibility_hash: 1,
        generator_compatibility_hashes: BTreeMap::from([
            ("region".into(), 10),
            ("loot".into(), 20),
        ]),
        archetype_schema_hashes: BTreeMap::new(),
    }
}

#[test]
fn version_bump_reports_exact_changed_generator() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 1, 1, Vec::new()).expect("save");
    let mut changed = project();
    changed
        .generator_compatibility_hashes
        .insert("loot".into(), 21);
    let err = load_snapshot_record(record, &changed).expect_err("incompatible");

    assert!(matches!(
        err,
        PersistenceError::Incompatible(SaveIncompatibility::GeneratorDiverged {
            name,
            saved_hash: 20,
            running_hash: 21,
        }) if name == "loot"
    ));
}

#[test]
fn engine_compatibility_mismatch_diagnostic_names_hashes() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 1, 1, Vec::new()).expect("save");
    let mut changed = project();
    changed.engine_compatibility_hash = 2;
    let err = load_snapshot_record(record, &changed).expect_err("incompatible");

    assert!(matches!(
        err,
        PersistenceError::Incompatible(SaveIncompatibility::EngineCompatibilityHashMismatch {
            saved_hash: 1,
            running_hash: 2,
        })
    ));
}
