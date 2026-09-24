import { expect, test } from "bun:test";
import { compileSurfaceRelief } from "@wrela/compiler";
import { contentKey, type MeshData, normalize } from "@wrela/model";

import {
  createSurfaceReliefCpuSource,
  measurePlanarReliefReference,
  SURFACE_RELIEF_CPU_CAP,
  SURFACE_RELIEF_CPU_RECIPES,
  samplePlanarReliefMesh,
} from "./fixtures/surface-relief-cpu";

function plane(xSlope: number, ySlope: number): MeshData {
  const points = [
    [-0.3, -0.35],
    [0.3, -0.35],
    [0.3, 0.35],
    [-0.3, 0.35],
  ];
  return {
    positions: new Float32Array(points.flatMap(([x, y]) => [x, y, 0.3 - (0.04 + xSlope * x + ySlope * y)])),
    normals: new Float32Array(points.flatMap(() => normalize([xSlope, ySlope, 1]))),
    indices: new Uint32Array([0, 1, 2, 0, 2, 3]),
    bounds: { min: [-0.3, -0.35, 0], max: [0.3, 0.35, 0.3] },
    materialGroups: [{ material: SURFACE_RELIEF_CPU_CAP, start: 0, count: 6 }],
  };
}

test("planar relief metric reconstructs known sloped depth and normals at a fixed uniform domain", () => {
  const samples = samplePlanarReliefMesh(plane(0.05, -0.03), 17);
  const result = measurePlanarReliefReference(samples, 0.3, ([x, y]) => 0.04 + x * 0.05 - y * 0.03, 1e-5);
  expect(result.samples).toBe(17 * 17);
  expect(result.interpolatedNormalDegrees.maximum).toBeLessThan(1e-5);
  expect(result.triangleNormalDegrees.maximum).toBeLessThan(1e-5);
  expect(result.depthMetres.maximum).toBeLessThan(2e-8);
  expect(result.gradientHalfStepDifferenceDegrees.maximum).toBeLessThan(1e-5);
});

test("metric detects a known shading-normal error independently of correct positions", () => {
  const mesh = plane(0, 0);
  const radians = (10 * Math.PI) / 180;
  for (let index = 0; index < mesh.normals.length; index += 3)
    mesh.normals.set([Math.sin(radians), 0, Math.cos(radians)], index);
  const result = measurePlanarReliefReference(samplePlanarReliefMesh(mesh, 9), 0.3, () => 0.04, 1e-5);
  expect(result.interpolatedNormalDegrees.rms).toBeCloseTo(10, 5);
  expect(result.triangleNormalDegrees.maximum).toBeLessThan(1e-6);
  expect(result.depthMetres.maximum).toBeLessThan(2e-8);
  expect(() => samplePlanarReliefMesh({ ...mesh, materialGroups: [] })).toThrow("not fully covered");
});

test("matched relief studies preserve source recipes and sample locations across mesh budgets", () => {
  const { mesh, planeZ } = createSurfaceReliefCpuSource();
  const sourceKey = contentKey(mesh);
  const recipe = SURFACE_RELIEF_CPU_RECIPES[1].recipe;
  const recipeKey = contentKey(recipe);
  const baseline = samplePlanarReliefMesh(mesh, 11);
  const low = compileSurfaceRelief(mesh, recipe, { material: SURFACE_RELIEF_CPU_CAP, maxTriangles: 512 });
  const high = compileSurfaceRelief(mesh, recipe, { material: SURFACE_RELIEF_CPU_CAP, maxTriangles: 2048 });
  const lowSamples = samplePlanarReliefMesh(low.mesh, 11);
  const highSamples = samplePlanarReliefMesh(high.mesh, 11);
  expect(baseline.every((sample) => sample.normal[2] > 0.999)).toBe(true);
  expect(baseline.every((sample) => Math.abs(sample.point[2] - planeZ) < 1e-7)).toBe(true);
  expect(lowSamples.map((sample) => sample.point.slice(0, 2))).toEqual(
    highSamples.map((sample) => sample.point.slice(0, 2)),
  );
  expect(lowSamples.some((sample) => sample.point[2] < planeZ - 0.001)).toBe(true);
  expect(high.review.triangles).toBeGreaterThan(low.review.triangles);
  expect(contentKey(mesh)).toBe(sourceKey);
  expect(contentKey(recipe)).toBe(recipeKey);
});
