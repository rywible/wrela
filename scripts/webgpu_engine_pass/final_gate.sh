#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ARTIFACT_ROOT="${WRELA_WEBGPU_ARTIFACT_ROOT:-${ROOT}/.artifacts/webgpu-engine-pass}"
WFE4_000_DIR="${ARTIFACT_ROOT}/WFE4-000"
WFE4_111_DIR="${ARTIFACT_ROOT}/WFE4-111"
WFE4_990_DIR="${ARTIFACT_ROOT}/WFE4-990"
FINAL_GATE_DIR="${ARTIFACT_ROOT}/WFE4-114/final-gate"
PLAN_LEDGER_PATH="${WFE4_000_DIR}/plan-ledger.json"
GATE_CONTRACT_PATH="${WFE4_000_DIR}/gate-contract.json"
ANTI_SHORTCUT_PATH="${WFE4_000_DIR}/anti-shortcut-report.txt"
SUMMARY_PATH="${FINAL_GATE_DIR}/summary.md"
REVIEW_REPORT_PATH="${WFE4_990_DIR}/review-report.md"
REVIEW_OUTCOME_PATH="${WFE4_990_DIR}/review-outcome.json"
CLI_GATE_LOG_PATH="${WFE4_111_DIR}/cli-gate-test.log"
CLI_GATE_MATRIX_PATH="${WFE4_111_DIR}/gate-failure-matrix.json"
METRICS_V2_PATH="${ARTIFACT_ROOT}/WFE4-110/runtime-metrics-v2.json"
GOVERNOR_TRACE_PATH="${ARTIFACT_ROOT}/WFE4-110/governor-action-trace.json"
AAA_ARTIFACT_ROOT="${WRELA_AAA_ARTIFACT_ROOT:-${ROOT}/.artifacts/aaa-forest-demo}"
AAA_LANE="${WRELA_AAA_LANE:-ORCH-00}"
AAA_ITERATION="${WRELA_AAA_ITERATION:-iteration-001}"
AAA_ITERATION_DIR="${AAA_ARTIFACT_ROOT}/${AAA_LANE}/${AAA_ITERATION}"
AAA_SMOKE_REPORT_PATH="${WRELA_AAA_SMOKE_REPORT_PATH:-${AAA_ITERATION_DIR}/smoke-report.json}"
AAA_ACTION_PAYLOAD_PATH="${WRELA_AAA_ACTION_PAYLOAD_PATH:-${ROOT}/scripts/webgpu_engine_pass/action_payloads.json}"

mkdir -p "${WFE4_000_DIR}" "${FINAL_GATE_DIR}"

fail() {
  echo "final gate failed: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  local label="$2"
  if [ ! -f "${path}" ]; then
    fail "missing ${label}: ${path}"
  fi
}

require_markdown_sections() {
  local path="$1"
  shift
  local section
  require_file "${path}" "report artifact"
  for section in "$@"; do
    if ! rg -Fq "${section}" "${path}"; then
      fail "missing required section '${section}' in ${path}"
    fi
  done
}

require_json_fields() {
  local path="$1"
  shift
  node - "${path}" "$@" <<'NODE'
const fs = require("node:fs");

const [path, ...fields] = process.argv.slice(2);
let parsed;
try {
  parsed = JSON.parse(fs.readFileSync(path, "utf8"));
} catch (error) {
  console.error(`invalid json artifact ${path}: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}

const readField = (value, dottedPath) => {
  return dottedPath.split(".").reduce((acc, key) => {
    if (acc === null || typeof acc !== "object" || !Object.prototype.hasOwnProperty.call(acc, key)) {
      return undefined;
    }
    return acc[key];
  }, value);
};

for (const field of fields) {
  if (readField(parsed, field) === undefined) {
    console.error(`missing required field '${field}' in ${path}`);
    process.exit(1);
  }
}
NODE
}

write_plan_ledger() {
  cat > "${PLAN_LEDGER_PATH}" <<'JSON'
{
  "schema_version": 1,
  "kind": "webgpu-engine-pass-plan-ledger-v2",
  "required_lane_sequence": [
    "WFE4-000",
    "WFE4-101",
    "WFE4-102",
    "WFE4-103",
    "WFE4-104",
    "WFE4-105",
    "WFE4-106",
    "WFE4-107",
    "WFE4-108",
    "WFE4-109",
    "WFE4-110",
    "WFE4-111",
    "WFE4-112",
    "WFE4-113",
    "WFE4-114"
  ],
  "lanes": [
    {
      "lane": "WFE4-000",
      "owner": "WL-00",
      "description": "final gate contract + anti-shortcut scanner hardening",
      "required_artifacts": [
        ".artifacts/webgpu-engine-pass/WFE4-000/plan-ledger.json",
        ".artifacts/webgpu-engine-pass/WFE4-000/gate-contract.json",
        ".artifacts/webgpu-engine-pass/WFE4-000/anti-shortcut-report.txt"
      ]
    },
    {
      "lane": "WFE4-111",
      "owner": "WL-11",
      "description": "CLI gate pass/fail + negative-case verification",
      "required_artifacts": [
        ".artifacts/webgpu-engine-pass/WFE4-111/cli-gate-test.log",
        ".artifacts/webgpu-engine-pass/WFE4-111/gate-failure-matrix.json"
      ]
    }
  ],
  "independent_review": {
    "lane": "WFE4-990",
    "required_artifacts": [
      ".artifacts/webgpu-engine-pass/WFE4-990/review-report.md",
      ".artifacts/webgpu-engine-pass/WFE4-990/review-outcome.json"
    ]
  }
}
JSON
}

write_gate_contract() {
  cat > "${GATE_CONTRACT_PATH}" <<'JSON'
{
  "schema_version": 1,
  "kind": "webgpu-engine-pass-gate-contract-v2",
  "required_lane_sequence": [
    "WFE4-000",
    "WFE4-101",
    "WFE4-102",
    "WFE4-103",
    "WFE4-104",
    "WFE4-105",
    "WFE4-106",
    "WFE4-107",
    "WFE4-108",
    "WFE4-109",
    "WFE4-110",
    "WFE4-111",
    "WFE4-112",
    "WFE4-113",
    "WFE4-114"
  ],
  "anti_shortcut": {
    "forbidden_schema_tokens": [
      "render-provenance-v1",
      "shader-provenance-v1",
      "render-schema-v3",
      "shader-bundle-v3",
      "asset-pack-v2",
      "world-chunk-v1",
      "legacy-smoke-report-v0"
    ]
  },
  "required_reports": {
    "cli_gate_matrix": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-111/gate-failure-matrix.json",
      "required_fields": [
        "schema_version",
        "kind",
        "generated_at",
        "cases",
        "cases.0.scenario",
        "cases.0.status",
        "cases.0.expected",
        "summary.passed",
        "summary.failed",
        "summary.skipped"
      ]
    },
    "independent_review": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-990/review-report.md",
      "required_sections": [
        "## Scope",
        "## Findings (P0-P2)",
        "## Verification"
      ]
    },
    "independent_review_outcome": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-990/review-outcome.json",
      "required_fields": [
        "schema_version",
        "kind",
        "p0_open",
        "p1_open",
        "p2_open",
        "blocking_open",
        "summary"
      ]
    },
    "runtime_metrics_v2": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-110/runtime-metrics-v2.json",
      "required_fields": [
        "schema_version",
        "kind",
        "frame_budget",
        "frame_budget.within_budget_frames",
        "frame_budget.over_budget_frames",
        "pass_timings",
        "governor",
        "governor.actions"
      ]
    },
    "governor_action_trace": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-110/governor-action-trace.json"
    },
    "final_gate_summary": {
      "path": ".artifacts/webgpu-engine-pass/WFE4-114/final-gate/summary.md",
      "required_sections": [
        "## Scope",
        "## Lane Validation",
        "## Anti-Shortcut Scan",
        "## AAA Forest Smoke Contract",
        "## Report Sections",
        "## Outcome"
      ]
    },
    "aaa_forest_smoke_report": {
      "path": ".artifacts/aaa-forest-demo/ORCH-00/iteration-001/smoke-report.json",
      "required_fields": [
        "schema_version",
        "kind",
        "appPath",
        "url",
        "artifact_root",
        "lane",
        "iteration",
        "preset_sequence",
        "required_preset_fields",
        "required_phase_report_fields",
        "passed",
        "failures",
        "phases",
        "phases.0.phase",
        "phases.0.relevantFailedAssertions",
        "runtime_metrics_v2",
        "governor_action_trace"
      ]
    },
    "aaa_forest_action_payloads": {
      "path": "scripts/webgpu_engine_pass/action_payloads.json",
      "required_fields": [
        "schema_version",
        "kind",
        "presets",
        "presets.idle_composition.meta.scenario",
        "presets.camera_orbit.meta.scenario",
        "presets.lock_toggle.meta.scenario",
        "presets.target_cycle.meta.scenario",
        "presets.dodge_parry_burst.meta.scenario",
        "presets.combo_burst.meta.scenario",
        "presets.death_restart_loop.meta.scenario"
      ]
    }
  }
}
JSON
}

write_plan_ledger
write_gate_contract

required_lanes=(
  "WFE4-000"
  "WFE4-101"
  "WFE4-102"
  "WFE4-103"
  "WFE4-104"
  "WFE4-105"
  "WFE4-106"
  "WFE4-107"
  "WFE4-108"
  "WFE4-109"
  "WFE4-110"
  "WFE4-111"
  "WFE4-112"
  "WFE4-113"
  "WFE4-114"
)

for lane in "${required_lanes[@]}"; do
  lane_dir="${ARTIFACT_ROOT}/${lane}"
  if [ ! -d "${lane_dir}" ]; then
    fail "missing required lane artifact directory ${lane_dir}"
  fi
  if [ "${lane}" = "WFE4-114" ]; then
    continue
  fi
  if ! find "${lane_dir}" -type f -print -quit | grep -q .; then
    fail "lane artifact directory is empty ${lane_dir}"
  fi
done

require_file "${CLI_GATE_LOG_PATH}" "CLI gate test log"
require_file "${CLI_GATE_MATRIX_PATH}" "CLI gate failure matrix"
require_file "${REVIEW_REPORT_PATH}" "independent review report"

require_json_fields "${CLI_GATE_MATRIX_PATH}" \
  "schema_version" \
  "kind" \
  "generated_at" \
  "cases" \
  "cases.0.scenario" \
  "cases.0.status" \
  "cases.0.expected" \
  "summary.passed" \
  "summary.failed" \
  "summary.skipped"

node - "${CLI_GATE_MATRIX_PATH}" <<'NODE'
const fs = require("node:fs");
const path = process.argv[2];
const parsed = JSON.parse(fs.readFileSync(path, "utf8"));
const required = new Map([
  ["cli_webgpu_final_gate_fails_fast_when_lane_artifact_missing", "failed"],
  ["cli_webgpu_final_gate_requires_independent_review_artifact", "failed"],
  ["cli_webgpu_final_gate_rejects_legacy_schema_tokens_in_accepted_paths", "failed"],
  ["cli_webgpu_final_gate_passes_with_complete_lane_artifacts", "passed"],
]);
const byScenario = new Map(
  (Array.isArray(parsed.cases) ? parsed.cases : []).map((entry) => [entry.scenario, entry])
);
for (const [scenario, expectedStatus] of required) {
  if (!byScenario.has(scenario)) {
    console.error(`missing required matrix scenario '${scenario}' in ${path}`);
    process.exit(1);
  }
  const entry = byScenario.get(scenario);
  if (entry.expected !== expectedStatus || entry.status !== expectedStatus) {
    console.error(
      `matrix scenario '${scenario}' expected/status mismatch: expected=${expectedStatus} actualExpected=${entry.expected} actualStatus=${entry.status}`
    );
    process.exit(1);
  }
}
NODE

require_markdown_sections "${REVIEW_REPORT_PATH}" \
  "## Scope" \
  "## Findings (P0-P2)" \
  "## Verification"

require_file "${REVIEW_OUTCOME_PATH}" "independent review outcome json"
require_json_fields "${REVIEW_OUTCOME_PATH}" \
  "schema_version" \
  "kind" \
  "p0_open" \
  "p1_open" \
  "p2_open" \
  "blocking_open" \
  "summary"

node - "${REVIEW_OUTCOME_PATH}" <<'NODE'
const fs = require("node:fs");
const path = process.argv[2];
const parsed = JSON.parse(fs.readFileSync(path, "utf8"));
const fail = (message) => {
  console.error(message);
  process.exit(1);
};
if (Number(parsed.p0_open ?? 0) > 0) fail(`review outcome has open P0 findings in ${path}`);
if (Number(parsed.p1_open ?? 0) > 0) fail(`review outcome has open P1 findings in ${path}`);
if (Number(parsed.blocking_open ?? 0) > 0) fail(`review outcome has blocking findings in ${path}`);
NODE

require_file "${METRICS_V2_PATH}" "runtime metrics v2 artifact"
require_file "${GOVERNOR_TRACE_PATH}" "governor action trace artifact"

node - "${METRICS_V2_PATH}" "${GOVERNOR_TRACE_PATH}" <<'NODE'
const fs = require("node:fs");
const [metricsPath, governorPath] = process.argv.slice(2);
const metrics = JSON.parse(fs.readFileSync(metricsPath, "utf8"));
const governorTrace = JSON.parse(fs.readFileSync(governorPath, "utf8"));
const fail = (message) => {
  console.error(message);
  process.exit(1);
};

if (metrics.kind !== "runtime-metrics-v2") fail(`unexpected metrics kind in ${metricsPath}`);
if (Number(metrics.schema_version) !== 2) fail(`unexpected metrics schema_version in ${metricsPath}`);

const budget = metrics.frame_budget ?? {};
const within = Number(budget.within_budget_frames ?? 0);
const over = Number(budget.over_budget_frames ?? 0);
if (within + over <= 0) fail(`placeholder frame budget counters in ${metricsPath}`);

const passTimings = Array.isArray(metrics.pass_timings) ? metrics.pass_timings : [];
if (passTimings.length === 0) fail(`missing pass timing evidence in ${metricsPath}`);

const governor = metrics.governor ?? {};
const adaptationCount = Number(governor.adaptation_count ?? 0);
const actions = Array.isArray(governor.actions) ? governor.actions : [];
if (adaptationCount <= 0 && actions.length === 0) {
  fail(`missing governor adaptation evidence in ${metricsPath}`);
}

if (!Array.isArray(governorTrace) || governorTrace.length === 0) {
  fail(`missing governor action trace entries in ${governorPath}`);
}
NODE

forest_required_presets=(
  "idle_composition"
  "camera_orbit"
  "lock_toggle"
  "target_cycle"
  "dodge_parry_burst"
  "combo_burst"
  "death_restart_loop"
)

require_file "${AAA_ACTION_PAYLOAD_PATH}" "AAA forest action payloads"
require_json_fields "${AAA_ACTION_PAYLOAD_PATH}" \
  "schema_version" \
  "kind" \
  "presets" \
  "presets.idle_composition.meta.scenario" \
  "presets.camera_orbit.meta.scenario" \
  "presets.lock_toggle.meta.scenario" \
  "presets.target_cycle.meta.scenario" \
  "presets.dodge_parry_burst.meta.scenario" \
  "presets.combo_burst.meta.scenario" \
  "presets.death_restart_loop.meta.scenario"

node - "${AAA_ACTION_PAYLOAD_PATH}" "${forest_required_presets[@]}" <<'NODE'
const fs = require("node:fs");
const [payloadPath, ...requiredPresets] = process.argv.slice(2);
const parsed = JSON.parse(fs.readFileSync(payloadPath, "utf8"));
const fail = (message) => {
  console.error(message);
  process.exit(1);
};
if (Number(parsed.schema_version) !== 2) {
  fail(`unexpected action payload schema_version in ${payloadPath}`);
}
if (parsed.kind !== "aaa-forest-action-payloads-v1") {
  fail(`unexpected action payload kind in ${payloadPath}`);
}
const presets = parsed?.presets;
if (!presets || typeof presets !== "object") {
  fail(`missing presets object in ${payloadPath}`);
}
for (const presetName of requiredPresets) {
  const preset = presets[presetName];
  if (!preset || typeof preset !== "object") {
    fail(`missing required preset '${presetName}' in ${payloadPath}`);
  }
  if (preset?.meta?.scenario !== presetName) {
    fail(`preset '${presetName}' meta.scenario mismatch in ${payloadPath}`);
  }
  if (!Array.isArray(preset.steps) || preset.steps.length === 0) {
    fail(`preset '${presetName}' missing non-empty steps in ${payloadPath}`);
  }
  if (!preset.expect || typeof preset.expect !== "object") {
    fail(`preset '${presetName}' missing expect object in ${payloadPath}`);
  }
}
NODE

require_file "${AAA_SMOKE_REPORT_PATH}" "AAA forest smoke report"
require_json_fields "${AAA_SMOKE_REPORT_PATH}" \
  "schema_version" \
  "kind" \
  "appPath" \
  "url" \
  "artifact_root" \
  "lane" \
  "iteration" \
  "preset_sequence" \
  "required_preset_fields" \
  "required_phase_report_fields" \
  "passed" \
  "failures" \
  "phases" \
  "phases.0.phase" \
  "phases.0.relevantFailedAssertions" \
  "runtime_metrics_v2" \
  "governor_action_trace"

node - "${AAA_SMOKE_REPORT_PATH}" "${AAA_ARTIFACT_ROOT}" "${AAA_LANE}" "${AAA_ITERATION}" "${forest_required_presets[@]}" <<'NODE'
const fs = require("node:fs");
const [reportPath, expectedRoot, expectedLane, expectedIteration, ...requiredPresets] = process.argv.slice(2);
const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const fail = (message) => {
  console.error(message);
  process.exit(1);
};
if (Number(report.schema_version) !== 3) {
  fail(`unexpected smoke report schema_version in ${reportPath}`);
}
if (report.kind !== "aaa-forest-browser-smoke-report-v3") {
  fail(`unexpected smoke report kind in ${reportPath}`);
}
if (report.artifact_root !== expectedRoot) {
  fail(`smoke report artifact_root mismatch in ${reportPath}`);
}
if (report.lane !== expectedLane) {
  fail(`smoke report lane mismatch in ${reportPath}`);
}
if (report.iteration !== expectedIteration) {
  fail(`smoke report iteration mismatch in ${reportPath}`);
}
if (report.passed !== true) {
  fail(`smoke report marked failed in ${reportPath}`);
}
const presetSequence = Array.isArray(report.preset_sequence) ? report.preset_sequence : [];
for (const preset of requiredPresets) {
  if (!presetSequence.includes(preset)) {
    fail(`smoke report missing preset '${preset}' in preset_sequence`);
  }
}
const requiredPresetFields = Array.isArray(report.required_preset_fields)
  ? report.required_preset_fields
  : [];
for (const field of ["meta.scenario", "meta.category", "expect.minTickDelta", "steps"]) {
  if (!requiredPresetFields.includes(field)) {
    fail(`smoke report missing required_preset_fields entry '${field}'`);
  }
}
const requiredPhaseFields = Array.isArray(report.required_phase_report_fields)
  ? report.required_phase_report_fields
  : [];
for (const field of ["schemaVersion", "actionPlan.preset", "assertions.failed", "strictExitCode"]) {
  if (!requiredPhaseFields.includes(field)) {
    fail(`smoke report missing required_phase_report_fields entry '${field}'`);
  }
}
const phases = Array.isArray(report.phases) ? report.phases : [];
if (phases.length < requiredPresets.length) {
  fail(`smoke report phase count too low in ${reportPath}`);
}
for (const preset of requiredPresets) {
  const phase = phases.find((entry) => entry?.phase === preset);
  if (!phase) {
    fail(`smoke report missing phase '${preset}' in ${reportPath}`);
  }
  if (Number(phase.strictExitCode ?? 1) !== 0) {
    fail(`smoke report phase '${preset}' strictExitCode is non-zero`);
  }
  if (Number(phase.relevantFailedAssertions ?? 0) !== 0) {
    fail(`smoke report phase '${preset}' has failed assertions`);
  }
}
NODE

scan_tokens=(
  "render-provenance-v1"
  "shader-provenance-v1"
  "render-schema-v3"
  "shader-bundle-v3"
  "asset-pack-v2"
  "world-chunk-v1"
  "legacy-smoke-report-v0"
)

accepted_paths=(
  "${ARTIFACT_ROOT}/WFE4-101"
  "${ARTIFACT_ROOT}/WFE4-102"
  "${ARTIFACT_ROOT}/WFE4-103"
  "${ARTIFACT_ROOT}/WFE4-104"
  "${ARTIFACT_ROOT}/WFE4-105"
  "${ARTIFACT_ROOT}/WFE4-106"
  "${ARTIFACT_ROOT}/WFE4-107"
  "${ARTIFACT_ROOT}/WFE4-108"
  "${ARTIFACT_ROOT}/WFE4-109"
  "${ARTIFACT_ROOT}/WFE4-110"
  "${WFE4_111_DIR}"
  "${ARTIFACT_ROOT}/WFE4-112"
  "${ARTIFACT_ROOT}/WFE4-113"
)

candidate_files=()
while IFS= read -r file; do
  candidate_files+=("${file}")
done < <(
  find "${accepted_paths[@]}" -type f \
    \( -name "*.json" -o -name "*.jsonl" -o -name "*.yaml" -o -name "*.yml" -o -name "*.csv" -o -name "*.tsv" \) | sort
)

scan_tmp="$(mktemp)"
trap 'rm -f "${scan_tmp}"' EXIT
: > "${scan_tmp}"

for token in "${scan_tokens[@]}"; do
  for file in "${candidate_files[@]}"; do
    if rg -n -F "${token}" "${file}" >/dev/null 2>&1; then
      rg -n -F "${token}" "${file}" >> "${scan_tmp}"
    fi
  done
done

scan_status="passed"
if [ -s "${scan_tmp}" ]; then
  scan_status="failed"
fi

{
  echo "WFE4 anti-shortcut scan report"
  echo "status: ${scan_status}"
  echo "artifact_root: ${ARTIFACT_ROOT}"
  echo "scanned_file_count: ${#candidate_files[@]}"
  echo "forbidden_tokens:"
  for token in "${scan_tokens[@]}"; do
    echo "  - ${token}"
  done
  echo "accepted_paths:"
  for path in "${accepted_paths[@]}"; do
    echo "  - ${path}"
  done
  echo "findings:"
  if [ -s "${scan_tmp}" ]; then
    sed 's/^/  /' "${scan_tmp}"
  else
    echo "  none"
  fi
} > "${ANTI_SHORTCUT_PATH}"

if [ "${scan_status}" != "passed" ]; then
  fail "anti-shortcut scanner found forbidden legacy schema IDs in accepted paths; see ${ANTI_SHORTCUT_PATH}"
fi

cat > "${SUMMARY_PATH}" <<SUM
# WFE4-114 Final Gate Summary

## Scope

1. Enforce WFE4 hard-cut lane artifact contract for WFE4-000/101/102/103/.../114.
2. Enforce mandatory independent review artifact presence and required report sections.
3. Run anti-shortcut scanner against accepted WFE4 artifact paths.

## Lane Validation

1. Required lanes present with at least one artifact file each:
   $(printf '%s ' "${required_lanes[@]}")
2. Required WL-11 artifacts present:
   - ${CLI_GATE_LOG_PATH}
   - ${CLI_GATE_MATRIX_PATH}
3. Required independent review artifact present:
   - ${REVIEW_REPORT_PATH}

## Anti-Shortcut Scan

1. Scanned files: ${#candidate_files[@]}
2. Report: ${ANTI_SHORTCUT_PATH}
3. Result: PASS

## AAA Forest Smoke Contract

1. Action payload contract: ${AAA_ACTION_PAYLOAD_PATH}
2. Smoke report contract: ${AAA_SMOKE_REPORT_PATH}
3. Required preset sequence:
   $(printf '%s ' "${forest_required_presets[@]}")
4. Artifact lane/iteration: ${AAA_LANE}/${AAA_ITERATION}

## Report Sections

1. Independent review required headings present.
2. Independent review required no-open-findings lines present.
3. CLI gate failure matrix required fields present.
4. AAA forest smoke report required fields present.
5. AAA action payload preset metadata required fields present.

## Outcome

1. Final gate PASS.
SUM

require_markdown_sections "${SUMMARY_PATH}" \
  "## Scope" \
  "## Lane Validation" \
  "## Anti-Shortcut Scan" \
  "## AAA Forest Smoke Contract" \
  "## Report Sections" \
  "## Outcome"

if ! find "${ARTIFACT_ROOT}/WFE4-114" -type f -print -quit | grep -q .; then
  fail "lane artifact directory is empty ${ARTIFACT_ROOT}/WFE4-114"
fi

echo "final gate summary: ${SUMMARY_PATH}"
