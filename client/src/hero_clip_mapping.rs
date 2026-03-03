#![cfg_attr(not(test), allow(dead_code))]

use std::fmt;

const IDLE_KEYWORD_PRIORITIES: &[&str] = &["idle", "stand"];
const WALK_KEYWORD_PRIORITIES: &[&str] = &["walk", "locomotion"];
const RUN_KEYWORD_PRIORITIES: &[&str] = &["run", "sprint", "jog"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroRuntimeClipMapping {
    pub idle_clip_index: usize,
    pub walk_clip_index: usize,
    pub run_clip_index: Option<usize>,
}

impl HeroRuntimeClipMapping {
    pub fn run_clip_index_or_walk(&self) -> usize {
        self.run_clip_index.unwrap_or(self.walk_clip_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeroRuntimeClipMappingError {
    MissingWalkClip { available_clip_names: Vec<String> },
}

impl fmt::Display for HeroRuntimeClipMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWalkClip {
                available_clip_names,
            } => write!(
                f,
                "missing required hero walk clip. Add or rename a clip to include one of {WALK_KEYWORD_PRIORITIES:?} (or one of {RUN_KEYWORD_PRIORITIES:?} for fallback). Single-clip GLBs are auto-mapped. Available clips: {}",
                format_available_clip_names(available_clip_names.as_slice())
            ),
        }
    }
}

impl std::error::Error for HeroRuntimeClipMappingError {}

pub fn find_clip_index_by_name_priority<T: AsRef<str>>(
    clip_names: &[T],
    keyword_priorities: &[&str],
) -> Option<usize> {
    let normalized_keywords: Vec<String> = keyword_priorities
        .iter()
        .map(|keyword| keyword.trim().to_ascii_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    if normalized_keywords.is_empty() {
        return None;
    }

    let normalized_clip_names: Vec<String> = clip_names
        .iter()
        .map(|clip_name| clip_name.as_ref().trim().to_ascii_lowercase())
        .collect();

    for keyword in &normalized_keywords {
        if let Some(index) = normalized_clip_names
            .iter()
            .position(|name| name == keyword)
        {
            return Some(index);
        }
    }

    for keyword in &normalized_keywords {
        if let Some(index) = normalized_clip_names
            .iter()
            .position(|name| name.contains(keyword))
        {
            return Some(index);
        }
    }

    None
}

pub fn resolve_hero_runtime_clip_mapping<T: AsRef<str>>(
    clip_names: &[T],
) -> Result<HeroRuntimeClipMapping, HeroRuntimeClipMappingError> {
    let available_clip_names: Vec<String> = clip_names
        .iter()
        .map(|clip_name| clip_name.as_ref().to_string())
        .collect();

    let walk_clip_index = find_clip_index_by_name_priority(clip_names, WALK_KEYWORD_PRIORITIES)
        .or_else(|| find_clip_index_by_name_priority(clip_names, RUN_KEYWORD_PRIORITIES))
        .or_else(|| (clip_names.len() == 1).then_some(0))
        .ok_or_else(|| HeroRuntimeClipMappingError::MissingWalkClip {
            available_clip_names: available_clip_names.clone(),
        })?;

    let idle_clip_index = find_clip_index_by_name_priority(clip_names, IDLE_KEYWORD_PRIORITIES)
        .unwrap_or(walk_clip_index);

    let run_clip_index = find_clip_index_by_name_priority(clip_names, RUN_KEYWORD_PRIORITIES);

    Ok(HeroRuntimeClipMapping {
        idle_clip_index,
        walk_clip_index,
        run_clip_index,
    })
}

fn format_available_clip_names(clip_names: &[String]) -> String {
    if clip_names.is_empty() {
        return "<none>".to_string();
    }
    clip_names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        HeroRuntimeClipMappingError, find_clip_index_by_name_priority,
        resolve_hero_runtime_clip_mapping,
    };

    #[test]
    fn find_clip_index_prefers_exact_match_over_partial_match() {
        let clip_names = vec!["hero_walk_cycle", "walk", "run"];
        let found = find_clip_index_by_name_priority(&clip_names, &["walk"]);
        assert_eq!(found, Some(1), "exact walk match should beat partial");
    }

    #[test]
    fn find_clip_index_supports_partial_name_matching() {
        let clip_names = vec!["hero_idle_loop", "hero_walk_forward"];
        let found = find_clip_index_by_name_priority(&clip_names, &["walk"]);
        assert_eq!(found, Some(1), "partial walk match should resolve");
    }

    #[test]
    fn find_clip_index_is_case_insensitive() {
        let clip_names = vec!["IDLE_BASE", "Walk_Cycle", "SPRINT_RUN_FAST"];
        assert_eq!(
            find_clip_index_by_name_priority(&clip_names, &["idle"]),
            Some(0)
        );
        assert_eq!(
            find_clip_index_by_name_priority(&clip_names, &["walk"]),
            Some(1)
        );
        assert_eq!(
            find_clip_index_by_name_priority(&clip_names, &["run"]),
            Some(2)
        );
    }

    #[test]
    fn find_clip_index_uses_keyword_priority_order() {
        let clip_names = vec!["walk", "run", "idle"];
        let found = find_clip_index_by_name_priority(&clip_names, &["idle", "walk", "run"]);
        assert_eq!(
            found,
            Some(2),
            "first keyword in priority list should win even if later in clip list"
        );
    }

    #[test]
    fn resolve_mapping_falls_back_to_walk_when_run_clip_is_missing() {
        let clip_names = vec!["Idle_Base", "Walk_Cycle"];
        let mapping = resolve_hero_runtime_clip_mapping(&clip_names).expect("mapping should build");

        assert_eq!(mapping.idle_clip_index, 0);
        assert_eq!(mapping.walk_clip_index, 1);
        assert_eq!(mapping.run_clip_index, None);
        assert_eq!(mapping.run_clip_index_or_walk(), 1);
    }

    #[test]
    fn resolve_mapping_reports_missing_walk_clip_with_actionable_error() {
        let clip_names = vec!["idle", "attack_heavy"];
        let error =
            resolve_hero_runtime_clip_mapping(&clip_names).expect_err("walk must be required");

        match &error {
            HeroRuntimeClipMappingError::MissingWalkClip {
                available_clip_names,
            } => {
                assert_eq!(
                    available_clip_names,
                    &vec!["idle".to_string(), "attack_heavy".to_string()]
                );
            }
        }

        let message = error.to_string();
        assert!(
            message.contains("Add or rename a clip"),
            "error should be actionable: {message}"
        );
        assert!(
            message.contains("walk"),
            "error should mention missing walk keyword: {message}"
        );
    }

    #[test]
    fn resolve_mapping_accepts_single_clip_without_keywords() {
        let clip_names = vec!["NlaTrack"];
        let mapping = resolve_hero_runtime_clip_mapping(&clip_names).expect("single clip fallback");
        assert_eq!(mapping.idle_clip_index, 0);
        assert_eq!(mapping.walk_clip_index, 0);
        assert_eq!(mapping.run_clip_index_or_walk(), 0);
    }
}
