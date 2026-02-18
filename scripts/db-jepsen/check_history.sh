#!/usr/bin/env bash
set -euo pipefail

echo "Running invariant history checker lane"
echo "Deterministic + adversarial synthetic history checker"
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_consistent_history
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_detects_lost_write_and_duplicate_commit
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_rejects_substring_and_lexicographic_cheats
echo "Coverage gate: insufficient observation must be surfaced explicitly"
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_flags_insufficient_observation
echo "Live DB trace checker"
cargo test -p wrela_runtime --test db_invariant_history invariant_checker_accepts_live_db_trace
