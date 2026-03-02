#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARTIFACT_ROOT="$ROOT/.artifacts/perf/local-db-write"
ASSERT_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_schema.sh"
STRICT_ASSERT_SCRIPT="$ROOT/scripts/perf/assert_strict_local_db_write_evidence.sh"
INDEX_SCRIPT="$ROOT/scripts/perf/index_local_db_write_artifact.sh"
CLAIM_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_claimable.sh"
OUT_DIR="$ARTIFACT_ROOT/meso"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/local_db_write_meso_compare.sh [--duration <sec>] [--concurrency <n>] [--payload <bytes>] [--with-control] [--require-lane-spread]

Runs:
  1) strict real-quorum run
  2) strict real-quorum run
  3) optional third strict run only if --with-control is set

Then emits a comparison artifact in:
  .artifacts/perf/local-db-write/meso/<run-id>.json
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool: $tool" >&2
    exit 1
  fi
}

is_uint() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

latest_run_id() {
  ls -1 "$ARTIFACT_ROOT" 2>/dev/null | grep -E '^[0-9]+$' | sort -n | tail -n 1 || true
}

run_harness() {
  local label="$1"
  local duration="$2"
  local concurrency="$3"
  local payload="$4"

  local before
  before="$(latest_run_id)"

  (
    cd "$ROOT"
    WRELA_LOCAL_PERF_DURATION_SECONDS="$duration" \
    WRELA_LOCAL_PERF_CONCURRENCY="$concurrency" \
    WRELA_LOCAL_PERF_PAYLOAD_BYTES="$payload" \
    cargo test -p wrela_runtime --test db_write_local_perf -- --ignored --nocapture >&2
  )

  local after
  after="$(latest_run_id)"
  if [[ -z "$after" ]]; then
    echo "error: no run artifacts detected after $label run" >&2
    exit 1
  fi
  if [[ -n "$before" ]] && (( after <= before )); then
    echo "error: expected new run id after $label (before=$before after=$after)" >&2
    exit 1
  fi
  echo "$after"
}

DURATION="${WRELA_LOCAL_PERF_DURATION_SECONDS:-12}"
CONCURRENCY="${WRELA_LOCAL_PERF_CONCURRENCY:-24}"
PAYLOAD="${WRELA_LOCAL_PERF_PAYLOAD_BYTES:-128}"
WITH_CONTROL="${WRELA_LOCAL_PERF_MESO_WITH_CONTROL:-0}"
REQUIRE_LANE_SPREAD="${WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --duration)
      DURATION="${2:-}"
      shift 2
      ;;
    --concurrency)
      CONCURRENCY="${2:-}"
      shift 2
      ;;
    --payload)
      PAYLOAD="${2:-}"
      shift 2
      ;;
    --with-control)
      WITH_CONTROL="1"
      shift
      ;;
    --require-lane-spread)
      REQUIRE_LANE_SPREAD="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

require_tool cargo
require_tool jq
[[ -x "$ASSERT_SCRIPT" ]] || {
  echo "error: schema assertion script missing or not executable: $ASSERT_SCRIPT" >&2
  exit 1
}
[[ -x "$STRICT_ASSERT_SCRIPT" ]] || {
  echo "error: strict evidence assertion script missing or not executable: $STRICT_ASSERT_SCRIPT" >&2
  exit 1
}
[[ -x "$INDEX_SCRIPT" ]] || {
  echo "error: artifact index script missing or not executable: $INDEX_SCRIPT" >&2
  exit 1
}
[[ -x "$CLAIM_SCRIPT" ]] || {
  echo "error: claimability script missing or not executable: $CLAIM_SCRIPT" >&2
  exit 1
}

is_uint "$DURATION" || {
  echo "error: duration must be integer seconds" >&2
  exit 1
}
is_uint "$CONCURRENCY" || {
  echo "error: concurrency must be integer" >&2
  exit 1
}
is_uint "$PAYLOAD" || {
  echo "error: payload must be integer bytes" >&2
  exit 1
}
[[ "$WITH_CONTROL" == "0" || "$WITH_CONTROL" == "1" ]] || {
  echo "error: --with-control must resolve to 0 or 1" >&2
  exit 1
}
if [[ -z "$REQUIRE_LANE_SPREAD" ]]; then
  # Writer lane count is now a typed DbConfig field (default 1 in test mode).
  REQUIRE_LANE_SPREAD="0"
fi
[[ "$REQUIRE_LANE_SPREAD" == "0" || "$REQUIRE_LANE_SPREAD" == "1" ]] || {
  echo "error: --require-lane-spread must resolve to 0 or 1" >&2
  exit 1
}

mkdir -p "$OUT_DIR"

echo "==> strict run #1"
STRICT_1="$(run_harness strict_1 "$DURATION" "$CONCURRENCY" "$PAYLOAD")"
echo "==> strict run #2"
STRICT_2="$(run_harness strict_2 "$DURATION" "$CONCURRENCY" "$PAYLOAD")"
STRICT_1_SUMMARY="$ARTIFACT_ROOT/$STRICT_1/summary.json"
STRICT_2_SUMMARY="$ARTIFACT_ROOT/$STRICT_2/summary.json"
CONTROL=""
CONTROL_SUMMARY=""
if [[ "$WITH_CONTROL" == "1" ]]; then
  echo "==> control run (strict real-quorum baseline)"
  CONTROL="$(run_harness control "$DURATION" "$CONCURRENCY" "$PAYLOAD")"
  CONTROL_SUMMARY="$ARTIFACT_ROOT/$CONTROL/summary.json"
fi

"$ASSERT_SCRIPT" "$STRICT_1_SUMMARY"
"$ASSERT_SCRIPT" "$STRICT_2_SUMMARY"
"$STRICT_ASSERT_SCRIPT" "$STRICT_1_SUMMARY"
"$STRICT_ASSERT_SCRIPT" "$STRICT_2_SUMMARY"
"$INDEX_SCRIPT" "$STRICT_1_SUMMARY" --kind "meso_strict_1" --strict-required 1
"$INDEX_SCRIPT" "$STRICT_2_SUMMARY" --kind "meso_strict_2" --strict-required 1
"$CLAIM_SCRIPT" "$STRICT_1_SUMMARY" --strict-required 1 --require-indexed 1 --require-lane-spread "$REQUIRE_LANE_SPREAD"
"$CLAIM_SCRIPT" "$STRICT_2_SUMMARY" --strict-required 1 --require-indexed 1 --require-lane-spread "$REQUIRE_LANE_SPREAD"
if [[ -n "$CONTROL_SUMMARY" ]]; then
  "$ASSERT_SCRIPT" "$CONTROL_SUMMARY"
  "$INDEX_SCRIPT" "$CONTROL_SUMMARY" --kind "meso_control" --strict-required 0
fi

OUT_ID="$(( $(date +%s) * 1000 ))"
OUT_PATH="$OUT_DIR/$OUT_ID.json"

jq_base='
  def as_map($s): ($s[0].workloads | map({key: .name, value: .}) | from_entries);
  def strict_rows($m1; $m2):
    [ "raw_write_leader_local", "raw_write_round_robin_nodes", "validated_write_path" ]
      | map(
          . as $k |
          {
            workload: $k,
            strict_avg_tps: (($m1[$k].tps + $m2[$k].tps) / 2),
            strict_tps_run_1: $m1[$k].tps,
            strict_tps_run_2: $m2[$k].tps,
            strict_tps_run_delta_pct: (
              if (($m1[$k].tps + $m2[$k].tps) / 2) == 0 then 0
              else ((($m1[$k].tps - $m2[$k].tps) | abs) * 100 / (($m1[$k].tps + $m2[$k].tps) / 2))
              end
            ),
            strict_avg_p99_ms: (($m1[$k].p99_ms + $m2[$k].p99_ms) / 2),
            strict_avg_stage_replicate_pct: (($m1[$k].stage_replicate_pct + $m2[$k].stage_replicate_pct) / 2),
            strict_avg_stage_wal_submit_wait_pct: (($m1[$k].stage_wal_submit_wait_pct + $m2[$k].stage_wal_submit_wait_pct) / 2),
            strict_avg_stage_wal_fdatasync_pct: (($m1[$k].stage_wal_fdatasync_pct + $m2[$k].stage_wal_fdatasync_pct) / 2),
            strict_real_quorum: ($m1[$k].replication.real_quorum_evidence and $m2[$k].replication.real_quorum_evidence),
            strict_sim_commits_sum: ($m1[$k].replication.simulation_commits + $m2[$k].replication.simulation_commits),
            strict_avg_replica_max_replication_ms: (($m1[$k].replication.replica_max_replication_ms + $m2[$k].replication.replica_max_replication_ms) / 2)
          }
      );
  (as_map($s1)) as $m1 |
  (as_map($s2)) as $m2 |
  {
    schema_version: 1,
    profile: "local-db-write-meso-compare",
    generated_at_epoch_ms: (now * 1000 | floor),
    config: {
      duration_seconds: $duration,
      concurrency: $concurrency,
      payload_bytes: $payload
    },
    runs: {
      strict_1: $strict_1,
      strict_2: $strict_2,
      control: $control
    },
    workloads: strict_rows($m1; $m2)
  }
'

if [[ -n "$CONTROL_SUMMARY" ]]; then
  jq -n \
    --arg strict_1 "$STRICT_1" \
    --arg strict_2 "$STRICT_2" \
    --arg control "$CONTROL" \
    --argjson duration "$DURATION" \
    --argjson concurrency "$CONCURRENCY" \
    --argjson payload "$PAYLOAD" \
    --slurpfile s1 "$STRICT_1_SUMMARY" \
    --slurpfile s2 "$STRICT_2_SUMMARY" \
    --slurpfile c "$CONTROL_SUMMARY" \
    '
    def as_map($s): ($s[0].workloads | map({key: .name, value: .}) | from_entries);
    def pct_delta($new; $old): if $old == 0 then 0 else (($new - $old) * 100.0 / $old) end;
    (as_map($s1)) as $m1 |
    (as_map($s2)) as $m2 |
    (as_map($c)) as $mc |
    ('"$jq_base"') as $base |
    $base
    | .workloads |= map(
      . as $row |
      . + {
        control_tps: $mc[$row.workload].tps,
        tps_delta_pct_strict_vs_control: pct_delta($row.strict_avg_tps; $mc[$row.workload].tps),
        control_p99_ms: $mc[$row.workload].p99_ms,
        control_stage_replicate_pct: $mc[$row.workload].stage_replicate_pct,
        control_stage_wal_submit_wait_pct: $mc[$row.workload].stage_wal_submit_wait_pct,
        control_stage_wal_fdatasync_pct: $mc[$row.workload].stage_wal_fdatasync_pct,
        control_sim_commits: $mc[$row.workload].replication.simulation_commits,
        control_replica_max_replication_ms: $mc[$row.workload].replication.replica_max_replication_ms
      }
    )
    ' >"$OUT_PATH"
else
  jq -n \
    --arg strict_1 "$STRICT_1" \
    --arg strict_2 "$STRICT_2" \
    --arg control "" \
    --argjson duration "$DURATION" \
    --argjson concurrency "$CONCURRENCY" \
    --argjson payload "$PAYLOAD" \
    --slurpfile s1 "$STRICT_1_SUMMARY" \
    --slurpfile s2 "$STRICT_2_SUMMARY" \
    "$jq_base" >"$OUT_PATH"
fi

echo "meso compare artifact: $OUT_PATH"
