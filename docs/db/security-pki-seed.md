# PKI Seed Contract (Phase 3 Slice)

Seed module:

- `/runtime/src/db/security/pki.rs`

Capabilities:

- Issue short-lived cert records with monotonically increasing serials.
- Revoke by serial.
- Validate cert by serial against revocation + expiry.
- Validate cert identity binding against `(cluster_id, node_id)`.

Command:

```bash
cargo test -p wrela_runtime issued_cert_is_valid_until_expiry revoked_cert_is_invalid_immediately unknown_serial_is_invalid -- --nocapture
```
