import { expect, test } from "bun:test";
import { runCreatureExperiments } from "./creature-review";

test("both body plans retain chart intent and transaction history through a public candidate edit", () => {
  for (const id of ["ash-warden", "reed-penitent"] as const) {
    const report = runCreatureExperiments(id);
    expect(report.E1.status).toBe("passed-source-correspondence");
    expect(report.E1.strictHeadPreservation.status).toBe(id === "reed-penitent" ? "rejected" : "supported");
    expect(report.E1.topologyChange.every((anchor) => anchor.status === "invalid")).toBe(true);
    expect(report.E1.anchorResults.some((anchor) => (anchor.displacement ?? 0) > 0.05)).toBe(true);
    expect(report.E7.restoredExactly).toBe(true);
    expect(report.E1.mountedComponents.every((component) => component.dimensionsPreserved)).toBe(true);
    expect(report.E1.mountedComponents.some((component) => component.displacement > 0.01)).toBe(true);
    expect(report.E3.scenarios.every((scenario) => scenario.status === "passed")).toBe(true);
    expect(report.E3.meanDisallowedShoulderInfluence.anatomical).toBe(0);
    expect(report.E3.meanDisallowedShoulderInfluence.envelope).toBeGreaterThan(0.1);
    expect(
      report.E3.poses.every(
        (pose) =>
          pose.unrelatedLimbShoulderDisplacement.anatomical === 0 && pose.outsideCorrectiveDisplacement === 0,
      ),
    ).toBe(true);
    expect(report.E3.poses[0].correctiveDisplacement).toBeGreaterThan(0);
    expect(report.E4.details.every((detail) => detail.nestedInPrior && detail.stableRootIdentities)).toBe(
      true,
    );
    expect(report.E4.noGroom.bytes).toBe(0);
    if (report.E4.alternate) {
      expect(report.E4.alternate.sameRootCoordinates).toBe(true);
      expect(report.E4.alternate.sameCanonicalGuideCurves).toBe(true);
      expect(report.E4.alternate.details[0].cost.triangles).toBeLessThan(report.E4.details[0].cost.triangles);
    }
    expect(report.E5.status).toBe("converged");
    expect(report.E5.evaluations).toBeLessThanOrEqual(64);
    expect(report.E5.residuals.every((residual) => residual.satisfied)).toBe(true);
    expect(report.E5.acceptedSourceUnchanged).toBe(true);
    expect(report.E7.reuse.recipeMatches.fieldWidth && report.E7.reuse.recipeMatches.chartWidth).toBe(true);
    expect(
      report.E7.reuse.motionChanged && report.E7.reuse.groomChanged && report.E7.reuse.undoRestoredExactly,
    ).toBe(true);
    expect(report.E2.generatedVertices).toBeGreaterThan(100);
    expect(report.E2.diagnostics.filter((diagnostic) => diagnostic.severity === "error")).toEqual([]);
    expect(report.visualApproval).toBe("not-reviewed");
    expect(report.hardware.status).toBe("not-run");
  }
}, 15_000);
