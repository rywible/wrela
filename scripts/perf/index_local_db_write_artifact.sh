#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ASSERT_SCHEMA_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_schema.sh"
ASSERT_STRICT_SCRIPT="$ROOT/scripts/perf/assert_strict_local_db_write_evidence.sh"
ARTIFACT_ROOT="$ROOT/.artifacts/perf/local-db-write"
INDEX_PATH="$ARTIFACT_ROOT/INDEX.jsonl"
LOCK_PATH="$ARTIFACT_ROOT/.index.lock"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/index_local_db_write_artifact.sh <run-dir|summary.json> [--kind <label>] [--sha <commit-sha>] [--strict-required <0|1>]

Appends a normalized index record to .artifacts/perf/local-db-write/INDEX.jsonl.
Each indexed record includes strict evidence status and key workload metrics.
USAGE
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "missing required tool: $tool"
  fi
}

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  usage
  exit 2
fi
shift || true

KIND="manual"
SHA=""
STRICT_REQUIRED="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --kind)
      KIND="${2:-}"
      shift 2
      ;;
    --sha)
      SHA="${2:-}"
      shift 2
      ;;
    --strict-required)
      STRICT_REQUIRED="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

require_tool jq
require_tool flock
[[ -x "$ASSERT_SCHEMA_SCRIPT" ]] || fail "missing schema assert script: $ASSERT_SCHEMA_SCRIPT"
[[ -x "$ASSERT_STRICT_SCRIPT" ]] || fail "missing strict assert script: $ASSERT_STRICT_SCRIPT"

[[ "$STRICT_REQUIRED" == "0" || "$STRICT_REQUIRED" == "1" ]] || fail "--strict-required must be 0 or 1"

SUMMARY_PATH=""
if [[ -d "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$TARGET" && pwd)/summary.json"
elif [[ -f "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
else
  fail "path not found: $TARGET"
fi

[[ -f "$SUMMARY_PATH" ]] || fail "missing summary json: $SUMMARY_PATH"
RUN_DIR="$(dirname "$SUMMARY_PATH")"

if [[ -z "$SHA" ]]; then
  SHA="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
fi

mkdir -p "$ARTIFACT_ROOT"

"$ASSERT_SCHEMA_SCRIPT" "$SUMMARY_PATH"
if [[ "$STRICT_REQUIRED" == "1" ]]; then
  "$ASSERT_STRICT_SCRIPT" "$SUMMARY_PATH"
fi

STRICT_EVIDENCE="false"
if "$ASSERT_STRICT_SCRIPT" "$SUMMARY_PATH" >/dev/null 2>&1; then
  STRICT_EVIDENCE="true"
fi

RUN_ID="$(jq -er '.run_id | strings | select(length > 0)' "$SUMMARY_PATH")"
SCHEMA_VERSION="$(jq -er '.schema_version' "$SUMMARY_PATH")"
GENERATED_AT_MS="$(jq -er '.generated_at_epoch_ms' "$SUMMARY_PATH")"
REAL_QUORUM_MODE="$(jq -er '.config.real_quorum_mode' "$SUMMARY_PATH")"

tmp_record="$(mktemp)"

jq -c -n \
  --argjson version 1 \
  --argjson indexed_at_epoch_ms "$(( $(date +%s) * 1000 ))" \
  --arg kind "$KIND" \
  --arg sha "$SHA" \
  --arg run_id "$RUN_ID" \
  --arg summary_path "$SUMMARY_PATH" \
  --arg artifacts_dir "$RUN_DIR" \
  --argjson schema_version "$SCHEMA_VERSION" \
  --argjson generated_at_epoch_ms "$GENERATED_AT_MS" \
  --argjson strict_required "$STRICT_REQUIRED" \
  --argjson strict_evidence "$STRICT_EVIDENCE" \
  --argjson real_quorum_mode "$REAL_QUORUM_MODE" \
  --slurpfile s "$SUMMARY_PATH" \
  '{
    version: $version,
    indexed_at_epoch_ms: $indexed_at_epoch_ms,
    kind: $kind,
    sha: $sha,
    run_id: $run_id,
    summary_path: $summary_path,
    artifacts_dir: $artifacts_dir,
    schema_version: $schema_version,
    generated_at_epoch_ms: $generated_at_epoch_ms,
    strict_required: ($strict_required == 1),
    strict_evidence: $strict_evidence,
    real_quorum_mode: $real_quorum_mode,
    config: {
      concurrency: $s[0].config.concurrency,
      duration_seconds: $s[0].config.duration_seconds,
      payload_bytes: $s[0].config.payload_bytes,
      logical_shards: $s[0].config.logical_shards,
      active_groups: $s[0].config.active_groups,
      writer_lane_count: $s[0].config.writer_lane_count,
      replication_batch_window_us: $s[0].config.replication_batch_window_us,
      replication_batch_max_ops: $s[0].config.replication_batch_max_ops,
      replication_batch_max_bytes: $s[0].config.replication_batch_max_bytes,
      replication_max_in_flight: $s[0].config.replication_max_in_flight,
      replication_max_targets: $s[0].config.replication_max_targets,
      replication_hedge_extra: $s[0].config.replication_hedge_extra,
      commit_visibility_mode: $s[0].config.commit_visibility_mode,
      replicated_log_backend: $s[0].config.replicated_log_backend,
      quorum_transport_mode: $s[0].config.quorum_transport_mode,
      require_lane_spread: $s[0].config.require_lane_spread
    },
    workloads: (
      $s[0].workloads
      | map({
          name,
          tps,
          p95_ms,
          p99_ms,
          p999_ms,
          replication: {
            simulation_commits: .replication.simulation_commits,
            real_quorum_evidence: .replication.real_quorum_evidence,
            quorum_failure_token: .replication.quorum_failure_token,
            queue_depth: .replication.queue_depth,
            queue_depth_peak: .replication.queue_depth_peak,
            successful_count: .replication.successful_count,
            failed_count: .replication.failed_count,
            cancelled_count: .replication.cancelled_count,
            contact_efficiency_bps: .replication.contact_efficiency_bps,
            target_efficiency_bps: .replication.target_efficiency_bps
          },
          writer_lanes: {
            lane_count: .writer_lanes.lane_count,
            active_lane_count: .writer_lanes.active_lane_count,
            total_assigned_shards: .writer_lanes.total_assigned_shards,
            max_assigned_shards: .writer_lanes.max_assigned_shards,
            max_assigned_shard_share_pct: .writer_lanes.max_assigned_shard_share_pct,
            assignment_hit_rate_bps: .writer_lanes.assignment_hit_rate_bps
          }
        })
    )
  }' >"$tmp_record"

exec 9>"$LOCK_PATH"
flock 9
cat "$tmp_record" >>"$INDEX_PATH"
rm -f "$tmp_record"

echo "indexed local db-write artifact: $SUMMARY_PATH"
echo "index path: $INDEX_PATH"
