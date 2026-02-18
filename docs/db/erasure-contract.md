# Erasure/Delete Compliance Contract

## Scope

This contract defines how delete intents propagate across:

- live storage deletion semantics,
- CDC downstream visibility,
- backup artifacts used for restore.

## Delete Intent Model

- `intent_id`: stable id for audit and replay
- `mode`: `HARD_DELETE` or `CRYPTOSHRED`
- `legal_hold_until_commit_seq`: optional block on physical prune
- `tombstone_retention_commits`: minimum retention window before prune
- `residency_scope`: policy scope (`US`, `EU`, etc)

## CDC Contract

Delete CDC events carry an `erasure_proof` payload:

- `intent_id`
- `proof_hash`

This provides downstream consumers (analytics/CDC sinks) an auditable link between a delete event and its compliance intent.

## Backup Contract

`BackupManifestRecord` includes `erasure_proofs[]` with:

- `intent_id`
- `proof_hash`
- `residency_scope`
- `mode`
- `commit_seq`
- `key_fingerprint`

Restore workflows must honor retained legal-hold and retention-window policy before any physical data resurrection.

## Verification

```bash
cargo test -p wrela_runtime --test db_erasure
```
