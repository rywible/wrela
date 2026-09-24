import { expect, test } from "bun:test";
import {
  createSurfaceAppearance,
  createSurfaceLayer,
  identityMatrix,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";

import { batchSurfaces } from "./batching";
import { CREATURE_MATERIAL_OFFSET } from "./creature-material";
import {
  LOCAL_LIGHT_OFFSET,
  OBJECT_FLOATS,
  packSurface,
  SURFACE_APPEARANCE_OFFSET,
  WATER_GLINT_OFFSET,
} from "./packing";
import { packSurfaceAppearance, SURFACE_APPEARANCE_FLOATS } from "./surface-appearance";

const surface = (): RenderSurface => ({
  id: "sphere",
  source: "sphere",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array(),
    normals: new Float32Array(),
    indices: new Uint32Array(),
    bounds: { min: [0, 0, 0], max: [1, 1, 1] },
  },
  material: {
    color: [1, 1, 1],
    secondary: [0, 0, 0],
    roughness: 0.8,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
});
test("surface uniforms append to stable creature/water layout without altering legacy responses", () => {
  const legacy = packSurface(surface());
  const authored = surface();
  authored.material.appearance = createSurfaceAppearance("foliage");
  const packed = packSurface(authored);
  expect(CREATURE_MATERIAL_OFFSET).toBe(152);
  expect(WATER_GLINT_OFFSET).toBe(184);
  expect(SURFACE_APPEARANCE_OFFSET).toBe(200);
  // Relief, water and local-light fields are followed by radiance bindings, emission and moving-receiver weights.
  expect(OBJECT_FLOATS).toBe(200 + SURFACE_APPEARANCE_FLOATS + 44);
  expect([...packed.slice(0, SURFACE_APPEARANCE_OFFSET)]).toEqual([
    ...legacy.slice(0, SURFACE_APPEARANCE_OFFSET),
  ]);
  expect([...legacy.slice(SURFACE_APPEARANCE_OFFSET, LOCAL_LIGHT_OFFSET)].every((value) => value === 0)).toBe(
    true,
  );
  expect(legacy[LOCAL_LIGHT_OFFSET]).toBe(255);
  expect(packed[SURFACE_APPEARANCE_OFFSET]).toBe(3);
});
test("world masks retain noise phase and height bands across rebases", () => {
  const appearance = createSurfaceAppearance();
  appearance.historyScale = 0.5;
  const layer = createSurfaceLayer("lichen");
  layer.mask.scale = 0.25;
  layer.mask.minimumHeight = 7;
  layer.mask.maximumHeight = 9;
  appearance.layers = [layer];
  const origin: Vec3 = [1000000000.125, 5, -1000000000.375];
  const packed = packSurfaceAppearance(appearance, origin, true);
  expect(packed[28 + 12]).toBe(2);
  expect(packed[28 + 13]).toBe(4);
  expect(packed[28 + 16]).toBeCloseTo((((origin[0] * 0.25) % 1024) + 1024) % 1024, 4);
  expect(packed[28 + 18]).toBeCloseTo((((origin[2] * 0.25) % 1024) + 1024) % 1024, 4);
  expect(packSurfaceAppearance(appearance, origin, false)[28 + 12]).toBe(7);
});
test("disabled layers preserve ordering without coverage and distinct appearance cannot batch together", () => {
  const appearance = createSurfaceAppearance();
  appearance.layers = [createSurfaceLayer("disabled"), createSurfaceLayer("enabled")];
  appearance.layers[0].enabled = false;
  const packed = packSurfaceAppearance(appearance);
  expect(packed[33]).toBe(0);
  expect(packed[57]).toBe(1);
  const a = surface(),
    b = surface();
  b.id = "second";
  b.material.appearance = appearance;
  expect(batchSurfaces([a, b], () => 1)).toHaveLength(2);
});

test("combined mask restrictions occupy reserved fields without changing buffer stride", () => {
  const appearance = createSurfaceAppearance();
  const layer = createSurfaceLayer("deposit");
  layer.mask = {
    ...layer.mask,
    kind: "combined",
    slopeInfluence: 0.4,
    heightInfluence: 0.7,
    slopeThreshold: 0.3,
    slopeSoftness: 0.2,
  };
  appearance.layers = [layer];
  const packed = packSurfaceAppearance(appearance);
  expect(packed.length).toBe(124);
  expect(packed[35]).toBe(4);
  expect([...packed.slice(48, 52)]).toEqual([0.4, 0.7, 0.3, 0.2].map(Math.fround));
});

test("anisotropic world detail preserves fractional frequencies and sediment phase across origin shifts", () => {
  const appearance = createSurfaceAppearance();
  appearance.detail = { kind: "wood", scale: 1, strength: 0.7 };
  const firstOrigin: Vec3 = [1024, 2048, -4096];
  const secondOrigin: Vec3 = [2048, 3072, -3072];
  const worldPoint: Vec3 = [2140.125, 3180.5, -2850.75];
  const first = packSurfaceAppearance(appearance, firstOrigin, true);
  const second = packSurfaceAppearance(appearance, secondOrigin, true);
  const periodic = (x: number, period: number) => ((x % period) + period) % period;
  for (const frequency of [0.55, 0.6, 0.8, 0.9, 1.2, 2, 5, 7, 12, 14, 24, 40]) {
    for (let axis = 0; axis < 3; axis++) {
      const phase = (packed: Float32Array, origin: Vec3) =>
        periodic(
          (worldPoint[axis] - origin[axis]) * frequency +
            (packed[24 + axis] + packed[[15, 19, 23][axis]]) * (frequency * 20),
          1024,
        );
      expect(phase(first, firstOrigin)).toBeCloseTo(phase(second, secondOrigin), 2);
    }
  }
  const sediment = (packed: Float32Array, origin: Vec3) =>
    Math.sin((worldPoint[1] - origin[1]) * 13.82 + packed[27]);
  expect(sediment(first, firstOrigin)).toBeCloseTo(sediment(second, secondOrigin), 5);
});
