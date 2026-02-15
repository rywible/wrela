#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
V2_ROOT="$ROOT/wrela-v2"

if [[ ! -d "$V2_ROOT" ]]; then
  echo "missing v2 root: $V2_ROOT" >&2
  exit 1
fi

if rg -n \
  -e '\bcargo\b' \
  -e '\brustc\b' \
  -e '\bcranelift\b' \
  -e 'CARGO_BIN_EXE_wrela' \
  -e 'wrela::backend::cranelift' \
  -e 'compiler/bin/wrela' \
  "$V2_ROOT" --glob '*.wr' >/dev/null; then
  echo "v2 no-cheating violation: Rust/V1 delegation markers detected in v2 .wr sources" >&2
  rg -n \
    -e '\bcargo\b' \
    -e '\brustc\b' \
    -e '\bcranelift\b' \
    -e 'CARGO_BIN_EXE_wrela' \
    -e 'wrela::backend::cranelift' \
    -e 'compiler/bin/wrela' \
    "$V2_ROOT" --glob '*.wr' >&2 || true
  exit 1
fi

# Keep anti-shortcut pressure without false-flagging valid linker relocation kinds.
if rg -n -i \
  -e '\btest_.*placeholder\b' \
  -e '\bplaceholder_(impl|implementation|only|success|pass|bypass|stub)\b' \
  -e '\b(fake|mock|stub)_(success|pass|ok)\b' \
  -e '\bskip_(check|pipeline|typecheck|lower|emit|link|runtime)\b' \
  "$V2_ROOT/src" "$V2_ROOT/tests" --glob '*.wr' >/dev/null; then
  echo "v2 no-cheating violation: placeholder/stub bypass markers detected in v2 source/tests" >&2
  rg -n -i \
    -e '\btest_.*placeholder\b' \
    -e '\bplaceholder_(impl|implementation|only|success|pass|bypass|stub)\b' \
    -e '\b(fake|mock|stub)_(success|pass|ok)\b' \
    -e '\bskip_(check|pipeline|typecheck|lower|emit|link|runtime)\b' \
    "$V2_ROOT/src" "$V2_ROOT/tests" --glob '*.wr' >&2 || true
  exit 1
fi

# Env/argv intrinsics are allowed only at explicit boundary files.
if rg -n \
  -e '__wr_env_get' \
  -e '__wr_process_argv' \
  "$V2_ROOT/src" --glob '*.wr' \
  --glob '!**/application/composition/**' \
  --glob '!**/application/env_vars.wr' \
  --glob '!**/application/argv_source.wr' \
  --glob '!**/main.wr' >/dev/null; then
  echo "v2 no-cheating violation: direct env/argv intrinsics leaked outside approved boundary files" >&2
  rg -n \
    -e '__wr_env_get' \
    -e '__wr_process_argv' \
    "$V2_ROOT/src" --glob '*.wr' \
    --glob '!**/application/composition/**' \
    --glob '!**/application/env_vars.wr' \
    --glob '!**/application/argv_source.wr' \
    --glob '!**/main.wr' >&2 || true
  exit 1
fi

echo "v2 no-cheating check passed"
