#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/assert_strict_local_db_write_evidence.sh <run-dir|summary.json>

Fails unless local DB-write artifacts prove strict real-quorum evidence:
- config.real_quorum_mode == true
- replication.real_quorum_evidence == true for all workloads
- replication.simulation_commits == 0 for all workloads
- replication.successful_count > 0 for all workloads
- no quorum_failure_token present
USAGE
}

fail() {
  echo "error: $*" >&2
  exit 1
}

if ! command -v jq >/dev/null 2>&1; then
  fail "missing required tool: jq"
fi

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  usage
  exit 2
fi

SUMMARY_PATH=""
if [[ -d "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$TARGET" && pwd)/summary.json"
elif [[ -f "$TARGET" ]]; then
  SUMMARY_PATH="$(cd "$(dirname "$TARGET")" && pwd)/$(basename "$TARGET")"
else
  fail "path not found: $TARGET"
fi

[[ -f "$SUMMARY_PATH" ]] || fail "missing summary JSON: $SUMMARY_PATH"

if ! jq -e '
  (.config.real_quorum_mode == true)
  and (.workloads | type == "array" and length > 0)
  and (all(.workloads[];
    (.replication.real_quorum_evidence == true)
    and (.replication.simulation_commits == 0)
    and (.replication.successful_count > 0)
    and (.replication.contacted_count >= .replication.successful_count)
    and ((.replication.quorum_failure_token == null) or (.replication.quorum_failure_token == ""))
  ))
' "$SUMMARY_PATH" >/dev/null; then
  fail "strict local DB-write evidence assertion failed"
fi

echo "strict evidence ok: $SUMMARY_PATH"
