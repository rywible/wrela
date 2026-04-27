use std::sync::atomic::{AtomicU64, Ordering};
use wrela_runtime::audio::ring::{SampleRing, StereoFrame};
use wrela_runtime::audio::underrun_counter::UnderrunCounter;
use wrela_runtime::audio::worker::{fill_output_from_ring, fill_output_from_ring_atomic};

#[test]
fn audio_worker_fills_missing_samples_with_silence_and_records_one_underrun_per_starved_block() {
    let ring = SampleRing::with_capacity(4);
    assert!(ring.push(StereoFrame::new(0.25, -0.25)));
    let underruns = UnderrunCounter::default();
    let mut output = [1.0; 6];
    fill_output_from_ring(&mut output, &ring, &underruns);
    assert_eq!(output, [0.25, -0.25, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        underruns.take(),
        1,
        "RFC 0011 Phase 68: one underrun is recorded per starved block, not per missing sample"
    );
}

#[test]
fn audio_worker_records_one_atomic_underrun_per_starved_block() {
    let ring = SampleRing::with_capacity(2);
    let underruns = AtomicU64::new(0);
    let mut output = [1.0; 4];
    fill_output_from_ring_atomic(&mut output, &ring, &underruns);
    assert_eq!(output, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(underruns.load(Ordering::Relaxed), 1);
}

#[test]
fn audio_worker_does_not_underrun_when_ring_satisfies_request() {
    let ring = SampleRing::with_capacity(8);
    let pushed = ring.push_block(&[StereoFrame::new(0.1, -0.1), StereoFrame::new(0.2, -0.2)]);
    assert_eq!(pushed, 2);
    let underruns = AtomicU64::new(0);
    let mut output = [0.0; 4];
    fill_output_from_ring_atomic(&mut output, &ring, &underruns);
    assert_eq!(output, [0.1, -0.1, 0.2, -0.2]);
    assert_eq!(underruns.load(Ordering::Relaxed), 0);
}
