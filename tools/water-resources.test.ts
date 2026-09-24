import { expect, test } from "bun:test";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { identityMatrix, type RenderSurface, waterSchema } from "@wrela/model";
import { packWaterBody, WaterBodyBuffers, waterBodyDynamicFloats } from "@wrela/render-webgpu/water-body";
import { WaterSpectrumPrograms } from "@wrela/render-webgpu/water-spectrum-gpu";
import { WaterBodyRuntime } from "@wrela/runtime/water-body";

function device() {
  Object.assign(globalThis, {
    GPUBufferUsage: { STORAGE: 1, COPY_DST: 2, COPY_SRC: 4, UNIFORM: 8 },
    GPUTextureUsage: { TEXTURE_BINDING: 1, STORAGE_BINDING: 2, COPY_SRC: 4, COPY_DST: 8 },
  });
  const writes: { buffer: GPUBuffer; data: Float32Array }[] = [];
  const destroyed: unknown[] = [];
  const gpu = {
    queue: {
      writeBuffer: (buffer: GPUBuffer, _offset: number, data: Float32Array) =>
        writes.push({ buffer, data: data.slice() }),
    },
    createBuffer: (d: GPUBufferDescriptor) => {
      const b = { size: d.size, destroy: () => destroyed.push(b) };
      return b;
    },
    createTexture: () => {
      const t = { createView: () => ({ texture: t }), destroy: () => destroyed.push(t) };
      return t;
    },
    createSampler: () => ({}),
    createShaderModule: () => ({}),
    createComputePipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createBindGroup: () => ({}),
  } as unknown as GPUDevice;
  return { gpu, writes, destroyed };
}
function fixture(ocean = false) {
  const project = createWaterLookdev(ocean ? "ocean" : "creek"),
    water = waterSchema.parse(project.documents.find((d) => d.id === project.entry)),
    runtime = new WaterBodyRuntime(water);
  const state = runtime.renderState();
  const surface: RenderSurface = {
    id: water.id,
    source: water.id,
    water,
    waterState: state,
    matrix: identityMatrix(),
    mesh: state.domain?.surface ?? {
      positions: new Float32Array(),
      normals: new Float32Array(),
      indices: new Uint32Array(),
      bounds: { min: [0, 0, 0], max: [0, 0, 0] },
    },
    material: {
      color: water.color,
      secondary: water.color,
      roughness: water.roughness,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
  return { water, runtime, surface };
}
test("first spectrum acquisition creates a real map, sharing retains its owner, and edits reuse a sole lease", () => {
  const h = device(),
    p = new WaterSpectrumPrograms(h.gpu),
    { surface } = fixture(true),
    spectrum = surface.waterState!.spectrum,
    data = packWaterBody(surface);
  const a = p.acquire("a", spectrum, data);
  expect(a).toBeDefined();
  expect(p.bytes).toBeGreaterThan(0);
  const b = p.acquire("a", spectrum, data);
  expect(b).toBe(a);
  p.release(a);
  expect(h.destroyed).toHaveLength(0);
  const c = p.acquire("edited", spectrum, data, b);
  expect(c).toBe(b);
  expect(h.writes).toHaveLength(2);
  p.release(c);
  expect(p.bytes).toBe(0);
  expect(h.destroyed.length).toBeGreaterThan(3);
  p.destroy();
});
test("bed and effects share a single upload; ticks only update the dynamic prefix and rebasing refreshes contact", () => {
  const h = device(),
    pool = new WaterBodyBuffers(h.gpu),
    { water, runtime, surface } = fixture();
  const bed = {
    ...surface,
    id: "bed",
    water: undefined,
    waterState: undefined,
    waterContact: { water, state: surface.waterState! },
  };
  const a = pool.acquire(surface),
    b = pool.acquire(bed);
  expect(a).toBe(b);
  const first = pool.upload(a, surface, [0, 0, 0], true);
  expect(pool.upload(b, bed, [0, 0, 0], true)).toBe(0);
  runtime.simulation!.step(1 / 60);
  surface.waterState = runtime.renderState();
  bed.waterContact.state = surface.waterState;
  const dynamic = pool.upload(a, surface, [0, 0, 0], true);
  expect(dynamic).toBe(waterBodyDynamicFloats(surface) * 4);
  expect(dynamic).toBeLessThan(first);
  expect(pool.upload(b, bed, [0, 0, 0], true)).toBe(0);
  expect(pool.upload(a, surface, [100, 20, 100], true)).toBe(first);
  pool.release(a);
  expect(h.destroyed).toHaveLength(0);
  pool.release(b);
  expect(h.destroyed).toHaveLength(1);
  pool.destroy();
});
test("foam and wetness packing keeps optical state within its error budget", () => {
  const { surface } = fixture();
  surface.waterState!.cells![3] = 0.376;
  surface.waterState!.wetness![0] = 0.63;
  const packed = packWaterBody(surface),
    offset = packed[5] * 4,
    code = packed[offset + 3],
    wet = Math.floor(code / 2);
  expect(Math.abs(code - wet * 2 - 0.376)).toBeLessThan(0.00004);
  expect(Math.abs(wet / 255 - 0.63)).toBeLessThan(1 / 255);
});
