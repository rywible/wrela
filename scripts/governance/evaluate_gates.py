#!/usr/bin/env python3
"""
Gate evaluation helper for WRE-604 / WRE-608.

Consumes:
- a threshold registry (docs/project-governance/gate-registry.json)
- a measured artifact report (ad-hoc JSON mapping metric name -> observed value)

Returns non-zero on threshold violation.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


def load_json(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--registry",
        default="docs/project-governance/gate-registry.json",
        help="Path to gate registry JSON",
    )
    parser.add_argument(
        "--report",
        required=True,
        help="Path to measured artifact report JSON",
    )
    args = parser.parse_args(argv)

    registry = load_json(args.registry)
    report = load_json(args.report)

    gates = registry.get("gates", {})
    violations = []

    for metric, cfg in gates.items():
        target = cfg.get("target")
        observed = report.get(metric)
        if observed is None:
            violations.append(f"missing measurement: {metric}")
            continue
        if observed > target:
            violations.append(f"{metric}: observed={observed} target={target}")

    for artifact in registry.get("required_artifacts", []):
        if artifact not in report.get("artifacts", []):
            violations.append(f"missing artifact: {artifact}")

    if violations:
        print("GATE FAILURE")
        for item in violations:
            print(f"- {item}")
        return 1

    print("All gates passed")
    return 0


if __name__ == "__main__":
    registry_path = pathlib.Path("docs/project-governance/gate-registry.json")
    if not registry_path.exists():
        print("missing gate registry file", file=sys.stderr)
        sys.exit(2)
    sys.exit(main())
