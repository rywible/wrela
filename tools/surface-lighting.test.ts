import { expect, test } from "bun:test";
import {
  compileIndirectGeometry,
  indirectProbeWeights,
  indirectSurfaceCacheSteps,
  surfaceTileVisibility,
} from "@wrela/compiler";
import { type EvaluatedScene, identityMatrix, type RenderSurface, type Vec3 } from "@wrela/model";
import { batchSurfaces } from "@wrela/render-webgpu/batching";
import {
  indirectLightingBytes,
  packIndirectLighting,
  withIndirectReceivers,
} from "@wrela/render-webgpu/indirect";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { indirectBoxFixture } from "./fixtures/indirect-scenes";

function exhaust<T>(steps: Generator<void, T>): T {
  let r = steps.next();
  while (!r.done) r = steps.next();
  return r.value;
}
function scene(): EvaluatedScene {
  const fixture = indirectBoxFixture();
  return {
    surfaces: fixture.surfaces,
    camera: fixture.camera,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
}

test("instances sharing geometry cannot consume another receiver's lighting chart", () => {
  const floor = scene().surfaces[0];
  const inputs = [
    { ...floor, indirectSurfaceChart: 1 },
    { ...floor, id: "other-floor", indirectSurfaceChart: 2 },
    floor,
  ];
  expect(batchSurfaces(inputs, () => 1).map((b) => b.surfaces.length)).toEqual([1, 1, 1]);
  expect(batchSurfaces([floor, { ...floor, id: "uncached-floor" }], () => 1)).toHaveLength(1);
});
async function build(s = scene(), physicalSky = false) {
  const cache = new IndirectLightingCache();
  const options = {
    dimensions: [8, 6, 8] as Vec3,
    samples: 64,
    skySamples: 4,
    physicalSky,
    surfaceCache: { spacing: 0.04, radiance: false },
  };
  cache.update(s, options);
  return { cache, field: await cache.waitReady(), options, s };
}

test("surface cache uses whole-tile visibility and bounded interpolation, preserves packed offsets and origin", async () => {
  const { cache, field, s } = await build();
  const product = field.surfaceCache;
  if (!product) throw Error("Missing compiled surface cache");
  expect(product.report.patches).toBe(4);
  expect(product.report.admitted / product.report.tiles).toBeGreaterThan(0.6);
  expect(product.data.byteLength).toBeLessThan(500_000);
  expect(product.report.excluded.some((v) => v.id === "neutral-block")).toBe(true);
  const d = product.data;
  let worst = 0,
    checked = 0;
  // Interior points distinct from the compiler's midpoint quality samples.
  for (let patch = 0; patch < product.sources.length; patch++) {
    const h = 8 + patch * 16,
      nx = d[h + 3],
      ny = d[h + 7];
    const n = Array.from(d.subarray(h + 12, h + 15)) as Vec3;
    const u = Array.from(d.subarray(h + 4, h + 7)),
      v = Array.from(d.subarray(h + 8, h + 11));
    const u2 = u.reduce((sum, x) => sum + x * x, 0),
      v2 = v.reduce((sum, x) => sum + x * x, 0);
    for (let y = 0; y < ny - 1; y++)
      for (let x = 0; x < nx - 1; x++) {
        if (!d[d[3] * 4 + d[h + 15] + x + (nx - 1) * y]) continue;
        for (const [fx, fy] of [
          [0.211, 0.733],
          [0.817, 0.147],
        ]) {
          const p = [0, 1, 2].map(
            (a) => field.origin[a] + d[h + a] + (u[a] * (x + fx)) / u2 + (v[a] * (y + fy)) / v2,
          ) as Vec3;
          const actual = indirectProbeWeights(field, p, n);
          if (!actual) throw Error("Admitted tile outside the probe field");
          const weights = [(1 - fx) * (1 - fy), fx * (1 - fy), (1 - fx) * fy, fx * fy];
          let error = 0;
          for (let c = 0; c < 8; c++) {
            const value = [0, 1, nx, nx + 1].reduce(
              (sum, o, k) => sum + d[d[2] * 4 + (d[h + 11] + x + nx * y + o) * 8 + c] * weights[k],
              0,
            );
            error += Math.abs(value - actual.weights[c]);
          }
          worst = Math.max(worst, error);
          checked++;
        }
      }
  }
  expect(checked).toBeGreaterThan(10_000);
  expect(worst).toBeLessThan(0.05);
  const packed = packIndirectLighting(field),
    partial = packIndirectLighting(field, [0, 0, 0], false);
  expect(packed.byteLength).toBe(indirectLightingBytes(field));
  expect([...partial]).toEqual([...packed.subarray(0, partial.length)]);
  expect(packed[12] * 4).toBe(
    16 + field.data.length + (field.reflections?.data.length ?? 0) + product.data.length,
  );
  const marked = withIndirectReceivers({ ...s, indirectLighting: field });
  expect(marked.surfaces[0].indirectSurfaceChart).toBe(1);
  const malformed = product.data.slice();
  malformed[2] += 1;
  expect(() => packIndirectLighting({ ...field, surfaceCache: { ...product, data: malformed } })).toThrow();
  const radiance = exhaust(
    indirectSurfaceCacheSteps(compileIndirectGeometry(s.surfaces), field, s.surfaces, { spacing: 0.08 }),
  );
  expect(radiance.data[5]).toBeGreaterThan(0);
  const withRadiance = { ...field, surfaceCache: radiance };
  expect(packIndirectLighting(withRadiance).byteLength).toBe(indirectLightingBytes(withRadiance));
  for (const edited of [
    { ...s.surfaces[0], material: { ...s.surfaces[0].material, normalStrength: 0.1 } },
    { ...s.surfaces[0], mesh: { ...s.surfaces[0].mesh, normals: s.surfaces[0].mesh.normals.slice() } },
    { ...s.surfaces[0], lightingMobility: "dynamic" as const },
  ])
    expect(
      withIndirectReceivers({ ...s, indirectLighting: field, surfaces: [edited] }).surfaces[0]
        .indirectSurfaceChart,
    ).toBeUndefined();
  expect(
    withIndirectReceivers({ ...marked, indirectLighting: undefined }).surfaces[0].indirectSurfaceChart,
  ).toBeUndefined();
  cache.dispose();
});

test("a tiny blocker between cache samples cannot be certified clear", () => {
  const blocker: RenderSurface = {
    ...indirectBoxFixture().surfaces[0],
    id: "tiny-blocker",
    matrix: identityMatrix(),
    mesh: {
      positions: new Float32Array([-0.02, 0.5, -0.02, 0.02, 0.5, -0.02, 0, 0.5, 0.02]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 1, 2]),
      bounds: { min: [-0.02, 0.5, -0.02], max: [0.02, 0.5, 0.02] },
    },
  };
  const geometry = compileIndirectGeometry([blocker]);
  const tile: Vec3[] = [
    [-0.1, 0, -0.1],
    [0.1, 0, -0.1],
    [-0.1, 0, 0.1],
    [0.1, 0, 0.1],
  ];
  expect(surfaceTileVisibility(geometry, tile, [0, 1, 0], [0, 1, 0])).toBeUndefined();
  const empty = compileIndirectGeometry([]);
  expect(surfaceTileVisibility(empty, tile, [0, 1, 0], [0, 1, 0])).toBe(true);
  const covered = {
    ...blocker,
    mesh: { ...blocker.mesh, positions: new Float32Array([-2, 0.5, -2, 2, 0.5, -2, 0, 0.5, 2]) },
  };
  expect(surfaceTileVisibility(compileIndirectGeometry([covered]), tile, [0, 1, 0], [0, 1, 0])).toBe(false);
});

test("cache is budgeted, relightable, cancels safely and invalidates changed receiver normals", async () => {
  const { cache, field, options, s } = await build(scene(), true);
  s.environment.sunIntensity *= 0.25;
  s.environment.skyColor = [0.8, 0.1, 0.05];
  expect(cache.update(s, options)).toBe(field);
  const data = field.surfaceCache?.data;
  expect(cache.update(s, options)?.surfaceCache?.data).toBe(data);
  const low = exhaust(
    indirectSurfaceCacheSteps(compileIndirectGeometry(s.surfaces), field, s.surfaces, { maxSamples: 4 }),
  );
  expect(low.report.samples).toBe(0);
  expect(low.report.excluded.length).toBe(s.surfaces.length);
  expect(() =>
    exhaust(
      indirectSurfaceCacheSteps(compileIndirectGeometry(s.surfaces), field, s.surfaces, { spacing: NaN }),
    ),
  ).toThrow();
  const changed = {
    ...s,
    surfaces: s.surfaces.map((v) => ({ ...v, mesh: { ...v.mesh, normals: v.mesh.normals.slice() } })),
  };
  expect(cache.update(changed, options)).toBeUndefined();
  cache.dispose();
  await expect(cache.waitReady()).rejects.toThrow();
});

test("absolute chart coordinates survive large origins; uncertain reflected transforms fall back", async () => {
  const s = scene();
  s.origin = [1e7, 0, -1e7];
  const { cache, field } = await build(s);
  expect(field.surfaceCache?.report.patches).toBe(4);
  expect(field.surfaceCache?.data[8]).toBeCloseTo(0.1, 5);
  const packed = packIndirectLighting(field, s.origin);
  expect(packed[0]).toBeCloseTo(-1.1, 5);
  expect(withIndirectReceivers({ ...s, indirectLighting: field }).surfaces[0].indirectSurfaceChart).toBe(1);
  const reflected = s.surfaces[0];
  reflected.matrix = reflected.matrix.slice();
  reflected.matrix[0] = -1;
  const result = exhaust(
    indirectSurfaceCacheSteps(
      compileIndirectGeometry([reflected], { origin: s.origin }),
      field,
      [reflected],
      {},
      s.origin,
    ),
  );
  expect(result.report.patches).toBe(0);
  // A source transform changing after compile may never consume the old chart.
  expect(
    withIndirectReceivers({ ...s, indirectLighting: field }).surfaces[0].indirectSurfaceChart,
  ).toBeUndefined();
  cache.dispose();
});
