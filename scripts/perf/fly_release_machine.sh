#!/usr/bin/env bash
set -euo pipefail

echo "Locks are automatically released by scripts/perf/fly_pr_perf_gate.sh via trap handlers." >&2
echo "If a lock is stuck, remove metadata under: ${FLY_LOCK_ROOT:-$HOME/.codex/state/wrela-perf-fly-locks}" >&2
exit 0
