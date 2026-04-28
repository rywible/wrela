//! Audio DSP / voice plan (RFC 0011 Phase 68).

use crate::audio_contract::{MediaSample, VoiceId};
use smol_str::SmolStr;
pub use wrela_runtime::audio::voice::{DspOp, DspProgram, DspProgramError, DspValue};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub block_size: u32,
    pub max_voices: usize,
    pub max_full_rate_media_queries: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            block_size: 256,
            max_voices: 64,
            max_full_rate_media_queries: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioVoicePlan {
    pub id: VoiceId,
    pub source_audio_field: Option<SmolStr>,
    pub source_audio_signature: u64,
    pub source_program: DspProgram,
    pub source_frequency_hz: f32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub gain: f32,
    pub priority: i32,
    pub media: MediaSample,
    pub gate: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioDspPlan {
    pub voices: Vec<AudioVoicePlan>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioSpatialPlan {
    pub listener_position: [f32; 3],
}
