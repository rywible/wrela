import { expect, test } from "bun:test";
import { type Bounds, identityMatrix, type RenderSurface, type Vec3 } from "@wrela/model";

import { buildHiZ, hiddenByHiZ } from "./hi-z";
import {
  analyticInnerSphere,
  hiddenFromDirection,
  hiddenFromPoint,
  hiddenFromPreparedDirection,
  hiddenFromPreparedPoint,
  prepareDirectionalOccluder,
  preparePointOccluder,
  sphereInsideDepth,
} from "./opaque-visibility";

const projection = identityMatrix();
projection[10] = 0.01;
const sphere = { center: [0, 0, 5] as Vec3, radius: 1 };
const behind: Bounds = { min: [-0.1, -0.1, 9], max: [0.1, 0.1, 10] };

test("sphere occlusion preserves camera/light separation, near-plane, grazing, and nonfinite fallbacks", () => {
  expect(hiddenFromPoint(behind, sphere, [0, 0, 0], projection)).toBe(true);
  expect(hiddenFromDirection(behind, sphere, [0, 0, -1])).toBe(true);
  expect(hiddenFromDirection(behind, sphere, [1, 0, 0])).toBe(false);
  expect(hiddenFromPoint(behind, sphere, [0, 0, 5], projection)).toBe(false);
  expect(hiddenFromPoint({ min: [0, 0, 0.1], max: [1, 1, 1] }, sphere, [0, 0, 0], projection)).toBe(false);
  expect(hiddenFromPoint({ min: [2, 0, 9], max: [3, 1, 10] }, sphere, [0, 0, 0], projection)).toBe(false);
  expect(hiddenFromPoint(behind, { ...sphere, radius: NaN }, [0, 0, 0], projection)).toBe(false);
  expect(hiddenFromPoint({ ...behind, min: [NaN, 0, 9] }, sphere, [0, 0, 0], projection)).toBe(false);
  expect(hiddenFromPoint(behind, sphere, [0, 0, 0], new Float32Array(3))).toBe(false);
  const clipped = identityMatrix();
  clipped[14] = -5;
  expect(hiddenFromPoint(behind, sphere, [0, 0, 0], clipped)).toBe(false);
  expect(sphereInsideDepth(sphere, clipped)).toBe(false);
  expect(sphereInsideDepth(sphere, projection)).toBe(true);
});

test("every culled box agrees with independent sphere-ray intersection through its interior", () => {
  let culled = 0;
  for (let i = 0; i < 600; i++) {
    const x = Math.sin(i * 2.37) * 4,
      y = Math.cos(i * 1.17) * 3,
      z = 4 + (i % 20);
    const bounds: Bounds = { min: [x - 0.1, y - 0.1, z - 0.1], max: [x + 0.1, y + 0.1, z + 0.1] };
    if (!hiddenFromPoint(bounds, sphere, [0, 0, 0], projection)) continue;
    culled++;
    for (const u of [0, 0.5, 1])
      for (const v of [0, 0.5, 1])
        for (const w of [0, 0.5, 1]) {
          const p = [x - 0.1 + 0.2 * u, y - 0.1 + 0.2 * v, z - 0.1 + 0.2 * w];
          const a = p.reduce((sum, value) => sum + value * value, 0),
            b = -10 * p[2],
            c = 24;
          const discriminant = b * b - 4 * a * c;
          expect(discriminant).toBeGreaterThan(0);
          const first = (-b - Math.sqrt(discriminant)) / (2 * a);
          expect(first).toBeGreaterThan(0);
          expect(first).toBeLessThan(1);
        }
  }
  expect(culled).toBeGreaterThan(50);
});

test("active analytic proxies reject deformation, incomplete geometry, bad matrices, and tiny numeric domains", () => {
  const surface = {
    matrix: identityMatrix(),
    mesh: { indices: new Uint32Array(12) },
    selectedRenderProduct: {
      kind: "analytic-quadric",
      primitive: { center: [0, 0, 0], radii: [1, 2, 3], rotation: [0, 0, 0] },
    },
  } as RenderSurface;
  const inner = analyticInnerSphere(surface);
  expect(inner?.radius).toBeLessThan(1);
  expect(inner?.radius).toBeGreaterThan(0.99);
  expect(analyticInnerSphere({ ...surface, wind: 1 })).toBeNull();
  expect(analyticInnerSphere({ ...surface, skin: {} as NonNullable<RenderSurface["skin"]> })).toBeNull();
  expect(analyticInnerSphere({ ...surface, drawRange: { start: 0, count: 3 } })).toBeNull();
  expect(analyticInnerSphere({ ...surface, selectedRenderProduct: undefined })).toBeNull();
  const matrix = identityMatrix();
  matrix[4] = 0.5;
  expect(analyticInnerSphere({ ...surface, matrix })).toBeNull();
  matrix[4] = 0;
  matrix[12] = 1e9;
  expect(analyticInnerSphere({ ...surface, matrix })).toBeNull();
});

test("conventional Hi-Z uses farthest depth, uncovered pixels and current frame data", () => {
  const covered = buildHiZ(3, 3, new Float32Array(9).fill(0.2));
  const pixels = { x0: 0, y0: 0, x1: 2, y1: 2 };
  expect(covered.map((level) => [level.width, level.height])).toEqual([
    [3, 3],
    [2, 2],
    [1, 1],
  ]);
  expect(hiddenByHiZ(covered, pixels, 0.4)).toBe(true);
  for (const hole of [1, NaN, Infinity]) {
    const depth = new Float32Array(9).fill(0.2);
    depth[4] = hole;
    expect(hiddenByHiZ(buildHiZ(3, 3, depth), pixels, 0.4)).toBe(false);
  }
  expect(hiddenByHiZ(covered, pixels, 0.2)).toBe(false);
  expect(hiddenByHiZ(covered, { ...pixels, x0: -1 }, 0.4)).toBe(false);
  expect(hiddenByHiZ(covered, pixels, NaN)).toBe(false);
});

test("prepared allocation-free occlusion preserves the original conservative tests across adversarial bounds", () => {
  const eye: Vec3 = [0, 0, 0],
    light: Vec3 = [0.2, -0.1, -1];
  const point = preparePointOccluder(sphere, eye, projection),
    direction = prepareDirectionalOccluder(sphere, light);
  if (!point || !direction) throw new Error("Missing prepared occluder");
  for (let i = 0; i < 3000; i++) {
    const x = Math.sin(i * 2.371) * 4,
      y = Math.cos(i * 1.171) * 3,
      z = 0.5 + (i % 83) / 3;
    const bounds: Bounds = { min: [x - 0.05, y - 0.05, z - 0.05], max: [x + 0.05, y + 0.05, z + 0.05] };
    expect(hiddenFromPreparedPoint(bounds, point)).toBe(hiddenFromPoint(bounds, sphere, eye, projection));
    expect(hiddenFromPreparedDirection(bounds, direction)).toBe(hiddenFromDirection(bounds, sphere, light));
  }
  expect(hiddenFromPreparedPoint({ min: [NaN, 0, 0], max: [1, 1, 1] }, point)).toBe(false);
  expect(hiddenFromPreparedDirection({ min: [-Infinity, 0, 0], max: [1, 1, 1] }, direction)).toBe(false);
  expect(preparePointOccluder(sphere, sphere.center, projection)).toBeNull();
  expect(prepareDirectionalOccluder(sphere, [0, 0, 0])).toBeNull();
});
