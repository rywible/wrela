//! Audio snapshot publisher (RFC 0011 Phase 68).
//!
//! The compiler is responsible for projecting the engine-side `AudioDspPlan`
//! into the runtime-owned, lock-free voice ledger. The compiler crate
//! intentionally does NOT render samples or own the [`SampleRing`]; the audio
//! worker in the runtime crate consumes the published [`VoiceLedgerSnapshot`]
//! and renders samples on the audio thread.

#![forbid(unsafe_code)]

pub mod rt_check;

use crate::audio_contract::VoiceId;
use crate::audio_plan::{AudioConfig, AudioDspPlan, AudioVoicePlan};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use wrela_runtime::audio::voice::{VoiceLedger, VoiceLedgerSnapshot, VoiceState};

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
    underrun_counter: Arc<AtomicU64>,
    last_seen_underruns: AtomicU64,
    media_query_cursor: AtomicU64,
}

impl AudioSnapshotPublisher {
    pub fn new(config: AudioConfig, ledger: Arc<VoiceLedger>) -> Self {
        Self {
            config,
            ledger,
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
            underrun_counter,
            last_seen_underruns: AtomicU64::new(0),
            media_query_cursor: AtomicU64::new(0),
        }
    }

    pub fn publish(&self, tick: u64, plan: &AudioDspPlan) -> AudioFrameReport {
        let mut voices = plan.voices.clone();
        voices.sort_by_key(|voice| (std::cmp::Reverse(voice.priority), voice.id.0));
        let stolen = voices.len().saturating_sub(self.config.max_voices);
        voices.truncate(self.config.max_voices);
        let media_queried_voice_ids = self.media_queries_for_frame(&voices);
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
        if media_queries > self.config.max_full_rate_media_queries {
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
        let len = voices.len();
        let start = (self.media_query_cursor.load(Ordering::Relaxed) as usize) % len;
        let count = cap.min(len);
        let mut ids = Vec::with_capacity(count);
        for offset in 0..count {
            ids.push(voices[(start + offset) % len].id.0);
        }
        let next = ((start + count) % len) as u64;
        self.media_query_cursor.store(next, Ordering::Release);
        ids
    }
}

fn voice_plan_to_state(plan: &AudioVoicePlan) -> VoiceState {
    VoiceState {
        id: plan.id.0,
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

pub fn sine_voice(id: u64, priority: i32, gain: f32) -> AudioVoicePlan {
    AudioVoicePlan {
        id: VoiceId(id),
        position: [0.0, 0.0, 0.0],
        velocity: [0.0, 0.0, 0.0],
        gain,
        priority,
        media: Default::default(),
        gate: true,
    }
}
