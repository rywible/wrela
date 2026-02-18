#[derive(Debug, Clone, PartialEq)]
pub struct SecurityConfig {
    pub require_mtls: bool,
    pub enforce_authz: bool,
    pub enable_audit_log: bool,
    pub allow_insecure_fallback: bool,
    pub cert_rotation_window_s: u64,
    pub key_rotation_interval_s: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityConfigError {
    InsecureFallbackEnabled,
    MissingMutualTls,
    AuthzNotEnforced,
    AuditLogDisabled,
    InvalidCertRotationWindow,
    InvalidKeyRotationInterval,
}

pub fn validate_security_config(config: &SecurityConfig) -> Result<(), SecurityConfigError> {
    if config.allow_insecure_fallback {
        return Err(SecurityConfigError::InsecureFallbackEnabled);
    }
    if !config.require_mtls {
        return Err(SecurityConfigError::MissingMutualTls);
    }
    if !config.enforce_authz {
        return Err(SecurityConfigError::AuthzNotEnforced);
    }
    if !config.enable_audit_log {
        return Err(SecurityConfigError::AuditLogDisabled);
    }
    if config.cert_rotation_window_s == 0 {
        return Err(SecurityConfigError::InvalidCertRotationWindow);
    }
    if config.key_rotation_interval_s == 0 {
        return Err(SecurityConfigError::InvalidKeyRotationInterval);
    }
    Ok(())
}

pub fn cert_rotation_due(not_after_epoch_s: u64, now_epoch_s: u64, rotate_window_s: u64) -> bool {
    if rotate_window_s == 0 {
        return true;
    }
    now_epoch_s.saturating_add(rotate_window_s) >= not_after_epoch_s
}

pub fn key_rotation_due(last_rotation_epoch_s: u64, now_epoch_s: u64, interval_s: u64) -> bool {
    if interval_s == 0 {
        return true;
    }
    now_epoch_s.saturating_sub(last_rotation_epoch_s) >= interval_s
}

#[cfg(test)]
mod tests {
    use super::{
        SecurityConfig, SecurityConfigError, cert_rotation_due, key_rotation_due,
        validate_security_config,
    };

    fn strict_config() -> SecurityConfig {
        SecurityConfig {
            require_mtls: true,
            enforce_authz: true,
            enable_audit_log: true,
            allow_insecure_fallback: false,
            cert_rotation_window_s: 3_600,
            key_rotation_interval_s: 86_400,
        }
    }

    #[test]
    fn strict_security_config_is_valid() {
        assert_eq!(validate_security_config(&strict_config()), Ok(()));
    }

    #[test]
    fn insecure_fallback_is_rejected() {
        let mut cfg = strict_config();
        cfg.allow_insecure_fallback = true;
        assert_eq!(
            validate_security_config(&cfg),
            Err(SecurityConfigError::InsecureFallbackEnabled)
        );
    }

    #[test]
    fn missing_mtls_or_authz_fails_closed() {
        let mut cfg = strict_config();
        cfg.require_mtls = false;
        assert_eq!(
            validate_security_config(&cfg),
            Err(SecurityConfigError::MissingMutualTls)
        );

        let mut cfg = strict_config();
        cfg.enforce_authz = false;
        assert_eq!(
            validate_security_config(&cfg),
            Err(SecurityConfigError::AuthzNotEnforced)
        );
    }

    #[test]
    fn rotation_deadlines_are_deterministic() {
        assert!(cert_rotation_due(10_000, 9_500, 600));
        assert!(!cert_rotation_due(10_000, 8_000, 600));

        assert!(key_rotation_due(1_000, 2_000, 1_000));
        assert!(!key_rotation_due(1_000, 1_500, 1_000));
    }
}
