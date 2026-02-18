#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <linear-issues.json> [report-dir]" >&2
  exit 2
fi

ISSUES_JSON="$1"
REPORT_DIR="${2:-artifacts/governance/weekly-drift}"
WEEK_TAG="$(date +%G-W%V)"
REPORT_PATH="$REPORT_DIR/g1-governance-drift-${WEEK_TAG}.md"

mkdir -p "$REPORT_DIR"

python3 scripts/governance/check_g1_governance.py \
  "$ISSUES_JSON" \
  --canonical-dag docs/project-governance/canonical-overlay-dag.md \
  --report "$REPORT_PATH"

echo "weekly governance report: $REPORT_PATH"
