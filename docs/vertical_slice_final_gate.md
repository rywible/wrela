# Vertical Slice Final Gate

`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

## Final AC Evidence

1. Launch command:
   1. `cargo run -p wrela -- game run apps/wrela-game-slice`
2. Determinism and rollback:
   1. `cargo run -p wrela -- game check apps/wrela-game-slice --determinism --rollback`
3. Browser smoke:
   1. `scripts/vertical_slice/browser_smoke.sh`

## Artifact Links

1. `.artifacts/vertical-slice/WFE-601/test-matrix.json`
2. `.artifacts/vertical-slice/WFE-602/smoke/smoke-report.json`
3. `.artifacts/vertical-slice/WFE-602/smoke/vertical-slice-smoke.png`
4. `.artifacts/vertical-slice/WFE-799/final-gate/summary.md`
