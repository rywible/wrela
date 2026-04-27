//! Audio contracts and media samples (RFC 0011 Phase 68).

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioContractId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioFamilyId {
    Voice,
    MediaSample,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaSample {
    pub occlusion_db: f32,
    pub reverb_send: f32,
    pub lowpass_hz: f32,
}

impl Default for MediaSample {
    fn default() -> Self {
        Self {
            occlusion_db: 0.0,
            reverb_send: 0.0,
            lowpass_hz: 20_000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceId(pub u64);
