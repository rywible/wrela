import { expect, test } from "bun:test";
import { finiteScatteringSeries, PHYSICAL_DIFFUSE_ORDERS, PHYSICAL_DIFFUSE_SIZE } from "./atmosphere";
import { PhysicalAtmosphereGpu } from "./atmosphere-gpu";
import { GLOBAL_FLOATS } from "./packing";

test("passive diffuse closure is nonnegative, finite, energy bounded, and vanishes without illumination", () => {
  for (const opticalDepth of [0, 1e-12, 0.01, 0.5, 2, 20, 1000])
    for (const scatteringAlbedo of [0, 0.5, 0.95, 1])
      for (const groundAlbedo of [0, 0.3, 1]) {
        const transmission = Math.exp(-opticalDepth);
        // Exact homogeneous segment: scatter or transmit to a passive Lambertian ground.
        const feedback = scatteringAlbedo * (1 - transmission) + groundAlbedo * transmission;
        const first = (1 - transmission) * scatteringAlbedo * 0.1 + transmission * groundAlbedo * 0.2;
        expect(feedback).toBeLessThanOrEqual(1);
        const total = finiteScatteringSeries(first, feedback);
        expect(total).toBeGreaterThanOrEqual(first);
        expect(total).toBeLessThanOrEqual(PHYSICAL_DIFFUSE_ORDERS * first + 1e-12);
        expect(finiteScatteringSeries(0, feedback)).toBe(0);
        expect(Number.isFinite(total)).toBe(true);
      }
  expect(finiteScatteringSeries(0.2, 1)).toBeCloseTo(3.2, 12);
  expect(finiteScatteringSeries(0.2, 0)).toBe(0.2);
  expect(() => finiteScatteringSeries(1, 1.01)).toThrow();
  expect(() => finiteScatteringSeries(-1, 0.5)).toThrow();
});
test("finite scattering orders converge monotonically with the expected geometric remainder", () => {
  for (const feedback of [0.1, 0.5, 0.85]) {
    let previous = 0;
    const infinite = 0.3 / (1 - feedback);
    for (let orders = 1; orders <= PHYSICAL_DIFFUSE_ORDERS; orders++) {
      const total = finiteScatteringSeries(0.3, feedback, orders);
      expect(total).toBeGreaterThanOrEqual(previous);
      expect(infinite - total).toBeCloseTo((0.3 * feedback ** orders) / (1 - feedback), 12);
      previous = total;
    }
  }
});

test("diffuse LUT rebuilds only for composition or ground, owns all bytes, and stays inside frame timestamps", async () => {
  Object.assign(globalThis, {
    GPUTextureUsage: { STORAGE_BINDING: 1, TEXTURE_BINDING: 2 },
    GPUBufferUsage: { UNIFORM: 1, COPY_DST: 2, STORAGE: 4 },
    GPUShaderStage: { COMPUTE: 4 },
  });
  const textures: { bytes: number; destroyed: boolean }[] = [];
  const buffers: { bytes: number; destroyed: boolean }[] = [];
  let uploaded = 0;
  const passes: GPUComputePassDescriptor[] = [];
  const pipelines: string[] = [];
  const device = {
    createTexture: (descriptor: { size: number[]; format: string; mipLevelCount?: number }) => {
      const record = {
        bytes:
          Array.from({ length: descriptor.mipLevelCount ?? 1 }, (_, mip) =>
            descriptor.size.reduce((a, b) => a * Math.max(1, b >> mip), 1),
          ).reduce((a, b) => a + b, 0) * (descriptor.format === "rgba8unorm" ? 4 : 8),
        destroyed: false,
      };
      textures.push(record);
      return {
        createView: () => ({}),
        destroy: () => {
          record.destroyed = true;
        },
      };
    },
    createBuffer: (descriptor: { size: number }) => {
      const record = { bytes: descriptor.size, destroyed: false };
      buffers.push(record);
      return {
        destroy: () => {
          record.destroyed = true;
        },
      };
    },
    createSampler: () => ({}),
    createBindGroupLayout: () => ({}),
    createPipelineLayout: () => ({}),
    createBindGroup: () => ({}),
    createComputePipeline: (descriptor: GPUComputePipelineDescriptor) => ({
      label: descriptor.label,
      getBindGroupLayout: () => ({}),
    }),
    createShaderModule: () => ({ getCompilationInfo: async () => ({ messages: [] }) }),
    queue: {
      writeBuffer: (_buffer: unknown, _offset: number, values: Float32Array) => {
        uploaded += values.byteLength;
      },
    },
  } as unknown as GPUDevice;
  let clears = 0;
  let clearAtPass = -1;
  const encoder = {
    clearBuffer: () => {
      clearAtPass = passes.length;
      clears++;
    },
    beginComputePass: (descriptor: GPUComputePassDescriptor) => {
      if (descriptor.timestampWrites)
        expect(
          descriptor.timestampWrites.beginningOfPassWriteIndex !== undefined ||
            descriptor.timestampWrites.endOfPassWriteIndex !== undefined,
        ).toBe(true);
      passes.push(descriptor);
      return {
        setPipeline: (pipeline: { label: string }) => pipelines.push(pipeline.label),
        setBindGroup: () => {},
        dispatchWorkgroups: () => {},
        end: () => {},
      };
    },
  } as unknown as GPUCommandEncoder;
  const gpu = await PhysicalAtmosphereGpu.create(device);
  const globals = new Float32Array(GLOBAL_FLOATS);
  expect(() => gpu.encode(encoder, new Float32Array(168), {} as GPUBuffer)).toThrow("complete");
  globals.set([0.5, 0.6, 0.7], 52);
  const table = {} as GPUBuffer;
  const timing = { querySet: {} as GPUQuerySet, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 };
  const assertStamps = (start: number) => {
    const run = passes.slice(start);
    expect(run[0].timestampWrites?.beginningOfPassWriteIndex).toBe(0);
    expect(run.at(-1)?.timestampWrites?.endOfPassWriteIndex).toBe(1);
    expect(run.filter((p) => p.timestampWrites?.beginningOfPassWriteIndex !== undefined).length).toBe(1);
    expect(run.filter((p) => p.timestampWrites?.endOfPassWriteIndex !== undefined).length).toBe(1);
  };
  gpu.encode(encoder, globals, table, timing);
  expect(passes.length).toBe(5);
  assertStamps(0);
  expect(pipelines).toEqual([
    "Compile periodic cloud noise",
    "Physical atmosphere diffuse build",
    "Physical atmosphere sky build",
    "Physical atmosphere aerial build",
    "Physical cloud reflection sky build",
    "Shared cloud light field",
    "Sky diffuse convolution",
    "GGX environment BRDF integration",
    ...Array.from({ length: 8 }, (_, mip) => `GGX reflection level ${mip}`),
  ]);
  expect(uploaded).toBe(gpu.frameUploadBytes);
  const initial = passes.length;
  expect(gpu.encode(encoder, globals, table, timing)).toBe(0);
  expect(passes.length).toBe(initial);
  globals[191] = 80;
  expect(gpu.encode(encoder, globals, table, timing)).toBe(0);
  expect(passes.length).toBe(initial);
  const beforeOrientation = pipelines.length;
  globals[64] += 0.1;
  gpu.encode(encoder, globals, table, timing);
  expect(pipelines.slice(beforeOrientation)).toEqual(["Physical atmosphere aerial build"]);
  assertStamps(initial);
  const beforeClouds = pipelines.length,
    cloudPassStart = passes.length;
  globals.set([0.5, 2, 0.5, 3], 188);
  gpu.encode(encoder, globals, table, timing);
  expect(pipelines.slice(beforeClouds)).toEqual([
    "Shared cloud optical depth build",
    "Physical atmosphere sky build",
    "Physical atmosphere aerial build",
    "Physical cloud reflection sky build",
    "Trace complete cloud view",
    "Shared cloud light field",
    "Resolve physical cloud view",
    "Sky diffuse convolution",
    ...Array.from({ length: 8 }, (_, mip) => `GGX reflection level ${mip}`),
  ]);
  assertStamps(cloudPassStart);
  const beforeView = pipelines.length,
    viewPassStart = passes.length;
  globals[64] += 0.1;
  gpu.encode(encoder, globals, table, timing);
  expect(pipelines.slice(beforeView)).toEqual([
    "Physical atmosphere aerial build",
    "Trace complete cloud view",
    "Resolve physical cloud view",
  ]);
  assertStamps(viewPassStart);
  const beforeDrift = pipelines.length;
  globals[191] = 4;
  gpu.encode(encoder, globals, table);
  expect(pipelines.slice(beforeDrift)).toEqual([
    "Shared cloud optical depth build",
    "Physical atmosphere sky build",
    "Physical atmosphere aerial build",
    "Physical cloud reflection sky build",
    "Trace complete cloud view",
    "Shared cloud light field",
    "Resolve physical cloud view",
    "Sky diffuse convolution",
    ...Array.from({ length: 8 }, (_, mip) => `GGX reflection level ${mip}`),
  ]);
  const beforeGround = pipelines.length;
  globals[52] = 0.1;
  gpu.encode(encoder, globals, table);
  expect(pipelines.slice(beforeGround)).toContain("Physical atmosphere diffuse build");
  expect(pipelines.slice(beforeGround)).toContain("Shared cloud optical depth build");
  // Rebase changes the world-to-planet relation even inside the same light-grid cell.
  const beforeRebase = pipelines.length;
  globals[149] += 1;
  gpu.encode(encoder, globals, table);
  expect(pipelines.slice(beforeRebase)).toContain("Shared cloud optical depth build");
  const beforeTable = pipelines.length;
  gpu.encode(encoder, globals, {} as GPUBuffer);
  expect(pipelines.slice(beforeTable)).toContain("Physical atmosphere diffuse build");
  // Exercise production reuse with an orthonormal camera, followed by a cut.
  globals.set([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1.5, 0.6, 0, 0], 64);
  gpu.encode(encoder, globals, table);
  const beforeReuse = pipelines.length;
  const reusePassStart = passes.length;
  globals[32] += 1;
  gpu.encode(encoder, globals, table);
  expect(clears).toBe(1);
  expect(clearAtPass).toBe(reusePassStart);
  expect(pipelines.slice(beforeReuse)).toContain("Trace compacted cloud rays");
  const beforeCut = pipelines.length;
  globals[32] += 1000;
  gpu.encode(encoder, globals, table);
  expect(pipelines.slice(beforeCut)).toContain("Trace complete cloud view");
  expect(clears).toBe(1);
  expect(textures.length).toBe(16);
  expect(gpu.bufferByteLength).toBe(buffers.reduce((sum, b) => sum + b.bytes, 0));
  expect(textures[3].bytes).toBe(PHYSICAL_DIFFUSE_SIZE[0] * PHYSICAL_DIFFUSE_SIZE[1] * 8);
  expect(gpu.byteLength).toBe([...textures, ...buffers].reduce((sum, resource) => sum + resource.bytes, 0));
  gpu.destroy();
  expect([...textures, ...buffers].every((resource) => resource.destroyed)).toBe(true);
});
