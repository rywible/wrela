import { describe, expect, test } from "bun:test";
import { shape } from "@wrela/examples";
import type { FieldDefinition } from "@wrela/model";
import { lower, value } from "../field-research/local-program";
import { boundedOpticalDepth, quadratureOpticalDepth, segmentBound, topDistance } from "./atmosphere";
import { certifiedInnerSphere } from "./field-occluders";
import {
  compilePositive,
  integralSigned,
  multiplyPrograms,
  orbitIntegral,
  positiveIntegral,
  signedModes,
} from "./positive-response";
import { besselJ0, compileSlopeAtlas, ggxCharacteristic, ggxSlope, sampleSlopeAtlas } from "./slope-atlas";
import {
  compileSpectrum,
  evaluatePlan,
  type Footprint,
  footprintReuseBound,
  integrateResponse,
  planSpectrum,
  tau,
} from "./spectral";
import {
  compileOccluders,
  crowdFixture,
  hidden,
  projectedSphere,
  sphereFront,
  tileOccluder,
} from "./visibility";

describe("compiled transport research", () => {
  test("field compiler proves interior occluders against complete CSG and rejects holes", () => {
    const definition: FieldDefinition = {
      root: "cut",
      resolution: 24,
      bounds: { min: [-3, -3, -3], max: [3, 3, 3] },
      nodes: [
        { ...shape("cut", "cut", [0, 0, 0], [1, 1, 1], "subtract"), children: ["outer", "hole"] },
        { ...shape("outer", "outer", [0, 0, 0], [2, 2, 2], "sphere"), radius: 2 },
        { ...shape("hole", "hole", [0, 0, 0], [0.6, 0.6, 0.6], "sphere"), radius: 0.6 },
      ],
    };
    const expression = lower(definition);
    expect(certifiedInnerSphere(expression, [0, 0, 0], 1)).toBeNull();
    const sphere = certifiedInnerSphere(expression, [1.2, 0, 0], 1);
    if (!sphere) throw new Error("Solid shell must admit an interior certificate");
    expect(sphere.radius).toBeGreaterThan(0.1);
    for (let i = 0; i < 1000; i++) {
      const z = (2 * (i + 0.5)) / 1000 - 1,
        xy = Math.sqrt(1 - z * z),
        angle = i * 2.39996323;
      expect(
        value(expression, [
          sphere.x + sphere.radius * xy * Math.cos(angle),
          sphere.radius * xy * Math.sin(angle),
          sphere.radius * z,
        ]),
      ).toBeLessThan(0);
    }
  });
  test("view-independent GGX characteristic has the correct limits and reconstructs a normalized density", () => {
    expect(ggxCharacteristic(0)).toBe(1);
    expect(ggxCharacteristic(1)).toBeCloseTo(0.6019072301972346, 12);
    expect(besselJ0(0)).toBe(1);
    expect(besselJ0(1)).toBeCloseTo(0.7651976865579666, 12);
    const atlas = compileSlopeAtlas([], 0.12, 64, 2);
    const mass = atlas.values.reduce((a, b) => a + b, 0) * (atlas.period / atlas.size) ** 2;
    expect(mass).toBeCloseTo(1, 12);
    expect(Math.min(...atlas.values)).toBeGreaterThan(0);
    expect(Math.abs(sampleSlopeAtlas(atlas, 0, 0) - ggxSlope(0, 0, 0.12))).toBeLessThan(0.01);
  });
  test("square-root response filtering is positive and equals direct integration of its squared amplitude", () => {
    const f = (a: number, b: number) => (1.2 + 0.8 * Math.cos(a) + 0.25 * Math.sin(2 * b)) ** 2;
    const positive = compilePositive(f, 2, 32);
    const footprint: Footprint = { dx: [8.1, 3.4], dy: [-2, 7], shutter: [0, 0] };
    for (let i = 0; i < 30; i++) {
      const a = 0.7 * i,
        b = 0.3 * i,
        filtered = positiveIntegral(positive.signed, a, b, footprint);
      expect(filtered).toBeGreaterThan(0);
      expect(Math.abs(filtered - integrateResponse(f, a, b, footprint, 512))).toBeLessThan(0.00001);
    }
  });
  test("integer orbit grouping preserves coherent beats, including phase shifts and negative frequencies", () => {
    const positive = compilePositive((a, b) => (1 + 0.3 * Math.cos(a) + 0.2 * Math.sin(b)) ** 2, 2, 32);
    for (const winding of [
      [1, 1],
      [2, -1],
      [0, 1],
    ] as [number, number][]) {
      const footprint: Footprint = {
        dx: winding.map((x) => tau * 8 * x) as [number, number],
        dy: [0, 0],
        shutter: [0, 0],
      };
      for (let i = 0; i < 20; i++)
        expect(orbitIntegral(positive.signed, i * 0.3, i * 0.7, winding)).toBeCloseTo(
          positiveIntegral(positive.signed, i * 0.3, i * 0.7, footprint),
          12,
        );
    }
  });
  test("light/material products retain covariance that separate averages lose", () => {
    const footprint: Footprint = { dx: [tau, 0], dy: [0, 0], shutter: [0, 0] };
    const light = signedModes({ dc: 1, modes: [{ m: 1, n: 0, re: 0.4, im: 0 }] });
    const material = signedModes({ dc: 0.5, modes: [{ m: 1, n: 0, re: -0.2, im: 0 }] });
    const joint = integralSigned(multiplyPrograms(light, material), 0, 0, footprint);
    expect(joint).toBeCloseTo(0.34, 12);
    expect(integralSigned(light, 0, 0, footprint) * integralSigned(material, 0, 0, footprint)).toBeCloseTo(
      0.5,
      12,
    );
  });
  test("Fourier lowering preserves mixed phases and integrates them through space and shutter", () => {
    const f = (a: number, b: number) => 2 + 0.7 * Math.cos(3 * a - 2 * b) - 0.4 * Math.sin(a + b);
    const spectrum = compileSpectrum(f, 32);
    const footprint: Footprint = { dx: [3.2, -0.9], dy: [0.8, 2.1], shutter: [-1.1, 0.8] };
    const plan = planSpectrum(spectrum, footprint, 2);
    expect(plan.omittedL1).toBeLessThan(1e-12);
    for (const [a, b] of [
      [0, 0],
      [1.1, 2.4],
      [4.2, 0.7],
    ]) {
      expect(Math.abs(evaluatePlan(plan, a, b) - integrateResponse(f, a, b, footprint, 80, 40))).toBeLessThan(
        0.00015,
      );
    }
  });
  test("equal fast phases retain their visible beat; independent filtering would erase it", () => {
    const spectrum = compileSpectrum((a, b) => 1 + Math.cos(a - b), 16);
    const plan = planSpectrum(spectrum, { dx: [100 * tau, 100 * tau], dy: [0, 0], shutter: [0, 0] }, 1);
    expect(evaluatePlan(plan, 0, 0)).toBeCloseTo(2, 12);
    expect(evaluatePlan(plan, Math.PI, 0)).toBeCloseTo(0, 12);
  });
  test("discarded coefficients and changed footprints have phase-independent error bounds", () => {
    const spectrum = compileSpectrum((a, b) => Math.exp(0.8 * Math.cos(a) + 0.5 * Math.sin(a + b)), 32);
    const from: Footprint = { dx: [1, 2], dy: [3, 1], shutter: [0, 0] };
    const to: Footprint = { dx: [1.03, 2.02], dy: [2.97, 1.03], shutter: [0.05, 0.01] };
    const small = planSpectrum(spectrum, from, 4),
      full = planSpectrum(spectrum, from, Infinity);
    const changed = planSpectrum(spectrum, to, Infinity),
      bound = footprintReuseBound(spectrum, from, to);
    for (let i = 0; i < 100; i++) {
      const a = i * 0.37,
        b = i * 0.63;
      expect(Math.abs(evaluatePlan(small, a, b) - evaluatePlan(full, a, b))).toBeLessThanOrEqual(
        small.omittedL1 + 1e-12,
      );
      expect(Math.abs(evaluatePlan(changed, a, b) - evaluatePlan(full, a, b))).toBeLessThanOrEqual(
        bound + 1e-12,
      );
    }
  });
  test("atmospheric chord/tangent enclose independently integrated optical depth, including horizon", () => {
    for (const height of [0, 1, 20])
      for (const cosine of [0, 0.001, 0.1, 0.5, 1])
        for (const scale of [1.2, 8]) {
          const ray = { height, cosine, scale, length: topDistance(height, cosine) };
          const exact = quadratureOpticalDepth(ray),
            result = boundedOpticalDepth(ray, 0.03, 1e-5, 256);
          expect(exact).toBeGreaterThanOrEqual(result.lower - 1e-9);
          expect(exact).toBeLessThanOrEqual(result.upper + 1e-9);
          expect(result.transmissionUpper - result.transmissionLower).toBeLessThanOrEqual(1.00001e-5);
          if (cosine === 1) expect(result.segments.length).toBe(1);
        }
  });
  test("a curved density segment is not treated as a constant-density slab", () => {
    const ray = { height: 0, cosine: 0, scale: 8, length: 1000 };
    const bound = segmentBound(ray, 0, ray.length),
      exact = quadratureOpticalDepth(ray);
    expect(bound.lower).toBeLessThan(exact);
    expect(bound.upper).toBeGreaterThan(exact);
  });
  test("four perspective corners bound the entire convex occluder tile", () => {
    for (let i = 0; i < 40; i++) {
      const sphere = { x: Math.sin(i) * 2, y: Math.cos(i * 0.3), z: 3 + i * 0.1, radius: 0.7 };
      const p = projectedSphere(sphere);
      if (!p) throw new Error("Forward sphere must have finite projection");
      const cx = (p.x0 + p.x1) / 2,
        cy = (p.y0 + p.y1) / 2;
      const rx = (p.x1 - p.x0) * 0.3,
        ry = (p.y1 - p.y0) * 0.3;
      const rect = { x0: cx - rx, x1: cx + rx, y0: cy - ry, y1: cy + ry };
      const bound = tileOccluder(sphere, rect);
      expect(Number.isFinite(bound)).toBe(true);
      for (let y = 0; y <= 10; y++)
        for (let x = 0; x <= 10; x++) {
          expect(
            sphereFront(
              sphere,
              rect.x0 + ((rect.x1 - rect.x0) * x) / 10,
              rect.y0 + ((rect.y1 - rect.y0) * y) / 10,
            ),
          ).toBeLessThanOrEqual(bound + 1e-10);
        }
    }
    expect(tileOccluder({ x: 0, y: 0, z: 1, radius: 2 }, { x0: -1, x1: 1, y0: -1, y1: 1 })).toBe(Infinity);
  });
  test("crowd culling never labels sampled visible sphere points hidden", () => {
    const { grid, occluders, candidates } = crowdFixture(),
      depths = compileOccluders(occluders, grid);
    let culled = 0,
      witnessed = 0;
    for (const sphere of candidates) {
      if (!hidden(sphere, grid, depths)) continue;
      culled++;
      const rect = projectedSphere(sphere);
      if (!rect) throw new Error("Forward sphere must have finite projection");
      for (let y = 0; y < 5; y++)
        for (let x = 0; x < 5; x++) {
          const u = rect.x0 + ((x + 0.5) / 5) * (rect.x1 - rect.x0),
            v = rect.y0 + ((y + 0.5) / 5) * (rect.y1 - rect.y0);
          const front = sphereFront(sphere, u, v);
          if (!Number.isFinite(front)) continue;
          const occluder = Math.min(...occluders.map((s) => sphereFront(s, u, v)));
          expect(front).toBeGreaterThan(occluder);
          witnessed++;
        }
    }
    expect(culled).toBeGreaterThan(1000);
    expect(witnessed).toBeGreaterThan(10000);
  });
});
