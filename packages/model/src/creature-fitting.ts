import type { Vec3 } from "./math";
import { cross, dot, sub } from "./math";

/** The four bilinear patch Jacobians must point into one common hemisphere.
 * This rejects folds and collapsed edges even when both render triangles have
 * nonzero area. A previous normal additionally protects chart orientation. */
export function creaturePatchHasConsistentOrientation(points: readonly Vec3[], previous?: readonly Vec3[]) {
  const [a, b, c, d] = points;
  if (!a || !b || !c || !d) return false;
  const normals = [
    cross(sub(b, a), sub(c, a)),
    cross(sub(b, a), sub(d, b)),
    cross(sub(d, c), sub(c, a)),
    cross(sub(d, c), sub(d, b)),
  ];
  const reference = previous
    ? cross(sub(previous[1], previous[0]), sub(previous[2], previous[0]))
    : normals[0];
  return normals.every((normal) => Math.hypot(...normal) >= 1e-8 && dot(normal, reference) > 0);
}
