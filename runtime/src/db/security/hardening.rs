#[derive(Debug, Clone, PartialEq)]
pub struct SecurityConfig {
    pub require_mtls: bool,
    pub enforce_authz: bool,
    pub enable_audit_log: bool,
    pub allow_insecure_fallback: bool,
    pub cert_rotation_window_s: u64,
    pub key_rotation_interval_s: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateRpcMtlsMode {
    Auto,
    Off,
    On,
}

impl PrivateRpcMtlsMode {
    fn parse(raw: Option<&str>) -> Result<Self, PrivateRpcSecurityPolicyError> {
        let normalized = raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("auto");
        match normalized {
            "auto" => Ok(Self::Auto),
            "off" => Ok(Self::Off),
            "on" => Ok(Self::On),
            other => Err(PrivateRpcSecurityPolicyError::InvalidMtlsMode {
                value: other.to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Off => "off",
            Self::On => "on",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateRpcSecurityPolicy {
    pub configured_mode: PrivateRpcMtlsMode,
    pub effective_mtls_enabled: bool,
    pub trusted_network: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateRpcSecurityPolicyError {
    InvalidMtlsMode { value: String },
    MtlsTransportUnavailable,
}

impl std::fmt::Display for PrivateRpcSecurityPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMtlsMode { value } => write!(
                f,
                "invalid WRELADB_PRIVATE_RPC_MTLS_MODE `{value}` (expected auto|off|on)"
            ),
            Self::MtlsTransportUnavailable => write!(
                f,
                "WRELADB_PRIVATE_RPC_MTLS_MODE=on requires TLS-capable private RPC transport"
            ),
        }
    }
}

impl std::error::Error for PrivateRpcSecurityPolicyError {}

fn normalize_trusted_network(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

pub fn resolve_private_rpc_security_policy(
    mtls_mode_raw: Option<&str>,
    trusted_network_raw: Option<&str>,
    is_fly_runtime: bool,
    tls_transport_available: bool,
) -> Result<PrivateRpcSecurityPolicy, PrivateRpcSecurityPolicyError> {
    let configured_mode = PrivateRpcMtlsMode::parse(mtls_mode_raw)?;
    let trusted_network = normalize_trusted_network(trusted_network_raw);
    let fly_wireguard = trusted_network.as_deref() == Some("fly-wireguard");

    let effective_mtls_enabled = match configured_mode {
        PrivateRpcMtlsMode::Auto => {
            if is_fly_runtime && fly_wireguard {
                false
            } else {
                tls_transport_available
            }
        }
        PrivateRpcMtlsMode::Off => false,
        PrivateRpcMtlsMode::On => {
            if !tls_transport_available {
                return Err(PrivateRpcSecurityPolicyError::MtlsTransportUnavailable);
            }
            true
        }
    };

    Ok(PrivateRpcSecurityPolicy {
        configured_mode,
        effective_mtls_enabled,
        trusted_network,
    })
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
        PrivateRpcMtlsMode, PrivateRpcSecurityPolicyError, SecurityConfig, SecurityConfigError,
        cert_rotation_due, key_rotation_due, resolve_private_rpc_security_policy,
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

    #[test]
    fn private_rpc_mtls_auto_turns_off_on_fly_wireguard() {
        let policy =
            resolve_private_rpc_security_policy(Some("auto"), Some("fly-wireguard"), true, false)
                .expect("policy");
        assert_eq!(policy.configured_mode, PrivateRpcMtlsMode::Auto);
        assert!(!policy.effective_mtls_enabled);
    }

    #[test]
    fn private_rpc_mtls_on_without_tls_transport_fails_closed() {
        let err = resolve_private_rpc_security_policy(Some("on"), None, true, false)
            .expect_err("mtls on requires transport");
        assert_eq!(err, PrivateRpcSecurityPolicyError::MtlsTransportUnavailable);
    }

    #[test]
    fn private_rpc_mtls_mode_rejects_invalid_value() {
        let err = resolve_private_rpc_security_policy(Some("bogus"), None, true, false)
            .expect_err("invalid mode");
        assert_eq!(
            err,
            PrivateRpcSecurityPolicyError::InvalidMtlsMode {
                value: "bogus".to_string()
            }
        );
    }
}
