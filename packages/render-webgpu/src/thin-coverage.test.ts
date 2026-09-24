import { expect, test } from "bun:test";
import type { ThinCoverage } from "@wrela/model";

import { createThinCoverageGpu, uploadThinCoverageGpu } from "./thin-coverage";

function harness(coverage: ThinCoverage) {
  Object.assign(globalThis, { GPUTextureUsage: { TEXTURE_BINDING: 1, COPY_DST: 2 } });
  const stored: Uint8Array[] = coverage.levels.map((level) => new Uint8Array(level.length));
  const writes: number[] = [];
  const texture = { createView: () => ({}) };
  const device = {
    createTexture: () => texture,
    queue: {
      writeTexture: (
        destination: { mipLevel: number; origin: { x: number; y: number; z?: number } },
        data: Uint8Array,
        layout: { offset: number; bytesPerRow: number },
        extent: { width: number; height: number },
      ) => {
        const width = Math.max(1, coverage.width >> destination.mipLevel);
        const height = Math.max(1, coverage.height >> destination.mipLevel);
        const layerOffset = (destination.origin.z ?? 0) * width * height;
        const stride = coverage.format === "coverage-normal" ? 4 : 1;
        for (let row = 0; row < extent.height; row++)
          for (let x = 0; x < extent.width * stride; x++)
            stored[destination.mipLevel][
              (layerOffset + (destination.origin.y + row) * width + destination.origin.x) * stride + x
            ] = data[layout.offset + row * layout.bytesPerRow + x];
        writes.push(extent.width * extent.height * stride);
      },
    },
  } as unknown as GPUDevice;
  return { device, stored, writes };
}
const coverage: ThinCoverage = {
  version: 1,
  key: "unequal-dimensions",
  width: 8,
  height: 4,
  uv: new Float32Array([0, 0]),
  levels: [32, 8, 2, 1].map((length, mip) => Uint8Array.from({ length }, (_, index) => 1 + index + mip * 40)),
};

test("deferred coverage makes bounded progress even when the frame budget is smaller than a row or mip", () => {
  const { device, stored, writes } = harness(coverage);
  const resource = createThinCoverageGpu(device, coverage, { deferUpload: true });
  expect(resource.uploaded).toBe(0);
  expect(writes).toEqual([]);
  expect(uploadThinCoverageGpu(device, resource, coverage, 0)).toBe(0);
  let frames = 0;
  while (resource.uploaded < resource.bytes && frames++ < 20) {
    const before = resource.uploaded;
    const bytes = uploadThinCoverageGpu(device, resource, coverage, 3);
    expect(bytes).toBeGreaterThan(0);
    expect(bytes).toBeLessThanOrEqual(3);
    expect(resource.uploaded - before).toBe(bytes);
  }
  expect(frames).toBe(15);
  expect(resource.uploaded).toBe(43);
  expect(stored).toEqual(coverage.levels);
  expect(writes.reduce((sum, bytes) => sum + bytes, 0)).toBe(43);
  expect(uploadThinCoverageGpu(device, resource, coverage, 100)).toBe(0);
});

test("default creation uploads complete rectangular mip chains with exact byte accounting", () => {
  const { device, stored, writes } = harness(coverage);
  const resource = createThinCoverageGpu(device, coverage);
  expect(resource.bytes).toBe(43);
  expect(resource.uploaded).toBe(resource.bytes);
  expect(stored).toEqual(coverage.levels);
  expect(writes).toEqual([32, 8, 2, 1]);
});

test("orientation coverage uploads whole texels under arbitrary byte budgets", () => {
  const rgba: ThinCoverage = {
    ...coverage,
    format: "coverage-normal",
    levels: coverage.levels.map((level) => Uint8Array.from({ length: level.length * 4 }, (_, i) => i % 255)),
  };
  const { device, stored, writes } = harness(rgba);
  const resource = createThinCoverageGpu(device, rgba, { deferUpload: true });
  expect(uploadThinCoverageGpu(device, resource, rgba, 3)).toBe(0);
  for (let frame = 0; resource.uploaded < resource.bytes && frame < 100; frame++) {
    const bytes = uploadThinCoverageGpu(device, resource, rgba, 7);
    expect(bytes).toBe(4);
  }
  expect(stored).toEqual(rgba.levels);
  expect(resource.uploaded).toBe(resource.bytes);
  expect(writes.reduce((sum, n) => sum + n, 0)).toBe(resource.bytes);
});

test("layer uploads never spill rows into another module or mix its mip chain", () => {
  const array: ThinCoverage = {
    ...coverage,
    layers: 3,
    layer: new Uint16Array([2]),
    levels: coverage.levels.map((level) => Uint8Array.from({ length: level.length * 3 }, (_, i) => i % 255)),
  };
  const { device, stored } = harness(array);
  const resource = createThinCoverageGpu(device, array, { deferUpload: true });
  while (resource.uploaded < resource.bytes)
    expect(uploadThinCoverageGpu(device, resource, array, 19)).toBeLessThanOrEqual(19);
  expect(stored).toEqual(array.levels);
});
