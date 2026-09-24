import type { Vec3 } from "./math";

/** A rigid local-space zero set. Deformation is deliberately not part of this contract. */
export type QuadricShape = { center: Vec3; radii: Vec3; rotation: Vec3; nodeId: string; material?: string };

export function quadricUnitToLocal(shape: QuadricShape): Float32Array {
  const [x, y, z] = shape.rotation;
  const cx = Math.cos(x),
    sx = Math.sin(x),
    cy = Math.cos(y),
    sy = Math.sin(y),
    cz = Math.cos(z),
    sz = Math.sin(z);
  const [a, b, c] = shape.radii;
  return new Float32Array([
    cz * cy * a,
    sz * cy * a,
    -sy * a,
    0,
    (cz * sy * sx - sz * cx) * b,
    (sz * sy * sx + cz * cx) * b,
    cy * sx * b,
    0,
    (cz * sy * cx + sz * sx) * c,
    (sz * sy * cx - cz * sx) * c,
    cy * cx * c,
    0,
    ...shape.center,
    1,
  ]);
}

/** Gauss-Jordan inversion with pivoting; invalid/singular transforms use the mesh fallback. */
export function inverseMatrix(matrix: Float32Array): Float32Array | null {
  if (matrix.length !== 16 || matrix.some((v) => !Number.isFinite(v))) return null;
  const rows = Array.from({ length: 4 }, (_, r) =>
    Array.from({ length: 8 }, (_, c) => (c < 4 ? matrix[c * 4 + r] : +(c - 4 === r))),
  );
  for (let c = 0; c < 4; c++) {
    let pivot = c;
    for (let r = c + 1; r < 4; r++) if (Math.abs(rows[r][c]) > Math.abs(rows[pivot][c])) pivot = r;
    if (Math.abs(rows[pivot][c]) < 1e-30) return null;
    [rows[c], rows[pivot]] = [rows[pivot], rows[c]];
    const divisor = rows[c][c];
    for (let k = 0; k < 8; k++) rows[c][k] /= divisor;
    for (let r = 0; r < 4; r++) {
      if (r === c) continue;
      const factor = rows[r][c];
      for (let k = 0; k < 8; k++) rows[r][k] -= factor * rows[c][k];
    }
  }
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) out[c * 4 + r] = rows[r][c + 4];
  return out.every(Number.isFinite) ? out : null;
}

export function multiplyMatrices(a: Float32Array, b: Float32Array): Float32Array {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++)
    for (let r = 0; r < 4; r++) {
      let value = 0;
      for (let k = 0; k < 4; k++) value += a[k * 4 + r] * b[c * 4 + k];
      out[c * 4 + r] = value;
    }
  return out;
}

/** Parameter t uses the caller's direction length. Roots use the stable q formulation.
 * A negative discriminant is never clamped into a false positive silhouette. */
export function intersectQuadric(
  worldToUnit: Float32Array,
  origin: Vec3,
  direction: Vec3,
  minimum = 0,
  maximum = Infinity,
): { distance: number; position: Vec3; normal: Vec3 } | null {
  const transform = (p: Vec3, w: number): Vec3 =>
    [0, 1, 2].map(
      (r) =>
        worldToUnit[r] * p[0] +
        worldToUnit[r + 4] * p[1] +
        worldToUnit[r + 8] * p[2] +
        worldToUnit[r + 12] * w,
    ) as Vec3;
  const o = transform(origin, 1),
    d = transform(direction, 0);
  const a = d[0] ** 2 + d[1] ** 2 + d[2] ** 2;
  const b = o[0] * d[0] + o[1] * d[1] + o[2] * d[2];
  const c = o[0] ** 2 + o[1] ** 2 + o[2] ** 2 - 1;
  // Closest-point discriminant avoids subtracting two large near-equal squares.
  const closest = o.map((v, i) => v - d[i] * (b / a));
  const discriminant = a * (1 - closest.reduce((sum, v) => sum + v * v, 0));
  if (!(a > 0) || !Number.isFinite(discriminant) || discriminant < 0) return null;
  const root = Math.sqrt(discriminant),
    q = -b - (b < 0 ? -root : root);
  const t0 = q === 0 ? -b / a : q / a,
    t1 = q === 0 ? t0 : c / q;
  const near = Math.min(t0, t1),
    far = Math.max(t0, t1),
    distance = near >= minimum ? near : far;
  if (!Number.isFinite(distance) || distance < minimum || distance > maximum) return null;
  const unit = o.map((v, i) => v + d[i] * distance);
  const normal = [0, 1, 2].map(
    (c) => worldToUnit[c * 4] * unit[0] + worldToUnit[c * 4 + 1] * unit[1] + worldToUnit[c * 4 + 2] * unit[2],
  ) as Vec3;
  const length = Math.hypot(...normal);
  if (!(length > 0)) return null;
  return {
    distance,
    position: origin.map((v, i) => v + direction[i] * distance) as Vec3,
    normal: normal.map((v) => v / length) as Vec3,
  };
}
