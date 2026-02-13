#!/usr/bin/env bash
set -euo pipefail

ROOT="${ROOT:-.data/db-local}"
REGIONS=("us-east-1" "us-central-1" "us-west-1")

mkdir -p "$ROOT"

idx=1
for region in "${REGIONS[@]}"; do
  node_dir="$ROOT/$region/node-$idx"
  mkdir -p "$node_dir"
  cat > "$node_dir/profile.json" <<JSON
{
  "node_id": $idx,
  "region": "$region",
  "wal_path": "$node_dir/wal.log",
  "status": "ready"
}
JSON
  : > "$node_dir/wal.log"
  idx=$((idx + 1))
done

echo "db-local smoke setup complete at $ROOT"

