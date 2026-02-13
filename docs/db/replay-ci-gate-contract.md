# Replay CI Gate Artifact Contract

Artifact path: `artifacts/replay-ci-gate-report.json`

Schema version: `1`

## Required Fields

- `schema_version`: integer version marker.
- `generated_at_unix_ms`: generation timestamp.
- `status`: `pass` or `fail`.
- `checks`:
  - `invariant_regression`:
    - `passed`
    - `error_count`
  - `determinism`:
    - `passed`
    - `mismatch_count`
    - `unexpected_candidate_count`
  - `perf_regression`:
    - `passed`
    - `failure_count`
- `evidence`:
  - `canonical_manifest`
  - `candidate_root`
  - `baseline_perf`
  - `candidate_perf`
- `summary`:
  - `expected_artifact_count`
  - `candidate_artifact_count`
  - `unexpected_candidate_artifacts`
  - `perf_deltas`
- `failures`: typed failure rows containing `code` and failure-specific fields.

## Failure Codes

- `determinism.invalid_manifest_schema`
- `determinism.invalid_manifest_shape`
- `determinism.empty_manifest`
- `invariant.regression`
- `determinism.invalid_manifest_entry`
- `determinism.duplicate_manifest_path`
- `determinism.missing_artifact`
- `determinism.unexpected_artifact`
- `determinism.mismatch`
- `perf.metric_missing`
- `perf.latency_regression`
- `perf.throughput_regression`

## Canonical Corpus

- Corpus root: `docs/db/replay-corpus/v1`
- Signature manifest: `docs/db/replay-corpus/v1/manifest.json`

## Gate Command

```bash
scripts/db-chaos/replay_ci_gate.py \
  --canonical-root docs/db/replay-corpus/v1 \
  --candidate-root tests/.artifacts \
  --baseline-perf artifacts/perf-baseline-main.json \
  --candidate-perf artifacts/perf-baseline.json \
  --out artifacts/replay-ci-gate-report.json
```

Expected stdout on pass:

```text
replay CI gate passed; report: artifacts/replay-ci-gate-report.json
```

Expected stdout on fail:

```text
replay CI gate failed; report: artifacts/replay-ci-gate-report.json
```

Expected report snippets:

- PASS: `"status": "pass"`, `"checks": {"determinism": {"passed": true, ...}}`
- FAIL (placeholder/invalid manifest): `"status": "fail"` plus one of
  `determinism.empty_manifest` or `determinism.invalid_manifest_schema` in `failures[*].code`.
