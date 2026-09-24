import type { GpuFrameTiming } from "@wrela/model";

/** Pass intervals can overlap on a tiled GPU. Their sum is not frame duration.
 * Use the earliest observed beginning and latest ending, regardless of encoding order. */
export function decodeGpuTiming(
  t: BigUint64Array,
  frame: number,
  water: boolean,
  temporal: boolean,
  fused: boolean,
  atmosphere = true,
  shadow = true,
  thin = false,
  waterSpectrum = false,
  indirect = false,
): GpuFrameTiming {
  const queries: [string, number, number][] = [["opaque", 4, 5]];
  if (indirect) queries.unshift(["indirect-relight", 16, 17]);
  if (thin) queries.push(["thin-depth", 12, 13]);
  if (waterSpectrum) queries.push(["water-spectrum", 14, 15]);
  if (shadow) queries.unshift(["shadow", 2, 3]);
  if (atmosphere) queries.unshift(["atmosphere", 0, 1]);
  if (water) queries.push(["water", 6, 7]);
  if (temporal && !fused) queries.push(["temporal", 10, 11]);
  queries.push([fused ? "temporal-display" : "display", 8, 9]);
  for (const [, a, b] of queries)
    if (t[a] === undefined || t[b] === undefined || t[b] < t[a])
      throw Error("Invalid GPU timestamp interval");
  const begin = queries.reduce((minimum, [, a]) => (t[a] < minimum ? t[a] : minimum), t[queries[0][1]]);
  const end = queries.reduce((maximum, [, , b]) => (t[b] > maximum ? t[b] : maximum), t[queries[0][2]]);
  const duration = (a: number, b: number) => Number(t[b] - t[a]) / 1e6;
  return {
    frame,
    atmosphereMs: atmosphere ? duration(0, 1) : 0,
    indirectMs: indirect ? duration(16, 17) : 0,
    thinDepthMs: thin ? duration(12, 13) : 0,
    shadowMs: shadow ? duration(2, 3) : 0,
    sceneMs: duration(4, 5) + (water ? duration(6, 7) : 0),
    waterMs: water ? duration(6, 7) : 0,
    temporalMs: fused ? duration(8, 9) : temporal ? duration(10, 11) : 0,
    displayMs: fused ? 0 : duration(8, 9),
    gpuMs: Number(end - begin) / 1e6,
    intervals: queries.map(([pass, a, b]) => ({
      pass,
      startMs: Number(t[a] - begin) / 1e6,
      endMs: Number(t[b] - begin) / 1e6,
    })),
  };
}

/** Timestamp decoding must never leave a pooled readback mapped on failure. */
export function consumeMappedTiming<T>(
  read: Pick<GPUBuffer, "getMappedRange" | "unmap">,
  consume: (timestamps: BigUint64Array) => T,
): T {
  try {
    return consume(new BigUint64Array(read.getMappedRange()));
  } finally {
    read.unmap();
  }
}
