#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

intrinsics=(
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

search_files=(
  "$ROOT/compiler/hir/semantic.rs"
  "$ROOT/compiler/hir/typeck.rs"
  "$ROOT/compiler/hir/project.rs"
  "$ROOT/compiler/mir/lower.rs"
  "$ROOT/compiler/backend/cranelift.rs"
)

for symbol in "${intrinsics[@]}"; do
  for file in "${search_files[@]}"; do
    if ! rg -n "$symbol" "$file" >/dev/null; then
      echo "phase0 surface wiring violation: missing ${symbol} in ${file}" >&2
      exit 1
    fi
  done
done

runtime_exports=(
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

for symbol in "${runtime_exports[@]}"; do
  if ! rg -n "pub extern \"C\" fn ${symbol}\\(" "$ROOT/runtime/src/lib.rs" >/dev/null; then
    echo "phase0 surface wiring violation: missing runtime export ${symbol}" >&2
    exit 1
  fi
done

stdlib_symbols=(
  "try_to_read_directory"
  "try_to_read_metadata"
  "try_to_create_directory"
  "try_to_remove_file"
  "try_to_remove_directory"
  "try_to_rename_path"
  "try_to_set_executable"
)

for symbol in "${stdlib_symbols[@]}"; do
  if ! rg -n "to ${symbol}\\(" "$ROOT/language/stdlib/host/fs.wr" >/dev/null; then
    echo "phase0 surface wiring violation: missing stdlib fs helper ${symbol}" >&2
    exit 1
  fi
done

process_symbols=(
  "get_argv"
  "try_to_get_current_working_directory"
  "create_process_spec"
  "try_to_run_process"
  "exit"
)

for symbol in "${process_symbols[@]}"; do
  if ! rg -n "to ${symbol}\\(" "$ROOT/language/stdlib/host/process.wr" >/dev/null; then
    echo "phase0 surface wiring violation: missing stdlib process helper ${symbol}" >&2
    exit 1
  fi
done

echo "phase0 surface wiring check passed"
