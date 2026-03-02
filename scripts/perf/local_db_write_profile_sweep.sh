#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_ROOT="$ROOT/.artifacts/perf/local-db-write"
SWEEP_DIR="$ARTIFACT_ROOT/sweeps"
ASSERT_SCHEMA="$ROOT/scripts/perf/assert_local_db_write_schema.sh"
ASSERT_STRICT="$ROOT/scripts/perf/assert_strict_local_db_write_evidence.sh"
INDEX_SCRIPT="$ROOT/scripts/perf/index_local_db_write_artifact.sh"
CLAIM_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_claimable.sh"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/local_db_write_profile_sweep.sh [--duration <sec>] [--payload <bytes>] [--profiles <spec>] [--require-lane-spread <0|1>]

Profile spec format (comma-separated):
  name:concurrency:lanes:shards:batch_window_us:max_ops:max_bytes:hedge:permit_timeout_ms

Example:
  scripts/perf/local_db_write_profile_sweep.sh \
    --profiles "base:8:2:8:500:64:262144:1:25,wide:16:4:16:800:128:524288:1:25"
USAGE
}

require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || {
    echo "error: missing required tool: $tool" >&2
    exit 1
  }
}

is_uint() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

latest_run_id() {
  ls -1 "$ARTIFACT_ROOT" 2>/dev/null | grep -E '^[0-9]+$' | sort -n | tail -n 1 || true
}

run_profile() {
  local name="$1"
  local concurrency="$2"
  local lanes="$3"
  local shards="$4"
  local batch_window_us="$5"
  local max_ops="$6"
  local max_bytes="$7"
  local hedge="$8"
  local permit_timeout_ms="$9"
  local duration="${10}"
  local payload="${11}"
  local require_lane_spread="${12}"

  local before
  before="$(latest_run_id)"

  (
    cd "$ROOT"
    WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD="$require_lane_spread" \
    WRELA_LOCAL_PERF_DURATION_SECONDS="$duration" \
    WRELA_LOCAL_PERF_CONCURRENCY="$concurrency" \
    WRELA_LOCAL_PERF_PAYLOAD_BYTES="$payload" \
    cargo test -p wrela_runtime --test db_write_local_perf -- --ignored --nocapture >&2
  )

  local after
  after="$(latest_run_id)"
  if [[ -z "$after" ]]; then
    echo "error: no run dir found for profile $name" >&2
    exit 1
  fi
  if [[ -n "$before" ]] && (( after <= before )); then
    echo "error: expected new run id for profile $name (before=$before after=$after)" >&2
    exit 1
  fi

  local summary="$ARTIFACT_ROOT/$after/summary.json"
  "$ASSERT_SCHEMA" "$summary" >&2
  "$ASSERT_STRICT" "$summary" >&2
  "$INDEX_SCRIPT" "$summary" --kind "sweep_$name" --strict-required 1 >&2
  "$CLAIM_SCRIPT" "$summary" --strict-required 1 --require-indexed 1 --require-lane-spread "$require_lane_spread" >&2

  jq -n \
    --arg name "$name" \
    --arg run_id "$after" \
    --arg summary "$summary" \
    --argjson concurrency "$concurrency" \
    --argjson lanes "$lanes" \
    --argjson shards "$shards" \
    --argjson batch_window_us "$batch_window_us" \
    --argjson max_ops "$max_ops" \
    --argjson max_bytes "$max_bytes" \
    --argjson hedge "$hedge" \
    --argjson permit_timeout_ms "$permit_timeout_ms" \
    --slurpfile s "$summary" \
    '{
      profile: $name,
      run_id: $run_id,
      summary_path: $summary,
      config: {
        concurrency: $concurrency,
        writer_lane_count: $lanes,
        logical_shards: $shards,
        replication_batch_window_us: $batch_window_us,
        replication_batch_max_ops: $max_ops,
        replication_batch_max_bytes: $max_bytes,
        replication_hedge_extra: $hedge,
        replication_rpc_permit_timeout_ms: $permit_timeout_ms
      },
      workloads: ($s[0].workloads | map({
        name,
        tps,
        p95_ms,
        p99_ms,
        p999_ms,
        stage_replicate_pct,
        stage_wal_submit_wait_pct,
        stage_wal_fdatasync_pct,
        replication_successful_count: .replication.successful_count,
        replication_cancelled_count: .replication.cancelled_count,
        replication_aborted_in_flight_count: .replication.aborted_in_flight_count,
        active_lane_count: .writer_lanes.active_lane_count,
        max_assigned_shard_share_pct: .writer_lanes.max_assigned_shard_share_pct
      }))
    }'
}

DURATION="3"
PAYLOAD="64"
REQUIRE_LANE_SPREAD="1"
PROFILES="base:8:2:8:500:64:262144:1:25,wide:12:4:16:800:128:524288:1:25,burst:16:4:16:500:128:262144:1:15,steady:8:2:8:1000:128:524288:1:25"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --duration)
      DURATION="${2:-}"
      shift 2
      ;;
    --payload)
      PAYLOAD="${2:-}"
      shift 2
      ;;
    --profiles)
      PROFILES="${2:-}"
      shift 2
      ;;
    --require-lane-spread)
      REQUIRE_LANE_SPREAD="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown arg $1" >&2
      usage
      exit 2
      ;;
  esac
done

require_tool cargo
require_tool jq
[[ -x "$ASSERT_SCHEMA" ]] || { echo "error: missing $ASSERT_SCHEMA" >&2; exit 1; }
[[ -x "$ASSERT_STRICT" ]] || { echo "error: missing $ASSERT_STRICT" >&2; exit 1; }
[[ -x "$INDEX_SCRIPT" ]] || { echo "error: missing $INDEX_SCRIPT" >&2; exit 1; }
[[ -x "$CLAIM_SCRIPT" ]] || { echo "error: missing $CLAIM_SCRIPT" >&2; exit 1; }

is_uint "$DURATION" || { echo "error: duration must be integer" >&2; exit 1; }
is_uint "$PAYLOAD" || { echo "error: payload must be integer" >&2; exit 1; }
[[ "$REQUIRE_LANE_SPREAD" == "0" || "$REQUIRE_LANE_SPREAD" == "1" ]] || {
  echo "error: require-lane-spread must be 0 or 1" >&2
  exit 1
}

mkdir -p "$SWEEP_DIR"
TMP_JSONL="$(mktemp)"
trap 'rm -f "$TMP_JSONL"' EXIT

IFS=',' read -r -a profile_items <<<"$PROFILES"
for item in "${profile_items[@]}"; do
  IFS=':' read -r name concurrency lanes shards batch_window_us max_ops max_bytes hedge permit_timeout_ms <<<"$item"
  [[ -n "$name" ]] || { echo "error: invalid profile entry: $item" >&2; exit 1; }
  for value in "$concurrency" "$lanes" "$shards" "$batch_window_us" "$max_ops" "$max_bytes" "$hedge" "$permit_timeout_ms"; do
    is_uint "$value" || { echo "error: non-integer profile value in $item" >&2; exit 1; }
  done
  echo "==> running profile $name (c=$concurrency lanes=$lanes shards=$shards window=$batch_window_us ops=$max_ops bytes=$max_bytes hedge=$hedge permit_to=$permit_timeout_ms)" >&2
  run_profile "$name" "$concurrency" "$lanes" "$shards" "$batch_window_us" "$max_ops" "$max_bytes" "$hedge" "$permit_timeout_ms" "$DURATION" "$PAYLOAD" "$REQUIRE_LANE_SPREAD" >>"$TMP_JSONL"
done

SWEEP_ID="$(( $(date +%s) * 1000 ))"
OUT_PATH="$SWEEP_DIR/$SWEEP_ID.json"

jq -s '
  def workload_map($entry): ($entry.workloads | map({key: .name, value: .}) | from_entries);
  {
    schema_version: 1,
    generated_at_epoch_ms: (now * 1000 | floor),
    profile_count: length,
    profiles: .,
    ranking_raw_write_leader_local: (
      map(. + { raw_write_leader_local: (workload_map(.)["raw_write_leader_local"] // {}) })
      | sort_by(.raw_write_leader_local.tps // 0)
      | reverse
      | map({
          profile,
          run_id,
          tps: (.raw_write_leader_local.tps // 0),
          p99_ms: (.raw_write_leader_local.p99_ms // 0),
          stage_replicate_pct: (.raw_write_leader_local.stage_replicate_pct // 0),
          stage_wal_submit_wait_pct: (.raw_write_leader_local.stage_wal_submit_wait_pct // 0)
        })
    ),
    ranking_validated_write_path: (
      map(. + { validated_write_path: (workload_map(.)["validated_write_path"] // {}) })
      | sort_by(.validated_write_path.tps // 0)
      | reverse
      | map({
          profile,
          run_id,
          tps: (.validated_write_path.tps // 0),
          p99_ms: (.validated_write_path.p99_ms // 0),
          stage_replicate_pct: (.validated_write_path.stage_replicate_pct // 0),
          stage_wal_submit_wait_pct: (.validated_write_path.stage_wal_submit_wait_pct // 0)
        })
    )
  }
' "$TMP_JSONL" >"$OUT_PATH"

echo "profile sweep artifact: $OUT_PATH"
