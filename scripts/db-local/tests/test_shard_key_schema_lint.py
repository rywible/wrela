import json
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "shard_key_schema_lint.py"


class ShardKeySchemaLintTests(unittest.TestCase):
    def run_lint(self, payload, *extra_args):
        with tempfile.NamedTemporaryFile(mode="w+", suffix=".json", delete=False) as f:
            json.dump(payload, f)
            f.flush()
            path = f.name

        cmd = ["python3", str(SCRIPT), path, *extra_args]
        return subprocess.run(cmd, check=False, capture_output=True, text=True)

    def test_passes_composite_shard_key(self):
        payload = {
            "table": "orders",
            "fields": {
                "tenant_id": {"type": "string"},
                "order_id": {"type": "string"},
            },
            "shard_key": {"fields": ["tenant_id", "order_id"]},
        }
        proc = self.run_lint(payload)
        self.assertEqual(proc.returncode, 0)
        self.assertIn("PASS", proc.stdout)

    def test_fails_single_field_without_waiver(self):
        payload = {
            "table": "users",
            "fields": {"tenant_id": {"type": "string"}},
            "shard_key": {"fields": ["tenant_id"]},
        }
        proc = self.run_lint(payload)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("single-field shard key is disallowed", proc.stdout)

    def test_waiver_requires_descriptive_reason(self):
        payload = {
            "table": "audit_log",
            "fields": {"tenant_id": {"type": "string"}},
            "shard_key": {
                "fields": ["tenant_id"],
                "allow_single_shard_key": {"reason": "ok"},
            },
        }
        proc = self.run_lint(payload)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("waiver_reason_required", proc.stdout)

    def test_low_cardinality_boolean_fails_in_strict_mode(self):
        payload = {
            "table": "flags",
            "fields": {
                "tenant_id": {"type": "string"},
                "is_active": {"type": "bool"},
            },
            "shard_key": {"fields": ["tenant_id", "is_active"]},
        }
        proc = self.run_lint(payload, "--strict-low-cardinality")
        self.assertEqual(proc.returncode, 1)
        self.assertIn("low_cardinality", proc.stdout)

    def test_tiny_enum_warns_by_default(self):
        payload = {
            "table": "events",
            "fields": {
                "tenant_id": {"type": "string"},
                "event_type": {"type": "enum", "variants": ["a", "b", "c"]},
            },
            "shard_key": {"fields": ["tenant_id", "event_type"]},
        }
        proc = self.run_lint(payload)
        self.assertEqual(proc.returncode, 0)
        self.assertIn("warning", proc.stdout)

    def test_json_output_includes_waiver_metadata(self):
        payload = {
            "table": "audit_log",
            "fields": {"tenant_id": {"type": "string"}},
            "shard_key": {
                "fields": ["tenant_id"],
                "allow_single_shard_key": {
                    "reason": "tenant-isolated deployment with bounded load profile"
                },
            },
        }
        proc = self.run_lint(payload, "--format", "json")
        self.assertEqual(proc.returncode, 0)
        body = json.loads(proc.stdout)
        self.assertEqual(body["status"], "pass")
        self.assertIsNotNone(body["waiver"])
        self.assertIn("reason", body["waiver"])


if __name__ == "__main__":
    unittest.main()
