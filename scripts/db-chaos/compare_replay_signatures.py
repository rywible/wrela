#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import time
from pathlib import Path


def find_trace_files(root: Path) -> dict[str, Path]:
    files: dict[str, Path] = {}
    if not root.exists():
        return files
    for path in sorted(root.rglob("*.json")):
        rel = str(path.relative_to(root))
        files[rel] = path
    return files


def replay_signature(path: Path) -> tuple[str | None, list[str]]:
    errors: list[str] = []
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        return None, [f"{path}: invalid json ({exc})"]

    version = payload.get("version")
    lane = payload.get("lane")
    seed = payload.get("seed")
    canonical = payload.get("canonical_test_id")
    events = payload.get("events")
    if not isinstance(events, list):
        return None, [f"{path}: missing events list"]

    signature_parts: list[str] = [f"v={version}|lane={lane}|seed={seed}|test={canonical}|"]
    expected_seq = 0
    expected_step = 0
    for event in events:
        seq = event.get("seq")
        if seq != expected_seq:
            errors.append(
                f"{path}: non-deterministic event sequence: got {seq}, expected {expected_seq}"
            )
            break
        timing = event.get("timing", {})
        logical_step = timing.get("logical_step")
        if logical_step != expected_step:
            errors.append(
                f"{path}: non-deterministic logical step: got {logical_step}, expected {expected_step}"
            )
            break
        route = event.get("route", {})
        if route.get("lane") != lane:
            errors.append(
                f"{path}: route lane mismatch: event lane {route.get('lane')!r} != artifact lane {lane!r}"
            )
        if route.get("scheduler_seed") != seed:
            errors.append(
                f"{path}: route seed mismatch: event seed {route.get('scheduler_seed')} != artifact seed {seed}"
            )
        fault = event.get("fault")
        if isinstance(fault, dict) and fault.get("seed") != seed:
            errors.append(
                f"{path}: fault seed mismatch: fault seed {fault.get('seed')} != artifact seed {seed}"
            )
        operation = event.get("operation", {})
        signature_parts.append(
            f"#{seq}:{operation.get('phase')}:{operation.get('action')}:{operation.get('commit_state')}:{event.get('outcome')}|"
        )
        expected_seq += 1
        expected_step += 1

    if errors:
        return None, errors
    return "".join(signature_parts), []


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-root", required=True, help="Directory with baseline replay traces")
    parser.add_argument(
        "--candidate-root", required=True, help="Directory with candidate replay traces"
    )
    parser.add_argument(
        "--out",
        default="artifacts/replay-signature-report.json",
        help="Output report path",
    )
    parser.add_argument(
        "--allow-extra-candidate",
        action="store_true",
        help="Allow candidate-only traces without failing",
    )
    args = parser.parse_args(argv)

    baseline_root = Path(args.baseline_root)
    candidate_root = Path(args.candidate_root)
    baseline = find_trace_files(baseline_root)
    candidate = find_trace_files(candidate_root)

    mismatch_rows: list[dict[str, str]] = []
    errors: list[str] = []
    baseline_only = sorted(set(baseline.keys()) - set(candidate.keys()))
    candidate_only = sorted(set(candidate.keys()) - set(baseline.keys()))

    compared = 0
    matched = 0
    for rel in sorted(set(baseline.keys()) & set(candidate.keys())):
        baseline_sig, baseline_errors = replay_signature(baseline[rel])
        candidate_sig, candidate_errors = replay_signature(candidate[rel])
        errors.extend(baseline_errors)
        errors.extend(candidate_errors)
        if baseline_errors or candidate_errors:
            continue
        compared += 1
        if baseline_sig == candidate_sig:
            matched += 1
            continue
        mismatch_rows.append(
            {
                "artifact": rel,
                "baseline_signature": baseline_sig or "",
                "candidate_signature": candidate_sig or "",
            }
        )

    report = {
        "generated_at_unix_ms": int(time.time() * 1000),
        "baseline_root": str(baseline_root),
        "candidate_root": str(candidate_root),
        "summary": {
            "baseline_count": len(baseline),
            "candidate_count": len(candidate),
            "compared": compared,
            "matched": matched,
            "mismatched": len(mismatch_rows),
            "missing_in_candidate": len(baseline_only),
            "extra_in_candidate": len(candidate_only),
            "validation_errors": len(errors),
        },
        "mismatches": mismatch_rows,
        "missing_in_candidate": baseline_only,
        "extra_in_candidate": candidate_only,
        "validation_errors": errors,
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    failed = bool(mismatch_rows or baseline_only or errors)
    if candidate_only and not args.allow_extra_candidate:
        failed = True
    if failed:
        print(f"replay signature gate failed; report: {out_path}")
        return 1
    print(f"replay signature gate passed; report: {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
