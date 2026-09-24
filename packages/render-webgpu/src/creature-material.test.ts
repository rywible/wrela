import { describe, expect, test } from "bun:test";
import { type CreatureMaterial, identityMatrix, type RenderSurface } from "@wrela/model";

import { batchSurfaces } from "./batching";
import {
  CREATURE_FAMILY,
  CREATURE_MATERIAL_FLOATS,
  CREATURE_MATERIAL_OFFSET,
  creatureMaterialCoordinates,
  creatureMaterialFrame,
  packCreatureMaterial,
} from "./creature-material";
import { OBJECT_FLOATS, packSurface } from "./packing";

const surface: RenderSurface = {
  id: "creature",
  source: "creature",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array([0, 0, 0]),
    normals: new Float32Array([0, 1, 0]),
    indices: new Uint32Array([0]),
    bounds: { min: [0, 0, 0], max: [0, 0, 0] },
  },
  material: {
    color: [0.5, 0.2, 0.1],
    secondary: [0.3, 0.1, 0.1],
    roughness: 0.7,
    metallic: 0,
    pattern: 1,
    scale: 10,
    normalStrength: 0.2,
  },
};
const dot = (a: number[], b: number[]) => a.reduce((sum, value, index) => sum + value * b[index], 0);

describe("creature material interface", () => {
  test("new blocks preserve every legacy object slot and append aligned data", () => {
    const legacy = packSurface(surface);
    const skin = packSurface({ ...surface, material: { ...surface.material, creature: { family: "skin" } } });
    const creatureEnd = CREATURE_MATERIAL_OFFSET + CREATURE_MATERIAL_FLOATS;
    expect(OBJECT_FLOATS).toBeGreaterThanOrEqual(creatureEnd);
    expect(skin.length).toBe(OBJECT_FLOATS);
    expect([...skin.slice(creatureEnd)]).toEqual([...legacy.slice(creatureEnd)]);
    expect(skin.byteLength % 16).toBe(0);
    expect([...skin.slice(0, CREATURE_MATERIAL_OFFSET)]).toEqual([
      ...legacy.slice(0, CREATURE_MATERIAL_OFFSET),
    ]);
    expect(skin[CREATURE_MATERIAL_OFFSET]).toBe(CREATURE_FAMILY.skin);
    expect(legacy[CREATURE_MATERIAL_OFFSET]).toBe(CREATURE_FAMILY.hard);
    expect(legacy[CREATURE_MATERIAL_OFFSET + 12]).toBe(0);
  });
  test("family defaults select separate optical behavior", () => {
    const skin = packCreatureMaterial({ family: "skin" });
    const fiber = packCreatureMaterial({ family: "fiber" });
    const cloth = packCreatureMaterial({ family: "cloth" });
    const eye = packCreatureMaterial({ family: "eye" });
    expect(skin[1]).toBeGreaterThan(0);
    expect(skin[2]).toBeGreaterThan(0);
    expect(fiber[11]).toBeGreaterThan(0);
    expect(cloth[7]).toBeGreaterThan(0);
    expect(eye[12]).toBeGreaterThan(0);
    expect(eye[13]).toBeLessThan(0.2);
  });
  test("untrusted numerical extremes cannot upload NaN or degenerate optical parameters", () => {
    const packed = packCreatureMaterial({
      family: "fiber",
      subsurface: Number.NaN,
      transmission: 12,
      thickness: -1,
      scatterColor: [Number.NaN, Number.POSITIVE_INFINITY, -1],
      fiberDirection: [0, 0, 0],
      anisotropy: -2,
      clearcoatRoughness: 0,
      frame: { origin: [Number.NaN, 0, 0], tangent: [0, 0, 0], normal: [0, 0, 0] },
    });
    expect([...packed].every(Number.isFinite)).toBe(true);
    expect(packed[2]).toBe(1);
    expect(packed[3]).toBe(0);
    expect(packed[11]).toBeCloseTo(-0.95);
    expect(packed[13]).toBeCloseTo(0.06);
    expect([...packed.slice(8, 11)]).toEqual([0, 1, 0]);
  });
  test("rest-space fields use anatomy axes instead of world axes", () => {
    const material: CreatureMaterial = {
      family: "skin",
      frame: { origin: [2, 3, 4], tangent: [0, 2, 0], normal: [0, 0, 3] },
    };
    expect(creatureMaterialCoordinates([2, 5, 7], material)).toEqual([2, 0, 3]);
    expect(creatureMaterialCoordinates([2, 5, 7])).toEqual([2, 5, 7]);
  });
  test("parallel, skewed and reversed authored frames have orthonormal finite bases", () => {
    for (const normal of [
      [0, 1, 0],
      [1, 1, 1],
      [0, -1, 0],
      [0, 0, 0],
    ] as const) {
      const frame = creatureMaterialFrame({
        family: "cloth",
        frame: { origin: [0, 0, 0], tangent: [0, 1, 0], normal: [...normal] },
      });
      for (const axis of [frame.x, frame.y, frame.z]) expect(dot(axis, axis)).toBeCloseTo(1, 7);
      expect(dot(frame.x, frame.y)).toBeCloseTo(0, 7);
      expect(dot(frame.x, frame.z)).toBeCloseTo(0, 7);
      expect(dot(frame.y, frame.z)).toBeCloseTo(0, 7);
    }
  });
  test("different creature optics and anatomy frames cannot share a uniform batch", () => {
    const first: RenderSurface = {
      ...surface,
      material: { ...surface.material, creature: { family: "skin" } },
    };
    const second: RenderSurface = {
      ...surface,
      material: { ...surface.material, creature: { family: "fiber" } },
    };
    const third: RenderSurface = {
      ...surface,
      material: {
        ...surface.material,
        creature: {
          family: "skin",
          frame: { origin: [1, 0, 0], tangent: [1, 0, 0], normal: [0, 0, 1] },
        },
      },
    };
    expect(batchSurfaces([first, second, third], () => 0).length).toBe(3);
  });
  test("very thick tissue stays representable after conversion to float32", () => {
    expect([...packCreatureMaterial({ family: "skin", thickness: 1e300 })].every(Number.isFinite)).toBe(true);
  });
});
