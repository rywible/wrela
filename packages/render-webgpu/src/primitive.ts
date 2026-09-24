import {
  inverseMatrix,
  multiplyMatrices,
  type QuadricShape,
  quadricUnitToLocal,
  type RenderSurface,
} from "@wrela/model";

export const PRIMITIVE_FLOATS = 72;
export type PrimitiveRectangle = [number, number, number, number];
const FULL_VIEWPORT: PrimitiveRectangle = [-1, -1, 1, 1];

/** A transformed unit sphere lies inside its support-function AABB. For positive
 * homogeneous w, x/w and y/w on a box attain extrema at vertices: a point is a
 * convex combination of corners and its projected value is the w-weighted
 * convex combination of their projected values. Any near-plane/w uncertainty
 * keeps the complete viewport, including camera-inside and clipped proxies.
 * Numeric padding is an engineering guard, not a numeric-error certificate. */
export function primitiveRectangle(unitToWorld: Float32Array, vp: Float32Array): PrimitiveRectangle {
  if (
    unitToWorld.length !== 16 ||
    vp.length !== 16 ||
    !unitToWorld.every(Number.isFinite) ||
    !vp.every(Number.isFinite)
  )
    return [...FULL_VIEWPORT];
  const center = [unitToWorld[12], unitToWorld[13], unitToWorld[14]];
  const extent = [0, 1, 2].map((r) => {
    const radius = Math.hypot(unitToWorld[r], unitToWorld[r + 4], unitToWorld[r + 8]);
    return radius + 0.000004 * (Math.abs(center[r]) + radius + 1);
  });
  const rectangle: PrimitiveRectangle = [Infinity, Infinity, -Infinity, -Infinity];
  for (let corner = 0; corner < 8; corner++) {
    const point = center.map((value, axis) => value + (corner & (1 << axis) ? extent[axis] : -extent[axis]));
    const clip = [0, 1, 2, 3].map(
      (r) => vp[r] * point[0] + vp[r + 4] * point[1] + vp[r + 8] * point[2] + vp[r + 12],
    );
    const rounding = [0, 1, 2, 3].map(
      (r) =>
        0.000002 *
        (Math.abs(vp[r] * point[0]) +
          Math.abs(vp[r + 4] * point[1]) +
          Math.abs(vp[r + 8] * point[2]) +
          Math.abs(vp[r + 12]) +
          1),
    );
    if (clip.some((v) => !Number.isFinite(v)) || clip[3] <= rounding[3] || clip[2] <= rounding[2])
      return [...FULL_VIEWPORT];
    for (let axis = 0; axis < 2; axis++) {
      // Expand numerator and denominator before division to cover cancellation.
      for (const numerator of [clip[axis] - rounding[axis], clip[axis] + rounding[axis]])
        for (const denominator of [clip[3] - rounding[3], clip[3] + rounding[3]]) {
          const projected = numerator / denominator;
          rectangle[axis] = Math.min(rectangle[axis], projected - 0.002);
          rectangle[axis + 2] = Math.max(rectangle[axis + 2], projected + 0.002);
        }
    }
  }
  return rectangle.map((value) => Math.max(-1, Math.min(1, value))) as PrimitiveRectangle;
}
export type PrimitiveViews = { inverseVP: Float32Array; inverseLightVP: Float32Array };
export function preparePrimitiveViews(cameraVP: Float32Array, lightVP: Float32Array): PrimitiveViews | null {
  const inverseVP = inverseMatrix(cameraVP),
    inverseLightVP = inverseMatrix(lightVP);
  return inverseVP && inverseLightVP ? { inverseVP, inverseLightVP } : null;
}

/** Affine cofactor inversion avoids allocating a Gauss-Jordan system per instance. */
function inverseAffine(matrix: Float32Array): Float32Array | null {
  if (matrix[3] !== 0 || matrix[7] !== 0 || matrix[11] !== 0 || matrix[15] !== 1) return null;
  const a = matrix[0],
    b = matrix[4],
    c = matrix[8],
    d = matrix[1],
    e = matrix[5],
    f = matrix[9],
    g = matrix[2],
    h = matrix[6],
    i = matrix[10];
  const determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
  if (!Number.isFinite(determinant) || Math.abs(determinant) < 1e-30) return null;
  const out = new Float32Array([
    (e * i - f * h) / determinant,
    (f * g - d * i) / determinant,
    (d * h - e * g) / determinant,
    0,
    (c * h - b * i) / determinant,
    (a * i - c * g) / determinant,
    (b * g - a * h) / determinant,
    0,
    (b * f - c * e) / determinant,
    (c * d - a * f) / determinant,
    (a * e - b * d) / determinant,
    0,
    0,
    0,
    0,
    1,
  ]);
  for (let r = 0; r < 3; r++)
    out[12 + r] = -(out[r] * matrix[12] + out[r + 4] * matrix[13] + out[r + 8] * matrix[14]);
  return out.every(Number.isFinite) ? out : null;
}
const primitiveMatrices = new WeakMap<QuadricShape, Float32Array>();

/** Per-instance storage uses render-relative matrices; inverses of view/light matrices are shared across packing. */
export function packPrimitive(
  shape: QuadricShape,
  model: Float32Array,
  cameraVP: Float32Array,
  lightVP: Float32Array,
  preparedViews?: PrimitiveViews,
): Float32Array | null {
  if (shape.radii.some((radius) => !(radius > 0) || !Number.isFinite(radius))) return null;
  let unitToLocal = primitiveMatrices.get(shape);
  if (!unitToLocal) {
    unitToLocal = quadricUnitToLocal(shape);
    primitiveMatrices.set(shape, unitToLocal);
  }
  const unitToWorld = multiplyMatrices(model, unitToLocal);
  const worldToUnit = inverseAffine(unitToWorld);
  const views = preparedViews ?? preparePrimitiveViews(cameraVP, lightVP);
  if (!worldToUnit || !views) return null;
  const { inverseVP, inverseLightVP } = views;
  // Reject severely ill-conditioned transforms before FP32 ray evaluation.
  const norm = (m: Float32Array) => Math.hypot(m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]);
  if (norm(unitToWorld) * norm(worldToUnit) > 10000) return null;
  const packed = new Float32Array(PRIMITIVE_FLOATS);
  packed.set(worldToUnit);
  packed.set(unitToLocal, 16);
  packed.set(inverseVP, 32);
  packed.set(inverseLightVP, 48);
  packed.set(primitiveRectangle(unitToWorld, cameraVP), 64);
  packed.set(primitiveRectangle(unitToWorld, lightVP), 68);
  return packed;
}

/** Records have the same order as the renderer instance buffer. No per-instance GPU resource is required. */
export function packPrimitiveBatch(
  surfaces: readonly RenderSurface[],
  cameraVP: Float32Array,
  lightVP: Float32Array,
): Float32Array | null {
  const views = preparePrimitiveViews(cameraVP, lightVP);
  if (!views) return null;
  const records = new Float32Array(surfaces.length * PRIMITIVE_FLOATS);
  for (let i = 0; i < surfaces.length; i++) {
    const surface = surfaces[i],
      product = surface.selectedRenderProduct;
    if (product?.kind !== "analytic-quadric" || surface.skin || surface.wind || surface.water) return null;
    const record = packPrimitive(product.primitive, surface.matrix, cameraVP, lightVP, views);
    if (!record) return null;
    records.set(record, i * PRIMITIVE_FLOATS);
  }
  return records;
}
