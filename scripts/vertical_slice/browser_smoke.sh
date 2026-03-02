#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP_PATH="${1:-apps/wrela-game-slice}"
ARTIFACT_ROOT="${2:-${ROOT_DIR}/.artifacts/vertical-slice/WFE-602/smoke}"
BIND_ADDR="${3:-${WRELA_GAME_BIND_ADDR:-127.0.0.1:8091}}"
SMOKE_URL="http://${BIND_ADDR}/"

PROTOCOL_ARTIFACT_ROOT="${ARTIFACT_ROOT}/protocol"
SCREENSHOTS_DIR="${ARTIFACT_ROOT}/screenshots"
SCREENSHOT_PATH="${SCREENSHOTS_DIR}/preview.png"
PREVIEW_REPORT_PATH="${ARTIFACT_ROOT}/preview-report.json"
INTERACTION_LOG_PATH="${ARTIFACT_ROOT}/interaction-log.json"
ASSERTION_SUMMARY_PATH="${ARTIFACT_ROOT}/assertion-summary.json"
SCREENSHOTS_PATH="${ARTIFACT_ROOT}/screenshots.json"
CONSOLE_ERROR_SUMMARY_PATH="${ARTIFACT_ROOT}/console-error-summary.json"
SERVER_LOG_PATH="${ARTIFACT_ROOT}/server.log"
BROWSER_LOG_PATH="${ARTIFACT_ROOT}/browser.log"
SMOKE_READY_RETRIES="${WRELA_FRONTEND_PREVIEW_READY_RETRIES:-120}"

fail() {
  echo "browser smoke failed: $*" >&2
  exit 1
}

mkdir -p "${PROTOCOL_ARTIFACT_ROOT}" "${SCREENSHOTS_DIR}"
rm -f \
  "${PREVIEW_REPORT_PATH}" \
  "${INTERACTION_LOG_PATH}" \
  "${ASSERTION_SUMMARY_PATH}" \
  "${SCREENSHOTS_PATH}" \
  "${CONSOLE_ERROR_SUMMARY_PATH}" \
  "${SERVER_LOG_PATH}" \
  "${BROWSER_LOG_PATH}"
rm -f "${SCREENSHOTS_DIR}"/*.png >/dev/null 2>&1 || true

cd "${ROOT_DIR}"

cargo run -p wrela -- game build "${APP_PATH}" --target=dual --client-runtime=compiled --shader-provenance --no-shortcuts >/dev/null

WRELA_GAME_ARTIFACT_DIR="${PROTOCOL_ARTIFACT_ROOT}" \
WRELA_GAME_BIND_ADDR="${BIND_ADDR}" \
cargo run -p wrela -- game run "${APP_PATH}" --client-runtime=compiled --shader-provenance --no-shortcuts >"${SERVER_LOG_PATH}" 2>&1 &
SERVER_PID=$!

cleanup() {
  kill "${SERVER_PID}" >/dev/null 2>&1 || true
  wait "${SERVER_PID}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in $(seq 1 "${SMOKE_READY_RETRIES}"); do
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

if [ ! -d "${ROOT_DIR}/scripts/vertical_slice/node_modules/playwright" ]; then
  npm --prefix "${ROOT_DIR}/scripts/vertical_slice" install --silent --no-fund --no-audit
fi

if ! ls "${ROOT_DIR}/.cache/ms-playwright"/chromium-* >/dev/null 2>&1; then
  PLAYWRIGHT_BROWSERS_PATH="${ROOT_DIR}/.cache/ms-playwright" \
  npx --prefix "${ROOT_DIR}/scripts/vertical_slice" playwright install chromium >/dev/null
fi

if ! PLAYWRIGHT_BROWSERS_PATH="${ROOT_DIR}/.cache/ms-playwright" \
  node "${ROOT_DIR}/scripts/vertical_slice/browser_smoke.mjs" \
    --url "${SMOKE_URL}" \
    --screenshot-path "${SCREENSHOT_PATH}" \
    --preview-report-path "${PREVIEW_REPORT_PATH}" \
    --interaction-log-path "${INTERACTION_LOG_PATH}" \
    --assertion-summary-path "${ASSERTION_SUMMARY_PATH}" \
    --screenshots-path "${SCREENSHOTS_PATH}" \
    --console-error-summary-path "${CONSOLE_ERROR_SUMMARY_PATH}" >"${BROWSER_LOG_PATH}" 2>&1; then
  fail "browser interaction/assertions failed for ${APP_PATH}; see ${ASSERTION_SUMMARY_PATH}, ${CONSOLE_ERROR_SUMMARY_PATH}, and ${BROWSER_LOG_PATH}"
fi

echo "browser smoke passed: app=${APP_PATH} artifact=${ARTIFACT_ROOT}"
