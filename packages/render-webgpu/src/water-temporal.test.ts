import { expect, test } from "bun:test";
import { WaterTemporalResolve } from "./water-temporal";

test("compact water history accounts odd dimensions, alternates ownership and resets without reallocating", () => {
  Object.assign(globalThis, {
    GPUTextureUsage: { TEXTURE_BINDING: 1, STORAGE_BINDING: 2 },
    GPUBufferUsage: { UNIFORM: 1, COPY_DST: 2 },
  });
  const resources: { destroyed: number }[] = [];
  const writes: number[] = [];
  const groups: GPUBindGroupDescriptor[] = [];
  const make = () => {
    const state = { destroyed: 0 };
    resources.push(state);
    return { ...state, createView: () => state, destroy: () => state.destroyed++ };
  };
  const device = {
    limits: { maxTextureDimension2D: 4096 },
    createTexture: make,
    createBuffer: make,
    createShaderModule: () => ({}),
    createComputePipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createRenderPipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createBindGroup: (d: GPUBindGroupDescriptor) => {
      groups.push(d);
      return {};
    },
    queue: { writeBuffer: (_b: unknown, _o: number, data: Uint32Array) => writes.push(data[4]) },
  } as unknown as GPUDevice;
  const draw = {
    setPipeline: () => {},
    setBindGroup: () => {},
    dispatchWorkgroups: () => {},
    draw: () => {},
    end: () => {},
  };
  const encoder = {
    beginComputePass: () => draw,
    beginRenderPass: () => draw,
  } as unknown as GPUCommandEncoder;
  const target = { width: 7, height: 5, sampleCount: 1, createView: () => ({}) } as unknown as GPUTexture;
  const history = new WaterTemporalResolve(device, 7, 5);
  expect(history.byteLength).toBe(4 * 3 * 40 + 48);
  history.encode(encoder, target, target, target);
  history.encode(encoder, target, target, target);
  history.reset();
  history.encode(encoder, target, target, target);
  expect(writes).toEqual([0, 1, 0]);
  const entries = groups.map((g) => Array.from(g.entries));
  expect(entries[0][3].resource).toBe(entries[2][5].resource);
  expect(entries[0][5].resource).toBe(entries[2][3].resource);
  expect(() => history.encode(encoder, target, target, { ...target, sampleCount: 4 } as GPUTexture)).toThrow(
    "single-sample",
  );
  history.destroy();
  history.destroy();
  expect(resources.map((r) => r.destroyed)).toEqual([1, 1, 1, 1, 1, 1]);
  expect(() => history.encode(encoder, target, target, target)).toThrow("destroyed");
});
