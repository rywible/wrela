#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Runtime-only unsafe primitives must not leak into user-facing language sources.
if rg -n "\bunsafe\b|\bPtr\[|\bNonNull\[|\bRuntimeBox\b|__wr_runtime_caps" \
  "$ROOT/language/stdlib" "$ROOT/apps" "$ROOT/benchmarks" >/dev/null; then
  echo "public API quarantine violation: runtime unsafe primitives leaked into user-facing sources" >&2
  exit 1
fi

echo "public API quarantine check passed"
