# Local Multi-Region Scale Model

This model provides deterministic local simulation and scale estimation for multi-region DB topology.

## Region Simulator

Generate a machine-readable region behavior report:

```bash
scripts/db-local/regions/simulate_multi_region.py \
  --regions scripts/db-local/regions/sample_regions.json \
  --ticks 120 \
  --seed 7 \
  --out artifacts/region-sim-report.json
```

The simulator emits:

- per-tick RTT and outage events,
- per-region average RTT,
- per-region outage count and availability.

## Scale Throughput Benchmark

Use the simulator output to estimate throughput scaling over shard/node matrix:

```bash
scripts/db-bench/scale/run_scale.py \
  --region-report artifacts/region-sim-report.json \
  --shards 8,16,32 \
  --nodes 3,6,9 \
  --out artifacts/scale-throughput-report.json
```

Output includes deterministic points with:

- `estimated_tps`
- `availability_factor`
- `latency_penalty`

## Validation

```bash
python3 scripts/db-local/tests/test_multi_region_simulator.py
python3 scripts/db-bench/tests/test_scale_report.py
```
