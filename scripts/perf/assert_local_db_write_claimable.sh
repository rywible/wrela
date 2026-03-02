#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ASSERT_SCHEMA_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_schema.sh"
ASSERT_STRICT_SCRIPT="$ROOT/scripts/perf/assert_strict_local_db_write_evidence.sh"
INDEX_PATH="$ROOT/.artifacts/perf/local-db-write/INDEX.jsonl"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/assert_local_db_write_claimable.sh <run-dir|summary.json> [--strict-required <0|1>] [--require-indexed <0|1>] [--require-lane-spread <0|1>]

Fail-closed gate to decide whether a local-db-write run is claimable as perf evidence.
Checks schema, optional strict evidence, and optional index-ledger presence.
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

STRICT_REQUIRED="1"
REQUIRE_INDEXED="1"
REQUIRE_LANE_SPREAD="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict-required)
      STRICT_REQUIRED="${2:-}"
      shift 2
      ;;
    --require-indexed)
      REQUIRE_INDEXED="${2:-}"
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
      fail "unknown argument: $1"
      ;;
  esac
done

[[ "$STRICT_REQUIRED" == "0" || "$STRICT_REQUIRED" == "1" ]] || fail "--strict-required must be 0 or 1"
[[ "$REQUIRE_INDEXED" == "0" || "$REQUIRE_INDEXED" == "1" ]] || fail "--require-indexed must be 0 or 1"
[[ "$REQUIRE_LANE_SPREAD" == "0" || "$REQUIRE_LANE_SPREAD" == "1" ]] || fail "--require-lane-spread must be 0 or 1"

require_tool jq
[[ -x "$ASSERT_SCHEMA_SCRIPT" ]] || fail "missing schema assert script: $ASSERT_SCHEMA_SCRIPT"
[[ -x "$ASSERT_STRICT_SCRIPT" ]] || fail "missing strict assert script: $ASSERT_STRICT_SCRIPT"

SUMMARY_PATH=""
if [[ -d "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$TARGET" && pwd)/summary.json"
elif [[ -f "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
else
  fail "path not found: $TARGET"
fi

[[ -f "$SUMMARY_PATH" ]] || fail "missing summary json: $SUMMARY_PATH"
"$ASSERT_SCHEMA_SCRIPT" "$SUMMARY_PATH"
if [[ "$STRICT_REQUIRED" == "1" ]]; then
  "$ASSERT_STRICT_SCRIPT" "$SUMMARY_PATH"
fi

RUN_ID="$(jq -er '.run_id | strings | select(length > 0)' "$SUMMARY_PATH")"
if [[ "$REQUIRE_INDEXED" == "1" ]]; then
  [[ -f "$INDEX_PATH" ]] || fail "missing index ledger: $INDEX_PATH"
  if [[ "$STRICT_REQUIRED" == "1" ]]; then
    jq -e -s --arg run_id "$RUN_ID" '
      map(select(type == "object" and .run_id == $run_id))
      | length > 0
      and any(.[]; .strict_evidence == true)
    ' "$INDEX_PATH" >/dev/null || fail "run not indexed with strict evidence: run_id=$RUN_ID"
  else
    jq -e -s --arg run_id "$RUN_ID" '
      map(select(type == "object" and .run_id == $run_id))
      | length > 0
    ' "$INDEX_PATH" >/dev/null || fail "run not indexed: run_id=$RUN_ID"
  fi
fi

if [[ "$REQUIRE_LANE_SPREAD" == "1" ]]; then
  spread_failures="$(
    jq -r '
      .workloads[]
      | select(
          (.writer_lanes.lane_count // 0) > 1
          and (
            (.writer_lanes.active_lane_count // 0) < 2
            or (.writer_lanes.total_assigned_shards // 0) < 2
            or (.writer_lanes.max_assigned_shard_share_pct // 100.0) >= 100.0
          )
        )
      | .name
    ' "$SUMMARY_PATH"
  )"
  if [[ -n "$spread_failures" ]]; then
    fail "lane spread evidence missing for workloads: $(echo "$spread_failures" | tr '\n' ',' | sed 's/,$//')"
  fi
fi

echo "claimable local-db-write evidence ok: run_id=$RUN_ID summary=$SUMMARY_PATH"
