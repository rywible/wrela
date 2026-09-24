import { expect, test } from "bun:test";
import { indirectProbeVisible, sampleIndirectField } from "@wrela/compiler";
import type { EvaluatedScene, RenderSurface } from "@wrela/model";

import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { evaluateEnvironment } from "./environment";
import { IndirectLightingCache } from "./indirect-lighting";

function fixture() {
  const data = indirectBoxFixture({ occluder: false });
  const scene: EvaluatedScene = {
    surfaces: data.surfaces,
    camera: data.camera,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  return { scene, lighting: data.lighting };
}
test("diffuse cache publishes progressively, reuses immutable inputs and invalidates source changes", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  const options = { dimensions: [2, 2, 2] as [number, number, number], lighting, samples: 32, skySamples: 2 };
  expect(cache.update(scene, options)).toBeUndefined();
  const first = await cache.waitReady();
  expect(first.completedProbes).toBe(8);
  expect(first.report.status).toBe("ready");
  expect(first.report.bounces).toBe(2);
  expect(cache.builds).toBe(1);
  expect(cache.update({ ...scene, time: 1 }, options)).toBe(first);
  // Camera/origin do not change absolute static transport.
  const rebased = {
    ...scene,
    origin: [16, 0, 0] as [number, number, number],
    surfaces: scene.surfaces.map((s) => {
      const matrix = s.matrix.slice();
      matrix[12] -= 16;
      return { ...s, matrix };
    }),
  };
  expect(cache.update(rebased, options)).toBe(first);
  expect(cache.builds).toBe(1);
  const edited = {
    ...scene,
    surfaces: scene.surfaces.map((s, i) =>
      i ? s : { ...s, material: { ...s.material, color: [0.1, 0.2, 0.3] as [number, number, number] } },
    ),
  };
  expect(cache.update(edited, options)).toBeUndefined();
  const second = await cache.waitReady();
  expect(second.key).not.toBe(first.key);
  expect(cache.builds).toBe(2);
  expect(cache.byteLength).toBe(
    8 * 60 * 4 +
      (second.visibility?.nodes.byteLength ?? 0) +
      (second.visibility?.triangles.byteLength ?? 0) +
      (second.visibility?.cells?.byteLength ?? 0) +
      (second.reflections?.data.byteLength ?? 0) +
      (second.surfaceCache?.data.byteLength ?? 0) +
      (second.triangleCache?.report.bytes ?? 0),
  );
  cache.dispose();
  expect(cache.field).toBeUndefined();
});

test("receiver-to-probe BVH checks prevent moment-field light leakage through closed-box corners", async () => {
  const data = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
  const scene: EvaluatedScene = {
    surfaces: data.surfaces,
    camera: data.camera,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  const cache = new IndirectLightingCache();
  cache.update(scene, { dimensions: [4, 4, 4], lighting: data.lighting, samples: 128, skySamples: 8 });
  const field = await cache.waitReady();
  let unguardedMaximum = 0;
  for (let y = 0; y < 10; y++)
    for (let x = 0; x < 10; x++) {
      const world: [number, number, number] = [-0.99 + x * 0.198, 0.01 + y * 0.198, -1],
        normal: [number, number, number] = [0, 0, 1];
      const guarded = sampleIndirectField(field, world, normal);
      expect(guarded.slice(0, 3)).toEqual([0, 0, 0]);
      expect(guarded[3]).toBe(1);
      unguardedMaximum = Math.max(
        unguardedMaximum,
        ...sampleIndirectField({ ...field, visibility: undefined }, world, normal).slice(0, 3),
      );
    }
  // This fixture must exercise the original failure, not merely a uniformly dark cache.
  expect(unguardedMaximum).toBeGreaterThan(0.01);
  cache.dispose();
});
test("budget refusal removes stale illumination; cancelled builds cannot publish", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  cache.update(scene, { lighting, dimensions: [2, 2, 2], samples: 16, skySamples: 1 });
  await cache.waitReady();
  cache.update(scene, { lighting, maxTriangles: 1 });
  expect(cache.field).toBeUndefined();
  await expect(cache.waitReady()).rejects.toThrow("whole build refused");
  expect(cache.report?.status).toBe("refused");
  cache.update(scene, { lighting, dimensions: [2, 2, 2], samples: 16, skySamples: 1 });
  cache.dispose();
  await expect(cache.waitReady()).rejects.toThrow();
  expect(cache.field).toBeUndefined();
});

test("excluded animated surfaces do not invalidate static lighting when they move", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  const moving = { ...scene.surfaces[0], id: "moving", wind: 1, matrix: scene.surfaces[0].matrix.slice() };
  scene.surfaces.push(moving);
  const options = { lighting, dimensions: [2, 2, 2] as [number, number, number], samples: 16, skySamples: 1 };
  cache.update(scene, options);
  const field = await cache.waitReady();
  moving.matrix[12] += 17;
  expect(cache.update(scene, options)).toBe(field);
  expect(cache.builds).toBe(1);
  expect(field.report.excluded.some((entry) => entry.id === "moving")).toBe(true);
  cache.dispose();
});

test("articulated rigid motion preserves a pending static cache and stationary frame occlusion", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  const frame = scene.surfaces.find((surface) => surface.id === "back-wall");
  if (!frame) throw Error("Missing static frame fixture");
  const moving: RenderSurface = {
    ...frame,
    id: "hinged-panel",
    lightingMobility: "dynamic",
    matrix: frame.matrix.slice(),
  };
  moving.matrix[14] = 1;
  scene.surfaces.push(moving);
  const options = { lighting, dimensions: [2, 2, 2] as [number, number, number], samples: 16, skySamples: 1 };
  cache.update(scene, options);
  moving.matrix[12] = 3;
  cache.update(scene, options);
  expect(cache.builds).toBe(1);
  const field = await cache.waitReady();
  expect(field.report.triangles).toBe(8);
  expect(field.report.excluded).toEqual([{ id: "hinged-panel", reason: "dynamic geometry excluded" }]);
  // The articulated panel is absent; its stationary parent/frame remains a blocker.
  expect(indirectProbeVisible(field, [0, 1, 0.5], [0, 0, -1], [0, 1, -0.5])).toBe(true);
  expect(indirectProbeVisible(field, [0, 1, 0.5], [0, 0, -1], [0, 1, -1.5])).toBe(false);
  moving.matrix[12] = -3;
  expect(cache.update(scene, options)).toBe(field);
  expect(cache.builds).toBe(1);
  cache.dispose();
});

test("physical sky transfer reuses geometry under intensity/color changes and invalidates changing sun direction", async () => {
  const { scene } = fixture(),
    cache = new IndirectLightingCache();
  const options = { dimensions: [2, 2, 2] as [number, number, number], samples: 32, skySamples: 2 };
  cache.update(scene, options);
  const field = await cache.waitReady();
  expect(field.report.source).toBe("physical-sky-and-directional-sun");
  expect(field.transfer?.length).toBe(8 * 360);
  scene.environment.sunIntensity *= 0.5;
  scene.environment.skyColor = [0.8, 0.2, 0.1];
  expect(cache.update(scene, options)).toBe(field);
  scene.environment.sunDirection = [0, 1, 0];
  expect(cache.update(scene, options)).toBeUndefined();
  await cache.waitReady();
  expect(cache.builds).toBe(2);
  cache.dispose();
});

test("camera volume holds world probe density and reuses only within its snapped absolute cell", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  scene.camera.target = [0, 0, 0];
  const options = {
    lighting,
    dimensions: [2, 2, 2] as [number, number, number],
    samples: 16,
    skySamples: 1,
    cameraVolume: { radius: 12, halfHeight: 6, snap: 4 },
  };
  cache.update(scene, options);
  const field = await cache.waitReady();
  expect(field.origin).toEqual([-12, -6, -12]);
  expect(field.spacing).toEqual([24, 12, 24]);
  scene.camera.target = [1.9, 0, 0];
  expect(cache.update(scene, options)).toBe(field);
  scene.camera.target = [2.1, 0, 0];
  expect(cache.update(scene, options)).toBeUndefined();
  const moved = await cache.waitReady();
  expect(moved.origin).toEqual([-8, -6, -12]);
  expect(cache.builds).toBe(2);
  cache.dispose();
});

test("surface caching is automatic, reuses explicit defaults and invalidates edited receiver normals", async () => {
  const { scene } = fixture(),
    cache = new IndirectLightingCache();
  const options = { dimensions: [8, 6, 8] as [number, number, number], samples: 16, skySamples: 1 };
  try {
    cache.update(scene, options);
    const field = await cache.waitReady();
    expect(field.surfaceCache?.report.admitted).toBeGreaterThan(0);
    // Default resolves irradiance/reflected SH, not just the diagnostic blend weights.
    expect(field.surfaceCache?.data[5]).toBeGreaterThan(0);
    expect(cache.update(scene, { ...options, surfaceCache: {} })).toBe(field);
    const floor = scene.surfaces[0];
    scene.surfaces[0] = { ...floor, mesh: { ...floor.mesh, normals: floor.mesh.normals.slice() } };
    expect(cache.update(scene, options)).toBeUndefined();
    await cache.waitReady();
    expect(cache.builds).toBe(2);
    cache.update(scene, { ...options, surfaceCache: false });
    expect((await cache.waitReady()).surfaceCache).toBeUndefined();
  } finally {
    cache.dispose();
  }
});

test("unsupported receivers allocate no surface cache and retain their probe lighting", async () => {
  const { scene, lighting } = fixture(),
    cache = new IndirectLightingCache();
  scene.surfaces = scene.surfaces.map((s) => ({ ...s, material: { ...s.material, normalStrength: 1 } }));
  try {
    cache.update(scene, { dimensions: [2, 2, 2], lighting, samples: 16, skySamples: 1 });
    const field = await cache.waitReady();
    expect(field.report.status).toBe("ready");
    expect(field.surfaceCache).toBeUndefined();
    expect(field.data.some((v) => v > 0)).toBe(true);
    expect(field.visibility?.triangles.length).toBeGreaterThan(0);
  } finally {
    cache.dispose();
  }
});
