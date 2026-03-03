#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ARTIFACT_ROOT="${WRELA_ANIMATION_ARTIFACT_ROOT:-${ROOT}/.artifacts/animation-factory-pass}"
SUMMARY_DIR="${ARTIFACT_ROOT}/ANIM-199/final-gate"
SUMMARY_PATH="${SUMMARY_DIR}/summary.md"
REVIEW_REPORT_PATH="${ARTIFACT_ROOT}/ANIM-990/review-report.md"
REVIEW_OUTCOME_PATH="${ARTIFACT_ROOT}/ANIM-990/review-outcome.json"
CONTRACT_EVIDENCE_PATH="${ARTIFACT_ROOT}/ANIM-111/contract-evidence.json"

mkdir -p "$SUMMARY_DIR"

fail() {
  echo "animation final gate failed: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  local label="$2"
  if [[ ! -f "$path" ]]; then
    fail "missing ${label}: ${path}"
  fi
}

reject_legacy_contract_tokens() {
  local root="$1"
  local hits
  hits="$(rg -n -S --glob '*.json' --glob '*.jsonl' --glob '*.md' --glob '*.txt' \
    'character-bundle-manifest-v1|character-bundle-manifest-v2|animation-clip-bundle-v1|animation-clip-bundle-v2|animation-graph-contract-v1|animation-quality-report-v1|protocol-v1|protocol-v2|protocol-v3|protocol-v4' \
    "$root" || true)"
  if [[ -n "$hits" ]]; then
    fail "legacy token detected in animation artifacts: ${hits}"
  fi
}

validate_lane_artifact_contract() {
  local lane="$1"
  local path="$2"
  if ! node - "$path" "$lane" <<'NODE'
const fs = require("node:fs");

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
NODE
  then
    fail "lane artifact contract violation for ${lane}: ${path}"
  fi
}

validate_lane_evidence_contract() {
  local lane="$1"
  local path="$2"
  if ! node - "$path" "$lane" <<'NODE'
const fs = require("node:fs");

const [evidencePath, expectedLane] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let payload;
try {
  payload = JSON.parse(fs.readFileSync(evidencePath, "utf8"));
} catch (error) {
  fail(`invalid lane evidence json ${evidencePath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (Number(payload?.schema_version) !== 1) {
  fail(`lane evidence schema_version must equal 1 in ${evidencePath}`);
}
if (payload?.kind !== "animation-lane-evidence-v1") {
  fail(`lane evidence kind must be 'animation-lane-evidence-v1' in ${evidencePath}; got '${payload?.kind}'`);
}
if (payload?.lane !== expectedLane) {
  fail(`lane evidence lane mismatch in ${evidencePath}: expected '${expectedLane}' got '${payload?.lane}'`);
}
if (payload?.passed !== true) {
  fail(`lane evidence passed must be true in ${evidencePath}`);
}
const checks = Array.isArray(payload?.checks) ? payload.checks : [];
if (checks.length === 0) {
  fail(`lane evidence checks must be non-empty in ${evidencePath}`);
}
NODE
  then
    fail "lane evidence contract violation for ${lane}: ${path}"
  fi
}

require_review_outcome_contract() {
  local path="$1"
  if ! node - "$path" <<'NODE'
const fs = require("node:fs");

const [reviewPath] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let payload;
try {
  payload = JSON.parse(fs.readFileSync(reviewPath, "utf8"));
} catch (error) {
  fail(`invalid review outcome json ${reviewPath}: ${error instanceof Error ? error.message : String(error)}`);
}

for (const key of ["schema_version", "kind", "p0_open", "p1_open", "p2_open", "blocking_open"]) {
  if (!Object.prototype.hasOwnProperty.call(payload ?? {}, key)) {
    fail(`review outcome missing field '${key}' in ${reviewPath}`);
  }
}

if (Number(payload?.schema_version) !== 1) {
  fail(`review outcome must use schema_version=1 in ${reviewPath}`);
}
if (payload?.kind !== "animation-factory-review-outcome-v1") {
  fail(`review outcome kind must be 'animation-factory-review-outcome-v1' in ${reviewPath}`);
}
NODE
  then
    fail "review outcome contract is invalid"
  fi
}

require_clean_review_outcome() {
  local path="$1"
  if ! node - "$path" <<'NODE'
const fs = require("node:fs");

const [reviewPath] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let payload;
try {
  payload = JSON.parse(fs.readFileSync(reviewPath, "utf8"));
} catch (error) {
  fail(`invalid review outcome json ${reviewPath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (Number(payload?.p0_open) !== 0) {
  fail(`review outcome must set p0_open=0 in ${reviewPath}`);
}
if (Number(payload?.p1_open) !== 0) {
  fail(`review outcome must set p1_open=0 in ${reviewPath}`);
}
if (Number(payload?.p2_open) !== 0) {
  fail(`review outcome must set p2_open=0 in ${reviewPath}`);
}
if (Number(payload?.blocking_open) !== 0) {
  fail(`review outcome must set blocking_open=0 in ${reviewPath}`);
}

const summary = payload?.summary;
if (summary && typeof summary === "object" && !Array.isArray(summary)) {
  if (Object.prototype.hasOwnProperty.call(summary, "command_failures") && Number(summary.command_failures) !== 0) {
    fail(`review outcome summary.command_failures must equal 0 in ${reviewPath}`);
  }
  if (Object.prototype.hasOwnProperty.call(summary, "matrix_passed") && summary.matrix_passed !== true) {
    fail(`review outcome summary.matrix_passed must equal true in ${reviewPath}`);
  }
}
NODE
  then
    fail "independent review outcome is not clean"
  fi
}

require_contract_evidence_v5() {
  local path="$1"
  if ! node - "$path" <<'NODE'
const fs = require("node:fs");

const [evidencePath] = process.argv.slice(2);

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

let payload;
try {
  payload = JSON.parse(fs.readFileSync(evidencePath, "utf8"));
} catch (error) {
  fail(`invalid contract evidence json ${evidencePath}: ${error instanceof Error ? error.message : String(error)}`);
}

if (Number(payload?.schema_version) !== 2) {
  fail(`contract evidence schema_version must equal 2 in ${evidencePath}`);
}
if (payload?.kind !== "animation-contract-evidence-v2") {
  fail(`contract evidence kind must be 'animation-contract-evidence-v2' in ${evidencePath}; got '${payload?.kind}'`);
}
if (payload?.passed !== true) {
  fail(`contract evidence passed must be true in ${evidencePath}`);
}

const requiredVersions = {
  character_bundle_manifest: "character-bundle-manifest-v3",
  animation_clip_bundle: "animation-clip-bundle-v3",
  animation_graph_contract: "animation-graph-contract-v2",
  animation_quality_report: "animation-quality-report-v2",
  protocol: "protocol-v5",
};
const actual = payload?.contract_versions ?? {};
for (const [field, expected] of Object.entries(requiredVersions)) {
  if (actual[field] !== expected) {
    fail(`contract evidence ${field} must equal '${expected}' in ${evidencePath}; got '${actual[field]}'`);
  }
}
NODE
  then
    fail "animation contract evidence is invalid"
  fi
}

required_lanes=(
  "ANIM-101"
  "ANIM-102"
  "ANIM-103"
  "ANIM-104"
  "ANIM-105"
  "ANIM-106"
  "ANIM-107"
  "ANIM-108"
  "ANIM-109"
  "ANIM-110"
  "ANIM-111"
)

for lane in "${required_lanes[@]}"; do
  lane_artifact_path="${ARTIFACT_ROOT}/${lane}/lane-artifact.json"
  lane_evidence_path="${ARTIFACT_ROOT}/${lane}/lane-evidence.json"
  require_file "${lane_artifact_path}" "lane artifact"
  require_file "${lane_evidence_path}" "lane evidence"
  validate_lane_artifact_contract "${lane}" "${lane_artifact_path}"
  validate_lane_evidence_contract "${lane}" "${lane_evidence_path}"
done

require_file "$REVIEW_REPORT_PATH" "independent review report"
require_file "$REVIEW_OUTCOME_PATH" "independent review outcome"
require_file "$CONTRACT_EVIDENCE_PATH" "contract evidence"

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

require_review_outcome_contract "$REVIEW_OUTCOME_PATH"
require_clean_review_outcome "$REVIEW_OUTCOME_PATH"
require_contract_evidence_v5 "$CONTRACT_EVIDENCE_PATH"
reject_legacy_contract_tokens "$ARTIFACT_ROOT"

cat > "$SUMMARY_PATH" <<'SUMMARY'
# Animation Factory Final Gate Summary

## Outcome

1. All animation synthesis lanes present and marked passed.
2. Independent review artifacts present with no blocking findings.
3. Animation schema/protocol evidence confirms v3/v2 contracts and protocol-v5.
4. Final gate status: PASS.
SUMMARY

echo "animation final gate: PASS"
