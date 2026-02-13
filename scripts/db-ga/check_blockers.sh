#!/usr/bin/env bash
set -euo pipefail

run() {
  echo "==> $*"
  "$@"
}

run cargo test -p wrela_runtime
run cargo test -p wrela_runtime --test db_upgrade_compat
run cargo test -p wrela_runtime --test db_analytics_public_api
run cargo test -p wrela_runtime --test db_analytics_durability_gate

echo "All GA blockers passed."
