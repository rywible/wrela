use wrela_runtime::db::txn::intents::{IntentAcquireResult, IntentConflict, IntentStore};

#[test]
fn test_intent_escalation_bug() {
    let mut store = IntentStore::default();

    // Txn 1 acquires Key("a")
    assert_eq!(
        store.acquire_key(1, b"a".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );

    // Txn 1 acquires Range("a".."c")
    let _ = store.acquire_range(1, b"a".to_vec(), b"c".to_vec(), 11);

    // Txn 2 attempts to acquire Key("b")
    // This SHOULD conflict with Txn 1's range intent!
    let res = store.acquire_key(2, b"b".to_vec(), 12);
    assert!(
        res.is_err(),
        "Txn 2 should have conflicted with Txn 1's range intent! Got: {:?}",
        res
    );
}

#[test]
fn range_range_overlap_conflict() {
    let mut store = IntentStore::default();

    // Txn 1 holds range [a, m)
    assert_eq!(
        store.acquire_range(1, b"a".to_vec(), b"m".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );

    // Txn 2 attempts overlapping range [f, z) — must conflict.
    let err = store
        .acquire_range(2, b"f".to_vec(), b"z".to_vec(), 11)
        .expect_err("overlapping ranges must conflict");
    assert_eq!(err.holder_txn_id, 1);

    // Txn 3 attempts adjacent non-overlapping range [m, z) — must succeed.
    assert_eq!(
        store.acquire_range(3, b"m".to_vec(), b"z".to_vec(), 12),
        Ok(IntentAcquireResult::Acquired)
    );
}

#[test]
fn range_range_no_overlap_succeeds() {
    let mut store = IntentStore::default();

    assert_eq!(
        store.acquire_range(1, b"a".to_vec(), b"d".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );
    // Disjoint range [e, g) — must succeed.
    assert_eq!(
        store.acquire_range(2, b"e".to_vec(), b"g".to_vec(), 11),
        Ok(IntentAcquireResult::Acquired)
    );
}

#[test]
fn same_txn_key_and_range_coexist() {
    let mut store = IntentStore::default();

    // Same txn acquires a key and an overlapping range — both should succeed
    // (same-txn does not conflict with itself).
    assert_eq!(
        store.acquire_key(1, b"b".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );
    assert_eq!(
        store.acquire_range(1, b"a".to_vec(), b"c".to_vec(), 11),
        Ok(IntentAcquireResult::Acquired)
    );

    // The txn should hold both intents.
    let locks = store.held_locks();
    let txn1_locks: Vec<_> = locks.iter().filter(|l| l.txn_id == 1).collect();
    assert_eq!(txn1_locks.len(), 2, "same txn holds both key and range");
}

#[test]
fn release_and_reacquire_by_different_txn() {
    let mut store = IntentStore::default();

    // Txn 1 acquires key "x".
    assert_eq!(
        store.acquire_key(1, b"x".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );

    // Txn 2 is blocked.
    assert_eq!(
        store.acquire_key(2, b"x".to_vec(), 11),
        Err(IntentConflict { holder_txn_id: 1 })
    );

    // Txn 1 releases.
    assert_eq!(store.release_txn(1), 1);

    // Txn 2 can now acquire.
    assert_eq!(
        store.acquire_key(2, b"x".to_vec(), 12),
        Ok(IntentAcquireResult::Acquired)
    );
}

#[test]
fn key_at_range_boundary_excluded() {
    let mut store = IntentStore::default();

    // Range [a, c) — 'c' itself is excluded.
    assert_eq!(
        store.acquire_range(1, b"a".to_vec(), b"c".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );
    // Key "c" must NOT conflict (exclusive end).
    assert_eq!(
        store.acquire_key(2, b"c".to_vec(), 11),
        Ok(IntentAcquireResult::Acquired)
    );
    // Key "a" MUST conflict (inclusive start).
    let err = store
        .acquire_key(3, b"a".to_vec(), 12)
        .expect_err("start is inclusive");
    assert_eq!(err.holder_txn_id, 1);
}

#[test]
fn multi_range_conflict_reports_lowest_txn_id() {
    let mut store = IntentStore::default();

    // Txn 5 and Txn 3 both hold overlapping ranges.
    assert_eq!(
        store.acquire_range(5, b"a".to_vec(), b"m".to_vec(), 10),
        Ok(IntentAcquireResult::Acquired)
    );
    assert_eq!(
        store.acquire_range(3, b"n".to_vec(), b"z".to_vec(), 11),
        Ok(IntentAcquireResult::Acquired)
    );

    // Txn 9 attempts a range that overlaps both — conflict should report
    // the lowest holder txn_id.
    let err = store
        .acquire_range(9, b"a".to_vec(), b"z".to_vec(), 12)
        .expect_err("must conflict");
    assert_eq!(
        err.holder_txn_id, 3,
        "conflict must report lowest holder txn_id"
    );
}
