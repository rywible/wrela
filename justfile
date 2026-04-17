# wrela development task runner

fast-rust-tests := "cargo test -p wrela --test repo_smoke"
fast-authored-tests := "cargo run -p wrela -- test language/spec --lane=fast"
full-rust-tests := "cargo test --workspace"
full-authored-tests := "cargo run -p wrela -- test language/spec --lane=full"
cleanroom-check-dir := ".artifacts/cargo-cleanroom/check"
cleanroom-test-dir := ".artifacts/cargo-cleanroom/test"
cleanroom-check := "CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/check cargo check --workspace"
cleanroom-test := "CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/test cargo test --workspace"
query-tests := "cargo test -p wrela --test query_contract_registry --test query_program_spine --test phase9_query_plan"
perf-smoke-cmd := "cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1"
perf-closure-cmd := "cargo run -p wrela -- perf benchmarks/whole_frame --profile=1080p120 --query-backend=wgsl"

default:
    @just --list

# Workspace typecheck / fast compile signal.
check:
    cargo check --workspace

# Cleanroom workspace typecheck with incremental disabled and isolated artifacts.
check-clean:
    rm -rf {{cleanroom-check-dir}}
    {{cleanroom-check}}

# Workspace build without running tests.
build:
    cargo build --workspace

# Optimized workspace build artifact.
build-release:
    cargo build --workspace --release

# Fast repo lane: repo smoke coverage plus the native authored fast lane.
test:
    {{fast-rust-tests}}
    {{fast-authored-tests}}

# Cleanroom Rust workspace verification with incremental disabled and isolated artifacts.
test-clean:
    rm -rf {{cleanroom-test-dir}}
    {{cleanroom-test}}

# Full repo lane: full Rust workspace verification plus the native authored full lane.
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

# Capture the Phase 52 developer-loop scorecard report.
baseline-devloop:
    python3 scripts/devloop_measure.py --report-name phase52-baseline

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
