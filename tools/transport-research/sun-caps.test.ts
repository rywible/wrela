import { describe, expect, test } from "bun:test";
import { normalized } from "./coherent-ggx";
import { capArea, capIntersection, integrateCapReference, visibilityInterval } from "./sun-caps";

describe("compiled finite sun visibility", () => {
  test("spherical lens area and first moment match independent arc integration", () => {
    for (const r1 of [0.008, 0.075, 0.6])
      for (const r2 of [0.00465, 0.04])
        for (const offset of [-0.9, -0.3, 0.1, 0.7, 0.97]) {
          const d = Math.max(0.0001, r1 + r2 * offset),
            center: [number, number, number] = [Math.sin(d), 0, Math.cos(d)];
          const exact = capIntersection(center, r1, [0, 0, 1], r2),
            reference = integrateCapReference(r1, r2, d, 32768);
          const scale = capArea(r2);
          expect(Math.abs(exact.area - reference.area) / scale).toBeLessThan(0.000003);
          for (let j = 0; j < 3; j++)
            expect(Math.abs(exact.vector[j] - reference.vector[j]) / scale).toBeLessThan(0.000003);
          expect(Math.hypot(...exact.vector)).toBeLessThanOrEqual(exact.area + 1e-12);
        }
  });
  test("containment and separated cases are exact", () => {
    expect(capIntersection([0, 0, 1], 0.1, [0, 0, 1], 0.00465).area).toBe(capArea(0.00465));
    expect(capIntersection([1, 0, 0], 0.1, [0, 0, 1], 0.00465).area).toBe(0);
    const n = normalized([0.3, 0.2, 1]);
    const full = capIntersection(n, 0.05, n, 0.1);
    expect(full.vector[0]).toBeCloseTo(Math.PI * Math.sin(0.05) ** 2 * n[0], 12);
  });
  test("inner/outer proxy bounds bracket an intermediate occluder", () => {
    const sun: [number, number, number] = [0, 0, 1],
      r = 0.00465;
    for (let i = 0; i < 100; i++) {
      const d = 0.071 + i * 0.00008,
        c: [number, number, number] = [Math.sin(d), 0, Math.cos(d)];
      const interval = visibilityInterval(sun, r, [{ center: c, inner: 0.074, outer: 0.076 }]);
      const truth = 1 - capIntersection(sun, r, c, 0.075).area / capArea(r);
      expect(truth).toBeGreaterThanOrEqual(interval.lo - 1e-10);
      expect(truth).toBeLessThanOrEqual(interval.hi + 1e-10);
    }
  });
});
