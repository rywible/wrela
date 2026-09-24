import { expect, test } from "bun:test";
import type { Vec3 } from "@wrela/model";

import { shootSkyVisibility } from "./shoot-visibility";

test("local canopy escape remains bounded and loses sky only with neighboring source area", () => {
  const center = { center: [0, 0, 0] as Vec3, projectedArea: 0.01 };
  expect(shootSkyVisibility([center])[0]).toBe(1);
  const neighbors = Array.from({ length: 150 }, (_, i) => ({
    center: [
      ((i % 5) - 2) * 0.24,
      0.3 + Math.floor(i / 25) * 0.24,
      ((Math.floor(i / 5) % 5) - 2) * 0.24,
    ] as Vec3,
    projectedArea: 0.03,
  }));
  const visibility = shootSkyVisibility([center, ...neighbors]);
  expect(visibility[0]).toBeLessThan(0.7);
  expect(Array.from(visibility).every((v) => v >= 0 && v <= 1)).toBe(true);
  expect(shootSkyVisibility([center, ...neighbors.map((n) => ({ ...n, projectedArea: 0 }))])[0]).toBe(1);
});
