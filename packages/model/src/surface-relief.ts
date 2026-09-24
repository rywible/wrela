import { z } from "zod";
import type { Vec3 } from "./math";

/** Physical authoring intent, independent of the near/far realization selected at runtime. */
export const surfaceReliefSchema = z.object({
  kind: z.enum(["bark", "stone"]),
  amplitude: z.number().finite().min(0).max(0.08),
  /** Typical groove/chip spacing in local metres. */
  scale: z.number().finite().min(0.005).max(2),
  seed: z.number().int().min(-2147483648).max(2147483647),
  /** Desired near-mesh edge length; the compiler reports when its budget cannot achieve it. */
  targetEdgeLength: z.number().finite().min(0.002).max(0.5),
  /** Local wood grain direction. Stone does not require an oriented material frame. */
  direction: z
    .tuple([z.number().finite(), z.number().finite(), z.number().finite()])
    .refine(
      (value) => Number.isFinite(Math.hypot(...value)) && Math.hypot(...value) > 1e-6,
      "Relief grain direction must have a finite nonzero length",
    )
    .default([0, 1, 0]),
});
export type SurfaceRelief = z.infer<typeof surfaceReliefSchema>;

/** Frequency allocation for one realized mesh. Coarse meshes use zero geometry weights. */
export type SurfaceReliefAppearance = {
  recipe: SurfaceRelief;
  geometryWeights: Vec3;
  residualWeights: Vec3;
  /** Nominal squared tangent slopes from deterministic pattern sampling, not a radiance bound. */
  slopeVariance: Vec3;
};
