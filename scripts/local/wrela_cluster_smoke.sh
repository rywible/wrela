#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_ROOT="$ROOT/artifacts/local"
RUN_ID="${1:-local-smoke-$(date +%Y%m%d%H%M%S)}"
RUN_DIR="$ARTIFACT_ROOT/$RUN_ID"
REPORT_PATH="$ARTIFACT_ROOT/${RUN_ID}-smoke-report.json"
mkdir -p "$RUN_DIR/logs"

NODE_IDS=(node-a node-b node-c)
BASE_PORT=$((20000 + (RANDOM % 15000)))
HTTP_PORTS=("$((BASE_PORT + 1))" "$((BASE_PORT + 2))" "$((BASE_PORT + 3))")
RPC_PORTS=("$((BASE_PORT + 101))" "$((BASE_PORT + 102))" "$((BASE_PORT + 103))")

APP_BIN="$RUN_DIR/wrela_local_cluster_node"
TRAFFIC_STOP="$RUN_DIR/traffic.stop"
TRAFFIC_SUMMARY="$RUN_DIR/traffic-summary.json"
STATUS="failed"
REASON=""

declare -a PIDS=()

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "missing required tool: $tool" >&2
    exit 1
  fi
}

require_tool cargo
require_tool curl
require_tool jq

node_index() {
  local node_id="$1"
  case "$node_id" in
    node-a) echo 0 ;;
    node-b) echo 1 ;;
    node-c) echo 2 ;;
    *) return 1 ;;
  esac
}

node_url() {
  local idx="$1"
  echo "http://127.0.0.1:${HTTP_PORTS[$idx]}"
}

cluster_nodes_csv() {
  IFS=,; echo "${NODE_IDS[*]}"
}

address_map_csv() {
  local parts=()
  for idx in "${!NODE_IDS[@]}"; do
    parts+=("${NODE_IDS[$idx]}=127.0.0.1:${RPC_PORTS[$idx]}")
  done
  IFS=,; echo "${parts[*]}"
}

kill_nodes() {
  for pid in "${PIDS[@]:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
}

write_report() {
  local total=0
  local failures=0
  if [[ -f "$TRAFFIC_SUMMARY" ]]; then
    total="$(jq -r '.total // 0' "$TRAFFIC_SUMMARY")"
    failures="$(jq -r '.failures // 0' "$TRAFFIC_SUMMARY")"
  fi
  jq -n \
    --arg run_id "$RUN_ID" \
    --arg status "$STATUS" \
    --arg reason "$REASON" \
    --arg run_dir "$RUN_DIR" \
    --argjson total "$total" \
    --argjson failures "$failures" \
    --arg generated_at "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    '{
      runId: $run_id,
      status: $status,
      reason: $reason,
      nodeCount: 3,
      traffic: {
        total: $total,
        failures: $failures
      },
      logsDir: ($run_dir + "/logs"),
      generatedAt: $generated_at
    }' >"$REPORT_PATH"
}

fail() {
  REASON="$1"
  STATUS="failed"
  write_report
  echo "[local-smoke] failure: $REASON" >&2
  echo "[local-smoke] report: $REPORT_PATH" >&2
  echo "[local-smoke] logs: $RUN_DIR/logs" >&2
  exit 1
}

cleanup() {
  if [[ -f "$TRAFFIC_STOP" ]]; then
    rm -f "$TRAFFIC_STOP"
  fi
  if [[ -n "${TRAFFIC_PID:-}" ]]; then
    wait "$TRAFFIC_PID" >/dev/null 2>&1 || true
  fi
  kill_nodes
}
trap cleanup EXIT

start_node() {
  local idx="$1"
  local node_id="${NODE_IDS[$idx]}"
  local data_dir="$RUN_DIR/data-$node_id"
  local stdout_log="$RUN_DIR/logs/$node_id.stdout.log"
  local stderr_log="$RUN_DIR/logs/$node_id.stderr.log"
  mkdir -p "$data_dir"
  : >"$stdout_log"
  : >"$stderr_log"

  PORT="${HTTP_PORTS[$idx]}" \
  WRELADB_DATA_DIR="$data_dir" \
  WRELADB_PRIVATE_RPC_ENABLED=1 \
  WRELADB_PRIVATE_RPC_PORT="${RPC_PORTS[$idx]}" \
  WRELADB_PRIVATE_RPC_BIND="127.0.0.1:${RPC_PORTS[$idx]}" \
  WRELADB_PRIVATE_RPC_MTLS_MODE=off \
  WRELADB_PRIVATE_RPC_TRUSTED_NETWORK=local-loopback \
  WRELADB_NODE_ID="$node_id" \
  FLY_MACHINE_ID="$node_id" \
  WRELADB_CLUSTER_NODES="$(cluster_nodes_csv)" \
  WRELADB_PRIVATE_RPC_ADDRESS_MAP="$(address_map_csv)" \
  WRELADB_REPLICATION_FACTOR=1 \
  WRELADB_WRITE_QUORUM=1 \
  WRELADB_TARGET_VOTERS=3 \
  WRELADB_PRIVATE_RPC_MIN_READY_NODES=1 \
  WRELADB_PRIVATE_RPC_TIMEOUT_MS=1000 \
  "$APP_BIN" >"$stdout_log" 2>"$stderr_log" &
  PIDS[$idx]=$!
}

restart_node() {
  local idx="$1"
  local pid="${PIDS[$idx]:-}"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" >/dev/null 2>&1 || true
  fi
  start_node "$idx"
}

wait_for_healthy() {
  local attempts="${1:-120}"
  for _ in $(seq 1 "$attempts"); do
    local ready=1
    for idx in "${!NODE_IDS[@]}"; do
      local url
      url="$(node_url "$idx")"
      local payload
      if ! payload="$(curl -fsS --connect-timeout 1 --max-time 2 "$url/api/health" 2>/dev/null)"; then
        ready=0
        break
      fi
      local ok
      ok="$(jq -r '.ok // false' <<<"$payload")"
      if [[ "$ok" != "true" ]]; then
        ready=0
        break
      fi
    done
    if [[ "$ready" == "1" ]]; then
      return 0
    fi
    sleep 0.2
  done
  return 1
}

wait_for_mesh_ready() {
  local attempts="${1:-120}"
  for _ in $(seq 1 "$attempts"); do
    local ready=1
    local leaders=()
    for idx in "${!NODE_IDS[@]}"; do
      local url payload
      url="$(node_url "$idx")"
      if ! payload="$(curl -fsS --connect-timeout 1 --max-time 2 "$url/api/probe/mesh" 2>/dev/null)"; then
        ready=0
        break
      fi
      if ! jq -e '.ok == true and .meshReady == true and .nodeCount == 3' >/dev/null <<<"$payload"; then
        ready=0
        break
      fi
      if ! jq -e '[.nodes[]] | sort == ["node-a","node-b","node-c"]' >/dev/null <<<"$payload"; then
        ready=0
        break
      fi
      leaders+=("$(jq -r '.leaderId // ""' <<<"$payload")")
    done
    if [[ "$ready" == "1" ]]; then
      local unique_count
      unique_count="$(printf "%s\n" "${leaders[@]}" | sort -u | wc -l | tr -d ' ')"
      if [[ "$unique_count" == "1" ]]; then
        return 0
      fi
    fi
    sleep 0.2
  done
  return 1
}

write_and_verify_node() {
  local idx="$1"
  local url payload ok version expected
  url="$(node_url "$idx")"
  payload="$(curl -fsS --connect-timeout 1 --max-time 3 -X POST "$url/api/probe/write")"
  ok="$(jq -r '.ok // false' <<<"$payload")"
  version="$(jq -r '.version // -1' <<<"$payload")"
  expected="$(jq -r '.value // ""' <<<"$payload")"
  if [[ "$ok" != "true" || "$version" -le 0 || -z "$expected" ]]; then
    echo "write failed idx=$idx payload=$payload" >&2
    return 1
  fi
  local attempts=60
  for _ in $(seq 1 "$attempts"); do
    local read_payload
    if read_payload="$(curl -fsS --connect-timeout 1 --max-time 2 "$url/api/probe/read" 2>/dev/null)"; then
      if jq -e --arg value "$expected" '.ok == true and .value == $value' >/dev/null <<<"$read_payload"; then
        return 0
      fi
    fi
    sleep 0.1
  done
  echo "read-your-write failed idx=$idx expected=$expected" >&2
  return 1
}

start_traffic_loop() {
  (
    local total=0
    local failures=0
    local idx=0
    while [[ ! -f "$TRAFFIC_STOP" ]]; do
      local node_idx=$((idx % 3))
      local url payload ok
      url="$(node_url "$node_idx")"
      if payload="$(curl -fsS --connect-timeout 1 --max-time 2 "$url/api/health" 2>/dev/null)"; then
        ok="$(jq -r '.ok // false' <<<"$payload")"
        if [[ "$ok" != "true" ]]; then
          failures=$((failures + 1))
        fi
      else
        failures=$((failures + 1))
      fi
      total=$((total + 1))
      printf '{"total":%d,"failures":%d}\n' "$total" "$failures" >"$TRAFFIC_SUMMARY"
      idx=$((idx + 1))
      sleep 0.05
    done
    printf '{"total":%d,"failures":%d}\n' "$total" "$failures" >"$TRAFFIC_SUMMARY"
  ) &
  TRAFFIC_PID=$!
}

stop_traffic_loop() {
  : >"$TRAFFIC_STOP"
  wait "$TRAFFIC_PID"
}

echo "[local-smoke] run_id=$RUN_ID run_dir=$RUN_DIR"
echo "[local-smoke] building local cluster node binary..."
cargo build --manifest-path "$ROOT/apps/wreladb-lab/Cargo.toml" --bin wreladb_lab
cp "$ROOT/apps/wreladb-lab/target/debug/wreladb_lab" "$APP_BIN"
chmod +x "$APP_BIN"

for idx in "${!NODE_IDS[@]}"; do
  start_node "$idx"
done

wait_for_healthy 160 || fail "cluster_not_healthy_after_boot"
wait_for_mesh_ready 160 || fail "cluster_not_mesh_ready_after_boot"

for idx in "${!NODE_IDS[@]}"; do
  write_and_verify_node "$idx" || fail "write_read_convergence_failed_boot_phase"
done

start_traffic_loop

leader="$(curl -fsS "$(node_url 0)/api/probe/mesh" | jq -r '.leaderId // ""')"
[[ -n "$leader" ]] || fail "leader_unavailable_before_rolling"
if [[ "$leader" == "node-a" ]]; then
  follower="node-b"
else
  follower="node-a"
fi
follower_idx="$(node_index "$follower")" || fail "invalid_follower_index"
leader_idx="$(node_index "$leader")" || fail "invalid_leader_index"

restart_node "$follower_idx"
wait_for_healthy 160 || fail "cluster_not_healthy_after_follower_restart"
wait_for_mesh_ready 160 || fail "cluster_not_mesh_ready_after_follower_restart"

restart_node "$leader_idx"
wait_for_healthy 160 || fail "cluster_not_healthy_after_leader_restart"
wait_for_mesh_ready 160 || fail "cluster_not_mesh_ready_after_leader_restart"

stop_traffic_loop
traffic_failures="$(jq -r '.failures // 0' "$TRAFFIC_SUMMARY")"
if [[ "$traffic_failures" != "0" ]]; then
  fail "traffic_failures_during_rolling_restart"
fi

for idx in "${!NODE_IDS[@]}"; do
  write_and_verify_node "$idx" || fail "write_read_convergence_failed_post_rolling"
done

STATUS="passed"
REASON=""
write_report

echo "[local-smoke] success report=$REPORT_PATH"
echo "[local-smoke] logs=$RUN_DIR/logs"
