import { expect, test } from "bun:test";
import { WaterTransportGpu } from "./water-transport";

/** Resource ownership/control-flow checks; hardware acceptance validates WGSL. */
function harness() {
  Object.assign(globalThis, {
    GPUTextureUsage: { COPY_DST: 1, TEXTURE_BINDING: 2, STORAGE_BINDING: 4 },
    GPUShaderStage: { COMPUTE: 4 },
  });
  const textures: { descriptor: GPUTextureDescriptor; destroyed: number }[] = [];
  let groupCount = 0;
  const device = {
    limits: { maxTextureDimension2D: 4096 },
    createTexture: (descriptor: GPUTextureDescriptor) => {
      const state = { descriptor, destroyed: 0 };
      textures.push(state);
      return {
        createView: () => ({ source: state }),
        destroy: () => {
          state.destroyed++;
        },
      };
    },
    createSampler: () => ({}),
    createBindGroupLayout: () => ({}),
    createPipelineLayout: () => ({}),
    createShaderModule: () => ({}),
    createComputePipeline: () => ({ getBindGroupLayout: () => ({}) }),
    createBindGroup: () => {
      groupCount++;
      return {};
    },
  } as unknown as GPUDevice;
  const events: string[] = [];
  const encoder = {
    copyTextureToTexture: () => {
      events.push("copy");
    },
    beginComputePass: () => ({
      setPipeline: () => {},
      setBindGroup: () => {},
      dispatchWorkgroups: (x: number, y: number) => {
        events.push(`resolve:${x}:${y}`);
      },
      end: () => {
        events.push("end");
      },
    }),
  } as unknown as GPUCommandEncoder;
  const color = { width: 65, height: 33, format: "rgba16float", sampleCount: 1 } as GPUTexture;
  const depth = {
    width: 65,
    height: 33,
    format: "depth32float",
    sampleCount: 4,
    createView: () => ({}),
  } as GPUTexture;
  return { device, encoder, color, depth, textures, events, groups: () => groupCount };
}
test("water transport owns distinct snapshots, accounts memory and releases each once", () => {
  const h = harness();
  const snapshots = new WaterTransportGpu(h.device, 65, 33, 4);
  expect(snapshots.byteLength).toBe(28472);
  expect(snapshots.colorView).not.toBe(snapshots.depthView);
  snapshots.capture(h.encoder, h.color, h.depth);
  snapshots.capture(h.encoder, h.color, h.depth);
  expect(h.groups()).toBe(7);
  expect(h.events.filter((e) => e === "copy")).toHaveLength(2);
  expect(h.events.filter((e) => e === "resolve:9:5")).toHaveLength(2);
  expect(h.events.filter((e) => e === "end")).toHaveLength(4);
  snapshots.destroy();
  snapshots.destroy();
  expect(h.textures.map((texture) => texture.destroyed)).toEqual([1, 1]);
  expect(() => snapshots.capture(h.encoder, h.color, h.depth)).toThrow("destroyed");
});
test("water transport refuses incompatible snapshots before submitting any copies", () => {
  const h = harness();
  expect(() => new WaterTransportGpu(h.device, 0, 33, 4)).toThrow();
  expect(() => new WaterTransportGpu(h.device, 65, 33, 8)).toThrow();
  expect(h.textures).toHaveLength(0);
  const snapshots = new WaterTransportGpu(h.device, 65, 33, 4);
  expect(() => snapshots.capture(h.encoder, h.color, { ...h.depth, sampleCount: 1 } as GPUTexture)).toThrow(
    "match",
  );
  expect(() => snapshots.capture(h.encoder, { ...h.color, width: 64 } as GPUTexture, h.depth)).toThrow(
    "match",
  );
  expect(h.events).toHaveLength(0);
  snapshots.destroy();
});

test("half-width transport retains full depth and accounts odd-width color", () => {
  const h = harness();
  const snapshots = new WaterTransportGpu(h.device, 65, 33, 4, 2);
  expect(snapshots.colorWidth).toBe(33);
  expect(snapshots.depthLevels).toBe(7);
  expect(snapshots.byteLength).toBe(20024);
  snapshots.capture(h.encoder, { ...h.color, createView: () => ({}) } as GPUTexture, h.depth);
  expect(h.events).not.toContain("copy");
  expect(h.events.filter((e) => e.startsWith("resolve:"))).toHaveLength(7);
  snapshots.destroy();
});
