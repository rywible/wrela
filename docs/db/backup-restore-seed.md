# Backup/Restore Seed Contract (WRE-491, WRE-493)

Seed module:

- `/runtime/src/db/backup/mod.rs`

Scope:

- Deterministic backup manifest envelope for restore orchestration.
- Pre-restore integrity verification against snapshot manifest metadata.
- Typed restore plan outcomes and failure reasons.
- Async multipart S3 upload session model with persisted/resumable state.
- Deterministic retry policy for failed multipart chunks.
- Typed multipart upload progress reporting.
- Typed retention policy engine for deterministic manifest pruning.
- Continuous integrity verification sampler with deterministic sample selection.
- Typed verification status summary for observability hooks.

## Contract

`BackupManifestEnvelope` is the backup handoff contract for restore planning:

- `version`: envelope schema version (`BACKUP_MANIFEST_VERSION = 1`).
- `target_uri`: backup object location (for example, `s3://...`).
- `snapshot`: copied snapshot metadata:
  - `last_index`
  - `last_term`
  - `checksum`

Determinism guarantee:

- `canonical_bytes()` emits a stable field-ordered representation:
  - `version,target_uri,last_index,last_term,checksum`
- This keeps envelope serialization stable for hashing/signing and deterministic comparisons.

## Integrity Verification

`verify_snapshot_manifest_consistency(envelope, snapshot_manifest)` validates before plan creation:

- Envelope version is supported.
- `target_uri` is non-empty.
- Snapshot manifest version is supported.
- Snapshot `last_index`, `last_term`, and `checksum` match envelope metadata.

Any mismatch returns a typed `RestorePlanError` variant.

## Restore Planning

`build_restore_plan(envelope, snapshot_manifest)` performs integrity verification first and then returns:

- `RestorePlanOutcome::PlanReady(RestorePlan)` on success.

Failure is explicit with `RestorePlanError`, including:

- `UnsupportedEnvelopeVersion`
- `InvalidTargetUri`
- `SnapshotVersionUnsupported`
- `SnapshotIndexMismatch`
- `SnapshotTermMismatch`
- `SnapshotChecksumMismatch`

## Tests

Module tests cover requested seed paths:

- Successful plan build from consistent envelope + snapshot manifest.
- Checksum mismatch failure path.
- Unsupported envelope version failure path.
- Interrupted multipart upload resume from persisted session bytes.
- Retry exhaustion returns typed failure for failed multipart part.
- Multipart completion reports typed successful progress status.

## Multipart Upload Session Seed

`MultipartUploadSession` is the persisted orchestration model for async multipart uploads:

- `version`: schema version (`MULTIPART_UPLOAD_SESSION_VERSION = 1`).
- `target_uri`: S3 object destination.
- `upload_id`: provider-issued multipart upload identifier.
- `parts`: ordered part list of `MultipartUploadPart`:
  - `part_number`: deterministic chunk number.
  - `checksum`: chunk checksum material.
  - `state`: `Pending | Uploaded | Failed`.
  - `attempt_count`: how many retry attempts were consumed.

Persistence and resume:

- `to_persisted_bytes()` validates then encodes session state into bytes.
- `from_persisted_bytes(bytes)` decodes and validates persisted state to resume an interrupted upload.
- Validation fails with typed `MultipartUploadError` variants (unsupported version, invalid identifiers, duplicate parts, invalid checksums, etc.).

Retry semantics:

- `RetryPolicy::deterministic_backoff_schedule()` emits a bounded, deterministic delay schedule.
- `retry_failed_parts(policy)` applies retries only to parts in `Failed` state:
  - increments `attempt_count`,
  - transitions each retriable part back to `Pending`,
  - returns deterministic `PartRetrySchedule` entries.
- Exhausted retries return typed `MultipartUploadError::RetryExhausted`.

Progress API:

- `progress()` returns `MultipartProgress` with typed `MultipartProgressStatus`:
  - `InProgress`
  - `Completed`
  - `Failed`
- Counts for total/pending/uploaded/failed parts are included for telemetry/UI reporting.

## Retention Policy Engine

`RetentionPolicy` defines deterministic prune behavior:

- `keep_last_n`: always retain the newest N manifests.
- `min_age_days`: only allow pruning manifests that are old enough.

`plan_retention_prune(policy, manifests, now_epoch_day)` returns `RetentionPrunePlan`:

- Stable ordering is deterministic:
  - newest `created_at_epoch_day` first,
  - then `manifest_id`,
  - then `target_uri`.
- `RetentionDecision` is emitted per manifest with a typed reason:
  - `KeepLastN`
  - `AgeBelowMin`
  - `AgeEligible`
- Final plan includes explicit `retained_manifest_ids` and `pruned_manifest_ids`.

This is order-independent: the same manifest set always produces the same plan.

## Continuous Verification Sampler

`VerificationSamplingPolicy` controls sample size:

- `sample_size`: number of manifests selected each run.

`plan_verification_sample(policy, manifests)` is deterministic:

- Every manifest gets a stable FNV-1a hash from canonical manifest fields.
- Manifests are sorted by hash (then `manifest_id`) and the first N are selected.
- Output is typed as `VerificationSamplePlan { sampled_manifest_ids }`.

## Verification Summary For Observability

`summarize_verification_results(results)` aggregates typed results:

- Per-manifest result type: `VerificationResult` with `VerificationStatus`:
  - `Verified`
  - `Corrupt`
  - `Skipped`
- Summary output type: `VerificationStatusSummary` with explicit counters and overall status:
  - `Empty`
  - `Healthy`
  - `Degraded`
  - `Failed`

Status policy:

- `Failed` if any manifest is `Corrupt`.
- `Degraded` if no corruption exists but at least one manifest is `Skipped`.
- `Healthy` when all sampled manifests are verified.
- `Empty` when no results were aggregated.

Verification command:

```bash
cargo test -p wrela_runtime backup::tests -- --nocapture
```
