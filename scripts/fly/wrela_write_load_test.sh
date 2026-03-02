#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_DIR="$ROOT/artifacts/fly"
mkdir -p "$REPORT_DIR"

APP_NAME="${1:-wrela-load-$(date +%Y%m%d%H%M%S)}"
REGION="${PRIMARY_REGION:-ord}"
MACHINES="${WRELA_WRITE_LOAD_MACHINES:-3}"
DEPLOY_POLICY_PATH="${WRELA_WRITE_LOAD_DEPLOY_POLICY:-}"
if [[ -n "$DEPLOY_POLICY_PATH" && -z "${WRELA_WRITE_LOAD_EXPECTED_MACHINES:-}" ]]; then
  EXPECTED_MACHINES="1"
else
  EXPECTED_MACHINES="${WRELA_WRITE_LOAD_EXPECTED_MACHINES:-$MACHINES}"
fi
AUTO_DESTROY="${WRELA_WRITE_LOAD_AUTO_DESTROY:-1}"
SCRIPT_TIMEOUT_SECONDS="${WRELA_WRITE_LOAD_SCRIPT_TIMEOUT_SECONDS:-1500}"
DEPLOY_TIMEOUT="${WRELA_FLY_DEPLOY_WAIT_TIMEOUT:-15m}"
DEPLOY_ORG="${WRELA_FLY_ORG:-personal}"
DEPLOY_USE_DEPOT="${WRELA_FLY_DEPLOY_USE_DEPOT:-true}"

STAGE_A_DURATION_SECONDS="${WRELA_WRITE_LOAD_STAGE_A_DURATION_SECONDS:-60}"
STAGE_A_CONCURRENCY="${WRELA_WRITE_LOAD_STAGE_A_CONCURRENCY:-8}"
STAGE_A_MAX_FAILURE_RATE_PCT="${WRELA_WRITE_LOAD_STAGE_A_MAX_FAILURE_RATE_PCT:-0.0}"
STAGE_A_ROLLING_REDEPLOY="0"

STAGE_B_DURATION_SECONDS="${WRELA_WRITE_LOAD_STAGE_B_DURATION_SECONDS:-180}"
STAGE_B_CONCURRENCY="${WRELA_WRITE_LOAD_STAGE_B_CONCURRENCY:-16}"
STAGE_B_MAX_FAILURE_RATE_PCT="${WRELA_WRITE_LOAD_STAGE_B_MAX_FAILURE_RATE_PCT:-0.1}"
STAGE_B_ROLLING_REDEPLOY="1"
STAGE_B_REDEPLOY_AFTER_SECONDS="${WRELA_WRITE_LOAD_STAGE_B_REDEPLOY_AFTER_SECONDS:-60}"

CURL_CONNECT_TIMEOUT="${WRELA_WRITE_LOAD_CURL_CONNECT_TIMEOUT:-2}"
CURL_MAX_TIME="${WRELA_WRITE_LOAD_CURL_MAX_TIME:-8}"
GATE_CURL_CONNECT_TIMEOUT="${WRELA_WRITE_LOAD_GATE_CURL_CONNECT_TIMEOUT:-1}"
GATE_CURL_MAX_TIME="${WRELA_WRITE_LOAD_GATE_CURL_MAX_TIME:-2}"

URL="https://${APP_NAME}.fly.dev"
REPORT_PATH="$REPORT_DIR/${APP_NAME}-write-load-report.json"
TMP_DIR="$(mktemp -d)"
STAGE_DIR="$TMP_DIR/stages"
mkdir -p "$STAGE_DIR"

STATUS="failed"
FAILURE_REASON=""
STAGE_RESULTS_JSON='[]'

EXIT_INITIAL_DEPLOY_FAIL=20
EXIT_STAGE_A_FAIL=21
EXIT_STAGE_B_FAIL=22
EXIT_GLOBAL_TIMEOUT=23

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

require_tool flyctl
require_tool curl
require_tool jq
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

targeted_live() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$GATE_CURL_CONNECT_TIMEOUT" \
    --max-time "$GATE_CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/live"
}

targeted_health() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$GATE_CURL_CONNECT_TIMEOUT" \
    --max-time "$GATE_CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/health"
}

targeted_mesh() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$GATE_CURL_CONNECT_TIMEOUT" \
    --max-time "$GATE_CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/probe/mesh"
}

targeted_probe_write() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    -X POST \
    "$URL/api/probe/write"
}

targeted_probe_read() {
  local machine_id="$1"
  curl -fsS \
    --connect-timeout "$GATE_CURL_CONNECT_TIMEOUT" \
    --max-time "$GATE_CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    "$URL/api/probe/read"
}

targeted_load_write() {
  local machine_id="$1"
  local _worker_id="$2"
  local _sequence="$3"
  local body_path="$4"
  local latency_path="$5"
  curl -fsS \
    --connect-timeout "$CURL_CONNECT_TIMEOUT" \
    --max-time "$CURL_MAX_TIME" \
    -H "fly-force-instance-id: $machine_id" \
    -X POST \
    -o "$body_path" \
    -w '%{time_total}\n' \
    "$URL/api/load/write" >"$latency_path"
}

verify_machine_identity() {
  local payload="$1"
  local expected_machine="$2"
  local actual_machine
  actual_machine="$(jq -r '.machineId // empty' <<<"$payload")"
  [[ -n "$actual_machine" && "$actual_machine" == "$expected_machine" ]]
}

wait_for_machines() {
  local attempts="${1:-120}"
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

wait_for_all_machines_live() {
  local attempts="${1:-120}"
  local delay_s="${2:-2}"
  local ids
  for _ in $(seq 1 "$attempts"); do
    ids="$(machine_ids)"
    if [[ -z "$ids" ]]; then
      sleep "$delay_s"
      continue
    fi
    local all_live="1"
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_live "$machine_id" 2>/dev/null)"; then
        all_live="0"
        break
      fi
      if ! jq -e '.ok == true and .alive == true' >/dev/null <<<"$payload"; then
        all_live="0"
        break
      fi
      if ! verify_machine_identity "$payload" "$machine_id"; then
        all_live="0"
        break
      fi
    done <<<"$ids"
    if [[ "$all_live" == "1" ]]; then
      return 0
    fi
    sleep "$delay_s"
  done
  return 1
}

wait_for_all_machines_ready() {
  local attempts="${1:-120}"
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
      if ! jq -e '.ok == true and .meshReady == true' >/dev/null <<<"$payload"; then
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
  return 1
}

wait_for_all_machines_mesh_ready() {
  local attempts="${1:-120}"
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
      if ! jq -e --argjson expected "$EXPECTED_MACHINES" '.ok == true and .meshReady == true and (.nodeCount // 0) >= $expected' >/dev/null <<<"$payload"; then
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
  return 1
}

wait_for_targeted_write_success() {
  local machine_id="$1"
  local attempts="${2:-20}"
  local delay_s="${3:-1}"
  for _ in $(seq 1 "$attempts"); do
    local payload
    if payload="$(targeted_probe_write "$machine_id" 2>/dev/null)"; then
      if jq -e '.ok == true and (.version // 0) > 0 and (.value // "") != ""' >/dev/null <<<"$payload" \
        && verify_machine_identity "$payload" "$machine_id"; then
        printf '%s\n' "$payload"
        return 0
      fi
    fi
    sleep "$delay_s"
  done
  return 1
}

wait_for_targeted_convergence() {
  local expected_value="$1"
  local attempts="${2:-45}"
  local delay_s="${3:-1}"
  local ids
  ids="$(machine_ids)"
  [[ -n "$ids" ]] || return 1

  for _ in $(seq 1 "$attempts"); do
    local converged="1"
    while IFS= read -r machine_id; do
      local payload
      if ! payload="$(targeted_probe_read "$machine_id" 2>/dev/null)"; then
        converged="0"
        break
      fi
      if ! jq -e --arg expected "$expected_value" '.ok == true and .value == $expected' >/dev/null <<<"$payload"; then
        converged="0"
        break
      fi
      if ! verify_machine_identity "$payload" "$machine_id"; then
        converged="0"
        break
      fi
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
  [[ -n "$ids" ]] || return 1

  while IFS= read -r machine_id; do
    local write_payload
    if ! write_payload="$(wait_for_targeted_write_success "$machine_id" 20 1)"; then
      echo "targeted write failed machine=$machine_id after retries" >&2
      return 1
    fi

    local expected
    expected="$(jq -r '.value // ""' <<<"$write_payload")"
    if [[ -z "$expected" ]]; then
      echo "targeted write missing expected value machine=$machine_id payload=$write_payload" >&2
      return 1
    fi

    if ! wait_for_targeted_convergence "$expected" 45 1; then
      echo "targeted convergence failed machine=$machine_id expected=$expected" >&2
      return 1
    fi
  done <<<"$ids"
}

capture_mesh_diagnostics() {
  local rows='[]'
  local ids
  ids="$(machine_ids || true)"
  if [[ -z "$ids" ]]; then
    echo '[]'
    return 0
  fi
  while IFS= read -r machine_id; do
    if payload="$(targeted_mesh "$machine_id" 2>/dev/null)"; then
      rows="$(jq -c --arg machine_id "$machine_id" --argjson payload "$payload" '. + [{machineId:$machine_id,payload:$payload}]' <<<"$rows")"
    else
      rows="$(jq -c --arg machine_id "$machine_id" '. + [{machineId:$machine_id,error:"mesh_probe_failed"}]' <<<"$rows")"
    fi
  done <<<"$ids"
  printf '%s\n' "$rows"
}

append_stage_result() {
  local stage_json="$1"
  STAGE_RESULTS_JSON="$(jq -c --argjson stage "$stage_json" '. + [$stage]' <<<"$STAGE_RESULTS_JSON")"
}

build_stage_payload() {
  local stage_name="$1"
  local duration_seconds="$2"
  local concurrency="$3"
  local max_failure_rate_pct="$4"
  local redeploy_enabled="$5"
  local redeploy_status="$6"
  local redeploy_error="$7"
  local pre_gate_status="$8"
  local post_gate_status="$9"
  local stage_status="${10}"
  local stage_reason="${11}"
  local worker_dir="${12}"

  local worker_rows_json totals_json total success failed latency_ms_sum latency_ms_max failure_rate avg_latency throughput
  if compgen -G "$worker_dir/*.summary.json" >/dev/null; then
    worker_rows_json="$(jq -s '.' "$worker_dir"/*.summary.json)"
  else
    worker_rows_json='[]'
  fi

  totals_json="$(jq -n --argjson workers "$worker_rows_json" '
    reduce $workers[] as $w (
      {total:0,success:0,failed:0,latency_ms_sum:0,latency_ms_max:0};
      .total += ($w.total // 0)
      | .success += ($w.success // 0)
      | .failed += ($w.failed // 0)
      | .latency_ms_sum += ($w.latency_ms_sum // 0)
      | if ($w.latency_ms_max // 0) > .latency_ms_max
        then .latency_ms_max = ($w.latency_ms_max // 0)
        else .
        end
    )
  ' )"

  total="$(jq -r '.total // 0' <<<"$totals_json")"
  success="$(jq -r '.success // 0' <<<"$totals_json")"
  failed="$(jq -r '.failed // 0' <<<"$totals_json")"
  latency_ms_sum="$(jq -r '.latency_ms_sum // 0' <<<"$totals_json")"
  latency_ms_max="$(jq -r '.latency_ms_max // 0' <<<"$totals_json")"

  if [[ "$total" -gt 0 ]]; then
    failure_rate="$(awk -v f="$failed" -v t="$total" 'BEGIN { printf "%.4f", (f*100.0)/t }')"
    avg_latency="$(awk -v s="$latency_ms_sum" -v t="$total" 'BEGIN { printf "%.2f", s/t }')"
    throughput="$(awk -v s="$success" -v d="$duration_seconds" 'BEGIN { if (d <= 0) printf "0.00"; else printf "%.2f", s/d }')"
  else
    failure_rate="0.0000"
    avg_latency="0.00"
    throughput="0.00"
  fi

  jq -nc \
    --arg name "$stage_name" \
    --arg status "$stage_status" \
    --arg reason "$stage_reason" \
    --arg pre_gate_status "$pre_gate_status" \
    --arg post_gate_status "$post_gate_status" \
    --arg redeploy_status "$redeploy_status" \
    --arg redeploy_error "$redeploy_error" \
    --argjson redeploy_enabled "$redeploy_enabled" \
    --argjson duration_seconds "$duration_seconds" \
    --argjson concurrency "$concurrency" \
    --argjson max_failure_rate_pct "$max_failure_rate_pct" \
    --argjson total "$total" \
    --argjson success "$success" \
    --argjson failed "$failed" \
    --argjson latency_ms_max "$latency_ms_max" \
    --argjson worker_summaries "$worker_rows_json" \
    --arg failure_rate "$failure_rate" \
    --arg avg_latency "$avg_latency" \
    --arg throughput "$throughput" \
    '{
      name: $name,
      status: $status,
      reason: $reason,
      durationSeconds: $duration_seconds,
      concurrency: $concurrency,
      maxFailureRatePct: $max_failure_rate_pct,
      gates: {
        pre: $pre_gate_status,
        post: $post_gate_status
      },
      rollingRedeploy: {
        enabled: ($redeploy_enabled == 1),
        status: $redeploy_status,
        error: $redeploy_error
      },
      metrics: {
        totalRequests: $total,
        successfulRequests: $success,
        failedRequests: $failed,
        failureRatePct: ($failure_rate | tonumber),
        avgLatencyMs: ($avg_latency | tonumber),
        maxLatencyMs: $latency_ms_max,
        successfulWritesPerSec: ($throughput | tonumber)
      },
      workerSummaries: $worker_summaries
    }'
}

write_report() {
  local mesh_json
  mesh_json="$(capture_mesh_diagnostics || echo '[]')"

  jq -n \
    --arg app "$APP_NAME" \
    --arg url "$URL" \
    --arg region "$REGION" \
    --arg status "$STATUS" \
    --arg reason "$FAILURE_REASON" \
    --arg replication_outside_lock "1" \
    --arg wal_encode_outside_lock "1" \
    --arg replicated_log_backend "canonical_only" \
    --arg insert_fast_lane "1" \
    --arg latency_frontier_mode "1" \
    --argjson expected_machines "$EXPECTED_MACHINES" \
    --argjson stages "$STAGE_RESULTS_JSON" \
    --argjson mesh_diagnostics "$mesh_json" \
    --arg generated_at "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    '{
      app: $app,
      url: $url,
      region: $region,
      status: $status,
      reason: $reason,
      expectedMachines: $expected_machines,
      stages: $stages,
      meshDiagnostics: $mesh_diagnostics,
      jupiterDefaults: {
        replicationOutsideLock: $replication_outside_lock,
        walEncodeOutsideLock: $wal_encode_outside_lock,
        replicatedLogBackend: $replicated_log_backend,
        insertFastLaneActive: $insert_fast_lane,
        latencyFrontierMode: $latency_frontier_mode
      },
      generatedAt: $generated_at
    }' >"$REPORT_PATH"
}

fail() {
  local reason="$1"
  local exit_code="$2"
  FAILURE_REASON="$reason"
  STATUS="failed"
  write_report
  echo "[write-load] failure: $FAILURE_REASON" >&2
  echo "[write-load] report: $REPORT_PATH" >&2
  echo "[write-load] app preserved for debugging" >&2
  echo "[write-load] inspect: flyctl status -a $APP_NAME" >&2
  echo "[write-load] logs: flyctl logs -a $APP_NAME" >&2
  echo "[write-load] cleanup: flyctl apps destroy $APP_NAME --yes" >&2
  exit "$exit_code"
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
  else
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
  fi
}

run_stage_gates() {
  local stage_name="$1"
  local phase="$2"

  if ! wait_for_machines 120 2; then
    return 10
  fi
  if ! wait_for_all_machines_live 120 2; then
    return 11
  fi
  if ! wait_for_all_machines_ready 120 2; then
    return 12
  fi
  if ! wait_for_all_machines_mesh_ready 120 2; then
    return 13
  fi
  if ! verify_targeted_roundtrip_all_machines; then
    return 14
  fi
  echo "[write-load] gate ${stage_name} ${phase}: passed"
  return 0
}

spawn_stage_worker() {
  local stage_name="$1"
  local worker_id="$2"
  local target_machine="$3"
  local end_epoch="$4"
  local worker_dir="$5"

  local summary_path="$worker_dir/worker-${worker_id}.summary.json"
  local body_path="$worker_dir/worker-${worker_id}.body.json"
  local latency_path="$worker_dir/worker-${worker_id}.latency"
  local error_path="$worker_dir/worker-${worker_id}.errors.log"

  (
    local total=0
    local success=0
    local failed=0
    local latency_ms_sum=0
    local latency_ms_max=0
    local sequence=0
    : >"$error_path"

    while [[ "$(date +%s)" -lt "$end_epoch" ]]; do
      sequence=$((sequence + 1))
      total=$((total + 1))
      if targeted_load_write "$target_machine" "$worker_id" "$sequence" "$body_path" "$latency_path" 2>/dev/null; then
        local payload latency_sec latency_ms
        payload="$(cat "$body_path")"
        latency_sec="$(cat "$latency_path")"
        latency_ms="$(awk -v t="$latency_sec" 'BEGIN { printf "%d", (t*1000.0)+0.5 }')"
        latency_ms_sum=$((latency_ms_sum + latency_ms))
        if [[ "$latency_ms" -gt "$latency_ms_max" ]]; then
          latency_ms_max="$latency_ms"
        fi

        if jq -e '.ok == true and (.version // 0) > 0 and (.key // "") != "" and (.value // "") != ""' >/dev/null <<<"$payload" \
          && verify_machine_identity "$payload" "$target_machine"; then
          success=$((success + 1))
        else
          failed=$((failed + 1))
          if [[ "$failed" -le 20 ]]; then
            printf 'stage=%s worker=%s machine=%s type=invalid_payload payload=%s\n' "$stage_name" "$worker_id" "$target_machine" "$payload" >>"$error_path"
          fi
        fi
      else
        failed=$((failed + 1))
        if [[ "$failed" -le 20 ]]; then
          printf 'stage=%s worker=%s machine=%s type=request_failed\n' "$stage_name" "$worker_id" "$target_machine" >>"$error_path"
        fi
      fi
    done

    jq -n \
      --arg stage "$stage_name" \
      --arg worker "$worker_id" \
      --arg machine "$target_machine" \
      --argjson total "$total" \
      --argjson success "$success" \
      --argjson failed "$failed" \
      --argjson latency_ms_sum "$latency_ms_sum" \
      --argjson latency_ms_max "$latency_ms_max" \
      '{
        stage: $stage,
        worker: $worker,
        machine: $machine,
        total: $total,
        success: $success,
        failed: $failed,
        latency_ms_sum: $latency_ms_sum,
        latency_ms_max: $latency_ms_max
      }' >"$summary_path"
  ) &

  WORKER_PIDS+=" $!"
}

run_stage() {
  local stage_name="$1"
  local duration_seconds="$2"
  local concurrency="$3"
  local max_failure_rate_pct="$4"
  local rolling_redeploy="$5"
  local redeploy_after_seconds="$6"
  local failure_exit_code="$7"

  local stage_worker_dir="$STAGE_DIR/$stage_name/workers"
  mkdir -p "$stage_worker_dir"

  local pre_gate_status="failed"
  local post_gate_status="skipped"
  local stage_status="pending"
  local stage_reason=""
  local redeploy_status="skipped"
  local redeploy_error=""

  echo "[write-load] stage=$stage_name duration=${duration_seconds}s concurrency=$concurrency redeploy=$rolling_redeploy"

  if run_stage_gates "$stage_name" "pre"; then
    pre_gate_status="passed"
  else
    local gate_code=$?
    stage_reason="${stage_name}_pre_gate_failed_${gate_code}"
    stage_status="failed"
  fi

  local worker_pids=""
  if [[ "$pre_gate_status" == "passed" ]]; then
    local ids=()
    while IFS= read -r id; do
      ids+=("$id")
    done < <(machine_ids)
    if [[ "${#ids[@]}" -eq 0 ]]; then
      stage_reason="${stage_name}_no_machine_ids"
      stage_status="failed"
    else
      WORKER_PIDS=""
      local end_epoch
      end_epoch="$(( $(date +%s) + duration_seconds ))"
      for worker in $(seq 1 "$concurrency"); do
        local index=$(( (worker - 1) % ${#ids[@]} ))
        spawn_stage_worker "$stage_name" "$worker" "${ids[$index]}" "$end_epoch" "$stage_worker_dir"
      done
      worker_pids="$WORKER_PIDS"

      if [[ "$rolling_redeploy" == "1" ]]; then
        redeploy_status="running"
        sleep "$redeploy_after_seconds"
        if run_deploy; then
          redeploy_status="passed"
          redeploy_error=""
        else
          redeploy_status="failed"
          redeploy_error="${stage_name}_rolling_redeploy_failed"
        fi
      fi

      for pid in $worker_pids; do
        wait "$pid" || true
      done

      if [[ "$redeploy_status" == "failed" ]]; then
        stage_reason="$redeploy_error"
        stage_status="failed"
      fi
    fi
  fi

  if [[ "$stage_status" != "failed" ]]; then
    if run_stage_gates "$stage_name" "post"; then
      post_gate_status="passed"
    else
      local gate_code=$?
      post_gate_status="failed"
      stage_reason="${stage_name}_post_gate_failed_${gate_code}"
      stage_status="failed"
    fi
  fi

  if [[ "$stage_status" != "failed" ]]; then
    stage_status="passed"
    stage_reason=""
  fi

  local stage_payload
  stage_payload="$(build_stage_payload \
    "$stage_name" \
    "$duration_seconds" \
    "$concurrency" \
    "$max_failure_rate_pct" \
    "$rolling_redeploy" \
    "$redeploy_status" \
    "$redeploy_error" \
    "$pre_gate_status" \
    "$post_gate_status" \
    "$stage_status" \
    "$stage_reason" \
    "$stage_worker_dir")"

  local observed_failure_rate
  observed_failure_rate="$(jq -r '.metrics.failureRatePct // 100' <<<"$stage_payload")"
  if [[ "$stage_status" == "passed" ]] && ! awk -v observed="$observed_failure_rate" -v max="$max_failure_rate_pct" 'BEGIN { exit !(observed <= max) }'; then
    stage_status="failed"
    stage_reason="${stage_name}_failure_rate_above_threshold"
    stage_payload="$(jq -c --arg status "$stage_status" --arg reason "$stage_reason" '.status = $status | .reason = $reason' <<<"$stage_payload")"
  fi

  append_stage_result "$stage_payload"

  if [[ "$stage_status" != "passed" ]]; then
    fail "$stage_reason" "$failure_exit_code"
  fi

  echo "[write-load] stage=$stage_name passed failureRatePct=$observed_failure_rate"
}

cleanup() {
  for pid in $(jobs -pr 2>/dev/null); do
    kill "$pid" >/dev/null 2>&1 || true
  done
  if [[ -n "${WATCHDOG_PID:-}" ]]; then
    kill "$WATCHDOG_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$STATUS" == "passed" && "$AUTO_DESTROY" == "1" ]]; then
    flyctl apps destroy "$APP_NAME" --yes >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

on_timeout() {
  FAILURE_REASON="global_timeout_exceeded"
  STATUS="failed"
  write_report
  echo "[write-load] failure: global timeout (${SCRIPT_TIMEOUT_SECONDS}s)" >&2
  echo "[write-load] report: $REPORT_PATH" >&2
  echo "[write-load] app preserved for debugging" >&2
  exit "$EXIT_GLOBAL_TIMEOUT"
}
trap on_timeout TERM

(
  sleep "$SCRIPT_TIMEOUT_SECONDS"
  kill -s TERM $$ >/dev/null 2>&1 || true
) &
WATCHDOG_PID=$!

echo "[write-load] app=$APP_NAME region=$REGION machines=$MACHINES expected=$EXPECTED_MACHINES"
if ! run_deploy; then
  fail "initial_deploy_failed" "$EXIT_INITIAL_DEPLOY_FAIL"
fi

run_stage \
  "stageA" \
  "$STAGE_A_DURATION_SECONDS" \
  "$STAGE_A_CONCURRENCY" \
  "$STAGE_A_MAX_FAILURE_RATE_PCT" \
  "$STAGE_A_ROLLING_REDEPLOY" \
  "0" \
  "$EXIT_STAGE_A_FAIL"

run_stage \
  "stageB" \
  "$STAGE_B_DURATION_SECONDS" \
  "$STAGE_B_CONCURRENCY" \
  "$STAGE_B_MAX_FAILURE_RATE_PCT" \
  "$STAGE_B_ROLLING_REDEPLOY" \
  "$STAGE_B_REDEPLOY_AFTER_SECONDS" \
  "$EXIT_STAGE_B_FAIL"

STATUS="passed"
FAILURE_REASON=""
write_report

echo "[write-load] success app=$APP_NAME report=$REPORT_PATH"
if [[ "$AUTO_DESTROY" == "1" ]]; then
  echo "[write-load] app will be destroyed during cleanup"
else
  echo "[write-load] app kept alive because WRELA_WRITE_LOAD_AUTO_DESTROY=$AUTO_DESTROY"
fi

exit 0
