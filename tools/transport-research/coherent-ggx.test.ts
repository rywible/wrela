import { describe, expect, test } from "bun:test";
import {
  coherentOrbit,
  directResponse,
  integrateOrbit,
  type Lighting,
  orbitFixture,
  orbitSample,
  prepareOrbit,
  quadraticAt,
  refineOrbit,
  ringDensity,
  seededRandom,
  slopeAt,
  smoothNumerator,
  type WaveOrbit,
  warpedOrbit,
} from "./coherent-ggx";
import { ggxSlope } from "./slope-atlas";
import { tau } from "./spectral";

describe("coherent GGX integration", () => {
  test("many equal-carrier waves reduce exactly to an ellipse", () => {
    const slopes: [number, number][] = [
        [0.1, 0.03],
        [-0.02, 0.1],
        [0.07, -0.04],
      ],
      offsets = [0.3, 1.7, 2.9];
    const orbit = coherentOrbit(slopes, offsets);
    for (let i = 0; i < 100; i++) {
      const theta = i * 0.723,
        result = slopeAt(orbit, Math.cos(theta), Math.sin(theta));
      for (let j = 0; j < 2; j++)
        expect(result[j]).toBeCloseTo(
          slopes.reduce((sum, v, k) => sum + v[j] * Math.cos(theta + offsets[k]), 0),
          12,
        );
    }
  });
  test("factored quadratic reproduces full vector GGX response at moving directions", () => {
    for (let i = 0; i < 40; i++) {
      const { orbit, lighting } = orbitFixture((i % 8) / 7, Math.floor(i / 8) / 4);
      const plan = prepareOrbit(orbit, lighting);
      if (!plan) throw new Error("Nonsingular orbit expected");
      for (let j = 0; j < 80; j++) {
        const slope = slopeAt(orbit, Math.cos(j * 0.27), Math.sin(j * 0.27));
        const truth = directResponse(slope, lighting),
          factored = smoothNumerator(slope, lighting) / quadraticAt(plan, slope) ** 2;
        expect(Math.abs(truth - factored) / Math.max(truth, 1e-12)).toBeLessThan(1e-10);
      }
    }
  });
  test("closed ring convolution agrees with independent angular integration", () => {
    for (const radius of [0, 0.04, 0.15])
      for (const alpha of [0.0064, 0.0324, 0.2])
        for (const x of [0, 0.06, 0.15, 0.4]) {
          let sum = 0;
          for (let i = 0; i < 8192; i++) {
            const theta = (tau * (i + 0.5)) / 8192;
            sum += ggxSlope(x - radius * Math.cos(theta), 0.013 - radius * Math.sin(theta), alpha);
          }
          const truth = sum / 8192;
          expect(Math.abs(ringDensity(radius, alpha, [x, 0.013]) - truth) / truth).toBeLessThan(1e-11);
        }
  });
  test("bounded rejection uses a normalized mixture, including forced uniform fallback", () => {
    const { orbit, lighting } = orbitFixture(0.13, 0.63),
      plan = prepareOrbit(orbit, lighting);
    if (!plan) throw new Error("Nonsingular orbit expected");
    const reference = integrateOrbit(orbit, lighting, 32768);
    for (const cap of [0, 1, 4]) {
      const random = seededRandom(124701 + cap),
        count = 131072;
      let sum = 0,
        squares = 0,
        fallback = 0;
      for (let i = 0; i < count; i++) {
        const sample = orbitSample(plan, random, cap);
        expect(sample.attempts).toBeLessThanOrEqual(cap);
        sum += sample.value;
        squares += sample.value ** 2;
        fallback += Number(sample.fallback);
      }
      const mean = sum / count,
        variance = squares / count - mean * mean;
      expect(Math.abs(mean - reference)).toBeLessThan(6 * Math.sqrt(variance / count) + 1e-8);
      expect(Math.abs(fallback / count - (1 - plan.acceptance) ** cap)).toBeLessThan(0.006);
    }
  });
  test("degenerate orbit rejects the two-dimensional inverse instead of inventing a result", () => {
    const orbit: WaveOrbit = { mean: [0, 0], a: [0.1, 0], b: [0.2, 0] };
    const lighting: Lighting = { view: [0, 1, 0], light: [0, 1, 0], roughness: 0.08, f0: 0.02 };
    expect(prepareOrbit(orbit, lighting)).toBeNull();
  });
  test("pole control preserves the integral across moving views and rejection caps", () => {
    for (const [x, y] of [
      [0.13, 0.63],
      [0.35, 0.31],
      [0.72, 0.19],
    ]) {
      const { orbit, lighting } = orbitFixture(x, y, 0.04);
      const initial = prepareOrbit(orbit, lighting);
      if (!initial) throw new Error("Nonsingular orbit expected");
      const plan = refineOrbit(initial),
        truth = integrateOrbit(orbit, lighting, 65536);
      for (const cap of [0, 1, 4]) {
        const random = seededRandom(573193 + cap),
          count = 262144;
        let sum = 0,
          squares = 0;
        for (let i = 0; i < count; i++) {
          const value = orbitSample(plan, random, cap).value;
          sum += value;
          squares += value * value;
        }
        const mean = sum / count,
          variance = Math.max(0, squares / count - mean * mean);
        expect(Math.abs(mean - truth)).toBeLessThan(6 * Math.sqrt(variance / count) + 1e-8);
      }
    }
  });
  test("randomly shifted Mobius quadrature preserves the integral and positivity", () => {
    for (const [x, y] of [
      [0.13, 0.63],
      [0.35, 0.31],
      [0.72, 0.19],
    ]) {
      const { orbit, lighting } = orbitFixture(x, y, 0.06),
        initial = prepareOrbit(orbit, lighting);
      if (!initial) throw new Error("Nonsingular orbit expected");
      const plan = refineOrbit(initial),
        truth = integrateOrbit(orbit, lighting, 65536);
      for (const nodes of [1, 2, 4, 8]) {
        const random = seededRandom(919031),
          count = 65536;
        let sum = 0,
          squares = 0,
          minimum = Infinity;
        for (let i = 0; i < count; i++) {
          const value = warpedOrbit(plan, nodes, random());
          sum += value;
          squares += value * value;
          minimum = Math.min(minimum, value);
        }
        const mean = sum / count,
          variance = Math.max(0, squares / count - mean * mean);
        expect(Math.abs(mean - truth)).toBeLessThan(6 * Math.sqrt(variance / count) + 1e-8);
        if (nodes >= 2) expect(minimum).toBeGreaterThanOrEqual(0);
      }
    }
  });
});
