#!/bin/sh
# Builds cpu.wasm for the page. Needs the wasm32-unknown-unknown target.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown --lib
cp target/wasm32-unknown-unknown/release/grazer_cpu.wasm ../cpu.wasm
ls -l ../cpu.wasm
