#[derive(Debug, Clone, PartialEq)]
pub enum WarpError {
    SampleCountMismatch { expected: usize, found: usize },
    DriftBoundExceeded { drift: f32, bound: f32 },
}

pub fn max_root_drift(reference: &[[f32; 3]], warped: &[[f32; 3]]) -> Result<f32, WarpError> {
    if reference.len() != warped.len() {
        return Err(WarpError::SampleCountMismatch {
            expected: reference.len(),
            found: warped.len(),
        });
    }

    let mut max_drift = 0.0_f32;
    for (base, warped_sample) in reference.iter().zip(warped.iter()) {
        let dx = base[0] - warped_sample[0];
        let dy = base[1] - warped_sample[1];
        let dz = base[2] - warped_sample[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if dist > max_drift {
            max_drift = dist;
        }
    }

    Ok(max_drift)
}

pub fn validate_root_drift_bound(
    reference: &[[f32; 3]],
    warped: &[[f32; 3]],
    bound: f32,
) -> Result<f32, WarpError> {
    let drift = max_root_drift(reference, warped)?;
    if drift > bound {
        return Err(WarpError::DriftBoundExceeded { drift, bound });
    }
    Ok(drift)
}

#[cfg(test)]
mod tests {
    use super::{WarpError, validate_root_drift_bound};

    #[test]
    fn root_drift_bounds() {
        let reference = [[0.0, 0.0, 0.0], [0.2, 0.0, 0.0], [0.4, 0.0, 0.0]];
        let warped_good = [[0.0, 0.0, 0.0], [0.201, 0.0, 0.0], [0.398, 0.0, 0.0]];
        let ok = validate_root_drift_bound(&reference, &warped_good, 0.01)
            .expect("drift should remain in bounds");
        assert!(ok <= 0.01, "expected drift <= 0.01, got {ok}");

        let warped_bad = [[0.0, 0.0, 0.0], [0.25, 0.0, 0.0], [0.46, 0.0, 0.0]];
        let err = validate_root_drift_bound(&reference, &warped_bad, 0.01);
        assert!(matches!(err, Err(WarpError::DriftBoundExceeded { .. })));
    }
}
