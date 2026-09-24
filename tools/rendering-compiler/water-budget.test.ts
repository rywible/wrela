import { expect, test } from "bun:test";
import { WATER_HIGHLIGHT_SAMPLE_BUDGET, waterHighlightCounts } from "@wrela/render-webgpu/water-budget";
import {
  integrateWaterBoxReference,
  type WaterReferenceQuery,
  waterReferenceQueries,
} from "./water-reference";

function footprintVariation(query: WaterReferenceQuery): [number, number, number] {
  const footprint = query.footprint ?? 0;
  const variation: [number, number, number] = [0, 0, 0];
  // Independent analytic bound for this test's two authored cosine waves.
  for (const [direction, phase, speed] of [
    [0, 0, 1],
    [query.direction, Math.PI / 2, query.secondSpeed ?? 1],
  ]) {
    const delta = [
      Math.PI * Math.abs(Math.cos(direction)) * footprint,
      Math.PI * Math.abs(Math.sin(direction)) * footprint,
      Math.PI * speed * query.shutter,
    ];
    for (let axis = 0; axis < 3; axis++) {
      variation[axis] +=
        0.1 *
        Math.min(
          2,
          delta[axis],
          Math.abs(Math.sin(phase - 2 * Math.PI * speed * query.time)) * delta[axis] + 0.5 * delta[axis] ** 2,
        );
    }
  }
  const motion = (0.5 * footprint) / (10 - footprint);
  const halfLength = Math.hypot(...query.view.map((value, axis) => value + query.light[axis]));
  variation[0] += motion / Math.max(halfLength - 2 * motion, 0.0001);
  variation[1] += motion / Math.max(halfLength - 2 * motion, 0.0001);
  return variation;
}

test("sharp water quadrature never multiplies per-axis limits beyond its total response budget", () => {
  for (const roughness of [0.001, 0.06, 0.18, 0.5, 1])
    for (const x of [0, 1e-8, 0.001, 0.1, 100, 1e20])
      for (const y of [0, 1e-8, 0.001, 0.1, 100, 1e20])
        for (const t of [0, 1e-8, 0.001, 0.1, 100, 1e20]) {
          const counts = waterHighlightCounts([x, y, t], roughness);
          expect(counts.reduce((product, count) => product * count, 1)).toBeLessThanOrEqual(
            WATER_HIGHLIGHT_SAMPLE_BUDGET,
          );
          expect(
            counts.every((count) => count >= 1 && count <= 32 && Number.isInteger(Math.log2(count))),
          ).toBe(true);
        }
  expect(waterHighlightCounts([0, 0, 0], 0.06)).toEqual([1, 1, 1]);
});

test("bounded sharp water preserves the independent spatial and shutter reference domain", () => {
  let squaredError = 0;
  let squaredEnergy = 0;
  let maximumError = 0;
  for (const query of waterReferenceQueries().filter((query) => query.coherent === false)) {
    const counts = waterHighlightCounts(footprintVariation(query), query.roughness);
    const actual = integrateWaterBoxReference(query, counts);
    const coarse = integrateWaterBoxReference(query, 32);
    let reference = integrateWaterBoxReference(query, 64);
    const difference = Math.hypot(...coarse.map((value, axis) => value - reference[axis]));
    if (difference / Math.max(0.01, Math.hypot(...reference)) > 1e-6) {
      reference = integrateWaterBoxReference(query, 128);
    }
    const error = Math.hypot(...actual.map((value, axis) => value - reference[axis]));
    const energy = Math.hypot(...reference);
    maximumError = Math.max(maximumError, error / Math.max(0.01, energy));
    squaredError += error ** 2;
    squaredEnergy += energy ** 2;
  }
  expect(Math.sqrt(squaredError / squaredEnergy)).toBeLessThan(0.01);
  expect(maximumError).toBeLessThan(0.05);
}, 30000);
