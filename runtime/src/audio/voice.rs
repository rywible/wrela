//! Lock-free voice ledger and sample-rendering primitives that the audio
//! worker reads on the real-time thread (RFC 0011 Phase 68).
//!
//! The ledger lives in the runtime so that the audio thread never has to call
//! into the compiler crate to render samples. The compiler-side
//! [`crate::audio_exec::AudioSnapshotPublisher`](../../../wrela/audio_exec/struct.AudioSnapshotPublisher.html)
//! converts its `AudioVoicePlan` values into the runtime [`VoiceState`] mirror
//! and publishes a [`VoiceLedgerSnapshot`] via [`VoiceLedger::publish`]. The
//! runtime audio device callback then uses [`VoiceLedger::load`] (which is
//! wait-free for readers thanks to `arc_swap`) to acquire the latest snapshot
//! and render it directly with [`VoiceRenderer`]. [`render_voices_to_ring`] is
//! retained for non-real-time tests and compatibility helpers.
//!
//! Layering: this module deliberately does not depend on any compiler types so
//! that the runtime can render the audio thread without pulling in the
//! compiler.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::audio::ring::{SampleRing, StereoFrame};

pub const DSP_PROGRAM_MAX_OPS: usize = 32;
pub const DSP_STACK_MAX: usize = 8;
const VOICE_PHASE_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DspValue {
    Const(f32),
    T,
    Freq,
    Gate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DspOp {
    Nop,
    Push(DspValue),
    Add,
    Sub,
    Mul,
    Div,
    Sin,
    SelectGate,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspProgram {
    pub ops: [DspOp; DSP_PROGRAM_MAX_OPS],
    pub len: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspProgramError {
    TooManyOps,
    Empty,
}

impl DspProgram {
    pub const fn sine() -> Self {
        let mut ops = [DspOp::Nop; DSP_PROGRAM_MAX_OPS];
        ops[0] = DspOp::Push(DspValue::T);
        ops[1] = DspOp::Sin;
        ops[2] = DspOp::Return;
        Self { ops, len: 3 }
    }

    pub fn from_ops<const N: usize>(ops: [DspOp; N]) -> Result<Self, DspProgramError> {
        if N == 0 {
            return Err(DspProgramError::Empty);
        }
        if N > DSP_PROGRAM_MAX_OPS {
            return Err(DspProgramError::TooManyOps);
        }
        let mut program_ops = [DspOp::Nop; DSP_PROGRAM_MAX_OPS];
        program_ops[..N].copy_from_slice(&ops);
        Ok(Self {
            ops: program_ops,
            len: N as u8,
        })
    }

    pub fn evaluate(&self, t: f32, freq: f32, gate: bool) -> f32 {
        let mut stack = [0.0f32; DSP_STACK_MAX];
        let mut sp = 0usize;
        for op in self.ops.iter().take(self.len as usize) {
            match *op {
                DspOp::Nop => {}
                DspOp::Push(value) => push(&mut stack, &mut sp, dsp_value(value, t, freq, gate)),
                DspOp::Add => binary(&mut stack, &mut sp, |lhs, rhs| lhs + rhs),
                DspOp::Sub => binary(&mut stack, &mut sp, |lhs, rhs| lhs - rhs),
                DspOp::Mul => binary(&mut stack, &mut sp, |lhs, rhs| lhs * rhs),
                DspOp::Div => binary(&mut stack, &mut sp, |lhs, rhs| {
                    if rhs.abs() <= f32::EPSILON {
                        0.0
                    } else {
                        lhs / rhs
                    }
                }),
                DspOp::Sin => unary(&mut stack, sp, f32::sin),
                DspOp::SelectGate => {
                    if sp >= 2 {
                        let false_value = pop(&stack, &mut sp);
                        let true_value = pop(&stack, &mut sp);
                        push(
                            &mut stack,
                            &mut sp,
                            if gate { true_value } else { false_value },
                        );
                    }
                }
                DspOp::Return => return stack.get(sp.saturating_sub(1)).copied().unwrap_or(0.0),
            }
        }
        stack.get(sp.saturating_sub(1)).copied().unwrap_or(0.0)
    }
}

impl Default for DspProgram {
    fn default() -> Self {
        Self::sine()
    }
}

/// Mirror of a single voice as understood by the runtime audio thread.
///
/// This is intentionally a value type with no compiler dependencies so the
/// runtime crate can stand alone (RFC 0011 Layering Doctrine).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceState {
    pub id: u64,
    /// Stable signature of the authored audio field used by this voice.
    ///
    /// Runtime stays compiler-layer free, but the compiler-side publisher
    /// projects the authored field identity/body into this scalar so rendered
    /// audio changes when authors change the source field instead of being
    /// keyed only by voice id.
    pub source_signature: u64,
    pub source_program: DspProgram,
    pub source_frequency_hz: f32,
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
    phases: [VoicePhase; VOICE_PHASE_CAP],
}

impl VoiceRenderer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            phases: [VoicePhase::default(); VOICE_PHASE_CAP],
        }
    }

    pub fn render_to_ring(
        &mut self,
        voices: &[VoiceState],
        ring: &SampleRing,
        frames: usize,
    ) -> usize {
        let mut block = vec![StereoFrame::SILENCE; frames];
        let rendered = self.render_block(voices, &mut block);
        ring.push_block(&block[..rendered])
    }

    pub fn render_block(&mut self, voices: &[VoiceState], output: &mut [StereoFrame]) -> usize {
        let sr = self.sample_rate as f32;
        self.retain_active_phases(voices);

        for frame in output.iter_mut() {
            let mut left = 0.0f32;
            let mut right = 0.0f32;
            for voice in voices {
                let freq = voice.source_frequency_hz.max(1.0);
                let phase_slot = self.phase_slot_for(voice.id);
                let phase = self.phases[phase_slot].phase;
                let gate_gain = if voice.gate { 1.0 } else { 0.0 };
                let sample = voice.source_program.evaluate(phase, freq, voice.gate)
                    * gate_gain
                    * voice.gain
                    * distance_attenuation(voice.position)
                    * media_gain(voice);
                let (left_gain, right_gain) = ild_gains(voice.position);
                left += sample * left_gain;
                right += sample * right_gain;
                self.phases[phase_slot].phase =
                    (phase + std::f32::consts::TAU * freq / sr) % std::f32::consts::TAU;
            }
            *frame = StereoFrame::new(left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0));
        }
        output.len()
    }

    fn retain_active_phases(&mut self, voices: &[VoiceState]) {
        for phase in &mut self.phases {
            if phase.active && !voices.iter().any(|voice| voice.id == phase.id) {
                *phase = VoicePhase::default();
            }
        }
    }

    fn phase_slot_for(&mut self, id: u64) -> usize {
        if let Some(index) = self
            .phases
            .iter()
            .position(|phase| phase.active && phase.id == id)
        {
            return index;
        }
        if let Some(index) = self.phases.iter().position(|phase| !phase.active) {
            self.phases[index] = VoicePhase {
                id,
                phase: 0.0,
                active: true,
            };
            return index;
        }
        let index = id as usize % VOICE_PHASE_CAP;
        self.phases[index] = VoicePhase {
            id,
            phase: 0.0,
            active: true,
        };
        index
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct VoicePhase {
    id: u64,
    phase: f32,
    active: bool,
}

fn distance_attenuation(position: [f32; 3]) -> f32 {
    let d2 = position[0] * position[0] + position[1] * position[1] + position[2] * position[2];
    1.0 / (1.0 + d2.sqrt() * 0.1)
}

fn media_gain(voice: &VoiceState) -> f32 {
    let occlusion_gain = 10.0f32.powf((-voice.occlusion_db.max(0.0)) / 20.0);
    let lowpass_gain = (voice.lowpass_hz.max(200.0) / 20_000.0)
        .sqrt()
        .clamp(0.1, 1.0);
    occlusion_gain * lowpass_gain
}

fn dsp_value(value: DspValue, t: f32, freq: f32, gate: bool) -> f32 {
    match value {
        DspValue::Const(value) => value,
        DspValue::T => t,
        DspValue::Freq => freq,
        DspValue::Gate => {
            if gate {
                1.0
            } else {
                0.0
            }
        }
    }
}

fn push(stack: &mut [f32; DSP_STACK_MAX], sp: &mut usize, value: f32) {
    if *sp < DSP_STACK_MAX {
        stack[*sp] = value;
        *sp += 1;
    }
}

fn pop(stack: &[f32; DSP_STACK_MAX], sp: &mut usize) -> f32 {
    if *sp == 0 {
        return 0.0;
    }
    *sp -= 1;
    stack[*sp]
}

fn unary(stack: &mut [f32; DSP_STACK_MAX], sp: usize, op: impl FnOnce(f32) -> f32) {
    if sp > 0 {
        stack[sp - 1] = op(stack[sp - 1]);
    }
}

fn binary(stack: &mut [f32; DSP_STACK_MAX], sp: &mut usize, op: impl FnOnce(f32, f32) -> f32) {
    if *sp >= 2 {
        let rhs = pop(stack, sp);
        let lhs = pop(stack, sp);
        push(stack, sp, op(lhs, rhs));
    }
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
