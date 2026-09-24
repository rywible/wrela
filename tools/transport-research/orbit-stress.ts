import {
  directResponse,
  integrateOrbit,
  type Lighting,
  normalized,
  orbitSample,
  prepareOrbit,
  refineOrbit,
  seededRandom,
  slopeAt,
  type WaveOrbit,
  warpedOrbit,
} from "./coherent-ggx";
import { tau } from "./spectral";

const random = seededRandom(993441),
  reports = [];
for (const domain of ["moderate", "thin-orbit", "grazing"]) {
  const errors: Record<string, number[]> = { regular64: [], regular256: [], warp4: [], warp8: [], pole4: [] };
  let convergence = 0,
    worstCondition = 0,
    minimumResponse = Infinity,
    maximumResponse = 0;
  for (let i = 0; i < 128; i++) {
    const orbit: WaveOrbit = {
      mean: [(random() - 0.5) * 0.2, (random() - 0.5) * 0.2],
      a: [0.04 + 0.18 * random(), (random() - 0.5) * 0.06],
      b: [(random() - 0.5) * 0.06, 0.04 + 0.18 * random()],
    };
    if (domain === "thin-orbit") orbit.b = [orbit.a[0] * 0.7, orbit.a[1] * 0.7 + 1e-5];
    if (domain === "grazing") {
      orbit.a = [1.2, 0.4];
      orbit.b = [0.2, 1];
    }
    const height = domain === "grazing" ? 0.05 : 0.85;
    const lighting: Lighting = {
      view: normalized([(random() - 0.5) * 1.4, height, (random() - 0.5) * 1.4]),
      light: normalized([(random() - 0.5) * 1.4, height, (random() - 0.5) * 1.4]),
      roughness: 0.06,
      f0: 0.02037,
    };
    const initial = prepareOrbit(orbit, lighting);
    if (!initial) throw new Error("Stress fixture should be nonsingular");
    const plan = refineOrbit(initial),
      truth = integrateOrbit(orbit, lighting, 32768);
    if (i % 7 === 0)
      convergence = Math.max(convergence, Math.abs(truth - integrateOrbit(orbit, lighting, 65536)));
    worstCondition = Math.max(worstCondition, plan.condition);
    minimumResponse = Math.min(minimumResponse, truth);
    maximumResponse = Math.max(maximumResponse, truth);
    for (const mode of Object.keys(errors)) {
      const count = Number(mode.replace(/\D/g, ""));
      let value = 0;
      if (mode.startsWith("warp")) value = warpedOrbit(plan, count, random());
      else
        for (let j = 0; j < count; j++) {
          const theta = (tau * (j + 0.5)) / count;
          value +=
            (mode.startsWith("pole")
              ? orbitSample(plan, random).value
              : directResponse(slopeAt(orbit, Math.cos(theta), Math.sin(theta)), lighting)) / count;
        }
      errors[mode].push(value - truth);
    }
  }
  const report = {
    domain,
    queries: 128,
    convergence,
    worstCondition,
    minimumResponse,
    maximumResponse,
    errors: Object.fromEntries(
      Object.entries(errors).map(([name, values]) => [
        name,
        {
          rms: Math.sqrt(values.reduce((s, x) => s + x * x, 0) / values.length),
          max: Math.max(...values.map(Math.abs)),
        },
      ]),
    ),
  };
  reports.push(report);
  console.log(JSON.stringify(report));
}
await Bun.write("output/transport-research/orbit-stress.json", JSON.stringify({ reports }, null, 2));
