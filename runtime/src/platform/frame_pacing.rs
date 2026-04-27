//! Single-frame-in-flight pacing primitive (RFC 0011 Phase 64).

use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug)]
pub struct FrameInFlightSemaphore {
    max_in_flight: u32,
    state: Mutex<u32>,
    cv: Condvar,
}

impl FrameInFlightSemaphore {
    pub fn new(max_in_flight: u32) -> Self {
        Self {
            max_in_flight: max_in_flight.max(1),
            state: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    pub fn acquire(&self) {
        let mut in_flight = self.state.lock().expect("frame pacing lock");
        while *in_flight >= self.max_in_flight {
            in_flight = self.cv.wait(in_flight).expect("frame pacing wait");
        }
        *in_flight = in_flight.saturating_add(1);
    }

    pub fn release(&self) {
        let mut in_flight = self.state.lock().expect("frame pacing lock");
        *in_flight = in_flight.saturating_sub(1);
        self.cv.notify_one();
    }

    pub fn release_after_submitted_work_done(self: &Arc<Self>, queue: &wgpu::Queue) {
        let semaphore = Arc::clone(self);
        queue.on_submitted_work_done(move || {
            semaphore.release();
        });
    }

    pub fn release_after_submitted_work_done_with<F>(self: &Arc<Self>, register: F)
    where
        F: FnOnce(Box<dyn FnOnce() + Send>),
    {
        let semaphore = Arc::clone(self);
        register(Box::new(move || {
            semaphore.release();
        }));
    }
}
