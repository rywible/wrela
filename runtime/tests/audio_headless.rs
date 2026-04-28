use std::sync::atomic::{AtomicU64, Ordering};
use wrela_runtime::audio::ring::{SampleRing, StereoFrame};
use wrela_runtime::audio::voice::{DspProgram, VoiceRenderer, VoiceState};
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

#[test]
fn renderer_mutes_gated_off_voice_even_when_program_ignores_gate() {
    let ring = SampleRing::with_capacity(16);
    let voice = VoiceState {
        id: 1,
        source_signature: 1,
        source_program: DspProgram::sine(),
        source_frequency_hz: 440.0,
        position: [0.0, 0.0, 0.0],
        velocity: [0.0, 0.0, 0.0],
        gain: 1.0,
        priority: 0,
        occlusion_db: 0.0,
        reverb_send: 0.0,
        lowpass_hz: 20_000.0,
        gate: false,
    };
    let mut renderer = VoiceRenderer::new(48_000);

    assert_eq!(renderer.render_to_ring(&[voice], &ring, 16), 16);

    let mut frames = [StereoFrame::default(); 16];
    assert_eq!(ring.pop_block(&mut frames), 16);
    assert!(
        frames
            .iter()
            .all(|frame| frame.left == 0.0 && frame.right == 0.0)
    );
}

#[test]
fn renderer_can_fill_host_output_block_without_sample_ring() {
    let voice = VoiceState {
        id: 7,
        source_signature: 7,
        source_program: DspProgram::sine(),
        source_frequency_hz: 440.0,
        position: [0.0, 0.0, 0.0],
        velocity: [0.0, 0.0, 0.0],
        gain: 0.5,
        priority: 0,
        occlusion_db: 0.0,
        reverb_send: 0.0,
        lowpass_hz: 20_000.0,
        gate: true,
    };
    let mut renderer = VoiceRenderer::new(48_000);
    let mut block = [StereoFrame::SILENCE; 32];

    assert_eq!(renderer.render_block(&[voice], &mut block), block.len());
    assert!(
        block
            .iter()
            .any(|frame| frame.left != 0.0 || frame.right != 0.0),
        "host output bridge should receive non-silent samples for an active voice"
    );
}
