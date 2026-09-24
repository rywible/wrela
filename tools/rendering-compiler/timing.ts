import type { GpuFrameTiming } from "@wrela/model";

const measuredPasses = [
  "gpuMs",
  "atmosphereMs",
  "shadowMs",
  "sceneMs",
  "waterMs",
  "temporalMs",
  "displayMs",
] as const;
export type AttributedSample = GpuFrameTiming & { trajectoryFrame: number };
export function quantiles(values: readonly number[]) {
  if (!values.length || values.some((value) => !Number.isFinite(value) || value < 0))
    throw Error("Timing samples must be nonempty finite durations");
  const sorted = [...values].sort((a, b) => a - b);
  const q = (fraction: number) =>
    sorted[Math.min(sorted.length - 1, Math.ceil(fraction * sorted.length) - 1)];
  return {
    samples: sorted.length,
    p50: q(0.5),
    p95: q(0.95),
    p99: q(0.99),
    minimum: sorted[0],
    maximum: sorted[sorted.length - 1],
  };
}
/** Rotate the initial condition and reverse direction to avoid always measuring a candidate after the reference. */
export function trialOrder<T>(variants: readonly T[], trial: number): T[] {
  if (!variants.length || !Number.isInteger(trial) || trial < 0) throw Error("Invalid alternating trial");
  const offset = trial % variants.length;
  const order = variants.map((_, index) => variants[(index + offset) % variants.length]);
  return Math.floor(trial / variants.length) % 2 ? order.reverse() : order;
}
/** Observed integer timestamp lattice, not a claim about device clock precision. */
export function observedTimingQuantumMs(samples: readonly GpuFrameTiming[]) {
  const gcd = (a: number, b: number): number => (b === 0 ? a : gcd(b, a % b));
  let lattice = 0;
  for (const sample of samples)
    for (const key of ["shadowMs", "sceneMs", "displayMs"] as const) {
      const ns = Math.round(sample[key] * 1e6);
      if (ns > 0) lattice = gcd(lattice, ns);
    }
  return lattice / 1e6;
}
export function summarizeMatchedTrial(
  variants: readonly { variant: string; gpu: readonly AttributedSample[] }[],
) {
  if (variants.length < 2) throw Error("A matched trial needs reference and candidate");
  const frames = variants.map(
    (variant) => new Map(variant.gpu.map((sample) => [sample.trajectoryFrame, sample])),
  );
  for (let i = 0; i < frames.length; i++)
    if (frames[i].size !== variants[i].gpu.length) throw Error("Duplicate trajectory-frame GPU timing");
  const shared = [...frames[0].keys()]
    .filter((frame) => frames.every((map) => map.has(frame)))
    .sort((a, b) => a - b);
  if (shared.length < 16) throw Error(`Only ${shared.length} matched trajectory frames have GPU timings`);
  return {
    trajectoryFrames: shared,
    variants: variants.map((variant, index) => {
      const samples = shared.map((frame) => {
        const sample = frames[index].get(frame);
        if (!sample) throw Error("Matched GPU sample disappeared");
        return sample;
      });
      return {
        variant: variant.variant,
        matchedSamples: samples.length,
        unmatchedSamples: variant.gpu.length - samples.length,
        observedQuantumMs: observedTimingQuantumMs(samples),
        passes: Object.fromEntries(
          measuredPasses.flatMap((key) => {
            const values = samples
              .map((sample) => sample[key])
              .filter((value): value is number => value !== undefined);
            return values.length === samples.length ? [[key, quantiles(values)]] : [];
          }),
        ),
      };
    }),
  };
}
export function summarizeAlternatingTrials(trials: ReturnType<typeof summarizeMatchedTrial>[]) {
  if (trials.length < 3) throw Error("At least three alternating trials are needed");
  const names = trials[0].variants.map((variant) => variant.variant);
  return names.map((name) => {
    const observations = trials.map((trial) => {
      const sample = trial.variants.find((variant) => variant.variant === name);
      if (!sample) throw Error(`Missing trial condition ${name}`);
      return sample;
    });
    return {
      variant: name,
      trials: trials.length,
      matchedGpuSamples: observations.reduce((n, sample) => n + sample.matchedSamples, 0),
      observedQuantumMs: Math.max(...observations.map((sample) => sample.observedQuantumMs)),
      passes: Object.fromEntries(
        measuredPasses.flatMap((key) => {
          const pass = observations.map((sample) => sample.passes[key]);
          return pass.every(Boolean)
            ? [
                [
                  key,
                  {
                    trialP50: quantiles(pass.map((sample) => sample.p50)),
                    trialP95: quantiles(pass.map((sample) => sample.p95)),
                  },
                ],
              ]
            : [];
        }),
      ),
    };
  });
}
