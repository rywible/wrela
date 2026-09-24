import type { WaterPhaseCarrier } from "@wrela/model";

export type Pair = [number, number];
/** Invert the authored two-carrier slope map once, never per lighting sample. */
export function compileGlintMap(carriers: readonly WaterPhaseCarrier[]) {
  const active = carriers.filter((c) => Math.hypot(...c.slope) > 0);
  if (active.length !== 2) return;
  const [a, b] = active;
  const determinant = a.slope[0] * b.slope[1] - a.slope[1] * b.slope[0];
  const scale = Math.hypot(...a.slope) * Math.hypot(...b.slope);
  if (Math.abs(determinant) < scale * 0.01) return;
  return {
    kind: "two-wave-glints" as const,
    indices: [a.waveIndex, b.waveIndex] as [number, number],
    inverseSlope: [
      b.slope[1] / determinant,
      -b.slope[0] / determinant,
      -a.slope[1] / determinant,
      a.slope[0] / determinant,
    ] as [number, number, number, number],
  };
}
/** Up to four phase-space specular events per period, without scanning phases. */
export function glintPhases(inverse: readonly number[], slope: Pair): Pair[] {
  const cosines = [
    inverse[0] * slope[0] + inverse[1] * slope[1],
    inverse[2] * slope[0] + inverse[3] * slope[1],
  ];
  if (cosines.some((c) => Math.abs(c) > 1)) return [];
  const a = Math.acos(cosines[0]),
    b = Math.acos(cosines[1]);
  return [
    [a, b],
    [a, -b],
    [-a, b],
    [-a, -b],
  ];
}
/** Exact integral of (delta+|center+x*dx+y*dy|²)^-2 over a unit box.
 * Green's theorem turns a narrow 2D peak into four smooth boundary integrals.
 * Null indicates a degenerate or numerically cancellation-prone transform. */
export function integrateGlintRectangle(center: Pair, dx: Pair, dy: Pair, delta: number): number | null {
  if (![...center, ...dx, ...dy, delta].every(Number.isFinite) || delta <= 0) return null;
  const cross = (a: Pair, b: Pair) => a[0] * b[1] - a[1] * b[0];
  const determinant = cross(dx, dy);
  if (Math.abs(determinant) < 1e-7 * Math.hypot(...dx) * Math.hypot(...dy) || Math.abs(determinant) < 1e-20)
    return null;
  const corners = [
    [-0.5, -0.5],
    [0.5, -0.5],
    [0.5, 0.5],
    [-0.5, 0.5],
  ].map(([x, y]) => [center[0] + x * dx[0] + y * dy[0], center[1] + x * dx[1] + y * dy[1]] as Pair);
  let sum = 0;
  for (let i = 0; i < 4; i++) {
    const a = corners[i],
      b = corners[(i + 1) % 4],
      v: [number, number] = [b[0] - a[0], b[1] - a[1]];
    const A = v[0] * v[0] + v[1] * v[1],
      B = a[0] * v[0] + a[1] * v[1];
    const D = Math.max(A * delta + cross(a, v) ** 2, 1e-40),
      root = Math.sqrt(D);
    const integral = Math.atan2(A * root, D + B * (A + B)) / root;
    sum += cross(a, v) * integral;
  }
  const result = sum / (2 * delta * determinant);
  return result > 0 && Number.isFinite(result) ? result : null;
}
