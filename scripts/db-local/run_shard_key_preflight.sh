#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <schema.json> <projected-shard-load.json>" >&2
  exit 2
fi

SCHEMA_JSON="$1"
LOAD_JSON="$2"
REPORT_DIR="${3:-artifacts/db/shard-key-preflight}"
mkdir -p "$REPORT_DIR"

python3 scripts/db-local/shard_key_schema_lint.py \
  "$SCHEMA_JSON" \
  --strict-low-cardinality \
  --report "$REPORT_DIR/schema-lint.md"

python3 scripts/db-local/shard_skew_preflight.py \
  "$LOAD_JSON" \
  --format json \
  > "$REPORT_DIR/skew-preflight.json"

echo "shard-key preflight artifacts:"
echo "  - $REPORT_DIR/schema-lint.md"
echo "  - $REPORT_DIR/skew-preflight.json"
