import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest


MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "replay_ci_gate.py"
SPEC = importlib.util.spec_from_file_location("replay_ci_gate", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MOD
SPEC.loader.exec_module(MOD)


TRACE = {
    "version": 1,
    "generated_at_unix_ms": 123,
    "test_id": "tests/sim/demo::test_demo",
    "canonical_test_id": "tests/sim/demo::test_demo",
    "lane": "sim",
    "seed": 7,
    "failure": "assertion failed",
    "events": [
        {
            "seq": 0,
            "operation": {"phase": "dispatch", "action": "start", "commit_state": "pre-commit"},
            "route": {"lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo"},
            "timing": {"logical_step": 0, "observed_unix_ms": 123},
            "fault": None,
            "outcome": "started",
        },
        {
            "seq": 1,
            "operation": {"phase": "dispatch", "action": "commit", "commit_state": "failed"},
            "route": {"lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo"},
            "timing": {"logical_step": 1, "observed_unix_ms": 123},
            "fault": {"kind": "injected_failure", "source": "lane_runtime", "seed": 7, "detail": "assertion failed"},
            "outcome": "failed",
        },
    ],
}


class ReplayCiGateTests(unittest.TestCase):
    def test_gate_passes_with_matching_trace_and_perf(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            canonical_root = root / "canonical"
            candidate_root = root / "candidate"
            trace_path = "sim/demo/7.json"

            canonical_trace = canonical_root / trace_path
            candidate_trace = candidate_root / trace_path
            canonical_trace.parent.mkdir(parents=True, exist_ok=True)
            candidate_trace.parent.mkdir(parents=True, exist_ok=True)
            canonical_trace.write_text(json.dumps(TRACE), encoding="utf-8")
            candidate_trace.write_text(json.dumps(TRACE), encoding="utf-8")

            signature, errors = MOD.COMPARE.replay_signature(candidate_trace)
            self.assertEqual(errors, [])
            manifest = {
                "schema_version": 1,
                "artifacts": [{"path": trace_path, "signature": signature}],
            }
            (canonical_root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

            baseline_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 50.0}
            candidate_perf = {"p95_ms": 102.0, "p99_ms": 123.0, "ops_per_sec": 49.0}
            baseline_perf_path = root / "baseline_perf.json"
            candidate_perf_path = root / "candidate_perf.json"
            baseline_perf_path.write_text(json.dumps(baseline_perf), encoding="utf-8")
            candidate_perf_path.write_text(json.dumps(candidate_perf), encoding="utf-8")

            out_path = root / "report.json"
            code = MOD.main(
                [
                    "--canonical-root",
                    str(canonical_root),
                    "--candidate-root",
                    str(candidate_root),
                    "--baseline-perf",
                    str(baseline_perf_path),
                    "--candidate-perf",
                    str(candidate_perf_path),
                    "--out",
                    str(out_path),
                ]
            )
            self.assertEqual(code, 0)
            report = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "pass")

    def test_gate_fails_on_determinism_and_perf_regression(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            canonical_root = root / "canonical"
            candidate_root = root / "candidate"
            trace_path = "sim/demo/7.json"

            canonical_trace = canonical_root / trace_path
            candidate_trace = candidate_root / trace_path
            canonical_trace.parent.mkdir(parents=True, exist_ok=True)
            candidate_trace.parent.mkdir(parents=True, exist_ok=True)
            canonical_trace.write_text(json.dumps(TRACE), encoding="utf-8")
            drifted = json.loads(json.dumps(TRACE))
            drifted["events"][1]["operation"]["commit_state"] = "ok"
            candidate_trace.write_text(json.dumps(drifted), encoding="utf-8")

            signature, errors = MOD.COMPARE.replay_signature(canonical_trace)
            self.assertEqual(errors, [])
            manifest = {
                "schema_version": 1,
                "artifacts": [{"path": trace_path, "signature": signature}],
            }
            (canonical_root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

            baseline_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 100.0}
            candidate_perf = {"p95_ms": 130.0, "p99_ms": 155.0, "ops_per_sec": 80.0}
            baseline_perf_path = root / "baseline_perf.json"
            candidate_perf_path = root / "candidate_perf.json"
            baseline_perf_path.write_text(json.dumps(baseline_perf), encoding="utf-8")
            candidate_perf_path.write_text(json.dumps(candidate_perf), encoding="utf-8")

            out_path = root / "report.json"
            code = MOD.main(
                [
                    "--canonical-root",
                    str(canonical_root),
                    "--candidate-root",
                    str(candidate_root),
                    "--baseline-perf",
                    str(baseline_perf_path),
                    "--candidate-perf",
                    str(candidate_perf_path),
                    "--max-latency-regression-pct",
                    "10",
                    "--max-throughput-regression-pct",
                    "5",
                    "--out",
                    str(out_path),
                ]
            )
            self.assertEqual(code, 1)
            report = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "fail")
            failure_codes = {failure["code"] for failure in report["failures"]}
            self.assertIn("determinism.mismatch", failure_codes)
            self.assertIn("perf.latency_regression", failure_codes)
            self.assertIn("perf.throughput_regression", failure_codes)

    def test_gate_fails_on_empty_manifest(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            canonical_root = root / "canonical"
            candidate_root = root / "candidate"
            canonical_root.mkdir(parents=True, exist_ok=True)
            candidate_root.mkdir(parents=True, exist_ok=True)

            manifest = {"schema_version": 1, "artifacts": []}
            (canonical_root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

            baseline_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 50.0}
            candidate_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 50.0}
            baseline_perf_path = root / "baseline_perf.json"
            candidate_perf_path = root / "candidate_perf.json"
            baseline_perf_path.write_text(json.dumps(baseline_perf), encoding="utf-8")
            candidate_perf_path.write_text(json.dumps(candidate_perf), encoding="utf-8")

            out_path = root / "report.json"
            code = MOD.main(
                [
                    "--canonical-root",
                    str(canonical_root),
                    "--candidate-root",
                    str(candidate_root),
                    "--baseline-perf",
                    str(baseline_perf_path),
                    "--candidate-perf",
                    str(candidate_perf_path),
                    "--out",
                    str(out_path),
                ]
            )

            self.assertEqual(code, 1)
            report = json.loads(out_path.read_text(encoding="utf-8"))
            failure_codes = {failure["code"] for failure in report["failures"]}
            self.assertIn("determinism.empty_manifest", failure_codes)
            self.assertFalse(report["checks"]["determinism"]["passed"])

    def test_gate_fails_on_invalid_manifest_schema(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            canonical_root = root / "canonical"
            candidate_root = root / "candidate"
            trace_path = "sim/demo/7.json"

            canonical_trace = canonical_root / trace_path
            candidate_trace = candidate_root / trace_path
            canonical_trace.parent.mkdir(parents=True, exist_ok=True)
            candidate_trace.parent.mkdir(parents=True, exist_ok=True)
            canonical_trace.write_text(json.dumps(TRACE), encoding="utf-8")
            candidate_trace.write_text(json.dumps(TRACE), encoding="utf-8")

            signature, errors = MOD.COMPARE.replay_signature(canonical_trace)
            self.assertEqual(errors, [])
            manifest = {
                "schema_version": 99,
                "artifacts": [{"path": trace_path, "signature": signature}],
            }
            (canonical_root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

            baseline_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 50.0}
            candidate_perf = {"p95_ms": 100.0, "p99_ms": 120.0, "ops_per_sec": 50.0}
            baseline_perf_path = root / "baseline_perf.json"
            candidate_perf_path = root / "candidate_perf.json"
            baseline_perf_path.write_text(json.dumps(baseline_perf), encoding="utf-8")
            candidate_perf_path.write_text(json.dumps(candidate_perf), encoding="utf-8")

            out_path = root / "report.json"
            code = MOD.main(
                [
                    "--canonical-root",
                    str(canonical_root),
                    "--candidate-root",
                    str(candidate_root),
                    "--baseline-perf",
                    str(baseline_perf_path),
                    "--candidate-perf",
                    str(candidate_perf_path),
                    "--out",
                    str(out_path),
                ]
            )
            self.assertEqual(code, 1)
            report = json.loads(out_path.read_text(encoding="utf-8"))
            failure_codes = {failure["code"] for failure in report["failures"]}
            self.assertIn("determinism.invalid_manifest_schema", failure_codes)
            self.assertFalse(report["checks"]["determinism"]["passed"])


if __name__ == "__main__":
    unittest.main()
