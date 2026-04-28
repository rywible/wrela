//! Audio snapshot publisher (RFC 0011 Phase 68).
//!
//! The compiler is responsible for projecting the engine-side `AudioDspPlan`
//! into the runtime-owned, lock-free voice ledger. The compiler crate
//! intentionally does NOT render samples or own the [`SampleRing`]; the audio
//! worker in the runtime crate consumes the published [`VoiceLedgerSnapshot`]
//! and renders samples on the audio thread.

#![forbid(unsafe_code)]

pub mod rt_check;

use crate::audio_contract::{MediaSample, VoiceId};
use crate::audio_plan::{AudioConfig, AudioDspPlan, AudioVoicePlan, DspOp, DspProgram, DspValue};
use crate::hir::{Arg, BinaryOp, Body, Expr, Function, Literal, Stmt};
use smol_str::SmolStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use wrela_runtime::audio::voice::{VoiceLedger, VoiceLedgerSnapshot, VoiceState};

pub trait MediaSampleProvider: Send + Sync + std::fmt::Debug {
    fn sample_media(&self, voice: &AudioVoicePlan) -> MediaSample;
}

#[derive(Debug, Default)]
struct PlanMediaSampleProvider;

impl MediaSampleProvider for PlanMediaSampleProvider {
    fn sample_media(&self, voice: &AudioVoicePlan) -> MediaSample {
        voice.media
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioFinding {
    Underrun,
    VoiceCountOverCap,
    MediaQueriesOverBudget,
}

impl AudioFinding {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioFinding::Underrun => "audio.underrun",
            AudioFinding::VoiceCountOverCap => "audio.voice_count_over_cap",
            AudioFinding::MediaQueriesOverBudget => "audio.media_queries_over_budget",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioFrameReport {
    pub published_voices: u32,
    pub stolen_voices: u32,
    pub media_queries: u32,
    pub media_queried_voice_ids: Vec<u64>,
    pub underruns: u64,
    pub structured_findings: Vec<AudioFinding>,
    pub findings: Vec<String>,
}

#[derive(Debug)]
pub struct AudioSnapshotPublisher {
    pub config: AudioConfig,
    pub ledger: Arc<VoiceLedger>,
    media_sample_provider: Arc<dyn MediaSampleProvider>,
    underrun_counter: Arc<AtomicU64>,
    last_seen_underruns: AtomicU64,
    media_query_cursor: AtomicU64,
}

impl AudioSnapshotPublisher {
    pub fn new(config: AudioConfig, ledger: Arc<VoiceLedger>) -> Self {
        Self {
            config,
            ledger,
            media_sample_provider: Arc::new(PlanMediaSampleProvider),
            underrun_counter: Arc::new(AtomicU64::new(0)),
            last_seen_underruns: AtomicU64::new(0),
            media_query_cursor: AtomicU64::new(0),
        }
    }

    pub fn underrun_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.underrun_counter)
    }

    pub fn with_runtime_underrun_counter(
        config: AudioConfig,
        ledger: Arc<VoiceLedger>,
        underrun_counter: Arc<AtomicU64>,
    ) -> Self {
        Self {
            config,
            ledger,
            media_sample_provider: Arc::new(PlanMediaSampleProvider),
            underrun_counter,
            last_seen_underruns: AtomicU64::new(0),
            media_query_cursor: AtomicU64::new(0),
        }
    }

    pub fn with_media_sample_provider(
        mut self,
        media_sample_provider: Arc<dyn MediaSampleProvider>,
    ) -> Self {
        self.media_sample_provider = media_sample_provider;
        self
    }

    pub fn publish(&self, tick: u64, plan: &AudioDspPlan) -> AudioFrameReport {
        let mut voices = plan.voices.clone();
        voices.sort_by_key(|voice| (std::cmp::Reverse(voice.priority), voice.id.0));
        let stolen = voices.len().saturating_sub(self.config.max_voices);
        voices.truncate(self.config.max_voices);
        let media_queried_voice_ids = self.media_queries_for_frame(&voices);
        for voice in &mut voices {
            if media_queried_voice_ids.contains(&voice.id.0) {
                voice.media = self.media_sample_provider.sample_media(voice);
            }
        }
        let media_queries = media_queried_voice_ids.len();
        let runtime_voices: Vec<VoiceState> = voices.iter().map(voice_plan_to_state).collect();
        let snapshot = VoiceLedgerSnapshot {
            tick,
            voices: Arc::from(runtime_voices),
        };
        let published_voices = snapshot.voices.len() as u32;
        self.ledger.publish(snapshot);
        let total_underruns = self.underrun_counter.load(Ordering::Acquire);
        let last_seen = self
            .last_seen_underruns
            .swap(total_underruns, Ordering::AcqRel);
        let underruns = total_underruns.saturating_sub(last_seen);
        let mut structured_findings = Vec::new();
        if underruns > 0 {
            structured_findings.push(AudioFinding::Underrun);
        }
        if stolen > 0 {
            structured_findings.push(AudioFinding::VoiceCountOverCap);
        }
        if media_queries > self.media_query_budget(voices.len()) {
            structured_findings.push(AudioFinding::MediaQueriesOverBudget);
        }
        let findings = structured_findings
            .iter()
            .map(|finding| finding.as_str().to_string())
            .collect();
        AudioFrameReport {
            published_voices,
            stolen_voices: stolen as u32,
            media_queries: media_queries as u32,
            media_queried_voice_ids,
            underruns,
            structured_findings,
            findings,
        }
    }

    fn media_queries_for_frame(&self, voices: &[AudioVoicePlan]) -> Vec<u64> {
        let cap = self.config.max_full_rate_media_queries;
        if cap == 0 || voices.is_empty() {
            return Vec::new();
        }
        let full_rate_count = cap.min(voices.len());
        let mut ids: Vec<u64> = voices
            .iter()
            .take(full_rate_count)
            .map(|voice| voice.id.0)
            .collect();

        let overflow = &voices[full_rate_count..];
        if !overflow.is_empty() {
            let wave = cap.min(overflow.len());
            let cursor = self
                .media_query_cursor
                .fetch_add(wave as u64, Ordering::AcqRel) as usize;
            for offset in 0..wave {
                ids.push(overflow[(cursor + offset) % overflow.len()].id.0);
            }
        }

        ids
    }

    fn media_query_budget(&self, voice_count: usize) -> usize {
        let cap = self.config.max_full_rate_media_queries;
        if cap == 0 {
            0
        } else if voice_count > cap {
            cap + cap.min(voice_count - cap)
        } else {
            cap
        }
    }
}

fn voice_plan_to_state(plan: &AudioVoicePlan) -> VoiceState {
    VoiceState {
        id: plan.id.0,
        source_signature: plan.source_audio_signature,
        source_program: plan.source_program,
        source_frequency_hz: plan.source_frequency_hz,
        position: plan.position,
        velocity: plan.velocity,
        gain: plan.gain,
        priority: plan.priority,
        occlusion_db: plan.media.occlusion_db,
        reverb_send: plan.media.reverb_send,
        lowpass_hz: plan.media.lowpass_hz,
        gate: plan.gate,
    }
}

#[doc(hidden)]
pub fn voice_plan_to_state_for_test(plan: &AudioVoicePlan) -> VoiceState {
    voice_plan_to_state(plan)
}

pub fn compile_audio_field_program(function: &Function) -> DspProgram {
    let Some(body) = function.body.as_ref() else {
        return DspProgram::default();
    };
    let mut ops = Vec::new();
    if compile_body_return(body, &body.root_stmts, &mut ops) {
        ops.push(DspOp::Return);
        return dsp_program_from_vec(ops);
    }
    DspProgram::default()
}

pub fn sine_voice(id: u64, priority: i32, gain: f32) -> AudioVoicePlan {
    AudioVoicePlan {
        id: VoiceId(id),
        source_audio_field: Some(SmolStr::new("sine")),
        source_audio_signature: 0,
        source_program: DspProgram::default(),
        source_frequency_hz: 220.0 + (id % 32) as f32 * 11.0,
        position: [0.0, 0.0, 0.0],
        velocity: [0.0, 0.0, 0.0],
        gain,
        priority,
        media: Default::default(),
        gate: true,
    }
}

fn compile_body_return(body: &Body, stmts: &[crate::hir::Idx<Stmt>], ops: &mut Vec<DspOp>) -> bool {
    for stmt_id in stmts {
        match &body.stmts[*stmt_id] {
            Stmt::Return(Some(expr)) => return compile_expr(body, *expr, ops),
            Stmt::Expr(expr) => return compile_expr(body, *expr, ops),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } if is_gate_expr(body, *condition) => {
                if compile_body_return(body, then_branch, ops)
                    && compile_body_return(body, else_branch.as_deref().unwrap_or(&[]), ops)
                {
                    ops.push(DspOp::SelectGate);
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn compile_expr(body: &Body, expr: crate::hir::Idx<Expr>, ops: &mut Vec<DspOp>) -> bool {
    match &body.exprs[expr] {
        Expr::Literal(Literal::Integer(value)) => {
            ops.push(DspOp::Push(DspValue::Const(*value as f32)));
            true
        }
        Expr::Literal(Literal::Float(value)) => {
            ops.push(DspOp::Push(DspValue::Const(*value as f32)));
            true
        }
        Expr::Literal(Literal::Boolean(value)) => {
            ops.push(DspOp::Push(DspValue::Const(if *value { 1.0 } else { 0.0 })));
            true
        }
        Expr::Variable(name) if name == "t" => {
            ops.push(DspOp::Push(DspValue::T));
            true
        }
        Expr::Variable(name) if name == "freq" => {
            ops.push(DspOp::Push(DspValue::Freq));
            true
        }
        Expr::Variable(name) if name == "gate" => {
            ops.push(DspOp::Push(DspValue::Gate));
            true
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if !compile_expr(body, *lhs, ops) || !compile_expr(body, *rhs, ops) {
                return false;
            }
            match op {
                BinaryOp::Add => ops.push(DspOp::Add),
                BinaryOp::Sub => ops.push(DspOp::Sub),
                BinaryOp::Mul => ops.push(DspOp::Mul),
                BinaryOp::Div => ops.push(DspOp::Div),
                _ => return false,
            }
            true
        }
        Expr::Call { callee, args, .. }
            if is_named_call(body, *callee, "sin") && args.len() == 1 =>
        {
            let Some(arg_expr) = positional_arg_expr(&args[0]) else {
                return false;
            };
            if compile_expr(body, arg_expr, ops) {
                ops.push(DspOp::Sin);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn is_gate_expr(body: &Body, expr: crate::hir::Idx<Expr>) -> bool {
    matches!(&body.exprs[expr], Expr::Variable(name) if name == "gate")
}

fn is_named_call(body: &Body, expr: crate::hir::Idx<Expr>, name: &str) -> bool {
    matches!(&body.exprs[expr], Expr::Variable(callee) if callee == name)
}

fn positional_arg_expr(arg: &Arg) -> Option<crate::hir::Idx<Expr>> {
    match arg {
        Arg::Positional { value, .. } => Some(*value),
        Arg::Named { .. } => None,
    }
}

fn dsp_program_from_vec(ops: Vec<DspOp>) -> DspProgram {
    let mut program_ops = [DspOp::Nop; wrela_runtime::audio::voice::DSP_PROGRAM_MAX_OPS];
    let len = ops
        .len()
        .min(wrela_runtime::audio::voice::DSP_PROGRAM_MAX_OPS);
    program_ops[..len].copy_from_slice(&ops[..len]);
    DspProgram {
        ops: program_ops,
        len: len as u8,
    }
}
