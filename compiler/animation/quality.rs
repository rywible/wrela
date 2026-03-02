#[derive(Debug, Clone, PartialEq)]
pub struct QualityMetrics {
    pub foot_lock_error: f32,
    pub root_drift: f32,
    pub clip_event_violations: u32,
    pub mesh_variance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityThresholds {
    pub max_foot_lock_error: f32,
    pub max_root_drift: f32,
    pub max_clip_event_violations: u32,
    pub max_mesh_variance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityFailure {
    pub metric: &'static str,
    pub observed: f32,
    pub allowed: f32,
}

pub fn evaluate(
    metrics: &QualityMetrics,
    thresholds: &QualityThresholds,
) -> Result<(), Vec<QualityFailure>> {
    let mut failures = Vec::new();

    if metrics.foot_lock_error > thresholds.max_foot_lock_error {
        failures.push(QualityFailure {
            metric: "foot_lock_error",
            observed: metrics.foot_lock_error,
            allowed: thresholds.max_foot_lock_error,
        });
    }
    if metrics.root_drift > thresholds.max_root_drift {
        failures.push(QualityFailure {
            metric: "root_drift",
            observed: metrics.root_drift,
            allowed: thresholds.max_root_drift,
        });
    }
    if metrics.clip_event_violations > thresholds.max_clip_event_violations {
        failures.push(QualityFailure {
            metric: "clip_event_violations",
            observed: metrics.clip_event_violations as f32,
            allowed: thresholds.max_clip_event_violations as f32,
        });
    }
    if metrics.mesh_variance > thresholds.max_mesh_variance {
        failures.push(QualityFailure {
            metric: "mesh_variance",
            observed: metrics.mesh_variance,
            allowed: thresholds.max_mesh_variance,
        });
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::{QualityMetrics, QualityThresholds, evaluate};

    #[test]
    fn fails_on_threshold_breach() {
        let metrics = QualityMetrics {
            foot_lock_error: 0.012,
            root_drift: 0.03,
            clip_event_violations: 1,
            mesh_variance: 0.009,
        };
        let thresholds = QualityThresholds {
            max_foot_lock_error: 0.01,
            max_root_drift: 0.02,
            max_clip_event_violations: 0,
            max_mesh_variance: 0.01,
        };

        let failures = evaluate(&metrics, &thresholds).expect_err("expected threshold failures");
        assert!(
            failures
                .iter()
                .any(|failure| failure.metric == "foot_lock_error")
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.metric == "root_drift")
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.metric == "clip_event_violations")
        );
    }
}
