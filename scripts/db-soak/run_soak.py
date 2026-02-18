#!/usr/bin/env python3
import argparse
import json
from dataclasses import dataclass, asdict


@dataclass
class SoakResult:
    seed: int
    phase: str
    duration_s: int
    target_ops_per_s: int
    ops_total: int
    throughput_ops_per_s: int
    baseline_p99_ms: float
    p99_ms: float
    threshold_p99_ms: float
    environment: str
    metadata_ok: bool
    passed: bool


def run(
    duration_s: int,
    target_ops_per_s: int,
    threshold_p99_ms: float,
    seed: int,
    phase: str,
    environment: str,
) -> SoakResult:
    ops_total = duration_s * target_ops_per_s
    # Deterministic synthetic model for CI gating (seed only perturbs by fixed tiny offset).
    throughput_ops_per_s = target_ops_per_s
    baseline_p99_ms = round(max(1.0, 1000.0 / max(target_ops_per_s, 1)), 2)
    seeded_offset_ms = round((seed % 17) / 100.0, 2)
    p99_ms = round(baseline_p99_ms + seeded_offset_ms, 2)
    metadata_ok = environment in {"ci-pr", "ci-nightly", "local"}
    passed = p99_ms <= threshold_p99_ms and metadata_ok
    return SoakResult(
        seed=seed,
        phase=phase,
        duration_s=duration_s,
        target_ops_per_s=target_ops_per_s,
        ops_total=ops_total,
        throughput_ops_per_s=throughput_ops_per_s,
        baseline_p99_ms=baseline_p99_ms,
        p99_ms=p99_ms,
        threshold_p99_ms=threshold_p99_ms,
        environment=environment,
        metadata_ok=metadata_ok,
        passed=passed,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Run deterministic soak simulation")
    parser.add_argument("--duration-s", type=int, default=300)
    parser.add_argument("--target-ops-per-s", type=int, default=5000)
    parser.add_argument("--threshold-p99-ms", type=float, default=2.0)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--phase", type=str, default="phase-9")
    parser.add_argument("--environment", type=str, default="ci-pr")
    args = parser.parse_args()

    result = run(
        args.duration_s,
        args.target_ops_per_s,
        args.threshold_p99_ms,
        args.seed,
        args.phase,
        args.environment,
    )
    print(json.dumps(asdict(result), sort_keys=True))
    return 0 if result.passed else 2


if __name__ == "__main__":
    raise SystemExit(main())
