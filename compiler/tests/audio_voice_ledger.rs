use std::sync::Arc;
use wrela::audio_exec::{AudioFinding, AudioSnapshotPublisher, sine_voice};
use wrela::audio_plan::{AudioConfig, AudioDspPlan};
use wrela_runtime::audio::ring::SampleRing;
use wrela_runtime::audio::voice::{VoiceLedger, VoiceRenderer, render_voices_to_ring};

#[test]
fn audio_voice_ledger_publishes_latest_snapshot_and_steals_deterministically() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 2,
            max_full_rate_media_queries: 1,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    );
    let report = publisher.publish(
        7,
        &AudioDspPlan {
            voices: vec![
                sine_voice(1, 1, 0.5),
                sine_voice(2, 10, 0.5),
                sine_voice(3, 5, 0.5),
            ],
        },
    );
    assert_eq!(report.published_voices, 2);
    assert_eq!(report.stolen_voices, 1);
    assert!(
        report
            .structured_findings
            .iter()
            .any(|f| *f == AudioFinding::VoiceCountOverCap)
    );
    assert!(
        !report
            .structured_findings
            .iter()
            .any(|f| *f == AudioFinding::MediaQueriesOverBudget),
        "exceeding the full-rate voice cap must not report over budget unless actual queries exceed the cap"
    );
    assert_eq!(report.media_queried_voice_ids, vec![2]);
    let latest = ledger.load();
    assert_eq!(latest.tick, 7);
    assert_eq!(latest.voices[0].id, 2);
    assert_eq!(latest.voices[1].id, 3);
}

#[test]
fn audio_underruns_are_drained_into_report_findings() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    publisher
        .underrun_counter()
        .fetch_add(2, std::sync::atomic::Ordering::Relaxed);
    let report = publisher.publish(1, &AudioDspPlan::default());
    assert_eq!(report.underruns, 2);
    assert!(
        report
            .structured_findings
            .iter()
            .any(|f| *f == AudioFinding::Underrun)
    );
    let next_report = publisher.publish(2, &AudioDspPlan::default());
    assert_eq!(
        next_report.underruns, 0,
        "engine-side underrun reporting must be a delta from a monotonic runtime counter"
    );
}

#[test]
fn runtime_renders_published_voice_snapshot_into_sample_ring() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    publisher.publish(
        3,
        &AudioDspPlan {
            voices: vec![sine_voice(7, 10, 0.5)],
        },
    );
    let snapshot = ledger.load();
    let ring = SampleRing::with_capacity(64);
    let written = render_voices_to_ring(
        &snapshot.voices,
        AudioConfig::default().sample_rate,
        &ring,
        32,
    );
    assert_eq!(written, 32);
    assert!(!ring.is_empty());
}

#[test]
fn media_queries_stagger_lower_priority_voices_without_spurious_budget_findings() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 4,
            max_full_rate_media_queries: 2,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    );
    let plan = AudioDspPlan {
        voices: vec![
            sine_voice(1, 100, 0.5),
            sine_voice(2, 90, 0.5),
            sine_voice(3, 10, 0.5),
            sine_voice(4, 5, 0.5),
        ],
    };

    let first = publisher.publish(1, &plan);
    let second = publisher.publish(2, &plan);
    let third = publisher.publish(3, &plan);

    assert_eq!(first.media_queries, 2);
    assert_eq!(second.media_queries, 2);
    assert_eq!(third.media_queries, 2);
    assert_eq!(first.media_queried_voice_ids, vec![1, 2]);
    assert_eq!(second.media_queried_voice_ids, vec![3, 4]);
    assert_eq!(third.media_queried_voice_ids, vec![1, 2]);
    assert!([first, second, third].iter().all(|report| {
        !report
            .structured_findings
            .contains(&AudioFinding::MediaQueriesOverBudget)
    }));
}

#[test]
fn renderer_keeps_voice_phase_continuous_across_blocks() {
    let voice = sine_voice(7, 10, 1.0);
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    publisher.publish(
        1,
        &AudioDspPlan {
            voices: vec![voice],
        },
    );
    let snapshot = ledger.load();
    let mut renderer = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let first = SampleRing::with_capacity(16);
    let second = SampleRing::with_capacity(16);

    renderer.render_to_ring(&snapshot.voices, &first, 8);
    renderer.render_to_ring(&snapshot.voices, &second, 8);

    let mut first_out = [[0.0; 2]; 8];
    let mut second_out = [[0.0; 2]; 8];
    first.pop_stereo_block(&mut first_out);
    second.pop_stereo_block(&mut second_out);
    assert_ne!(
        second_out[0], first_out[0],
        "second block must continue oscillator phase instead of restarting at t=0"
    );
    let boundary_delta = (second_out[0][0] - first_out[7][0]).abs();
    assert!(
        boundary_delta < 0.1,
        "sample-to-sample block boundary jump {boundary_delta} is too large"
    );
}

#[test]
fn voice_ledger_load_is_lock_free_and_returns_latest_snapshot() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    let ledger = Arc::new(VoiceLedger::new());
    let stop = Arc::new(AtomicBool::new(false));

    let publisher_ledger = Arc::clone(&ledger);
    let publisher_stop = Arc::clone(&stop);
    let writer = thread::spawn(move || {
        let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), publisher_ledger);
        let mut tick: u64 = 1;
        while !publisher_stop.load(Ordering::Relaxed) {
            publisher.publish(
                tick,
                &AudioDspPlan {
                    voices: vec![sine_voice(tick, 1, 0.5)],
                },
            );
            tick = tick.wrapping_add(1);
        }
        tick
    });

    let mut last_seen: u64 = 0;
    for _ in 0..1_000 {
        let snap = ledger.load();
        assert!(
            snap.tick >= last_seen,
            "lock-free load must observe monotonic publish order"
        );
        last_seen = snap.tick;
    }

    stop.store(true, Ordering::Relaxed);
    writer.join().expect("writer thread");
}
