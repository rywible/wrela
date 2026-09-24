import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { compileWaterPhases } from "./phase";
import { glintPhases, integrateGlintRectangle } from "./water-glints";

test("compiled two-wave inverse finds the authored target slopes and rejects impossible glints", () => {
  const water = referenceProject().documents.find((d) => d.kind === "water");
  if (water?.kind !== "water") throw Error("missing water");
  const p = compileWaterPhases(water);
  expect(p.glints).toBeDefined();
  if (!p.glints) throw Error("Missing compiled glints");
  const inverse = p.glints.inverseSlope;
  for (const phases of [
    [0.3, 1.4],
    [2.3, -0.5],
    [-1.2, 2.8],
  ]) {
    const slope = p.carriers.reduce(
      (sum, c, i) =>
        [sum[0] + c.slope[0] * Math.cos(phases[i]), sum[1] + c.slope[1] * Math.cos(phases[i])] as [
          number,
          number,
        ],
      [0, 0] as [number, number],
    );
    for (const found of glintPhases(inverse, slope))
      for (let axis = 0; axis < 2; axis++)
        expect(p.carriers.reduce((v, c, i) => v + c.slope[axis] * Math.cos(found[i]), 0)).toBeCloseTo(
          slope[axis],
          12,
        );
  }
  expect(glintPhases(inverse, [100, 100])).toEqual([]);
  expect(compileWaterPhases({ ...water, waves: [water.waves[0]] }).glints).toBeUndefined();
});
test("boundary integration preserves narrow peak energy including rotated and reversed footprints", () => {
  for (const center of [
    [0, 0],
    [0.03, 0.02],
    [0.12, -0.07],
  ] as [number, number][])
    for (const dx of [
      [0.1, 0.02],
      [-0.1, -0.02],
    ] as [number, number][]) {
      const dy: [number, number] = [0.01, 0.09],
        delta = 0.0004,
        steps = 512;
      let reference = 0;
      for (let y = 0; y < steps; y++)
        for (let x = 0; x < steps; x++) {
          const u = (x + 0.5) / steps - 0.5,
            v = (y + 0.5) / steps - 0.5;
          reference +=
            1 /
            (delta + (center[0] + dx[0] * u + dy[0] * v) ** 2 + (center[1] + dx[1] * u + dy[1] * v) ** 2) **
              2;
        }
      reference /= steps * steps;
      const analytic = integrateGlintRectangle(center, dx, dy, delta);
      if (analytic === null) throw Error("Unexpected degenerate footprint");
      expect(Math.abs(analytic - reference) / reference).toBeLessThan(0.00005);
    }
  expect(integrateGlintRectangle([0, 0], [0, 0], [0, 0], 1)).toBeNull();
});
