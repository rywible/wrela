#!/usr/bin/env python3
"""Deterministic local multi-region simulator with latency/failure overlays."""

from __future__ import annotations

import argparse
import json
import random
from dataclasses import dataclass


@dataclass
class RegionConfig:
    name: str
    base_rtt_ms: int
    failure_probability: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--regions", required=True, help="JSON file containing region configs")
    parser.add_argument("--ticks", type=int, default=50)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--out", default="artifacts/region-sim-report.json")
    return parser.parse_args()


def run_simulation(regions: list[RegionConfig], ticks: int, seed: int) -> dict:
    rng = random.Random(seed)
    events: list[dict] = []
    outage_counts = {region.name: 0 for region in regions}
    total_rtt = {region.name: 0 for region in regions}

    for tick in range(ticks):
        for region in regions:
            outage = rng.random() < region.failure_probability
            jitter = rng.randint(-4, 8)
            rtt = max(1, region.base_rtt_ms + jitter)
            total_rtt[region.name] += rtt
            if outage:
                outage_counts[region.name] += 1
            events.append(
                {
                    "tick": tick,
                    "region": region.name,
                    "rtt_ms": rtt,
                    "outage": outage,
                }
            )

    summary = {}
    for region in regions:
        summary[region.name] = {
            "avg_rtt_ms": total_rtt[region.name] / ticks,
            "outage_ticks": outage_counts[region.name],
            "availability": (ticks - outage_counts[region.name]) / ticks,
        }

    return {"ticks": ticks, "seed": seed, "events": events, "summary": summary}


def main() -> int:
    args = parse_args()
    with open(args.regions, "r", encoding="utf-8") as f:
        payload = json.load(f)

    regions = [
        RegionConfig(
            name=item["name"],
            base_rtt_ms=int(item["base_rtt_ms"]),
            failure_probability=float(item.get("failure_probability", 0.0)),
        )
        for item in payload.get("regions", [])
    ]
    if not regions:
        raise SystemExit("regions payload must include at least one region")

    report = run_simulation(regions, args.ticks, args.seed)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)
        f.write("\n")

    print(f"region simulator report written: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
