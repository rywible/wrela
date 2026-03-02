use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipEvent {
    pub frame: u32,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub frame_count: u32,
    pub events: Vec<ClipEvent>,
    pub root_track: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipError {
    EventOutOfRange {
        index: usize,
        frame: u32,
        frame_count: u32,
    },
    EventTrackUnsorted {
        index: usize,
    },
    EmptyEventTag {
        index: usize,
    },
    DuplicateEvent {
        frame: u32,
        tag: String,
    },
}

pub fn validate_event_track(clip: &Clip) -> Result<(), ClipError> {
    let mut seen = BTreeSet::new();
    let mut last_frame = 0_u32;
    for (index, event) in clip.events.iter().enumerate() {
        if event.frame >= clip.frame_count {
            return Err(ClipError::EventOutOfRange {
                index,
                frame: event.frame,
                frame_count: clip.frame_count,
            });
        }
        if index > 0 && event.frame < last_frame {
            return Err(ClipError::EventTrackUnsorted { index });
        }
        if event.tag.trim().is_empty() {
            return Err(ClipError::EmptyEventTag { index });
        }
        if !seen.insert((event.frame, event.tag.clone())) {
            return Err(ClipError::DuplicateEvent {
                frame: event.frame,
                tag: event.tag.clone(),
            });
        }
        last_frame = event.frame;
    }
    Ok(())
}

pub fn clip_hash(clip: &Clip) -> String {
    let mut hasher = Sha256::new();
    hasher.update(clip.frame_count.to_le_bytes());

    hasher.update((clip.events.len() as u64).to_le_bytes());
    for event in &clip.events {
        hasher.update(event.frame.to_le_bytes());
        hasher.update(event.tag.as_bytes());
        hasher.update([0x00]);
    }

    hasher.update((clip.root_track.len() as u64).to_le_bytes());
    for sample in &clip.root_track {
        hasher.update(sample[0].to_bits().to_le_bytes());
        hasher.update(sample[1].to_bits().to_le_bytes());
        hasher.update(sample[2].to_bits().to_le_bytes());
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Clip, ClipError, ClipEvent, clip_hash, validate_event_track};

    fn sample_clip() -> Clip {
        Clip {
            frame_count: 48,
            events: vec![
                ClipEvent {
                    frame: 4,
                    tag: "windup".to_owned(),
                },
                ClipEvent {
                    frame: 16,
                    tag: "hit".to_owned(),
                },
                ClipEvent {
                    frame: 33,
                    tag: "recover".to_owned(),
                },
            ],
            root_track: vec![[0.0, 0.0, 0.0], [0.02, 0.0, 0.0], [0.04, 0.0, 0.0]],
        }
    }

    #[test]
    fn event_track_integrity() {
        let clip = sample_clip();
        assert!(validate_event_track(&clip).is_ok());

        let unsorted = Clip {
            events: vec![
                ClipEvent {
                    frame: 20,
                    tag: "hit".to_owned(),
                },
                ClipEvent {
                    frame: 10,
                    tag: "windup".to_owned(),
                },
            ],
            ..sample_clip()
        };

        let result = validate_event_track(&unsorted);
        assert!(matches!(result, Err(ClipError::EventTrackUnsorted { .. })));
    }

    #[test]
    fn clip_hash_stability() {
        let clip = sample_clip();
        let hash_a = clip_hash(&clip);
        let hash_b = clip_hash(&clip);
        assert_eq!(hash_a, hash_b);

        let mut changed = sample_clip();
        changed.root_track[2][0] = 0.05;
        let hash_c = clip_hash(&changed);
        assert_ne!(hash_a, hash_c);
    }
}
