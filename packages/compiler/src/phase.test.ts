import { describe, expect, test } from "bun:test";
import type { WaterDefinition } from "@wrela/model";

import {
  coherentPhaseGroups,
  coherentSlopeOrbit,
  compileWaterPhases,
  evaluateGGXWarp,
  evaluateGGXWarpInterval,
  evaluatePhasePolynomial,
  fitPhasePolynomial,
  integrateCoherentGGX,
  integratePhaseBox,
  integratePhaseInterval,
  type PhaseLighting,
  type PhaseOrbit,
  phaseFootprintFactor,
  phaseGGXFeatureWidth,
  phaseOrbitSlope,
  prepareGGXWarp,
  selectPhasePolynomial,
  splitPhaseInterval,
  waterPhaseFootprint,
} from "./phase";

const tau = 2 * Math.PI;
const water: WaterDefinition = {
  id: "test-water",
  kind: "water",
  name: "Test",
  schemaVersion: 1,
  dependencies: [],
  level: 2,
  color: [0.1, 0.2, 0.3],
  roughness: 0.06,
  waves: Array.from({ length: 8 }, (_, i) => ({
    amplitude: 0.02 * (i + 1),
    wavelength: 1 + i,
    speed: (i - 3) * 0.2,
    direction: 0.7 * i,
    phase: 0.1 * i,
  })),
};
function reference(slope: [number, number], lighting: PhaseLighting) {
  const normalize = (v: number[]) => {
    const d = Math.hypot(...v);
    return v.map((x) => x / d);
  };
  const dot = (a: number[], b: number[]) => a.reduce((s, x, i) => s + x * b[i], 0);
  const n = normalize([-slope[0], 1, -slope[1]]),
    v = normalize(lighting.view),
    l = normalize(lighting.light);
  const h = normalize(v.map((x, i) => x + l[i]));
  const nv = dot(n, v),
    nl = dot(n, l),
    nh = Math.max(0, dot(n, h));
  if (nv <= 0 || nl <= 0) return 0;
  const a2 = lighting.roughness ** 4;
  const lambda = (c: number) => (Math.sqrt(1 + (a2 * (1 - c * c)) / (c * c)) - 1) / 2;
  return (
    ((a2 / (Math.PI * (1 + (a2 - 1) * nh * nh) ** 2) / (1 + lambda(nv) + lambda(nl))) *
      (lighting.f0 + (1 - lighting.f0) * (1 - dot(v, h)) ** 5)) /
    (4 * nv)
  );
}
describe("production water phase programs", () => {
  test("lowering preserves all eight authored carriers and excludes unrelated appearance edits", () => {
    const product = compileWaterPhases(water);
    expect(product.carriers).toHaveLength(8);
    expect(compileWaterPhases({ ...water, roughness: 0.4, color: [1, 0, 0] }).sourceKey).toBe(
      product.sourceKey,
    );
    expect(
      compileWaterPhases({
        ...water,
        waves: water.waves.map((w, i) => (i ? w : { ...w, speed: w.speed + 0.001 })),
      }).sourceKey,
    ).not.toBe(product.sourceKey);
    const f = waterPhaseFootprint(product, [1.2, -2.3], 3.1, [0.3, 0.1], [-0.2, 0.6], 0.03);
    const slope = product.carriers.reduce(
      (s, w, i) => [s[0] + w.slope[0] * Math.cos(f.origin[i]), s[1] + w.slope[1] * Math.cos(f.origin[i])],
      [0, 0],
    );
    const height = (x: number, z: number) =>
      water.waves.reduce(
        (s, w) =>
          s +
          w.amplitude *
            Math.sin(
              (tau / w.wavelength) * (Math.cos(w.direction) * x + Math.sin(w.direction) * z - w.speed * 3.1) +
                w.phase,
            ),
        water.level,
      );
    const e = 1e-5;
    expect(slope[0]).toBeCloseTo((height(1.2 + e, -2.3) - height(1.2 - e, -2.3)) / (2 * e), 8);
    expect(slope[1]).toBeCloseTo((height(1.2, -2.3 + e) - height(1.2, -2.3 - e)) / (2 * e), 8);
  });
  test("correlated footprint retains a beat carrier after individual carriers vanish", () => {
    const f = { origin: [0.3, 0.1], dx: [tau * 12, tau * 12 + 0.2], dy: [0.3, 0.4], shutter: [0.7, 0.8] };
    expect(Math.abs(phaseFootprintFactor([1, 0], f))).toBeLessThan(1e-14);
    const beat = phaseFootprintFactor([1, -1], f);
    expect(beat).toBeGreaterThan(0.99);
    const expected = Math.cos(0.2) * beat;
    const actual = integratePhaseBox(f, (p) => Math.cos(p[0] - p[1]), 24);
    expect(Math.abs(actual - expected)).toBeLessThan(0.00001);
    expect(() => phaseFootprintFactor([1.1, 0], f)).toThrow();
  });
  test("phase polynomial sorts modes after footprint filtering and labels truncation separately", () => {
    const f = { origin: [0.1, 0.4], dx: [tau * 8, tau * 8], dy: [0, 0], shutter: [0, 0] };
    const polynomial = {
      constant: 1,
      terms: [
        { mode: [1, 0], real: 100, imaginary: 0 },
        { mode: [1, -1], real: 0.5, imaginary: 0 },
      ],
      sourceKey: "water",
      parameterKey: "light",
      sourceFit: { kind: "unknown" as const, reason: "test" },
    };
    const result = evaluatePhasePolynomial(polynomial, f, 1);
    expect(result.value).toBeCloseTo(1 + Math.cos(-0.3), 10);
    expect(result.omittedPolynomialBound).toBeLessThan(1e-12);
    expect(selectPhasePolynomial(polynomial, f, "water", "light", 1).selected).toBe(false);
    expect(
      selectPhasePolynomial(
        { ...polynomial, sourceFit: { kind: "measured", rms: 0, maximum: 0, reference: "independent" } },
        f,
        "water",
        "moved-light",
        1,
      ),
    ).toEqual({ selected: false, rejection: "stale-parameters" });
  });
  test("bounded fitting preserves nonlinear correlation and does not invent source-fit evidence", () => {
    const fit = fitPhasePolynomial({
      dimensions: 2,
      nodesPerDimension: 16,
      modes: [
        [1, -1],
        [1, 1],
      ],
      sourceKey: "water",
      parameterKey: "fixed",
      response: (p) => Math.cos(p[0]) * Math.cos(p[1]),
    });
    expect(fit.evaluations).toBe(256);
    expect(fit.polynomial.sourceFit.kind).toBe("unknown");
    const f = { origin: [0.3, 0.7], dx: [3, 2], dy: [1, -1], shutter: [0.2, 0.3] };
    expect(
      Math.abs(
        evaluatePhasePolynomial(fit.polynomial, f).value -
          integratePhaseBox(f, (p) => Math.cos(p[0]) * Math.cos(p[1]), 32),
      ),
    ).toBeLessThan(0.0003);
    expect(() =>
      fitPhasePolynomial({
        dimensions: 8,
        nodesPerDimension: 16,
        modes: [],
        sourceKey: "",
        parameterKey: "",
        response: () => 0,
      }),
    ).toThrow();
  });
  test("exact groups never merge nearly equal rates; finite periods retain the tail", () => {
    expect(coherentPhaseGroups([1, 1, 1 + 1e-12, 0]).map((g) => g.indices)).toEqual([[0, 1], [2], [3]]);
    const start = 0.3,
      length = 3 * tau + 0.8;
    const split = splitPhaseInterval(length);
    expect(split.periods).toBe(3);
    expect(split.periodWeight + split.remainderWeight).toBeCloseTo(1, 14);
    expect(integratePhaseInterval(start, length, Math.sin, 512)).toBeCloseTo(
      (Math.cos(start) - Math.cos(start + length)) / length,
      7,
    );
    expect(integratePhaseInterval(start, 0, Math.sin)).toBe(Math.sin(start));
  });
  test("coherent reduction retains phase offsets, including eight waves", () => {
    const slopes = water.waves.map((w): [number, number] => [
      w.amplitude * Math.cos(w.direction),
      w.amplitude * Math.sin(w.direction),
    ]);
    const offsets = water.waves.map((w) => w.phase);
    const orbit = coherentSlopeOrbit(slopes, offsets);
    for (const angle of [0.1, 1.2, 4.7]) {
      const expected = [0, 1].map((axis) =>
        slopes.reduce((sum, s, i) => sum + s[axis] * Math.cos(angle + offsets[i]), 0),
      );
      phaseOrbitSlope(orbit, angle).forEach((v, i) => {
        expect(v).toBeCloseTo(expected[i], 12);
      });
    }
  });
  test("warped four/eight nodes reproduce an independently integrated rotational GGX orbit", () => {
    const orbit: PhaseOrbit = { mean: [0, 0], a: [0.1, 0], b: [0, 0.1] };
    const lighting: PhaseLighting = { view: [0, 1, 0], light: [0, 1, 0], roughness: 0.06, f0: 0.02 };
    const plan = prepareGGXWarp(orbit, lighting);
    expect(plan).not.toBeNull();
    if (!plan) throw new Error("Unexpected degenerate fixture");
    let expected = 0;
    for (let i = 0; i < 8192; i++) {
      const angle = (tau * (i + 0.5)) / 8192;
      expected += reference([0.1 * Math.cos(angle), 0.1 * Math.sin(angle)], lighting) / 8192;
    }
    for (const nodes of [4, 8] as const)
      for (const shift of [0, 0.13, 0.5, 0.99])
        expect(evaluateGGXWarp(plan, nodes, shift)).toBeCloseTo(expected, 12);
  });
  test("selection rejects unmeasured, collinear, broad and incoherent cases", () => {
    const orbit: PhaseOrbit = { mean: [0, 0], a: [0.1, 0], b: [0, 0.1] };
    const lighting: PhaseLighting = { view: [0, 1, 0], light: [0, 1, 0], roughness: 0.06, f0: 0.02 };
    const options = {
      orbit,
      lighting,
      start: 0.3,
      length: tau * 2 + 0.1,
      rates: [1, 1],
      sourceKey: "source",
      parameterKey: "domain",
      nodes: 4 as const,
      maximumRms: 0.01,
      maximumError: 0.01,
    };
    expect(integrateCoherentGGX(options).rejection).toBe("unvalidated-domain");
    expect(integrateCoherentGGX({ ...options, orbit: { ...orbit, b: [0.2, 1e-10] } }).rejection).toBe(
      "degenerate-orbit",
    );
    expect(integrateCoherentGGX({ ...options, lighting: { ...lighting, roughness: 0.18 } }).rejection).toBe(
      "broad-lobe",
    );
    expect(integrateCoherentGGX({ ...options, length: 0.2 }).rejection).toBe("no-complete-period");
    const unequal = integrateCoherentGGX({
      ...options,
      rates: [1, 1.01],
      regularResponse: (t) => Math.cos(t) * Math.cos(1.01 * t),
      regularNodes: 4096,
    });
    const L = options.length,
      s = options.start;
    const expected =
      ((Math.sin(0.01 * (s + L)) - Math.sin(0.01 * s)) / 0.01 +
        (Math.sin(2.01 * (s + L)) - Math.sin(2.01 * s)) / 2.01) /
      (2 * L);
    expect(unequal.rejection).toBe("different-carrier-rates");
    expect(unequal.value).toBeCloseTo(expected, 6);
    const selected = integrateCoherentGGX({
      ...options,
      evidence: {
        kind: "measured",
        sourceKey: "source",
        parameterKey: "domain",
        reference: "rotational-independent",
        nodes: 4,
        maximumCondition: 2,
        roughness: [0.06, 0.06],
        rms: 1e-10,
        maximum: 1e-9,
      },
    });
    expect(selected.selected).toBe("warped-4");
    expect(Number.isFinite(selected.value)).toBe(true);
  });
});

test("randomized warp expectation follows independently sampled moving highlights", () => {
  const orbit: PhaseOrbit = { mean: [0, 0], a: [0.095, 0.022], b: [0.008, 0.115] };
  for (const roughness of [0.06, 0.1, 0.18]) {
    for (const offset of [-0.15, 0, 0.2]) {
      const lighting: PhaseLighting = {
        view: [-0.05 + offset, 0.9, -0.12],
        light: [0.16, 0.9, 0.18 - offset],
        roughness,
        f0: 0.02,
      };
      const plan = prepareGGXWarp(orbit, lighting);
      if (!plan) throw new Error("Unexpected degenerate moving fixture");
      let expected = 0;
      for (let i = 0; i < 32768; i++) {
        const angle = (tau * (i + 0.5)) / 32768;
        expected +=
          reference(
            [
              orbit.a[0] * Math.cos(angle) + orbit.b[0] * Math.sin(angle),
              orbit.a[1] * Math.cos(angle) + orbit.b[1] * Math.sin(angle),
            ],
            lighting,
          ) / 32768;
      }
      for (const nodes of [4, 8] as const) {
        // Deterministic integration over uniform random shifts checks expectation, not any one low-node error claim.
        let average = 0;
        for (let shift = 0; shift < 256; shift++)
          average += evaluateGGXWarp(plan, nodes, (shift + 0.5) / 256) / 256;
        expect(Math.abs(average - expected)).toBeLessThan(1e-7 * Math.max(1, expected));
      }
    }
  }
});

test("warped finite tails retain a moving highlight at both endpoints", () => {
  const orbit: PhaseOrbit = { mean: [0, 0], a: [0.095, 0.022], b: [0.008, 0.115] };
  const lighting: PhaseLighting = {
    view: [-0.05, 0.9, -0.12],
    light: [0.16, 0.9, 0.18],
    roughness: 0.06,
    f0: 0.02037,
  };
  const plan = prepareGGXWarp(orbit, lighting);
  if (!plan) throw new Error("Unexpected degenerate interval fixture");
  for (const start of [-12.3, -3.1, -0.1, 0.2, 2.7, 5.8]) {
    for (const width of [0.01, 0.4, 1.7, 5.8, tau]) {
      let expected = 0;
      for (let i = 0; i < 32768; i++)
        expected += reference(phaseOrbitSlope(orbit, start + (width * (i + 0.5)) / 32768), lighting) / 32768;
      expect(Math.abs(evaluateGGXWarpInterval(plan, start, width, 1024) - expected)).toBeLessThan(
        2e-6 * Math.max(1, expected),
      );
    }
  }
});

test("small phase widths can still contain a resolved narrow GGX highlight", () => {
  const lighting: PhaseLighting = { view: [0, 1, 0], light: [0, 1, 0], roughness: 0.06, f0: 0.02037 };
  const width = 0.019;
  const center = reference([0, 0], lighting);
  let integrated = 0;
  for (let i = 0; i < 32768; i++)
    integrated +=
      reference([0.1 * Math.cos(Math.PI / 2 + width * ((i + 0.5) / 32768 - 0.5)), 0], lighting) / 32768;
  // Regression evidence for using authored slope variation relative to roughness²,
  // rather than a fixed phase-width threshold, in production direct selection.
  expect((center - integrated) / integrated).toBeGreaterThan(0.04);
  const slopeVariationBound = (0.1 * width) / 2;
  expect(slopeVariationBound).toBeGreaterThan(0.02 * lighting.roughness ** 2);
});

test("query-local resolved gates retain spatial highlights while resolving smooth off-highlight footprints", () => {
  let resolved = 0,
    rejected = 0;
  for (const roughness of [0.06, 0.18, 0.5])
    for (const cameraHeight of [0.15, 1])
      for (const offset of [0, 0.4, 0.9])
        for (const footprint of [0.002, 0.01, 0.03]) {
          const normalize = (v: [number, number, number]): [number, number, number] => {
            const length = Math.hypot(...v);
            return v.map((x) => x / length) as [number, number, number];
          };
          const phases = [1.2, 0.7],
            slopes: [number, number][] = [
              [0.1, 0.02],
              [-0.01, 0.12],
            ];
          const responseSlope = (u: number, v: number): [number, number] =>
            [0, 1].map(
              (axis) =>
                slopes[0][axis] * Math.cos(phases[0] + footprint * (u + 0.3 * v)) +
                slopes[1][axis] * Math.cos(phases[1] + footprint * (-0.4 * u + v)),
            ) as [number, number];
          const center = responseSlope(0, 0),
            n = normalize([-center[0], 1, -center[1]]);
          const view = normalize([0.4, cameraHeight, 0.2]);
          const half = normalize([n[0] + offset, n[1], n[2]]);
          const dot = view.reduce((sum, x, i) => sum + x * half[i], 0);
          const light = normalize(half.map((x, i) => 2 * dot * x - view[i]) as [number, number, number]);
          const lighting: PhaseLighting = { view, light, roughness, f0: 0.02037 };
          const cone =
            Math.hypot(...slopes[0]) * footprint * 0.65 + Math.hypot(...slopes[1]) * footprint * 0.7;
          const width = phaseGGXFeatureWidth(center, lighting, cone);
          if (cone > 0.05 * width) {
            rejected++;
            continue;
          }
          resolved++;
          const complete = (slope: [number, number]) => {
            const normal = normalize([-slope[0], 1, -slope[1]]);
            const nv = Math.max(
              0,
              normal.reduce((sum, x, i) => sum + x * view[i], 0),
            );
            // An independently evaluated smooth, nonuniform sky plus direct GGX.
            const reflectedY = -view[1] + 2 * nv * normal[1];
            return (
              reference(slope, lighting) + (0.2 + 0.1 * reflectedY) * (0.02037 + 0.97963 * (1 - nv) ** 5)
            );
          };
          let expected = 0;
          for (let y = 0; y < 64; y++)
            for (let x = 0; x < 64; x++)
              expected += complete(responseSlope((x + 0.5) / 64 - 0.5, (y + 0.5) / 64 - 0.5)) / 4096;
          expect(Math.abs(complete(center) - expected) / Math.max(expected, 0.01)).toBeLessThan(0.01);
        }
  expect(resolved).toBeGreaterThan(20);
  expect(rejected).toBeGreaterThan(5);
});
