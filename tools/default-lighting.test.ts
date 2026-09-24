import { expect, test } from "bun:test";
import { compileSkyVisibilitySteps } from "@wrela/compiler";
import type { EvaluatedScene, RenderSurface, Vec3 } from "@wrela/model";
import { evaluateEnvironment, SkyVisibilityCache } from "@wrela/runtime";
import {
  compileIndirectGeometry,
  traceIndirectRay,
  traceSkyDistance,
} from "../packages/compiler/src/indirect-query";
import { LOCAL_LIGHT_OFFSET, packSurface, packVertices } from "../packages/render-webgpu/src/packing";
import { indirectBoxFixture } from "./fixtures/indirect-scenes";

test("local sky visibility darkens a sealed enclosure and retains both sides", () => {
  const fixture = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
  const steps = compileSkyVisibilitySteps(fixture.surfaces);
  let step = steps.next();
  while (!step.done) step = steps.next();
  const floor = step.value.meshes.get("floor")?.skyVisibility;
  expect(floor).toBeDefined();
  expect(floor?.every(Number.isFinite)).toBe(true);
  if (!floor) throw Error("Missing floor sky");
  for (let i = 0; i < floor.length; i += 4) {
    expect(Array.from(floor.slice(i, i + 3))).toEqual([0, 0, 0]);
    expect(floor[i + 3]).toBe(1);
  }
  expect(step.value.report.rays).toBeGreaterThan(0);
});

test("sky cache reuses lighting/camera/rebase edits, rejects changed geometry, and cancels disposal", async () => {
  const fixture = indirectBoxFixture({ occluder: false });
  const scene: EvaluatedScene = {
    ...fixture,
    environment: evaluateEnvironment(),
    mode: "beauty",
    grid: false,
    time: 0,
  };
  const cache = new SkyVisibilityCache();
  try {
    cache.apply(scene);
    await cache.waitReady();
    cache.apply(scene);
    expect(scene.surfaces.some((s) => s.mesh.skyVisibility)).toBe(true);
    expect(scene.indirectLighting).toBeUndefined();
    const built = cache.builds;
    scene.environment.sunIntensity *= 0.5;
    scene.camera.position[0] += 0.2;
    cache.apply(scene);
    expect(cache.builds).toBe(built);
    scene.origin = [16, 0, -32];
    scene.surfaces = scene.surfaces.map((s) => ({
      ...s,
      matrix: Float32Array.from(
        s.matrix,
        (v, i) => v - (i >= 12 && i < 15 ? (scene.origin?.[i - 12] ?? 0) : 0),
      ),
    }));
    cache.apply(scene);
    expect(cache.builds).toBe(built);
    const s = scene.surfaces[0];
    s.mesh = { ...s.mesh, normals: s.mesh.normals.slice() };
    cache.apply(scene);
    expect(cache.builds).toBe(built + 1);
    expect(scene.surfaces.every((s) => !s.mesh.skyVisibility)).toBe(true);
    cache.dispose();
    await expect(cache.waitReady()).rejects.toThrow();
  } finally {
    cache.dispose();
  }
});

test("open surfaces retain their sky, topology and packing; dynamic receivers are excluded", () => {
  const source = indirectBoxFixture({ occluder: false }).surfaces[0];
  const steps = compileSkyVisibilitySteps([source, { ...source, id: "moving", lightingMobility: "dynamic" }]);
  let item = steps.next();
  while (!item.done) item = steps.next();
  const mesh = item.value.meshes.get(source.id);
  if (!mesh?.skyVisibility) throw Error("Missing open sky");
  expect(mesh.positions).toBe(source.mesh.positions);
  expect(mesh.indices).toBe(source.mesh.indices);
  const packed = packVertices(mesh),
    count = mesh.positions.length / 3,
    stride = packed.length / count;
  for (let i = 0; i < count; i++) {
    expect(Array.from(mesh.skyVisibility.slice(i * 4, i * 4 + 4))).toEqual([0, 1, 0, 1]);
    expect(Array.from(packed.slice(i * stride + 13, i * stride + 17))).toEqual([0, 1, 0, 1]);
  }
  expect(item.value.meshes.has("moving")).toBe(false);
  expect(item.value.report.excluded).toEqual([{ id: "moving", reason: "dynamic geometry excluded" }]);
});

test("distance-only sky queries preserve closest-hit visibility, including the fade annulus", () => {
  const f = indirectBoxFixture({ ceiling: true, front: true, occluder: true });
  const geometry = compileIndirectGeometry(f.surfaces);
  const fade = (d: number, r: number) => {
    const t = Math.max(0, Math.min(1, (d - r * 0.75) / (r * 0.25)));
    return t * t * (3 - 2 * t);
  };
  for (let i = 0; i < 120; i++) {
    const origin: Vec3 = [Math.sin(i * 1.7) * 4, Math.cos(i * 0.71) * 3, Math.sin(i * 0.13) * 4];
    const raw = [Math.cos(i * 0.23), Math.sin(i * 0.53), Math.cos(i * 0.17)],
      length = Math.hypot(...raw);
    const direction = raw.map((x) => x / length) as Vec3,
      radius = 1 + (i % 7);
    const reference = traceIndirectRay(geometry, origin, direction, radius)?.distance ?? radius;
    expect(fade(traceSkyDistance(geometry, origin, direction, radius), radius)).toBeCloseTo(
      fade(reference, radius),
      12,
    );
  }
});

test("alternate realizations cannot consume a mesh sky stream", () => {
  const surface = indirectBoxFixture({ occluder: false }).surfaces[0];
  surface.mesh = {
    ...surface.mesh,
    skyVisibility: new Float32Array((surface.mesh.positions.length / 3) * 4),
  };
  expect(packSurface(surface)[LOCAL_LIGHT_OFFSET + 3]).toBe(3);
  surface.selectedRenderProduct = { kind: "analytic-quadric" } as RenderSurface["selectedRenderProduct"];
  expect(packSurface(surface)[LOCAL_LIGHT_OFFSET + 3]).toBe(0);
});

test("all resident blockers invalidate sky until directional dependency reuse is certified", async () => {
  const f = indirectBoxFixture({ occluder: false });
  const near = f.surfaces[0],
    far = { ...near, id: "distant", matrix: near.matrix.slice() };
  far.matrix[12] = 100;
  const scene: EvaluatedScene = {
    ...f,
    surfaces: [near, far],
    environment: evaluateEnvironment(),
    mode: "beauty",
    grid: false,
    time: 0,
  };
  const cache = new SkyVisibilityCache();
  try {
    cache.apply(scene);
    await cache.waitReady();
    cache.apply(scene);
    const original = scene.surfaces[0].mesh;
    scene.surfaces[1].matrix[12] += 1;
    cache.apply(scene);
    await cache.waitReady();
    cache.apply(scene);
    expect(scene.surfaces[0].mesh).not.toBe(original);
    expect(cache.report?.reusedVertices).toBe(0);
    scene.surfaces.push(f.surfaces[1]);
    cache.apply(scene);
    await cache.waitReady();
    cache.apply(scene);
    expect(scene.surfaces[0].mesh).not.toBe(original);
    const report = cache.report;
    if (!report) throw Error("Missing rebuilt sky report");
    expect(report.rays).toBeGreaterThan(0);
    expect(cache.byteLength).toBe(report.bytes);
  } finally {
    cache.dispose();
  }
});

test("receiver budgets count the full allocated stream even for a tiny draw range", () => {
  const floor = indirectBoxFixture({ occluder: false }).surfaces[0];
  const positions = new Float32Array(65000 * 3),
    normals = new Float32Array(65000 * 3);
  positions.set(floor.mesh.positions);
  normals.set(floor.mesh.normals);
  const large = {
    ...floor,
    id: "large",
    mesh: { ...floor.mesh, positions, normals },
    drawRange: { start: 0, count: 3 },
  };
  const second = {
    ...floor,
    id: "second",
    mesh: { ...floor.mesh, positions: new Float32Array(1000 * 3), normals: new Float32Array(1000 * 3) },
  };
  const steps = compileSkyVisibilitySteps([large, second]);
  let result = steps.next();
  while (!result.done) result = steps.next();
  expect(result.value.report.vertices).toBe(65000);
  expect(result.value.report.bytes).toBe(65000 * 16);
  expect(result.value.meshes.has("second")).toBe(false);
  expect(result.value.report.excluded).toContainEqual({ id: "second", reason: "sky receiver budget" });
});

test("a refused geometry budget publishes no incomplete lighting", async () => {
  const f = indirectBoxFixture({ occluder: false });
  const surface = f.surfaces[0];
  surface.mesh = { ...surface.mesh, indices: new Uint32Array(600003) };
  const scene: EvaluatedScene = {
    ...f,
    surfaces: [surface],
    environment: evaluateEnvironment(),
    mode: "beauty",
    grid: false,
    time: 0,
  };
  const cache = new SkyVisibilityCache();
  try {
    cache.apply(scene);
    await expect(cache.waitReady()).rejects.toThrow("exceeds 200000 triangle budget");
    expect(cache.error).toContain("whole build refused");
    expect(cache.byteLength).toBe(0);
    expect(scene.surfaces[0].mesh.skyVisibility).toBeUndefined();
  } finally {
    cache.dispose();
  }
});
