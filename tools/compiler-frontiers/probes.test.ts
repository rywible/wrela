import { expect, test } from "bun:test";
import { evaluatePhasePolynomial } from "@wrela/compiler";
import { normalize, type Vec3 } from "@wrela/model";

import { compileFourier, correlatedExpression } from "./fourier";
import { dot, excludeLight, lightFixture, random } from "./probes";

test("normal cone exclusion contains random position and normal perturbations", () => {
  const rng = random(917);
  let excluded = 0;
  for (let i = 0; i < 1000; i++) {
    const angle = rng() * 0.9,
      radius = rng() * 3;
    const domain = {
      center: [0, 0, 0] as Vec3,
      radius,
      axis: [0, 1, 0] as Vec3,
      halfAngle: angle,
      oneSided: true,
    };
    const light: Vec3 = [(rng() - 0.5) * 30, (rng() - 0.5) * 30, (rng() - 0.5) * 30];
    if (!excludeLight(domain, light)) continue;
    excluded++;
    for (let j = 0; j < 64; j++) {
      const a = rng() * angle,
        phi = rng() * Math.PI * 2;
      const n = [Math.sin(a) * Math.cos(phi), Math.cos(a), Math.sin(a) * Math.sin(phi)];
      const p = normalize([rng() - 0.5, rng() - 0.5, rng() - 0.5]).map((v) => v * radius * rng());
      expect(
        dot(
          n,
          light.map((v, k) => v - p[k]),
        ),
      ).toBeLessThanOrEqual(0);
    }
  }
  expect(excluded).toBeGreaterThan(50);
});
test("transmission, uncertain motion and enclosed light retain the generic path", () => {
  const domain = {
    center: [0, 0, 0] as Vec3,
    radius: 1,
    axis: [0, 1, 0] as Vec3,
    halfAngle: 0.2,
    oneSided: true,
  };
  expect(excludeLight(domain, [0, -10, 0])).toBe(true);
  expect(excludeLight({ ...domain, oneSided: false }, [0, -10, 0])).toBe(false);
  expect(excludeLight(domain, [0, -10, 0], 1.5)).toBe(false);
  expect(excludeLight(domain, [0, -0.5, 0])).toBe(false);
});
test("world-space bounds do not falsely prune the actual winter terrain samples", () => {
  for (const kind of ["mixed", "above"] as const) expect(lightFixture(kind).summary.falseExclusions).toBe(0);
});

test("symbolic products retain correlated DC energy and reproduce source point evaluations", () => {
  const program = compileFourier(correlatedExpression, 2);
  const rng = random(841);
  expect(program.terms).toHaveLength(4);
  for (let i = 0; i < 200; i++) {
    const p = rng() * 8,
      d = rng() * 8;
    const value = evaluatePhasePolynomial(program, {
      origin: [p, d],
      dx: [0, 0],
      dy: [0, 0],
      shutter: [0, 0],
    }).value;
    expect(value).toBeCloseTo((0.6 + 0.35 * Math.cos(p)) * (0.5 + 0.45 * Math.cos(p + d)), 12);
  }
  const correlated = compileFourier(
    {
      kind: "multiply",
      left: { kind: "cosine", mode: [1], phase: 0 },
      right: { kind: "cosine", mode: [1], phase: 0 },
    },
    1,
  );
  expect(
    evaluatePhasePolynomial(correlated, { origin: [0.4], dx: [Math.PI * 2], dy: [0], shutter: [0] }).value,
  ).toBeCloseTo(0.5, 12);
});
test("phase offsets, negative carriers and cancellation preserve algebra", () => {
  const p = compileFourier(
    {
      kind: "add",
      left: { kind: "cosine", mode: [-2, 3], phase: 0.8 },
      right: { kind: "constant", value: -0.7 },
    },
    2,
  );
  expect(
    evaluatePhasePolynomial(p, { origin: [0.3, 0.2], dx: [0, 0], dy: [0, 0], shutter: [0, 0] }).value,
  ).toBeCloseTo(Math.cos(0.8) - 0.7, 12);
  expect(() => compileFourier({ kind: "cosine", mode: [0.5], phase: 0 }, 1)).toThrow();
});
