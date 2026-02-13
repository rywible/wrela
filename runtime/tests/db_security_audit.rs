use wrela_runtime::db::audit::{AuditEventKind, AuditLog};
use wrela_runtime::db::security::authz::{CertIdentity, MembershipRole, RpcClass, authorize};
use wrela_runtime::db::security::hardening::{SecurityConfig, validate_security_config};
use wrela_runtime::db::security::pki::PkiStore;
use wrela_runtime::db::security::residency::{ResidencyPolicy, ResidencyRule};

#[test]
fn security_controls_emit_auditable_events() {
    let cfg = SecurityConfig {
        require_mtls: true,
        enforce_authz: true,
        enable_audit_log: true,
        allow_insecure_fallback: false,
        cert_rotation_window_s: 3_600,
        key_rotation_interval_s: 86_400,
    };
    assert_eq!(validate_security_config(&cfg), Ok(()));

    let mut audit = AuditLog::default();
    let mut pki = PkiStore::default();
    let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 3600);
    audit.record_cert_issued("ca", format!("cert:{}", cert.serial), "ttl=3600", 100);

    let denied = authorize(
        &CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role: MembershipRole::Gateway,
        },
        RpcClass::RaftVote,
    )
    .expect_err("gateway must not access raft vote");
    audit.record_authz_denied("node-1", "rpc:raft_vote", denied.reason, 101);

    let residency = ResidencyPolicy::with_rules(vec![ResidencyRule {
        shard: b"core".to_vec(),
        allowed_regions: vec!["us".to_string()],
    }]);
    let err = residency
        .authorize_egress(b"core", "eu")
        .expect_err("egress to eu must be denied");
    audit.record_residency_denied("policy", "shard:core", err.fail_closed_message(), 102);

    assert!(pki.revoke(cert.serial));
    audit.record_cert_revoked("ca", format!("cert:{}", cert.serial), "reason=rotate", 103);
    audit.record_key_rotation("kms", "key:db", "token=version2", 104);

    assert_eq!(audit.events().len(), 5);
    assert_eq!(audit.by_kind(AuditEventKind::AuthzDenied).len(), 1);
    assert_eq!(audit.by_kind(AuditEventKind::ResidencyDenied).len(), 1);
    assert_eq!(audit.by_kind(AuditEventKind::CertIssued).len(), 1);
    assert_eq!(audit.by_kind(AuditEventKind::CertRevoked).len(), 1);
    assert_eq!(audit.by_kind(AuditEventKind::KeyRotation).len(), 1);

    let exported = audit.redact_for_export();
    assert!(
        exported
            .iter()
            .any(|event| event.detail.contains("[REDACTED]")),
        "export must redact sensitive tokens"
    );
}
