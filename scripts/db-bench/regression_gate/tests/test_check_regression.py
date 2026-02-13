import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
SCRIPT = ROOT / "scripts" / "db-bench" / "regression_gate" / "check_regression.py"


def run_script(*args: str) -> tuple[int, dict]:
    proc = subprocess.run(
        [str(SCRIPT), *args],
        check=False,
        text=True,
        capture_output=True,
    )
    payload = json.loads(proc.stdout.strip())
    return proc.returncode, payload


def test_regression_gate_pass():
    code, out = run_script(
        "--baseline-tps",
        "10000",
        "--current-tps",
        "9800",
        "--baseline-p99-ms",
        "2.0",
        "--current-p99-ms",
        "2.2",
        "--max-drop-pct",
        "5.0",
        "--max-tail-increase-pct",
        "15.0",
        "--baseline-phase",
        "phase-8",
        "--current-phase",
        "phase-9",
    )
    assert code == 0
    assert out["passed"]
    assert out["throughput_passed"]
    assert out["tail_latency_passed"]


def test_regression_gate_fail():
    code, out = run_script(
        "--baseline-tps",
        "10000",
        "--current-tps",
        "9000",
        "--baseline-p99-ms",
        "2.0",
        "--current-p99-ms",
        "2.7",
        "--max-drop-pct",
        "5.0",
        "--max-tail-increase-pct",
        "15.0",
    )
    assert code == 3
    assert not out["passed"]


def test_regression_gate_fails_on_tail_latency_even_if_tps_ok():
    code, out = run_script(
        "--baseline-tps",
        "10000",
        "--current-tps",
        "9800",
        "--baseline-p99-ms",
        "2.0",
        "--current-p99-ms",
        "2.6",
        "--max-drop-pct",
        "5.0",
        "--max-tail-increase-pct",
        "15.0",
    )
    assert code == 3
    assert out["throughput_passed"]
    assert not out["tail_latency_passed"]
