#[derive(Debug, Clone, PartialEq)]
pub enum IkError {
    SampleCountMismatch { expected: usize, found: usize },
    ThresholdBreach { error: f32, threshold: f32 },
}

pub fn foot_lock_error(reference: &[[f32; 3]], solved: &[[f32; 3]]) -> Result<f32, IkError> {
    if reference.len() != solved.len() {
        return Err(IkError::SampleCountMismatch {
            expected: reference.len(),
            found: solved.len(),
        });
    }
    if reference.is_empty() {
        return Ok(0.0);
    }

    let mut accumulated = 0.0_f32;
    for (expected, actual) in reference.iter().zip(solved.iter()) {
        let dx = expected[0] - actual[0];
        let dy = expected[1] - actual[1];
        let dz = expected[2] - actual[2];
        accumulated += (dx * dx + dy * dy + dz * dz).sqrt();
    }

    Ok(accumulated / reference.len() as f32)
}

pub fn validate_foot_lock(
    reference: &[[f32; 3]],
    solved: &[[f32; 3]],
    threshold: f32,
) -> Result<f32, IkError> {
    let error = foot_lock_error(reference, solved)?;
    if error > threshold {
        return Err(IkError::ThresholdBreach { error, threshold });
    }
    Ok(error)
}

#[cfg(test)]
mod tests {
    use super::validate_foot_lock;

    #[test]
    fn foot_lock_error_below_threshold() {
        let expected = [[0.0, 0.0, 0.0], [0.05, 0.0, 0.0], [0.10, 0.0, 0.0]];
        let solved = [[0.0, 0.0, 0.0], [0.051, 0.0, 0.0], [0.098, 0.0, 0.0]];

        let result = validate_foot_lock(&expected, &solved, 0.01)
            .expect("expected error to stay below threshold");
        assert!(result <= 0.01, "expected <= 0.01, got {result}");
    }
}
