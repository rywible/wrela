import { mkdir } from "node:fs/promises";
import { boundedOpticalDepth, fixedOpticalDepth, quadratureOpticalDepth, topDistance } from "./atmosphere";
import {
  compileSpectrum,
  evaluatePlan,
  type Footprint,
  integrateResponse,
  planSpectrum,
  tau,
  transfer,
  waterResponse,
} from "./spectral";
import { compileOccluders, crowdFixture, hidden } from "./visibility";

await mkdir("output/transport-research", { recursive: true });
const stats = (values: number[]) => ({
  rms: Math.sqrt(values.reduce((s, x) => s + x * x, 0) / values.length),
  max: Math.max(...values.map(Math.abs)),
});
const compileStart = performance.now(),
  spectrum = compileSpectrum(waterResponse, 256),
  compileMs = performance.now() - compileStart;
const cases: { name: string; footprint: Footprint }[] = [
  { name: "resolved", footprint: { dx: [0.15, 0.08], dy: [-0.04, 0.19], shutter: [0, 0] } },
  { name: "oblique", footprint: { dx: [3.1, 2.7], dy: [-0.6, 0.9], shutter: [0, 0] } },
  { name: "distant", footprint: { dx: [13.1, 10.7], dy: [-3.6, 5.9], shutter: [0, 0] } },
  { name: "long-correlated", footprint: { dx: [8 * tau, 8 * tau], dy: [-0.12, 0.12], shutter: [0, 0] } },
];
const fit = { errors: [] as number[], peak: 0 };
const noFilter: Footprint = { dx: [0, 0], dy: [0, 0], shutter: [0, 0] };
const full = planSpectrum(spectrum, noFilter, Infinity);
for (let i = 0; i < 1000; i++) {
  const a = (i * 2.3999632297 + 0.017) % tau,
    b = (i * 4.118231024 + 0.031) % tau;
  const actual = waterResponse(a, b);
  fit.errors.push(evaluatePlan(full, a, b) - actual);
  fit.peak = Math.max(fit.peak, actual);
}
const phaseCases = cases.map(({ name, footprint }) => {
  const plans = [8, 16, 32, 64, 128, 256].map((k) => planSpectrum(spectrum, footprint, k));
  const errors = plans.map(() => [] as number[]),
    point: number[] = [],
    mean: number[] = [],
    sample8: number[] = [],
    sample16: number[] = [],
    referenceConvergence: number[] = [];
  const truth: number[] = [];
  const attenuation: [number, number] = [transfer(1, 0, footprint), transfer(0, 1, footprint)];
  for (let i = 0; i < 64; i++) {
    const a = (i * 2.3999632297 + 0.13) % tau,
      b = (i * 4.118231024 + 0.31) % tau;
    // An integer number of equal full periods has the same integral as one period.
    // Reduce only the independent reference's integration domain to avoid aliasing it.
    const referenceFootprint: Footprint =
      name === "long-correlated" ? { ...footprint, dx: [tau, tau] } : footprint;
    const reference = integrateResponse(waterResponse, a, b, referenceFootprint, 512);
    truth.push(reference);
    referenceConvergence.push(integrateResponse(waterResponse, a, b, referenceFootprint, 256) - reference);
    point.push(waterResponse(a, b) - reference);
    mean.push(waterResponse(a, b, attenuation) - reference);
    sample8.push(integrateResponse(waterResponse, a, b, footprint, 8) - reference);
    sample16.push(integrateResponse(waterResponse, a, b, footprint, 16) - reference);
    plans.forEach((plan, k) => {
      errors[k].push(evaluatePlan(plan, a, b) - reference);
    });
  }
  const result = {
    name,
    footprint,
    referenceMean: truth.reduce((a, b) => a + b, 0) / truth.length,
    referenceConvergence: stats(referenceConvergence),
    point: stats(point),
    meanNormal: stats(mean),
    samples64: stats(sample8),
    samples256: stats(sample16),
    plans: plans.map((plan, i) => ({
      count: plan.modes.length,
      omittedL1: plan.omittedL1,
      error: stats(errors[i]),
      maxFrequency: Math.max(...plan.modes.flatMap((m) => [Math.abs(m.m), Math.abs(m.n)])),
    })),
  };
  console.log(JSON.stringify(result));
  return result;
});
const atmosphere: Record<string, unknown>[] = [];
for (const height of [0, 1, 10, 40])
  for (const cosine of [0, 0.001, 0.01, 0.1, 0.5, 1])
    for (const scale of [1.2, 8]) {
      const ray = { height, cosine, scale, length: topDistance(height, cosine) },
        extinction = scale === 8 ? 0.012 : 0.04;
      const reference = quadratureOpticalDepth(ray, 32768),
        independent = quadratureOpticalDepth(ray, 16384);
      const bounded = boundedOpticalDepth(ray, extinction, 1e-5);
      atmosphere.push({
        ray,
        extinction,
        reference,
        convergence: Math.abs(reference - independent),
        segments: bounded.segments.length,
        lower: bounded.lower,
        upper: bounded.upper,
        transmissionWidth: bounded.transmissionUpper - bounded.transmissionLower,
        fixed: [4, 8, 16, 32].map((n) => {
          const f = fixedOpticalDepth(ray, n);
          return {
            n,
            error: Math.abs(Math.exp(-extinction * f.estimate) - Math.exp(-extinction * reference)),
            width: Math.exp(-extinction * f.lower) - Math.exp(-extinction * f.upper),
          };
        }),
      });
    }
const crowd = crowdFixture(),
  start = performance.now(),
  depth = compileOccluders(crowd.occluders, crowd.grid);
const visibilityMs = performance.now() - start,
  cullStart = performance.now();
const culled = crowd.candidates.filter((s) => hidden(s, crowd.grid, depth)).length,
  cullMs = performance.now() - cullStart;
const result = {
  created: new Date().toISOString(),
  spectral: {
    size: spectrum.size,
    compileMs,
    dc: spectrum.dc,
    discardedNyquistL1: spectrum.discardedNyquistL1,
    pointFit: stats(fit.errors),
    peak: fit.peak,
    cases: phaseCases,
  },
  atmosphere,
  crowd: {
    occluders: crowd.occluders.length,
    candidates: crowd.candidates.length,
    culled,
    survivorFraction: 1 - culled / crowd.candidates.length,
    visibilityMs,
    cullMs,
  },
};
await Bun.write("output/transport-research/cpu.json", JSON.stringify(result, null, 2));
console.log(
  JSON.stringify({
    compileMs,
    fit: stats(fit.errors),
    crowd: result.crowd,
    atmosphereCases: atmosphere.length,
  }),
);
