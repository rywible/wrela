#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

list_output="$(cargo test -p wrela --test cli -- --list)"
mapfile -t tests < <(printf '%s\n' "$list_output" | awk -F: '/: test$/ {print $1}' | sort)

if [[ ${#tests[@]} -eq 0 ]]; then
  echo "no cli tests discovered"
  exit 1
fi

log_dir="${repo_root}/target/wrela_cli_suite"
mkdir -p "$log_dir"
log_file="$log_dir/run.log"
: > "$log_file"

start_epoch="$(date +%s)"
pass_count=0
fail_count=0

echo "running ${#tests[@]} cli tests individually" | tee -a "$log_file"
for name in "${tests[@]}"; do
  echo "[run] $name" | tee -a "$log_file"
  if cargo test -p wrela --test cli "$name" -- --exact --nocapture >>"$log_file" 2>&1; then
    pass_count=$((pass_count + 1))
    echo "[ok]  $name" | tee -a "$log_file"
  else
    fail_count=$((fail_count + 1))
    echo "[fail] $name" | tee -a "$log_file"
  fi
  echo "" >>"$log_file"
done

elapsed=$(( $(date +%s) - start_epoch ))

echo "summary: passed=${pass_count} failed=${fail_count} total=${#tests[@]} elapsed_s=${elapsed}" | tee -a "$log_file"
if [[ $fail_count -ne 0 ]]; then
  exit 1
fi
