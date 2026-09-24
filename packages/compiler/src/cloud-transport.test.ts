import { describe, expect, test } from "bun:test";
import { CLOUD_DOMAIN, compileCloudAdvection, projectCloudDepthInterval } from "./cloud-transport";

describe("compiled cloud correspondence", () => {
  test("advects the exact input of arbitrary downstream warps under changed wind and time", () => {
    for (const domain of [
      CLOUD_DOMAIN,
      { worldScale: [2, -3, 0.4] as const, windScale: [-0.8, 1.2, -2] as const },
    ]) {
      const { motion } = compileCloudAdvection(domain);
      for (let i = 0; i < 100; i++) {
        const point = [i * 12 - 500, 1100 + i * 9, -i * i];
        const before = [0.7 * i, 0.2 * i, -0.4 * i],
          after = [-0.1 * i, 0.4 * i, 0.9 * i];
        for (let axis = 0; axis < 3; axis++) {
          const moved = point[axis] + motion[axis] * (after[axis] - before[axis]);
          const a = point[axis] * domain.worldScale[axis] + before[axis] * domain.windScale[axis];
          const b = moved * domain.worldScale[axis] + after[axis] * domain.windScale[axis];
          expect(b).toBeCloseTo(a, 10);
        }
      }
    }
  });
  test("projective endpoints enclose every interior depth under oblique motion", () => {
    for (let i = 0; i < 80; i++) {
      const origin: [number, number, number] = [80 * Math.sin(i), 30 * Math.cos(i), i - 50];
      const ray: [number, number, number] = [Math.sin(i * 0.1), Math.cos(i * 0.3), 1];
      const basis = [1, 0, 0, 0, 1, 0, 0, 0, 1] as const;
      const interval = projectCloudDepthInterval(origin, ray, [100, 5000], basis);
      expect(interval).not.toBeNull();
      if (!interval) throw Error("Expected a finite projected interval");
      for (let k = 0; k <= 100; k++) {
        const depth = 100 + 49 * k;
        for (let axis = 0; axis < 2; axis++) {
          const value = (origin[axis] + ray[axis] * depth) / (origin[2] + depth);
          expect(value).toBeGreaterThanOrEqual(interval.min[axis] - 1e-12);
          expect(value).toBeLessThanOrEqual(interval.max[axis] + 1e-12);
        }
      }
    }
  });
  test("rejects singular domains and intervals crossing the camera plane", () => {
    expect(() => compileCloudAdvection({ worldScale: [1, 0, 1], windScale: [1, 1, 1] })).toThrow();
    expect(projectCloudDepthInterval([0, 0, -5], [0, 0, 1], [0, 10], [1, 0, 0, 0, 1, 0, 0, 0, 1])).toBeNull();
  });
});
