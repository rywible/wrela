pub fn latest_visible_version(versions: &[u64], read_version: u64) -> Option<u64> {
    let idx = versions.partition_point(|version| *version <= read_version);
    idx.checked_sub(1)
        .and_then(|index| versions.get(index))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_versions_returns_none() {
        assert_eq!(latest_visible_version(&[], 10), None);
    }

    #[test]
    fn read_before_earliest_returns_none() {
        assert_eq!(latest_visible_version(&[5, 10, 15], 3), None);
    }

    #[test]
    fn exact_version_match() {
        assert_eq!(latest_visible_version(&[5, 10, 15], 10), Some(10));
    }

    #[test]
    fn between_versions_returns_earlier() {
        assert_eq!(latest_visible_version(&[5, 10, 15], 12), Some(10));
    }

    #[test]
    fn read_after_all_versions_returns_latest() {
        assert_eq!(latest_visible_version(&[5, 10, 15], 100), Some(15));
    }

    #[test]
    fn single_version_at_boundary() {
        assert_eq!(latest_visible_version(&[1], 1), Some(1));
        assert_eq!(latest_visible_version(&[1], 0), None);
    }

    #[test]
    fn monotonic_versions_many() {
        let versions: Vec<u64> = (1..=1000).collect();
        for rv in [1, 500, 999, 1000, 1001] {
            let result = latest_visible_version(&versions, rv);
            if rv >= 1 && rv <= 1000 {
                assert_eq!(result, Some(rv));
            } else if rv > 1000 {
                assert_eq!(result, Some(1000));
            }
        }
    }
}
