#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: $0 <combat_feel_metrics_json> <report_path>" >&2
  exit 2
fi

METRICS_PATH="$1"
REPORT_PATH="$2"
SUMMARY_PATH="$(dirname "$REPORT_PATH")/summary.md"

mkdir -p "$(dirname "$REPORT_PATH")"

if [[ ! -f "$METRICS_PATH" ]]; then
  echo "combat feel metrics file not found: ${METRICS_PATH}" >&2
  exit 1
fi

readability_score="$(node -e 'const fs=require("node:fs");const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));process.stdout.write(String(Number(p.readability_score ?? NaN)));' "$METRICS_PATH")"
cancel_response_ms="$(node -e 'const fs=require("node:fs");const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));process.stdout.write(String(Number(p.cancel_response_ms ?? NaN)));' "$METRICS_PATH")"

readability_threshold=0.80
cancel_threshold_ms=120

status="pass"
if ! awk "BEGIN {exit !($readability_score >= $readability_threshold)}"; then
  status="fail"
fi
if ! awk "BEGIN {exit !($cancel_response_ms <= $cancel_threshold_ms)}"; then
  status="fail"
fi

cat > "$REPORT_PATH" <<JSON
{
  "schema_version": 1,
  "kind": "animation-combat-feel-report-v1",
  "status": "${status}",
  "thresholds": {
    "readability_score": ${readability_threshold},
    "cancel_response_ms": ${cancel_threshold_ms}
  },
  "metrics": {
    "readability_score": ${readability_score},
    "cancel_response_ms": ${cancel_response_ms}
  }
}
JSON

cat > "$SUMMARY_PATH" <<SUMMARY
# Combat Feel Gate Summary

status: ${status}
readability_score: ${readability_score}
cancel_response_ms: ${cancel_response_ms}
SUMMARY

if [[ "$status" == "pass" ]]; then
  exit 0
fi

echo "combat feel gate failed: thresholds exceeded" >&2
exit 1
