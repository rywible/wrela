#!/usr/bin/env zsh
set -euo pipefail

CODEX_BIN="codex"
LOCK_FILE="$HOME/.codex-hourly.lock"
RUN_FILE="$HOME/.codex-hourly.exec.pid"
LOG_FILE="$HOME/.codex-hourly.log"

PROMPT='Using your linear issue skill, execute the next available phase to completion of the Runtime v2 Ownership + Toolchain Independence project. I know the skill is defined for single issues, so execute the skill workflow multiple times in order to complete the project. When you complete the whole project, make sure that the parent issue is marked as done as well so the project gets properly closed out. If the whole project is complete then you can just exit.'

# Prevent multiple loop instances.
if [[ -e "$LOCK_FILE" ]]; then
  echo "Loop lock exists, exiting." >> "$LOG_FILE"
  exit 0
fi

echo $$ > "$LOCK_FILE"
trap 'rm -f "$LOCK_FILE"' EXIT

run_once() {
  # If a prior exec is still running, skip this tick.
  if [[ -f "$RUN_FILE" ]]; then
    local running_pid
    running_pid=$(cat "$RUN_FILE" 2>/dev/null || true)
    if [[ -n "$running_pid" ]] && kill -0 "$running_pid" 2>/dev/null; then
      echo "Exec still running (pid=$running_pid), skipping." >> "$LOG_FILE"
      return 0
    fi
  fi

  "$CODEX_BIN" exec "$PROMPT" >> "$LOG_FILE" 2>&1 &
  echo $! > "$RUN_FILE"
  wait $(cat "$RUN_FILE") 2>/dev/null || true
  rm -f "$RUN_FILE"
}

while true; do
  run_once
  sleep 1800
 done
