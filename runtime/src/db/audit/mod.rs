#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventKind {
    AuthzDenied,
    CertIssued,
    CertRevoked,
    KeyRotation,
    ResidencyDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub kind: AuditEventKind,
    pub actor: String,
    pub resource: String,
    pub detail: String,
    pub ts_epoch_s: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditLog {
    events: Vec<AuditEvent>,
}

impl AuditLog {
    pub fn append(&mut self, event: AuditEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    pub fn by_kind(&self, kind: AuditEventKind) -> Vec<AuditEvent> {
        self.events
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }

    pub fn redact_for_export(&self) -> Vec<AuditEvent> {
        self.events
            .iter()
            .map(|event| AuditEvent {
                kind: event.kind,
                actor: event.actor.clone(),
                resource: event.resource.clone(),
                detail: redact_detail(&event.detail),
                ts_epoch_s: event.ts_epoch_s,
            })
            .collect()
    }

    pub fn record_authz_denied(
        &mut self,
        actor: impl Into<String>,
        resource: impl Into<String>,
        detail: impl Into<String>,
        ts_epoch_s: u64,
    ) {
        self.append(AuditEvent {
            kind: AuditEventKind::AuthzDenied,
            actor: actor.into(),
            resource: resource.into(),
            detail: detail.into(),
            ts_epoch_s,
        });
    }

    pub fn record_cert_issued(
        &mut self,
        actor: impl Into<String>,
        resource: impl Into<String>,
        detail: impl Into<String>,
        ts_epoch_s: u64,
    ) {
        self.append(AuditEvent {
            kind: AuditEventKind::CertIssued,
            actor: actor.into(),
            resource: resource.into(),
            detail: detail.into(),
            ts_epoch_s,
        });
    }

    pub fn record_cert_revoked(
        &mut self,
        actor: impl Into<String>,
        resource: impl Into<String>,
        detail: impl Into<String>,
        ts_epoch_s: u64,
    ) {
        self.append(AuditEvent {
            kind: AuditEventKind::CertRevoked,
            actor: actor.into(),
            resource: resource.into(),
            detail: detail.into(),
            ts_epoch_s,
        });
    }

    pub fn record_key_rotation(
        &mut self,
        actor: impl Into<String>,
        resource: impl Into<String>,
        detail: impl Into<String>,
        ts_epoch_s: u64,
    ) {
        self.append(AuditEvent {
            kind: AuditEventKind::KeyRotation,
            actor: actor.into(),
            resource: resource.into(),
            detail: detail.into(),
            ts_epoch_s,
        });
    }

    pub fn record_residency_denied(
        &mut self,
        actor: impl Into<String>,
        resource: impl Into<String>,
        detail: impl Into<String>,
        ts_epoch_s: u64,
    ) {
        self.append(AuditEvent {
            kind: AuditEventKind::ResidencyDenied,
            actor: actor.into(),
            resource: resource.into(),
            detail: detail.into(),
            ts_epoch_s,
        });
    }
}

fn redact_detail(detail: &str) -> String {
    if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(detail) {
        redact_json_value(&mut json, None);
        return json.to_string();
    }
    redact_text_detail(detail)
}

fn redact_text_detail(detail: &str) -> String {
    let mut redactions_after_auth_header = 0usize;
    let mut out = Vec::new();
    for token in detail.split_whitespace() {
        if redactions_after_auth_header > 0 {
            out.push("[REDACTED]".to_string());
            redactions_after_auth_header = redactions_after_auth_header.saturating_sub(1);
            continue;
        }

        let normalized = token
            .trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == ',' || ch == ';')
            .to_ascii_lowercase();
        let is_sensitive = normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("password")
            || normalized.starts_with("authorization")
            || normalized.contains("apikey")
            || normalized.contains("api_key")
            || normalized.contains("key=");
        if is_sensitive {
            if normalized.starts_with("authorization") {
                redactions_after_auth_header = 2;
            }
            out.push("[REDACTED]".to_string());
        } else {
            out.push(token.to_string());
        }
    }
    out.join(" ")
}

fn redact_json_value(value: &mut serde_json::Value, parent_key: Option<&str>) {
    if parent_key.is_some_and(is_sensitive_key) {
        *value = serde_json::Value::String("[REDACTED]".to_string());
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                redact_json_value(nested, Some(key));
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                redact_json_value(nested, None);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    normalized.contains("secret")
        || normalized.contains("token")
        || normalized.contains("password")
        || normalized == "authorization"
        || normalized == "api_key"
        || normalized == "apikey"
}

#[cfg(test)]
mod tests {
    use super::{AuditEvent, AuditEventKind, AuditLog};

    #[test]
    fn appends_filters_and_redacts_security_events() {
        let mut log = AuditLog::default();
        log.append(AuditEvent {
            kind: AuditEventKind::AuthzDenied,
            actor: "gateway-1".to_string(),
            resource: "db.write".to_string(),
            detail: "token=abc secret=raw".to_string(),
            ts_epoch_s: 100,
        });
        log.append(AuditEvent {
            kind: AuditEventKind::CertRevoked,
            actor: "admin".to_string(),
            resource: "cert:42".to_string(),
            detail: "reason compromise".to_string(),
            ts_epoch_s: 101,
        });

        let denied = log.by_kind(AuditEventKind::AuthzDenied);
        assert_eq!(denied.len(), 1);

        let redacted = log.redact_for_export();
        assert!(redacted[0].detail.contains("[REDACTED]"));
    }

    #[test]
    fn helper_methods_emit_expected_kinds() {
        let mut log = AuditLog::default();
        log.record_authz_denied("gw", "rpc:write", "TOKEN=abc", 1);
        log.record_cert_issued("ca", "cert:1", "ttl=3600", 2);
        log.record_cert_revoked("ca", "cert:1", "reason=rotate", 3);
        log.record_key_rotation("kms", "key:db", "new_version=2", 4);
        log.record_residency_denied("policy", "shard:core", "sink_region=eu", 5);

        assert_eq!(log.events().len(), 5);
        assert_eq!(log.by_kind(AuditEventKind::CertIssued).len(), 1);
        assert_eq!(log.by_kind(AuditEventKind::KeyRotation).len(), 1);

        let exported = log.redact_for_export();
        assert!(exported[0].detail.contains("[REDACTED]"));
    }

    #[test]
    fn redaction_catches_delimiter_tokens_and_auth_headers() {
        let mut log = AuditLog::default();
        log.record_authz_denied(
            "gw",
            "rpc:write",
            "token:abc secret=foo; Authorization: Bearer xyz",
            1,
        );
        let exported = log.redact_for_export();
        assert_eq!(
            exported[0].detail,
            "[REDACTED] [REDACTED] [REDACTED] [REDACTED] [REDACTED]"
        );
    }

    #[test]
    fn redaction_catches_json_secret_fields() {
        let mut log = AuditLog::default();
        log.record_authz_denied(
            "gw",
            "rpc:write",
            r#"{"user":"ada","token":"abc","nested":{"api_key":"k1"}}"#,
            1,
        );
        let exported = log.redact_for_export();
        let payload: serde_json::Value =
            serde_json::from_str(&exported[0].detail).expect("redacted payload remains json");
        assert_eq!(payload["token"], "[REDACTED]");
        assert_eq!(payload["nested"]["api_key"], "[REDACTED]");
        assert_eq!(payload["user"], "ada");
    }
}
