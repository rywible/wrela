import { mkdir } from "node:fs/promises";
import {
  directResponse,
  integrateOrbit,
  orbitFixture,
  orbitSample,
  prepareOrbit,
  refineOrbit,
  seededRandom,
  slopeAt,
  warpedOrbit,
} from "./coherent-ggx";
import { tau } from "./spectral";

await mkdir("output/transport-research", { recursive: true });
const reports = [];
for (const roughness of [0.04, 0.06, 0.1, 0.18]) {
  const modes = [
    "regular8",
    "regular64",
    "regular256",
    "stochastic8",
    "balanced8",
    "pole1",
    "pole4",
    "pole8",
    "warp2",
    "warp4",
    "warp8",
    "fixed2",
    "fixed4",
    "fixed8",
  ];
  const errors: Record<string, number[]> = Object.fromEntries(modes.map((m) => [m, []]));
  let reference2 = 0,
    referenceMean = 0,
    peak = 0,
    convergence = 0,
    refined = 0,
    fallback = 0,
    attempts = 0,
    samples = 0,
    negative = 0;
  for (let y = 0; y < 16; y++)
    for (let x = 0; x < 16; x++) {
      const index = y * 16 + x,
        { orbit, lighting } = orbitFixture((x + 0.37) / 16, (y + 0.61) / 16, roughness);
      const plan = prepareOrbit(orbit, lighting);
      if (!plan) throw new Error("Nonsingular orbit expected");
      const pole = refineOrbit(plan),
        truth = integrateOrbit(orbit, lighting, 16384);
      if (index % 17 === 0)
        convergence = Math.max(convergence, Math.abs(truth - integrateOrbit(orbit, lighting, 32768)));
      refined += Number(pole.refined);
      reference2 += truth * truth;
      referenceMean += truth;
      peak = Math.max(peak, truth);
      for (const mode of modes) {
        const count = Number(mode.replace(/\D/g, "")),
          random = seededRandom(index * 391 + 137);
        if (mode.startsWith("warp") || mode.startsWith("fixed")) {
          errors[mode].push(warpedOrbit(pole, count, mode.startsWith("warp") ? random() : 0.5) - truth);
          continue;
        }
        let sum = 0;
        for (let i = 0; i < count; i++) {
          if (mode.startsWith("regular") || mode.startsWith("stochastic")) {
            const theta = (tau * (i + (mode.startsWith("regular") ? 0.5 : random()))) / count;
            sum += directResponse(slopeAt(orbit, Math.cos(theta), Math.sin(theta)), lighting);
          } else {
            const sample = orbitSample(mode.startsWith("pole") ? pole : plan, random, 4);
            sum += sample.value;
            if (mode === "pole8") {
              fallback += Number(sample.fallback);
              attempts += sample.attempts;
              samples++;
              negative += Number(sample.value < 0);
            }
          }
        }
        errors[mode].push(sum / count - truth);
      }
    }
  const result = {
    roughness,
    queries: 256,
    refined,
    referenceMean: referenceMean / 256,
    referencePeak: peak,
    referenceConvergence: convergence,
    fallbackRate: fallback / samples,
    attemptsPerSample: attempts / samples,
    negativeSamples: negative,
    errors: Object.fromEntries(
      Object.entries(errors).map(([mode, values]) => [
        mode,
        {
          rms: Math.sqrt(values.reduce((s, x) => s + x * x, 0) / values.length),
          relativeL2: Math.sqrt(values.reduce((s, x) => s + x * x, 0) / reference2),
          bias: values.reduce((s, x) => s + x, 0) / values.length,
          max: Math.max(...values.map(Math.abs)),
        },
      ]),
    ),
  };
  reports.push(result);
  console.log(JSON.stringify(result));
}
await Bun.write(
  "output/transport-research/coherent-cpu.json",
  JSON.stringify({ created: new Date().toISOString(), reports }, null, 2),
);
