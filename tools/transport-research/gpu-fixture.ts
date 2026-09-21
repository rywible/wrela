import { atmosphereExperiment } from "./gpu-atmosphere";
import { gpuContext } from "./gpu-common";
import { spectralExperiment } from "./gpu-spectral";
import { visibilityExperiment } from "./gpu-visibility";
import { tau } from "./spectral";

export async function createExperiment() {
  const ctx = await gpuContext();
  return {
    adapter: ctx.adapter,
    errors: ctx.errors,
    spectral: (name: string) => {
      if (name === "distant")
        return spectralExperiment(ctx, name, { dx: [13.1, 10.7], dy: [-3.6, 5.9], shutter: [0, 0] }, 16);
      if (name === "oblique")
        return spectralExperiment(ctx, name, { dx: [3.1, 2.7], dy: [-0.6, 0.9], shutter: [0, 0] }, 128);
      if (name === "long-correlated")
        return spectralExperiment(
          ctx,
          name,
          { dx: [8 * tau, 8 * tau], dy: [-0.12, 0.12], shutter: [0, 0] },
          16,
        );
      throw new Error(`Unknown spectral fixture ${name}`);
    },
    atmosphere: (scale: number) => atmosphereExperiment(ctx, scale),
    visibility: (fraction: number) => visibilityExperiment(ctx, fraction),
  };
}
