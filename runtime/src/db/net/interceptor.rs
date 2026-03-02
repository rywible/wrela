use crate::db::security::authz::{AuthzError, CertIdentity, RpcClass, authorize};
use crate::db::security::pki::PkiStore;

pub fn authorize_rpc_call(identity: &CertIdentity, rpc: RpcClass) -> Result<(), AuthzError> {
    authorize(identity, rpc)
}

pub fn intercept_rpc<T, F>(
    identity: &CertIdentity,
    rpc: RpcClass,
    handler: F,
) -> Result<T, AuthzError>
where
    F: FnOnce() -> T,
{
    authorize_rpc_call(identity, rpc)?;
    Ok(handler())
}

pub fn authorize_rpc_call_with_pki(
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
    identity: &CertIdentity,
    rpc: RpcClass,
) -> Result<(), AuthzError> {
    if !pki.validates_identity(
        cert_serial,
        &identity.cluster_id,
        &identity.node_id,
        now_epoch_s,
    ) {
        return Err(AuthzError {
            reason: format!(
                "certificate identity validation failed: serial={cert_serial} cluster={} node={}",
                identity.cluster_id, identity.node_id
            ),
        });
    }
    authorize_rpc_call(identity, rpc)
}

pub fn intercept_rpc_with_pki<T, F>(
    pki: &PkiStore,
    cert_serial: u64,
    now_epoch_s: u64,
    identity: &CertIdentity,
    rpc: RpcClass,
    handler: F,
) -> Result<T, AuthzError>
where
    F: FnOnce() -> T,
{
    authorize_rpc_call_with_pki(pki, cert_serial, now_epoch_s, identity, rpc)?;
    Ok(handler())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::security::authz::MembershipRole;
    use crate::db::security::pki::PkiStore;
    use std::cell::Cell;

    fn identity(role: MembershipRole) -> CertIdentity {
        CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role,
        }
    }

    #[test]
    fn interceptor_denies_unauthorized_rpc_without_executing_handler() {
        let called = Cell::new(false);
        let id = identity(MembershipRole::Gateway);
        let result = intercept_rpc(&id, RpcClass::RaftAppend, || {
            called.set(true);
            1u64
        });
        assert!(result.is_err());
        assert!(!called.get(), "handler must not execute on deny");
    }

    #[test]
    fn interceptor_allows_authorized_rpc_and_executes_handler() {
        let called = Cell::new(false);
        let id = identity(MembershipRole::Voter);
        let result = intercept_rpc(&id, RpcClass::RaftAppend, || {
            called.set(true);
            7u64
        });
        assert_eq!(result.expect("authorized"), 7);
        assert!(called.get(), "handler must execute on allow");
    }

    #[test]
    fn interceptor_with_pki_denies_revoked_identity() {
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);
        assert!(pki.revoke(cert.serial));
        let id = identity(MembershipRole::Voter);
        let result =
            intercept_rpc_with_pki(&pki, cert.serial, 110, &id, RpcClass::RaftAppend, || 5u64);
        assert!(result.is_err());
    }

    #[test]
    fn interceptor_with_pki_denies_cluster_node_mismatch() {
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);
        let id = CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-x".to_string(),
            role: MembershipRole::Voter,
        };
        let result =
            intercept_rpc_with_pki(&pki, cert.serial, 110, &id, RpcClass::RaftAppend, || 5u64);
        assert!(result.is_err());
    }

    #[test]
    fn interceptor_with_pki_allows_valid_identity_and_executes_handler() {
        let mut pki = PkiStore::default();
        let cert = pki.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 60);
        let id = identity(MembershipRole::Voter);
        let called = Cell::new(false);
        let result =
            intercept_rpc_with_pki(&pki, cert.serial, 110, &id, RpcClass::RaftAppend, || {
                called.set(true);
                9u64
            });
        assert_eq!(result.expect("authorized"), 9);
        assert!(called.get(), "handler must execute on allow");
    }
}
