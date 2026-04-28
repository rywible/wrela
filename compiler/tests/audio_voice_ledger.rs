use std::sync::Arc;
use wrela::audio_contract::MediaSample;
use wrela::audio_exec::{
    AudioFinding, AudioSnapshotPublisher, MediaSampleProvider, compile_audio_field_program,
    sine_voice,
};
use wrela::audio_plan::{AudioConfig, AudioDspPlan, AudioVoicePlan};
use wrela::hir::{self, FunctionRole};
use wrela::parser::ast::{AstNode, Root};
use wrela::parser::parse;
use wrela_runtime::audio::ring::SampleRing;
use wrela_runtime::audio::voice::{
    DspOp, DspProgram, DspValue, VoiceLedger, VoiceRenderer, render_voices_to_ring,
};

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
    assert_eq!(report.media_queried_voice_ids, vec![2, 3]);
    let latest = ledger.load();
    assert_eq!(latest.tick, 7);
    assert_eq!(latest.voices[0].id, 2);
    assert_eq!(latest.voices[1].id, 3);
    assert!(latest.voices[0].source_frequency_hz > 0.0);
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
fn authored_source_projection_changes_rendered_samples() {
    let mut voice_a = sine_voice(42, 10, 0.5);
    let mut voice_b = sine_voice(42, 10, 0.5);
    voice_a.source_audio_signature = 4;
    voice_a.source_frequency_hz = 220.0;
    voice_a.source_program = DspProgram::from_ops([
        DspOp::Push(DspValue::T),
        DspOp::Push(DspValue::Freq),
        DspOp::Mul,
        DspOp::Sin,
        DspOp::Return,
    ])
    .expect("program");
    voice_b.source_audio_signature = 8;
    voice_b.source_frequency_hz = 220.0;
    voice_b.source_program = DspProgram::from_ops([
        DspOp::Push(DspValue::T),
        DspOp::Push(DspValue::Freq),
        DspOp::Mul,
        DspOp::Sin,
        DspOp::Push(DspValue::Const(0.25)),
        DspOp::Mul,
        DspOp::Return,
    ])
    .expect("program");

    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    publisher.publish(
        1,
        &AudioDspPlan {
            voices: vec![voice_a.clone()],
        },
    );
    let first_snapshot = ledger.load();
    publisher.publish(
        2,
        &AudioDspPlan {
            voices: vec![voice_b.clone()],
        },
    );
    let second_snapshot = ledger.load();
    assert_ne!(
        first_snapshot.voices[0].source_signature,
        second_snapshot.voices[0].source_signature
    );
    assert_eq!(
        first_snapshot.voices[0].source_frequency_hz,
        second_snapshot.voices[0].source_frequency_hz
    );

    let mut renderer_a = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let mut renderer_b = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let ring_a = SampleRing::with_capacity(32);
    let ring_b = SampleRing::with_capacity(32);
    renderer_a.render_to_ring(&first_snapshot.voices, &ring_a, 16);
    renderer_b.render_to_ring(&second_snapshot.voices, &ring_b, 16);
    let mut out_a = [[0.0; 2]; 16];
    let mut out_b = [[0.0; 2]; 16];
    ring_a.pop_stereo_block(&mut out_a);
    ring_b.pop_stereo_block(&mut out_b);
    assert_ne!(
        out_a, out_b,
        "rendered samples must depend on authored source projection, not only voice id"
    );
}

#[test]
fn runtime_dsp_program_is_semantic_not_signature_modulo() {
    let mut loud = sine_voice(7, 10, 1.0);
    let mut quiet = sine_voice(7, 10, 1.0);
    loud.source_audio_signature = 4;
    quiet.source_audio_signature = 8;
    loud.source_frequency_hz = 220.0;
    quiet.source_frequency_hz = 220.0;
    loud.source_program = DspProgram::from_ops([
        DspOp::Push(DspValue::T),
        DspOp::Push(DspValue::Freq),
        DspOp::Mul,
        DspOp::Sin,
        DspOp::Return,
    ])
    .expect("program");
    quiet.source_program = DspProgram::from_ops([
        DspOp::Push(DspValue::T),
        DspOp::Push(DspValue::Freq),
        DspOp::Mul,
        DspOp::Sin,
        DspOp::Push(DspValue::Const(0.5)),
        DspOp::Mul,
        DspOp::Return,
    ])
    .expect("program");

    let mut renderer_a = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let mut renderer_b = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let ring_a = SampleRing::with_capacity(32);
    let ring_b = SampleRing::with_capacity(32);
    renderer_a.render_to_ring(
        &[wrela::audio_exec::voice_plan_to_state_for_test(&loud)],
        &ring_a,
        16,
    );
    renderer_b.render_to_ring(
        &[wrela::audio_exec::voice_plan_to_state_for_test(&quiet)],
        &ring_b,
        16,
    );

    let mut out_a = [[0.0; 2]; 16];
    let mut out_b = [[0.0; 2]; 16];
    ring_a.pop_stereo_block(&mut out_a);
    ring_b.pop_stereo_block(&mut out_b);
    let peak_a = out_a.iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);
    let peak_b = out_b.iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);

    assert!(
        peak_b < peak_a * 0.75,
        "same signature modulo class must still render according to the compiled program"
    );
}

#[test]
fn compiler_projects_authored_audio_field_body_into_rendered_program() {
    let module_a = lower_inline_module_from_source(
        r#"
@audio_rt audio field Tone(t: F32, freq: F32, gate: Boolean) -> F32 {
    if gate {
        return sin(t)
    } else {
        return 0.0
    }
}
"#,
    );
    let module_b = lower_inline_module_from_source(
        r#"
@audio_rt audio field Tone(t: F32, freq: F32, gate: Boolean) -> F32 {
    if gate {
        return sin(t) * 0.25
    } else {
        return 0.0
    }
}
"#,
    );
    let program_a = compile_audio_field_program(audio_field(&module_a, "Tone"));
    let program_b = compile_audio_field_program(audio_field(&module_b, "Tone"));
    assert_ne!(program_a, program_b);

    let mut voice_a = sine_voice(11, 10, 1.0);
    let mut voice_b = sine_voice(11, 10, 1.0);
    voice_a.source_audio_signature = 4;
    voice_b.source_audio_signature = 8;
    voice_a.source_program = program_a;
    voice_b.source_program = program_b;

    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(AudioConfig::default(), Arc::clone(&ledger));
    publisher.publish(
        1,
        &AudioDspPlan {
            voices: vec![voice_a],
        },
    );
    let snapshot_a = ledger.load();
    publisher.publish(
        2,
        &AudioDspPlan {
            voices: vec![voice_b],
        },
    );
    let snapshot_b = ledger.load();

    let mut renderer_a = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let mut renderer_b = VoiceRenderer::new(AudioConfig::default().sample_rate);
    let ring_a = SampleRing::with_capacity(32);
    let ring_b = SampleRing::with_capacity(32);
    renderer_a.render_to_ring(&snapshot_a.voices, &ring_a, 16);
    renderer_b.render_to_ring(&snapshot_b.voices, &ring_b, 16);

    let mut out_a = [[0.0; 2]; 16];
    let mut out_b = [[0.0; 2]; 16];
    ring_a.pop_stereo_block(&mut out_a);
    ring_b.pop_stereo_block(&mut out_b);
    let peak_a = out_a.iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);
    let peak_b = out_b.iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);
    assert!(
        peak_b < peak_a * 0.5,
        "authored field body changes must render as authored DSP, not hash modulo"
    );
}

#[test]
fn media_queries_keep_high_priority_voices_full_rate_and_stagger_lower_priority_voices() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 6,
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
            sine_voice(5, 4, 0.5),
            sine_voice(6, 3, 0.5),
        ],
    };

    let first = publisher.publish(1, &plan);
    let second = publisher.publish(2, &plan);
    let third = publisher.publish(3, &plan);

    assert_eq!(first.media_queries, 4);
    assert_eq!(second.media_queries, 4);
    assert_eq!(third.media_queries, 4);
    assert_eq!(first.media_queried_voice_ids[..2], [1, 2]);
    assert_eq!(second.media_queried_voice_ids[..2], [1, 2]);
    assert_eq!(third.media_queried_voice_ids[..2], [1, 2]);
    assert_eq!(&first.media_queried_voice_ids[2..], [3, 4]);
    assert_eq!(&second.media_queried_voice_ids[2..], [5, 6]);
    assert_eq!(&third.media_queried_voice_ids[2..], [3, 4]);
    assert!([first, second, third].iter().all(|report| {
        !report
            .structured_findings
            .contains(&AudioFinding::MediaQueriesOverBudget)
    }));
}

#[derive(Debug)]
struct IdMediaSampleProvider;

impl MediaSampleProvider for IdMediaSampleProvider {
    fn sample_media(&self, voice: &AudioVoicePlan) -> MediaSample {
        MediaSample {
            occlusion_db: -(voice.id.0 as f32),
            reverb_send: voice.id.0 as f32 * 0.1,
            lowpass_hz: 1_000.0 + voice.id.0 as f32,
        }
    }
}

#[test]
fn queried_voice_media_is_refreshed_before_publication() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 3,
            max_full_rate_media_queries: 1,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    )
    .with_media_sample_provider(Arc::new(IdMediaSampleProvider));
    let stale = MediaSample {
        occlusion_db: -96.0,
        reverb_send: 0.0,
        lowpass_hz: 200.0,
    };
    let mut voices = vec![
        sine_voice(1, 100, 0.5),
        sine_voice(2, 50, 0.5),
        sine_voice(3, 10, 0.5),
    ];
    for voice in &mut voices {
        voice.media = stale;
    }

    let report = publisher.publish(1, &AudioDspPlan { voices });
    assert_eq!(report.media_queried_voice_ids, vec![1, 2]);
    let snapshot = ledger.load();
    let voice_1 = snapshot.voices.iter().find(|voice| voice.id == 1).unwrap();
    let voice_2 = snapshot.voices.iter().find(|voice| voice.id == 2).unwrap();
    assert_eq!(voice_1.occlusion_db, -1.0);
    assert_eq!(voice_1.reverb_send, 0.1);
    assert_eq!(voice_1.lowpass_hz, 1_001.0);
    assert_eq!(voice_2.occlusion_db, -2.0);
    assert_eq!(voice_2.reverb_send, 0.2);
    assert_eq!(voice_2.lowpass_hz, 1_002.0);
}

#[test]
fn unqueried_voice_media_keeps_stale_plan_value() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 3,
            max_full_rate_media_queries: 1,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    )
    .with_media_sample_provider(Arc::new(IdMediaSampleProvider));
    let stale = MediaSample {
        occlusion_db: -12.0,
        reverb_send: 0.25,
        lowpass_hz: 700.0,
    };
    let mut voices = vec![
        sine_voice(1, 100, 0.5),
        sine_voice(2, 50, 0.5),
        sine_voice(3, 10, 0.5),
    ];
    for voice in &mut voices {
        voice.media = stale;
    }

    let report = publisher.publish(1, &AudioDspPlan { voices });
    assert_eq!(report.media_queried_voice_ids, vec![1, 2]);
    let snapshot = ledger.load();
    let voice_3 = snapshot.voices.iter().find(|voice| voice.id == 3).unwrap();
    assert_eq!(voice_3.occlusion_db, stale.occlusion_db);
    assert_eq!(voice_3.reverb_send, stale.reverb_send);
    assert_eq!(voice_3.lowpass_hz, stale.lowpass_hz);
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

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let root = Root::cast(parse(source)).expect("root");
    wrela::hir::lower::lower(root)
}

fn audio_field<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, function)| function.role == FunctionRole::AudioField && function.name == name)
        .map(|(_, function)| function)
        .expect("audio field")
}
