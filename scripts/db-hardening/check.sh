#!/usr/bin/env bash
set -euo pipefail

echo "DB hardening gate: clippy hygiene (db paths)"
bash scripts/db-hardening/clippy-db.sh

echo "DB hardening gate: raft append"
cargo test -p wrela_runtime db::raft::append::tests -- --nocapture

echo "DB hardening gate: replication"
cargo test -p wrela_runtime db::replication:: -- --nocapture

echo "DB hardening gate: phase2 restart/auth/close/convergence"
cargo test -p wrela_runtime raft_membership_state_persists_across_restart -- --nocapture
cargo test -p wrela_runtime raft_durable_state_corruption_fails_open_fail_closed -- --nocapture
cargo test -p wrela_runtime membership_mutations_require_cluster_admin_role -- --nocapture
cargo test -p wrela_runtime close_db_removes_handle_even_when_clock_flush_fails -- --nocapture
cargo test -p wrela_runtime replication_converges_for_conflict_distance_beyond_legacy_cap -- --nocapture

echo "DB hardening gate: sql"
cargo test -p wrela_runtime db::sql::tests -- --nocapture

echo "DB hardening gate: safe-time"
cargo test -p wrela_runtime db::time::safe_time::tests -- --nocapture

echo "DB hardening gate: invariant history"
cargo test -p wrela_runtime --test db_invariant_history -- --nocapture

echo "DB hardening gate: jepsen history script"
bash scripts/db-jepsen/check_history.sh

echo "DB hardening gate: runtime blocker suite"
cargo test -p wrela_runtime
