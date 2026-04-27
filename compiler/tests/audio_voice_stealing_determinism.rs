use std::sync::Arc;
use wrela::audio_exec::{AudioSnapshotPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan};
use wrela_runtime::audio::voice::VoiceLedger;

#[test]
fn voice_stealing_uses_priority_then_voice_id_order() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 3,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    );

    publisher.publish(
        1,
        &AudioDspPlan {
            voices: vec![
                sine_voice(30, 1, 0.5),
                sine_voice(10, 10, 0.5),
                sine_voice(20, 10, 0.5),
                sine_voice(40, 1, 0.5),
            ],
        },
    );

    let ids = ledger
        .load()
        .voices
        .iter()
        .map(|voice| voice.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![10, 20, 30]);
}
