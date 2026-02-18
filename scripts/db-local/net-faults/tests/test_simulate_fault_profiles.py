import importlib.util
import pathlib
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parents[1] / "simulate_fault_profiles.py"
SPEC = importlib.util.spec_from_file_location("net_fault_profiles", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class NetFaultProfileTests(unittest.TestCase):
    def test_report_is_deterministic(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_a = pathlib.Path(tmp) / "a.json"
            out_b = pathlib.Path(tmp) / "b.json"
            rep_a = MOD.run(out_a)
            rep_b = MOD.run(out_b)

            self.assertEqual(rep_a, rep_b)
            self.assertEqual(len(rep_a["profiles"]), 3)
            failing = [row for row in rep_a["profiles"] if not row["passes_slo"]]
            self.assertTrue(any(row["profile"] == "partition-ish" for row in failing))


if __name__ == "__main__":
    unittest.main()
