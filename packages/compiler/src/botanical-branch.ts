import { add, random01, scale, sub, type Vec3 } from "@wrela/model";

export type BotanicalBranch = {
  id: string;
  parent: string | null;
  level: number;
  habit?: "long" | "short";
  start: Vec3;
  bend: Vec3;
  end: Vec3;
  radius: number;
  tipRadius?: number;
  cohort?: { age: number; retention: number; count: number; tint: number };
  bare: boolean;
  broken: boolean;
  points?: Vec3[];
};
/** Shared attachment curve: two straight stem segments, matching the rendered tubes exactly. */
export function botanicalBranchPoint(branch: BotanicalBranch, t: number): Vec3 {
  if (branch.points && branch.points.length > 1) {
    const sample = Math.max(0, Math.min(1, t)) * (branch.points.length - 1);
    const index = Math.min(branch.points.length - 2, Math.floor(sample));
    return add(
      branch.points[index],
      scale(sub(branch.points[index + 1], branch.points[index]), sample - index),
    );
  }
  const midpoint = add(add(scale(branch.start, 0.25), scale(branch.bend, 0.5)), scale(branch.end, 0.25));
  return t <= 0.5
    ? add(branch.start, scale(sub(midpoint, branch.start), t * 2))
    : add(midpoint, scale(sub(branch.end, midpoint), (t - 0.5) * 2));
}
export const MAX_BOTANICAL_BRANCHES = 768;

/** Stable keyed randomness means pruning one limb never reshuffles its siblings. */
export function botanicalRandom(seed: number, key: string): number {
  let hash = seed | 0;
  for (let i = 0; i < key.length; i++) hash = Math.imul(hash ^ key.charCodeAt(i), 16777619);
  return random01(hash);
}
