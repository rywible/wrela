#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-baseline}"
OUT_FILE="${2:-core/spec/runtime_rewrite/perf_${MODE}_raw.tsv}"
RUNS=3

if [[ "$MODE" != "baseline" && "$MODE" != "current" ]]; then
  echo "usage: $0 [baseline|current] [output_tsv]" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUT_FILE")"
echo -e "scenario\trun\tseconds\tcommand" > "$OUT_FILE"

declare -a SCENARIOS=(
  "pool_queue_mpsc_multi_producer|cargo test -p wrela_runtime pool_queue_mpsc_multi_producer -- --exact"
  "native_pool_backpressure_config_smoke|cargo test -p wrela --test codegen native_pool_backpressure_config_smoke -- --exact"
)

for item in "${SCENARIOS[@]}"; do
  scenario="${item%%|*}"
  cmd="${item#*|}"
  for run in $(seq 1 "$RUNS"); do
    sec="$(/usr/bin/time -p sh -c "$cmd" 2>&1 >/dev/null | awk '/^real /{print $2}')"
    echo -e "${scenario}\t${run}\t${sec}\t${cmd}" >> "$OUT_FILE"
  done
done

SUMMARY_FILE="${OUT_FILE%.tsv}_summary.tsv"
echo -e "scenario\tp50\tp95\tp99\tsamples" > "$SUMMARY_FILE"

for scenario in $(awk -F'\t' 'NR>1 {print $1}' "$OUT_FILE" | sort -u); do
  values="$(awk -F'\t' -v scenario="$scenario" 'NR>1 && $1==scenario {print $3}' "$OUT_FILE" | sort -n)"
  count="$(printf '%s\n' "$values" | sed '/^$/d' | wc -l | tr -d ' ')"
  if [[ "$count" -eq 0 ]]; then
    continue
  fi

  p50_index=$(( (count - 1) * 50 / 100 + 1 ))
  p95_index=$(( (count - 1) * 95 / 100 + 1 ))
  p99_index=$(( (count - 1) * 99 / 100 + 1 ))
  p50="$(printf '%s\n' "$values" | sed -n "${p50_index}p")"
  p95="$(printf '%s\n' "$values" | sed -n "${p95_index}p")"
  p99="$(printf '%s\n' "$values" | sed -n "${p99_index}p")"
  echo -e "${scenario}\t${p50}\t${p95}\t${p99}\t${count}" >> "$SUMMARY_FILE"
done

echo "wrote: $OUT_FILE"
echo "wrote: $SUMMARY_FILE"
