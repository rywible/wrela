#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP_PATH="${1:-apps/wrela-game-slice}"
TASK_ARTIFACT_DIR="${2:-${ROOT_DIR}/.artifacts/full-compiler-pass/WFE2-602/smoke/$(basename "${APP_PATH}")}"
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
WEB_GAME_ACTIONS_PATH="${WEB_GAME_ACTIONS:-${WEB_GAME_SKILL_DIR}/references/action_payloads.json}"
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

cargo run -p wrela -- game build "${APP_PATH}" --target=dual --client-runtime=compiled --shader-provenance --no-shortcuts >/dev/null

WRELA_GAME_ARTIFACT_DIR="${PROTOCOL_ARTIFACT_ROOT}" \
WRELA_GAME_BIND_ADDR="${BIND_ADDR}" \
cargo run -p wrela -- game run "${APP_PATH}" >"${SERVER_LOG_PATH}" 2>&1 &
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
  PRESETS=("warmup" "movement_sweep" "website_interaction" "stress")
else
  PRESETS=("warmup" "movement_sweep" "pickup_attempt" "stress")
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
      --strict true \
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

for (const reportFile of phaseReports) {
  if (!fs.existsSync(reportFile)) {
    failures.push(`missing phase report: ${reportFile}`);
    continue;
  }
  const report = JSON.parse(fs.readFileSync(reportFile, "utf8"));
  const phaseName = path.basename(path.dirname(reportFile));
  const failedAssertions = Number(report?.assertions?.failed ?? 0);
  const strictExitCode = Number(report?.strictExitCode ?? 0);
  const diagnosticsErrorCount = Number(report?.diagnostics?.errorEventCount ?? 0);
  const iterationCount = Array.isArray(report?.iterations) ? report.iterations.length : 0;
  const nearBlankFrames = (report?.iterations || []).filter(
    (iter) => iter?.imageStats?.nearBlank === true
  ).length;

  if (report?.status !== "ok") {
    failures.push(`phase '${phaseName}' status=${report?.status}`);
  }
  if (strictExitCode !== 0) {
    failures.push(`phase '${phaseName}' strictExitCode=${strictExitCode}`);
  }
  if (failedAssertions > 0) {
    failures.push(`phase '${phaseName}' failedAssertions=${failedAssertions}`);
  }
  if (diagnosticsErrorCount > 0) {
    failures.push(`phase '${phaseName}' diagnosticsErrorCount=${diagnosticsErrorCount}`);
  }
  if (iterationCount < 1) {
    failures.push(`phase '${phaseName}' produced no iterations`);
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
    diagnosticsErrorCount,
    iterationCount,
    nearBlankFrames,
    finishedAt: report?.finishedAt ?? null,
  });
}

if (finalShot && fs.existsSync(finalShot)) {
  fs.copyFileSync(finalShot, screenshotPath);
}

const summary = {
  appPath,
  url: smokeUrl,
  passed: failures.length === 0,
  failures,
  phases,
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
