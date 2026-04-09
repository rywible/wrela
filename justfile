# wrela development task runner

test:
    cargo test --workspace

test-runtime:
    cargo test -p wrela_runtime

test-compiler:
    cargo test -p wrela

test-cli:
    cargo test -p wrela --test cli

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check
