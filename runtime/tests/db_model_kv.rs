use std::collections::BTreeMap;

use tempfile::TempDir;
use wrela_runtime::db::api;
use wrela_runtime::db::types::{BatchOp, ErrorCode};

#[derive(Debug, Clone)]
struct ModelRow {
    value: Vec<u8>,
    version: u64,
}

#[derive(Debug, Clone)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn pick(&mut self, max_exclusive: usize) -> usize {
        (self.next_u64() % max_exclusive as u64) as usize
    }
}

fn test_db() -> (TempDir, i64) {
    let dir = tempfile::tempdir().expect("temp dir");
    let handle = api::open(dir.path()).expect("open db");
    (dir, handle)
}

fn as_bytes(input: &str) -> Vec<u8> {
    input.as_bytes().to_vec()
}

fn read_key_version(handle: i64, namespace: &[u8], key: &[u8]) -> u64 {
    let mut end = key.to_vec();
    end.push(0xFF);
    let rows =
        api::scan(handle, namespace.to_vec(), key.to_vec(), end, 1).expect("scan key version");
    assert_eq!(rows.len(), 1, "expected one row for key version lookup");
    rows[0].2
}

#[test]
fn model_state_machine_occ_ryw_and_atomic_batch() {
    let (_dir, handle) = test_db();
    let namespace = as_bytes("core");
    let keys = [
        as_bytes("k-a"),
        as_bytes("k-b"),
        as_bytes("k-c"),
        as_bytes("k-d"),
    ];
    let mut model: BTreeMap<Vec<u8>, ModelRow> = BTreeMap::new();
    let mut rng = Lcg::new(0xC0FFEE);

    for step in 0..300 {
        let key = keys[rng.pick(keys.len())].clone();
        match rng.pick(7) {
            0 | 1 => {
                let value = format!("v-{step}-{}", rng.next_u64()).into_bytes();
                let result = api::put(handle, namespace.clone(), key.clone(), value.clone(), None)
                    .expect("unconditional put");
                model.insert(
                    key.clone(),
                    ModelRow {
                        value,
                        version: result,
                    },
                );
            }
            2 => {
                let expected_version = model.get(&key).map(|row| row.version);
                let value = format!("occ-good-{step}").into_bytes();
                let result = api::put(
                    handle,
                    namespace.clone(),
                    key.clone(),
                    value.clone(),
                    expected_version,
                )
                .expect("conditional put with current version");
                model.insert(
                    key.clone(),
                    ModelRow {
                        value,
                        version: result,
                    },
                );
            }
            3 => {
                if let Some(row) = model.get(&key) {
                    let stale_version = row.version.saturating_sub(1);
                    let err = api::put(
                        handle,
                        namespace.clone(),
                        key.clone(),
                        format!("occ-bad-{step}").into_bytes(),
                        Some(stale_version),
                    )
                    .expect_err("stale version must fail");
                    assert_eq!(err.code, ErrorCode::OccMismatch);
                }
            }
            4 => {
                let got = api::get(handle, namespace.clone(), key.clone()).expect("read point");
                let expected = model.get(&key).map(|row| row.value.clone());
                assert_eq!(got, expected, "point read mismatch for key={key:?}");
            }
            5 => {
                let i1 = rng.pick(keys.len());
                let i2 = (i1 + 1 + rng.pick(keys.len() - 1)) % keys.len();
                let k1 = keys[i1].clone();
                let k2 = keys[i2].clone();
                let v1 = format!("batch-a-{step}").into_bytes();
                let v2 = format!("batch-b-{step}").into_bytes();
                let batch = vec![
                    BatchOp::Put {
                        namespace: namespace.clone(),
                        key: k1.clone(),
                        value: v1.clone(),
                        expected_version: model.get(&k1).map(|row| row.version),
                    },
                    BatchOp::Put {
                        namespace: namespace.clone(),
                        key: k2.clone(),
                        value: v2.clone(),
                        expected_version: model.get(&k2).map(|row| row.version),
                    },
                ];
                api::batch(handle, &batch).expect("atomic batch success");
                let v1_version = read_key_version(handle, &namespace, &k1);
                let v2_version = read_key_version(handle, &namespace, &k2);
                model.insert(
                    k1.clone(),
                    ModelRow {
                        value: v1.clone(),
                        version: v1_version,
                    },
                );
                model.insert(
                    k2.clone(),
                    ModelRow {
                        value: v2.clone(),
                        version: v2_version,
                    },
                );
                let got_1 = api::get(handle, namespace.clone(), k1).expect("read batch key 1");
                let got_2 = api::get(handle, namespace.clone(), k2).expect("read batch key 2");
                assert_eq!(got_1, Some(v1), "batch key 1 should be visible immediately");
                assert_eq!(got_2, Some(v2), "batch key 2 should be visible immediately");
            }
            _ => {
                let scan = api::scan(
                    handle,
                    namespace.clone(),
                    as_bytes("k-"),
                    as_bytes("k-z"),
                    64,
                )
                .expect("range scan");
                let expected: Vec<(Vec<u8>, Vec<u8>, u64)> = model
                    .iter()
                    .filter(|(key, _)| key.as_slice() >= b"k-" && key.as_slice() < b"k-z")
                    .map(|(key, row)| (key.clone(), row.value.clone(), row.version))
                    .collect();
                assert_eq!(scan, expected, "range scan diverged from reference model");
            }
        }

        // Explicit RYW check after every mutation step.
        if step % 3 == 0 {
            for (key, row) in &model {
                let got =
                    api::get(handle, namespace.clone(), key.clone()).expect("read your write");
                assert_eq!(got, Some(row.value.clone()), "ryw mismatch for key={key:?}");
            }
        }
    }

    assert!(api::close(handle), "close db");
}

#[test]
fn batch_occ_mismatch_is_all_or_nothing() {
    let (_dir, handle) = test_db();
    let namespace = as_bytes("core");
    let key_a = as_bytes("k-a");
    let key_b = as_bytes("k-b");

    let v1 = api::put(
        handle,
        namespace.clone(),
        key_a.clone(),
        as_bytes("base-a"),
        None,
    )
    .expect("seed key-a");
    let _v2 = api::put(
        handle,
        namespace.clone(),
        key_b.clone(),
        as_bytes("base-b"),
        None,
    )
    .expect("seed key-b");

    let failing_batch = vec![
        BatchOp::Put {
            namespace: namespace.clone(),
            key: key_a.clone(),
            value: as_bytes("next-a"),
            expected_version: Some(v1),
        },
        BatchOp::Put {
            namespace: namespace.clone(),
            key: key_b.clone(),
            value: as_bytes("bad-b"),
            expected_version: Some(1), // stale on purpose
        },
    ];

    let err =
        api::batch(handle, &failing_batch).expect_err("batch must fail on stale expected_version");
    assert_eq!(err.code, ErrorCode::OccMismatch);

    // If batch atomicity is broken, key_a would have been mutated.
    let got_a = api::get(handle, namespace.clone(), key_a.clone())
        .expect("read key-a")
        .expect("key-a present");
    let got_b = api::get(handle, namespace.clone(), key_b.clone())
        .expect("read key-b")
        .expect("key-b present");
    assert_eq!(got_a, as_bytes("base-a"));
    assert_eq!(got_b, as_bytes("base-b"));

    assert!(api::close(handle), "close db");
}
