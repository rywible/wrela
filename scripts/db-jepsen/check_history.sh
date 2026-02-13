#!/usr/bin/env bash
set -euo pipefail

echo "Running invariant history checker lane"
cargo test -p wrela_runtime --test db_invariant_history
