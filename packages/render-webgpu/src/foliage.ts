import type { Vec3 } from "@wrela/model";

/** Empirical thin-leaf diffuse closure, not fitted PROSPECT optical coefficients. */
export type FoliageOptics = { albedo: Vec3; scatterColor: Vec3; transmission: number; thickness: number };
export type FoliageBudget = { reflection: Vec3; transmission: Vec3 };
const unit = (value: number) => Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
export const FOLIAGE_ATTENUATION_METRES = 0.005;
export const FOLIAGE_DIFFUSE_BUDGET = 0.96;
/** Reserve 4% for a normal-incidence dielectric interface. The separate GGX specular
 * lobe is not part of the certified diffuse R+T budget, especially at grazing angles. */
export function foliageBudget(optics: FoliageOptics): FoliageBudget {
  const thickness = Number.isFinite(optics.thickness) ? Math.max(0, optics.thickness) : 0;
  const fraction = unit(optics.transmission) * Math.exp(-thickness / FOLIAGE_ATTENUATION_METRES);
  return {
    reflection: optics.albedo.map((value) => unit(value) * FOLIAGE_DIFFUSE_BUDGET * (1 - fraction)) as Vec3,
    transmission: optics.albedo.map(
      (value, axis) => unit(value) * FOLIAGE_DIFFUSE_BUDGET * unit(optics.scatterColor[axis]) * fraction,
    ) as Vec3,
  };
}
/** Camera-facing normal convention matches the two-sided production shader. */
export function foliageDiffuse(budget: FoliageBudget, lightCosine: number, visibility: number): Vec3 {
  const cosine = Math.max(-1, Math.min(1, lightCosine));
  const front = Math.max(cosine, 0) * unit(visibility);
  const back = Math.max(-cosine, 0) * unit(visibility);
  return budget.reflection.map(
    (value, axis) => (value * front + budget.transmission[axis] * back) / Math.PI,
  ) as Vec3;
}
