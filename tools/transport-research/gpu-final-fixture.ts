import { coherentExperiment } from "./gpu-coherent";
import { gpuContext } from "./gpu-common";
import { sunExperiment } from "./gpu-sun";

export async function createExperiment() {
  const ctx = await gpuContext();
  return {
    adapter: ctx.adapter,
    errors: ctx.errors,
    coherent: (roughness: number) => coherentExperiment(ctx, roughness),
    sun: () => sunExperiment(ctx),
  };
}
