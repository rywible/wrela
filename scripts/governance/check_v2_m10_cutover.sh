#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAUNCHER_RS="$ROOT_DIR/compiler/bin/wrela/v2_launcher.rs"
ENTRY_RS="$ROOT_DIR/compiler/bin/wrela.rs"

if [[ ! -f "$LAUNCHER_RS" ]]; then
    echo "[m10-cutover] Missing launcher module: compiler/bin/wrela/v2_launcher.rs" >&2
    exit 1
fi

if [[ ! -f "$ENTRY_RS" ]]; then
    echo "[m10-cutover] Missing entrypoint: compiler/bin/wrela.rs" >&2
    exit 1
fi

if ! rg -q "error: m10 cutover is darwin-arm64 only" "$LAUNCHER_RS"; then
    echo "[m10-cutover] Missing deterministic non-darwin fail-closed message" >&2
    exit 1
fi

if ! rg -q "WRELA_USE_V1_FALLBACK" "$LAUNCHER_RS"; then
    echo "[m10-cutover] Missing explicit fallback env gate" >&2
    exit 1
fi

if rg -q "fallback.*on.*v2.*fail|auto.*fallback|silently.*fallback" "$LAUNCHER_RS"; then
    echo "[m10-cutover] Auto-fallback pattern detected" >&2
    exit 1
fi

if ! rg -q "try_run_cutover_launcher" "$ENTRY_RS"; then
    echo "[m10-cutover] Entrypoint is not using cutover launcher" >&2
    exit 1
fi

echo "[m10-cutover] launcher and cutover guardrails look sane"
