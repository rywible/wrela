import { expect, test } from "bun:test";
import { indirectProbeVisible, sampleIndirectField } from "@wrela/compiler";
import type { EvaluatedScene, Vec3 } from "@wrela/model";
import { packIndirectLighting } from "@wrela/render-webgpu/indirect";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { indirectBoxFixture } from "./fixtures/indirect-scenes";

test("probe-plane receivers retain stable light across FP32-sized coordinate changes", async () => {
  const f = indirectBoxFixture(),
    cache = new IndirectLightingCache();
  const scene: EvaluatedScene = {
    ...f,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  cache.update(scene, { dimensions: [12, 8, 12], lighting: f.lighting, samples: 64, skySamples: 4 });
  const field = await cache.waitReady();
  const values = [-1e-7, 0, 1e-7].map((dz) =>
    sampleIndirectField(field, [-0.17083335, 0.01787252, Math.fround(0.3) + dz], [0, 0, 1]),
  );
  expect(values[1][2]).toBeGreaterThan(0.05);
  for (const value of values)
    for (let c = 0; c < 3; c++) expect(Math.abs(value[c] - values[1][c])).toBeLessThan(0.002);
  cache.dispose();
});

test("nonzero-world-origin rebasing preserves both irradiance and packed BVH wall rejection", async () => {
  const f = indirectBoxFixture({ ceiling: true, front: true, occluder: false }),
    cache = new IndirectLightingCache();
  const origin: Vec3 = [1e7, 100000, -1e7];
  const scene: EvaluatedScene = {
    ...f,
    origin,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  const options = { dimensions: [4, 4, 4] as Vec3, lighting: f.lighting, samples: 64, skySamples: 4 };
  cache.update(scene, options);
  const field = await cache.waitReady();
  const world = origin.map((v, i) => v + [0.7, 1.7, -1][i]) as Vec3,
    normal: Vec3 = [0, 0, 1];
  const exteriorProbe = origin.map((v, i) => v + [1.1, 2.1, -0.35][i]) as Vec3;
  expect(indirectProbeVisible(field, world, normal, exteriorProbe)).toBe(false);
  const nextOrigin: Vec3 = [origin[0] + 16, origin[1], origin[2] - 32];
  const rebased: EvaluatedScene = {
    ...scene,
    origin: nextOrigin,
    surfaces: scene.surfaces.map((s) => {
      const matrix = s.matrix.slice();
      matrix[12] -= 16;
      matrix[14] += 32;
      return { ...s, matrix };
    }),
  };
  expect(cache.update(rebased, options)).toBe(field);
  expect(cache.builds).toBe(1);
  const before = packIndirectLighting(field, origin),
    after = packIndirectLighting(field, nextOrigin);
  const queryOffset = before[12] * 4;
  expect(after.slice(queryOffset)).toEqual(before.slice(queryOffset));
  for (const renderOrigin of [origin, nextOrigin]) {
    const packed = packIndirectLighting(field, renderOrigin),
      relative = { ...field, origin: [...packed.subarray(0, 3)] as Vec3 };
    const local = world.map((v, i) => v - renderOrigin[i]) as Vec3;
    const localProbe = exteriorProbe.map((v, i) => v - renderOrigin[i]) as Vec3;
    expect(indirectProbeVisible(relative, local, normal, localProbe)).toBe(false);
    expect(sampleIndirectField(relative, local, normal)).toEqual([0, 0, 0, 1]);
  }
  cache.dispose();
});
