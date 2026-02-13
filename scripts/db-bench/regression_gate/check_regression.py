#!/usr/bin/env python3
import argparse
import json


def _drop_pct(baseline: float, current: float) -> float:
    return ((baseline - current) / baseline) * 100.0


def _increase_pct(baseline: float, current: float) -> float:
    return ((current - baseline) / baseline) * 100.0


def evaluate(
    baseline_tps: float,
    current_tps: float,
    baseline_p99_ms: float,
    current_p99_ms: float,
    max_drop_pct: float,
    max_tail_increase_pct: float,
    baseline_phase: str,
    current_phase: str,
) -> dict:
    if baseline_tps <= 0:
        raise ValueError("baseline_tps must be > 0")
    if baseline_p99_ms <= 0:
        raise ValueError("baseline_p99_ms must be > 0")
    drop_pct = _drop_pct(baseline_tps, current_tps)
    tail_increase_pct = _increase_pct(baseline_p99_ms, current_p99_ms)
    throughput_passed = drop_pct <= max_drop_pct
    tail_latency_passed = tail_increase_pct <= max_tail_increase_pct
    return {
        "lineage": {
            "baseline_phase": baseline_phase,
            "current_phase": current_phase,
        },
        "baseline": {"tps": baseline_tps, "p99_ms": baseline_p99_ms},
        "current": {"tps": current_tps, "p99_ms": current_p99_ms},
        "drop_pct": round(drop_pct, 3),
        "tail_increase_pct": round(tail_increase_pct, 3),
        "max_drop_pct": max_drop_pct,
        "max_tail_increase_pct": max_tail_increase_pct,
        "throughput_passed": throughput_passed,
        "tail_latency_passed": tail_latency_passed,
        "passed": throughput_passed and tail_latency_passed,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Check perf regression")
    parser.add_argument("--baseline-tps", type=float, required=True)
    parser.add_argument("--current-tps", type=float, required=True)
    parser.add_argument("--baseline-p99-ms", type=float, required=True)
    parser.add_argument("--current-p99-ms", type=float, required=True)
    parser.add_argument("--max-drop-pct", type=float, default=5.0)
    parser.add_argument("--max-tail-increase-pct", type=float, default=15.0)
    parser.add_argument("--baseline-phase", type=str, default="phase-8")
    parser.add_argument("--current-phase", type=str, default="phase-9")
    args = parser.parse_args()

    result = evaluate(
        args.baseline_tps,
        args.current_tps,
        args.baseline_p99_ms,
        args.current_p99_ms,
        args.max_drop_pct,
        args.max_tail_increase_pct,
        args.baseline_phase,
        args.current_phase,
    )
    print(json.dumps(result, sort_keys=True))
    return 0 if result["passed"] else 3


if __name__ == "__main__":
    raise SystemExit(main())
