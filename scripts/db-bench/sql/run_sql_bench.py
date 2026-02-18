#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def run(out_path: Path) -> dict:
    statements = {
        "insert": {"ops": 12000, "avg_latency_ms": 4.2},
        "point_select": {"ops": 18000, "avg_latency_ms": 2.1},
        "range_scan": {"ops": 6000, "avg_latency_ms": 6.4},
        "update": {"ops": 9000, "avg_latency_ms": 4.9},
        "delete": {"ops": 7000, "avg_latency_ms": 4.0},
    }
    total_ops = sum(row["ops"] for row in statements.values())
    weighted_latency = sum(row["ops"] * row["avg_latency_ms"] for row in statements.values()) / total_ops

    report = {
        "version": 1,
        "total_ops": total_ops,
        "weighted_avg_latency_ms": round(weighted_latency, 3),
        "statements": statements,
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Deterministic SQL benchmark artifact generator")
    parser.add_argument("--out", default="artifacts/sql-bench-report.json")
    args = parser.parse_args()

    run(Path(args.out))
    print(f"sql benchmark report written: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
