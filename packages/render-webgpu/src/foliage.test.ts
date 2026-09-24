import { expect, test } from "bun:test";
import { FOLIAGE_DIFFUSE_BUDGET, type FoliageOptics, foliageBudget, foliageDiffuse } from "./foliage";

const white: FoliageOptics = { albedo: [1, 1, 1], scatterColor: [1, 1, 1], transmission: 1, thickness: 0 };
test("leaf diffuse reflection and transmission share a per-channel energy budget", () => {
  for (const transmission of [0, 0.1, 0.5, 1])
    for (const thickness of [0, 0.0001, 0.001, 0.01, 1]) {
      const optics: FoliageOptics = {
        ...white,
        albedo: [0.15, 0.75, 1],
        scatterColor: [0.4, 0.8, 1],
        transmission,
        thickness,
      };
      const budget = foliageBudget(optics);
      for (let axis = 0; axis < 3; axis++) {
        expect(budget.reflection[axis] + budget.transmission[axis]).toBeLessThanOrEqual(
          optics.albedo[axis] * FOLIAGE_DIFFUSE_BUDGET + 1e-12,
        );
        expect(budget.reflection[axis]).toBeGreaterThanOrEqual(0);
        expect(budget.transmission[axis]).toBeGreaterThanOrEqual(0);
      }
    }
});
test("fully opaque and fully transmissive thin limits exchange hemispheres, never double the albedo", () => {
  const transparent = foliageBudget(white);
  const opaque = foliageBudget({ ...white, transmission: 0 });
  for (const cosine of [0.001, 0.1, 0.5, 1]) {
    expect(foliageDiffuse(transparent, cosine, 1)).toEqual([0, 0, 0]);
    expect(foliageDiffuse(opaque, -cosine, 1)).toEqual([0, 0, 0]);
    expect(foliageDiffuse(transparent, -cosine, 1)).toEqual(foliageDiffuse(opaque, cosine, 1));
    expect(foliageDiffuse(transparent, -cosine, 0)).toEqual([0, 0, 0]);
  }
});
test("increasing thickness reduces transmission and returns its share to reflection", () => {
  const thin = foliageBudget({ ...white, thickness: 0.0005 });
  const thick = foliageBudget({ ...white, thickness: 0.01 });
  expect(thin.transmission[1]).toBeGreaterThan(thick.transmission[1]);
  expect(thin.reflection[1]).toBeLessThan(thick.reflection[1]);
  expect(thin.reflection[1] + thin.transmission[1]).toBeCloseTo(
    thick.reflection[1] + thick.transmission[1],
    12,
  );
});
