#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ARTIFACT_ROOT="${WRELA_AAA_ARTIFACT_ROOT:-${ROOT}/.artifacts/aaa-engine-pass}"
SUMMARY_DIR="${ARTIFACT_ROOT}/INT-02"
SUMMARY_PATH="${SUMMARY_DIR}/summary.md"
REVIEW_REPORT_PATH="${ARTIFACT_ROOT}/REV-999/review-report.md"
REVIEW_OUTCOME_PATH="${ARTIFACT_ROOT}/REV-999/review-outcome.json"

mkdir -p "$SUMMARY_DIR"

fail() {
  echo "aaa final gate failed: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  local label="$2"
  if [[ ! -f "$path" ]]; then
    fail "missing ${label}: ${path}"
  fi
}

validate_lane_artifact_contract() {
  local lane="$1"
  local path="$2"
  if ! node - "$path" "$lane" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [artifactPath, expectedLane] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let payload;
try {
  payload = JSON.parse(fs.readFileSync(artifactPath, "utf8"));
} catch (error) {
  fail(`invalid lane artifact json ${artifactPath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (Number(payload?.schema_version) !== 1) {
  fail(`lane artifact schema_version must equal 1 in ${artifactPath}`);
}
if (payload?.lane !== expectedLane) {
  fail(`lane artifact lane mismatch in ${artifactPath}: expected '${expectedLane}' got '${payload?.lane}'`);
}
if (payload?.status !== "passed") {
  fail(`lane artifact status must be 'passed' in ${artifactPath}; got '${payload?.status}'`);
}

if (expectedLane !== "INT-01") {
  process.exit(0);
}

const checks = payload?.checks;
if (!checks || typeof checks !== "object" || Array.isArray(checks)) {
  fail(`INT-01 lane artifact missing checks object in ${artifactPath}`);
}

const requiredChecks = [
  "overall_pass",
  "determinism_parity",
  "render_lane_pass",
  "asset_streaming_pass",
];
for (const key of requiredChecks) {
  if (checks[key] !== true) {
    fail(`INT-01 checks.${key} must be true in ${artifactPath}`);
  }
}

if (!Object.prototype.hasOwnProperty.call(payload, "test_matrix_path")) {
  process.exit(0);
}

const matrixPathValue = payload.test_matrix_path;
if (typeof matrixPathValue !== "string" || matrixPathValue.trim().length === 0) {
  fail(`INT-01 test_matrix_path must be a non-empty string in ${artifactPath}`);
}

const resolvedMatrixPath = path.isAbsolute(matrixPathValue)
  ? matrixPathValue
  : path.resolve(path.dirname(artifactPath), matrixPathValue);

if (!fs.existsSync(resolvedMatrixPath)) {
  fail(`INT-01 test_matrix_path does not exist: ${resolvedMatrixPath}`);
}

let matrixPayload;
try {
  matrixPayload = JSON.parse(fs.readFileSync(resolvedMatrixPath, "utf8"));
} catch (error) {
  fail(`invalid INT-01 test matrix json ${resolvedMatrixPath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (matrixPayload?.overall_pass !== true) {
  fail(`INT-01 test matrix overall_pass must be true in ${resolvedMatrixPath}`);
}

if (
  matrixPayload?.determinism &&
  typeof matrixPayload.determinism === "object" &&
  !Array.isArray(matrixPayload.determinism) &&
  Object.prototype.hasOwnProperty.call(matrixPayload.determinism, "parity_passed") &&
  matrixPayload.determinism.parity_passed !== true
) {
  fail(`INT-01 test matrix determinism.parity_passed must be true in ${resolvedMatrixPath}`);
}
NODE
  then
    fail "lane artifact contract violation for ${lane}: ${path}"
  fi
}

required_lanes=(
  "CUT-01" "CUT-02" "CUT-03"
  "MT-01" "MT-02" "MT-03" "PHY-01" "MMO-01" "CI-01"
  "MT-04" "MT-05" "RD-01" "PHY-02" "MMO-02" "CI-02"
  "RD-02" "RD-03" "RD-04" "PHY-03" "PHY-04" "MMO-03" "MMO-04" "CI-03"
  "INT-01" "INT-02"
)

for lane in "${required_lanes[@]}"; do
  lane_artifact_path="${ARTIFACT_ROOT}/${lane}/lane-artifact.json"
  require_file "${lane_artifact_path}" "lane artifact"
  validate_lane_artifact_contract "${lane}" "${lane_artifact_path}"
done

require_file "$REVIEW_REPORT_PATH" "independent review report"
require_file "$REVIEW_OUTCOME_PATH" "independent review outcome"

for section in "## Scope" "## Findings (P0-P2)" "## Verification"; do
  if ! rg -Fq "$section" "$REVIEW_REPORT_PATH"; then
    fail "review report missing required section '${section}'"
  fi
done

if ! rg -Fq "No open P0 findings." "$REVIEW_REPORT_PATH"; then
  fail "review report must explicitly state no open P0 findings"
fi
if ! rg -Fq "No open P1 findings." "$REVIEW_REPORT_PATH"; then
  fail "review report must explicitly state no open P1 findings"
fi

if ! rg -q -e '"blocking_open"[[:space:]]*:[[:space:]]*0([[:space:],}]|$)' "$REVIEW_OUTCOME_PATH"; then
  fail "review outcome must set blocking_open=0"
fi
if ! rg -q -e '"p0_open"[[:space:]]*:[[:space:]]*0([[:space:],}]|$)' "$REVIEW_OUTCOME_PATH"; then
  fail "review outcome must set p0_open=0"
fi
if ! rg -q -e '"p1_open"[[:space:]]*:[[:space:]]*0([[:space:],}]|$)' "$REVIEW_OUTCOME_PATH"; then
  fail "review outcome must set p1_open=0"
fi

forbidden_tokens=(
  "render-schema-v5"
  "shader-bundle-v5"
  "render-schema-v3"
  "shader-bundle-v3"
  "protocol-v3"
)
for token in "${forbidden_tokens[@]}"; do
  if rg -Fq "$token" "$ARTIFACT_ROOT"; then
    fail "legacy token detected in artifact root: ${token}"
  fi
done

cat > "$SUMMARY_PATH" <<'SUMMARY'
# AAA Engine Final Gate Summary

## Outcome

1. All required lanes present.
2. All lane artifacts satisfy schema v1 + passed status contract.
3. Independent review artifact present with no blocking findings.
4. No forbidden legacy schema/protocol tokens detected.
5. Final gate status: PASS.
SUMMARY

echo "aaa final gate: PASS"
