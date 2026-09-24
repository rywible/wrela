import type { Vec3 } from "./math";

export type CreaturePatchSource = {
  points: readonly [Vec3, Vec3, Vec3, Vec3];
  /** Region-local offsets from the corner-driven bilinear surface, indexed [v][u]. */
  controlOffsets?: readonly (readonly Vec3[])[];
};
const mix = (a: Vec3, b: Vec3, t: number): Vec3 =>
  a.map((value, axis) => value + (b[axis] - value) * t) as Vec3;
function cubic(a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: number): Vec3 {
  return b.map((value, axis) => {
    const m0 = (c[axis] - a[axis]) * 0.5,
      m1 = (d[axis] - value) * 0.5;
    return (
      (2 * t ** 3 - 3 * t * t + 1) * value +
      (t ** 3 - 2 * t * t + t) * m0 +
      (-2 * t ** 3 + 3 * t * t) * c[axis] +
      (t ** 3 - t * t) * m1
    );
  }) as Vec3;
}
function rowSample(row: readonly Vec3[], u: number): Vec3 {
  const scaled = u * (row.length - 1),
    index = Math.min(row.length - 2, Math.floor(scaled)),
    t = scaled - index;
  return cubic(
    row[Math.max(0, index - 1)],
    row[index],
    row[index + 1],
    row[Math.min(row.length - 1, index + 2)],
    t,
  );
}
/** Smooth offset lofts preserve exact fitted corners and a single UV domain.
 * Geometry compilation, physical cloth and anchors use this same source query. */
export function creaturePatchPoint(patch: CreaturePatchSource, u: number, v: number): Vec3 {
  const [a, b, c, d] = patch.points,
    base = mix(mix(a, b, u), mix(c, d, u), v),
    grid = patch.controlOffsets;
  if (!grid) return base;
  const scaled = v * (grid.length - 1),
    index = Math.min(grid.length - 2, Math.floor(scaled)),
    t = scaled - index;
  const offset = cubic(
    rowSample(grid[Math.max(0, index - 1)], u),
    rowSample(grid[index], u),
    rowSample(grid[index + 1], u),
    rowSample(grid[Math.min(grid.length - 1, index + 2)], u),
    t,
  );
  return base.map((value, axis) => value + offset[axis]) as Vec3;
}
