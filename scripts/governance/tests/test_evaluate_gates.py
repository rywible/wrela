import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest


MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "evaluate_gates.py"
SPEC = importlib.util.spec_from_file_location("evaluate_gates", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MOD
SPEC.loader.exec_module(MOD)


class EvaluateGatesTests(unittest.TestCase):
    def test_gate_passes_with_all_required_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            registry = root / "registry.json"
            report = root / "report.json"
            registry.write_text(
                json.dumps(
                    {
                        "gates": {"latency.p99.ms": {"target": 250}},
                        "required_artifacts": [
                            "slo-burn-rate-alerts.json",
                            "incident-command-runbook.md",
                            "incident-drill-artifacts.json",
                        ],
                    }
                ),
                encoding="utf-8",
            )
            report.write_text(
                json.dumps(
                    {
                        "latency.p99.ms": 200,
                        "artifacts": [
                            "slo-burn-rate-alerts.json",
                            "incident-command-runbook.md",
                            "incident-drill-artifacts.json",
                        ],
                    }
                ),
                encoding="utf-8",
            )

            code = MOD.main(["--registry", str(registry), "--report", str(report)])
            self.assertEqual(code, 0)

    def test_gate_fails_when_runbook_artifact_missing(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            registry = root / "registry.json"
            report = root / "report.json"
            registry.write_text(
                json.dumps(
                    {
                        "gates": {"latency.p99.ms": {"target": 250}},
                        "required_artifacts": [
                            "slo-burn-rate-alerts.json",
                            "incident-command-runbook.md",
                        ],
                    }
                ),
                encoding="utf-8",
            )
            report.write_text(
                json.dumps(
                    {
                        "latency.p99.ms": 200,
                        "artifacts": ["slo-burn-rate-alerts.json"],
                    }
                ),
                encoding="utf-8",
            )

            code = MOD.main(["--registry", str(registry), "--report", str(report)])
            self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()
