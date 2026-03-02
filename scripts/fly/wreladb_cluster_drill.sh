#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_DIR="$ROOT/artifacts/fly"
mkdir -p "$REPORT_DIR"

APP_NAME="${1:-}"
if [[ -z "$APP_NAME" ]]; then
  APP_NAME="$($ROOT/scripts/fly/wreladb_bootstrap.sh | awk -F= '/^APP_NAME=/{print $2}')"
fi

URL="https://${APP_NAME}.fly.dev"
REPORT="$REPORT_DIR/${APP_NAME}-drill-report.md"
AUTO_DESTROY="${WRELADB_LAB_AUTO_DESTROY:-1}"

cleanup() {
  if [[ "$AUTO_DESTROY" == "1" ]]; then
    "$ROOT/scripts/fly/wreladb_destroy_lab.sh" "$APP_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

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
      echo "[drill] syncing WRELADB_CLUSTER_NODES=$nodes (attempt $attempt/$attempts)"
      if flyctl secrets set WRELADB_CLUSTER_NODES="$nodes" -a "$APP_NAME" >/dev/null; then
        return 0
      fi
    fi
    if [[ "$attempt" -lt "$attempts" ]]; then
      sleep "$delay_s"
    fi
  done
  echo "[drill] failed to sync WRELADB_CLUSTER_NODES after $attempts attempts" >&2
  return 1
}

write_probe() {
  curl -fsS --connect-timeout 3 --max-time 15 --retry 3 --retry-delay 1 --retry-all-errors -X POST "$URL/api/probe/write"
}

read_probe() {
  curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/probe/read"
}

cluster_probe() {
  curl -fsS --connect-timeout 3 --max-time 15 --retry 3 --retry-delay 1 --retry-all-errors "$URL/api/probe/cluster_read"
}

write_probe_with_retry() {
  local attempts="${1:-6}"
  local write_json=""
  for _ in $(seq 1 "$attempts"); do
    if write_json="$(write_probe 2>/dev/null)"; then
      local ok
      ok="$(jq -r '.ok // false' <<<"$write_json" 2>/dev/null || echo false)"
      local quorum_ok
      quorum_ok="$(jq -r '((.replicationAcks // 0) >= (.requiredAcks // 1))' <<<"$write_json" 2>/dev/null || echo false)"
      local value
      value="$(jq -r '.value // empty' <<<"$write_json" 2>/dev/null || true)"
      if [[ "$ok" == "true" && "$quorum_ok" == "true" && -n "$value" ]]; then
        printf '%s\n' "$write_json"
        return 0
      fi
    fi
    sleep 1
  done
  return 1
}

verify_cluster_discovery_complete() {
  local cluster_json
  if ! cluster_json="$(cluster_probe)"; then
    echo "[drill] cluster probe request failed while checking discovery" >&2
    return 1
  fi
  local discovered_count
  discovered_count="$(jq -r '.discoveredCount // 0' <<<"$cluster_json")"
  local discovery_complete
  discovery_complete="$(jq -r '.discoveryComplete // false' <<<"$cluster_json")"
  local readings_count
  readings_count="$(jq -r '.readings | length' <<<"$cluster_json")"
  if [[ "$discovered_count" -lt 3 || "$discovery_complete" != "true" || "$readings_count" -lt 3 ]]; then
    echo "[drill] cluster discovery incomplete: $cluster_json" >&2
    return 1
  fi
  return 0
}

wait_for_cluster_ready() {
  local attempts="${1:-45}"
  local sleep_secs="${2:-2}"

  for _ in $(seq 1 "$attempts"); do
    local count
    count="$(flyctl machines list -a "$APP_NAME" --json | jq 'length')"
    local health
    health="$(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/health" | jq -r '.ok' || true)"
    if [[ "$count" -ge 3 && "$health" == "true" ]] && verify_cluster_discovery_complete; then
      return 0
    fi
    sleep "$sleep_secs"
  done

  return 1
}

verify_replicated_value() {
  local expected_value="$1"
  local cluster_json
  if ! cluster_json="$(cluster_probe)"; then
    echo "[drill] cluster probe request failed while checking replicated value" >&2
    return 1
  fi
  local discovered_count
  discovered_count="$(jq -r '.discoveredCount // 0' <<<"$cluster_json")"
  if [[ "$discovered_count" -lt 3 ]]; then
    echo "[drill] cluster probe discovered fewer than 3 machines: $cluster_json" >&2
    return 1
  fi
  local unequal_count
  unequal_count="$(jq -r --arg expected "$expected_value" '[.readings[] | select(.ok != true or (.value // "") != $expected)] | length' <<<"$cluster_json")"
  if [[ "$unequal_count" -gt 0 ]]; then
    echo "[drill] replicated value mismatch for expected=$expected_value: $cluster_json" >&2
    return 1
  fi
  return 0
}

verify_replicated_value_with_retry() {
  local expected_value="$1"
  local attempts="${2:-6}"
  for _ in $(seq 1 "$attempts"); do
    if verify_replicated_value "$expected_value"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if ! wait_for_cluster_ready 60 2; then
  echo "[drill] cluster did not become healthy with full 3-machine discovery before initial probes" >&2
  exit 1
fi

if ! sync_cluster_nodes_secret; then
  echo "[drill] failed to sync cluster-node inventory secret before probes" >&2
  exit 1
fi

if ! wait_for_cluster_ready 60 2; then
  echo "[drill] cluster did not become healthy after inventory sync" >&2
  exit 1
fi

echo "[drill] initial probes"
for _ in {1..10}; do
  if ! WRITE_JSON="$(write_probe_with_retry)"; then
    echo "[drill] failed to obtain successful probe write after retries" >&2
    exit 1
  fi
  WRITE_VALUE="$(jq -r '.value' <<<"$WRITE_JSON")"
  if ! verify_replicated_value_with_retry "$WRITE_VALUE" 8; then
    echo "[drill] replicated value failed to converge for $WRITE_VALUE" >&2
    exit 1
  fi
  sleep 1
done

FAILED_MACHINE="$(list_machine_ids | head -n1)"
echo "[drill] destroying machine $FAILED_MACHINE"
flyctl machine destroy "$FAILED_MACHINE" -a "$APP_NAME" --force
sleep 10

CURRENT_COUNT="$(flyctl machines list -a "$APP_NAME" --json | jq 'length')"
if [[ "$CURRENT_COUNT" -lt 3 ]]; then
  echo "[drill] count dropped to $CURRENT_COUNT; cloning to restore quorum"
  SOURCE_MACHINE="$(list_machine_ids | head -n1)"
  flyctl machine clone "$SOURCE_MACHINE" -a "$APP_NAME" --region ord >/dev/null
fi

if ! sync_cluster_nodes_secret; then
  echo "[drill] failed to sync cluster-node inventory secret after recovery actions" >&2
  exit 1
fi

echo "[drill] waiting for post-failure recovery"
if ! wait_for_cluster_ready 60 2; then
  echo "[drill] cluster failed to recover to healthy replicated 3-machine state" >&2
  exit 1
fi

if ! WRITE_JSON="$(write_probe_with_retry)"; then
  echo "[drill] failed to obtain successful probe write after recovery" >&2
  exit 1
fi
WRITE_VALUE="$(jq -r '.value' <<<"$WRITE_JSON")"
if ! verify_replicated_value_with_retry "$WRITE_VALUE" 10; then
  echo "[drill] replicated value failed to converge after recovery for $WRITE_VALUE" >&2
  exit 1
fi

$ROOT/scripts/fly/wreladb_deploy_safe.sh "$APP_NAME"

if ! POST_DEPLOY_WRITE="$(write_probe_with_retry)"; then
  echo "[drill] failed to obtain successful post-deploy probe write" >&2
  exit 1
fi
POST_DEPLOY_VALUE="$(jq -r '.value' <<<"$POST_DEPLOY_WRITE")"
if ! verify_replicated_value_with_retry "$POST_DEPLOY_VALUE" 10; then
  echo "[drill] replicated value failed to converge after deploy for $POST_DEPLOY_VALUE" >&2
  exit 1
fi

cat > "$REPORT" <<MD
# WrelaDB Fly Sandbox Drill Report

- App: $APP_NAME
- Region: ord
- URL: $URL
- Destroyed machine: $FAILED_MACHINE
- Final machine count: $(flyctl machines list -a "$APP_NAME" --json | jq 'length')
- Health: $(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/health" | jq -r '.ok')
- Cluster: $(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/cluster")
- Schema epoch: $(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/schema/epoch" | jq -r '.epoch')
- Teardown: $([[ "$AUTO_DESTROY" == "1" ]] && echo "auto-destroy enabled" || echo "kept for inspection")

## Probe Samples

- Write: $POST_DEPLOY_WRITE
- Read: $(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 "$URL/api/probe/read")
- Checkpoint: $(curl -fsS --connect-timeout 3 --max-time 15 --retry 2 --retry-delay 1 -X POST "$URL/api/checkpoint")

## Machine Snapshots

- Cluster probe: $(cluster_probe)

## Notes

- Drill ran destructive single-machine failure and recovered to 3 machines.
- Cluster probe checks validated replicated value convergence across all discovered machines.
- Safe deploy used rolling strategy and health gates pre/post deploy.
MD

echo "[drill] report written: $REPORT"
echo "APP_NAME=$APP_NAME"
