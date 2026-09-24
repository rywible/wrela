import { expect, test } from "bun:test";
import {
  add,
  cross,
  dot,
  normalize,
  type SurfaceRelief,
  scale,
  surfaceReliefSchema,
  type Vec3,
} from "@wrela/model";

import {
  surfaceReliefBands,
  surfaceReliefDepth,
  surfaceReliefGradient,
  surfaceReliefResidual,
  surfaceReliefSlopeVariance,
} from "./surface-relief-pattern";
import {
  surfaceReliefEdgePriority,
  surfaceReliefGeometryWeights,
  surfaceReliefPatternSpacing,
} from "./surface-relief-sampling";

const recipe = (kind: SurfaceRelief["kind"]): SurfaceRelief =>
  surfaceReliefSchema.parse({ kind, amplitude: 0.018, scale: 0.1, seed: 37, targetEdgeLength: 0.02 });

test("bark harmonic bands reconstruct the exact authored rib profile independently of tessellation", () => {
  const source = recipe("bark"),
    n: Vec3 = [1, 0, 0];
  // Same grain/axial position: noise flake changes cancel when comparing two band sums.
  for (let sample = 0; sample < 128; sample++) {
    const p: Vec3 = [0.137, sample * 0.0017, sample * 0.0031];
    const { mean, bands } = surfaceReliefBands(p, n, source);
    const full = surfaceReliefDepth(p, n, source);
    expect(full).toBeCloseTo(mean + bands.reduce((sum, value) => sum + value, 0), 12);
    expect(full).toBeGreaterThanOrEqual(0);
    expect(full).toBeLessThanOrEqual(1);
    const geometryWeights: Vec3 = [0.8, 0.23, 0];
    const physical = source.amplitude * surfaceReliefDepth(p, n, source, geometryWeights);
    expect(physical + surfaceReliefResidual(p, n, source, geometryWeights)).toBeCloseTo(
      full * source.amplitude,
      12,
    );
  }
  // Golden samples from the original sine-power implementation protect the authored recipe.
  for (const [point, expected] of [
    [[0.13, 0.27, -0.38], 0.08902935375620971],
    [[0.001, 0.019, 0.028], 0.10386034980165806],
    [[0.193, -0.032, 0.116], 0.14442143669470472],
    [[-0.3, 0.47, 0.001], 0.1442125768349222],
  ] as [Vec3, number][])
    expect(surfaceReliefDepth(point, n, source)).toBeCloseTo(expected, 12);
});

test("coarser geometry carries unresolved bands as complementary appearance without changing the recipe", () => {
  const source = recipe("stone");
  const finer = surfaceReliefGeometryWeights(0.005, source),
    coarser = surfaceReliefGeometryWeights(0.04, source);
  expect(finer[0]).toBe(1);
  expect(coarser[0]).toBeGreaterThan(0);
  expect(coarser[1]).toBe(0);
  expect(coarser[2]).toBe(0);
  expect(finer.every((value, index) => value >= coarser[index])).toBe(true);
  expect(surfaceReliefGeometryWeights(0, source)).toEqual([1, 1, 1]);
  expect(surfaceReliefGeometryWeights(1, source)).toEqual([0, 0, 0]);
});

test("anisotropic bark sampling prefers transverse ribs and includes axial warping", () => {
  const source = recipe("bark"),
    across: Vec3 = [0.04, 0, 0],
    along: Vec3 = [0, 0.1, 0];
  expect(surfaceReliefEdgePriority(across, source)).toBeGreaterThan(surfaceReliefEdgePriority(along, source));
  expect(surfaceReliefPatternSpacing(along, source)).toBeCloseTo(0.01);
  expect(surfaceReliefPatternSpacing(across, source)).toBeCloseTo(0.04);
});

test("physical relief gradients are tangent and stable under finite difference refinement", () => {
  const source = recipe("stone"),
    p: Vec3 = [0.132, 0.237, -0.092],
    n = normalize([1, 2, 3]);
  const gradient = surfaceReliefGradient(p, n, source),
    tangent = normalize(cross(n, [0, 1, 0]));
  const epsilon = source.scale * 0.00005;
  const derivative =
    (source.amplitude *
      (surfaceReliefDepth(add(p, scale(tangent, epsilon)), n, source) -
        surfaceReliefDepth(add(p, scale(tangent, -epsilon)), n, source))) /
    (2 * epsilon);
  expect(dot(gradient, n)).toBeCloseTo(0, 10);
  expect(dot(gradient, tangent)).toBeCloseTo(derivative, 5);
  const moments = surfaceReliefSlopeVariance(source);
  expect(moments.every((value) => Number.isFinite(value) && value > 0)).toBe(true);
  expect(
    surfaceReliefSlopeVariance({ ...source, amplitude: source.amplitude * 2 }).map((value) => value / 4),
  ).toEqual(moments);
  expect(surfaceReliefSlopeVariance({ ...source, amplitude: 0 })).toEqual([0, 0, 0]);
});
