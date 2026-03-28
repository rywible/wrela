# wrela development task runner

# Default: fast tests
test: test-fast

# --- Tier 0: fast (non-ignored) ---
test-fast:
    cargo test -p wrela_runtime

test-runtime:
    cargo test -p wrela_runtime

test-compiler:
    cargo test -p wrela --test cli

# --- Tier 0 subsets ---
test-consensus:
    cargo test -p wrela_runtime --test db_consensus_faults --test db_raft_property --test db_network_chaos --test db_failover_quorum --test db_dynamic_quorum_selector

test-wal:
    cargo test -p wrela_runtime --test db_wal_failure_isolation

# --- Tier 1: perf harnesses (manual, #[ignore]) ---
test-perf:
    cargo test -p wrela_runtime --release --test db_write_local_perf -- --ignored --nocapture

test-perf-rpc:
    cargo test -p wrela_runtime --release --test db_private_rpc_perf -- --ignored --nocapture

# --- Tier 2: local cluster (manual, requires wreladb-lab) ---
test-cluster:
    cargo test -p wrela_runtime \
        --test db_local_cluster_smoke \
        --test db_local_cluster_rolling \
        --test db_local_cluster_runtime_stability \
        --test db_local_cluster_load \
        --test db_local_cluster_quorum \
        -- --ignored --nocapture

test-cluster-smoke:
    cargo test -p wrela_runtime --test db_local_cluster_smoke -- --ignored --nocapture

# --- Cross-tier ---
test-all: test-fast test-perf test-cluster

# --- Build ---
build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

# --- Lint / format ---
lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check
