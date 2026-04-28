use std::sync::Arc;
use wrela::audio_exec::{AudioSnapshotPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan};
use wrela_runtime::audio::ring::SampleRing;
use wrela_runtime::audio::voice::{VoiceLedger, VoiceRenderer};

#[test]
fn stereo_spatialization_attenuates_far_ear_and_carries_media_fields() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    let mut right = sine_voice(9, 10, 1.0);
    right.position = [1.0, 0.0, 0.0];
    right.media.reverb_send = 0.35;
    right.media.lowpass_hz = 4_000.0;
    publisher.publish(
        1,
        &AudioDspPlan {
            voices: vec![right.clone()],
        },
    );
    let snapshot = ledger.load();

    assert_eq!(snapshot.voices[0].reverb_send, 0.35);
    assert_eq!(snapshot.voices[0].lowpass_hz, 4_000.0);
    assert_eq!(
        snapshot.voices[0].source_signature,
        right.source_audio_signature
    );
    assert_eq!(
        snapshot.voices[0].source_frequency_hz,
        right.source_frequency_hz
    );

    let mut renderer = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let ring = SampleRing::with_capacity(4);
    renderer.render_to_ring(&snapshot.voices, &ring, 4);
    let mut out = [[0.0; 2]; 4];
    ring.pop_stereo_block(&mut out);

    assert!(
        out.iter().any(|frame| frame[1].abs() > frame[0].abs()),
        "a voice to listener-right should be louder in the right channel"
    );
}
