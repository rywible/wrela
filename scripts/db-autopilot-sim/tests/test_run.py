import json
import pathlib
import tempfile
import unittest
import importlib.util

MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "run.py"
SPEC = importlib.util.spec_from_file_location("autopilot_run", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class ReplayHarnessTests(unittest.TestCase):
    def test_deterministic_report_and_failure_detection(self):
        payload = {
            "scenarios": [
                {
                    "scenario_id": "balanced",
                    "max_to_mean_ratio": 1.2,
                    "skew_threshold": 1.5,
                    "survivable_additional_failures": 1,
                    "required_additional_failures": 1,
                    "degraded_selected": 0,
                    "max_degraded_selected": 1,
                },
                {
                    "scenario_id": "hotspot",
                    "max_to_mean_ratio": 2.9,
                    "skew_threshold": 1.5,
                    "survivable_additional_failures": 0,
                    "required_additional_failures": 1,
                    "degraded_selected": 2,
                    "max_degraded_selected": 0,
                },
            ]
        }

        with tempfile.TemporaryDirectory() as tmp:
            inp = pathlib.Path(tmp) / "in.json"
            out_a = pathlib.Path(tmp) / "out-a.json"
            out_b = pathlib.Path(tmp) / "out-b.json"
            inp.write_text(json.dumps(payload))

            report_a = MOD.run(inp, out_a)
            report_b = MOD.run(inp, out_b)

            self.assertEqual(report_a, report_b)
            self.assertFalse(report_a["all_passed"])

            hotspot = next(row for row in report_a["scenarios"] if row["scenario_id"] == "hotspot")
            self.assertFalse(hotspot["passed"])
            self.assertTrue(any("skew ratio" in reason for reason in hotspot["reasons"]))


if __name__ == "__main__":
    unittest.main()
