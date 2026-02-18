# GCP Spot Perf Workflow for Agents

This workflow runs Wrela perf checks on ephemeral Linux VMs for both `amd64` and `arm64`.

## Defaults

- Spot instances are on by default (`GCP_USE_SPOT=1`).
- Spot retries are enabled (`GCP_SPOT_MAX_RETRIES=3`, `GCP_SPOT_BACKOFF_SEC=20`).
- No implicit on-demand fallback (`GCP_ALLOW_FALLBACK_ONDEMAND=0`).
- PR gate requires both arches to pass.
- Ephemeral runners default to custom prewarmed image families:
  - `GCP_AMD64_IMAGE_FAMILY=wrela-perf-amd64`
  - `GCP_ARM64_IMAGE_FAMILY=wrela-perf-arm64`
- PR runs rebuild `wrela` from the current branch/worktree before perf (`FORCE_REBUILD_WRELA=1` by default).

## Required auth

Use an active `gcloud` session with a configured project:

```bash
gcloud auth login
gcloud config set project <project-id>
```

## PR gate command

```bash
scripts/perf/gcp_pr_perf_gate.sh --sha <commit-sha>
```

Artifacts and summary are written to:

- `.artifacts/perf/gcp/<run-id>/summary.json`
- `.artifacts/perf/gcp/<run-id>/amd64/artifacts/`
- `.artifacts/perf/gcp/<run-id>/arm64/artifacts/`

Summary reasons include:

- `ok`
- `infra_preempted`
- `perf_failed`
- `infra_error`

## Main canonical refresh command

```bash
scripts/perf/gcp_refresh_main_baseline.sh --sha <main-sha>
```

Behavior:

1. Runs dual-arch Spot perf gate.
2. Marks run state as `passed`, `stale`, `infra_preempted`, `perf_failed`, or `infra_error`.
3. Updates `.artifacts/perf/main/CANONICAL.json` only when run passed and target SHA is still current `main` head.

Canonical baseline artifacts are stored under:

- `.artifacts/perf/main/<sha>/`

Refresh reports are stored under:

- `.artifacts/perf/main/refresh-<run-id>.json`

## Optional on-demand fallback

To allow fallback only after Spot retries are exhausted:

```bash
GCP_ALLOW_FALLBACK_ONDEMAND=1 scripts/perf/gcp_pr_perf_gate.sh --sha <commit-sha>
```

This is opt-in and off by default.

## Build / refresh prewarmed image families

Run this when you need to initialize or refresh the baked images:

```bash
scripts/perf/gcp_build_perf_images.sh
```

Key behavior:

- Creates temporary builder VMs for `amd64` and `arm64`.
- Installs Rust/toolchain/deps.
- Seeds and builds Wrela (`cargo build -p wrela --release`) by default.
- Publishes fresh family images:
  - `wrela-perf-amd64`
  - `wrela-perf-arm64`

Useful knobs:

- `WARM_WRELA=0` to skip code warmup.
- `WARM_SYNC_MODE=git` to seed from remote branch clone instead of archive upload.
- `KEEP_BUILDERS=1` to keep builder VMs for debugging.

Important:

- This image build is a maintenance workflow, not a per-PR workflow.
- Normal PR perf runs should call only `gcp_pr_perf_gate.sh` against existing image families.
