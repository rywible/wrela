import importlib.util
import pathlib
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "scale" / "run_scale.py"
SPEC = importlib.util.spec_from_file_location("run_scale", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC is not None and SPEC.loader is not None
SPEC.loader.exec_module(MOD)


class ScaleReportTests(unittest.TestCase):
    def test_capacity_points_are_sorted_and_positive(self):
        summary = {
            "us": {"availability": 0.99, "avg_rtt_ms": 8.0},
            "eu": {"availability": 0.98, "avg_rtt_ms": 12.0},
            "ap": {"availability": 0.97, "avg_rtt_ms": 20.0},
        }
        report = MOD.compute_capacity(summary, [8, 16], [3, 6])
        self.assertGreater(report["avg_availability"], 0)
        self.assertEqual(report["points"][0]["shards"], 8)
        self.assertTrue(all(p["estimated_tps"] > 0 for p in report["points"]))


if __name__ == "__main__":
    unittest.main()
