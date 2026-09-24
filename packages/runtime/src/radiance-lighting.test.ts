import { expect, test } from "bun:test";
import type { EvaluatedScene } from "@wrela/model";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { evaluateEnvironment } from "./environment";
import { RadianceLightingCache } from "./radiance-lighting";
import { SkyVisibilityCache } from "./sky-visibility";

function scene(): EvaluatedScene {
  const f = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
  return {
    ...f,
    camera: { position: [0, 1, 0], target: [0, 1, -1], fov: 65 },
    environment: evaluateEnvironment(),
    mode: "beauty",
    grid: false,
    time: 0,
  };
}

test("uniform emissive brightness edits reuse normalized transport; relative source changes rebuild", async () => {
  const s = scene(),
    cache = new RadianceLightingCache(null);
  s.surfaces[0].material.emission = { color: [1, 0.4, 0.1], intensity: 0.5 };
  s.surfaces[1].material.emission = { color: [0.2, 0.5, 1], intensity: 0.25 };
  const apply = () => {
    cache.strip(s);
    cache.apply(s);
  };
  try {
    apply();
    await cache.waitReady();
    apply();
    const first = s.radianceLighting!,
      builds = cache.builds;
    const transfer = first.transfer.slice(),
      emission = first.receiverEmission!.slice();
    expect(first.emissionScale).toBe(0.5);
    for (const surface of s.surfaces) if (surface.material.emission) surface.material.emission.intensity *= 8;
    apply();
    expect(cache.builds).toBe(builds);
    expect(s.radianceLighting).toBe(first);
    expect(first.emissionScale).toBe(4);
    expect(first.transfer).toEqual(transfer);
    expect(first.receiverEmission).toEqual(emission);
    const strengths = s.surfaces.map((surface) => surface.material.emission?.intensity);
    for (const surface of s.surfaces) if (surface.material.emission) surface.material.emission.intensity = 0;
    apply();
    expect(cache.builds).toBe(builds);
    expect(s.radianceLighting).toBe(first);
    expect(first.emissionScale).toBe(0);
    s.surfaces.forEach((surface, i) => {
      if (surface.material.emission) surface.material.emission.intensity = strengths[i]!;
    });
    apply();
    expect(cache.builds).toBe(builds);
    expect(first.emissionScale).toBe(4);
    s.surfaces[1].material.emission!.intensity *= 2;
    apply();
    expect(cache.builds).toBe(builds + 1);
    expect(s.radianceLighting).toBeUndefined();
    await cache.waitReady();
    apply();
    expect(s.radianceLighting!.key).not.toBe(first.key);
    s.surfaces[0].material.emission!.color = [0, 1, 0];
    apply();
    expect(cache.builds).toBe(builds + 2);
  } finally {
    cache.dispose();
  }
});

test("an initially dormant emitter compiles a reusable unit basis without lighting the scene", async () => {
  const s = scene(),
    cache = new RadianceLightingCache(null);
  s.surfaces[0].material.emission = { color: [1, 0.4, 0.1], intensity: 0 };
  try {
    cache.apply(s);
    await cache.waitReady();
    cache.apply(s);
    const field = s.radianceLighting!,
      builds = cache.builds;
    expect(field.emissionScale).toBe(0);
    expect(field.receiverEmission!.some((v) => v > 0)).toBe(true);
    s.surfaces[0].material.emission.intensity = 2;
    cache.strip(s);
    cache.apply(s);
    expect(cache.builds).toBe(builds);
    expect(s.radianceLighting).toBe(field);
    expect(field.emissionScale).toBe(2);
  } finally {
    cache.dispose();
  }
});
test("automatic caches compose without identity churn; radiometric edits, motion and rebases reuse geometry", async () => {
  const s = scene(),
    cache = new RadianceLightingCache(),
    sky = new SkyVisibilityCache();
  const moving = {
    ...s.surfaces[0],
    id: "moving",
    matrix: s.surfaces[0].matrix.slice(),
    lightingMobility: "dynamic" as const,
  };
  moving.matrix[13] = 1;
  s.surfaces.push(moving);
  const apply = () => {
    cache.strip(s);
    sky.apply(s);
    cache.apply(s);
  };
  try {
    apply();
    await Promise.all([cache.waitReady(), sky.waitReady()]);
    apply();
    const key = s.radianceLighting!.key,
      builds = cache.builds,
      skyBuilds = sky.builds;
    expect(s.surfaces[0].mesh.radianceProbes).toBeDefined();
    expect(s.surfaces[0].mesh.skyVisibility).toBeDefined();
    expect(s.surfaces.at(-1)!.radianceProbes![0]).toBeGreaterThan(0);
    expect(s.surfaces.at(-1)!.mesh.radianceProbes).toBeUndefined();
    expect(s.surfaces.at(-1)!.radianceWeights!.reduce((sum, w) => sum + w, 0)).toBeCloseTo(1, 6);
    s.environment.sunIntensity = 0;
    s.environment.ambient = 0.4;
    s.surfaces.at(-1)!.matrix[12] += 0.1;
    apply();
    expect(cache.builds).toBe(builds);
    expect(sky.builds).toBe(skyBuilds);
    s.origin = [32, 0, -16];
    s.camera.position = [-32, 1, 16];
    for (const surface of s.surfaces) {
      surface.matrix[12] -= 32;
      surface.matrix[14] += 16;
    }
    apply();
    expect(cache.builds).toBe(builds);
    expect(s.radianceLighting!.key).toBe(key);
    s.surfaces[0].material = { ...s.surfaces[0].material, color: [0.1, 0.2, 0.3] };
    apply();
    expect(cache.builds).toBe(builds + 1);
    expect(s.radianceLighting).toBeUndefined();
  } finally {
    cache.dispose();
    sky.dispose();
  }
});
test("independent scenes have distinct GPU identities; disposal cancels a pending build", async () => {
  const a = new RadianceLightingCache(),
    b = new RadianceLightingCache(),
    sa = scene(),
    sb = scene();
  try {
    a.apply(sa);
    b.apply(sb);
    await Promise.all([a.waitReady(), b.waitReady()]);
    a.apply(sa);
    b.apply(sb);
    expect(sa.radianceLighting!.key).not.toBe(sb.radianceLighting!.key);
    sa.surfaces[0].matrix[12] += 1;
    a.strip(sa);
    a.apply(sa);
    a.dispose();
    await expect(a.waitReady()).rejects.toThrow();
    expect(a.byteLength).toBe(0);
  } finally {
    a.dispose();
    b.dispose();
  }
});

test("moving lights preserve valid sky transport and coalesce pending rebuilds", async () => {
  const cache = new RadianceLightingCache(),
    s = scene();
  s.environment.pointLights = [{ position: [0, 1, 0], color: [1, 1, 1], intensity: 1, range: 8 }];
  try {
    cache.apply(s);
    await cache.waitReady();
    cache.apply(s);
    const old = s.radianceLighting,
      builds = cache.builds;
    s.environment.pointLights[0].position[0] += 0.1;
    cache.strip(s);
    cache.apply(s);
    expect(s.radianceLighting).toBe(old);
    expect(cache.builds).toBe(builds + 1);
    s.environment.pointLights[0].position[0] += 0.1;
    cache.strip(s);
    cache.apply(s);
    expect(s.radianceLighting).toBe(old);
    expect(cache.builds).toBe(builds + 1);
    await cache.waitReady();
    cache.strip(s);
    cache.apply(s);
    expect(cache.builds).toBe(builds + 2);
  } finally {
    cache.dispose();
  }
});

test("camera regions have hysteresis, reuse geometry and samples, and retain a bounded return path", async () => {
  const cache = new RadianceLightingCache(),
    s = scene();
  const apply = () => {
    cache.strip(s);
    cache.apply(s);
  };
  try {
    apply();
    await cache.waitReady();
    apply();
    const originalMesh = s.surfaces[0].mesh;
    const original = s.radianceLighting,
      initial = cache.builds;
    s.camera.position[0] = 9;
    apply();
    expect(cache.builds).toBe(initial);
    s.camera.position[0] = 13;
    apply();
    await cache.waitReady();
    apply();
    expect(cache.builds).toBe(initial + 1);
    expect(cache.report?.reusedGeometry).toBe(true);
    expect(cache.report?.reusedSamples).toBeGreaterThan(0);
    s.camera.position[0] = 0;
    apply();
    expect(cache.builds).toBe(initial + 1);
    expect(cache.regionCacheHits).toBe(1);
    expect(s.radianceLighting).toBe(original);
    expect(s.surfaces[0].mesh).toBe(originalMesh);
    for (const x of [32, 48, 64, 80]) {
      s.camera.position[0] = x;
      apply();
      await cache.waitReady();
    }
    expect(cache.retainedRegions).toBeLessThanOrEqual(4);
    expect(cache.byteLength).toBeLessThanOrEqual(32 * 1024 * 1024);
    s.surfaces[0].material = { ...s.surfaces[0].material, color: [0.2, 0.3, 0.4] };
    apply();
    expect(cache.retainedRegions).toBe(0);
    expect(s.radianceLighting).toBeUndefined();
  } finally {
    cache.dispose();
  }
  expect(cache.byteLength).toBe(0);
});

test("fresh caches restore persisted lighting without recompiling and use distinct GPU identities", async () => {
  const records = new Map<string, import("@wrela/compiler").CookedRadiance>();
  const store: import("./radiance-store").RadianceProductStore = {
    async get(key) {
      const value = records.get(key);
      return value && structuredClone(value);
    },
    async put(value) {
      records.set(value.source, structuredClone(value));
      return true;
    },
    async remove(key) {
      records.delete(key);
    },
  };
  const first = new RadianceLightingCache(store),
    original = scene();
  first.apply(original);
  const compiled = await first.waitReady();
  await first.waitForStorage();
  expect(first.storageWrites).toBe(1);
  first.dispose();
  const second = new RadianceLightingCache(store),
    fresh = scene();
  try {
    second.apply(fresh);
    const restored = await second.waitReady();
    second.apply(fresh);
    expect(second.builds).toBe(0);
    expect(second.storageHits).toBe(1);
    expect(restored.field.key).not.toBe(compiled.field.key);
    expect(restored.field.transfer).toEqual(compiled.field.transfer);
    expect(fresh.surfaces[0].mesh.radianceProbes).toEqual(
      compiled.meshes.get(fresh.surfaces[0].id)?.radianceProbes,
    );
    second.strip(fresh);
    fresh.surfaces[0].material = { ...fresh.surfaces[0].material, color: [0.3, 0.4, 0.5] };
    second.apply(fresh);
    await second.waitReady();
    expect(second.builds).toBe(1);
    expect(second.storageHits).toBe(1);
  } finally {
    second.dispose();
  }
});

test("storage failure and corrupt records fall back to compilation without a lighting error", async () => {
  for (const unavailable of [true, false]) {
    let removed = 0;
    const store: import("./radiance-store").RadianceProductStore = {
      async get() {
        if (unavailable) throw Error("storage disabled");
        return {} as import("@wrela/compiler").CookedRadiance;
      },
      async put() {
        throw Error("quota full");
      },
      async remove() {
        removed++;
      },
    };
    const cache = new RadianceLightingCache(store);
    try {
      cache.apply(scene());
      await cache.waitReady();
      await cache.waitForStorage();
      expect(cache.builds).toBe(1);
      expect(cache.storageHits).toBe(0);
      expect(cache.error).toBeUndefined();
      expect(cache.storageError).toBeDefined();
      expect(removed).toBe(1);
    } finally {
      cache.dispose();
    }
  }
});

test("observed camera travel prepares the neighbor without replacing current light; crossing consumes it without rebuilding", async () => {
  const cache = new RadianceLightingCache(null),
    s = scene();
  const apply = () => {
    cache.strip(s);
    cache.apply(s);
  };
  try {
    apply();
    await cache.waitReady();
    apply();
    const original = s.radianceLighting,
      builds = cache.builds;
    s.camera.target[0] += 1;
    apply();
    expect(cache.prefetchBuilds).toBe(0);
    s.camera.position[0] = 2;
    apply();
    expect(cache.prefetchBuilds).toBe(1);
    expect(s.radianceLighting).toBe(original);
    await cache.waitForPrefetch();
    apply();
    expect(s.radianceLighting).toBe(original);
    s.camera.position[0] = 13;
    apply();
    expect(cache.builds).toBe(builds);
    expect(cache.prefetchHits).toBe(1);
    expect(s.radianceLighting).not.toBe(original);
    s.camera.position[0] = 0;
    apply();
    expect(s.radianceLighting).toBe(original);
  } finally {
    cache.dispose();
  }
});

test("crossing during preparation promotes the existing job, while geometry edits cancel stale preparation", async () => {
  const cache = new RadianceLightingCache(null),
    s = scene();
  const apply = () => {
    cache.strip(s);
    cache.apply(s);
  };
  try {
    apply();
    await cache.waitReady();
    apply();
    const builds = cache.builds;
    s.camera.position[0] = 2;
    apply();
    s.camera.position[0] = 13;
    apply();
    await cache.waitReady();
    apply();
    expect(cache.builds).toBe(builds);
    expect(cache.prefetchBuilds).toBe(1);
    expect(cache.prefetchHits).toBe(1);
    s.camera.position[0] = 18;
    apply();
    const pending = cache.waitForPrefetch();
    s.surfaces[0].material = { ...s.surfaces[0].material, color: [0.1, 0.3, 0.7] };
    apply();
    expect(s.radianceLighting).toBeUndefined();
    await pending;
    await cache.waitReady();
    apply();
    expect(cache.retainedRegions).toBe(1);
    expect(cache.builds).toBe(builds + 1);
  } finally {
    cache.dispose();
  }
});
