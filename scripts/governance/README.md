# Governance Check Scripts

Primary checker (`WRE-603`, `WRE-610`, `WRE-636`, `WRE-637`):

```bash
python3 scripts/governance/check_g1_governance.py /tmp/linear-issues.json
python3 scripts/governance/check_g1_governance.py /tmp/linear-issues.json \
  --canonical-dag docs/project-governance/canonical-overlay-dag.md \
  --report /tmp/g1-governance-report.md
```

Weekly report wrapper (`WRE-603`):

```bash
scripts/governance/run_weekly_g1_drift.sh /tmp/linear-issues.json
scripts/governance/run_weekly_g1_drift.sh /tmp/linear-issues.json artifacts/governance/weekly-drift
```

`linear-issues.json` should be a Linear export with an `issues` array containing
issue identifier, description, title, blockedBy fields, assignee, and dueDate.

Checks implemented:

- Canonical blockers are parsed from:
  - `docs/project-governance/canonical-overlay-dag.md`
- Dependency drift check for canonical nodes in the DAG doc.
- Explicit phase-overlay assertion for `P10/P11/P12/P13`:
  - `WRE-612`, `WRE-613`, `WRE-614`, `WRE-627`
  - Node presence and dependency-edge presence are required.
- Non-umbrella issue descriptions include the policy sentinel used by `WRE-636`.
- Non-umbrella issues include assignee + due date + at least one dependency edge
  (`WRE-610` completeness rule).
- Explicit DAG exceptions are allowed only when issue description includes
  `G1-DAG-EXCEPTION: <rationale>`.
- Markdown report output links findings to Linear issue pages and includes
  corrective action hints.

Testing:

```bash
python3 -m unittest discover -s scripts/governance/tests -v
```

Injected drift coverage includes at least one mismatch case for each overlay node:
`WRE-612`, `WRE-613`, `WRE-614`, and `WRE-627`.

Additional G2 artifacts:

- `docs/project-governance/gate-registry.json`
- `scripts/governance/evaluate_gates.py`

Authoring guardrail for non-umbrella issues:

- `docs/project-governance/non-umbrella-issue-template.md`
