import { describe, expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface, transformMatrix } from "@wrela/model";
import { batchSurfaces } from "./batching";
import { packCreatureDeformation, validateCreatureDeformation } from "./creature-deformation";
import { DetailSelector } from "./detail";
import { WebGPURenderer } from "./index";
import { VERTEX_FLOATS } from "./packing";
import { surfaceBounds } from "./visibility";
import { WaterBodyBuffers } from "./water-body";

const surface: RenderSurface = {
  id: "creature:skin",
  source: "creature",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]),
    normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
    colors: new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1]),
    indices: new Uint32Array([0, 1, 2]),
    bounds: { min: [0, 0, 0], max: [1, 1, 0] },
  },
  material: {
    color: [1, 1, 1],
    secondary: [1, 1, 1],
    roughness: 1,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
  deformation: {
    revision: "pose1",
    vertexIndices: new Uint32Array([1]),
    positionDeltas: new Float32Array([0, 0, 0.5]),
    normalDeltas: new Float32Array([0, 1, -1]),
    maxDisplacement: 0.5,
  },
};
const fixtureDeformation = surface.deformation as NonNullable<RenderSurface["deformation"]>;
const environment: EvaluatedScene["environment"] = {
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
};

describe("creature deformation stream", () => {
  test("sparse correctives update position and normal while preserving immutable mesh and other streams", () => {
    const packed = packCreatureDeformation(surface);
    expect([...packed.slice(VERTEX_FLOATS, VERTEX_FLOATS + 6)]).toEqual([1, 0, 0.5, 0, 1, 0]);
    expect([...surface.mesh.positions]).toEqual([0, 0, 0, 1, 0, 0, 0, 1, 0]);
    expect([...packed.slice(VERTEX_FLOATS + 6, VERTEX_FLOATS + 9)]).toEqual([0, 1, 0]);
    const same = packCreatureDeformation(
      {
        ...surface,
        deformation: {
          revision: "pose2",
          vertexIndices: new Uint32Array([0]),
          positionDeltas: new Float32Array([0.25, 0, 0]),
          maxDisplacement: 0.25,
        },
      },
      packed,
    );
    expect(same).toBe(packed);
    expect([...same.slice(VERTEX_FLOATS, VERTEX_FLOATS + 6)]).toEqual([1, 0, 0, 0, 0, 1]);
    expect(same[0]).toBe(0.25);
  });
  test("invalid mappings and understated envelopes are rejected before visibility", () => {
    for (const deformation of [
      { ...fixtureDeformation, maxDisplacement: 0.1 },
      { ...fixtureDeformation, vertexIndices: new Uint32Array([99]) },
      { ...fixtureDeformation, positionDeltas: new Float32Array([NaN, 0, 0]) },
      { ...fixtureDeformation, positionDeltas: new Float32Array(2) },
      { ...fixtureDeformation, normalDeltas: new Float32Array([NaN, 0, 0]) },
      {
        ...fixtureDeformation,
        vertexIndices: new Uint32Array([1, 1]),
        positionDeltas: new Float32Array(6),
        normalDeltas: undefined,
      },
    ])
      expect(validateCreatureDeformation({ ...surface, deformation })).toBeDefined();
  });
  test("envelopes expand before skin and object scale, so camera and shadow culling contain corrections", () => {
    const deformed = {
      ...surface,
      matrix: new Float32Array([2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 1]),
      skin: {
        jointIndices: new Uint16Array(12),
        weights: new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]),
        matrices: transformMatrix([1, 0, 0]),
      },
    };
    const bounds = surfaceBounds(deformed, environment);
    expect(bounds.min[0]).toBeLessThanOrEqual(1);
    expect(bounds.max[0]).toBeGreaterThanOrEqual(5);
    expect(bounds.min[2]).toBeLessThanOrEqual(-2);
    expect(bounds.max[2]).toBeGreaterThanOrEqual(2);
  });
  test("independent actor offsets never instance together or select an unmapped detail", () => {
    expect(batchSurfaces([surface, { ...surface, id: "other:skin" }], () => 1).length).toBe(2);
    const detail = { ...surface.mesh, positions: new Float32Array(3) };
    const scene: EvaluatedScene = {
      surfaces: [
        {
          ...surface,
          details: [{ label: "distant", mesh: detail, maxProjectedDiameter: 1e6, maxError: null }],
        },
      ],
      camera: { position: [0, 0, 100], target: [0, 0, 0], fov: 50 },
      environment,
      time: 0,
      grid: false,
      mode: "beauty",
    };
    expect(new DetailSelector().select(scene, 100).scene.surfaces[0].mesh).toBe(surface.mesh);
  });
  test("renderer reuses actor GPU buffers for 100 pose revisions and releases stream on removal", () => {
    // CPU-only resource bookkeeping test; this never initializes or calls a GPU adapter.
    Object.assign(globalThis, { GPUBufferUsage: { VERTEX: 1, COPY_DST: 2, STORAGE: 4, UNIFORM: 8 } });
    const buffers: { label: string; size: number; destroyed: boolean; destroy(): void }[] = [];
    const writes = new Map<unknown, number>();
    const Constructor = WebGPURenderer as unknown as new (canvas: HTMLCanvasElement, options: object) => any;
    const renderer = new Constructor({} as HTMLCanvasElement, {});
    renderer.thinFallback = { view: {}, bytes: 1, texture: { destroy() {} } };
    renderer.thinSampler = {};
    renderer.device = {
      queue: { writeBuffer: (buffer: unknown) => writes.set(buffer, (writes.get(buffer) ?? 0) + 1) },
      createBuffer: (descriptor: { label: string; size: number }) => {
        const buffer = {
          ...descriptor,
          destroyed: false,
          destroy() {
            this.destroyed = true;
          },
        };
        buffers.push(buffer);
        return buffer;
      },
      createBindGroup: () => ({}),
    };
    renderer.waterBodyBuffers = new WaterBodyBuffers(renderer.device);
    renderer.waterSpectrumPrograms = { bytes: 0, fallbackView: {}, sampler: {} };
    const make = (input: RenderSurface) =>
      renderer.object({ key: "stable-actor", surfaces: [input], visibility: { camera: true, shadow: true } });
    const first = make(surface);
    const stream = first.deformation.vertex;
    const previousStream = first.deformation.previous;
    const byteCount = renderer.bytes;
    const allocationCount = buffers.length;
    for (let i = 0; i < 100; i++) {
      const object = make({ ...surface, deformation: { ...fixtureDeformation, revision: `pose${i}` } });
      expect(object.deformation.vertex).toBe(stream);
    }
    expect(buffers.length).toBe(allocationCount);
    expect(renderer.bytes).toBe(byteCount);
    const before = writes.get(stream);
    make({ ...surface, deformation: { ...fixtureDeformation, revision: "pose99" } });
    expect(writes.get(stream)).toBe(before); // Repeated poses do not upload the current deformation again.
    make({ ...surface, deformation: undefined });
    expect(stream.destroyed).toBe(true);
    if (previousStream) expect(previousStream.destroyed).toBe(true);
    expect(renderer.bytes).toBe(byteCount - stream.size - (previousStream?.size ?? 0));
    expect(first.deformation).toBeUndefined();
    const piece = (
      id: string,
      actor: string,
      deformation: RenderSurface["deformation"] = fixtureDeformation,
    ) =>
      renderer.object({
        key: id,
        surfaces: [{ ...surface, id, instanceId: actor, deformation }],
        visibility: { camera: true, shadow: true },
      });
    const left = piece("skin-piece", "warden"),
      right = piece("eye-piece", "warden");
    const shared = left.deformation;
    expect(right.deformation).toBe(shared);
    expect(writes.get(shared.vertex)).toBe(1);
    expect(writes.get(shared.previous)).toBe(1);
    const other = piece("other-actor", "second-warden");
    expect(other.deformation).not.toBe(shared);
    renderer.object({
      key: "skin-piece",
      surfaces: [{ ...surface, id: "skin-piece", instanceId: "warden", deformation: undefined }],
      visibility: { camera: true, shadow: true },
    });
    expect(shared.vertex.destroyed).toBe(false);
    renderer.object({
      key: "eye-piece",
      surfaces: [{ ...surface, id: "eye-piece", instanceId: "warden", deformation: undefined }],
      visibility: { camera: true, shadow: true },
    });
    expect(shared.vertex.destroyed).toBe(true);
    expect(shared.previous.destroyed).toBe(true);
  });
});
