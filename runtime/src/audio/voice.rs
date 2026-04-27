//! Lock-free voice ledger and sample-rendering primitives that the audio
//! worker reads on the real-time thread (RFC 0011 Phase 68).
//!
//! The ledger lives in the runtime so that the audio thread never has to call
//! into the compiler crate to render samples. The compiler-side
//! [`crate::audio_exec::AudioSnapshotPublisher`](../../../wrela/audio_exec/struct.AudioSnapshotPublisher.html)
//! converts its `AudioVoicePlan` values into the runtime [`VoiceState`] mirror
//! and publishes a [`VoiceLedgerSnapshot`] via [`VoiceLedger::publish`]. The
//! runtime audio worker then uses [`VoiceLedger::load`] (which is wait-free for
//! readers thanks to `arc_swap`) to acquire the latest snapshot and render it
//! into a [`SampleRing`] with [`render_voices_to_ring`].
//!
//! Layering: this module deliberately does not depend on any compiler types so
//! that the runtime can render the audio thread without pulling in the
//! compiler.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::audio::ring::{SampleRing, StereoFrame};

/// Mirror of a single voice as understood by the runtime audio thread.
///
/// This is intentionally a value type with no compiler dependencies so the
/// runtime crate can stand alone (RFC 0011 Layering Doctrine).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceState {
    pub id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub gain: f32,
    pub priority: i32,
    pub occlusion_db: f32,
    pub reverb_send: f32,
    pub lowpass_hz: f32,
    pub gate: bool,
}

/// Snapshot of voice state published for a particular simulation tick.
///
/// `voices` is stored as `Arc<[VoiceState]>` so cloning the snapshot is cheap
/// (one atomic increment) and the audio thread never allocates.
#[derive(Debug, Clone)]
pub struct VoiceLedgerSnapshot {
    pub tick: u64,
    pub voices: Arc<[VoiceState]>,
}

impl Default for VoiceLedgerSnapshot {
    fn default() -> Self {
        Self {
            tick: 0,
            voices: Arc::from(Vec::<VoiceState>::new()),
        }
    }
}

/// Lock-free, wait-free-on-read voice ledger used to hand voice state from the
/// engine thread to the runtime audio worker (RFC 0011 Phase 68 AC: voice
/// ledger is lock-free).
///
/// The reader (audio worker) calls [`VoiceLedger::load`] which is wait-free,
/// performs only an atomic load and an `Arc` clone, and never blocks the
/// real-time audio thread. The writer (engine thread) calls
/// [`VoiceLedger::publish`] which is lock-free; concurrent writers may retry
/// internally inside `arc_swap` but this side does not run on the audio
/// thread.
#[derive(Debug, Default)]
pub struct VoiceLedger {
    cell: ArcSwap<VoiceLedgerSnapshot>,
}

impl VoiceLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish the latest voice snapshot from the engine thread.
    pub fn publish(&self, snapshot: VoiceLedgerSnapshot) {
        self.cell.store(Arc::new(snapshot));
    }

    /// Load the most recently published snapshot. This is wait-free for the
    /// audio worker thread.
    pub fn load(&self) -> Arc<VoiceLedgerSnapshot> {
        self.cell.load_full()
    }

    /// Convenience for tests: returns the current tick.
    pub fn latest_tick(&self) -> u64 {
        self.cell.load().tick
    }
}

/// Render the supplied voice snapshot into the given [`SampleRing`] using a
/// simple bank of distance-attenuated sine voices.
///
/// This is the runtime-owned renderer (RFC 0011 Phase 68 AC: engine thread
/// does not render samples). `frames` controls how many sample frames will be
/// computed; the function returns the number of samples actually pushed into
/// the ring (which may be less if the ring filled).
pub fn render_voices_to_ring(
    voices: &[VoiceState],
    sample_rate: u32,
    ring: &SampleRing,
    frames: usize,
) -> usize {
    let mut renderer = VoiceRenderer::new(sample_rate);
    renderer.render_to_ring(voices, ring, frames)
}

#[derive(Debug, Clone)]
pub struct VoiceRenderer {
    sample_rate: u32,
    phases: HashMap<u64, f32>,
}

impl VoiceRenderer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            phases: HashMap::new(),
        }
    }

    pub fn render_to_ring(
        &mut self,
        voices: &[VoiceState],
        ring: &SampleRing,
        frames: usize,
    ) -> usize {
        let sr = self.sample_rate as f32;
        self.phases
            .retain(|id, _| voices.iter().any(|voice| voice.id == *id));

        let mut written = 0usize;
        for _ in 0..frames {
            let mut left = 0.0f32;
            let mut right = 0.0f32;
            for voice in voices.iter().filter(|voice| voice.gate) {
                let freq = voice_frequency(voice.id);
                let phase = self.phases.entry(voice.id).or_insert(0.0);
                let sample = phase.sin()
                    * voice.gain
                    * distance_attenuation(voice.position)
                    * media_gain(voice);
                let (left_gain, right_gain) = ild_gains(voice.position);
                left += sample * left_gain;
                right += sample * right_gain;
                *phase = (*phase + std::f32::consts::TAU * freq / sr) % std::f32::consts::TAU;
            }
            if !ring.push(StereoFrame::new(
                left.clamp(-1.0, 1.0),
                right.clamp(-1.0, 1.0),
            )) {
                break;
            }
            written += 1;
        }
        written
    }
}

fn distance_attenuation(position: [f32; 3]) -> f32 {
    let d2 = position[0] * position[0] + position[1] * position[1] + position[2] * position[2];
    1.0 / (1.0 + d2.sqrt() * 0.1)
}

fn voice_frequency(id: u64) -> f32 {
    220.0 + (id % 32) as f32 * 11.0
}

fn media_gain(voice: &VoiceState) -> f32 {
    let occlusion_gain = 10.0f32.powf((-voice.occlusion_db.max(0.0)) / 20.0);
    let lowpass_gain = (voice.lowpass_hz.max(200.0) / 20_000.0)
        .sqrt()
        .clamp(0.1, 1.0);
    occlusion_gain * lowpass_gain
}

fn ild_gains(position: [f32; 3]) -> (f32, f32) {
    let lateral = position[0];
    let depth = position[2].abs() + 1.0;
    let pan = (lateral / depth).clamp(-1.0, 1.0);
    if pan >= 0.0 {
        (1.0 - pan * 0.35, 1.0)
    } else {
        (1.0, 1.0 + pan * 0.35)
    }
}
