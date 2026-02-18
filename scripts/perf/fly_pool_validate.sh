#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POOL_CONFIG="${1:-${FLY_POOL_CONFIG:-${ROOT}/scripts/perf/fly_pool.json}}"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

if [[ ! -f "${POOL_CONFIG}" ]]; then
  echo "error: missing pool config: ${POOL_CONFIG}" >&2
  exit 1
fi

jq -e '
  .version == 1 and
  (.runners | type == "array") and
  (.runners | length > 0) and
  all(.runners[];
    (.name | type == "string" and length > 0) and
    (.app | type == "string" and length > 0) and
    (.machine_id | type == "string" and length > 0) and
    (.region | type == "string" and length > 0) and
    (.cpu_kind | type == "string" and length > 0) and
    (.enabled | type == "boolean")
  )
' "${POOL_CONFIG}" >/dev/null

jq -r '.runners[] | "\(.app)::\(.machine_id)"' "${POOL_CONFIG}" | sort | uniq -d | rg . >/dev/null && {
  echo "error: duplicate app+machine_id entries in ${POOL_CONFIG}" >&2
  exit 1
}

echo "Pool config valid: ${POOL_CONFIG}"
