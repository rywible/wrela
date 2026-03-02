#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertRecord {
    pub serial: u64,
    pub cluster_id: String,
    pub node_id: String,
    pub not_after_epoch_s: u64,
    pub revoked: bool,
}

#[derive(Debug, Default)]
pub struct PkiStore {
    next_serial: u64,
    certs: Vec<CertRecord>,
}

impl PkiStore {
    pub fn issue_cert(
        &mut self,
        cluster_id: String,
        node_id: String,
        now_epoch_s: u64,
        ttl_secs: u64,
    ) -> CertRecord {
        self.next_serial = self.next_serial.saturating_add(1);
        let cert = CertRecord {
            serial: self.next_serial,
            cluster_id,
            node_id,
            not_after_epoch_s: now_epoch_s.saturating_add(ttl_secs.max(1)),
            revoked: false,
        };
        self.certs.push(cert.clone());
        cert
    }

    pub fn revoke(&mut self, serial: u64) -> bool {
        if let Some(cert) = self.certs.iter_mut().find(|cert| cert.serial == serial) {
            cert.revoked = true;
            return true;
        }
        false
    }

    pub fn is_valid(&self, serial: u64, now_epoch_s: u64) -> bool {
        self.certs
            .iter()
            .find(|cert| cert.serial == serial)
            .map(|cert| !cert.revoked && now_epoch_s <= cert.not_after_epoch_s)
            .unwrap_or(false)
    }

    pub fn cert(&self, serial: u64) -> Option<&CertRecord> {
        self.certs.iter().find(|cert| cert.serial == serial)
    }

    pub fn validates_identity(
        &self,
        serial: u64,
        cluster_id: &str,
        node_id: &str,
        now_epoch_s: u64,
    ) -> bool {
        self.cert(serial)
            .map(|cert| {
                !cert.revoked
                    && now_epoch_s <= cert.not_after_epoch_s
                    && cert.cluster_id == cluster_id
                    && cert.node_id == node_id
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_cert_is_valid_until_expiry() {
        let mut store = PkiStore::default();
        let cert = store.issue_cert("cluster-a".to_string(), "node-1".to_string(), 100, 30);
        assert!(store.is_valid(cert.serial, 100));
        assert!(store.is_valid(cert.serial, 130));
        assert!(!store.is_valid(cert.serial, 131));
    }

    #[test]
    fn revoked_cert_is_invalid_immediately() {
        let mut store = PkiStore::default();
        let cert = store.issue_cert("cluster-a".to_string(), "node-2".to_string(), 100, 60);
        assert!(store.is_valid(cert.serial, 120));
        assert!(store.revoke(cert.serial));
        assert!(!store.is_valid(cert.serial, 120));
    }

    #[test]
    fn unknown_serial_is_invalid() {
        let store = PkiStore::default();
        assert!(!store.is_valid(999, 100));
    }

    #[test]
    fn validates_identity_checks_cluster_and_node_binding() {
        let mut store = PkiStore::default();
        let cert = store.issue_cert("cluster-a".to_string(), "node-3".to_string(), 100, 60);
        assert!(store.validates_identity(cert.serial, "cluster-a", "node-3", 120));
        assert!(!store.validates_identity(cert.serial, "cluster-b", "node-3", 120));
        assert!(!store.validates_identity(cert.serial, "cluster-a", "node-x", 120));
    }
}
