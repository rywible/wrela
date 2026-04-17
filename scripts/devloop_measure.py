#!/usr/bin/env python3
"""Measure the Phase 52 developer loop and write JSON reports under .artifacts/devloop/."""

from __future__ import annotations

import argparse
import json
import os
import signal
import shutil
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ARTIFACT_DIR = REPO_ROOT / ".artifacts" / "devloop"

CHECK_WARM = "cargo check --workspace"
CHECK_CLEANROOM = (
    "CARGO_INCREMENTAL=0 "
    "CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/check "
    "cargo check --workspace"
)
TEST_WORKSPACE_WARM = "cargo test --workspace"
TEST_CLEANROOM = (
    "CARGO_INCREMENTAL=0 "
    "CARGO_TARGET_DIR=.artifacts/cargo-cleanroom/test "
    "cargo test --workspace"
)
FAST_VERIFY = "cargo test -p wrela --test repo_smoke && cargo run -p wrela -- test language/spec --lane=fast"
FULL_VERIFY = "cargo test --workspace && cargo run -p wrela -- test language/spec --lane=full"
FAST_VERIFY_BUDGET_MS = 60_000

CLI_SPLIT_BLOCKERS = [
    "Non-CLI integration crates still invoke CARGO_BIN_EXE_wrela "
    "(preview_project, contract_blackbox, spec_project_integrity, extraction_regression, repo_smoke).",
    "Default repo lanes still run cargo check --workspace and cargo test --workspace, "
    "so a split would not yet remove the everyday workspace rebuild cost.",
    "The wrela binary still lives under compiler/Cargo.toml, so package ownership and test ownership "
    "are not separated enough to pay for a crate split cleanly yet.",
]


def scenario_catalog() -> dict[str, dict[str, object]]:
    return {
        "check_warm": {
            "command": "just check",
            "resolved_command": CHECK_WARM,
            "warm": True,
            "timeout_ms": 600_000,
            "notes": "Default incremental workspace typecheck on a primed SHA.",
            "exclusions": "No tests, no cleanroom artifact dir, and no authored-world runner.",
        },
        "check_cleanroom": {
            "command": "just check-clean",
            "resolved_command": CHECK_CLEANROOM,
            "warm": False,
            "timeout_ms": 600_000,
            "cleanroom_target_dir": ".artifacts/cargo-cleanroom/check",
            "notes": "Cleanroom workspace typecheck with incremental disabled and isolated artifacts.",
            "exclusions": "Not the everyday path; measures cleanroom truth-first cost.",
        },
        "test_workspace_warm": {
            "command": "cargo test --workspace",
            "resolved_command": TEST_WORKSPACE_WARM,
            "warm": True,
            "timeout_ms": 900_000,
            "notes": "Warm workspace Rust test pass using the default incremental target dir.",
            "exclusions": "No authored-world fast/full lane composition.",
        },
        "test_cleanroom": {
            "command": "just test-clean",
            "resolved_command": TEST_CLEANROOM,
            "warm": False,
            "timeout_ms": 900_000,
            "cleanroom_target_dir": ".artifacts/cargo-cleanroom/test",
            "notes": "Cleanroom workspace Rust test pass with incremental disabled and isolated artifacts.",
            "exclusions": "Not the default developer loop and does not run the authored-world lane.",
        },
        "fast_verify": {
            "command": "just test",
            "resolved_command": FAST_VERIFY,
            "warm": True,
            "budget_ms": FAST_VERIFY_BUDGET_MS,
            "timeout_ms": 900_000,
            "notes": "Default repo fast lane: repo smoke plus authored fast verification.",
            "exclusions": "No full workspace Rust sweep and no cleanroom artifact dir.",
        },
        "full_verify": {
            "command": "just test-all",
            "resolved_command": FULL_VERIFY,
            "warm": True,
            "timeout_ms": 1_200_000,
            "notes": "Full repo verification lane: workspace Rust tests plus authored full verification.",
            "exclusions": "No lint, fmt-check, or perf lanes.",
        },
        "frontend_edit_check": {
            "command": "just check",
            "resolved_command": CHECK_WARM,
            "warm": True,
            "timeout_ms": 600_000,
            "notes": "Representative frontend-only edit burst after touching the parser surface.",
            "exclusions": "Touch-only edit scope; file contents are unchanged.",
            "context": "frontend",
            "touched_files": ["compiler/parser/mod.rs"],
        },
        "query_exec_edit_check": {
            "command": "just check",
            "resolved_command": CHECK_WARM,
            "warm": True,
            "timeout_ms": 600_000,
            "notes": "Representative query-exec edit burst after touching query execution.",
            "exclusions": "Touch-only edit scope; file contents are unchanged.",
            "context": "query_exec",
            "touched_files": ["compiler/query_exec/context.rs"],
        },
        "cli_edit_check": {
            "command": "just check",
            "resolved_command": CHECK_WARM,
            "warm": True,
            "timeout_ms": 600_000,
            "notes": "Representative CLI edit burst after touching CLI argument parsing.",
            "exclusions": "Touch-only edit scope; file contents are unchanged.",
            "context": "cli",
            "touched_files": ["compiler/bin/wrela/cli_args.rs"],
        },
    }


def git_output(*args: str) -> str:
    proc = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout.strip()


def terminate_process_group(pid: int, proc: subprocess.Popen[str]) -> None:
    try:
        os.killpg(pid, signal.SIGTERM)
    except (ProcessLookupError, PermissionError):
        pass
    try:
        proc.terminate()
    except ProcessLookupError:
        return
    subprocess.run(["pkill", "-TERM", "-P", str(pid)], check=False, capture_output=True)
    time.sleep(1)
    try:
        os.killpg(pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    try:
        proc.kill()
    except ProcessLookupError:
        return
    subprocess.run(["pkill", "-KILL", "-P", str(pid)], check=False, capture_output=True)


def shell_run(command: str, timeout_ms: int | None) -> dict[str, object]:
    proc = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        shell=True,
        executable="/bin/bash",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    timeout_seconds = None if timeout_ms is None else timeout_ms / 1000.0
    try:
        stdout, stderr = proc.communicate(timeout=timeout_seconds)
        return {
            "returncode": proc.returncode,
            "stdout": stdout,
            "stderr": stderr,
            "timed_out": False,
        }
    except subprocess.TimeoutExpired:
        terminate_process_group(proc.pid, proc)
        stdout, stderr = proc.communicate()
        return {
            "returncode": proc.returncode if proc.returncode is not None else 124,
            "stdout": stdout,
            "stderr": stderr,
            "timed_out": True,
        }


def capture_tail(text: str, max_lines: int = 20) -> list[str]:
    lines = [line for line in text.splitlines() if line.strip()]
    return lines[-max_lines:]


def touch_files(paths: list[str]) -> None:
    now = time.time()
    for rel_path in paths:
        path = REPO_ROOT / rel_path
        if not path.exists():
            raise FileNotFoundError(f"touch target does not exist: {rel_path}")
        os.utime(path, (now, now))


def measure_scenario(
    scenario_id: str,
    scenario: dict[str, object],
    skip_warmup: bool,
) -> dict[str, object]:
    warm = bool(scenario.get("warm", False))
    touched_files = list(scenario.get("touched_files", []))
    cleanroom_target_dir = scenario.get("cleanroom_target_dir")
    timeout_ms = int(scenario["timeout_ms"]) if scenario.get("timeout_ms") is not None else None
    warmup_result: dict[str, object] | None = None
    if warm and not skip_warmup:
        priming = shell_run(str(scenario["resolved_command"]), timeout_ms)
        warmup_result = {
            "success": int(priming["returncode"]) == 0 and not bool(priming["timed_out"]),
            "timed_out": priming["timed_out"],
            "exit_code": priming["returncode"],
            "stdout_tail": capture_tail(str(priming["stdout"])),
            "stderr_tail": capture_tail(str(priming["stderr"])),
        }

    if touched_files:
        touch_files(touched_files)

    if cleanroom_target_dir is not None:
        shutil.rmtree(REPO_ROOT / str(cleanroom_target_dir), ignore_errors=True)

    started_at = datetime.now(timezone.utc)
    started = time.perf_counter()
    proc = shell_run(str(scenario["resolved_command"]), timeout_ms)
    elapsed_ms = int(round((time.perf_counter() - started) * 1000))

    return {
        "id": scenario_id,
        "command": scenario["command"],
        "resolved_command": scenario["resolved_command"],
        "budget_ms": scenario.get("budget_ms"),
        "timeout_ms": timeout_ms,
        "warm": warm,
        "warmup_mode": "auto" if warm and not skip_warmup else "manual_or_skipped",
        "warmup_result": warmup_result,
        "started_at_utc": started_at.isoformat(),
        "elapsed_ms": elapsed_ms,
        "timed_out": bool(proc["timed_out"]),
        "success": int(proc["returncode"]) == 0 and not bool(proc["timed_out"]),
        "exit_code": proc["returncode"],
        "notes": scenario["notes"],
        "exclusions": scenario["exclusions"],
        "context": scenario.get("context"),
        "touched_files": touched_files,
        "stdout_tail": capture_tail(str(proc["stdout"])),
        "stderr_tail": capture_tail(str(proc["stderr"])),
    }


def build_scorecard(results: list[dict[str, object]]) -> list[dict[str, object]]:
    scorecard: list[dict[str, object]] = []
    for result in results:
        success = bool(result["success"])
        budget_ms = result.get("budget_ms")
        elapsed_ms = int(result["elapsed_ms"])
        if not success:
            status = "failed"
            delta_ms = None
        elif budget_ms is None:
            status = "measured"
            delta_ms = None
        else:
            budget_value = int(budget_ms)
            delta_ms = elapsed_ms - budget_value
            status = "within_budget" if elapsed_ms <= budget_value else "missed_budget"
        scorecard.append(
            {
                "id": result["id"],
                "status": status,
                "elapsed_ms": elapsed_ms,
                "budget_ms": budget_ms,
                "delta_ms": delta_ms,
            }
        )
    return scorecard


def round_ratio(numerator: int, denominator: int) -> float | None:
    if denominator <= 0:
        return None
    return round(numerator / denominator, 3)


def build_lookup(results: list[dict[str, object]]) -> dict[str, dict[str, object]]:
    return {str(result["id"]): result for result in results}


def compare_pair(
    lookup: dict[str, dict[str, object]],
    comparison_id: str,
    warm_id: str,
    cleanroom_id: str,
) -> dict[str, object] | None:
    warm = lookup.get(warm_id)
    cleanroom = lookup.get(cleanroom_id)
    if warm is None or cleanroom is None:
        return None
    warm_elapsed = int(warm["elapsed_ms"])
    cleanroom_elapsed = int(cleanroom["elapsed_ms"])
    return {
        "id": comparison_id,
        "warm_scenario": warm_id,
        "cleanroom_scenario": cleanroom_id,
        "warm_elapsed_ms": warm_elapsed,
        "cleanroom_elapsed_ms": cleanroom_elapsed,
        "delta_ms": cleanroom_elapsed - warm_elapsed,
        "ratio": round_ratio(cleanroom_elapsed, warm_elapsed),
    }


def build_warm_vs_cleanroom(lookup: dict[str, dict[str, object]]) -> list[dict[str, object]]:
    comparisons: list[dict[str, object]] = []
    for comparison_id, warm_id, cleanroom_id in [
        ("workspace_check", "check_warm", "check_cleanroom"),
        ("workspace_test", "test_workspace_warm", "test_cleanroom"),
    ]:
        comparison = compare_pair(lookup, comparison_id, warm_id, cleanroom_id)
        if comparison is not None:
            comparisons.append(comparison)
    return comparisons


def build_compile_bursts(lookup: dict[str, dict[str, object]]) -> list[dict[str, object]]:
    baseline = lookup.get("check_warm")
    baseline_elapsed = int(baseline["elapsed_ms"]) if baseline is not None else None
    bursts: list[dict[str, object]] = []
    for scenario_id in ["frontend_edit_check", "query_exec_edit_check", "cli_edit_check"]:
        scenario = lookup.get(scenario_id)
        if scenario is None:
            continue
        elapsed_ms = int(scenario["elapsed_ms"])
        bursts.append(
            {
                "id": scenario_id,
                "context": scenario.get("context"),
                "touched_files": scenario.get("touched_files", []),
                "elapsed_ms": elapsed_ms,
                "baseline_check_elapsed_ms": baseline_elapsed,
                "delta_from_check_warm_ms": (
                    None if baseline_elapsed is None else elapsed_ms - baseline_elapsed
                ),
            }
        )
    return bursts


def build_cli_boundary_assessment(lookup: dict[str, dict[str, object]]) -> dict[str, object]:
    baseline = lookup.get("check_warm")
    cli_burst = lookup.get("cli_edit_check")
    frontend_burst = lookup.get("frontend_edit_check")
    query_exec_burst = lookup.get("query_exec_edit_check")
    return {
        "decision": "deferred",
        "decision_record": "docs/architecture/crate_split_decision.md",
        "why_not_yet": CLI_SPLIT_BLOCKERS,
        "evidence": {
            "check_warm_ms": baseline.get("elapsed_ms") if baseline else None,
            "frontend_edit_check_ms": frontend_burst.get("elapsed_ms") if frontend_burst else None,
            "query_exec_edit_check_ms": (
                query_exec_burst.get("elapsed_ms") if query_exec_burst else None
            ),
            "cli_edit_check_ms": cli_burst.get("elapsed_ms") if cli_burst else None,
            "note": (
                "CLI edit bursts still run through cargo check --workspace because the first split "
                "is intentionally deferred until non-CLI binary consumers and default lanes are untangled."
            ),
        },
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scenario",
        action="append",
        dest="scenarios",
        help=(
            "Scenario id to run. Repeat to select multiple ids. Defaults to the checked-in "
            "Phase 52 baseline scenarios."
        ),
    )
    parser.add_argument(
        "--report-name",
        default="phase52-baseline",
        help="Base name for the output report (default: phase52-baseline).",
    )
    parser.add_argument(
        "--machine-tag",
        default=os.environ.get("DEVLOOP_MACHINE_TAG") or socket.gethostname(),
        help="Machine tag to write into the report.",
    )
    parser.add_argument(
        "--skip-warmup",
        action="store_true",
        help="Mark warm scenarios as warm but skip the automatic priming pass.",
    )
    return parser.parse_args()


def default_scenario_ids() -> list[str]:
    return [
        "check_warm",
        "check_cleanroom",
        "test_workspace_warm",
        "test_cleanroom",
        "frontend_edit_check",
        "query_exec_edit_check",
        "cli_edit_check",
        "fast_verify",
    ]


def main() -> int:
    args = parse_args()
    catalog = scenario_catalog()
    scenario_ids = args.scenarios or default_scenario_ids()
    unknown = [scenario_id for scenario_id in scenario_ids if scenario_id not in catalog]
    if unknown:
        print(f"unknown scenario ids: {', '.join(unknown)}", file=sys.stderr)
        return 2

    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)

    results = [
        measure_scenario(scenario_id, catalog[scenario_id], args.skip_warmup)
        for scenario_id in scenario_ids
    ]
    generated_at = datetime.now(timezone.utc)
    git_sha = git_output("rev-parse", "--short", "HEAD")
    git_dirty = bool(git_output("status", "--short"))
    scorecard = build_scorecard(results)
    lookup = build_lookup(results)
    report = {
        "schema_version": 3,
        "report_name": args.report_name,
        "phase": 52,
        "generated_at_utc": generated_at.isoformat(),
        "repo_root": str(REPO_ROOT),
        "machine_tag": args.machine_tag,
        "git_sha": git_sha,
        "git_dirty": git_dirty,
        "warm_definition": (
            "Warm means the same resolved command completed once on the same git SHA in the same "
            "worktree before the measured run. Edit-scope scenarios warm first, then touch the "
            "representative file, then run the measured command. Cleanroom scenarios intentionally "
            "skip warm incremental state and use isolated target dirs with CARGO_INCREMENTAL=0."
        ),
        "scenario_count": len(results),
        "success_count": sum(1 for result in results if result["success"]),
        "failure_count": sum(1 for result in results if not result["success"]),
        "scorecard": scorecard,
        "budget_miss_count": sum(1 for entry in scorecard if entry["status"] == "missed_budget"),
        "warm_vs_cleanroom": build_warm_vs_cleanroom(lookup),
        "compile_bursts": build_compile_bursts(lookup),
        "cli_boundary_assessment": build_cli_boundary_assessment(lookup),
        "scenarios": results,
    }

    timestamp = generated_at.strftime("%Y%m%dT%H%M%SZ")
    stable_path = ARTIFACT_DIR / f"{args.report_name}.json"
    timestamp_path = ARTIFACT_DIR / f"{args.report_name}-{timestamp}.json"
    payload = json.dumps(report, indent=2, sort_keys=False)
    stable_path.write_text(payload + "\n", encoding="utf-8")
    timestamp_path.write_text(payload + "\n", encoding="utf-8")

    print(stable_path)
    print(timestamp_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
