#!/usr/bin/env python3
"""
Utility checks for WRE-603 / WRE-610 / WRE-636 / WRE-637 governance checks.

Inputs:
  - JSON export of Linear issues (list under top-level "issues").
  - Canonical DAG markdown doc (`docs/project-governance/canonical-overlay-dag.md`)
    used as the source of truth for expected blockers.

Checks:
  - Dependency drift: issue `blockedBy` edges must match canonical DAG edges.
  - Phase overlay guardrail: explicitly assert P10/P11/P12/P13 nodes and edges
    (`WRE-612`, `WRE-613`, `WRE-614`, `WRE-627`) are present in canonical DAG and live data.
  - Policy block presence in non-umbrella issue descriptions.
  - Completeness checks for non-umbrella issues (assignee + due date + dependency edge).

This script is intentionally dependency-free so it can be dropped into CI/scheduled jobs.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
from dataclasses import dataclass

POLICY_SENTINEL = (
    "Implement as much as possible in native Wrela (`language/packages/db/*`)."
)
EXCEPTION_SENTINEL = "G1-DAG-EXCEPTION:"
RE_ISSUE_HEADER = re.compile(r"^- `(?P<issue>WRE-\d+)`.*depends on:")
RE_DEP = re.compile(r"^  - `(?P<dep>WRE-\d+)`")
PHASE_OVERLAY_IDS = ("WRE-612", "WRE-613", "WRE-614", "WRE-627")


@dataclass(frozen=True)
class Finding:
    identifier: str
    reason: str


def parse_canonical_blockers(canonical_dag_path: pathlib.Path) -> dict[str, set[str]]:
    """Parse expected blockers from canonical overlay DAG markdown."""
    blockers: dict[str, set[str]] = {}
    current_issue: str | None = None

    for raw in canonical_dag_path.read_text(encoding="utf-8").splitlines():
        issue_match = RE_ISSUE_HEADER.match(raw)
        if issue_match:
            current_issue = issue_match.group("issue")
            blockers.setdefault(current_issue, set())
            continue

        dep_match = RE_DEP.match(raw)
        if dep_match and current_issue:
            blockers[current_issue].add(dep_match.group("dep"))
            continue

        # End dependency collection when indentation structure ends.
        if current_issue and raw and not raw.startswith("  - "):
            current_issue = None

    return blockers


def build_issue_indexes(issues: list[dict]) -> tuple[dict[str, dict], dict[str, str]]:
    by_identifier = {}
    id_to_identifier = {}
    for item in issues:
        if not isinstance(item, dict):
            continue
        identifier = item.get("identifier")
        issue_id = item.get("id")
        if identifier:
            by_identifier[identifier] = item
        if issue_id and identifier:
            id_to_identifier[issue_id] = identifier
    return by_identifier, id_to_identifier


def to_blocked_identifiers(
    blocked_by: list | None,
    id_to_identifier: dict[str, str],
) -> set[str]:
    out = set()
    for item in blocked_by or []:
        if isinstance(item, str):
            out.add(id_to_identifier.get(item, item))
            continue
        if isinstance(item, dict):
            identifier = item.get("identifier")
            issue_id = item.get("id")
            if identifier:
                out.add(identifier)
            elif issue_id:
                out.add(id_to_identifier.get(issue_id, issue_id))
    return {x for x in out if x}


def is_umbrella_or_control(title: str) -> bool:
    return (
        "Umbrella" in title
        or title.startswith("G1-")
        or title.startswith("G2-")
        or title.startswith("Phase ")
    )


def run_checks(issues_payload: dict, canonical_blockers: dict[str, set[str]]) -> list[Finding]:
    issues = issues_payload.get("issues", [])
    by_identifier, id_to_identifier = build_issue_indexes(issues)
    findings: list[Finding] = []

    def fail(identifier: str, reason: str) -> None:
        findings.append(Finding(identifier=identifier, reason=reason))

    if not canonical_blockers:
        fail("CANONICAL", "no canonical blockers parsed from DAG document")
        return findings

    # Drift check for all canonical issues.
    for issue_id, expected in canonical_blockers.items():
        issue = by_identifier.get(issue_id)
        if not issue:
            fail(issue_id, "missing from input")
            continue

        blocked = to_blocked_identifiers(issue.get("blockedBy"), id_to_identifier)
        missing = sorted(expected - blocked)
        description = issue.get("description") or ""
        if missing and EXCEPTION_SENTINEL not in description:
            fail(issue_id, f"missing blockers: {', '.join(missing)}")

    # Explicit acceptance gate for P10/P11/P12/P13 overlay nodes and edges.
    for issue_id in PHASE_OVERLAY_IDS:
        expected = canonical_blockers.get(issue_id)
        if not expected:
            fail(issue_id, "missing canonical P10/P11/P12/P13 edge definition")
            continue

        issue = by_identifier.get(issue_id)
        if not issue:
            fail(issue_id, "missing overlay issue in input")
            continue

        blocked = to_blocked_identifiers(issue.get("blockedBy"), id_to_identifier)
        if not blocked:
            fail(issue_id, "overlay issue has no dependency edges")

    # WRE-610 completeness and WRE-636 policy checks for non-umbrella issues.
    for identifier, issue in by_identifier.items():
        title = issue.get("title", "")
        if is_umbrella_or_control(title):
            continue

        description = issue.get("description") or ""
        if POLICY_SENTINEL not in description:
            fail(identifier, "missing policy block")

        assignee = issue.get("assignee")
        if not assignee:
            fail(identifier, "missing assignee")

        due_date = issue.get("dueDate")
        if not due_date:
            fail(identifier, "missing due date")

        blocked = to_blocked_identifiers(issue.get("blockedBy"), id_to_identifier)
        if not blocked:
            fail(identifier, "missing dependency edge")

    return findings


def write_report(report_file, findings: list[Finding]) -> None:
    report_file.write("# G1 Governance Drift Report\n\n")
    if not findings:
        report_file.write("- Status: PASS\n")
        return

    report_file.write("- Status: FAIL\n\n")
    report_file.write("## Findings\n\n")
    report_file.write("Each finding includes a direct issue link and corrective action hint.\n\n")
    for finding in findings:
        report_file.write(
            f"- `{finding.identifier}`: {finding.reason} "
            f"([open](https://linear.app/wrela/issue/{finding.identifier})) "
            f"[action: update blockedBy/description/owner/dueDate]\n"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("issues_json", type=argparse.FileType("r"))
    parser.add_argument(
        "--canonical-dag",
        default="docs/project-governance/canonical-overlay-dag.md",
        help="path to canonical DAG markdown file",
    )
    parser.add_argument("--report", type=argparse.FileType("w"))
    args = parser.parse_args()

    payload = json.load(args.issues_json)
    canonical_path = pathlib.Path(args.canonical_dag)
    canonical_blockers = parse_canonical_blockers(canonical_path)
    findings = run_checks(payload, canonical_blockers)

    for finding in findings:
        print(f"ERROR: {finding.identifier} {finding.reason}")

    if args.report:
        write_report(args.report, findings)

    if findings:
        print(f"Governance check failed: {len(findings)} failure(s)")
        return 1

    print("Governance checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
