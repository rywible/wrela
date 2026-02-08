use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub struct TaskSignal {
    epoch: Mutex<u64>,
    condvar: Condvar,
}

impl TaskSignal {
    pub fn new() -> Self {
        Self {
            epoch: Mutex::new(0),
            condvar: Condvar::new(),
        }
    }

    pub fn notify_one(&self) {
        let mut epoch = self.epoch.lock().expect("task signal epoch lock");
        *epoch += 1;
        self.condvar.notify_one();
    }

    pub fn notify_waiters(&self) {
        let mut epoch = self.epoch.lock().expect("task signal epoch lock");
        *epoch += 1;
        self.condvar.notify_all();
    }

    pub fn wait(&self, observed_epoch: u64) -> u64 {
        let mut epoch = self.epoch.lock().expect("task signal epoch lock");
        while *epoch <= observed_epoch {
            epoch = self.condvar.wait(epoch).expect("task signal condvar wait");
        }
        *epoch
    }

    pub fn wait_timeout(&self, observed_epoch: u64, timeout: Duration) -> (u64, bool) {
        let mut epoch = self.epoch.lock().expect("task signal epoch lock");
        if *epoch > observed_epoch {
            return (*epoch, true);
        }
        let (epoch_after_wait, result) = self
            .condvar
            .wait_timeout(epoch, timeout)
            .expect("task signal condvar timeout");
        epoch = epoch_after_wait;
        (*epoch, !result.timed_out())
    }

    pub fn snapshot(&self) -> u64 {
        *self.epoch.lock().expect("task signal epoch lock")
    }
}

impl Default for TaskSignal {
    fn default() -> Self {
        Self::new()
    }
}
