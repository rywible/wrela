#!/usr/bin/env bash
set -euo pipefail

echo "Running invariant history checker lane"
echo "Deterministic synthetic history checker"
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_consistent_history
echo "Live DB trace checker"
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_live_db_trace
