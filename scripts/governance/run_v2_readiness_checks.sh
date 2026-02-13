#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

pushd "$ROOT" >/dev/null

bash scripts/governance/check_unsafe_allowlist.sh
bash scripts/governance/check_public_api_quarantine.sh
cargo test -p wrela --test contract_blackbox
cargo test -p wrela --test unsafe_quarantine
cargo test -p wrela_v2_scaffold
cargo run -q -p wrela -- test apps/ledger-lite

popd >/dev/null

echo "v2 readiness checks passed"
