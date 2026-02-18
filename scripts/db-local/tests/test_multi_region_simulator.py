import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "regions" / "simulate_multi_region.py"
SPEC = importlib.util.spec_from_file_location("simulate_multi_region", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC is not None and SPEC.loader is not None
sys.modules[SPEC.name] = MOD
SPEC.loader.exec_module(MOD)


class MultiRegionSimulatorTests(unittest.TestCase):
    def test_simulation_is_deterministic_given_seed(self):
        regions = [
            MOD.RegionConfig("us", 8, 0.01),
            MOD.RegionConfig("eu", 12, 0.02),
        ]
        a = MOD.run_simulation(regions, ticks=10, seed=11)
        b = MOD.run_simulation(regions, ticks=10, seed=11)
        self.assertEqual(a, b)

    def test_cli_writes_report(self):
        payload = {
            "regions": [
                {"name": "us", "base_rtt_ms": 8, "failure_probability": 0.0},
                {"name": "eu", "base_rtt_ms": 12, "failure_probability": 0.0},
            ]
        }
        with tempfile.TemporaryDirectory() as td:
            input_path = pathlib.Path(td) / "regions.json"
            out_path = pathlib.Path(td) / "report.json"
            input_path.write_text(json.dumps(payload), encoding="utf-8")
            args = MOD.parse_args.__globals__["argparse"].Namespace(
                regions=str(input_path), ticks=5, seed=7, out=str(out_path)
            )
            MOD.parse_args = lambda: args
            rc = MOD.main()
            self.assertEqual(rc, 0)
            report = json.loads(out_path.read_text(encoding="utf-8"))
            self.assertIn("summary", report)


if __name__ == "__main__":
    unittest.main()
