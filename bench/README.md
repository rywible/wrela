# Perf Baselines

This directory tracks perf baselines for CI gating.

## Phase Snapshots

For phase-by-phase visibility, store point-in-time microbench snapshots under `bench/snapshots/<phase>-<YYYY-MM-DD>/`.

Example:

```
bench/snapshots/phase-1-2026-02-03/microbench.txt
```

## Update Baseline

Run the perf gate script with update enabled and a target key.

```sh
WRELA_PERF_KEY=macos-aarch64 WRELA_PERF_UPDATE=1 python3 scripts/perf_gate.py
```

By default the script runs 3 samples and keeps the best p99. Override with:

```sh
WRELA_PERF_RUNS=5 python3 scripts/perf_gate.py
```

## Override Noisy Runs

If a CI run is noisy and you need a one-off bypass, set:

```sh
WRELA_PERF_ALLOW_REGRESSION=1 python3 scripts/perf_gate.py
```

## Baseline Format

`bench/baselines/perf.json` stores per-target p50/p99 and allocs/request values.
