#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP_PATH="${1:-apps/wrela-forest}"
AAA_ARTIFACT_ROOT="${WRELA_AAA_ARTIFACT_ROOT:-${ROOT_DIR}/.artifacts/aaa-forest-demo}"
AAA_LANE="${WRELA_AAA_LANE:-ORCH-00}"
AAA_ITERATION="${WRELA_AAA_ITERATION:-iteration-001}"
DEFAULT_TASK_ARTIFACT_DIR="${AAA_ARTIFACT_ROOT}/${AAA_LANE}/${AAA_ITERATION}"
TASK_ARTIFACT_DIR="${2:-${WRELA_AAA_TASK_ARTIFACT_DIR:-${DEFAULT_TASK_ARTIFACT_DIR}}}"
BIND_ADDR="${3:-${WRELA_GAME_BIND_ADDR:-127.0.0.1:8091}}"
SMOKE_URL="http://${BIND_ADDR}/"

PROTOCOL_ARTIFACT_ROOT="${TASK_ARTIFACT_DIR}/protocol"
PLAYWRIGHT_ARTIFACT_DIR="${TASK_ARTIFACT_DIR}/playwright"
SCREENSHOT_PATH="${TASK_ARTIFACT_DIR}/smoke.png"
REPORT_PATH="${TASK_ARTIFACT_DIR}/smoke-report.json"
SERVER_LOG_PATH="${TASK_ARTIFACT_DIR}/server.log"
BROWSER_LOG_PATH="${TASK_ARTIFACT_DIR}/browser.log"
WEB_GAME_SKILL_DIR="${CODEX_HOME:-$HOME/.codex}/skills/develop-web-game"
WEB_GAME_CLIENT_PATH="${WEB_GAME_SKILL_DIR}/scripts/web_game_playwright_client.js"
DEFAULT_WEB_GAME_ACTIONS_PATH="${ROOT_DIR}/scripts/webgpu_engine_pass/action_payloads.json"
WEB_GAME_ACTIONS_PATH="${WEB_GAME_ACTIONS:-${DEFAULT_WEB_GAME_ACTIONS_PATH}}"
PW_BROWSER="${WRELA_SMOKE_BROWSER:-chromium}"
PW_HEADLESS="${WRELA_SMOKE_HEADLESS:-false}"
PW_ALLOW_HEADLESS_WEBGPU="${WRELA_SMOKE_ALLOW_HEADLESS_WEBGPU:-false}"

ARTIFACT_LANE="${AAA_LANE}"
ARTIFACT_ITERATION="${AAA_ITERATION}"
if [[ "${TASK_ARTIFACT_DIR}" == "${AAA_ARTIFACT_ROOT}"/* ]]; then
  RELATIVE_ARTIFACT_PATH="${TASK_ARTIFACT_DIR#${AAA_ARTIFACT_ROOT}/}"
  RELATIVE_LANE="${RELATIVE_ARTIFACT_PATH%%/*}"
  RELATIVE_REST="${RELATIVE_ARTIFACT_PATH#*/}"
  RELATIVE_ITERATION="${RELATIVE_REST%%/*}"
  if [ -n "${RELATIVE_LANE}" ] && [ "${RELATIVE_LANE}" != "${RELATIVE_ARTIFACT_PATH}" ]; then
    ARTIFACT_LANE="${RELATIVE_LANE}"
  fi
  if [ -n "${RELATIVE_ITERATION}" ] && [ "${RELATIVE_ITERATION}" != "${RELATIVE_REST}" ]; then
    ARTIFACT_ITERATION="${RELATIVE_ITERATION}"
  fi
fi

fail() {
  echo "browser smoke failed: $*" >&2
  exit 1
}

mkdir -p "${PROTOCOL_ARTIFACT_ROOT}" "${PLAYWRIGHT_ARTIFACT_DIR}"
rm -f "${SCREENSHOT_PATH}" "${REPORT_PATH}" "${SERVER_LOG_PATH}" "${BROWSER_LOG_PATH}"
rm -rf "${PLAYWRIGHT_ARTIFACT_DIR:?}/"*

cd "${ROOT_DIR}"

cargo run -p wrela -- game build "${APP_PATH}" --target=dual --render=webgpu --host=pure-wasm --client-runtime=compiled --shader-provenance --no-shortcuts >/dev/null

WRELA_GAME_ARTIFACT_DIR="${PROTOCOL_ARTIFACT_ROOT}" \
WRELA_GAME_BIND_ADDR="${BIND_ADDR}" \
cargo run -p wrela -- game run "${APP_PATH}" --render=webgpu --host=pure-wasm >"${SERVER_LOG_PATH}" 2>&1 &
SERVER_PID=$!

cleanup() {
  kill "${SERVER_PID}" >/dev/null 2>&1 || true
  wait "${SERVER_PID}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in $(seq 1 120); do
  if ! kill -0 "${SERVER_PID}" >/dev/null 2>&1; then
    fail "game server exited before ready; see ${SERVER_LOG_PATH}"
  fi
  if rg -n "failed to bind game slice server|Address already in use" "${SERVER_LOG_PATH}" >/dev/null 2>&1; then
    fail "game server failed to bind ${BIND_ADDR}; see ${SERVER_LOG_PATH}"
  fi
  if curl -fsS "${SMOKE_URL}" >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done

if ! curl -fsS "${SMOKE_URL}" >/dev/null 2>&1; then
  fail "server never became ready for ${APP_PATH} at ${SMOKE_URL}; see ${SERVER_LOG_PATH}"
fi

if [ ! -f "${WEB_GAME_CLIENT_PATH}" ]; then
  fail "skill Playwright client missing at ${WEB_GAME_CLIENT_PATH}"
fi
if [ ! -f "${WEB_GAME_ACTIONS_PATH}" ]; then
  fail "action payload file missing at ${WEB_GAME_ACTIONS_PATH}"
fi

if [ ! -d "${WEB_GAME_SKILL_DIR}/node_modules/playwright" ]; then
  npm --prefix "${WEB_GAME_SKILL_DIR}" install --silent --no-fund --no-audit
fi

if ! ls "${ROOT_DIR}/.cache/ms-playwright"/chromium-* >/dev/null 2>&1; then
  PLAYWRIGHT_BROWSERS_PATH="${ROOT_DIR}/.cache/ms-playwright" \
  npx --prefix "${WEB_GAME_SKILL_DIR}" playwright install chromium >/dev/null
fi

if [[ "${APP_PATH}" != *"forest"* ]]; then
  fail "hard-cut smoke harness now supports forest demo app paths only; received '${APP_PATH}'"
fi

PRESETS=(
  "idle_composition"
  "camera_orbit"
  "lock_toggle"
  "target_cycle"
  "dodge_parry_burst"
  "combo_burst"
  "death_restart_loop"
)
PRESET_FIELDS=(
  "meta.scenario"
  "meta.lane"
  "meta.category"
  "meta.focus"
  "expect.minTickDelta"
  "expect.minDrawCallsDelta"
  "expect.requireHashChange"
  "steps"
)
PRESETS_CSV="$(IFS=,; echo "${PRESETS[*]}")"
PRESET_FIELDS_CSV="$(IFS=,; echo "${PRESET_FIELDS[*]}")"

if ! node -e '
const fs = require("node:fs");
const file = process.argv[1];
const expected = process.argv[2].split(",").filter(Boolean);
const requiredFields = process.argv[3].split(",").filter(Boolean);
let parsed;
try {
  parsed = JSON.parse(fs.readFileSync(file, "utf8"));
} catch (error) {
  console.error(`invalid actions file: ${file}: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
}
const presets = parsed && typeof parsed === "object" ? parsed.presets : null;
if (!presets || typeof presets !== "object") {
  console.error(`actions file missing presets object: ${file}`);
  process.exit(1);
}
const missing = expected.filter((name) => !Object.prototype.hasOwnProperty.call(presets, name));
if (missing.length > 0) {
  console.error(`actions file missing required preset(s): ${missing.join(", ")}`);
  process.exit(1);
}
const getPathValue = (root, pointer) => {
  const keys = pointer.split(".");
  let current = root;
  for (const key of keys) {
    if (!current || typeof current !== "object" || !(key in current)) {
      return undefined;
    }
    current = current[key];
  }
  return current;
};
for (const presetName of expected) {
  const preset = presets[presetName];
  for (const field of requiredFields) {
    const value = getPathValue(preset, field);
    if (value === undefined) {
      console.error(`actions preset ${presetName} missing required field ${field}`);
      process.exit(1);
    }
  }
  if (!Array.isArray(preset.steps) || preset.steps.length === 0) {
    console.error(`actions preset ${presetName} must declare non-empty steps`);
    process.exit(1);
  }
}
' "${WEB_GAME_ACTIONS_PATH}" "${PRESETS_CSV}" "${PRESET_FIELDS_CSV}"; then
  fail "actions file does not define required presets for ${APP_PATH}; see ${WEB_GAME_ACTIONS_PATH}"
fi

PHASE_REPORTS=()
for PRESET in "${PRESETS[@]}"; do
  PHASE_DIR="${PLAYWRIGHT_ARTIFACT_DIR}/${PRESET}"
  PHASE_REPORT="${PHASE_DIR}/report.json"
  mkdir -p "${PHASE_DIR}"
  if ! PLAYWRIGHT_BROWSERS_PATH="${ROOT_DIR}/.cache/ms-playwright" \
    node "${WEB_GAME_CLIENT_PATH}" \
      --url "${SMOKE_URL}" \
      --browser "${PW_BROWSER}" \
      --headless "${PW_HEADLESS}" \
      --background true \
      --allow-headless-webgpu "${PW_ALLOW_HEADLESS_WEBGPU}" \
      --strict false \
      --fail-on-near-blank true \
      --fail-on-diagnostic-errors true \
      --fail-on-requestfailed true \
      --iterations 2 \
      --pause-ms 280 \
      --actions-file "${WEB_GAME_ACTIONS_PATH}" \
      --action-preset "${PRESET}" \
      --screenshot-dir "${PHASE_DIR}" \
      --report-file "${PHASE_REPORT}" >>"${BROWSER_LOG_PATH}" 2>&1; then
    fail "Playwright phase '${PRESET}' failed for ${APP_PATH}; see ${BROWSER_LOG_PATH}"
  fi
  PHASE_REPORTS+=("${PHASE_REPORT}")
done

if [ "${#PHASE_REPORTS[@]}" -eq 0 ]; then
  fail "no Playwright phase reports were generated"
fi

if ! node - "${REPORT_PATH}" "${SCREENSHOT_PATH}" "${APP_PATH}" "${SMOKE_URL}" "${AAA_ARTIFACT_ROOT}" "${ARTIFACT_LANE}" "${ARTIFACT_ITERATION}" "${PRESETS_CSV}" "${PRESET_FIELDS_CSV}" "${PHASE_REPORTS[@]}" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [
  reportPath,
  screenshotPath,
  appPath,
  smokeUrl,
  artifactRoot,
  artifactLane,
  artifactIteration,
  presetCsv,
  presetFieldsCsv,
  ...phaseReports
] = process.argv.slice(2);
const failures = [];
const phases = [];
let finalShot = null;
const requiredPresetNames = presetCsv.split(",").filter(Boolean);
const requiredPresetFields = presetFieldsCsv.split(",").filter(Boolean);

const asNumber = (value) => {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
};

const getPathValue = (root, pointer) => {
  const keys = pointer.split(".");
  let current = root;
  for (const key of keys) {
    if (!current || typeof current !== "object" || !(key in current)) {
      return undefined;
    }
    current = current[key];
  }
  return current;
};

const validateRuntimeMetricsV2 = (metricsV2, phaseName) => {
  const errors = [];
  if (!metricsV2 || typeof metricsV2 !== "object") {
    errors.push(`phase '${phaseName}' missing metrics.runtime_metrics_v2 object`);
    return errors;
  }
  const requiredChecks = [
    ["schema_version", (value) => Number(value) === 2],
    ["kind", (value) => value === "runtime-metrics-v2"],
    ["pass_timings_supported", (value) => typeof value === "boolean"],
    ["pass_timing_fallback_used", (value) => typeof value === "boolean"],
    ["pass_timings", (value) => Array.isArray(value)],
    ["frame_budget.long_frame_count", (value) => Number.isFinite(Number(value))],
    ["frame_budget.hitch_count", (value) => Number.isFinite(Number(value))],
    ["frame_budget.last_outcome.within_budget", (value) => typeof value === "boolean"],
    ["governor.initialized_from_contracts", (value) => typeof value === "boolean"],
    ["governor.bounds.target_frame_time_ms", (value) => Number.isFinite(Number(value))],
    ["governor.budgets.dynamic_resolution_scale", (value) => Number.isFinite(Number(value))],
    ["governor.budgets.shadow_quality_tier", (value) => Number.isFinite(Number(value))],
    ["governor.budgets.ssr_quality_tier", (value) => Number.isFinite(Number(value))],
    ["governor.budgets.probe_update_rate", (value) => Number.isFinite(Number(value))],
    ["governor.budgets.volumetric_steps", (value) => Number.isFinite(Number(value))],
    ["governor.actions", (value) => Array.isArray(value)],
  ];
  for (const [pointer, check] of requiredChecks) {
    const value = getPathValue(metricsV2, pointer);
    if (!check(value)) {
      errors.push(`phase '${phaseName}' runtime_metrics_v2 missing/invalid '${pointer}'`);
    }
  }
  return errors;
};

const requiredPhaseReportFields = [
  "schemaVersion",
  "config.url",
  "config.actionPreset",
  "actionPlan.source",
  "actionPlan.preset",
  "actionPlan.stepCount",
  "assertions.total",
  "assertions.failed",
  "diagnostics.eventCount",
  "diagnostics.errorEventCount",
  "status",
  "strictExitCode",
  "iterations",
];

const missingFieldPaths = (root, fields) => {
  const missing = [];
  for (const field of fields) {
    if (getPathValue(root, field) === undefined) {
      missing.push(field);
    }
  }
  return missing;
};

const combatCounterPathCandidates = {
  lock_toggles: [
    "combat_events.lock_toggles",
    "combat_camera.combat_events.lock_toggles",
    "combat_camera.event_counters.lock_toggles",
    "combat_camera.lock_toggles",
  ],
  target_cycles: [
    "combat_events.target_cycles",
    "combat_camera.combat_events.target_cycles",
    "combat_camera.event_counters.target_cycles",
    "combat_camera.target_cycles",
  ],
  light_attacks: [
    "combat_events.attack_light",
    "combat_events.light_attacks",
    "combat_camera.combat_events.light_attacks",
    "combat_camera.event_counters.light_attacks",
    "combat_camera.light_attacks",
  ],
  heavy_attacks: [
    "combat_events.attack_heavy",
    "combat_events.heavy_attacks",
    "combat_camera.combat_events.heavy_attacks",
    "combat_camera.event_counters.heavy_attacks",
    "combat_camera.heavy_attacks",
  ],
  parries: [
    "combat_events.parry",
    "combat_events.parries",
    "combat_camera.combat_events.parries",
    "combat_camera.event_counters.parries",
    "combat_camera.parries",
  ],
  dodges: [
    "combat_events.dodge",
    "combat_events.dodges",
    "combat_camera.combat_events.dodges",
    "combat_camera.event_counters.dodges",
    "combat_camera.dodges",
  ],
  deaths: [
    "combat_events.deaths",
    "combat_camera.combat_events.deaths",
    "combat_camera.event_counters.deaths",
    "combat_camera.deaths",
  ],
  restarts: [
    "combat_events.restarts",
    "combat_camera.combat_events.restarts",
    "combat_camera.event_counters.restarts",
    "combat_camera.restarts",
  ],
};

const loadPhaseStateSnapshots = (phaseName, iterations) => {
  const snapshots = [];
  for (let index = 0; index < iterations.length; index += 1) {
    const iter = iterations[index];
    const statePath = iter?.statePath;
    if (typeof statePath !== "string" || statePath.length === 0) {
      failures.push(`phase '${phaseName}' iteration[${index}] missing statePath`);
      continue;
    }
    if (!fs.existsSync(statePath)) {
      failures.push(`phase '${phaseName}' missing state snapshot: ${statePath}`);
      continue;
    }
    try {
      const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
      snapshots.push({ index, statePath, state });
    } catch (error) {
      failures.push(
        `phase '${phaseName}' failed to parse state snapshot '${statePath}': ${
          error instanceof Error ? error.message : String(error)
        }`
      );
    }
  }
  return snapshots;
};

const readCounterSeries = (phaseName, snapshots, counterKey) => {
  const candidates = combatCounterPathCandidates[counterKey] ?? [];
  if (candidates.length === 0) {
    return {
      ok: false,
      error: `phase '${phaseName}' has no telemetry path candidates configured for '${counterKey}'`,
    };
  }
  const values = [];
  let selectedPath = null;
  for (const snapshot of snapshots) {
    let matched = null;
    for (const pointer of candidates) {
      const numeric = asNumber(getPathValue(snapshot.state, pointer));
      if (numeric != null) {
        matched = { pointer, value: numeric };
        break;
      }
    }
    if (!matched) {
      return {
        ok: false,
        error: `phase '${phaseName}' missing required telemetry '${counterKey}' in state '${snapshot.statePath}'`,
      };
    }
    values.push(matched.value);
    if (!selectedPath) {
      selectedPath = matched.pointer;
    }
  }
  if (values.length < 2) {
    return {
      ok: false,
      error: `phase '${phaseName}' requires at least 2 snapshots to compute '${counterKey}' delta`,
    };
  }
  return {
    ok: true,
    counterKey,
    path: selectedPath,
    values,
    delta: values[values.length - 1] - values[0],
  };
};

const evaluatePhaseSemantics = (phaseName, snapshots) => {
  const semanticFailures = [];
  const counters = {};
  const details = {};
  if (snapshots.length < 2) {
    semanticFailures.push(
      `phase '${phaseName}' requires at least 2 state snapshots for semantic validation`
    );
    return { failures: semanticFailures, counters, details };
  }

  const requireCounterDelta = (counterKey, label) => {
    const series = readCounterSeries(phaseName, snapshots, counterKey);
    if (!series.ok) {
      semanticFailures.push(series.error);
      return null;
    }
    const maxValue = Math.max(...series.values);
    counters[counterKey] = {
      path: series.path,
      values: series.values,
      delta: series.delta,
      max: maxValue,
    };
    if (!(series.delta > 0 || maxValue > 0)) {
      semanticFailures.push(
        `phase '${phaseName}' expected ${label} activity but observed values=${series.values.join(
          "->"
        )}`
      );
    }
    return series;
  };

  if (phaseName === "lock_toggle") {
    requireCounterDelta("lock_toggles", "lock toggle");
    const lockStates = [];
    for (const snapshot of snapshots) {
      const flag = getPathValue(snapshot.state, "combat_camera.lock_on_active");
      if (typeof flag !== "boolean") {
        semanticFailures.push(
          `phase '${phaseName}' missing required telemetry 'combat_camera.lock_on_active' in '${snapshot.statePath}'`
        );
        continue;
      }
      lockStates.push(flag);
    }
    details.lock_on_active_observed = lockStates.some(Boolean);
    if (!details.lock_on_active_observed) {
      semanticFailures.push(
        `phase '${phaseName}' expected lock-on activation but never observed lock_on_active=true`
      );
    }
  }

  if (phaseName === "target_cycle") {
    requireCounterDelta("target_cycles", "target cycle");
    const enemyCounts = [];
    const renderedEnemyCounts = [];
    for (const snapshot of snapshots) {
      const value = asNumber(getPathValue(snapshot.state, "combat_camera.enemy_count"));
      if (value == null) {
        semanticFailures.push(
          `phase '${phaseName}' missing required telemetry 'combat_camera.enemy_count' in '${snapshot.statePath}'`
        );
        continue;
      }
      enemyCounts.push(value);
      const renderedCount = asNumber(
        getPathValue(snapshot.state, "combat_camera.rendered_enemy_instance_count")
      );
      if (renderedCount == null) {
        semanticFailures.push(
          `phase '${phaseName}' missing required telemetry 'combat_camera.rendered_enemy_instance_count' in '${snapshot.statePath}'`
        );
      } else {
        renderedEnemyCounts.push(renderedCount);
        if (renderedCount < value) {
          semanticFailures.push(
            `phase '${phaseName}' rendered enemy instances (${renderedCount}) below enemy_count (${value}) in '${snapshot.statePath}'`
          );
        }
      }
    }
    details.max_enemy_count = enemyCounts.length > 0 ? Math.max(...enemyCounts) : null;
    details.max_rendered_enemy_instances =
      renderedEnemyCounts.length > 0 ? Math.max(...renderedEnemyCounts) : null;
    if (!(details.max_enemy_count >= 2)) {
      semanticFailures.push(
        `phase '${phaseName}' expected multi-enemy presence (enemy_count>=2) but observed ${String(
          details.max_enemy_count
        )}`
      );
    }
  }

  if (phaseName === "combo_burst") {
    requireCounterDelta("light_attacks", "light attack");
    requireCounterDelta("heavy_attacks", "heavy attack");
  }

  if (phaseName === "dodge_parry_burst") {
    requireCounterDelta("dodges", "dodge");
    requireCounterDelta("parries", "parry");
  }

  if (phaseName === "death_restart_loop") {
    requireCounterDelta("deaths", "death");
    requireCounterDelta("restarts", "restart");
  }

  return { failures: semanticFailures, counters, details };
};

let latestMetrics = null;
let latestRuntimeMetricsV2 = null;
const governorActionTrace = [];
const governorActionKeys = new Set();

for (const reportFile of phaseReports) {
  if (!fs.existsSync(reportFile)) {
    failures.push(`missing phase report: ${reportFile}`);
    continue;
  }
  const report = JSON.parse(fs.readFileSync(reportFile, "utf8"));
  const phaseName = path.basename(path.dirname(reportFile));
  const missingFields = missingFieldPaths(report, requiredPhaseReportFields);
  if (missingFields.length > 0) {
    failures.push(
      `phase '${phaseName}' missing required report field(s): ${missingFields.join(", ")}`
    );
  }
  if (report?.actionPlan?.preset !== phaseName) {
    failures.push(
      `phase '${phaseName}' actionPlan.preset mismatch: ${String(report?.actionPlan?.preset)}`
    );
  }
  if (report?.config?.actionPreset !== phaseName) {
    failures.push(
      `phase '${phaseName}' config.actionPreset mismatch: ${String(report?.config?.actionPreset)}`
    );
  }
  const availablePresets = Array.isArray(report?.actionPlan?.availablePresets)
    ? report.actionPlan.availablePresets
    : [];
  const missingAvailable = requiredPresetNames.filter((preset) => !availablePresets.includes(preset));
  if (missingAvailable.length > 0) {
    failures.push(
      `phase '${phaseName}' actionPlan.availablePresets missing required preset(s): ${missingAvailable.join(", ")}`
    );
  }
  const failedAssertionsList = Array.isArray(report?.assertions?.failedAssertions)
    ? report.assertions.failedAssertions
    : [];
  const relevantFailedAssertions = failedAssertionsList.filter(
    (assertion) => assertion?.name !== "draw_calls_increase"
  );
  const failedAssertions = Number(report?.assertions?.failed ?? 0);
  const strictExitCode = Number(report?.strictExitCode ?? 0);
  const diagnosticsErrorCount = Number(report?.diagnostics?.errorEventCount ?? 0);
  const iterations = Array.isArray(report?.iterations) ? report.iterations : [];
  const phaseSnapshots = loadPhaseStateSnapshots(phaseName, iterations);
  const semanticResult = evaluatePhaseSemantics(phaseName, phaseSnapshots);
  if (semanticResult.failures.length > 0) {
    failures.push(...semanticResult.failures);
  }
  const iterationCount = iterations.length;
  const nearBlankFrames = iterations.filter(
    (iter) => iter?.imageStats?.nearBlank === true
  ).length;
  const firstIteration = iterations[0] ?? null;
  const lastIteration = iterations[iterations.length - 1] ?? null;
  const firstRuntime = firstIteration?.runtime?.runtime ?? {};
  const lastRuntime = lastIteration?.runtime?.runtime ?? {};
  const firstCounters = firstIteration?.runtime?.counters ?? {};
  const lastCounters = lastIteration?.runtime?.counters ?? {};
  const firstAck = asNumber(firstCounters?.ack?.value ?? firstRuntime?.ack);
  const lastAck = asNumber(lastCounters?.ack?.value ?? lastRuntime?.ack);
  const ackDelta = firstAck == null || lastAck == null ? null : lastAck - firstAck;
  const firstCorrections = asNumber(firstRuntime?.corrections);
  const lastCorrections = asNumber(lastRuntime?.corrections);
  const correctionsDelta =
    firstCorrections == null || lastCorrections == null ? null : lastCorrections - firstCorrections;
  const firstTick = asNumber(firstCounters?.tick?.value ?? firstRuntime?.tick);
  const lastTick = asNumber(lastCounters?.tick?.value ?? lastRuntime?.tick);
  const tickDelta = firstTick == null || lastTick == null ? null : lastTick - firstTick;
  const firstDrawCalls = asNumber(firstCounters?.drawCalls?.value ?? firstIteration?.runtime?.metrics?.gpu?.draw_calls);
  const lastDrawCalls = asNumber(lastCounters?.drawCalls?.value ?? lastIteration?.runtime?.metrics?.gpu?.draw_calls);
  const drawCallsDelta =
    firstDrawCalls == null || lastDrawCalls == null ? null : lastDrawCalls - firstDrawCalls;
  const firstHash = typeof firstRuntime?.hash === "string" ? firstRuntime.hash : null;
  const lastHash = typeof lastRuntime?.hash === "string" ? lastRuntime.hash : null;
  const hashChanged = firstHash != null && lastHash != null ? firstHash !== lastHash : false;
  const phaseMetrics =
    (lastIteration?.runtime?.metrics && typeof lastIteration.runtime.metrics === "object")
      ? lastIteration.runtime.metrics
      : ((firstIteration?.runtime?.metrics && typeof firstIteration.runtime.metrics === "object")
          ? firstIteration.runtime.metrics
          : null);
  if (!phaseMetrics) {
    failures.push(`phase '${phaseName}' missing runtime metrics payload`);
  } else {
    latestMetrics = phaseMetrics;
    const metricsV2 = phaseMetrics?.runtime_metrics_v2;
    const metricsV2Errors = validateRuntimeMetricsV2(metricsV2, phaseName);
    if (metricsV2Errors.length > 0) {
      failures.push(...metricsV2Errors);
    } else {
      latestRuntimeMetricsV2 = metricsV2;
    }

    const phaseGovernorActions = Array.isArray(phaseMetrics?.governor_action_trace)
      ? phaseMetrics.governor_action_trace
      : (Array.isArray(metricsV2?.governor?.actions) ? metricsV2.governor.actions : []);
    for (const action of phaseGovernorActions) {
      if (!action || typeof action !== "object") {
        continue;
      }
      const dedupeKey = [
        action.tick,
        action.frame_index,
        action.action,
        action.reason,
        action.now_ms
      ].join("|");
      if (governorActionKeys.has(dedupeKey)) {
        continue;
      }
      governorActionKeys.add(dedupeKey);
      governorActionTrace.push({
        ...action,
        phase: phaseName,
        reportFile,
      });
    }
  }

  if (report?.status !== "ok") {
    failures.push(`phase '${phaseName}' status=${report?.status}`);
  }
  if (strictExitCode !== 0) {
    failures.push(`phase '${phaseName}' strictExitCode=${strictExitCode}`);
  }
  if (relevantFailedAssertions.length > 0) {
    failures.push(
      `phase '${phaseName}' failedAssertions=${relevantFailedAssertions.length}`
    );
  }
  if (diagnosticsErrorCount > 0) {
    failures.push(`phase '${phaseName}' diagnosticsErrorCount=${diagnosticsErrorCount}`);
  }
  if (iterationCount < 1) {
    failures.push(`phase '${phaseName}' produced no iterations`);
  }
  if (!((ackDelta != null && ackDelta > 0) || (correctionsDelta != null && correctionsDelta > 0))) {
    failures.push(
      `phase '${phaseName}' missing protocol progress (ackDelta=${String(
        ackDelta
      )}, correctionsDelta=${String(correctionsDelta)})`
    );
  }
  if ((drawCallsDelta == null || drawCallsDelta <= 0) && (tickDelta == null || tickDelta <= 0)) {
    failures.push(
      `phase '${phaseName}' missing runtime progression (drawCallsDelta=${String(
        drawCallsDelta
      )}, tickDelta=${String(tickDelta)})`
    );
  }

  const shots = (report?.iterations || [])
    .map((iter) => iter?.screenshotPath)
    .filter((value) => typeof value === "string" && value.length > 0);
  if (shots.length > 0) {
    finalShot = shots[shots.length - 1];
  }

  phases.push({
    phase: phaseName,
    reportFile,
    status: report?.status ?? "unknown",
    strictExitCode,
    failedAssertions,
    relevantFailedAssertions: relevantFailedAssertions.length,
    diagnosticsErrorCount,
    iterationCount,
    nearBlankFrames,
    ackDelta,
    correctionsDelta,
    tickDelta,
    drawCallsDelta,
    hashChanged,
    runtimeMetricsV2Present: Boolean(phaseMetrics?.runtime_metrics_v2),
    governorActionCount: Array.isArray(phaseMetrics?.governor_action_trace)
      ? phaseMetrics.governor_action_trace.length
      : (Array.isArray(phaseMetrics?.runtime_metrics_v2?.governor?.actions)
          ? phaseMetrics.runtime_metrics_v2.governor.actions.length
          : 0),
    semanticCounters: semanticResult.counters,
    semanticDetails: semanticResult.details,
    semanticFailureCount: semanticResult.failures.length,
    finishedAt: report?.finishedAt ?? null,
  });
}

if (!latestRuntimeMetricsV2) {
  failures.push("missing complete runtime_metrics_v2 payload across smoke phases");
}

const seenPhases = new Set(phases.map((entry) => entry.phase));
const missingPhases = requiredPresetNames.filter((name) => !seenPhases.has(name));
if (missingPhases.length > 0) {
  failures.push(`missing required preset phase report(s): ${missingPhases.join(", ")}`);
}
if (phases.length !== requiredPresetNames.length) {
  failures.push(
    `phase count mismatch: expected=${requiredPresetNames.length} actual=${phases.length}`
  );
}

if (finalShot && fs.existsSync(finalShot)) {
  fs.copyFileSync(finalShot, screenshotPath);
}

const summary = {
  schema_version: 3,
  kind: "aaa-forest-browser-smoke-report-v3",
  appPath,
  url: smokeUrl,
  artifact_root: artifactRoot,
  lane: artifactLane,
  iteration: artifactIteration,
  preset_sequence: requiredPresetNames,
  required_preset_fields: requiredPresetFields,
  required_phase_report_fields: requiredPhaseReportFields,
  passed: failures.length === 0,
  failures,
  phases,
  metrics: latestMetrics ?? {},
  runtime_metrics_v2: latestRuntimeMetricsV2 ?? null,
  governor_action_trace: governorActionTrace,
  smokeScreenshot: screenshotPath,
};
fs.mkdirSync(path.dirname(reportPath), { recursive: true });
fs.writeFileSync(reportPath, JSON.stringify(summary, null, 2));

if (failures.length > 0) {
  console.error("browser smoke assertions failed:");
  for (const failure of failures) {
    console.error(` - ${failure}`);
  }
  console.error(`report: ${reportPath}`);
  process.exit(1);
}
NODE
then
  fail "phase aggregation failed for ${APP_PATH}; see ${REPORT_PATH}"
fi

echo "browser smoke passed: app=${APP_PATH} artifact=${TASK_ARTIFACT_DIR}"
