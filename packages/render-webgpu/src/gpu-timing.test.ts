import { expect, test } from "bun:test";
import { consumeMappedTiming, decodeGpuTiming } from "./gpu-timing";

test("GPU frame extent includes overlapping and reordered pass intervals without double counting", () => {
  const t = new BigUint64Array(
    [300, 400, 100, 500, 200, 800, 400, 1100, 700, 1200, 600, 1000].map((n) => BigInt(n) * 1000n),
  );
  const result = decodeGpuTiming(t, 42, true, true, false);
  expect(result.gpuMs).toBe(1.1);
  expect(result.frame).toBe(42);
  expect(result.intervals?.[1].startMs).toBe(0);
  expect(result.sceneMs).toBe(1.2999999999999998);
  const fused = decodeGpuTiming(t, 43, false, true, true);
  expect(fused.temporalMs).toBe(0.5);
  expect(fused.displayMs).toBe(0);
  expect(fused.intervals?.length).toBe(4);
  t[3] = 0n;
  expect(() => decodeGpuTiming(t, 1, false, false, false)).toThrow();
});

test("reused atmosphere excludes stale query slots from the current frame", () => {
  const t = new BigUint64Array(
    [1, 2, 1000000, 1000002, 1000001, 1000007, 1000004, 1000009, 1000009, 1000012, 0, 0].map(BigInt),
  );
  const sample = decodeGpuTiming(t, 1, true, true, true, false);
  expect(sample.atmosphereMs).toBe(0);
  expect(sample.gpuMs).toBe(12 / 1e6);
  expect(sample.intervals?.some((interval) => interval.pass === "atmosphere")).toBe(false);
});

test("empty shadow passes ignore unwritten end timestamps", () => {
  const t = new BigUint64Array([10, 20, 30, 0, 40, 50, 0, 0, 45, 60, 0, 0].map(BigInt));
  const sample = decodeGpuTiming(t, 1, false, false, false, true, false);
  expect(sample.shadowMs).toBe(0);
  expect(sample.gpuMs).toBe(50 / 1e6);
  expect(sample.intervals?.some((interval) => interval.pass === "shadow")).toBe(false);
});

test("rejected timing decoding releases the readback for the next frame", () => {
  let mapped = true;
  const read = {
    getMappedRange: () => new BigUint64Array([10, 20, 30, 0, 40, 50, 0, 0, 45, 60, 0, 0].map(BigInt)).buffer,
    unmap: () => {
      mapped = false;
      return undefined;
    },
  };
  expect(() => consumeMappedTiming(read, (t) => decodeGpuTiming(t, 1, false, false, false))).toThrow();
  expect(mapped).toBe(false);
});

test("thin coverage intervals extend frame bounds without folding them into color cost", () => {
  const t = new BigUint64Array([0, 0, 0, 0, 40, 80, 0, 0, 70, 90, 0, 0, 10, 60].map(BigInt));
  const sample = decodeGpuTiming(t, 7, false, false, false, false, false, true);
  expect(sample.gpuMs).toBe(80 / 1e6);
  expect(sample.thinDepthMs).toBe(50 / 1e6);
  expect(sample.sceneMs).toBe(40 / 1e6);
  const opaque = decodeGpuTiming(t, 8, false, false, false, false, false);
  expect(opaque.gpuMs).toBe(50 / 1e6);
  expect(opaque.thinDepthMs).toBe(0);
});

test("wave synthesis is included even when the sky and shadow passes are reused", () => {
  const t = new BigUint64Array([0, 0, 0, 0, 40, 80, 60, 90, 85, 100, 0, 0, 0, 0, 10, 55].map(BigInt));
  const sample = decodeGpuTiming(t, 9, true, false, false, false, false, false, true);
  expect(sample.gpuMs).toBe(90 / 1e6);
  expect(sample.intervals?.find((interval) => interval.pass === "water-spectrum")).toEqual({
    pass: "water-spectrum",
    startMs: 0,
    endMs: 45 / 1e6,
  });
  const frozen = decodeGpuTiming(t, 10, true, false, false, false, false, false, false);
  expect(frozen.gpuMs).toBe(60 / 1e6);
});

test("surface relighting extends frame timing only on frames that actually resolve it", () => {
  const t = new BigUint64Array([0, 0, 0, 0, 40, 80, 0, 0, 70, 90, 0, 0, 0, 0, 0, 0, 10, 45].map(BigInt));
  const lit = decodeGpuTiming(t, 1, false, false, false, false, false, false, false, true);
  expect(lit.indirectMs).toBe(35 / 1e6);
  expect(lit.gpuMs).toBe(80 / 1e6);
  const reused = decodeGpuTiming(t, 2, false, false, false, false, false, false, false, false);
  expect(reused.indirectMs).toBe(0);
  expect(reused.gpuMs).toBe(50 / 1e6);
});
