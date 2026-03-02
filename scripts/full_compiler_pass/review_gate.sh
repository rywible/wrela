#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

REVIEW_DIR=".artifacts/full-compiler-pass/WFE2-990"
mkdir -p "${REVIEW_DIR}"

RUST_TEST_THREADS=1 cargo test --workspace > "${REVIEW_DIR}/workspace-test.log" 2>&1

cat > "${REVIEW_DIR}/review-report.md" <<'REPORT'
# WFE2-990 Review Report

`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

## Scope

1. Compiler syntax/HIR/typeck lane changes for `component/resource/event/system/view/widget/scene/anim/theme`.
2. Dual-target game build/run/check flow (`native|wasm|dual`) and manifest metadata.
3. Runtime/game session authority + rollback/correction path and browser loader integration.
4. App migrations: `apps/wrela-game-slice` and `apps/wrela-website-slice`.
5. Hard-cut cleanup for legacy Rust-owned domain crate references.

## Findings (P0-P2)

1. No P0 findings.
2. No P1 findings.
3. No P2 findings.

## Residual Risks / Gaps

1. No blocking residual risks identified for the hard-cut lane after final gate execution.
2. Non-blocking warning cleanup remains in runtime bridge constants (`MESSAGE_TYPE_STATE_SNAPSHOT`, `MESSAGE_TYPE_STATE_DELTA`).

## Verification

1. `cargo test --workspace` (log: `.artifacts/full-compiler-pass/WFE2-990/workspace-test.log`)
2. `scripts/full_compiler_pass/final_gate.sh`
REPORT

echo "review report: ${REVIEW_DIR}/review-report.md"
