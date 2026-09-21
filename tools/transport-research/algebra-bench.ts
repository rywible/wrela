import { compilePositive, orbitIntegral, positiveIntegral, residualSample } from "./positive-response";
import {
  compileSpectrum,
  evaluatePlan,
  type Footprint,
  integrateResponse,
  materialResponse,
  planSpectrum,
  tau,
  transfer,
  waterResponse,
} from "./spectral";

const stats = (v: number[]) => {
  const mean = v.reduce((a, b) => a + b, 0) / v.length;
  const variance = v.reduce((a, b) => a + (b - mean) ** 2, 0) / v.length;
  return { rms: Math.sqrt(variance + mean * mean), mean, variance, max: Math.max(...v.map(Math.abs)) };
};
const footprint: Footprint = { dx: [13.1, 10.7], dy: [-3.6, 5.9], shutter: [0, 0] };
const materialStart = performance.now(),
  material = compileSpectrum(materialResponse, 512),
  materialCompileMs = performance.now() - materialStart;
const plans = [16, 32, 64, 128].map((count) => planSpectrum(material, footprint, count));
const errors = plans.map(() => [] as number[]),
  point: number[] = [],
  filteredInputs: number[] = [],
  samples64: number[] = [],
  samples256: number[] = [],
  convergence: number[] = [];
let totalReference = 0;
for (let i = 0; i < 64; i++) {
  const a = (i * 2.3999632297 + 0.13) % tau,
    b = (i * 4.118231024 + 0.31) % tau;
  const truth = integrateResponse(materialResponse, a, b, footprint, 512);
  totalReference += truth;
  convergence.push(truth - integrateResponse(materialResponse, a, b, footprint, 256));
  point.push(materialResponse(a, b) - truth);
  filteredInputs.push(materialResponse(a, b, footprint) - truth);
  samples64.push(integrateResponse(materialResponse, a, b, footprint, 8) - truth);
  samples256.push(integrateResponse(materialResponse, a, b, footprint, 16) - truth);
  plans.forEach((plan, j) => {
    errors[j].push(evaluatePlan(plan, a, b) - truth);
  });
}
const materialReport = {
  compileMs: materialCompileMs,
  size: material.size,
  referenceMean: totalReference / 64,
  convergence: stats(convergence),
  point: stats(point),
  filteredInputs: stats(filteredInputs),
  samples64: stats(samples64),
  samples256: stats(samples256),
  plans: plans.map((plan, i) => ({
    retainedModes: plan.modes.length,
    omittedL1: plan.omittedL1,
    error: stats(errors[i]),
  })),
};
console.log(JSON.stringify({ material: materialReport }));
const positive = compilePositive(waterResponse, 64),
  positiveErrors: number[] = [],
  amplitudeErrors: number[] = [],
  orbitErrors: number[] = [];
let minIntegral = Infinity;
const orbit: Footprint = { dx: [8 * tau, 8 * tau], dy: [0, 0], shutter: [0, 0] };
for (let i = 0; i < 100; i++) {
  const a = i * 2.39996323,
    b = i * 4.11823,
    filtered = positiveIntegral(positive.signed, a, b, footprint);
  minIntegral = Math.min(minIntegral, filtered);
  amplitudeErrors.push(evaluatePlan(positive.amplitude, a, b) - Math.sqrt(waterResponse(a, b)));
  positiveErrors.push(filtered - integrateResponse(waterResponse, a, b, footprint, 256));
  orbitErrors.push(
    orbitIntegral(positive.signed, a, b, [1, 1]) - positiveIntegral(positive.signed, a, b, orbit),
  );
}
// These independent random queries measure estimator variance, not GPU throughput.
const spectrum = compileSpectrum(waterResponse),
  predictor = planSpectrum(spectrum, { dx: [0, 0], dy: [0, 0], shutter: [0, 0] }, 128);
// Derive the predictor integral from precisely the same unfiltered modes.
const filtered = {
  ...predictor,
  modes: predictor.modes.map((m) => ({
    ...m,
    re: m.re * transfer(m.m, m.n, footprint),
    im: m.im * transfer(m.m, m.n, footprint),
  })),
};
let seed = 1983147;
const random = () => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return seed / 4294967296;
};
const naive: number[] = [],
  residual: number[] = [],
  a = 0.317,
  b = 1.223;
for (let i = 0; i < 65536; i++) {
  const u = random() - 0.5,
    v = random() - 0.5;
  naive.push(
    waterResponse(
      a + footprint.dx[0] * u + footprint.dy[0] * v,
      b + footprint.dx[1] * u + footprint.dy[1] * v,
    ),
  );
  residual.push(residualSample(waterResponse, predictor, a, b, footprint, u, v));
}
const truth = integrateResponse(waterResponse, a, b, footprint, 1024),
  n = stats(naive),
  r = stats(residual);
const report = {
  created: new Date().toISOString(),
  material: materialReport,
  positive: {
    amplitudeModes: 129,
    measuredAmplitudeError: stats(amplitudeErrors),
    amplitudeOmittedL1: positive.amplitude.omittedL1,
    filteredError: stats(positiveErrors),
    minIntegral,
    orbitParityMax: stats(orbitErrors).max,
  },
  residual: {
    samples: naive.length,
    reference: truth,
    naiveMean: n.mean,
    correctedMean: evaluatePlan(filtered, a, b) + r.mean,
    naiveVariance: n.variance,
    residualVariance: r.variance,
    varianceRatio: n.variance / r.variance,
    standardError: Math.sqrt(r.variance / residual.length),
    note: "Unbiased control-variate identity, not a GPU speedup claim. Predictor evaluation cost is included in any future renderer comparison.",
  },
};
await Bun.write("output/transport-research/algebra.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify({ positive: report.positive, residual: report.residual }));
