use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentTarget {
    Key(Vec<u8>),
    Range { start: Vec<u8>, end: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxnIntent {
    pub txn_id: u64,
    pub target: IntentTarget,
    pub acquired_ts: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntentConflict {
    pub holder_txn_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentAcquireResult {
    Acquired,
    AlreadyHeld,
}

#[derive(Debug, Default)]
pub struct IntentStore {
    intents: Vec<TxnIntent>,
    intent_count_by_txn: HashMap<u64, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldLock {
    pub txn_id: u64,
    pub key: Vec<u8>,
    pub range_end: Option<Vec<u8>>,
    pub acquired_ts: u64,
}

fn target_overlaps(a: &IntentTarget, b: &IntentTarget) -> bool {
    match (a, b) {
        (IntentTarget::Key(ka), IntentTarget::Key(kb)) => ka == kb,
        (IntentTarget::Key(key), IntentTarget::Range { start, end })
        | (IntentTarget::Range { start, end }, IntentTarget::Key(key)) => {
            key.as_slice() >= start.as_slice() && key.as_slice() < end.as_slice()
        }
        (
            IntentTarget::Range {
                start: start_a,
                end: end_a,
            },
            IntentTarget::Range {
                start: start_b,
                end: end_b,
            },
        ) => start_a.as_slice() < end_b.as_slice() && start_b.as_slice() < end_a.as_slice(),
    }
}

fn target_equal(a: &IntentTarget, b: &IntentTarget) -> bool {
    match (a, b) {
        (IntentTarget::Key(ka), IntentTarget::Key(kb)) => ka == kb,
        (
            IntentTarget::Range {
                start: start_a,
                end: end_a,
            },
            IntentTarget::Range {
                start: start_b,
                end: end_b,
            },
        ) => start_a == start_b && end_a == end_b,
        _ => false,
    }
}

impl IntentStore {
    fn acquire_target(
        &mut self,
        txn_id: u64,
        target: IntentTarget,
        acquired_ts: u64,
    ) -> Result<IntentAcquireResult, IntentConflict> {
        for existing in &self.intents {
            if existing.txn_id != txn_id {
                continue;
            }
            if target_equal(&existing.target, &target) || target_overlaps(&existing.target, &target)
            {
                return Ok(IntentAcquireResult::AlreadyHeld);
            }
        }

        let mut conflict_holder: Option<u64> = None;
        for existing in &self.intents {
            if existing.txn_id == txn_id {
                continue;
            }
            if target_overlaps(&existing.target, &target) {
                conflict_holder = Some(match conflict_holder {
                    Some(prev) => prev.min(existing.txn_id),
                    None => existing.txn_id,
                });
            }
        }

        if let Some(holder_txn_id) = conflict_holder {
            return Err(IntentConflict { holder_txn_id });
        }

        self.intents.push(TxnIntent {
            txn_id,
            target,
            acquired_ts,
        });
        *self.intent_count_by_txn.entry(txn_id).or_default() += 1;
        Ok(IntentAcquireResult::Acquired)
    }

    pub fn acquire_key(
        &mut self,
        txn_id: u64,
        key: Vec<u8>,
        acquired_ts: u64,
    ) -> Result<IntentAcquireResult, IntentConflict> {
        self.acquire_target(txn_id, IntentTarget::Key(key), acquired_ts)
    }

    pub fn acquire_range(
        &mut self,
        txn_id: u64,
        start: Vec<u8>,
        end: Vec<u8>,
        acquired_ts: u64,
    ) -> Result<IntentAcquireResult, IntentConflict> {
        self.acquire_target(txn_id, IntentTarget::Range { start, end }, acquired_ts)
    }

    pub fn holder(&self, key: &[u8]) -> Option<u64> {
        let target = IntentTarget::Key(key.to_vec());
        let mut holder: Option<u64> = None;
        for existing in &self.intents {
            if target_overlaps(&existing.target, &target) {
                holder = Some(match holder {
                    Some(prev) => prev.min(existing.txn_id),
                    None => existing.txn_id,
                });
            }
        }
        holder
    }

    pub fn release_txn(&mut self, txn_id: u64) -> usize {
        let before = self.intents.len();
        self.intents.retain(|intent| intent.txn_id != txn_id);
        let released = before.saturating_sub(self.intents.len());
        if released > 0 {
            self.intent_count_by_txn.remove(&txn_id);
        }
        released
    }

    pub fn held_locks(&self) -> Vec<HeldLock> {
        let mut out: Vec<HeldLock> = self
            .intents
            .iter()
            .map(|intent| match &intent.target {
                IntentTarget::Key(key) => HeldLock {
                    txn_id: intent.txn_id,
                    key: key.clone(),
                    range_end: None,
                    acquired_ts: intent.acquired_ts,
                },
                IntentTarget::Range { start, end } => HeldLock {
                    txn_id: intent.txn_id,
                    key: start.clone(),
                    range_end: Some(end.clone()),
                    acquired_ts: intent.acquired_ts,
                },
            })
            .collect();
        out.sort_by(|a, b| {
            a.txn_id
                .cmp(&b.txn_id)
                .then_with(|| a.key.cmp(&b.key))
                .then_with(|| a.range_end.cmp(&b.range_end))
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquires_reentrant_and_conflicts_cross_txn() {
        let mut store = IntentStore::default();
        assert_eq!(
            store.acquire_key(7, b"core:k1".to_vec(), 10),
            Ok(IntentAcquireResult::Acquired)
        );
        assert_eq!(
            store.acquire_key(7, b"core:k1".to_vec(), 11),
            Ok(IntentAcquireResult::AlreadyHeld)
        );

        let conflict = store
            .acquire_key(8, b"core:k1".to_vec(), 12)
            .expect_err("different txn should conflict");
        assert_eq!(conflict.holder_txn_id, 7);
    }

    #[test]
    fn key_range_and_range_range_overlap_conflict() {
        let mut store = IntentStore::default();
        assert_eq!(
            store.acquire_range(11, b"a".to_vec(), b"m".to_vec(), 1),
            Ok(IntentAcquireResult::Acquired)
        );

        let key_conflict = store
            .acquire_key(12, b"k".to_vec(), 2)
            .expect_err("key in range should conflict");
        assert_eq!(key_conflict.holder_txn_id, 11);

        let range_conflict = store
            .acquire_range(13, b"h".to_vec(), b"z".to_vec(), 3)
            .expect_err("overlapping range should conflict");
        assert_eq!(range_conflict.holder_txn_id, 11);

        assert_eq!(
            store.acquire_key(14, b"z".to_vec(), 4),
            Ok(IntentAcquireResult::Acquired)
        );
    }

    #[test]
    fn release_txn_reclaims_all_keys_and_ranges() {
        let mut store = IntentStore::default();
        store.acquire_key(11, b"a".to_vec(), 1).expect("a");
        store
            .acquire_range(11, b"b".to_vec(), b"d".to_vec(), 2)
            .expect("range");
        store.acquire_key(12, b"x".to_vec(), 3).expect("x");

        assert_eq!(store.release_txn(11), 2);
        assert_eq!(store.holder(b"a"), None);
        assert_eq!(store.holder(b"c"), None);
        assert_eq!(store.holder(b"x"), Some(12));
    }

    #[test]
    fn snapshot_is_sorted_and_stable() {
        let mut store = IntentStore::default();
        store.acquire_key(12, b"c".to_vec(), 3).expect("c");
        store.acquire_key(11, b"a".to_vec(), 1).expect("a");
        store
            .acquire_range(11, b"d".to_vec(), b"e".to_vec(), 2)
            .expect("range");

        let locks = store.held_locks();
        assert_eq!(locks.len(), 3);
        assert_eq!(locks[0].txn_id, 11);
        assert_eq!(locks[0].key, b"a".to_vec());
        assert_eq!(locks[0].range_end, None);

        assert_eq!(locks[1].txn_id, 11);
        assert_eq!(locks[1].key, b"d".to_vec());
        assert_eq!(locks[1].range_end, Some(b"e".to_vec()));

        assert_eq!(locks[2].txn_id, 12);
        assert_eq!(locks[2].key, b"c".to_vec());
        assert_eq!(locks[2].range_end, None);
    }
}
