import { dot, normalize, type SurfaceRelief, type Vec3 } from "@wrela/model";

/** Spend bark triangles across grain before subdividing its slowly varying length. */
export function surfaceReliefEdgePriority(edge: Vec3, source: SurfaceRelief): number {
  const length = Math.hypot(...edge);
  if (source.kind === "stone") return length;
  const along = Math.abs(dot(edge, normalize(source.direction)));
  const across = Math.sqrt(Math.max(0, length * length - along * along));
  return Math.max(across * 3, along * 0.4);
}

export function surfaceReliefPatternSpacing(edge: Vec3, source: SurfaceRelief): number {
  const length = Math.hypot(...edge);
  if (source.kind === "stone") return length;
  const along = Math.abs(dot(edge, normalize(source.direction)));
  const across = Math.sqrt(Math.max(0, length * length - along * along));
  // Includes slow axial warping as well as the transverse rib carrier.
  return across + along * 0.1;
}

/** Conservative reconstruction taper: below five samples/cycle progressively move energy to shading. */
export function surfaceReliefGeometryWeights(patternSpacing: number, source: SurfaceRelief): Vec3 {
  const frequencies: Vec3 = source.kind === "stone" ? [1, 2.7, 7.1] : [1, 3, 8];
  return frequencies.map((frequency) => {
    const cycles = (patternSpacing * frequency) / source.scale;
    const t = Math.max(0, Math.min(1, (cycles - 0.2) / 0.45));
    return 1 - t * t * (3 - 2 * t);
  }) as Vec3;
}
