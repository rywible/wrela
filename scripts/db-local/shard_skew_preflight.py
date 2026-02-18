#!/usr/bin/env python3
"""
Deterministic shard skew preflight gate.

Input JSON shape:
{
  "profile": "global-3x",
  "shards": {
    "shard-a": 1200,
    "shard-b": 800,
    "shard-c": 1000
  }
}

Gate fails when max shard load exceeds configured skew ratio relative to mean.
"""

from __future__ import annotations

import argparse
import json
import math
import sys


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("input_json", type=argparse.FileType("r"))
    p.add_argument(
        "--max-skew-ratio",
        type=float,
        default=1.50,
        help="max allowed max/mean shard load ratio",
    )
    p.add_argument(
        "--min-shards",
        type=int,
        default=3,
        help="minimum shard count required by profile preflight",
    )
    p.add_argument("--format", choices=("text", "json"), default="text")
    return p.parse_args()


def evaluate(shards: dict[str, int], max_skew_ratio: float, min_shards: int) -> tuple[bool, dict]:
    if len(shards) < min_shards:
        return False, {
            "reason": f"insufficient shard count: got {len(shards)}, need >= {min_shards}",
            "recommendation": "increase shard fanout or use a composite shard key",
        }

    values = [int(v) for v in shards.values()]
    if any(v < 0 for v in values):
        return False, {
            "reason": "negative shard load observed",
            "recommendation": "fix telemetry source before preflight",
        }

    total = sum(values)
    mean = total / len(values)
    max_load = max(values)
    ratio = math.inf if mean == 0 and max_load > 0 else (1.0 if mean == 0 else max_load / mean)

    passed = ratio <= max_skew_ratio
    detail = {
        "shard_count": len(values),
        "total": total,
        "mean": mean,
        "max": max_load,
        "max_over_mean_ratio": ratio,
        "threshold": max_skew_ratio,
        "recommendation": "prefer composite keys like tenant_id + entity_id",
    }

    if not passed:
        detail["reason"] = (
            f"skew ratio {ratio:.3f} exceeds threshold {max_skew_ratio:.3f}"
        )

    return passed, detail


def main() -> int:
    args = parse_args()
    payload = json.load(args.input_json)
    shards = payload.get("shards") or {}
    if not isinstance(shards, dict) or not shards:
        msg = {
            "reason": "input must include non-empty 'shards' object",
            "recommendation": "provide per-shard projected load counts",
        }
        if args.format == "json":
            print(json.dumps({"status": "fail", **msg}))
        else:
            print(f"FAIL: {msg['reason']}")
        return 1

    passed, detail = evaluate(shards, args.max_skew_ratio, args.min_shards)

    if args.format == "json":
        print(json.dumps({"status": "pass" if passed else "fail", **detail}, sort_keys=True))
    else:
        prefix = "PASS" if passed else "FAIL"
        if passed:
            print(
                f"{prefix}: skew ratio {detail['max_over_mean_ratio']:.3f} <= {detail['threshold']:.3f} "
                f"(shards={detail['shard_count']})"
            )
        else:
            print(f"{prefix}: {detail['reason']}")
            print(f"hint: {detail['recommendation']}")

    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
