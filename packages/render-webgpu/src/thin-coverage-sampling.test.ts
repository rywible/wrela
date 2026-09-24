import { expect, test } from "bun:test";
import { thinCoverageHashReference, thinCoverageThresholdReference } from "./thin-coverage-sampling";

test("footprints larger than the whole field retain uniform expected coverage", () => {
  const samples = 65536;
  for (const footprint of [256, 1024, 65536])
    for (const blend of [0.25, 0.5, 0.75]) {
      let accepted = 0;
      for (let sample = 0; sample < samples; sample++) {
        const salt: [number, number] = [(sample % 256) * 67, Math.floor(sample / 256) * 103];
        if (thinCoverageThresholdReference([0.5, 0.5], footprint, blend, salt) < 0.1) accepted++;
      }
      expect(Math.abs(accepted / samples - 0.1)).toBeLessThan(0.006);
    }
  // The former equal-cell hashes produce this concrete wrong distribution.
  let previous = 0;
  for (let sample = 0; sample < samples; sample++) {
    const value = thinCoverageHashReference((sample % 256) * 67, Math.floor(sample / 256) * 103);
    const threshold = value < 0.5 ? 2 * value * value : 1 - 2 * (1 - value) ** 2;
    if (threshold < 0.1) previous++;
  }
  expect(previous / samples).toBeGreaterThan(0.21);
});

test("adjacent footprint levels meet at exactly the same threshold", () => {
  for (const footprint of [1, 2, 16, 256, 1024, 65536])
    for (const position of [
      [0.5, 0.5],
      [63.2, 209.1],
      [-71.8, 53.2],
    ] as const) {
      for (const salt of [
        [0, 0],
        [170, 671],
        [-987, 23],
      ] as const) {
        expect(thinCoverageThresholdReference(position, footprint, 1, salt)).toBe(
          thinCoverageThresholdReference(position, footprint * 2, 0, salt),
        );
        expect(
          Math.abs(
            thinCoverageThresholdReference(position, footprint, 1 - 1e-5, salt) -
              thinCoverageThresholdReference(position, footprint * 2, 1e-5, salt),
          ),
        ).toBeLessThan(3e-5);
      }
    }
});
