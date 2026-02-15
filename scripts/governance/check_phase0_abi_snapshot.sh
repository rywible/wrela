#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SNAPSHOT="$ROOT/language/spec/thin_core_snapshot.txt"

if [[ ! -f "$SNAPSHOT" ]]; then
  echo "missing thin-core snapshot: $SNAPSHOT" >&2
  exit 1
fi

required_intrinsics=(
  "__wr_fs_read_dir"
  "__wr_fs_metadata"
  "__wr_fs_mkdir_all"
  "__wr_fs_remove_file"
  "__wr_fs_remove_dir_all"
  "__wr_fs_rename"
  "__wr_fs_set_executable"
  "__wr_process_argv"
  "__wr_process_cwd"
  "__wr_process_run"
  "__wr_process_exit"
)

required_exports=(
  "wr_fs_read_dir"
  "wr_fs_metadata"
  "wr_fs_mkdir_all"
  "wr_fs_remove_file"
  "wr_fs_remove_dir_all"
  "wr_fs_rename"
  "wr_fs_set_executable"
  "wr_process_argv"
  "wr_process_cwd"
  "wr_process_run"
  "wr_process_exit"
)

for symbol in "${required_intrinsics[@]}"; do
  if ! rg -n "^intrinsic=${symbol}$" "$SNAPSHOT" >/dev/null; then
    echo "phase0 abi snapshot violation: missing intrinsic ${symbol}" >&2
    exit 1
  fi
done

for symbol in "${required_exports[@]}"; do
  if ! rg -n "^runtime_export=${symbol}$" "$SNAPSHOT" >/dev/null; then
    echo "phase0 abi snapshot violation: missing runtime export ${symbol}" >&2
    exit 1
  fi
done

echo "phase0 abi snapshot check passed"
