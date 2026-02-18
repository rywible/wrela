# Fly Perf Workflow for Agents

This workflow runs Wrela perf checks on pooled Fly Machines for `amd64`.

## Defaults

- Perf runs are `amd64` only.
- Runner inventory is checked in at `scripts/perf/fly_pool.json`.
- Claiming uses host-global locks under `~/.codex/state/wrela-perf-fly-locks` to avoid cross-worktree collisions.
- PR runs rebuild `wrela` from the target pushed SHA before perf (`FORCE_REBUILD_WRELA=1` by default).
- Runner machines should use a pinned perf image built from `scripts/perf/fly/Dockerfile`.

## Build runner image

```bash
scripts/perf/fly_build_runner_image.sh --bootstrap-ref main
```

This image includes:

- Rust toolchain
- Linux build deps (`clang`, `llvm`, `pkg-config`, OpenSSL headers, etc.)
- a warmed bootstrap `cargo build -p wrela --release` at `/opt/wrela-bootstrap`

## Provision / refresh runner pool

```bash
scripts/perf/fly_provision_pool.sh --image <registry.fly.io/...:tag> --count 6 --vm-size performance-4x --refresh
```

This command creates one app + one machine per runner and rewrites `scripts/perf/fly_pool.json`.

## Required auth

Use an active `flyctl` session:

```bash
flyctl auth login
```

## PR gate command

```bash
scripts/perf/fly_pr_perf_gate.sh --sha <commit-sha>
```

Artifacts and summary are written to:

- `.artifacts/perf/fly/<run-id>/summary.json`
- `.artifacts/perf/fly/<run-id>/amd64/artifacts/`

Summary reasons include:

- `ok`
- `infra_unavailable`
- `infra_error`
- `perf_failed`

## Main canonical refresh command

```bash
scripts/perf/fly_refresh_main_baseline.sh --sha <main-sha>
```

Behavior:

1. Runs amd64 Fly perf gate.
2. Marks run state as `passed`, `stale`, `infra_unavailable`, `perf_failed`, or `infra_error`.
3. Updates `.artifacts/perf/main/CANONICAL.json` only when run passed and target SHA is still current `main` head.

Canonical baseline artifacts are stored under:

- `.artifacts/perf/main/<sha>/`

Refresh reports are stored under:

- `.artifacts/perf/main/refresh-<run-id>.json`

## Strict source sync policy

- Perf scripts require a pushed SHA on `origin`.
- If a SHA is not available on `origin`, the run fails fast.
- This keeps perf runs reproducible across worktrees and reruns.

## Pool maintenance

- Validate pool config:

```bash
scripts/perf/fly_pool_validate.sh
```

- Keep runners pre-provisioned and stopped when idle.
- Use `performance` CPU kind for benchmark stability.
