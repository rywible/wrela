#!/usr/bin/env python3
"""Compute deterministic throughput scaling envelope from region simulator output."""

from __future__ import annotations

import argparse
import json


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--region-report", required=True)
    parser.add_argument("--shards", default="8,16,32")
    parser.add_argument("--nodes", default="3,6,9")
    parser.add_argument("--out", default="artifacts/scale-throughput-report.json")
    return parser.parse_args()


def compute_capacity(summary: dict, shards: list[int], nodes: list[int]) -> dict:
    avg_availability = sum(item["availability"] for item in summary.values()) / len(summary)
    avg_rtt = sum(item["avg_rtt_ms"] for item in summary.values()) / len(summary)

    points = []
    for shard_count in shards:
        for node_count in nodes:
            base = shard_count * node_count * 110.0
            latency_penalty = max(1.0, avg_rtt / 10.0)
            tps = (base * avg_availability) / latency_penalty
            points.append(
                {
                    "shards": shard_count,
                    "nodes": node_count,
                    "estimated_tps": round(tps, 2),
                    "availability_factor": round(avg_availability, 4),
                    "latency_penalty": round(latency_penalty, 4),
                }
            )

    points.sort(key=lambda p: (p["shards"], p["nodes"]))
    return {
        "avg_availability": round(avg_availability, 4),
        "avg_rtt_ms": round(avg_rtt, 4),
        "points": points,
    }


def main() -> int:
    args = parse_args()
    with open(args.region_report, "r", encoding="utf-8") as f:
        region_payload = json.load(f)

    summary = region_payload.get("summary", {})
    if not summary:
        raise SystemExit("region report summary is required")

    shards = [int(token) for token in args.shards.split(",") if token.strip()]
    nodes = [int(token) for token in args.nodes.split(",") if token.strip()]
    report = compute_capacity(summary, shards, nodes)

    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)
        f.write("\n")

    print(f"scale benchmark report written: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
