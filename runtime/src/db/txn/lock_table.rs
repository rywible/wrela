use crate::db::txn::deadlock::WaitForGraph;
use crate::db::txn::intents::{HeldLock, IntentAcquireResult, IntentStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockAcquireOutcome {
    Acquired,
    AlreadyHeld,
    Waiting {
        holder_txn_id: u64,
        victim_txn_id: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockTableSnapshot {
    pub held_locks: Vec<HeldLock>,
    pub waits: Vec<(u64, u64)>,
}

#[derive(Debug, Default)]
pub struct TxnLockTable {
    intents: IntentStore,
    wait_for: WaitForGraph,
}

impl TxnLockTable {
    pub fn acquire(&mut self, txn_id: u64, key: Vec<u8>, acquired_ts: u64) -> LockAcquireOutcome {
        self.acquire_inner(txn_id, |intents| {
            intents.acquire_key(txn_id, key, acquired_ts)
        })
    }

    pub fn acquire_range(
        &mut self,
        txn_id: u64,
        start: Vec<u8>,
        end: Vec<u8>,
        acquired_ts: u64,
    ) -> LockAcquireOutcome {
        self.acquire_inner(txn_id, |intents| {
            intents.acquire_range(txn_id, start, end, acquired_ts)
        })
    }

    fn acquire_inner<F>(&mut self, txn_id: u64, acquire: F) -> LockAcquireOutcome
    where
        F: FnOnce(
            &mut IntentStore,
        ) -> Result<IntentAcquireResult, crate::db::txn::intents::IntentConflict>,
    {
        match acquire(&mut self.intents) {
            Ok(IntentAcquireResult::Acquired) => {
                self.wait_for.clear_waits_for(txn_id);
                LockAcquireOutcome::Acquired
            }
            Ok(IntentAcquireResult::AlreadyHeld) => {
                self.wait_for.clear_waits_for(txn_id);
                LockAcquireOutcome::AlreadyHeld
            }
            Err(conflict) => {
                self.wait_for.add_wait(txn_id, conflict.holder_txn_id);
                let victim_txn_id = self.wait_for.cycle_victim_from(txn_id);
                LockAcquireOutcome::Waiting {
                    holder_txn_id: conflict.holder_txn_id,
                    victim_txn_id,
                }
            }
        }
    }

    pub fn release_txn(&mut self, txn_id: u64) -> usize {
        self.wait_for.remove_txn(txn_id);
        self.intents.release_txn(txn_id)
    }

    pub fn snapshot(&self) -> LockTableSnapshot {
        LockTableSnapshot {
            held_locks: self.intents.held_locks(),
            waits: self.wait_for.waits(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquires_and_releases() {
        let mut table = TxnLockTable::default();
        assert_eq!(
            table.acquire(1, b"k1".to_vec(), 10),
            LockAcquireOutcome::Acquired
        );
        assert_eq!(table.release_txn(1), 1);
    }

    #[test]
    fn detects_cycle_and_selects_deterministic_victim() {
        let mut table = TxnLockTable::default();

        assert_eq!(
            table.acquire(10, b"a".to_vec(), 1),
            LockAcquireOutcome::Acquired
        );
        assert_eq!(
            table.acquire(20, b"b".to_vec(), 2),
            LockAcquireOutcome::Acquired
        );

        assert_eq!(
            table.acquire(10, b"b".to_vec(), 3),
            LockAcquireOutcome::Waiting {
                holder_txn_id: 20,
                victim_txn_id: None,
            }
        );

        assert_eq!(
            table.acquire(20, b"a".to_vec(), 4),
            LockAcquireOutcome::Waiting {
                holder_txn_id: 10,
                victim_txn_id: Some(20),
            }
        );
    }

    #[test]
    fn key_lock_waits_on_overlapping_range_lock() {
        let mut table = TxnLockTable::default();
        assert_eq!(
            table.acquire_range(10, b"a".to_vec(), b"m".to_vec(), 1),
            LockAcquireOutcome::Acquired
        );

        assert_eq!(
            table.acquire(20, b"k".to_vec(), 2),
            LockAcquireOutcome::Waiting {
                holder_txn_id: 10,
                victim_txn_id: None,
            }
        );
    }

    #[test]
    fn reports_stable_snapshot_of_locks_and_waits() {
        let mut table = TxnLockTable::default();
        assert_eq!(
            table.acquire(10, b"a".to_vec(), 1),
            LockAcquireOutcome::Acquired
        );
        assert_eq!(
            table.acquire(20, b"b".to_vec(), 2),
            LockAcquireOutcome::Acquired
        );
        assert_eq!(
            table.acquire(10, b"b".to_vec(), 3),
            LockAcquireOutcome::Waiting {
                holder_txn_id: 20,
                victim_txn_id: None,
            }
        );

        let snap = table.snapshot();
        assert_eq!(snap.held_locks.len(), 2);
        assert_eq!(snap.waits, vec![(10, 20)]);

        assert_eq!(
            table.acquire(10, b"a".to_vec(), 4),
            LockAcquireOutcome::AlreadyHeld
        );
    }
}
