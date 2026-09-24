import { expect, test } from "bun:test";
import { identityMatrix } from "@wrela/model";

import {
  gpuVisibilityBytes,
  projectVisibilityBounds,
  shouldUseGpuVisibility,
  visibilityLevels,
} from "./gpu-visibility";

test("GPU visibility projections expand raster coverage and preserve uncertain domains", () => {
  const matrix = identityMatrix();
  const projected = projectVisibilityBounds(
    { min: [-0.1, -0.2, 0.6], max: [0.2, 0.1, 0.7] },
    matrix,
    100,
    100,
  );
  expect(projected).not.toBeNull();
  if (!projected) throw new Error("Expected projected bounds");
  expect(projected.x0).toBeLessThan(45);
  expect(projected.x1).toBeGreaterThan(60);
  expect(projected.y0).toBeLessThan(45);
  expect(projected.y1).toBeGreaterThan(60);
  expect(projected.nearest).toBeLessThan(0.6);
  for (const bounds of [
    { min: [-0.1, -0.1, -0.1], max: [0.1, 0.1, 0.1] },
    { min: [-1, -0.1, 0.2], max: [0.1, 0.1, 0.3] },
    { min: [NaN, 0, 0], max: [1, 1, 1] },
  ])
    expect(
      projectVisibilityBounds(bounds as Parameters<typeof projectVisibilityBounds>[0], matrix, 100, 100),
    ).toBeNull();
});

test("odd Hi-Z levels account for every source pixel and allocation", () => {
  const plan = visibilityLevels(7, 3);
  expect(plan.levels).toEqual([
    { width: 7, height: 3, offset: 0 },
    { width: 4, height: 2, offset: 21 },
    { width: 2, height: 1, offset: 29 },
    { width: 1, height: 1, offset: 31 },
  ]);
  expect(plan.values).toBe(32);
  expect(gpuVisibilityBytes(7, 3, 4, 128, 32)).toBeGreaterThan(7 * 3 * 4 * 4 + 32 * 4 + 128 * 80 * 2);
  expect(() => visibilityLevels(0, 3)).toThrow();
  expect(() => visibilityLevels(100000, 100000)).toThrow();
});

test("visibility work estimate bypasses scenes that cannot amortize its prepass", () => {
  expect(shouldUseGpuVisibility(500, 2_000_000, 20_000)).toBe(true);
  expect(shouldUseGpuVisibility(5, 2_000_000, 20_000)).toBe(false);
  expect(shouldUseGpuVisibility(500, 20_000, 200)).toBe(false);
  expect(shouldUseGpuVisibility(500, 2_000_000, 500_000)).toBe(false);
  expect(shouldUseGpuVisibility(500, 2_000_000, 0)).toBe(false);
  expect(shouldUseGpuVisibility(500, 2_000_000, 20_000, 1920 * 1080)).toBe(false);
  expect(shouldUseGpuVisibility(500, 8_000_000, 20_000, 1920 * 1080)).toBe(true);
  expect(shouldUseGpuVisibility(500, 8_000_000, 20_000, 1920 * 1080, 4)).toBe(false);
});
