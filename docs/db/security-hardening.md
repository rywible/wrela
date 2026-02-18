# Security Hardening

## Controls

- AuthZ fail-closed for missing identity or role mismatch.
- PKI validity checks for serial, revocation, expiry, and node binding.
- Residency egress enforcement with typed deny/unsat tokens.
- Security audit log with sensitive token redaction.
- Security config validator rejects insecure fallback modes (`allow_insecure_fallback=true`) and missing mandatory controls (mTLS/AuthZ/Audit).
- Rotation policy helpers define deterministic cert/key rotation due windows.

## Audit Events

`db::audit` emits typed events:

- `AuthzDenied`
- `CertIssued`
- `CertRevoked`
- `KeyRotation`
- `ResidencyDenied`

## Verification

```bash
cargo test -p wrela_runtime db::security::
cargo test -p wrela_runtime db::audit::
cargo test -p wrela_runtime --test db_security_audit
```
