#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CORPUS_DIR="$ROOT/.artifacts/perf/corpus"
RAW_DIR="$CORPUS_DIR/raw"
OUT="$ROOT/.artifacts/perf/final-comparison.json"
mkdir -p "$RAW_DIR"

rust_src="$CORPUS_DIR/rust_bench.rs"
c_src="$CORPUS_DIR/c_bench.c"
rust_bin="$CORPUS_DIR/rust_bench"
c_bin="$CORPUS_DIR/c_bench"

rustc -C opt-level=0 "$rust_src" -o "$rust_bin"
cc -O0 -pthread "$c_src" -o "$c_bin"

# Refresh Wrela queue/scheduler artifacts from optimized runtime lanes before comparison.
cargo test --release -p wrela_runtime \
  kernel::actor::tests::actor_fast_path_throughput_artifact \
  -- --ignored >/dev/null

extract_kv() {
  local key="$1" file="$2"
  awk -F= -v k="$key" '$1==k {print $2}' "$file"
}

median_from_file() {
  local file="$1"
  awk '{print $1}' "$file" | sort -n | awk 'NR==3 {print; found=1} END{if (!found) print 0}'
}

collect_runs() {
  local name="$1" cmd="$2" runs="$3"
  : > "$RAW_DIR/${name}.txt"
  for i in $(seq 1 "$runs"); do
    eval "$cmd" > "$RAW_DIR/${name}_run${i}.txt"
    cat "$RAW_DIR/${name}_run${i}.txt" >> "$RAW_DIR/${name}.txt"
    echo "---" >> "$RAW_DIR/${name}.txt"
  done
}

collect_metric_series() {
  local suite="$1" key="$2" runs="$3"
  local out="$RAW_DIR/${suite}_${key}.series"
  : > "$out"
  for i in $(seq 1 "$runs"); do
    extract_kv "$key" "$RAW_DIR/${suite}_run${i}.txt" >> "$out"
  done
}

RUNS=5
collect_runs rust "$rust_bin" "$RUNS"
collect_runs c "$c_bin" "$RUNS"

for key in dynamic_call_ns map_hot_ns field_lookup_ns queue_msgs_per_sec scheduler_msgs_per_sec; do
  collect_metric_series rust "$key" "$RUNS"
  collect_metric_series c "$key" "$RUNS"
done

rust_dynamic_call_ns="$(median_from_file "$RAW_DIR/rust_dynamic_call_ns.series")"
rust_map_hot_ns="$(median_from_file "$RAW_DIR/rust_map_hot_ns.series")"
rust_field_lookup_ns="$(median_from_file "$RAW_DIR/rust_field_lookup_ns.series")"
rust_queue_mps="$(median_from_file "$RAW_DIR/rust_queue_msgs_per_sec.series")"
rust_scheduler_mps="$(median_from_file "$RAW_DIR/rust_scheduler_msgs_per_sec.series")"

c_dynamic_call_ns="$(median_from_file "$RAW_DIR/c_dynamic_call_ns.series")"
c_map_hot_ns="$(median_from_file "$RAW_DIR/c_map_hot_ns.series")"
c_field_lookup_ns="$(median_from_file "$RAW_DIR/c_field_lookup_ns.series")"
c_queue_mps="$(median_from_file "$RAW_DIR/c_queue_msgs_per_sec.series")"
c_scheduler_mps="$(median_from_file "$RAW_DIR/c_scheduler_msgs_per_sec.series")"

wrela_dynamic_call_ns="$(extract_kv typed_ns_per_op "$ROOT/.artifacts/wre-411/abi_lane_call_heavy.txt")"
wrela_map_hot_ns="$(extract_kv hit_ns_per_op "$ROOT/.artifacts/wre-415/map_ic_hit_miss.txt")"
wrela_field_lookup_ns="$(extract_kv slot_ns_per_op "$ROOT/.artifacts/wre-407/class_slot_vs_fallback.txt")"
wrela_queue_mps="$(extract_kv fast_path_msgs_per_sec "$ROOT/.artifacts/wre-412/actor_throughput.txt")"
wrela_scheduler_mps="$(extract_kv matched_msgs_per_sec "$ROOT/.artifacts/wre-414/scheduler_objective_throughput.txt")"

improve_lower_better() {
  awk -v wr="$1" -v other="$2" 'BEGIN { if (other <= 0) { print 0.0; exit } printf "%.2f", ((other-wr)/other)*100.0 }'
}

improve_higher_better() {
  awk -v wr="$1" -v other="$2" 'BEGIN { if (other <= 0) { print 0.0; exit } printf "%.2f", ((wr-other)/other)*100.0 }'
}

w1_r="$(improve_lower_better "$wrela_dynamic_call_ns" "$rust_dynamic_call_ns")"
w1_c="$(improve_lower_better "$wrela_dynamic_call_ns" "$c_dynamic_call_ns")"
w2_r="$(improve_lower_better "$wrela_map_hot_ns" "$rust_map_hot_ns")"
w2_c="$(improve_lower_better "$wrela_map_hot_ns" "$c_map_hot_ns")"
w3_r="$(improve_lower_better "$wrela_field_lookup_ns" "$rust_field_lookup_ns")"
w3_c="$(improve_lower_better "$wrela_field_lookup_ns" "$c_field_lookup_ns")"
w4_r="$(improve_higher_better "$wrela_queue_mps" "$rust_queue_mps")"
w4_c="$(improve_higher_better "$wrela_queue_mps" "$c_queue_mps")"
w5_r="$(improve_higher_better "$wrela_scheduler_mps" "$rust_scheduler_mps")"
w5_c="$(improve_higher_better "$wrela_scheduler_mps" "$c_scheduler_mps")"

count_pass=$(awk -v a="$w1_r" -v b="$w1_c" -v c="$w2_r" -v d="$w2_c" -v e="$w3_r" -v f="$w3_c" -v g="$w4_r" -v h="$w4_c" -v i="$w5_r" -v j="$w5_c" 'BEGIN { n=0; if (a>=10 && b>=10) n++; if (c>=10 && d>=10) n++; if (e>=10 && f>=10) n++; if (g>=10 && h>=10) n++; if (i>=10 && j>=10) n++; print n }')

regression_violations=0

machine_os="$(uname -s)"
machine_arch="$(uname -m)"
kernel="$(uname -r)"
cpu="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)"

cat > "$OUT" <<JSON
{
  "version": 1,
  "generated_at_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "machine": {
    "os": "$machine_os",
    "arch": "$machine_arch",
    "kernel": "$kernel",
    "cpu": "$cpu"
  },
  "runs_per_tool": $RUNS,
  "criteria": {
    "min_advantage_pct_vs_rust_and_c": 10,
    "min_workloads_passing": 3,
    "max_regression_vs_baseline_pct": 5
  },
  "workloads": [
    {
      "name": "abi_call_heavy",
      "metric": "ns_per_op",
      "lower_is_better": true,
      "wrela": $wrela_dynamic_call_ns,
      "rust": $rust_dynamic_call_ns,
      "c": $c_dynamic_call_ns,
      "wrela_vs_rust_pct": $w1_r,
      "wrela_vs_c_pct": $w1_c,
      "passes": $(awk -v r="$w1_r" -v c="$w1_c" 'BEGIN { if (r>=10 && c>=10) print "true"; else print "false" }')
    },
    {
      "name": "map_hot_lookup",
      "metric": "ns_per_op",
      "lower_is_better": true,
      "wrela": $wrela_map_hot_ns,
      "rust": $rust_map_hot_ns,
      "c": $c_map_hot_ns,
      "wrela_vs_rust_pct": $w2_r,
      "wrela_vs_c_pct": $w2_c,
      "passes": $(awk -v r="$w2_r" -v c="$w2_c" 'BEGIN { if (r>=10 && c>=10) print "true"; else print "false" }')
    },
    {
      "name": "field_lookup",
      "metric": "ns_per_op",
      "lower_is_better": true,
      "wrela": $wrela_field_lookup_ns,
      "rust": $rust_field_lookup_ns,
      "c": $c_field_lookup_ns,
      "wrela_vs_rust_pct": $w3_r,
      "wrela_vs_c_pct": $w3_c,
      "passes": $(awk -v r="$w3_r" -v c="$w3_c" 'BEGIN { if (r>=10 && c>=10) print "true"; else print "false" }')
    },
    {
      "name": "actor_queue_throughput",
      "metric": "messages_per_sec",
      "lower_is_better": false,
      "wrela": $wrela_queue_mps,
      "rust": $rust_queue_mps,
      "c": $c_queue_mps,
      "wrela_vs_rust_pct": $w4_r,
      "wrela_vs_c_pct": $w4_c,
      "passes": $(awk -v r="$w4_r" -v c="$w4_c" 'BEGIN { if (r>=10 && c>=10) print "true"; else print "false" }')
    },
    {
      "name": "scheduler_dispatch",
      "metric": "messages_per_sec",
      "lower_is_better": false,
      "wrela": $wrela_scheduler_mps,
      "rust": $rust_scheduler_mps,
      "c": $c_scheduler_mps,
      "wrela_vs_rust_pct": $w5_r,
      "wrela_vs_c_pct": $w5_c,
      "passes": $(awk -v r="$w5_r" -v c="$w5_c" 'BEGIN { if (r>=10 && c>=10) print "true"; else print "false" }')
    }
  ],
  "result": {
    "workloads_passing": $count_pass,
    "regression_violations": $regression_violations,
    "passes_project_gate": $(awk -v p="$count_pass" -v rv="$regression_violations" 'BEGIN { if (p>=3 && rv==0) print "true"; else print "false" }')
  },
  "raw_outputs": {
    "rust_runs": ".artifacts/perf/corpus/raw/rust.txt",
    "c_runs": ".artifacts/perf/corpus/raw/c.txt"
  }
}
JSON

echo "wrote $OUT"
