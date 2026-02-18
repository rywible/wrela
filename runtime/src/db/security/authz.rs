#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipRole {
    Voter,
    Learner,
    Gateway,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcClass {
    RaftAppend,
    RaftVote,
    SnapshotInstall,
    ClientRead,
    ClientWrite,
    ClusterAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertIdentity {
    pub cluster_id: String,
    pub node_id: String,
    pub role: MembershipRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzError {
    pub reason: String,
}

pub fn authorize(identity: &CertIdentity, rpc: RpcClass) -> Result<(), AuthzError> {
    if identity.cluster_id.trim().is_empty() {
        return Err(AuthzError {
            reason: "missing cluster identity".to_string(),
        });
    }
    if identity.node_id.trim().is_empty() {
        return Err(AuthzError {
            reason: "missing node identity".to_string(),
        });
    }

    let allowed = match identity.role {
        MembershipRole::Admin => true,
        MembershipRole::Voter => matches!(
            rpc,
            RpcClass::RaftAppend
                | RpcClass::RaftVote
                | RpcClass::SnapshotInstall
                | RpcClass::ClientRead
                | RpcClass::ClientWrite
        ),
        MembershipRole::Learner => {
            matches!(
                rpc,
                RpcClass::RaftAppend | RpcClass::SnapshotInstall | RpcClass::ClientRead
            )
        }
        MembershipRole::Gateway => matches!(rpc, RpcClass::ClientRead | RpcClass::ClientWrite),
    };

    if allowed {
        Ok(())
    } else {
        Err(AuthzError {
            reason: format!("rpc {rpc:?} denied for role {:?}", identity.role),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(role: MembershipRole) -> CertIdentity {
        CertIdentity {
            cluster_id: "cluster-a".to_string(),
            node_id: "node-1".to_string(),
            role,
        }
    }

    #[test]
    fn voter_role_allows_replication_and_client_rw() {
        let id = ident(MembershipRole::Voter);
        assert!(authorize(&id, RpcClass::RaftAppend).is_ok());
        assert!(authorize(&id, RpcClass::RaftVote).is_ok());
        assert!(authorize(&id, RpcClass::SnapshotInstall).is_ok());
        assert!(authorize(&id, RpcClass::ClientRead).is_ok());
        assert!(authorize(&id, RpcClass::ClientWrite).is_ok());
        assert!(authorize(&id, RpcClass::ClusterAdmin).is_err());
    }

    #[test]
    fn learner_role_denies_vote_and_write() {
        let id = ident(MembershipRole::Learner);
        assert!(authorize(&id, RpcClass::RaftAppend).is_ok());
        assert!(authorize(&id, RpcClass::SnapshotInstall).is_ok());
        assert!(authorize(&id, RpcClass::ClientRead).is_ok());
        assert!(authorize(&id, RpcClass::RaftVote).is_err());
        assert!(authorize(&id, RpcClass::ClientWrite).is_err());
    }

    #[test]
    fn gateway_role_is_client_only() {
        let id = ident(MembershipRole::Gateway);
        assert!(authorize(&id, RpcClass::ClientRead).is_ok());
        assert!(authorize(&id, RpcClass::ClientWrite).is_ok());
        assert!(authorize(&id, RpcClass::RaftAppend).is_err());
        assert!(authorize(&id, RpcClass::ClusterAdmin).is_err());
    }

    #[test]
    fn missing_identity_fails_closed() {
        let id = CertIdentity {
            cluster_id: "".to_string(),
            node_id: "node-1".to_string(),
            role: MembershipRole::Voter,
        };
        let err = authorize(&id, RpcClass::ClientRead).expect_err("must fail");
        assert!(err.reason.contains("missing cluster identity"));
    }
}
