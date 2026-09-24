import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type MeshData, type RenderSurface } from "@wrela/model";

import { INSTANCE_FLOATS, packInstances } from "./batching";
import { ShootSelector } from "./shoot-selection";

function fixture(): EvaluatedScene {
  const transforms = new Float32Array(32);
  transforms.set(identityMatrix());
  transforms.set(identityMatrix(), 16);
  transforms[30] = -30;
  const mesh: MeshData = {
    positions: new Float32Array([0, 0, 0]),
    normals: new Float32Array([0, 1, 0]),
    indices: new Uint32Array([0, 0, 0]),
    bounds: { min: [-0.1, -0.1, -30.1], max: [0.1, 0.1, 0.1] },
    shoots: {
      transforms,
      anchors: new Float32Array(8),
      motion: new Float32Array(8),
      sourceIds: ["pine/close", "pine/far"],
      templateBounds: { min: [-0.1, -0.1, -0.1], max: [0.1, 0.1, 0.1] },
    },
  };
  const surface: RenderSurface = {
    id: "tree",
    source: "pine",
    mesh,
    details: [{ label: "projected-shoot", mesh: { ...mesh }, maxProjectedDiameter: 96, maxError: null }],
    matrix: identityMatrix(),
    material: {
      color: [1, 1, 1],
      secondary: [1, 1, 1],
      roughness: 1,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
  return {
    surfaces: [surface],
    camera: { position: [0, 0, 1], target: [0, 0, 0], fov: 50 },
    time: 0,
    grid: false,
    mode: "beauty",
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
  };
}

test("each shoot chooses camera and sun detail independently with complete pass ownership", () => {
  const scene = fixture(),
    selector = new ShootSelector();
  selector.begin();
  const selected = selector.select(scene, scene.surfaces[0], 1080, 0.1);
  const resolved = selected.find((s) => s.id.endsWith("/resolved"))!,
    projected = selected.find((s) => s.id.endsWith("/projected"))!;
  expect(Array.from(resolved.shootSelection!.indices)).toEqual([0]);
  expect(Array.from(resolved.shootSelection!.masks)).toEqual([1]);
  expect(Array.from(projected.shootSelection!.indices)).toEqual([0, 1]);
  expect(Array.from(projected.shootSelection!.masks)).toEqual([2, 3]);
  for (const i of [0, 1])
    for (const pass of [1, 2])
      expect(
        selected.reduce((n, s) => {
          const at = s.shootSelection!.indices.indexOf(i);
          return n + Number(at >= 0 && !!(s.shootSelection!.masks[at] & pass));
        }, 0),
      ).toBe(1);
  const tinyShadow = selector.select(scene, scene.surfaces[0], 1080, 0.0001);
  expect(tinyShadow.find((s) => s.id.endsWith("/resolved"))!.shootSelection!.indices.length).toBe(2);
});

test("packed shoot records compose transforms, stable identity and independent masks", () => {
  const scene = fixture(),
    surface = scene.surfaces[0];
  surface.matrix[12] = 7;
  surface.shootSelection = { indices: new Uint32Array([1]), masks: new Uint8Array([2]) };
  const packed = packInstances([surface], () => identityMatrix());
  expect(packed.length).toBe(INSTANCE_FLOATS);
  expect(packed[12]).toBe(7);
  expect(packed[14]).toBe(-30);
  expect(packed[32]).toBe(0);
  expect(packed[34]).toBe(-30);
  expect(packed[19]).toBe(1);
  expect(packed[60]).toBe(6);
});

test("subtree review remains restricted through detail selection", () => {
  const scene = fixture(),
    surface = scene.surfaces[0];
  surface.shootSelection = { indices: new Uint32Array([1]), masks: new Uint8Array([3]) };
  const selector = new ShootSelector();
  selector.begin();
  const result = selector.select(scene, surface, 1080, 0.1);
  expect(result.flatMap((s) => Array.from(s.shootSelection!.indices))).toEqual([1]);
});

test("stationary selections and instance packets survive camera and wind updates", async () => {
  const { InstancePacketCache } = await import("./batching");
  const scene = fixture(),
    selector = new ShootSelector(),
    cache = new InstancePacketCache();
  selector.begin();
  const a = selector.select(scene, scene.surfaces[0], 1080, 0.1);
  const first = cache.pack(a, (s) => s.matrix);
  scene.time = 8;
  scene.camera.position[0] += 0.001;
  selector.begin();
  const b = selector.select(scene, scene.surfaces[0], 1080, 0.1);
  expect(b[0].shootSelection).toBe(a[0].shootSelection);
  expect(cache.pack(b, (s) => s.matrix)).toBe(first);
  b[0].matrix = b[0].matrix.slice();
  b[0].matrix[12] = 2;
  expect(cache.pack(b, (s) => s.matrix)).not.toBe(first);
  const moved = cache.pack(b, (s) => s.matrix);
  expect(cache.pack(b, () => undefined)).not.toBe(moved);
});

test("camera rejection retains offscreen sun casters and motion envelopes", () => {
  const scene = fixture(),
    selector = new ShootSelector(),
    surface = scene.surfaces[0];
  surface.matrix[12] = 100;
  selector.begin();
  const selected = selector.select(scene, surface, 1080, 0.1, 16 / 9);
  expect(selected.every((s) => s.shootSelection!.masks.every((mask) => (mask & 1) === 0))).toBe(true);
  expect(selected.flatMap((s) => Array.from(s.shootSelection!.indices)).sort()).toEqual([0, 1]);
});
test("whole-group distant proof agrees with individually selected occurrences", () => {
  const scene = fixture(),
    surface = scene.surfaces[0],
    selector = new ShootSelector();
  surface.matrix[14] = -50;
  const fast = selector.select(scene, surface, 1080, 0.1);
  const individual = selector.select(
    scene,
    { ...surface, shootSelection: { indices: new Uint32Array([0, 1]), masks: new Uint8Array([3, 3]) } },
    1080,
    0.1,
  );
  expect(
    fast.map((s) => [s.id, Array.from(s.shootSelection!.indices), Array.from(s.shootSelection!.masks)]),
  ).toEqual(
    individual.map((s) => [s.id, Array.from(s.shootSelection!.indices), Array.from(s.shootSelection!.masks)]),
  );
});

test("a representation change invalidates only newly visible occurrences", () => {
  const scene = fixture(),
    surface = scene.surfaces[0];
  surface.shootSelection = { indices: new Uint32Array([0, 1]), masks: new Uint8Array([3, 3]) };
  const before = { indices: new Uint32Array([0, 1]), masks: new Uint8Array([1, 2]) };
  const packed = packInstances(
    [surface],
    () => surface.matrix,
    () => before,
  );
  expect(packed[19]).toBe(1);
  expect(packed[INSTANCE_FLOATS + 19]).toBe(0);
});

test("finite local lights retain detailed casters only within their support", () => {
  const scene = fixture(),
    surface = scene.surfaces[0],
    selector = new ShootSelector();
  scene.environment.pointLights = [{ position: [0, 0, 0], color: [1, 1, 1], intensity: 1, range: 2 }];
  const selected = selector.select(scene, surface, 1080, 0.1);
  const shadowOwner = (index: number) =>
    selected.filter((s) => {
      const at = s.shootSelection!.indices.indexOf(index);
      return at >= 0 && s.shootSelection!.masks[at] & 2;
    });
  expect(shadowOwner(0).map((s) => s.id)).toEqual(["tree/resolved"]);
  expect(shadowOwner(1).map((s) => s.id)).toEqual(["tree/projected"]);
  scene.environment.pointLights[0].range = undefined;
  const unbounded = selector.select(scene, surface, 1080, 0.1);
  expect(
    Array.from(unbounded.find((s) => s.id.endsWith("/resolved"))!.shootSelection!.masks).every((m) => m & 2),
  ).toBe(true);
});

test("local-light detail admission includes world transforms and maximum wind travel", () => {
  const scene = fixture(),
    surface = scene.surfaces[0],
    selector = new ShootSelector();
  surface.matrix[12] = 10;
  surface.matrix[0] = 2;
  surface.wind = 1;
  scene.environment.wind = [10, 0, 0];
  scene.environment.pointLights = [{ position: [11.7, 0, 0], color: [1, 1, 1], intensity: 1, range: 0.5 }];
  const moving = selector.select(scene, surface, 1080, 0.1);
  expect(moving.find((s) => s.id.endsWith("/resolved"))!.shootSelection!.masks[0] & 2).toBe(2);
  surface.wind = 0;
  const still = selector.select(scene, surface, 1080, 0.1);
  expect(still.find((s) => s.id.endsWith("/projected"))!.shootSelection!.masks[0] & 2).toBe(2);
  scene.environment.pointLights[0].position = [0, 0, 0];
  expect(
    selector
      .select(scene, surface, 1080, 0.1)
      .filter((s) => s.id.endsWith("/resolved"))
      .every((s) => s.shootSelection!.masks.every((m) => !(m & 2))),
  ).toBe(true);
});

test("scaled shoot occurrences retain local-light casters over their full motion envelope", () => {
  const scene = fixture(),
    surface = scene.surfaces[0],
    selector = new ShootSelector();
  for (const axis of [0, 5, 10]) surface.mesh.shoots!.transforms[axis] = 4;
  surface.wind = 1;
  scene.environment.wind = [10, 0, 0];
  scene.environment.pointLights = [{ position: [3, 0, 0], color: [1, 1, 1], intensity: 1, range: 0.2 }];
  const result = selector.select(scene, surface, 1080, 0.1);
  const resolved = result.find((s) => s.id.endsWith("/resolved"))!;
  const at = resolved.shootSelection!.indices.indexOf(0);
  expect(at).toBeGreaterThanOrEqual(0);
  expect(resolved.shootSelection!.masks[at] & 2).toBe(2);
});
