import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest


MODULE_PATH = (
    pathlib.Path(__file__).resolve().parents[1] / "compare_replay_signatures.py"
)
SPEC = importlib.util.spec_from_file_location("compare_replay_signatures", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MOD
SPEC.loader.exec_module(MOD)


def sample_trace(seed: int) -> dict:
    return {
        "version": 1,
        "generated_at_unix_ms": 123,
        "test_id": "tests/sim/demo::test_demo",
        "canonical_test_id": "tests/sim/demo::test_demo",
        "lane": "sim",
        "seed": seed,
        "failure": "assertion failed",
        "events": [
            {
                "seq": 0,
                "operation": {"phase": "dispatch", "action": "start", "commit_state": "pre-commit"},
                "route": {
                    "lane": "sim",
                    "scheduler_seed": seed,
                    "target": "tests/sim/demo::test_demo",
                },
                "timing": {"logical_step": 0, "observed_unix_ms": 123},
                "fault": None,
                "outcome": "started",
            },
            {
                "seq": 1,
                "operation": {"phase": "dispatch", "action": "commit", "commit_state": "failed"},
                "route": {
                    "lane": "sim",
                    "scheduler_seed": seed,
                    "target": "tests/sim/demo::test_demo",
                },
                "timing": {"logical_step": 1, "observed_unix_ms": 123},
                "fault": {
                    "kind": "injected_failure",
                    "source": "lane_runtime",
                    "seed": seed,
                    "detail": "assertion failed",
                },
                "outcome": "failed",
            },
        ],
    }


class CompareReplaySignaturesTests(unittest.TestCase):
    def test_reports_mismatch_when_signatures_drift(self):
        with tempfile.TemporaryDirectory() as baseline_dir, tempfile.TemporaryDirectory() as candidate_dir:
            baseline_path = pathlib.Path(baseline_dir) / "sim" / "demo" / "7.json"
            candidate_path = pathlib.Path(candidate_dir) / "sim" / "demo" / "7.json"
            baseline_path.parent.mkdir(parents=True, exist_ok=True)
            candidate_path.parent.mkdir(parents=True, exist_ok=True)
            baseline_path.write_text(json.dumps(sample_trace(7)), encoding="utf-8")
            changed = sample_trace(7)
            changed["events"][1]["operation"]["commit_state"] = "ok"
            candidate_path.write_text(json.dumps(changed), encoding="utf-8")

            report_path = pathlib.Path(candidate_dir) / "report.json"
            exit_code = MOD.main(
                [
                    "--baseline-root",
                    baseline_dir,
                    "--candidate-root",
                    candidate_dir,
                    "--out",
                    str(report_path),
                ]
            )
            self.assertEqual(exit_code, 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["summary"]["mismatched"], 1)
            self.assertEqual(report["mismatches"][0]["artifact"], "sim/demo/7.json")

    def test_passes_when_signatures_match(self):
        with tempfile.TemporaryDirectory() as baseline_dir, tempfile.TemporaryDirectory() as candidate_dir:
            baseline_path = pathlib.Path(baseline_dir) / "sim" / "demo" / "7.json"
            candidate_path = pathlib.Path(candidate_dir) / "sim" / "demo" / "7.json"
            baseline_path.parent.mkdir(parents=True, exist_ok=True)
            candidate_path.parent.mkdir(parents=True, exist_ok=True)
            trace = sample_trace(7)
            baseline_path.write_text(json.dumps(trace), encoding="utf-8")
            candidate_path.write_text(json.dumps(trace), encoding="utf-8")

            report_path = pathlib.Path(candidate_dir) / "report.json"
            exit_code = MOD.main(
                [
                    "--baseline-root",
                    baseline_dir,
                    "--candidate-root",
                    candidate_dir,
                    "--out",
                    str(report_path),
                ]
            )
            self.assertEqual(exit_code, 0)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["summary"]["matched"], 1)
            self.assertEqual(report["summary"]["mismatched"], 0)


if __name__ == "__main__":
    unittest.main()
