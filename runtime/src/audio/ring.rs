//! SPSC sample ring used by the audio output thread (RFC 0011 Phase 68).
//!
//! Bulk push/pop operations dominate steady-state audio I/O; per-sample
//! atomic operations would dominate the cost in `fill_output_from_ring`. The
//! ring holds typed stereo frames; the worker expands them into interleaved
//! callback buffers.

use rtrb::{Consumer as RtrbConsumer, Producer as RtrbProducer, RingBuffer};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StereoFrame {
    pub left: f32,
    pub right: f32,
}

impl StereoFrame {
    pub const SILENCE: Self = Self {
        left: 0.0,
        right: 0.0,
    };

    pub fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }

    pub fn mono(sample: f32) -> Self {
        Self {
            left: sample,
            right: sample,
        }
    }

    pub fn write_interleaved(self, output: &mut [f32]) {
        if let Some(left) = output.get_mut(0) {
            *left = self.left;
        }
        if let Some(right) = output.get_mut(1) {
            *right = self.right;
        }
    }
}

pub struct SampleProducer {
    producer: RtrbProducer<StereoFrame>,
}

impl SampleProducer {
    /// Push a single stereo frame. Returns true if the frame was enqueued.
    /// Prefer `push_block` for the steady-state path.
    pub fn push(&mut self, frame: StereoFrame) -> bool {
        self.producer.push(frame).is_ok()
    }

    /// Push as many stereo frames from `block` as the ring can accept. Returns
    /// the number of frames actually pushed.
    pub fn push_block(&mut self, block: &[StereoFrame]) -> usize {
        if block.is_empty() {
            return 0;
        }
        let mut pushed = 0;
        for frame in block {
            if self.producer.push(*frame).is_ok() {
                pushed += 1;
            } else {
                break;
            }
        }
        pushed
    }
}

pub struct SampleConsumer {
    consumer: RtrbConsumer<StereoFrame>,
}

impl SampleConsumer {
    /// Pop a single stereo frame. Prefer `pop_block` on the audio callback.
    pub fn pop(&mut self) -> Option<StereoFrame> {
        self.consumer.pop().ok()
    }

    /// Drain up to `output.len()` frames into `output`, returning the number
    /// of frames actually written. Untouched frames are left as the caller
    /// initialised them (so the audio callback can pre-zero output once).
    pub fn pop_block(&mut self, output: &mut [StereoFrame]) -> usize {
        if output.is_empty() {
            return 0;
        }
        let mut popped = 0;
        for slot in output.iter_mut() {
            match self.consumer.pop() {
                Ok(frame) => {
                    *slot = frame;
                    popped += 1;
                }
                Err(_) => break,
            }
        }
        popped
    }

    pub fn len(&self) -> usize {
        self.consumer.slots()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Single-producer / single-consumer ring of stereo sample frames.
///
/// This lock-backed wrapper is for non-real-time helpers and compatibility
/// tests. CPAL callbacks must own a [`SampleConsumer`] from [`SampleRing::split`]
/// instead, so the audio thread never waits on a mutex.
pub struct SampleRing {
    capacity: usize,
    producer: Mutex<SampleProducer>,
    consumer: Mutex<SampleConsumer>,
}

impl SampleRing {
    pub fn split(capacity: usize) -> (SampleProducer, SampleConsumer) {
        let capacity = capacity.max(1);
        let (producer, consumer) = RingBuffer::new(capacity);
        (SampleProducer { producer }, SampleConsumer { consumer })
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (producer, consumer) = Self::split(capacity);
        Self {
            capacity,
            producer: Mutex::new(producer),
            consumer: Mutex::new(consumer),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Push a single stereo frame. Returns true if the frame was enqueued.
    /// Prefer `push_block` for the steady-state path.
    pub fn push(&self, sample: StereoFrame) -> bool {
        let mut producer = self.producer.lock().expect("SampleRing producer lock");
        producer.push(sample)
    }

    /// Push as many frames from `block` as the ring can accept. Returns the
    /// number of frames actually pushed.
    pub fn push_block(&self, block: &[StereoFrame]) -> usize {
        if block.is_empty() {
            return 0;
        }
        let mut producer = self.producer.lock().expect("SampleRing producer lock");
        producer.push_block(block)
    }

    pub fn push_mono_for_test(&self, sample: f32) -> bool {
        self.push(StereoFrame::mono(sample))
    }

    /// Pop a single stereo frame. Prefer `pop_block` on the audio callback.
    pub fn pop(&self) -> Option<StereoFrame> {
        let mut consumer = self.consumer.lock().expect("SampleRing consumer lock");
        consumer.pop()
    }

    /// Drain up to `output.len()` frames into `output`, returning the number
    /// of frames actually written. Untouched frames are left as the caller
    /// initialised them (so the audio callback can pre-zero output once).
    pub fn pop_block(&self, output: &mut [StereoFrame]) -> usize {
        if output.is_empty() {
            return 0;
        }
        let mut consumer = self.consumer.lock().expect("SampleRing consumer lock");
        consumer.pop_block(output)
    }

    pub fn pop_stereo_block(&self, output: &mut [[f32; 2]]) -> usize {
        let mut popped = 0;
        for frame in output.iter_mut() {
            let Some(sample) = self.pop() else {
                break;
            };
            *frame = [sample.left, sample.right];
            popped += 1;
        }
        popped
    }

    pub fn len(&self) -> usize {
        let consumer = self.consumer.lock().expect("SampleRing consumer lock");
        consumer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
