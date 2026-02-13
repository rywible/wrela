pub fn latest_visible_version(versions: &[u64], read_version: u64) -> Option<u64> {
    versions
        .iter()
        .copied()
        .filter(|v| *v <= read_version)
        .max()
}
