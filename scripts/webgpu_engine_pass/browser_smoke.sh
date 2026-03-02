#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP_PATH="${1:-apps/wrela-game-slice}"
TASK_ARTIFACT_DIR="${2:-${ROOT_DIR}/.artifacts/webgpu-engine-pass/WFE3-602/smoke/$(basename "${APP_PATH}")}"
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
USE_SHARED_STRESS_PRESET=false
if [ ! -f "${WEB_GAME_ACTIONS_PATH}" ]; then
  WEB_GAME_ACTIONS_PATH="${WEB_GAME_SKILL_DIR}/references/action_payloads.json"
  USE_SHARED_STRESS_PRESET=true
fi
if ! node -e '
const fs = require("node:fs");
const path = process.argv[1];
try {
  const parsed = JSON.parse(fs.readFileSync(path, "utf8"));
  const presets = parsed && typeof parsed === "object" ? parsed.presets : null;
  const hasSplitStress = presets && typeof presets === "object"
    && Object.prototype.hasOwnProperty.call(presets, "game_stress")
    && Object.prototype.hasOwnProperty.call(presets, "website_stress");
  process.exit(hasSplitStress ? 0 : 1);
} catch {
  process.exit(1);
}
' "${WEB_GAME_ACTIONS_PATH}"; then
  WEB_GAME_ACTIONS_PATH="${WEB_GAME_SKILL_DIR}/references/action_payloads.json"
  USE_SHARED_STRESS_PRESET=true
fi
PW_BROWSER="${WRELA_SMOKE_BROWSER:-chromium}"
PW_HEADLESS="${WRELA_SMOKE_HEADLESS:-false}"
PW_ALLOW_HEADLESS_WEBGPU="${WRELA_SMOKE_ALLOW_HEADLESS_WEBGPU:-false}"

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

if [[ "${APP_PATH}" == *"website"* ]]; then
  if [ "${USE_SHARED_STRESS_PRESET}" = true ]; then
    PRESETS=("warmup" "movement_sweep" "website_interaction" "stress")
  else
    PRESETS=("warmup" "movement_sweep" "website_interaction" "website_stress")
  fi
else
  if [ "${USE_SHARED_STRESS_PRESET}" = true ]; then
    PRESETS=("warmup" "movement_sweep" "pickup_attempt" "stress")
  else
    PRESETS=("warmup" "movement_sweep" "pickup_attempt" "game_stress")
  fi
fi

if ! node -e '
const fs = require("node:fs");
const file = process.argv[1];
const expected = process.argv.slice(2);
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
' "${WEB_GAME_ACTIONS_PATH}" "${PRESETS[@]}"; then
  fail "actions file does not define required presets for ${APP_PATH}; see ${WEB_GAME_ACTIONS_PATH}"
fi

PHASE_REPORTS=()
for PRESET in "${PRESETS[@]}"; do
  PHASE_DIR="${PLAYWRIGHT_ARTIFACT_DIR}/${PRESET}"
  PHASE_REPORT="${PHASE_DIR}/report.json"
  PHASE_STRICT=true
  if [[ "${APP_PATH}" == *"website"* ]] && [[ "${PRESET}" == "website_interaction" || "${PRESET}" == "website_stress" || "${PRESET}" == "stress" ]]; then
    PHASE_STRICT=false
  fi
  mkdir -p "${PHASE_DIR}"
  if ! PLAYWRIGHT_BROWSERS_PATH="${ROOT_DIR}/.cache/ms-playwright" \
    node "${WEB_GAME_CLIENT_PATH}" \
      --url "${SMOKE_URL}" \
      --browser "${PW_BROWSER}" \
      --headless "${PW_HEADLESS}" \
      --background true \
      --allow-headless-webgpu "${PW_ALLOW_HEADLESS_WEBGPU}" \
      --strict "${PHASE_STRICT}" \
      --fail-on-near-blank true \
      --fail-on-diagnostic-errors true \
      --fail-on-requestfailed true \
      --click-selector "canvas" \
      --require-click-selector true \
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

if ! node - "${REPORT_PATH}" "${SCREENSHOT_PATH}" "${APP_PATH}" "${SMOKE_URL}" "${PHASE_REPORTS[@]}" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [reportPath, screenshotPath, appPath, smokeUrl, ...phaseReports] = process.argv.slice(2);
const failures = [];
const phases = [];
let finalShot = null;
const isWebsiteApp = appPath.includes("website");

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
  const failedAssertionsList = Array.isArray(report?.assertions?.failedAssertions)
    ? report.assertions.failedAssertions
    : [];
  const ignoredAssertionNames = [];
  const relevantFailedAssertions =
    isWebsiteApp &&
    (phaseName === "website_interaction" || phaseName === "website_stress" || phaseName === "stress")
      ? failedAssertionsList.filter((assertion) => {
          const ignore =
            assertion?.name === "tick_monotonic" ||
            assertion?.name === "hash_changes" ||
            assertion?.name === "expect_min_tick_delta" ||
            assertion?.name === "expect_hash_change";
          if (ignore) {
            ignoredAssertionNames.push(assertion?.name ?? "unknown");
          }
          return !ignore;
        })
      : failedAssertionsList;
  const failedAssertions = Number(report?.assertions?.failed ?? 0);
  const strictExitCode = Number(report?.strictExitCode ?? 0);
  const diagnosticsErrorCount = Number(report?.diagnostics?.errorEventCount ?? 0);
  const iterations = Array.isArray(report?.iterations) ? report.iterations : [];
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
  if (
    isWebsiteApp &&
    (phaseName === "website_interaction" || phaseName === "website_stress" || phaseName === "stress")
  ) {
    const websiteProgress =
      (drawCallsDelta != null && drawCallsDelta > 0) &&
      ((correctionsDelta != null && correctionsDelta > 0) ||
        (tickDelta != null && tickDelta > 0) ||
        hashChanged);
    if (!websiteProgress) {
      failures.push(
        `phase '${phaseName}' missing website progress (drawCallsDelta=${String(
          drawCallsDelta
        )}, correctionsDelta=${String(correctionsDelta)}, tickDelta=${String(
          tickDelta
        )}, hashChanged=${hashChanged})`
      );
    }
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
    ignoredAssertionNames: ignoredAssertionNames.length > 0 ? [...new Set(ignoredAssertionNames)] : [],
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
    finishedAt: report?.finishedAt ?? null,
  });
}

if (!latestRuntimeMetricsV2) {
  failures.push("missing complete runtime_metrics_v2 payload across smoke phases");
}

if (finalShot && fs.existsSync(finalShot)) {
  fs.copyFileSync(finalShot, screenshotPath);
}

const summary = {
  schema_version: 2,
  kind: "webgpu-browser-smoke-report-v2",
  appPath,
  url: smokeUrl,
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
