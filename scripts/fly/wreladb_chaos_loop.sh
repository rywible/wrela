#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_DIR="$ROOT/artifacts/fly"
mkdir -p "$REPORT_DIR"

APP_NAME="${1:-}"
CYCLES="${2:-20}"
DEPLOY_EVERY="${3:-5}"

if [[ -z "$APP_NAME" ]]; then
  APP_NAME="$($ROOT/scripts/fly/wreladb_bootstrap.sh | awk -F= '/^APP_NAME=/{print $2}')"
fi

URL="https://${APP_NAME}.fly.dev"
STAMP="$(date +%Y%m%d%H%M%S)"
RUN_DIR="$REPORT_DIR/${APP_NAME}-chaos-${STAMP}"
mkdir -p "$RUN_DIR"
NDJSON="$RUN_DIR/cycles.ndjson"
: > "$NDJSON"

health_ok() {
  curl -fsS "$URL/api/health" | jq -e '.ok == true' >/dev/null
}

probe_ok() {
  local write_json
  local expected
  local cluster_json
  write_json="$(curl -fsS -X POST "$URL/api/probe/write")"
  if ! jq -e '
    (.ok // false) == true
    and (.replicationAcks // 0) >= (.requiredAcks // 1)
  ' <<<"$write_json" >/dev/null; then
    return 1
  fi
  expected="$(jq -r '.value // empty' <<<"$write_json")"
  if [[ -z "$expected" ]]; then
    return 1
  fi
  cluster_json="$(curl -fsS "$URL/api/probe/cluster_read")"
  jq -e --arg expected "$expected" '
    (.discoveryComplete // false) == true
    and (.discoveredCount // 0) >= (.targetVoters // 3)
    and ([.readings[] | select(.ok != true or (.value // "") != $expected)] | length) == 0
  ' <<<"$cluster_json" >/dev/null
}

machine_count() {
  flyctl machines list -a "$APP_NAME" --json | jq 'length'
}

list_machine_ids() {
  flyctl machines list -a "$APP_NAME" --json | jq -r '.[].id' | sort
}

cluster_nodes_csv() {
  list_machine_ids | paste -sd, -
}

sync_cluster_nodes_secret() {
  local attempts=8
  local delay_s=2
  local attempt
  for attempt in $(seq 1 "$attempts"); do
    local nodes
    nodes="$(cluster_nodes_csv || true)"
    if [[ -n "$nodes" ]]; then
      echo "[chaos] syncing WRELADB_CLUSTER_NODES=$nodes (attempt $attempt/$attempts)"
      if flyctl secrets set WRELADB_CLUSTER_NODES="$nodes" -a "$APP_NAME" >/dev/null; then
        return 0
      fi
    fi
    if [[ "$attempt" -lt "$attempts" ]]; then
      sleep "$delay_s"
    fi
  done
  echo "[chaos] failed to sync WRELADB_CLUSTER_NODES after $attempts attempts" >&2
  return 1
}

wait_for_healthy_cluster() {
  local timeout_s="${1:-240}"
  local start
  start="$(date +%s)"
  while true; do
    local count
    count="$(machine_count)"
    if [[ "$count" -ge 3 ]] && health_ok; then
      return 0
    fi
    if (( $(date +%s) - start > timeout_s )); then
      return 1
    fi
    sleep 2
  done
}

recover_to_three_nodes() {
  local source
  while [[ "$(machine_count)" -lt 3 ]]; do
    source="$(flyctl machines list -a "$APP_NAME" --json | jq -r '.[0].id')"
    flyctl machine clone "$source" -a "$APP_NAME" --region ord >/dev/null
    sleep 2
  done
}

echo "[chaos] app=$APP_NAME cycles=$CYCLES deploy_every=$DEPLOY_EVERY"
wait_for_healthy_cluster 240
sync_cluster_nodes_secret
wait_for_healthy_cluster 240

pass_count=0
fail_count=0

for i in $(seq 1 "$CYCLES"); do
  start_epoch="$(date +%s)"
  scenario="single_fault"
  fault_nodes=()
  status="pass"
  message="ok"

  if ! wait_for_healthy_cluster 180; then
    status="fail"
    message="pre-cycle cluster unhealthy"
  else
    victim1="$(flyctl machines list -a "$APP_NAME" --json | jq -r '.[0].id')"
    fault_nodes+=("$victim1")
    echo "[chaos:$i] destroying $victim1"
    flyctl machine destroy "$victim1" -a "$APP_NAME" --force >/dev/null

    if (( i % 5 == 0 )); then
      scenario="double_fault_mid_reconcile"
      sleep 3
      remaining="$(flyctl machines list -a "$APP_NAME" --json | jq -r '.[0].id')"
      fault_nodes+=("$remaining")
      echo "[chaos:$i] mid-reconcile destroy $remaining"
      flyctl machine destroy "$remaining" -a "$APP_NAME" --force >/dev/null
    fi

    recover_to_three_nodes
    if ! sync_cluster_nodes_secret; then
      status="fail"
      message="failed to sync cluster-node inventory"
    elif ! wait_for_healthy_cluster 240; then
      status="fail"
      message="cluster did not recover to healthy=3"
    elif ! probe_ok; then
      status="fail"
      message="read/write probe failed after recovery"
    fi
  fi

  if [[ "$status" == "pass" ]] && (( i % DEPLOY_EVERY == 0 )); then
    echo "[chaos:$i] rolling deploy gate"
    if ! "$ROOT/scripts/fly/wreladb_deploy_safe.sh" "$APP_NAME" >/dev/null; then
      status="fail"
      message="deploy-safe failed"
    elif ! probe_ok; then
      status="fail"
      message="probe failed after deploy-safe"
    fi
  fi

  end_epoch="$(date +%s)"
  duration="$((end_epoch - start_epoch))"
  final_count="$(machine_count)"
  if [[ "$status" == "pass" ]]; then
    ((pass_count+=1))
  else
    ((fail_count+=1))
  fi

  jq -nc \
    --argjson cycle "$i" \
    --arg scenario "$scenario" \
    --arg status "$status" \
    --arg message "$message" \
    --argjson duration_s "$duration" \
    --argjson machine_count "$final_count" \
    --argjson faults "$(printf '%s\n' "${fault_nodes[@]}" | jq -R . | jq -s .)" \
    '{cycle:$cycle,scenario:$scenario,status:$status,message:$message,duration_s:$duration_s,machine_count:$machine_count,faults:$faults}' \
    >> "$NDJSON"

  echo "[chaos:$i] status=$status scenario=$scenario count=$final_count duration=${duration}s"
done

SUMMARY_JSON="$RUN_DIR/summary.json"
SUMMARY_MD="$RUN_DIR/summary.md"

jq -nc \
  --arg app "$APP_NAME" \
  --arg url "$URL" \
  --arg started_at "$STAMP" \
  --argjson cycles "$CYCLES" \
  --argjson passed "$pass_count" \
  --argjson failed "$fail_count" \
  --arg ndjson "$NDJSON" \
  '{app:$app,url:$url,started_at:$started_at,cycles:$cycles,passed:$passed,failed:$failed,cycle_log:$ndjson}' \
  > "$SUMMARY_JSON"

cat > "$SUMMARY_MD" <<MD
# WrelaDB Fly Chaos Loop Report

- App: $APP_NAME
- URL: $URL
- Cycles requested: $CYCLES
- Cycles passed: $pass_count
- Cycles failed: $fail_count
- Cycle log: $NDJSON

## Failure Details

$(jq -r 'select(.status!="pass") | "- cycle \(.cycle): \(.scenario) :: \(.message)"' "$NDJSON")
MD

echo "[chaos] summary: $SUMMARY_MD"
echo "APP_NAME=$APP_NAME"
echo "SUMMARY_MD=$SUMMARY_MD"
