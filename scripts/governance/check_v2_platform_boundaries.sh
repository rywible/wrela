#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
V2_ROOT="$ROOT/wrela-v2"

if [[ ! -d "$V2_ROOT" ]]; then
  echo "missing v2 root: $V2_ROOT" >&2
  exit 1
fi

CORE_PATHS=(
  "$V2_ROOT/src/domain"
  "$V2_ROOT/src/application"
)

# Core layers must never import platform adapters directly.
if rg -n \
  -e 'infrastructure/platform/adapters' \
  "${CORE_PATHS[@]}" \
  --glob '*.wr' \
  --glob '!**/application/composition/**' \
  --glob '!**/application/main.wr' >/dev/null; then
  echo "v2 platform-boundary violation: adapter imports leaked outside composition/main" >&2
  rg -n \
    -e 'infrastructure/platform/adapters' \
    "${CORE_PATHS[@]}" \
    --glob '*.wr' \
    --glob '!**/application/composition/**' \
    --glob '!**/application/main.wr' >&2 || true
  exit 1
fi

# Core layers must not directly import host/runtime stdlib primitives.
if rg -n \
  -e 'language/stdlib/host/' \
  -e 'language/stdlib/runtime/reactor' \
  "${CORE_PATHS[@]}" \
  --glob '*.wr' \
  --glob '!**/application/composition/**' \
  --glob '!**/application/main.wr' >/dev/null; then
  echo "v2 platform-boundary violation: host/runtime stdlib imports leaked outside composition/main" >&2
  rg -n \
    -e 'language/stdlib/host/' \
    -e 'language/stdlib/runtime/reactor' \
    "${CORE_PATHS[@]}" \
    --glob '*.wr' \
    --glob '!**/application/composition/**' \
    --glob '!**/application/main.wr' >&2 || true
  exit 1
fi

# Adapter layer is where platform-specific references are allowed,
# but Linux paths must be present to prevent macOS lock-in.
ADAPTERS_DIR="$V2_ROOT/src/infrastructure/platform/adapters"
if [[ ! -f "$ADAPTERS_DIR/darwin_kqueue_adapter.wr" ]]; then
  echo "missing Darwin adapter: $ADAPTERS_DIR/darwin_kqueue_adapter.wr" >&2
  exit 1
fi
if [[ ! -f "$ADAPTERS_DIR/linux_io_uring_adapter.wr" ]]; then
  echo "missing Linux io_uring adapter: $ADAPTERS_DIR/linux_io_uring_adapter.wr" >&2
  exit 1
fi
if [[ ! -f "$ADAPTERS_DIR/linux_epoll_adapter.wr" ]]; then
  echo "missing Linux epoll adapter: $ADAPTERS_DIR/linux_epoll_adapter.wr" >&2
  exit 1
fi

COMPOSITION_FILE="$V2_ROOT/src/application/composition/platform_ports.wr"
TARGET_ENV_FILE="$V2_ROOT/src/application/composition/platform_target_env.wr"
MAIN_FILE="$V2_ROOT/src/main.wr"
if [[ ! -f "$COMPOSITION_FILE" ]]; then
  echo "missing platform composition file: $COMPOSITION_FILE" >&2
  exit 1
fi

if [[ ! -f "$TARGET_ENV_FILE" ]]; then
  echo "missing platform target-env composition file: $TARGET_ENV_FILE" >&2
  exit 1
fi

if [[ ! -f "$MAIN_FILE" ]]; then
  echo "missing v2 main file: $MAIN_FILE" >&2
  exit 1
fi

if ! rg -n "LinuxIoUringReactor|prefer_io_uring" "$COMPOSITION_FILE" >/dev/null; then
  echo "missing Linux io_uring composition path in $COMPOSITION_FILE" >&2
  exit 1
fi

if ! rg -n "LinuxEpollReactor" "$COMPOSITION_FILE" >/dev/null; then
  echo "missing Linux epoll fallback composition path in $COMPOSITION_FILE" >&2
  exit 1
fi

if ! rg -n "reactor_backend_name\\s*=\\s*\"io_uring\"|reactor_backend_name\\s*=\\s*\"epoll\"|reactor_backend_name\\s*=\\s*\"kqueue\"" "$COMPOSITION_FILE" >/dev/null; then
  echo "missing explicit reactor backend mapping in $COMPOSITION_FILE" >&2
  exit 1
fi

if ! rg -n "host_family_name\\s*=\\s*\"linux\"|host_family_name\\s*=\\s*\"darwin\"" "$COMPOSITION_FILE" >/dev/null; then
  echo "missing explicit host-family mapping in $COMPOSITION_FILE" >&2
  exit 1
fi

if ! rg -n "current_platform_target|normalize_os_name" "$TARGET_ENV_FILE" >/dev/null; then
  echo "missing target normalization wiring in $TARGET_ENV_FILE" >&2
  exit 1
fi

RUNTIME_DIR="$V2_ROOT/src/application/runtime"
if rg -n \
  -e 'fake_success' \
  -e 'skip_runtime' \
  -e 'placeholder_runtime' \
  -e 'skip_cancel' \
  -e 'fake_cancel' \
  -e 'placeholder_shutdown' \
  "$RUNTIME_DIR" \
  --glob '*.wr' >/dev/null; then
  echo "v2 platform-boundary violation: runtime bypass marker detected in $RUNTIME_DIR" >&2
  rg -n \
    -e 'fake_success' \
    -e 'skip_runtime' \
    -e 'placeholder_runtime' \
    -e 'skip_cancel' \
    -e 'fake_cancel' \
    -e 'placeholder_shutdown' \
    "$RUNTIME_DIR" \
    --glob '*.wr' >&2 || true
  exit 1
fi

echo "v2 platform boundary check passed"
