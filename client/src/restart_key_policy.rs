#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn is_restart_key(key: &str) -> bool {
    matches!(key, "r" | "R" | "Enter")
}

#[cfg(test)]
mod tests {
    use super::is_restart_key;

    #[test]
    fn accepts_restart_keys() {
        for key in ["r", "R", "Enter"] {
            assert!(is_restart_key(key), "expected '{key}' to be accepted");
        }
    }

    #[test]
    fn rejects_non_restart_keys_including_space() {
        for key in ["", " ", "Space", "space", "Spacebar", "a", "Escape"] {
            assert!(!is_restart_key(key), "expected '{key}' to be rejected");
        }
    }
}
