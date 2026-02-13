import importlib.util
import pathlib
import sys
import tempfile
import unittest


MODULE_PATH = (
    pathlib.Path(__file__).resolve().parents[1] / "check_g1_governance.py"
)
SPEC = importlib.util.spec_from_file_location("check_g1_governance", MODULE_PATH)
MOD = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MOD
SPEC.loader.exec_module(MOD)


def build_issue(identifier: str, blocked_by: set[str], title: str = "Phase Node") -> dict:
    return {
        "id": f"id-{identifier}",
        "identifier": identifier,
        "title": title,
        "description": "",
        "blockedBy": [{"identifier": dep} for dep in sorted(blocked_by)],
        "assignee": "Ryan Wible",
        "dueDate": "2026-02-20",
    }


class GovernanceChecksTest(unittest.TestCase):
    def setUp(self) -> None:
        self.repo_root = pathlib.Path(__file__).resolve().parents[3]
        self.canonical_path = (
            self.repo_root / "docs" / "project-governance" / "canonical-overlay-dag.md"
        )
        self.canonical = MOD.parse_canonical_blockers(self.canonical_path)

    def build_baseline_payload(self) -> dict:
        issues = []
        for issue_id, blockers in self.canonical.items():
            title = "Phase Overlay" if issue_id.startswith("WRE-6") else "Umbrella"
            issues.append(build_issue(issue_id, blockers, title=title))

        # Add one non-umbrella sample so completeness/policy checks are exercised.
        issues.append(
            {
                "id": "id-WRE-999",
                "identifier": "WRE-999",
                "title": "P9-1: Sample execution task",
                "description": MOD.POLICY_SENTINEL,
                "blockedBy": [{"identifier": "WRE-592"}],
                "assignee": "Ryan Wible",
                "dueDate": "2026-03-01",
            }
        )
        return {"issues": issues}

    def test_passes_with_matching_canonical_edges(self):
        findings = MOD.run_checks(self.build_baseline_payload(), self.canonical)
        self.assertEqual(findings, [])

    def test_injected_phase_overlay_mismatch_is_reported_for_each_overlay(self):
        for overlay in MOD.PHASE_OVERLAY_IDS:
            with self.subTest(overlay=overlay):
                payload = self.build_baseline_payload()
                issues = payload["issues"]
                for issue in issues:
                    if issue["identifier"] == overlay:
                        issue["blockedBy"] = []
                        break

                findings = MOD.run_checks(payload, self.canonical)
                reasons = [f.reason for f in findings if f.identifier == overlay]
                self.assertTrue(
                    any("missing blockers" in reason or "no dependency edges" in reason for reason in reasons),
                    f"expected overlay finding for {overlay}, got: {reasons}",
                )

    def test_report_includes_issue_links_and_action_hints(self):
        findings = [MOD.Finding("WRE-612", "missing blockers: WRE-593")]
        with tempfile.NamedTemporaryFile(mode="w+", delete=False) as report_file:
            MOD.write_report(report_file, findings)
            report_file.flush()
            report_path = pathlib.Path(report_file.name)

        text = report_path.read_text(encoding="utf-8")
        self.assertIn("https://linear.app/wrela/issue/WRE-612", text)
        self.assertIn("action: update blockedBy/description/owner/dueDate", text)


if __name__ == "__main__":
    unittest.main()
