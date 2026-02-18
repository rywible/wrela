import json
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "shard_skew_preflight.py"


class ShardSkewPreflightTests(unittest.TestCase):
    def run_gate(self, payload, *extra_args):
        with tempfile.NamedTemporaryFile(mode="w+", suffix=".json", delete=False) as f:
            json.dump(payload, f)
            f.flush()
            path = f.name

        cmd = ["python3", str(SCRIPT), path, *extra_args]
        return subprocess.run(cmd, check=False, capture_output=True, text=True)

    def test_passes_balanced_distribution(self):
        payload = {
            "profile": "global-3x",
            "shards": {"a": 1000, "b": 980, "c": 1020},
        }
        proc = self.run_gate(payload)
        self.assertEqual(proc.returncode, 0)
        self.assertIn("PASS", proc.stdout)

    def test_fails_skewed_distribution(self):
        payload = {
            "profile": "global-3x",
            "shards": {"a": 2300, "b": 300, "c": 200},
        }
        proc = self.run_gate(payload)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("skew ratio", proc.stdout)
        self.assertIn("tenant_id + entity_id", proc.stdout)

    def test_fails_when_shard_count_below_minimum(self):
        payload = {
            "profile": "us-3",
            "shards": {"a": 1200, "b": 1200},
        }
        proc = self.run_gate(payload)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("insufficient shard count", proc.stdout)

    def test_json_output_is_machine_readable(self):
        payload = {
            "profile": "global-3x",
            "shards": {"a": 100, "b": 100, "c": 100},
        }
        proc = self.run_gate(payload, "--format", "json")
        self.assertEqual(proc.returncode, 0)
        body = json.loads(proc.stdout)
        self.assertEqual(body["status"], "pass")
        self.assertIn("max_over_mean_ratio", body)


if __name__ == "__main__":
    unittest.main()
