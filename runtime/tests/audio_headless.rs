use std::sync::atomic::{AtomicU64, Ordering};
use wrela_runtime::audio::ring::{SampleRing, StereoFrame};
use wrela_runtime::audio::worker::fill_output_from_consumer_atomic;

#[test]
fn callback_owned_consumer_drains_without_lock_or_underrun() {
    let (mut producer, mut consumer) = SampleRing::split(16);
    assert_eq!(
        producer.push_block(&[StereoFrame::new(0.1, -0.1), StereoFrame::new(0.2, -0.2),]),
        2
    );

    let underruns = AtomicU64::new(0);
    let mut out = [0.0; 4];
    fill_output_from_consumer_atomic(&mut out, &mut consumer, &underruns);

    assert_eq!(out, [0.1, -0.1, 0.2, -0.2]);
    assert_eq!(underruns.load(Ordering::Relaxed), 0);
}
