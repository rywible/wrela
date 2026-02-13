#!/usr/bin/env python3
import argparse
import json
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class FaultProfile:
    name: str
    latency_ms: int
    jitter_ms: int
    loss_percent: int
    reorder_percent: int


def estimate_metrics(profile: FaultProfile) -> dict:
    base_commit = 12
    commit_p99 = base_commit + profile.latency_ms + (2 * profile.jitter_ms) + (profile.loss_percent // 2)
    timeout_rate = min(100.0, round(profile.loss_percent * 0.9 + profile.reorder_percent * 0.4, 2))
    retry_rate = min(100.0, round(profile.loss_percent * 1.2 + profile.jitter_ms * 0.3, 2))
    passes = commit_p99 <= 250 and timeout_rate <= 15.0
    return {
        "profile": profile.name,
        "commit_p99_ms": commit_p99,
        "timeout_rate_pct": timeout_rate,
        "retry_rate_pct": retry_rate,
        "passes_slo": passes,
    }


def run(out_path: Path) -> dict:
    profiles = [
        FaultProfile("steady", latency_ms=5, jitter_ms=2, loss_percent=0, reorder_percent=0),
        FaultProfile("lossy", latency_ms=25, jitter_ms=8, loss_percent=12, reorder_percent=4),
        FaultProfile("partition-ish", latency_ms=60, jitter_ms=15, loss_percent=20, reorder_percent=10),
    ]
    rows = [estimate_metrics(profile) for profile in profiles]
    report = {
        "version": 1,
        "profiles": rows,
        "all_pass": all(row["passes_slo"] for row in rows),
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Deterministic net-fault profile simulator")
    parser.add_argument("--out", default="artifacts/network-chaos-report.json")
    args = parser.parse_args()

    report = run(Path(args.out))
    if report["all_pass"]:
        print(f"network chaos profiles pass; report: {args.out}")
    else:
        failing = [row["profile"] for row in report["profiles"] if not row["passes_slo"]]
        print(
            f"network chaos profiles fail (non-gating): {','.join(failing)}; report: {args.out}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
