import { describe, expect, test } from "bun:test";
import type { RenderSurface } from "@wrela/model";

import { identityColor, identityMatrix, normalizeWind, transformMatrix } from "@wrela/model";

import { batchSurfaces, INSTANCE_FLOATS, MAX_BATCH_INSTANCES, packInstances } from "./batching";
import { cameraRay, lookAt, multiply, perspective } from "./math";
import {
  NOISE_PERIOD,
  noiseOrigin,
  OBJECT_FLOATS,
  packSurface,
  packVertices,
  VERTEX_FLOATS,
} from "./packing";

const surface: RenderSurface = {
  id: "subject",
  source: "object",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array([1, 2, 3]),
    normals: new Float32Array([0, 1, 0]),
    indices: new Uint32Array([0]),
    bounds: { min: [1, 2, 3], max: [1, 2, 3] },
  },
  material: {
    color: [0.1, 0.2, 0.3],
    secondary: [0.4, 0.5, 0.6],
    roughness: 0.7,
    metallic: 0.8,
    pattern: 2,
    scale: 3,
    normalStrength: 0.4,
  },
};
const transform = (m: Float32Array, v: number[]) =>
  [0, 1, 2, 3].map((r) => v.reduce((s, n, c) => s + m[c * 4 + r] * n, 0));
describe("WebGPU matrix convention", () => {
  test("near and far map to zero and one", () => {
    const projection = perspective(60, 1.5, 0.1, 100);
    for (const [z, depth] of [
      [-0.1, 0],
      [-100, 1],
    ]) {
      const v = transform(projection, [0, 0, z, 1]);
      expect(v[2] / v[3]).toBeCloseTo(depth, 6);
    }
  });
  test("view maps eye to origin and target down negative Z", () => {
    const view = lookAt([3, 4, 5], [1, 2, 3]);
    const eye = transform(view, [3, 4, 5, 1]);
    expect(eye[0]).toBeCloseTo(0);
    expect(eye[1]).toBeCloseTo(0);
    expect(eye[2]).toBeCloseTo(0);
    const target = transform(view, [1, 2, 3, 1]);
    expect(target[0]).toBeCloseTo(0);
    expect(target[1]).toBeCloseTo(0);
    expect(target[2]).toBeLessThan(0);
  });
  test("pole camera basis remains finite", () => {
    expect([...lookAt([0, 10, 0], [0, 0, 0])].every(Number.isFinite)).toBe(true);
  });
  test("matrix multiplication preserves identity", () => {
    const m = perspective(50, 2);
    expect([...multiply(identityMatrix(), m)]).toEqual([...m]);
  });
  test("center ray targets the camera target", () => {
    const ray = cameraRay({ position: [0, 0, 5], target: [0, 0, 0], fov: 60 }, 0, 0, 2);
    expect(ray.direction).toEqual([0, 0, -1]);
  });
});
describe("WGSL host layout", () => {
  test("ordinary vertex stream preserves stationary foliage without allocating relief attributes", () => {
    const packed = packVertices(surface.mesh);
    expect(packed.length).toBe(VERTEX_FLOATS);
    expect([...packed.slice(0, 6)]).toEqual([1, 2, 3, 0, 1, 0]);
    expect([...packed.slice(6, 9)]).toEqual([1, 1, 1]);
    expect([...packed.slice(13, 17)]).toEqual([1, 0, 0, 0]);
    expect([...packed.slice(17, 23)]).toEqual([0, 0, 0, 0, 0, 0]);
    expect(packed.length).toBe(23);
  });
  test("foliage phase and amplitude survive vertex packing", () => {
    const packed = packVertices({ ...surface.mesh, wind: new Float32Array([0.4, 0.2, 1.3, 0.03]) });
    expect([...packed.slice(17, 21)]).toEqual([...new Float32Array([0.4, 0.2, 1.3, 0.03])]);
  });
  test("skin influences occupy four separate weights and indices", () => {
    const packed = packVertices(surface.mesh, {
      jointIndices: new Uint16Array([2, 4, 6, 8]),
      weights: new Float32Array([0.1, 0.2, 0.3, 0.4]),
      matrices: identityMatrix(),
    });
    expect([...packed.slice(9, 13)]).toEqual([2, 4, 6, 8]);
    expect(packed[16]).toBeCloseTo(0.4);
  });
  test("material values use aligned vec4 blocks", () => {
    const p = packSurface(surface);
    expect(p.length).toBe(OBJECT_FLOATS);
    expect(p[19]).toBeCloseTo(0.7);
    expect(p[23]).toBeCloseTo(0.8);
    expect([...p.slice(24, 26)]).toEqual([2, 3]);
  });
  test("flat water preserves height with no waves", () => {
    const p = packSurface({
      ...surface,
      water: {
        id: "water",
        name: "Water",
        schemaVersion: 1,
        dependencies: [],
        kind: "water",
        level: 7,
        color: [0, 0.2, 0.3],
        roughness: 0.2,
        waves: [],
      },
    });
    expect(p[28]).toBe(1);
    expect(p[31]).toBe(0);
    expect(p[39]).toBe(7);
  });
  test("wave propagation parameters retain physical units and phase", () => {
    const p = packSurface({
      ...surface,
      water: {
        id: "water",
        name: "Water",
        schemaVersion: 1,
        dependencies: [],
        kind: "water",
        level: -2,
        color: [0, 0.2, 0.3],
        roughness: 0.2,
        waves: [{ amplitude: 0.2, wavelength: 4, speed: -3, direction: 1.2, phase: 0.8 }],
      },
    });
    expect(p[31]).toBe(1);
    expect(p[32]).toBeCloseTo(0.2);
    expect(p[33]).toBe(4);
    expect(p[34]).toBe(-3);
    expect(p[35]).toBeCloseTo(1.2);
    expect(p[36]).toBeCloseTo(0.8);
    expect(p[39]).toBe(-2);
  });
});

describe("bounded compatible instancing", () => {
  test("material, selection, mesh and deformation incompatibility split draws", () => {
    const skin = {
      jointIndices: new Uint16Array(4),
      weights: new Float32Array([1, 0, 0, 0]),
      matrices: identityMatrix(),
    };
    const inputs = [
      surface,
      { ...surface, id: "second", matrix: transformMatrix([3, 0, 0]) },
      { ...surface, id: "selected", selected: true },
      { ...surface, id: "metal", material: { ...surface.material, metallic: 0 } },
      { ...surface, id: "other-mesh", mesh: { ...surface.mesh } },
      { ...surface, id: "pose-a", skin },
      { ...surface, id: "pose-b", skin },
    ];
    const groups = batchSurfaces(inputs, (s) => (s.mesh === surface.mesh ? 1 : 2));
    expect(groups.map((g) => g.surfaces.length)).toEqual([2, 1, 1, 1, 1, 1]);
  });
  test("large populations split into bounded batches with stable keys", () => {
    const inputs = Array.from({ length: 600 }, (_, i) => ({ ...surface, id: String(i) }));
    const groups = batchSurfaces(inputs, () => 7);
    expect(groups.map((g) => g.surfaces.length)).toEqual([MAX_BATCH_INSTANCES, MAX_BATCH_INSTANCES, 88]);
    expect(batchSurfaces(inputs.slice().reverse(), () => 7).map((g) => g.key)).toEqual(
      groups.map((g) => g.key),
    );
  });
  test("instance stride preserves current and previous transforms and generated instance identity", () => {
    const next = {
      ...surface,
      id: "material-piece",
      instanceId: "tree-123",
      matrix: transformMatrix([4, 5, 6], 2),
    };
    const packed = packInstances([surface, next]);
    expect(packed.length).toBe(INSTANCE_FLOATS * 2);
    expect([...packed.slice(INSTANCE_FLOATS, INSTANCE_FLOATS + 16)]).toEqual([...next.matrix]);
    for (let i = 0; i < 3; i++)
      expect(packed[INSTANCE_FLOATS + 16 + i]).toBeCloseTo(identityColor("tree-123")[i], 6);
    expect([...packed.slice(20, 36)]).toEqual([...surface.matrix]);
    expect(packed[19]).toBe(0);
    const previous = transformMatrix([-1, 2, 3]);
    const withHistory = packInstances([next], () => previous);
    expect([...withHistory.slice(20, 36)]).toEqual([...previous]);
    expect(withHistory[19]).toBe(1);
    expect(identityColor("tree-123")).not.toEqual(identityColor("tree-124"));
  });
});

test("wind normalization preserves direction and a finite conservative cap across authored numeric extremes", () => {
  expect(normalizeWind([3, 99, 4])).toEqual([3, 0, 4]);
  expect(normalizeWind([0, 0, 0])).toEqual([0, 0, 0]);
  for (const magnitude of [1e20, 1e200, Number.MAX_VALUE]) {
    const wind = normalizeWind([magnitude, 0, -magnitude]);
    expect(wind.every(Number.isFinite)).toBe(true);
    expect(Math.hypot(...wind)).toBeCloseTo(10, 12);
    expect(wind[0]).toBeCloseTo(-wind[2], 12);
  }
});

test("world material lattices and layer offsets preserve coordinates across positive and negative rebases", () => {
  const modulo = (value: number) => ((value % NOISE_PERIOD) + NOISE_PERIOD) % NOISE_PERIOD;
  const world: RenderSurface = {
    ...surface,
    material: {
      ...surface.material,
      domain: "world",
      layers: [
        {
          color: [1, 1, 1],
          roughness: 0.9,
          metallic: 0,
          coverage: 0.5,
          slopeBias: 1,
          noiseScale: 0.125,
          normalStrength: 0.2,
        },
      ],
    },
  };
  for (const origin of [-256, 256, 999999744]) {
    const packed = packSurface(world, [origin, 0, -origin]);
    for (const [i, octave] of [1, 2, 7, 21].entries()) {
      const frequency = world.material.scale * octave;
      const relative = 17.5;
      expect(modulo(relative * frequency + packed[100 + i * 4])).toBeCloseTo(
        modulo((origin + relative) * frequency),
        3,
      );
    }
    expect(packed[132]).toBeCloseTo(noiseOrigin([origin, 0, -origin], 0.125)[0], 6);
  }
  const local = packSurface({ ...world, material: { ...world.material, domain: "local" } }, [256, 0, 256]);
  expect([...local.slice(100, 120)]).toEqual(new Array(20).fill(0));
});

test("material domains and layers split otherwise compatible instance batches", () => {
  const layer = {
    color: [1, 1, 1] as [number, number, number],
    roughness: 0.9,
    metallic: 0,
    coverage: 0.5,
    slopeBias: 1,
    noiseScale: 0.125,
    normalStrength: 0.2,
  };
  const inputs = [
    surface,
    { ...surface, material: { ...surface.material, domain: "world" as const } },
    { ...surface, material: { ...surface.material, layers: [layer] } },
  ];
  expect(batchSurfaces(inputs, () => 1)).toHaveLength(3);
});

test("shared material packing preserves nested edits, selection, and independent skin instances", () => {
  const a = {
    ...surface,
    id: "a",
    material: {
      ...surface.material,
      layers: [
        {
          color: [1, 0, 0] as [number, number, number],
          roughness: 0.5,
          metallic: 0,
          coverage: 0.5,
          slopeBias: 0,
          noiseScale: 1,
          normalStrength: 0,
        },
      ],
    },
  };
  const b = { ...a, id: "b", material: structuredClone(a.material) };
  expect(batchSurfaces([a, b], () => 1)).toHaveLength(1);
  b.material.layers[0].color[0] = 0.5;
  expect(batchSurfaces([a, b], () => 1)).toHaveLength(2);
  b.material.layers[0].color[0] = 1;
  expect(batchSurfaces([a, { ...b, selected: true }], () => 1)).toHaveLength(2);
  const skin = { jointIndices: new Uint16Array(4), weights: new Float32Array(4), matrices: identityMatrix() };
  expect(
    batchSurfaces(
      [
        { ...a, skin },
        { ...b, skin },
      ],
      () => 1,
    ),
  ).toHaveLength(2);
});
