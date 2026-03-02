#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_DIR="$ROOT/artifacts/fly"
mkdir -p "$REPORT_DIR"

APP_NAME="${1:-wrela-smoke-$(date +%Y%m%d%H%M%S)}"
REGION="${PRIMARY_REGION:-ord}"
MACHINES="${WRELA_DEPLOY_SMOKE_MACHINES:-3}"
DEPLOY_POLICY_PATH="${WRELA_DEPLOY_SMOKE_DEPLOY_POLICY:-}"
RAW_EXPECTED_MACHINES="${WRELA_DEPLOY_SMOKE_EXPECTED_MACHINES:-}"
if [[ -n "$DEPLOY_POLICY_PATH" && -z "$RAW_EXPECTED_MACHINES" ]]; then
  EXPECTED_MACHINES="1"
else
  EXPECTED_MACHINES="${RAW_EXPECTED_MACHINES:-$MACHINES}"
fi
AUTO_DESTROY="${WRELA_DEPLOY_SMOKE_AUTO_DESTROY:-1}"
DEPLOY_TIMEOUT="${WRELA_FLY_DEPLOY_WAIT_TIMEOUT:-15m}"
DEPLOY_ORG="${WRELA_FLY_ORG:-personal}"
DEPLOY_USE_DEPOT="${WRELA_FLY_DEPLOY_USE_DEPOT:-true}"

URL="https://${APP_NAME}.fly.dev"
REPORT_PATH="$REPORT_DIR/${APP_NAME}-smoke-report.json"
TMP_DIR="$(mktemp -d)"
TRAFFIC_LOG="$TMP_DIR/traffic.log"
TRAFFIC_SUMMARY="$TMP_DIR/traffic-summary.json"
TRAFFIC_STOP="$TMP_DIR/traffic.stop"
MESH_DIAGNOSTICS="$TMP_DIR/mesh-diagnostics.json"
CURL_CONNECT_TIMEOUT="${WRELA_DEPLOY_SMOKE_CURL_CONNECT_TIMEOUT:-2}"
CURL_MAX_TIME="${WRELA_DEPLOY_SMOKE_CURL_MAX_TIME:-8}"
TRAFFIC_CURL_CONNECT_TIMEOUT="${WRELA_DEPLOY_SMOKE_TRAFFIC_CURL_CONNECT_TIMEOUT:-1}"
TRAFFIC_CURL_MAX_TIME="${WRELA_DEPLOY_SMOKE_TRAFFIC_CURL_MAX_TIME:-4}"

STATUS="failed"
FAILURE_REASON=""

traffic_total() {
  if [[ -f "$TRAFFIC_SUMMARY" ]]; then
    jq -r '.total // 0' "$TRAFFIC_SUMMARY"
  else
    echo 0
  fi
}

traffic_failures() {
  if [[ -f "$TRAFFIC_SUMMARY" ]]; then
    jq -r '.failures // 0' "$TRAFFIC_SUMMARY"
  else
    echo 0
  fi
}

write_report() {
  local total failures mesh_diagnostics
  total="$(traffic_total)"
  failures="$(traffic_failures)"
  if [[ -f "$MESH_DIAGNOSTICS" ]]; then
    mesh_diagnostics="$(cat "$MESH_DIAGNOSTICS")"
  else
    mesh_diagnostics='[]'
  fi
  jq -n \
    --arg app "$APP_NAME" \
    --arg region "$REGION" \
    --arg url "$URL" \
    --arg status "$STATUS" \
    --arg reason "$FAILURE_REASON" \
    --arg replication_outside_lock "1" \
    --arg wal_encode_outside_lock "1" \
    --arg replicated_log_backend "canonical_only" \
    --arg insert_fast_lane "1" \
    --arg latency_frontier_mode "1" \
    --argjson machines "$EXPECTED_MACHINES" \
    --argjson traffic_total "$total" \
    --argjson traffic_failures "$failures" \
    --argjson mesh_diagnostics "$mesh_diagnostics" \
    --arg generated_at "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    '{
      app: $app,
      region: $region,
      url: $url,
      status: $status,
      reason: $reason,
      machines: $machines,
      rolling_redeploy_traffic: {
        total_requests: $traffic_total,
        failed_requests: $traffic_failures
      },
      mesh_diagnostics: $mesh_diagnostics,
      jupiter_defaults: {
        replication_outside_lock: $replication_outside_lock,
        wal_encode_outside_lock: $wal_encode_outside_lock,
        replicated_log_backend: $replicated_log_backend,
        insert_fast_lane_active: $insert_fast_lane,
        latency_frontier_mode: $latency_frontier_mode
      },
      generated_at: $generated_at
    }' >"$REPORT_PATH"
}

capture_mesh_diagnostics() {
  local ids payload rows
  rows='[]'
  ids="$(machine_ids || true)"
  if [[ -z "$ids" ]]; then
    echo '[]' >"$MESH_DIAGNOSTICS"
    return 0
  fi
  while IFS= read -r machine_id; do
    if payload="$(targeted_mesh "$machine_id" 2>/dev/null)"; then
      rows="$(jq -c \
        --arg machine_id "$machine_id" \
        --argjson payload "$payload" \
        '. + [{machineId: $machine_id, payload: $payload}]' <<<"$rows")"
    else
      rows="$(jq -c \
        --arg machine_id "$machine_id" \
        '. + [{machineId: $machine_id, error: "mesh_probe_failed"}]' <<<"$rows")"
    fi
  done <<<"$ids"
  printf '%s\n' "$rows" >"$MESH_DIAGNOSTICS"
}

on_error() {
  if [[ "$STATUS" == "passed" ]]; then
    return
  fi
  if [[ -z "$FAILURE_REASON" ]]; then
    FAILURE_REASON="unexpected_script_error"
  fi
  capture_mesh_diagnostics || true
  write_report
  echo "[smoke] failed app=$APP_NAME reason=$FAILURE_REASON" >&2
  echo "[smoke] app preserved for debugging" >&2
  echo "[smoke] inspect: flyctl status -a $APP_NAME" >&2
  echo "[smoke] logs: flyctl logs -a $APP_NAME" >&2
  echo "[smoke] cleanup: flyctl apps destroy $APP_NAME --yes" >&2
}

fail() {
  FAILURE_REASON="$1"
  capture_mesh_diagnostics || true
  write_report
  echo "[smoke] failure: $FAILURE_REASON" >&2
  echo "[smoke] app preserved for debugging" >&2
  echo "[smoke] inspect: flyctl status -a $APP_NAME" >&2
  echo "[smoke] logs: flyctl logs -a $APP_NAME" >&2
  echo "[smoke] cleanup: flyctl apps destroy $APP_NAME --yes" >&2
  exit 1
}

cleanup() {
  if [[ -f "$TRAFFIC_STOP" ]]; then
    rm -f "$TRAFFIC_STOP"
  fi
  if [[ -n "${TRAFFIC_PID:-}" ]]; then
    wait "$TRAFFIC_PID" 2>/dev/null || true
  fi
  if [[ "$STATUS" == "passed" && "$AUTO_DESTROY" == "1" ]]; then
    flyctl apps destroy "$APP_NAME" --yes >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT
trap on_error ERR

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

require_tool flyctl
require_tool jq
require_tool curl
require_tool cargo

resolve_deploy_app_path() {
  if [[ -n "${WRELA_FLY_DEPLOY_APP_PATH:-}" ]]; then
    printf '%s\n' "$WRELA_FLY_DEPLOY_APP_PATH"
    return 0
  fi
  local candidates=(
    "apps/wrela-http-db-smoke"
    "apps/ledger-lite"
  )
  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -d "$ROOT/$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "no deploy app path found; set WRELA_FLY_DEPLOY_APP_PATH" >&2
  return 1
}

ensure_fly_app_exists() {
  if flyctl status -a "$APP_NAME" >/dev/null 2>&1; then
    return 0
  fi
  flyctl apps create "$APP_NAME" --org "$DEPLOY_ORG" >/dev/null
}

machine_ids() {
  flyctl machines list -a "$APP_NAME" --json | jq -r '.[].id' | sort
}

machine_count() {
  machine_ids | wc -l | tr -d ' '
}

targeted_write() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    -X POST \
    "$URL/api/probe/write"
}

targeted_live() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/live"
}

targeted_read() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/probe/read"
}

targeted_health() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/health"
}

targeted_mesh() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/probe/mesh"
}

wait_for_targeted_write_success() {
  local machine_id="$1"
  local attempts="${2:-15}"
  local delay_s="${3:-1}"
  for _ in $(seq 1 "$attempts"); do
    local payload
    if payload="$(targeted_write "$machine_id" 2>/dev/null)"; then
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      local committed_version
      committed_version="$(jq -r '.version // -1' <<<"$payload")"
      if [[ "$ok" == "true" && "$committed_version" -gt 0 ]] && verify_machine_identity "$payload" "$machine_id"; then
        printf '%s\n' "$payload"
        return 0
      fi
    fi
    sleep "$delay_s"
  done
  return 1
}

verify_machine_identity() {
  local payload="$1"
  local expected_machine="$2"
  local actual_machine
  actual_machine="$(jq -r '.machineId // empty' <<<"$payload")"
  [[ -n "$actual_machine" && "$actual_machine" == "$expected_machine" ]]
}

wait_for_all_machines_live() {
  local attempts="${1:-90}"
  local delay_s="${2:-2}"
  local ids
  for _ in $(seq 1 "$attempts"); do
    ids="$(machine_ids)"
    if [[ -z "$ids" ]]; then
      sleep "$delay_s"
      continue
    fi
    local all_healthy="1"
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_live "$machine_id" 2>/dev/null)"; then
        all_healthy="0"
        break
      fi
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      if [[ "$ok" != "true" ]] || ! verify_machine_identity "$payload" "$machine_id"; then
        all_healthy="0"
        break
      fi
    done <<<"$ids"
    if [[ "$all_healthy" == "1" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  return 1
}

wait_for_all_machines_ready() {
  local attempts="${1:-90}"
  local delay_s="${2:-2}"
  local ids
  for _ in $(seq 1 "$attempts"); do
    ids="$(machine_ids)"
    if [[ -z "$ids" ]]; then
      sleep "$delay_s"
      continue
    fi
    local all_ready="1"
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_health "$machine_id" 2>/dev/null)"; then
        all_ready="0"
        break
      fi
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      local mesh_ready
      mesh_ready="$(jq -r '.meshReady // false' <<<"$payload")"
      if [[ "$ok" != "true" || "$mesh_ready" != "true" ]] || ! verify_machine_identity "$payload" "$machine_id"; then
        all_ready="0"
        break
      fi
    done <<<"$ids"
    if [[ "$all_ready" == "1" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  return 1
}

wait_for_all_machines_mesh_ready() {
  local attempts="${1:-90}"
  local delay_s="${2:-2}"
  local ids
  for _ in $(seq 1 "$attempts"); do
    ids="$(machine_ids)"
    if [[ -z "$ids" ]]; then
      sleep "$delay_s"
      continue
    fi
    local all_ready="1"
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_mesh "$machine_id" 2>/dev/null)"; then
        all_ready="0"
        break
      fi
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      local mesh_ready
      mesh_ready="$(jq -r '.meshReady // false' <<<"$payload")"
      if [[ "$ok" != "true" || "$mesh_ready" != "true" ]]; then
        all_ready="0"
        break
      fi
      if ! verify_machine_identity "$payload" "$machine_id"; then
        all_ready="0"
        break
      fi
    done <<<"$ids"
    if [[ "$all_ready" == "1" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  capture_mesh_diagnostics || true
  return 1
}

wait_for_targeted_convergence() {
  local expected_value="$1"
  local attempts="${2:-45}"
  local delay_s="${3:-1}"
  local ids
  ids="$(machine_ids)"
  if [[ -z "$ids" ]]; then
    return 1
  fi

  for _ in $(seq 1 "$attempts"); do
    local converged="1"
    local read_payloads=()
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_read "$machine_id" 2>/dev/null)"; then
        converged="0"
        break
      fi
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      local value
      value="$(jq -r '.value // ""' <<<"$payload")"
      if [[ "$ok" != "true" || "$value" != "$expected_value" ]]; then
        converged="0"
        break
      fi
      if ! verify_machine_identity "$payload" "$machine_id"; then
        converged="0"
        break
      fi
      read_payloads+=("$payload")
    done <<<"$ids"

    if [[ "$converged" == "1" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  return 1
}

verify_targeted_roundtrip_all_machines() {
  local ids
  ids="$(machine_ids)"
  if [[ -z "$ids" ]]; then
    return 1
  fi
  while IFS= read -r machine_id; do
    local write_payload
    if ! write_payload="$(wait_for_targeted_write_success "$machine_id" 15 1)"; then
      echo "targeted write failed for machine=$machine_id after retries" >&2
      return 1
    fi
    local ok
    ok="$(jq -r '.ok // false' <<<"$write_payload")"
    if [[ "$ok" != "true" ]]; then
      echo "targeted write failed for machine=$machine_id payload=$write_payload" >&2
      return 1
    fi
    if ! verify_machine_identity "$write_payload" "$machine_id"; then
      echo "targeted write routed to unexpected machine for target=$machine_id payload=$write_payload" >&2
      return 1
    fi
    local expected_value
    expected_value="$(jq -r '.value // ""' <<<"$write_payload")"
    local committed_version
    committed_version="$(jq -r '.version // -1' <<<"$write_payload")"
    if [[ -z "$expected_value" ]]; then
      echo "targeted write missing value for machine=$machine_id payload=$write_payload" >&2
      return 1
    fi
    if [[ "$committed_version" -le 0 ]]; then
      echo "targeted write returned invalid commit version for machine=$machine_id payload=$write_payload" >&2
      return 1
    fi
    if ! wait_for_targeted_convergence "$expected_value" 45 1; then
      echo "cluster value did not converge for machine=$machine_id expected=$expected_value" >&2
      return 1
    fi
  done <<<"$ids"
}

start_probe_traffic() {
  (
    local total=0
    local failures=0
    while [[ ! -f "$TRAFFIC_STOP" ]]; do
      local payload
      local request_ok="0"
      for _attempt in $(seq 1 15); do
        if payload="$(curl -fsS --connect-timeout "$TRAFFIC_CURL_CONNECT_TIMEOUT" --max-time "$TRAFFIC_CURL_MAX_TIME" -X POST "$URL/api/probe/write" 2>/dev/null)"; then
          local ok
          ok="$(jq -r '.ok // false' <<<"$payload")"
          local committed_version
          committed_version="$(jq -r '.version // -1' <<<"$payload")"
          if [[ "$ok" == "true" && "$committed_version" -gt 0 ]]; then
            request_ok="1"
            break
          fi
        fi
        sleep 0.2
      done
      if [[ "$request_ok" != "1" ]]; then
        failures=$((failures + 1))
      fi
      total=$((total + 1))
      printf '{"total":%d,"failures":%d}\n' "$total" "$failures" >"$TRAFFIC_LOG"
      sleep 0.5
    done
    printf '{"total":%d,"failures":%d}\n' "$total" "$failures" >"$TRAFFIC_SUMMARY"
  ) &
  TRAFFIC_PID=$!
}

stop_probe_traffic() {
  : >"$TRAFFIC_STOP"
  wait "$TRAFFIC_PID"
}

run_deploy() {
  local deploy_app_path
  deploy_app_path="$(resolve_deploy_app_path)" || return 1
  if [[ -n "$DEPLOY_POLICY_PATH" ]]; then
    cargo run -p wrela -- deploy "$deploy_app_path" \
      --target=fly \
      --app="$APP_NAME" \
      --deploy-policy="$DEPLOY_POLICY_PATH" \
      --force
    return 0
  fi

  local deploy_config="$ROOT/$deploy_app_path/fly.toml"
  local deploy_dockerfile="$ROOT/$deploy_app_path/Dockerfile"
  [[ -f "$deploy_config" ]] || {
    echo "missing fly config: $deploy_config" >&2
    return 1
  }
  [[ -f "$deploy_dockerfile" ]] || {
    echo "missing dockerfile: $deploy_dockerfile" >&2
    return 1
  }

  ensure_fly_app_exists || return 1

  flyctl deploy \
    --remote-only \
    --depot="$DEPLOY_USE_DEPOT" \
    --config "$deploy_config" \
    --dockerfile "$deploy_dockerfile" \
    -a "$APP_NAME" \
    --strategy rolling \
    --yes \
    --wait-timeout "$DEPLOY_TIMEOUT"

  flyctl scale count "$MACHINES" -a "$APP_NAME" --yes >/dev/null
}

wait_for_machines() {
  local attempts="${1:-90}"
  local delay_s="${2:-2}"
  for _ in $(seq 1 "$attempts"); do
    local count
    count="$(machine_count)"
    if [[ "$count" -ge "$EXPECTED_MACHINES" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  return 1
}

echo "[smoke] app=$APP_NAME region=$REGION machines=$MACHINES expected=$EXPECTED_MACHINES policy=${DEPLOY_POLICY_PATH:-none}"
if ! run_deploy; then
  fail "initial_deploy_failed"
fi

if ! wait_for_machines 90 2; then
  fail "machine_count_not_ready_after_initial_deploy"
fi

if ! wait_for_all_machines_live 90 2; then
  fail "machines_not_live_after_initial_deploy"
fi

if ! wait_for_all_machines_ready 90 2; then
  fail "machines_not_ready_after_initial_deploy"
fi

if ! wait_for_all_machines_mesh_ready 90 2; then
  fail "machines_not_mesh_ready_after_initial_deploy"
fi

if ! verify_targeted_roundtrip_all_machines; then
  fail "targeted_roundtrip_failed_after_initial_deploy"
fi

start_probe_traffic
if ! run_deploy; then
  fail "rolling_redeploy_failed"
fi
stop_probe_traffic

if [[ ! -f "$TRAFFIC_SUMMARY" ]]; then
  fail "missing_traffic_summary"
fi

TRAFFIC_TOTAL="$(jq -r '.total // 0' "$TRAFFIC_SUMMARY")"
TRAFFIC_FAILURES="$(jq -r '.failures // 0' "$TRAFFIC_SUMMARY")"
if [[ "$TRAFFIC_FAILURES" != "0" ]]; then
  fail "traffic_failures_during_rolling_redeploy"
fi

if ! wait_for_machines 90 2; then
  fail "machine_count_not_ready_after_redeploy"
fi

if ! wait_for_all_machines_live 90 2; then
  fail "machines_not_live_after_redeploy"
fi

if ! wait_for_all_machines_ready 90 2; then
  fail "machines_not_ready_after_redeploy"
fi

if ! wait_for_all_machines_mesh_ready 90 2; then
  fail "machines_not_mesh_ready_after_redeploy"
fi

if ! verify_targeted_roundtrip_all_machines; then
  fail "targeted_roundtrip_failed_after_redeploy"
fi

STATUS="passed"
write_report

echo "[smoke] success app=$APP_NAME report=$REPORT_PATH"
if [[ "$AUTO_DESTROY" == "1" ]]; then
  echo "[smoke] app will be destroyed during cleanup"
else
  echo "[smoke] app kept alive because WRELA_DEPLOY_SMOKE_AUTO_DESTROY=$AUTO_DESTROY"
fi

exit 0
