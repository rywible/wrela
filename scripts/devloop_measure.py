#!/usr/bin/env python3
"""Measure the Phase 49 developer loop and write JSON reports under .artifacts/devloop/."""

from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ARTIFACT_DIR = REPO_ROOT / ".artifacts" / "devloop"

FAST_RUST_TESTS = (
    "cargo test -p wrela --test one_shot_metrics_harness --test spec_project_integrity "
    "--test thin_core_snapshot"
)
FAST_AUTHORED_TESTS = "cargo run -p wrela -- test language/spec --lane=spec"
FULL_RUST_TESTS = "cargo test --workspace"
FULL_AUTHORED_TESTS = "cargo run -p wrela -- test language/spec"
FMT_CHECK = "cargo fmt --all -- --check"
LINT = "cargo clippy --workspace --all-targets -- -D warnings"
PERF_SMOKE = "cargo run -p wrela -- perf benchmarks/micro --profile=smoke --runs=1"


def scenario_catalog() -> dict[str, dict[str, object]]:
    fast_verify = f"{FAST_RUST_TESTS} && {FAST_AUTHORED_TESTS}"
    full_verify = f"{FULL_RUST_TESTS} && {FULL_AUTHORED_TESTS}"
    ship = f"{FMT_CHECK} && {LINT} && {fast_verify} && {full_verify} && {PERF_SMOKE}"
    return {
        "check_warm": {
            "command": "just check",
            "resolved_command": "cargo check --workspace",
            "warm": True,
            "notes": "Warm means one successful priming run on the same git SHA before measurement.",
            "exclusions": "No test execution, no linking, and no authored-world proving surface.",
        },
        "build_no_run_warm": {
            "command": "cargo test --workspace --no-run",
            "resolved_command": "cargo test --workspace --no-run",
            "warm": True,
            "notes": "Builds test targets but does not execute them.",
            "exclusions": "No Rust or authored test execution.",
        },
        "rust_fast_lane": {
            "command": "just test",
            "resolved_command": fast_verify,
            "warm": True,
            "notes": "Fast repo lane: small Rust integrity proofs plus the authored spec lane.",
            "exclusions": "No full Rust workspace sweep and no perf closure lane.",
        },
        "rust_full_lane": {
            "command": "just test-all",
            "resolved_command": full_verify,
            "warm": True,
            "notes": "Full repo lane: full Rust workspace sweep plus the authored spec project.",
            "exclusions": "No lint, fmt-check, or closure perf.",
        },
        "perf_smoke": {
            "command": "just perf-smoke",
            "resolved_command": PERF_SMOKE,
            "warm": True,
            "notes": "Cheap perf sanity lane.",
            "exclusions": "Not the representative 1080p120 whole-frame closure lane.",
        },
        "ship": {
            "command": "just ship",
            "resolved_command": ship,
            "warm": True,
            "notes": "Local pre-ship gate in repo workflow order.",
            "exclusions": "Uses perf-smoke, not perf-closure.",
        },
        "frontend_edit_check": {
            "command": "just check",
            "resolved_command": "cargo check --workspace",
            "warm": True,
            "notes": "Representative frontend edit: parser-facing change followed by a warm check.",
            "exclusions": "Touch-only edit scope; source contents are unchanged.",
            "touched_files": ["compiler/parser/mod.rs"],
        },
        "query_exec_edit_check": {
            "command": "just check",
            "resolved_command": "cargo check --workspace",
            "warm": True,
            "notes": "Representative query execution edit followed by a warm check.",
            "exclusions": "Touch-only edit scope; source contents are unchanged.",
            "touched_files": ["compiler/query_exec/context.rs"],
        },
        "cli_edit_check": {
            "command": "just check",
            "resolved_command": "cargo check --workspace",
            "warm": True,
            "notes": "Representative CLI/tooling edit followed by a warm check.",
            "exclusions": "Touch-only edit scope; source contents are unchanged.",
            "touched_files": ["compiler/bin/wrela/cli_args.rs"],
        },
        "full_workspace_no_run": {
            "command": "cargo test --workspace --no-run",
            "resolved_command": "cargo test --workspace --no-run",
            "warm": True,
            "notes": "Representative no-run build of the full workspace.",
            "exclusions": "Does not execute tests.",
        },
        "fast_verify": {
            "command": "just test",
            "resolved_command": fast_verify,
            "warm": True,
            "notes": "Named repo fast verification lane.",
            "exclusions": "No full Rust workspace sweep and no perf closure lane.",
        },
        "full_verify": {
            "command": "just test-all",
            "resolved_command": full_verify,
            "warm": True,
            "notes": "Named repo full verification lane.",
            "exclusions": "No lint, fmt-check, or closure perf.",
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


def shell_run(command: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=REPO_ROOT,
        shell=True,
        executable="/bin/bash",
        capture_output=True,
        text=True,
        check=False,
    )


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
    warmup_result: dict[str, object] | None = None
    if warm and not skip_warmup:
        priming = shell_run(str(scenario["resolved_command"]))
        warmup_result = {
            "success": priming.returncode == 0,
            "exit_code": priming.returncode,
            "stdout_tail": capture_tail(priming.stdout),
            "stderr_tail": capture_tail(priming.stderr),
        }

    if touched_files:
        touch_files(touched_files)

    started_at = datetime.now(timezone.utc)
    started = time.perf_counter()
    proc = shell_run(str(scenario["resolved_command"]))
    elapsed_ms = int(round((time.perf_counter() - started) * 1000))

    return {
        "id": scenario_id,
        "command": scenario["command"],
        "resolved_command": scenario["resolved_command"],
        "warm": warm,
        "warmup_mode": "auto" if warm and not skip_warmup else "manual_or_skipped",
        "warmup_result": warmup_result,
        "started_at_utc": started_at.isoformat(),
        "elapsed_ms": elapsed_ms,
        "success": proc.returncode == 0,
        "exit_code": proc.returncode,
        "notes": scenario["notes"],
        "exclusions": scenario["exclusions"],
        "touched_files": touched_files,
        "stdout_tail": capture_tail(proc.stdout),
        "stderr_tail": capture_tail(proc.stderr),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scenario",
        action="append",
        dest="scenarios",
        help="Scenario id to run. Repeat to select multiple ids. Defaults to all Phase 49 scenarios.",
    )
    parser.add_argument(
        "--report-name",
        default="phase49-baseline",
        help="Base name for the output report (default: phase49-baseline).",
    )
    parser.add_argument(
        "--machine-tag",
        default=os.environ.get("DEVLOOP_MACHINE_TAG") or socket.gethostname(),
        help="Machine tag to write into the report.",
    )
    parser.add_argument(
        "--skip-warmup",
        action="store_true",
        help="Mark scenarios warm but do not run the automatic priming pass.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    catalog = scenario_catalog()
    scenario_ids = args.scenarios or list(catalog.keys())
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
    report = {
        "schema_version": 1,
        "report_name": args.report_name,
        "phase": 49,
        "generated_at_utc": generated_at.isoformat(),
        "repo_root": str(REPO_ROOT),
        "machine_tag": args.machine_tag,
        "git_sha": git_sha,
        "git_dirty": git_dirty,
        "warm_definition": (
            "Warm means the same resolved command completed once on the same git SHA in the same "
            "worktree before the measured run. Edit-scope scenarios warm first, then touch the "
            "representative file, then run the measured command."
        ),
        "scenario_count": len(results),
        "success_count": sum(1 for result in results if result["success"]),
        "failure_count": sum(1 for result in results if not result["success"]),
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
