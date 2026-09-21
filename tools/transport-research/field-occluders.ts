import type { Vec3 } from "@wrela/model";
import { type Expression, specialize } from "../field-research/local-program";
import type { Sphere } from "./visibility";

/** Bake an interior certificate from the complete ordered CSG, including cuts.
 * Testing a containing cube is deliberately conservative. World transforms may
 * preserve the sphere only under rigid motion/uniform scale; skinning needs a
 * separate pose-domain containment proof, never a transformed bind-pose guess. */
export function certifiedInnerSphere(
  expression: Expression,
  center: Vec3,
  maximumRadius: number,
): Sphere | null {
  if (!(maximumRadius > 0) || !Number.isFinite(maximumRadius)) return null;
  const inside = (radius: number) =>
    specialize(expression, {
      min: center.map((v) => v - radius) as Vec3,
      max: center.map((v) => v + radius) as Vec3,
    }).range.hi < -1e-9;
  if (!inside(0)) return null;
  let lo = 0,
    hi = maximumRadius;
  for (let i = 0; i < 32; i++) {
    const mid = (lo + hi) / 2;
    if (inside(mid)) lo = mid;
    else hi = mid;
  }
  return lo > 1e-8 ? { x: center[0], y: center[1], z: center[2], radius: lo } : null;
}
