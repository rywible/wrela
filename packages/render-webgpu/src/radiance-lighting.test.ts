import { expect, test } from "bun:test";
import { materialSchema, type RadianceLightingField } from "@wrela/model";
import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { batchSurfaces } from "./batching";
import { EMISSION_OFFSET, packSurface, packVertices } from "./packing";
import { packRadianceLighting, radianceLightingBytes } from "./radiance-lighting";

test("radiance payload validates its layout and rebases light and receiver positions", () => {
  const field: RadianceLightingField = {
    key: "one",
    revision: 1,
    positions: [[1001, 2, 3]],
    enclosed: new Uint32Array([12]),
    lights: [{ position: [1002, 4, 6], range: 5 }],
    transfer: new Float32Array(9 * 27 * 4),
    skyVisibility: new Float32Array(9),
    report: {
      samples: 1,
      triangles: 0,
      rays: 0,
      vertices: 0,
      unmappedVertices: 0,
      bytes: 0,
      buildMs: 0,
      excluded: [],
    },
  };
  const packed = packRadianceLighting(field, [1000, 0, 0]);
  expect(packed.byteLength).toBe(radianceLightingBytes(field));
  expect(packed[3]).toBe(-1);
  expect(packed[12]).toBe(1);
  field.emissionScale = 4;
  expect(packRadianceLighting(field)[12]).toBe(4);
  field.emissionScale = -1;
  expect(() => packRadianceLighting(field)).toThrow();
  field.emissionScale = Infinity;
  expect(() => packRadianceLighting(field)).toThrow();
  field.emissionScale = 1;
  expect(Array.from(packed.slice(16, 20))).toEqual([2, 4, 6, 25]);
  expect(Array.from(packed.slice(48, 52))).toEqual([1, 2, 3, 12]);
  field.surfaceDiffuse = {
    positions: [[1003, 5, 7]],
    transfer: new Float32Array(108),
    receivers: new Float32Array([0, 0, 0, 0, 0.25, 0.5, 0.25, 0, 1, 2, 3, 0]),
  };
  field.surfaceDiffuse.transfer[2] = 0.75;
  const cached = packRadianceLighting(field, [1000, 0, 0]);
  expect(cached.byteLength).toBe(radianceLightingBytes(field));
  expect(cached[10]).toBe(1);
  expect(cached[13]).toBe(1);
  expect(Array.from(cached.subarray(cached[9] * 4, cached[9] * 4 + 3))).toEqual([3, 5, 7]);
  expect(cached[cached[9] * 4 + 10]).toBe(0.75);
  expect(Array.from(cached.subarray(cached[11] * 4))).toEqual(Array.from(field.surfaceDiffuse.receivers));
  field.surfaceDiffuse.receivers[0] = 1;
  expect(() => packRadianceLighting(field)).toThrow();
  field.surfaceDiffuse.receivers[0] = 0;
  field.surfaceDiffuse.receivers[4] = -0.25;
  expect(() => packRadianceLighting(field)).toThrow();
  field.surfaceDiffuse = undefined;
  field.transfer[0] = NaN;
  expect(() => packRadianceLighting(field)).toThrow();
});
test("emission survives authoring and packing; different lighting bindings cannot batch together", () => {
  const schema = materialSchema.shape.emission;
  expect(schema.parse({ color: [1, 0.2, 0], intensity: 3 })).toEqual({ color: [1, 0.2, 0], intensity: 3 });
  expect(schema.safeParse({ color: [1, 0, 0], intensity: -1 }).success).toBe(false);
  const s = indirectBoxFixture().surfaces[0];
  s.material.emission = { color: [1, 0.2, 0], intensity: 3 };
  const data = packSurface(s);
  expect(data[EMISSION_OFFSET]).toBe(1);
  expect(data[EMISSION_OFFSET + 3]).toBe(3);
  const a = { ...s, radianceProbes: [1, 2, 1, 2] as [number, number, number, number], radianceWeight: 1 };
  const b = {
    ...s,
    id: "other",
    radianceProbes: [3, 4, 3, 4] as [number, number, number, number],
    radianceWeight: 1,
  };
  expect(batchSurfaces([a, b], () => 1)).toHaveLength(2);
});

test("vertex packing distinguishes surface-cache records from ordinary probe pairs", () => {
  const mesh = indirectBoxFixture().surfaces[0].mesh;
  mesh.radianceProbes = new Float32Array((mesh.positions.length / 3) * 4);
  mesh.radianceProbes.set([1024, 65536, 1, 2.25]);
  expect(() => packVertices(mesh)).not.toThrow();
  mesh.radianceProbes[1] = 65537;
  expect(() => packVertices(mesh)).toThrow();
  mesh.radianceProbes[1] = 1.25;
  expect(() => packVertices(mesh)).toThrow();
  mesh.radianceProbes.set([1, 513]);
  expect(() => packVertices(mesh)).toThrow();
});

test("four-sample GPU records reject malformed indices and normalization", () => {
  const f: RadianceLightingField = {
    key: "mixture",
    revision: 1,
    positions: [
      [0, 0, 0],
      [1, 0, 0],
    ],
    enclosed: new Uint32Array(2),
    transfer: new Float32Array(2 * 9 * 27 * 4),
    skyVisibility: new Float32Array(18),
    lights: [],
    receivers: new Float32Array([1, 2, 0, 0, 0.25, 0.75, 0, 0]),
    report: {
      samples: 2,
      triangles: 0,
      rays: 0,
      vertices: 0,
      unmappedVertices: 0,
      bytes: 0,
      buildMs: 0,
      excluded: [],
    },
  };
  const data = packRadianceLighting(f);
  expect(data[5]).toBe(1);
  expect(Array.from(data.subarray(data[4] * 4))).toEqual(Array.from(f.receivers!));
  f.receiverEmission = new Float32Array(3);
  f.receiverEmission.set([1, 0.4, 0.1]);
  expect(() => packRadianceLighting(f)).toThrow();
  f.directEmission = Float32Array.from({ length: 54 }, (_, i) => i / 54);
  const local = packRadianceLighting(f);
  expect(local.byteLength).toBe(radianceLightingBytes(f));
  expect(local[6]).toBe(local[4] + 2);
  expect(Array.from(local.subarray(local[6] * 4, local[7] * 4))).toEqual([...f.receiverEmission, 0]);
  expect(local[8]).toBe(local[7] + f.positions.length * 9);
  for (let i = 0; i < 18; i++) {
    const start = local[7] * 4 + i * 4;
    expect(Array.from(local.subarray(start, start + 3))).toEqual(
      Array.from(f.directEmission.subarray(i * 3, i * 3 + 3)),
    );
    expect(local[start + 3]).toBe(0);
  }
  expect(local.subarray(local[8] * 4).every((v) => v === 0)).toBe(true);
  f.receiverEmission = new Float32Array(20);
  expect(() => packRadianceLighting(f)).toThrow();
  f.receiverEmission = new Float32Array(3);
  f.receiverEmission[0] = NaN;
  expect(() => packRadianceLighting(f)).toThrow();
  f.receiverEmission[0] = 0;
  f.directEmission[0] = NaN;
  expect(() => packRadianceLighting(f)).toThrow();
  f.directEmission = new Float32Array(3);
  expect(() => packRadianceLighting(f)).toThrow();
  f.directEmission = undefined;
  f.receiverEmission = undefined;
  f.receivers![0] = 3;
  expect(() => packRadianceLighting(f)).toThrow();
  f.receivers![0] = 1;
  f.receivers![4] = 0.5;
  expect(() => packRadianceLighting(f)).toThrow();
});
