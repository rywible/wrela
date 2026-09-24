import { expect, test } from "bun:test";
import {
  aggregateImageDifferences,
  compareFrontierImages,
  compareScreenErrorDelta,
  observedDistribution,
  silhouetteDistance,
} from "./frontier-image-metrics";

test("HDR differences exclude alpha, retain maxima and use a finite binary16 comparison envelope", () => {
  const reference = new Float32Array([1, 0.5, 0.25, 0]),
    value = new Float32Array([1.125, 0.5, 0.25, 1]);
  const measured = compareFrontierImages(reference, value);
  expect(measured.maximum).toBe(0.125);
  expect(measured.rms).toBeCloseTo(0.125 / Math.sqrt(3), 12);
  expect(measured.samples).toBe(3);
  expect(measured.halfFormatBound).toBeGreaterThan(0);
  expect(aggregateImageDifferences([measured, compareFrontierImages(reference, reference)]).rms).toBeCloseTo(
    measured.rms / Math.sqrt(2),
    12,
  );
});
test("screen error delta cancels real image changes and detects changes in approximation error", () => {
  const before = new Float32Array([0, 0, 0, 1]),
    after = new Float32Array([1, 1, 1, 1]);
  expect(compareScreenErrorDelta(before, before, after, after).maximum).toBe(0);
  expect(compareScreenErrorDelta(before, before, after, new Float32Array([0.75, 1, 1, 1])).maximum).toBe(
    0.25,
  );
});
test("finite silhouette distance measures shifts and refuses to invent a distance to a missing contour", () => {
  const mask = (x: number | null) => {
    const value = new Float32Array(5 * 3 * 4);
    if (x !== null) value.fill(1, (5 + x) * 4, (5 + x) * 4 + 3);
    return value;
  };
  expect(silhouetteDistance(mask(1), mask(3), 5, 3)).toBe(2);
  expect(silhouetteDistance(mask(null), mask(null), 5, 3)).toBe(0);
  expect(silhouetteDistance(mask(1), mask(null), 5, 3)).toBeNull();
  const invalid = mask(1);
  invalid[0] = NaN;
  expect(() => silhouetteDistance(invalid, mask(1), 5, 3)).toThrow("binary silhouette");
  expect(() => silhouetteDistance(new Float32Array(), new Float32Array(), 0, 0)).toThrow("dimensions");
});
test("observed timing intervals retain every measured sample without claiming future confidence", () => {
  const result = observedDistribution([1, 1.1, 0.7, 2, 1.3]);
  expect(result.p95 - result.uncertainty).toBeLessThanOrEqual(result.minimum);
  expect(result.p95 + result.uncertainty).toBeGreaterThanOrEqual(result.maximum);
  expect(() => observedDistribution([])).toThrow();
  expect(() => compareFrontierImages(new Float32Array([NaN, 0, 0, 1]), new Float32Array(4))).toThrow(
    "Nonfinite",
  );
});
