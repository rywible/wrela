import { mkdir } from "node:fs/promises";
import { compileWaves, type Footprint, sampledMoments, waveMoments } from "./wave-moments";

const waves = compileWaves(
  Array.from({ length: 8 }, (_, i) => ({
    amplitude: 0.015 + i * 0.007,
    wavelength: 0.8 + i * 0.43,
    speed: 0.2 + i * 0.17,
    direction: i * 1.31,
    phase: i * 0.29,
  })),
);
const footprint: Footprint = { dx: [1.7, 0.3], dy: [0.2, 1.1], shutter: 0.03 };
let checksum = 0;
for (let i = 0; i < 2000; i++) checksum += waveMoments(waves, i * 0.17, -1.3, 0.4, footprint).mean[0];
sampledMoments(waves, 0, -1.3, 0.4, footprint, 128, 8);
const analyticTimes: number[] = [],
  quadratureTimes: number[] = [],
  errors: number[] = [];
for (let repeat = 0; repeat < 9; repeat++) {
  let start = performance.now();
  for (let i = 0; i < 2000; i++) checksum += waveMoments(waves, i * 0.17, -1.3, 0.4, footprint).mean[0];
  analyticTimes.push((performance.now() - start) / 2000);
  start = performance.now();
  const numerical = sampledMoments(waves, repeat * 0.17, -1.3, 0.4, footprint, 128, 8);
  quadratureTimes.push(performance.now() - start);
  const exact = waveMoments(waves, repeat * 0.17, -1.3, 0.4, footprint);
  errors.push(
    Math.max(
      ...exact.mean.map((v, i) => Math.abs(v - numerical.mean[i])),
      ...exact.covariance.map((v, i) => Math.abs(v - numerical.covariance[i])),
    ),
  );
}
const median = (a: number[]) => [...a].sort((a, b) => a - b)[Math.floor(a.length / 2)];
const report = {
  created: new Date().toISOString(),
  waves: 8,
  quadratureSamplesPerFootprint: 128 * 128 * 8,
  analyticMsPerFootprint: median(analyticTimes),
  quadratureMsPerFootprint: median(quadratureTimes),
  speedupVsNumericalIntegration: median(quadratureTimes) / median(analyticTimes),
  maxMomentDifference: Math.max(...errors),
  analyticTimes,
  quadratureTimes,
  checksum,
  scope:
    "CPU cost of first and second slope moments only, compared with 131072-sample midpoint quadrature. This is an integration reference, NOT a production rendering baseline or measured image-quality improvement.",
};
await mkdir("output/field-research", { recursive: true });
await Bun.write("output/field-research/waves.json", JSON.stringify(report, null, 2));
console.log(JSON.stringify(report));
