import { expect, test } from "bun:test";
import type { EvaluatedScene, Vec3 } from "@wrela/model";

import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import {
  compileLightMask,
  lightIntersectsBounds,
  PointShadowsGpu,
  pointShadowFaceIntersectsBounds,
  pointShadowMatrices,
} from "./local-lighting";
import { GLOBAL_FLOATS } from "./packing";

test("light influence never discards unbounded lights and preserves tangency", () => {
  const bounds = { min: [-1, -1, -1] as Vec3, max: [1, 1, 1] as Vec3 };
  const source = { color: [1, 1, 1] as Vec3, intensity: 2 };
  expect(
    compileLightMask(bounds, [
      { ...source, position: [1000, 0, 0] },
      { ...source, position: [3, 0, 0], range: 2 },
      { ...source, position: [3.001, 0, 0], range: 2 },
    ]),
  ).toBe(3);
  expect(lightIntersectsBounds({ min: [NaN, 0, 0], max: [1, 1, 1] }, [0, 0, 0], 2)).toBe(true);
});
test("all six point shadow views map their forward direction to center with consistent depth", () => {
  const position: Vec3 = [7, -2, 13];
  const dirs: Vec3[] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
  ];
  const matrices = pointShadowMatrices(position, 80);
  for (let face = 0; face < 6; face++)
    for (const d of [0.03, 1, 79.9]) {
      const p = [...position.map((v, a) => v + dirs[face][a] * d), 1];
      const m = matrices[face];
      const q = [0, 1, 2, 3].map((row) => p.reduce((sum, v, col) => sum + m[col * 4 + row] * v, 0));
      expect(Math.abs(q[0] / q[3])).toBeLessThan(1e-5);
      expect(Math.abs(q[1] / q[3])).toBeLessThan(1e-5);
      expect(q[2] / q[3]).toBeGreaterThan(-0.00005);
      expect(q[2] / q[3]).toBeLessThan(1);
    }
});

test("cached local shadows invalidate geometric changes, retain radiometric edits, and release ownership", () => {
  Object.assign(globalThis, {
    GPUTextureUsage: { RENDER_ATTACHMENT: 1, TEXTURE_BINDING: 2 },
    GPUBufferUsage: { UNIFORM: 1, COPY_DST: 2 },
  });
  let destroyed = 0;
  const device = {
    createTexture: () => ({ createView: () => ({}), destroy: () => destroyed++ }),
    createBuffer: ({ size }: { size: number }) => ({ size, destroy: () => destroyed++ }),
    createBindGroup: () => ({}),
    queue: { writeBuffer: () => {} },
  } as unknown as GPUDevice;
  const shadows = new PointShadowsGpu(device, {} as GPUBindGroupLayout, 128);
  const encoder = {
    beginRenderPass: () => ({ setBindGroup: () => {}, end: () => {} }),
  } as unknown as GPUCommandEncoder;
  const fixture = indirectBoxFixture();
  const lights = [{ position: [0, 1, 0] as Vec3, color: [1, 1, 1] as Vec3, intensity: 2, range: 8 }];
  const scene: EvaluatedScene = {
    surfaces: fixture.surfaces,
    camera: fixture.camera,
    environment: {
      sunDirection: [0, 1, 0],
      sunColor: [1, 1, 1],
      sunIntensity: 1,
      ambient: 1,
      skyColor: [1, 1, 1],
      horizonColor: [1, 1, 1],
      groundColor: [1, 1, 1],
      fogDensity: 0,
      wind: [0, 0, 0],
      exposure: 1,
    },
    time: 0,
    mode: "beauty",
    grid: false,
  };
  const frame = new Float32Array(GLOBAL_FLOATS);
  const render = (complete = true) => {
    shadows.encode(encoder, shadows.prepare(scene, lights), frame, complete, () => 1);
    return shadows.lastPasses;
  };
  expect(render(false)).toBe(0);
  expect(render()).toBe(6);
  expect(render()).toBe(0);
  lights[0].intensity = 4;
  lights[0].color = [1, 0, 0];
  expect(render()).toBe(0);
  scene.surfaces[0].matrix[12] += 0.1;
  const changedFaces = render();
  expect(changedFaces).toBeGreaterThan(0);
  expect(changedFaces).toBeLessThan(6);
  expect(render()).toBe(0);
  lights[0].position[0] += 0.1;
  expect(render()).toBe(6);
  scene.surfaces[0].wind = 1;
  expect(render()).toBeGreaterThan(0);
  scene.time += 0.1;
  expect(render()).toBeGreaterThan(0);
  scene.surfaces[0].wind = 0;
  scene.surfaces[0].castsShadow = false;
  expect(shadows.prepare(scene, lights)[0].casters.has(scene.surfaces[0])).toBe(false);
  expect(shadows.byteLength).toBe(128 * 128 * 48 * 4 + shadows.bufferBytes);
  shadows.destroy();
  expect(destroyed).toBe(51);
});

test("all eight authored lights retain shadow slots across camera moves and inactive neighbors", () => {
  Object.assign(globalThis, {
    GPUTextureUsage: { RENDER_ATTACHMENT: 1, TEXTURE_BINDING: 2 },
    GPUBufferUsage: { UNIFORM: 1, COPY_DST: 2 },
  });
  const device = {
    createTexture: () => ({ createView: () => ({}), destroy() {} }),
    createBuffer: ({ size }: { size: number }) => ({ size, destroy() {} }),
    createBindGroup: () => ({}),
    queue: { writeBuffer() {} },
  } as unknown as GPUDevice;
  const shadows = new PointShadowsGpu(device, {} as GPUBindGroupLayout, 128);
  const f = indirectBoxFixture();
  const scene = {
    ...f,
    environment: { wind: [0, 0, 0] },
    mode: "beauty",
    time: 0,
  } as unknown as EvaluatedScene;
  const lights = Array.from({ length: 8 }, (_, i) => ({
    position: [i * 0.2, 1, 0] as Vec3,
    color: [1, 1, 1] as Vec3,
    intensity: 1,
    range: 200,
  }));
  expect(shadows.prepare(scene, lights).map((p) => [p.index, p.slot, p.far])).toEqual(
    lights.map((_, i) => [i, i, 200]),
  );
  scene.camera.position = [100, 0, 0];
  lights[2].intensity = 0;
  expect(shadows.prepare(scene, lights).map((p) => p.slot)).toEqual([0, 1, 3, 4, 5, 6, 7]);
  const fullBudget = shadows.byteLength;
  lights.forEach((light, index) => {
    light.intensity = index >= 6 ? 1 : 0;
  });
  expect(shadows.prepare(scene, lights).map((p) => [p.index, p.slot])).toEqual([
    [6, 0],
    [7, 1],
  ]);
  expect(shadows.resolution).toBe(256);
  expect(shadows.byteLength).toBe(fullBudget);
  const cachedView = shadows.view;
  lights[6].color = [1, 0, 0];
  expect(shadows.prepare(scene, lights).map((p) => p.slot)).toEqual([0, 1]);
  expect(shadows.view).toBe(cachedView);
  lights[2].intensity = 1;
  lights[3].intensity = 1;
  expect(shadows.prepare(scene, lights).map((p) => p.slot)).toEqual([0, 1, 2, 3]);
  expect(shadows.resolution).toBe(176);
  expect(shadows.byteLength).toBeLessThanOrEqual(fullBudget);
  lights.forEach((light) => {
    light.intensity = 1;
  });
  expect(shadows.prepare(scene, lights).map((p) => p.slot)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
  expect(shadows.resolution).toBe(128);
  expect(shadows.byteLength).toBe(fullBudget);
  shadows.destroy();
});

test("cube face culling retains boundaries and rejects opposite-axis casters", () => {
  const bounds = { min: [2, -0.1, -0.1] as Vec3, max: [3, 0.1, 0.1] as Vec3 };
  expect(
    Array.from({ length: 6 }, (_, face) => pointShadowFaceIntersectsBounds(bounds, [0, 0, 0], face)),
  ).toEqual([true, false, false, false, false, false]);
  expect(pointShadowFaceIntersectsBounds({ min: [2, 3, 0], max: [3, 4, 1] }, [0, 0, 0], 0)).toBe(true);
});
