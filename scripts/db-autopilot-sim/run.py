#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def evaluate_scenario(row):
    skew_ok = row["max_to_mean_ratio"] <= row["skew_threshold"]
    failure_budget_ok = row["survivable_additional_failures"] >= row["required_additional_failures"]
    degraded_ok = row["degraded_selected"] <= row["max_degraded_selected"]

    passed = skew_ok and failure_budget_ok and degraded_ok
    reasons = []
    if not skew_ok:
        reasons.append("skew ratio exceeds threshold")
    if not failure_budget_ok:
        reasons.append("failure budget not met")
    if not degraded_ok:
        reasons.append("degraded selected exceeds max")
    if not reasons:
        reasons.append("safety simulation passed")

    return {
        "scenario_id": row["scenario_id"],
        "passed": passed,
        "reasons": reasons,
        "timeline": [
            f"scenario={row['scenario_id']}",
            f"passes={str(passed).lower()}",
            f"max_to_mean_ratio={row['max_to_mean_ratio']:.6f}",
            f"survivable_additional_failures={row['survivable_additional_failures']}",
            f"degraded_selected={row['degraded_selected']}",
        ],
    }


def run(input_path, output_path):
    payload = json.loads(Path(input_path).read_text())
    scenarios = [evaluate_scenario(row) for row in payload["scenarios"]]
    report = {
        "version": 1,
        "all_passed": all(row["passed"] for row in scenarios),
        "scenarios": scenarios,
    }
    out = Path(output_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description="Deterministic autopilot replay harness")
    parser.add_argument("--input", required=True, help="Input scenario JSON path")
    parser.add_argument("--out", required=True, help="Output report JSON path")
    args = parser.parse_args()

    report = run(args.input, args.out)
    if report["all_passed"]:
        print(f"autopilot replay passed; report: {args.out}")
        return 0

    print(f"autopilot replay failed; report: {args.out}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
