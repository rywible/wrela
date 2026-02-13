#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts}"
mkdir -p "$ARTIFACT_DIR"
OUT_JSON="$ARTIFACT_DIR/codec-bench.json"

RAW_OUTPUT="$(cargo test -p wrela_runtime db::codec::tests::codec_benchmark_report -- --exact --nocapture 2>&1)"
echo "$RAW_OUTPUT"
REPORT_LINE="$(printf '%s\n' "$RAW_OUTPUT" | rg '^codec_bench_report=' | tail -n 1)"
if [[ -z "$REPORT_LINE" ]]; then
  echo "codec benchmark report line not found" >&2
  exit 1
fi

printf '%s\n' "${REPORT_LINE#codec_bench_report=}" > "$OUT_JSON"
echo "codec benchmark artifact emitted to $OUT_JSON"
