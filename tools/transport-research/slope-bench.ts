import {
  compileSlopeAtlas,
  ggxCharacteristic,
  periodizationBound,
  sampledSlopeDensity,
  sampleSlopeAtlas,
} from "./slope-atlas";
import { type Pair, water } from "./spectral";

const slopes = water.slopes as Pair[],
  alpha = water.roughness ** 2;
const start = performance.now(),
  atlas = compileSlopeAtlas(slopes, alpha),
  compileMs = performance.now() - start;
let error2 = 0,
  maxError = 0,
  reference2 = 0,
  referenceConvergence = 0;
const queries: { sx: number; sy: number; reference: number; estimate: number }[] = [];
for (let i = 0; i < 256; i++) {
  const sx = (((i % 16) / 15) * 2 - 1) * 0.25 + 0.00013,
    sy = ((Math.floor(i / 16) / 15) * 2 - 1) * 0.25 + 0.00017;
  const reference = sampledSlopeDensity(slopes, alpha, sx, sy, 256),
    estimate = sampleSlopeAtlas(atlas, sx, sy),
    error = estimate - reference;
  error2 += error * error;
  reference2 += reference * reference;
  maxError = Math.max(maxError, Math.abs(error));
  if (i % 16 === 0)
    referenceConvergence = Math.max(
      referenceConvergence,
      Math.abs(reference - sampledSlopeDensity(slopes, alpha, sx, sy, 512)),
    );
  queries.push({ sx, sy, reference, estimate });
}
const measured = ggxCharacteristic(1),
  known = 0.6019072301972346;
const result = {
  created: new Date().toISOString(),
  compileMs,
  size: atlas.size,
  period: atlas.period,
  alpha,
  normalization: atlas.values.reduce((a, b) => a + b, 0) * (atlas.period / atlas.size) ** 2,
  minimumDensity: Math.min(...atlas.values),
  rmsError: Math.sqrt(error2 / queries.length),
  relativeL2: Math.sqrt(error2 / reference2),
  maxError,
  referenceConvergence,
  periodicImageUpperBound: periodizationBound(
    alpha,
    atlas.period,
    Math.SQRT2 * 0.251 + slopes.reduce((s, v) => s + Math.hypot(...v), 0),
  ),
  specialFunctionCheck: {
    q: 1,
    result: measured,
    independentKnownK1: known,
    error: Math.abs(measured - known),
  },
  queries,
  note: "View-independent additive-slope NDF under complete phase decorrelation. Not a full filtered BRDF or a finite-pixel correlation solution. Periodic-image bound excludes interpolation and frequency truncation errors.",
};
await Bun.write("output/transport-research/slope.json", JSON.stringify(result, null, 2));
console.log(JSON.stringify({ ...result, queries: result.queries.length }));
