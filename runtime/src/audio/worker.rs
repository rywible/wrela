//! Audio worker helpers (RFC 0011 Phase 68).
//!
//! The audio callback drains a block at a time and reports *one* underrun per
//! block when the ring did not provide enough samples to satisfy the request.
//! Per-sample underruns inflated the metric and made it useless for SLO
//! reporting.

use super::ring::{SampleConsumer, SampleRing, StereoFrame};
use super::underrun_counter::UnderrunCounter;
use std::sync::atomic::{AtomicU64, Ordering};

/// Drain a full block of samples from `ring` into `output`. If the ring is
/// short of supplying a complete block, the remainder of `output` is set to
/// silence (`0.0`) and *one* underrun is recorded for the entire block.
pub fn fill_output_from_ring(output: &mut [f32], ring: &SampleRing, underruns: &UnderrunCounter) {
    let popped = fill_stereo_interleaved_from_pop(output, || ring.pop());
    if popped < stereo_frame_count(output) {
        output[popped.saturating_mul(2)..].fill(0.0);
        underruns.increment();
    }
}

/// Atomic-counter variant; counts *one* underrun per block that is not fully
/// satisfied.
pub fn fill_output_from_ring_atomic(output: &mut [f32], ring: &SampleRing, underruns: &AtomicU64) {
    let popped = fill_stereo_interleaved_from_pop(output, || ring.pop());
    if popped < stereo_frame_count(output) {
        output[popped.saturating_mul(2)..].fill(0.0);
        underruns.fetch_add(1, Ordering::Relaxed);
    }
}

/// Callback-owned consumer variant for the CPAL real-time path. The callback
/// owns `consumer`, so this avoids taking locks on the audio thread.
pub fn fill_output_from_consumer_atomic(
    output: &mut [f32],
    consumer: &mut SampleConsumer,
    underruns: &AtomicU64,
) {
    let popped = fill_stereo_interleaved_from_pop(output, || consumer.pop());
    if popped < stereo_frame_count(output) {
        output[popped.saturating_mul(2)..].fill(0.0);
        underruns.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn fill_output_from_consumer_channels_atomic(
    output: &mut [f32],
    channels: usize,
    consumer: &mut SampleConsumer,
    underruns: &AtomicU64,
) {
    let channels = channels.max(1);
    let frames = output.len() / channels;
    let mut popped = 0;
    for frame_index in 0..frames {
        let Some(frame) = consumer.pop() else {
            break;
        };
        let start = frame_index * channels;
        write_channels(frame, &mut output[start..start + channels]);
        popped += 1;
    }
    if popped < frames {
        output[popped * channels..].fill(0.0);
        underruns.fetch_add(1, Ordering::Relaxed);
    }
}

fn stereo_frame_count(output: &[f32]) -> usize {
    output.len() / 2
}

fn fill_stereo_interleaved_from_pop<F>(output: &mut [f32], mut pop: F) -> usize
where
    F: FnMut() -> Option<StereoFrame>,
{
    let frames = stereo_frame_count(output);
    let mut popped = 0;
    for frame_index in 0..frames {
        let Some(frame) = pop() else {
            break;
        };
        frame.write_interleaved(&mut output[frame_index * 2..frame_index * 2 + 2]);
        popped += 1;
    }
    if output.len() % 2 == 1 {
        if let Some(last) = output.last_mut() {
            *last = 0.0;
        }
    }
    popped
}

fn write_channels(frame: StereoFrame, output: &mut [f32]) {
    match output {
        [] => {}
        [mono] => *mono = (frame.left + frame.right) * 0.5,
        [left, right, rest @ ..] => {
            *left = frame.left;
            *right = frame.right;
            rest.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_block_is_drained_without_underrun() {
        let (mut producer, mut consumer) = SampleRing::split(8);
        producer.push_block(&[StereoFrame::new(0.1, -0.1), StereoFrame::new(0.2, -0.2)]);
        let mut out = [0.0f32; 4];
        let counter = AtomicU64::new(0);
        fill_output_from_consumer_atomic(&mut out, &mut consumer, &counter);
        assert_eq!(out, [0.1, -0.1, 0.2, -0.2]);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn partial_block_records_exactly_one_underrun() {
        let ring = SampleRing::with_capacity(8);
        ring.push_block(&[StereoFrame::new(0.1, -0.1)]);
        let mut out = [0.0f32; 4];
        let counter = AtomicU64::new(0);
        fill_output_from_ring_atomic(&mut out, &ring, &counter);
        assert_eq!(out, [0.1, -0.1, 0.0, 0.0]);
        assert_eq!(
            counter.load(Ordering::Relaxed),
            1,
            "one underrun per starved block"
        );
    }

    #[test]
    fn fully_starved_block_is_one_underrun_not_block_size() {
        let ring = SampleRing::with_capacity(8);
        let mut out = [1.0f32; 64];
        let counter = AtomicU64::new(0);
        fill_output_from_ring_atomic(&mut out, &ring, &counter);
        assert!(out.iter().all(|s| *s == 0.0));
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
