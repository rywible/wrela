# wrela development task runner

fast-rust-tests := "cargo test -p wrela --test one_shot_metrics_harness --test spec_project_integrity --test thin_core_snapshot"
fast-authored-tests := "cargo run -p wrela -- test language/spec --lane=spec"
full-rust-tests := "cargo test --workspace"
full-authored-tests := "cargo run -p wrela -- test language/spec"
query-tests := "cargo test -p wrela --test query_contract_registry --test query_program_spine --test phase9_query_plan"
perf-smoke-cmd := "cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1"
perf-closure-cmd := "cargo run -p wrela -- perf benchmarks/whole_frame --profile=1080p120 --query-backend=wgsl"

default:
    @just --list

# Workspace typecheck / fast compile signal.
check:
    cargo check --workspace

# Workspace build without running tests.
build:
    cargo build --workspace

# Optimized workspace build artifact.
build-release:
    cargo build --workspace --release

# Fast repo lane: small Rust integrity proofs plus the executable spec lane.
test:
    {{fast-rust-tests}}
    {{fast-authored-tests}}

# Full repo lane: full Rust workspace verification plus the authored spec project.
test-all:
    {{full-rust-tests}}
    {{full-authored-tests}}

# Focused runtime crate lane.
test-runtime:
    cargo test -p wrela_runtime

# Focused compiler crate lane.
test-compiler:
    cargo test -p wrela

# Focused CLI integration lane.
test-cli:
    cargo test -p wrela --test cli

# Focused query-contract and query-planning lane.
test-query:
    {{query-tests}}

# Cheap perf sanity lane.
perf-smoke:
    {{perf-smoke-cmd}}

# Representative whole-frame closure lane.
perf-closure:
    {{perf-closure-cmd}}

# Capture the Phase 49 developer-loop baseline report.
baseline-devloop:
    python3 scripts/devloop_measure.py --report-name phase49-baseline

# Workspace clippy gate.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format the Rust workspace.
fmt:
    cargo fmt --all

# Formatting verification gate.
fmt-check:
    cargo fmt --all -- --check

# Best-effort cargo fix followed by formatting.
fix:
    cargo fix --workspace --allow-dirty --allow-staged
    cargo fmt --all

# Authoritative local pre-ship gate.
ship:
    just fmt-check
    just lint
    just test
    just test-all
    just perf-smoke
