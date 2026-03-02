#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/assert_local_db_write_schema.sh <run-dir|summary.json>

Validates local DB write perf artifact schema for both summary and per-workload
reports. Fails non-zero on schema drift.
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool: $tool" >&2
    exit 1
  fi
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_tool jq

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  usage
  exit 2
fi

RUN_DIR=""
SUMMARY_PATH=""
if [[ -d "$TARGET" ]]; then
  RUN_DIR="$(cd "$TARGET" && pwd)"
  SUMMARY_PATH="$RUN_DIR/summary.json"
elif [[ -f "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
  RUN_DIR="$(dirname "$SUMMARY_PATH")"
else
  fail "path not found: $TARGET"
fi

[[ -f "$SUMMARY_PATH" ]] || fail "missing summary JSON: $SUMMARY_PATH"

JQ_WORKLOAD_EXPR='
  def num: type == "number" and isfinite;
  def nonneg_num: num and . >= 0;
  def pct: nonneg_num and . <= 100;
  def nonempty_str: type == "string" and length > 0;
  def has_p99_p999($p):
    (getpath($p + ["p99_ms"]) | nonneg_num) and
    (getpath($p + ["p999_ms"]) | nonneg_num);
  def workload_ok:
    type == "object"
    and (.schema_version | num)
    and (.name | nonempty_str)
    and (.attempts | nonneg_num)
    and (.success | nonneg_num)
    and (.failures | nonneg_num)
    and (.tps | nonneg_num)
    and (.p50_ms | nonneg_num)
    and (.p95_ms | nonneg_num)
    and (.p99_ms | nonneg_num)
    and (.p999_ms | nonneg_num)
    and (.retry_after_by_cause | type == "object")
    and (.replication.queue_depth | nonneg_num)
    and (.replication.target_count | nonneg_num)
    and (.replication.contacted_count | nonneg_num)
    and (.replication.wave_count | nonneg_num)
    and (.replication.wave_avg_targets | nonneg_num)
    and (.replication.wave_max_targets | nonneg_num)
    and (.replication.successful_count | nonneg_num)
    and (.replication.failed_count | nonneg_num)
    and (.replication.cancelled_count | nonneg_num)
    and (.replication.contact_efficiency_bps | nonneg_num and . <= 10000)
    and (.replication.target_efficiency_bps | nonneg_num and . <= 10000)
    and (.replication.skipped_count | nonneg_num)
    and (.replication.aborted_in_flight_count | nonneg_num)
    and (.replication.batch_samples | nonneg_num)
    and (.replication.batch_ops_le_1 | nonneg_num)
    and (.replication.batch_ops_le_4 | nonneg_num)
    and (.replication.batch_ops_le_16 | nonneg_num)
    and (.replication.batch_ops_le_64 | nonneg_num)
    and (.replication.batch_ops_gt_64 | nonneg_num)
    and (.replication.batch_bytes_le_1k | nonneg_num)
    and (.replication.batch_bytes_le_4k | nonneg_num)
    and (.replication.batch_bytes_le_16k | nonneg_num)
    and (.replication.batch_bytes_le_64k | nonneg_num)
    and (.replication.batch_bytes_gt_64k | nonneg_num)
    and (.replication.contacted_ratio_pct | pct)
    and (.replication.skipped_ratio_pct | pct)
    and (.replication.simulation_commits | nonneg_num)
    and (.replication.rpc_max_in_flight | nonneg_num and . > 0)
    and (.replication.rpc_in_flight | nonneg_num)
    and (.replication.rpc_available_permits | nonneg_num)
    and (.replication.rpc_backpressure_timeouts | nonneg_num)
    and (.replication.rpc_backpressure_closed | nonneg_num)
    and (.replication.real_quorum_evidence | type == "boolean")
    and (.replication.quorum_transport_mode | nonempty_str)
    and (.replication.replicated_log_backend | nonempty_str)
    and (.replication.replicated_log_shadow_payload_bytes | nonneg_num)
    and (.replication.replicated_log_shadow_wal_bytes | nonneg_num)
    and (.replication.replicated_log_shadow_overhead_bytes | nonneg_num)
    and (.replication.failure_counters | type == "object")
    and (.replication.replica_ack_count | nonneg_num)
    and (.replication.telemetry_sample_period_ms | nonneg_num and . > 0)
    and (.writer_lanes.lane_count | nonneg_num and . > 0)
    and (.writer_lanes.active_lane_count | nonneg_num)
    and (.writer_lanes.total_assigned_shards | nonneg_num)
    and (.writer_lanes.max_assigned_shards | nonneg_num)
    and (.writer_lanes.max_assigned_shard_share_pct | pct)
    and (.writer_lanes.max_queue_depth | nonneg_num)
    and (.writer_lanes.max_lane_retry_after_bps | nonneg_num)
    and (.writer_lanes.max_lane_saturation_bps | nonneg_num)
    and (.writer_lanes.max_enqueue_attempt_share_bps | nonneg_num)
    and (.writer_lanes.assignment_lookups | nonneg_num)
    and (.writer_lanes.assignment_hits | nonneg_num)
    and (.writer_lanes.assignment_misses | nonneg_num)
    and (.writer_lanes.assignment_hit_rate_bps | nonneg_num and . <= 10000)
    and (.apply_lanes.lane_count | nonneg_num and . > 0)
    and (.apply_lanes.active_lane_count | nonneg_num)
    and (.apply_lanes.max_queue_depth | nonneg_num)
    and (.lsm.compaction_debt_bytes_estimate | nonneg_num)
    and (.lsm.shadow_bytes_estimate | nonneg_num)
    and (.lsm.live_bytes_estimate | nonneg_num)
    and (.lsm.total_bytes_estimate | nonneg_num)
    and (.lsm.version_count | nonneg_num)
    and (.lsm.tombstone_count | nonneg_num)
    and (.replication.depth_timeline | type == "array")
    and (.replication.quorum_failure_token | type == "string" or . == null)
    and (all(.replication.depth_timeline[]?;
      (.elapsed_ms | nonneg_num)
      and (.queue_depth | nonneg_num)
      and (.apply_backlog_depth | nonneg_num)
    ))
    and (.client_write_path.response_wait_pct | pct)
    and has_p99_p999(["stage_percentiles", "total"])
    and has_p99_p999(["client_write_path", "total_percentiles"]);
  workload_ok
'

JQ_SUMMARY_EXPR='
  def num: type == "number" and isfinite;
  def nonneg_num: num and . >= 0;
  def pct: nonneg_num and . <= 100;
  def nonempty_str: type == "string" and length > 0;
  def has_p99_p999($p):
    (getpath($p + ["p99_ms"]) | nonneg_num) and
    (getpath($p + ["p999_ms"]) | nonneg_num);
  def workload_ok:
    type == "object"
    and (.schema_version | num)
    and (.name | nonempty_str)
    and (.attempts | nonneg_num)
    and (.success | nonneg_num)
    and (.failures | nonneg_num)
    and (.tps | nonneg_num)
    and (.p50_ms | nonneg_num)
    and (.p95_ms | nonneg_num)
    and (.p99_ms | nonneg_num)
    and (.p999_ms | nonneg_num)
    and (.retry_after_by_cause | type == "object")
    and (.replication.queue_depth | nonneg_num)
    and (.replication.target_count | nonneg_num)
    and (.replication.contacted_count | nonneg_num)
    and (.replication.wave_count | nonneg_num)
    and (.replication.wave_avg_targets | nonneg_num)
    and (.replication.wave_max_targets | nonneg_num)
    and (.replication.successful_count | nonneg_num)
    and (.replication.failed_count | nonneg_num)
    and (.replication.cancelled_count | nonneg_num)
    and (.replication.contact_efficiency_bps | nonneg_num and . <= 10000)
    and (.replication.target_efficiency_bps | nonneg_num and . <= 10000)
    and (.replication.skipped_count | nonneg_num)
    and (.replication.aborted_in_flight_count | nonneg_num)
    and (.replication.batch_samples | nonneg_num)
    and (.replication.batch_ops_le_1 | nonneg_num)
    and (.replication.batch_ops_le_4 | nonneg_num)
    and (.replication.batch_ops_le_16 | nonneg_num)
    and (.replication.batch_ops_le_64 | nonneg_num)
    and (.replication.batch_ops_gt_64 | nonneg_num)
    and (.replication.batch_bytes_le_1k | nonneg_num)
    and (.replication.batch_bytes_le_4k | nonneg_num)
    and (.replication.batch_bytes_le_16k | nonneg_num)
    and (.replication.batch_bytes_le_64k | nonneg_num)
    and (.replication.batch_bytes_gt_64k | nonneg_num)
    and (.replication.contacted_ratio_pct | pct)
    and (.replication.skipped_ratio_pct | pct)
    and (.replication.simulation_commits | nonneg_num)
    and (.replication.rpc_max_in_flight | nonneg_num and . > 0)
    and (.replication.rpc_in_flight | nonneg_num)
    and (.replication.rpc_available_permits | nonneg_num)
    and (.replication.rpc_backpressure_timeouts | nonneg_num)
    and (.replication.rpc_backpressure_closed | nonneg_num)
    and (.replication.real_quorum_evidence | type == "boolean")
    and (.replication.quorum_transport_mode | nonempty_str)
    and (.replication.replicated_log_backend | nonempty_str)
    and (.replication.replicated_log_shadow_payload_bytes | nonneg_num)
    and (.replication.replicated_log_shadow_wal_bytes | nonneg_num)
    and (.replication.replicated_log_shadow_overhead_bytes | nonneg_num)
    and (.replication.failure_counters | type == "object")
    and (.writer_lanes.lane_count | nonneg_num and . > 0)
    and (.writer_lanes.active_lane_count | nonneg_num)
    and (.writer_lanes.total_assigned_shards | nonneg_num)
    and (.writer_lanes.max_assigned_shards | nonneg_num)
    and (.writer_lanes.max_assigned_shard_share_pct | pct)
    and (.writer_lanes.max_lane_retry_after_bps | nonneg_num)
    and (.writer_lanes.max_lane_saturation_bps | nonneg_num)
    and (.writer_lanes.max_enqueue_attempt_share_bps | nonneg_num)
    and (.writer_lanes.assignment_lookups | nonneg_num)
    and (.writer_lanes.assignment_hits | nonneg_num)
    and (.writer_lanes.assignment_misses | nonneg_num)
    and (.writer_lanes.assignment_hit_rate_bps | nonneg_num and . <= 10000)
    and (.apply_lanes.lane_count | nonneg_num and . > 0)
    and (.apply_lanes.active_lane_count | nonneg_num)
    and (.apply_lanes.max_queue_depth | nonneg_num)
    and (.lsm.compaction_debt_bytes_estimate | nonneg_num)
    and (.lsm.shadow_bytes_estimate | nonneg_num)
    and (.lsm.live_bytes_estimate | nonneg_num)
    and (.lsm.total_bytes_estimate | nonneg_num)
    and (.lsm.version_count | nonneg_num)
    and (.lsm.tombstone_count | nonneg_num)
    and (.client_write_path.response_wait_pct | pct)
    and has_p99_p999(["stage_percentiles", "total"])
    and has_p99_p999(["client_write_path", "total_percentiles"]);
  type == "object"
  and (.schema_version | num)
  and (.run_id | nonempty_str)
  and (.generated_at_epoch_ms | nonneg_num)
  and (.artifacts_dir | nonempty_str)
  and (.run_metadata.os | nonempty_str)
  and (.run_metadata.arch | nonempty_str)
  and (.config.concurrency | nonneg_num and . > 0)
  and (.config.duration_seconds | nonneg_num and . > 0)
  and (.config.payload_bytes | nonneg_num and . > 0)
  and (.config.writer_lane_count | nonneg_num and . > 0)
  and (.config.apply_lane_count | nonneg_num and . > 0)
  and (.config.private_rpc_channels_per_target | nonneg_num and . > 0)
  and (.config.logical_shards | nonneg_num and . > 0)
  and (.config.active_groups | nonneg_num and . > 0)
  and (.config.replication_max_in_flight | nonneg_num and . > 0)
  and (.config.replication_max_targets | nonneg_num and . > 0)
  and (.config.replication_hedge_extra | nonneg_num)
  and (.config.replication_batch_max_ops | nonneg_num and . > 0)
  and (.config.replication_batch_max_bytes | nonneg_num and . > 0)
  and (.config.replication_factor | nonneg_num and . > 0)
  and (.config.write_quorum | nonneg_num and . > 0)
  and (.config.quorum_transport_mode | nonempty_str)
  and (.config.replicated_log_backend | nonempty_str)
  and (.config.real_quorum_mode | type == "boolean")
  and (.config.require_lane_spread | type == "boolean")
  and (.config.replication_batch_window_us | nonneg_num and . > 0)
  and (.config.commit_visibility_mode | nonempty_str)
  and (.workloads | type == "array" and length > 0)
  and (all(.workloads[]; workload_ok))
'

if ! jq -e "$JQ_SUMMARY_EXPR" "$SUMMARY_PATH" >/dev/null; then
  fail "summary schema assertion failed: $SUMMARY_PATH"
fi

WORKLOAD_NAMES="$(jq -r '.workloads[].name' "$SUMMARY_PATH")"
[[ -n "$WORKLOAD_NAMES" ]] || fail "summary has no workloads: $SUMMARY_PATH"

while IFS= read -r workload_name; do
  [[ -n "$workload_name" ]] || continue
  workload_path="$RUN_DIR/${workload_name}.json"
  [[ -f "$workload_path" ]] || fail "missing workload report: $workload_path"

  if ! jq -e --arg expected_name "$workload_name" "($JQ_WORKLOAD_EXPR) and (.name == \$expected_name)" "$workload_path" >/dev/null; then
    fail "workload schema assertion failed: $workload_path"
  fi

  if ! jq -e --arg expected_name "$workload_name" ".workloads[] | select(.name == \$expected_name) | ($JQ_WORKLOAD_EXPR)" "$SUMMARY_PATH" >/dev/null; then
    fail "summary inline workload payload failed schema: $workload_name"
  fi
done <<<"$WORKLOAD_NAMES"

# If shadow backend is enabled, shadow telemetry must prove data actually flowed.
if ! jq -e '
  .workloads
  | all(.[];
      if .replication.replicated_log_backend == "shadow_canonical" then
        (.replication.replicated_log_shadow_payload_bytes > 0)
        and (.replication.replicated_log_shadow_wal_bytes >= .replication.replicated_log_shadow_payload_bytes)
        and (.replication.replicated_log_shadow_overhead_bytes
             == (.replication.replicated_log_shadow_wal_bytes - .replication.replicated_log_shadow_payload_bytes))
      else
        true
      end
    )
' "$SUMMARY_PATH" >/dev/null; then
  fail "shadow backend anti-cheat failed: expected non-zero, internally consistent shadow log telemetry"
fi

if ! jq -e '
  .workloads
  | all(.[];
    (.replication.real_quorum_evidence == true)
    and (.replication.simulation_commits == 0)
    and (.replication.contacted_count > 0)
    and ((.replication.quorum_failure_token == null) or (.replication.quorum_failure_token == ""))
  )
' "$SUMMARY_PATH" >/dev/null; then
  fail "strict real-quorum anti-cheat failed: expected non-simulated quorum evidence for all workloads"
fi

echo "schema ok: $SUMMARY_PATH"
