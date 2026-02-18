import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts" / "db-soak" / "run_soak.py"


def run_script(*args: str) -> tuple[int, dict]:
    proc = subprocess.run(
        [str(SCRIPT), *args],
        check=False,
        text=True,
        capture_output=True,
    )
    payload = json.loads(proc.stdout.strip())
    return proc.returncode, payload


def test_soak_passes_with_high_throughput():
    code, out = run_script(
        "--duration-s",
        "60",
        "--target-ops-per-s",
        "10000",
        "--threshold-p99-ms",
        "2.2",
        "--seed",
        "7",
        "--phase",
        "phase-9",
        "--environment",
        "ci-pr",
    )
    assert code == 0
    assert out["passed"]
    assert out["ops_total"] == 600000
    assert out["phase"] == "phase-9"


def test_soak_fails_with_low_throughput():
    code, out = run_script(
        "--duration-s",
        "60",
        "--target-ops-per-s",
        "100",
        "--threshold-p99-ms",
        "2.0",
        "--seed",
        "7",
        "--environment",
        "ci-pr",
    )
    assert code == 2
    assert not out["passed"]


def test_soak_fails_with_bad_environment_metadata():
    code, out = run_script(
        "--duration-s",
        "60",
        "--target-ops-per-s",
        "10000",
        "--threshold-p99-ms",
        "2.2",
        "--seed",
        "7",
        "--environment",
        "unknown-env",
    )
    assert code == 2
    assert not out["metadata_ok"]
