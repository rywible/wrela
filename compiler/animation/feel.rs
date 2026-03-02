#[derive(Debug, Clone, PartialEq)]
pub enum FeelError {
    ReadabilityBelowThreshold { score: f32, threshold: f32 },
}

pub fn readability_score(
    key_pose_spacing: f32,
    silhouette_variety: f32,
    timing_clarity: f32,
) -> f32 {
    let weighted =
        (key_pose_spacing * 0.40) + (silhouette_variety * 0.35) + (timing_clarity * 0.25);
    weighted.clamp(0.0, 1.0)
}

pub fn ensure_readability_threshold(score: f32, threshold: f32) -> Result<(), FeelError> {
    if score < threshold {
        return Err(FeelError::ReadabilityBelowThreshold { score, threshold });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_readability_threshold, readability_score};

    #[test]
    fn readability_score_threshold() {
        let score = readability_score(0.88, 0.81, 0.75);
        assert!(
            ensure_readability_threshold(score, 0.80).is_ok(),
            "expected readability score to clear threshold, score={score}"
        );
    }
}
