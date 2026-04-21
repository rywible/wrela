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
engine-frame-tests := "cargo test -p wrela --test engine_frame"
perf-smoke-cmd := "cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1"
perf-engine-closure-cmd := "cargo run --release -p wrela -- perf benchmarks/engine_frame --profile=1080p120 --query-backend=wgsl"
perf-engine-audit-cmd := "WRELA_PERF_ENGINE_AUDIT=1 cargo run --release -p wrela -- perf benchmarks/engine_frame --profile=1080p120 --query-backend=wgsl --perf-debug"

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

# Focused engine-frame/reporting lane.
test-engine-frame:
    {{engine-frame-tests}}

# Cheap perf sanity lane.
perf-smoke:
    {{perf-smoke-cmd}}

# Canonical engine-frame closure lane.
perf-engine-closure:
    {{perf-engine-closure-cmd}}

# Non-canonical audit lane for live-vs-compatibility engine-frame measurements.
perf-engine-audit:
    {{perf-engine-audit-cmd}}

# Canonical closure alias.
perf-closure:
    {{perf-engine-closure-cmd}}

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
    just test
    just test-all
    just perf-smoke
