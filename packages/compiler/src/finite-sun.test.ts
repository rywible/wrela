import { expect, test } from "bun:test";
import {
  evaluateSphereSun,
  proxySunVisibility,
  sphericalCapArea,
  sphericalCapIntersection,
} from "./finite-sun";

// Independent equal-solid-angle azimuthal arc quadrature around the sun.
function reference(blockerRadius: number, sunRadius: number, separation: number) {
  const steps = 32768,
    height = 2 * Math.sin(sunRadius / 2) ** 2;
  let area = 0,
    x = 0,
    z = 0;
  for (let i = 0; i < steps; i++) {
    const h = (height * (i + 0.5)) / steps,
      cz = 1 - h,
      sz = Math.sqrt(h * (2 - h));
    const cut = (Math.cos(blockerRadius) - Math.cos(separation) * cz) / (Math.sin(separation) * sz);
    const angle = Math.acos(Math.max(-1, Math.min(1, cut)));
    area += 2 * angle;
    x += 2 * sz * Math.sin(angle);
    z += 2 * cz * angle;
  }
  return [area, x, 0, z].map((v) => (v * height) / steps);
}
test("production spherical moments agree with independent cap quadrature across penumbrae", () => {
  for (const radius of [0.008, 0.075, 0.6])
    for (const sunRadius of [0.00465, 0.04])
      for (const offset of [-0.9, -0.3, 0.1, 0.7, 0.97]) {
        const separation = Math.max(0.0001, radius + offset * sunRadius);
        const result = sphericalCapIntersection(
          [Math.sin(separation), 0, Math.cos(separation)],
          radius,
          [0, 0, 1],
          sunRadius,
        );
        const truth = reference(radius, sunRadius, separation);
        [result.area, ...result.vector].forEach((v, i) => {
          expect(Math.abs(v - truth[i]) / sphericalCapArea(sunRadius)).toBeLessThan(3e-6);
        });
        expect(Math.hypot(...result.vector)).toBeLessThanOrEqual(result.area + 1e-12);
      }
});
test("cap containment, zero, symmetry, and invalid domains have explicit behavior", () => {
  expect(sphericalCapIntersection([0, 0, 3], 0.1, [0, 0, 1], 0.00465).area).toBe(sphericalCapArea(0.00465));
  expect(sphericalCapIntersection([1, 0, 0], 0.1, [0, 0, 1], 0.00465).area).toBe(0);
  expect(sphericalCapIntersection([0, 0, 1], 0, [0, 0, 1], 0.1).area).toBe(0);
  expect(() => sphericalCapArea(Number.NaN)).toThrow();
  expect(() => sphericalCapArea(2)).toThrow();
  expect(() => sphericalCapIntersection([0, 0, 0], 0.1, [0, 0, 1], 0.1)).toThrow();
  const a = sphericalCapIntersection([0, 0, 1], 0.1, [0.1, 0, 1], 0.05);
  const b = sphericalCapIntersection([0.1, 0, 1], 0.05, [0, 0, 1], 0.1);
  expect(a.area).toBeCloseTo(b.area, 13);
  expect(a.vector).toEqual(b.vector);
});
test("actual sphere diffuse lighting rejects horizons, transparency, deformation, and inside receivers", () => {
  const sphere = { center: [0, 0, 10] as const, radius: 1, opaque: true, rigid: true };
  const query = (normal: readonly [number, number, number], blocker = sphere) =>
    evaluateSphereSun([0, 0, 0], normal, [0, 0, 1], 0.00465, blocker);
  const blocked = query([0, 0, 1]);
  expect(blocked.kind).toBe("evaluated");
  if (blocked.kind === "evaluated") {
    expect(blocked.fraction).toBe(0);
    expect(blocked.diffuseIntegral).toBe(0);
    expect(blocked.numericError).toBe("unknown");
  }
  expect(query([1, 0, 0])).toEqual({ kind: "fallback", reason: "horizon-clipping" });
  expect(query([0, 0, 1], { ...sphere, opaque: false }).kind).toBe("fallback");
  expect(query([0, 0, 1], { ...sphere, rigid: false }).kind).toBe("fallback");
  expect(query([0, 0, 1], { ...sphere, radius: 11 })).toEqual({ kind: "fallback", reason: "inside-blocker" });
  expect(evaluateSphereSun([0, 0, 0], [0, 0, 1], [0, 0, 1], 0, sphere).kind).toBe("fallback");
});
test("proxy union bracket never adds overlapping interiors or certifies numeric culling", () => {
  const direction = [Math.sin(0.075), 0, Math.cos(0.075)] as const;
  const proxy = { direction, innerRadius: 0.074, outerRadius: 0.076 };
  const one = proxySunVisibility([0, 0, 1], 0.00465, [proxy]);
  const duplicate = proxySunVisibility([0, 0, 1], 0.00465, [proxy, proxy]);
  const actual =
    1 - sphericalCapIntersection([0, 0, 1], 0.00465, direction, 0.075).area / sphericalCapArea(0.00465);
  expect(one.lower).toBeLessThan(actual);
  expect(one.upper).toBeGreaterThan(actual);
  expect(duplicate.upper).toBe(one.upper);
  expect(duplicate.lower).toBeLessThanOrEqual(one.lower);
  expect(duplicate.fallbackRequired).toBe(true);
  expect(duplicate.numericError).toBe("unknown");
});
