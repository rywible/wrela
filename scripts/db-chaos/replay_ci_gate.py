#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import time
from pathlib import Path

MANIFEST_SCHEMA_VERSION = 1


def load_compare_module():
    path = Path(__file__).resolve().parent / "compare_replay_signatures.py"
    spec = importlib.util.spec_from_file_location("compare_replay_signatures", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load helper module: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


COMPARE = load_compare_module()


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def validate_manifest(manifest: dict) -> list[dict]:
    failures: list[dict] = []
    schema_version = manifest.get("schema_version")
    if schema_version != MANIFEST_SCHEMA_VERSION:
        failures.append(
            {
                "code": "determinism.invalid_manifest_schema",
                "expected_schema_version": MANIFEST_SCHEMA_VERSION,
                "actual_schema_version": schema_version,
            }
        )

    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list):
        failures.append(
            {
                "code": "determinism.invalid_manifest_shape",
                "detail": "artifacts must be a list",
            }
        )
        return failures

    if not artifacts:
        failures.append(
            {
                "code": "determinism.empty_manifest",
                "detail": "manifest has no canonical replay artifacts",
            }
        )
    return failures


def compare_perf(
    baseline: dict,
    candidate: dict,
    max_latency_regression_pct: float,
    max_throughput_regression_pct: float,
) -> tuple[list[dict], dict]:
    failures: list[dict] = []
    deltas: dict[str, float] = {}

    for metric in ("p95_ms", "p99_ms"):
        base = baseline.get(metric)
        cand = candidate.get(metric)
        if not isinstance(base, (int, float)) or not isinstance(cand, (int, float)) or base <= 0:
            failures.append(
                {
                    "code": "perf.metric_missing",
                    "metric": metric,
                    "detail": f"invalid baseline/candidate values ({base!r}, {cand!r})",
                }
            )
            continue
        regression_pct = ((cand - base) / base) * 100.0
        deltas[f"{metric}_regression_pct"] = regression_pct
        if regression_pct > max_latency_regression_pct:
            failures.append(
                {
                    "code": "perf.latency_regression",
                    "metric": metric,
                    "baseline": base,
                    "candidate": cand,
                    "regression_pct": regression_pct,
                    "max_allowed_pct": max_latency_regression_pct,
                }
            )

    base_ops = baseline.get("ops_per_sec")
    cand_ops = candidate.get("ops_per_sec")
    if not isinstance(base_ops, (int, float)) or not isinstance(cand_ops, (int, float)) or base_ops <= 0:
        failures.append(
            {
                "code": "perf.metric_missing",
                "metric": "ops_per_sec",
                "detail": f"invalid baseline/candidate values ({base_ops!r}, {cand_ops!r})",
            }
        )
    else:
        drop_pct = ((base_ops - cand_ops) / base_ops) * 100.0
        deltas["ops_per_sec_drop_pct"] = drop_pct
        if drop_pct > max_throughput_regression_pct:
            failures.append(
                {
                    "code": "perf.throughput_regression",
                    "metric": "ops_per_sec",
                    "baseline": base_ops,
                    "candidate": cand_ops,
                    "drop_pct": drop_pct,
                    "max_allowed_pct": max_throughput_regression_pct,
                }
            )

    return failures, deltas


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--canonical-root",
        default="docs/db/replay-corpus/v1",
        help="Canonical replay corpus root",
    )
    parser.add_argument(
        "--candidate-root",
        default="tests/.artifacts",
        help="Candidate replay artifacts root",
    )
    parser.add_argument(
        "--baseline-perf",
        required=True,
        help="Baseline performance artifact JSON path",
    )
    parser.add_argument(
        "--candidate-perf",
        required=True,
        help="Candidate performance artifact JSON path",
    )
    parser.add_argument(
        "--max-latency-regression-pct",
        type=float,
        default=10.0,
        help="Maximum allowed p95/p99 latency regression percentage",
    )
    parser.add_argument(
        "--max-throughput-regression-pct",
        type=float,
        default=5.0,
        help="Maximum allowed ops/sec drop percentage",
    )
    parser.add_argument(
        "--out",
        default="artifacts/replay-ci-gate-report.json",
        help="Typed replay gate report artifact path",
    )
    args = parser.parse_args(argv)

    canonical_root = Path(args.canonical_root)
    candidate_root = Path(args.candidate_root)
    baseline_perf_path = Path(args.baseline_perf)
    candidate_perf_path = Path(args.candidate_perf)
    manifest_path = canonical_root / "manifest.json"

    failures: list[dict] = []
    invariant_errors: list[str] = []
    deterministic_mismatches: list[dict] = []

    manifest = load_json(manifest_path)
    failures.extend(validate_manifest(manifest))
    expected_artifacts = manifest.get("artifacts", [])
    if not isinstance(expected_artifacts, list):
        expected_artifacts = []

    expected_paths = set()
    duplicate_manifest_paths = set()
    for entry in expected_artifacts:
        rel_path = entry.get("path")
        expected_signature = entry.get("signature")
        if not isinstance(rel_path, str) or not isinstance(expected_signature, str):
            failures.append(
                {
                    "code": "determinism.invalid_manifest_entry",
                    "entry": entry,
                }
            )
            continue
        if rel_path in expected_paths:
            duplicate_manifest_paths.add(rel_path)
            continue
        expected_paths.add(rel_path)
        candidate_path = candidate_root / rel_path
        if not candidate_path.exists():
            failures.append(
                {
                    "code": "determinism.missing_artifact",
                    "artifact": rel_path,
                }
            )
            continue
        signature, errors = COMPARE.replay_signature(candidate_path)
        invariant_errors.extend(errors)
        if errors:
            continue
        if signature != expected_signature:
            deterministic_mismatches.append(
                {
                    "artifact": rel_path,
                    "expected_signature": expected_signature,
                    "candidate_signature": signature,
                }
            )

    for rel_path in sorted(duplicate_manifest_paths):
        failures.append(
            {
                "code": "determinism.duplicate_manifest_path",
                "artifact": rel_path,
            }
        )

    candidate_files = {
        rel for rel in COMPARE.find_trace_files(candidate_root).keys() if rel != "manifest.json"
    }
    unexpected_candidate = sorted(candidate_files - expected_paths)
    for rel_path in unexpected_candidate:
        failures.append(
            {
                "code": "determinism.unexpected_artifact",
                "artifact": rel_path,
            }
        )

    if invariant_errors:
        failures.append(
            {
                "code": "invariant.regression",
                "errors": invariant_errors,
            }
        )
    if deterministic_mismatches:
        failures.append(
            {
                "code": "determinism.mismatch",
                "mismatches": deterministic_mismatches,
            }
        )

    baseline_perf = load_json(baseline_perf_path)
    candidate_perf = load_json(candidate_perf_path)
    perf_failures, perf_deltas = compare_perf(
        baseline_perf,
        candidate_perf,
        args.max_latency_regression_pct,
        args.max_throughput_regression_pct,
    )
    failures.extend(perf_failures)

    determinism_failures = [failure for failure in failures if failure.get("code", "").startswith("determinism.")]
    status = "pass" if not failures else "fail"
    report = {
        "schema_version": 1,
        "generated_at_unix_ms": int(time.time() * 1000),
        "status": status,
        "checks": {
            "invariant_regression": {"passed": not invariant_errors, "error_count": len(invariant_errors)},
            "determinism": {
                "passed": not determinism_failures,
                "mismatch_count": len(deterministic_mismatches),
                "unexpected_candidate_count": len(unexpected_candidate),
            },
            "perf_regression": {"passed": not perf_failures, "failure_count": len(perf_failures)},
        },
        "evidence": {
            "canonical_manifest": str(manifest_path),
            "candidate_root": str(candidate_root),
            "baseline_perf": str(baseline_perf_path),
            "candidate_perf": str(candidate_perf_path),
        },
        "summary": {
            "expected_artifact_count": len(expected_paths),
            "candidate_artifact_count": len(candidate_files),
            "unexpected_candidate_artifacts": unexpected_candidate,
            "perf_deltas": perf_deltas,
        },
        "failures": failures,
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if status == "fail":
        print(f"replay CI gate failed; report: {out_path}")
        return 1
    print(f"replay CI gate passed; report: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
