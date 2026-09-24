import type { Bounds, RenderSurface, Vec3 } from "@wrela/model";

export type InnerSphere = { center: Vec3; radius: number };
export type VisibilityRejection =
  | "inactive-realization"
  | "deformed"
  | "partial-surface"
  | "invalid-transform"
  | "numeric-margin";
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const finiteBounds = (bounds: Bounds) =>
  [...bounds.min, ...bounds.max].every(Number.isFinite) &&
  bounds.min.every((value, axis) => value <= bounds.max[axis]);
// Accounts for ordinary FP32 transform/projection accumulation, and deliberately
// gives up at large coordinates or tiny features. This is not a general shader
// interval certificate: activation remains explicit until differential captures
// establish the supported adapter/realization domain.
const margin = (...values: number[]) => Math.max(1, ...values.map(Math.abs)) * 128 * 2 ** -23;

/** Only the active exact realization authorizes a proxy. Authored field facts
 * cannot stand in for the currently drawn mesh, pose, or partial material group. */
export function analyticInnerSphere(
  surface: RenderSurface,
  reject?: (reason: VisibilityRejection) => void,
): InnerSphere | null {
  const fail = (reason: VisibilityRejection) => {
    reject?.(reason);
    return null;
  };
  const product = surface.selectedRenderProduct;
  if (product?.kind !== "analytic-quadric") return fail("inactive-realization");
  if (surface.skin || surface.deformation || surface.wind || surface.water) return fail("deformed");
  if (
    surface.drawRange &&
    (surface.drawRange.start !== 0 || surface.drawRange.count !== surface.mesh.indices.length)
  )
    return fail("partial-surface");
  const m = surface.matrix;
  if (
    m.length !== 16 ||
    ![...m].every(Number.isFinite) ||
    m[3] !== 0 ||
    m[7] !== 0 ||
    m[11] !== 0 ||
    m[15] !== 1
  )
    return fail("invalid-transform");
  const axes = [0, 4, 8].map((offset) => [m[offset], m[offset + 1], m[offset + 2]] as Vec3);
  const lengths = axes.map((axis) => dot(axis, axis));
  const largest = Math.max(...lengths);
  if (
    !(largest > 0) ||
    lengths.some((value) => Math.abs(value - largest) > largest * 1e-6) ||
    axes.some((axis, i) => axes.some((other, j) => i !== j && Math.abs(dot(axis, other)) > largest * 1e-6))
  )
    return fail("invalid-transform");
  // Gershgorin lower bound on the minimum squared singular value also retains
  // safety for the small off-diagonal terms from rounded rotation matrices.
  const lower = Math.min(
    ...axes.map(
      (axis, i) =>
        lengths[i] - axes.reduce((sum, other, j) => sum + (i === j ? 0 : Math.abs(dot(axis, other))), 0),
    ),
  );
  const shape = product.primitive;
  if (
    ![...shape.center, ...shape.radii, ...shape.rotation].every(Number.isFinite) ||
    shape.radii.some((r) => r <= 0)
  )
    return fail("numeric-margin");
  const center = [0, 1, 2].map(
    (row) =>
      m[row] * shape.center[0] + m[row + 4] * shape.center[1] + m[row + 8] * shape.center[2] + m[row + 12],
  ) as Vec3;
  const rawRadius = Math.min(...shape.radii) * Math.sqrt(Math.max(0, lower));
  const radius = rawRadius - margin(...center, rawRadius, ...m);
  if (!Number.isFinite(radius) || !(radius > 0)) return fail("numeric-margin");
  return { center, radius };
}

function corners(bounds: Bounds): Vec3[] {
  return Array.from({ length: 8 }, (_, i) => [
    bounds[i & 1 ? "max" : "min"][0],
    bounds[i & 2 ? "max" : "min"][1],
    bounds[i & 4 ? "max" : "min"][2],
  ]);
}

/** The cone behind a sphere is convex. Containing every AABB corner therefore
 * contains the entire candidate, including its current deformation envelope.
 * Use the far sphere plane rather than an unstable grazing intersection root. */
export function hiddenFromPoint(
  bounds: Bounds,
  sphere: InnerSphere,
  eye: Vec3,
  viewProjection: Float32Array,
): boolean {
  if (
    !finiteBounds(bounds) ||
    ![...sphere.center, sphere.radius, ...eye, ...viewProjection].every(Number.isFinite) ||
    sphere.radius <= 0 ||
    viewProjection.length !== 16
  )
    return false;
  const epsilon = margin(...bounds.min, ...bounds.max, ...sphere.center, ...eye, sphere.radius);
  const radius = sphere.radius - epsilon;
  const delta = sub(sphere.center, eye),
    distance = Math.hypot(...delta);
  if (!(radius > 0) || !(distance > sphere.radius + epsilon)) return false;
  // Never use an interior clipped by the near plane as a depth occluder.
  const m = viewProjection;
  const near = m[2] * sphere.center[0] + m[6] * sphere.center[1] + m[10] * sphere.center[2] + m[14];
  if (!(near > sphere.radius * Math.hypot(m[2], m[6], m[10]) + epsilon)) return false;
  const direction = delta.map((v) => v / distance) as Vec3;
  const slope = radius / Math.sqrt(distance * distance - radius * radius);
  return corners(bounds).every((point) => {
    const ray = sub(point, eye),
      axial = dot(ray, direction);
    // Cross product avoids catastrophic cancellation in |ray|² - axial².
    const lateral = Math.hypot(
      ray[1] * direction[2] - ray[2] * direction[1],
      ray[2] * direction[0] - ray[0] * direction[2],
      ray[0] * direction[1] - ray[1] * direction[0],
    );
    return axial > distance + sphere.radius + epsilon && lateral + epsilon < axial * slope;
  });
}

/** Direction toward the sun; this test has no dependency on camera visibility. */
export function hiddenFromDirection(bounds: Bounds, sphere: InnerSphere, towardLight: Vec3): boolean {
  if (!finiteBounds(bounds) || ![...sphere.center, sphere.radius, ...towardLight].every(Number.isFinite))
    return false;
  const length = Math.hypot(...towardLight);
  if (!(length > 0)) return false;
  const direction = towardLight.map((v) => v / length) as Vec3;
  const epsilon = margin(...bounds.min, ...bounds.max, ...sphere.center, sphere.radius);
  const radius = sphere.radius - epsilon;
  if (!(radius > 0)) return false;
  return corners(bounds).every((point) => {
    const delta = sub(point, sphere.center),
      axial = dot(delta, direction);
    const lateral = Math.hypot(
      delta[1] * direction[2] - delta[2] * direction[1],
      delta[2] * direction[0] - delta[0] * direction[2],
      delta[0] * direction[1] - delta[1] * direction[0],
    );
    return axial < -sphere.radius - epsilon && lateral + epsilon < radius;
  });
}

/** A clipped proxy cannot certify depths in the affected pass. */
export function sphereInsideDepth(sphere: InnerSphere, matrix: Float32Array): boolean {
  if (matrix.length !== 16 || ![...matrix, ...sphere.center, sphere.radius].every(Number.isFinite))
    return false;
  const epsilon = margin(...sphere.center, sphere.radius, ...matrix);
  const planes = [
    [matrix[2], matrix[6], matrix[10], matrix[14]],
    [matrix[3] - matrix[2], matrix[7] - matrix[6], matrix[11] - matrix[10], matrix[15] - matrix[14]],
  ];
  return planes.every(
    (plane) =>
      plane[0] * sphere.center[0] + plane[1] * sphere.center[1] + plane[2] * sphere.center[2] + plane[3] >
      sphere.radius * Math.hypot(plane[0], plane[1], plane[2]) + epsilon,
  );
}

export type PreparedPointOccluder = {
  sphere: InnerSphere;
  eye: Vec3;
  direction: Vec3;
  distance: number;
  near: number;
  nearScale: number;
  coordinateScale: number;
};
export type PreparedDirectionalOccluder = { sphere: InnerSphere; direction: Vec3; coordinateScale: number };
const NUMERIC_MARGIN = 128 * 2 ** -23;
function boundsScale(bounds: Bounds): number {
  let maximum = 1;
  for (let axis = 0; axis < 3; axis++) {
    const low = bounds.min[axis],
      high = bounds.max[axis];
    if (!Number.isFinite(low) || !Number.isFinite(high) || low > high) return Infinity;
    maximum = Math.max(maximum, Math.abs(low), Math.abs(high));
  }
  return maximum;
}
/** Validate proxy and view invariants once, not once for every candidate object. */
export function preparePointOccluder(
  sphere: InnerSphere,
  eye: Vec3,
  viewProjection: Float32Array,
): PreparedPointOccluder | null {
  if (
    viewProjection.length !== 16 ||
    ![...sphere.center, sphere.radius, ...eye, ...viewProjection].every(Number.isFinite) ||
    sphere.radius <= 0
  )
    return null;
  const delta = sub(sphere.center, eye),
    distance = Math.hypot(...delta);
  if (!(distance > sphere.radius)) return null;
  const m = viewProjection;
  return {
    sphere,
    eye,
    distance,
    direction: delta.map((v) => v / distance) as Vec3,
    near: m[2] * sphere.center[0] + m[6] * sphere.center[1] + m[10] * sphere.center[2] + m[14],
    nearScale: Math.hypot(m[2], m[6], m[10]),
    coordinateScale: Math.max(1, ...sphere.center.map(Math.abs), ...eye.map(Math.abs), sphere.radius),
  };
}
export function prepareDirectionalOccluder(
  sphere: InnerSphere,
  towardLight: Vec3,
): PreparedDirectionalOccluder | null {
  if (![...sphere.center, sphere.radius, ...towardLight].every(Number.isFinite) || sphere.radius <= 0)
    return null;
  const length = Math.hypot(...towardLight);
  if (!(length > 0)) return null;
  return {
    sphere,
    direction: towardLight.map((v) => v / length) as Vec3,
    coordinateScale: Math.max(1, ...sphere.center.map(Math.abs), sphere.radius),
  };
}
export function hiddenFromPreparedPoint(bounds: Bounds, query: PreparedPointOccluder): boolean {
  const epsilon = Math.max(boundsScale(bounds), query.coordinateScale) * NUMERIC_MARGIN;
  const radius = query.sphere.radius - epsilon;
  if (
    !(radius > 0) ||
    !(query.distance > query.sphere.radius + epsilon) ||
    !(query.near > query.sphere.radius * query.nearScale + epsilon)
  )
    return false;
  const slope = radius / Math.sqrt(query.distance ** 2 - radius ** 2),
    direction = query.direction;
  for (let corner = 0; corner < 8; corner++) {
    const x = bounds[corner & 1 ? "max" : "min"][0] - query.eye[0],
      y = bounds[corner & 2 ? "max" : "min"][1] - query.eye[1],
      z = bounds[corner & 4 ? "max" : "min"][2] - query.eye[2];
    const axial = x * direction[0] + y * direction[1] + z * direction[2];
    if (!(axial > query.distance + query.sphere.radius + epsilon)) return false;
    const lateral = Math.hypot(
      y * direction[2] - z * direction[1],
      z * direction[0] - x * direction[2],
      x * direction[1] - y * direction[0],
    );
    if (!(lateral + epsilon < axial * slope)) return false;
  }
  return true;
}
export function hiddenFromPreparedDirection(bounds: Bounds, query: PreparedDirectionalOccluder): boolean {
  const epsilon = Math.max(boundsScale(bounds), query.coordinateScale) * NUMERIC_MARGIN;
  const radius = query.sphere.radius - epsilon;
  if (!(radius > 0)) return false;
  const direction = query.direction;
  for (let corner = 0; corner < 8; corner++) {
    const x = bounds[corner & 1 ? "max" : "min"][0] - query.sphere.center[0],
      y = bounds[corner & 2 ? "max" : "min"][1] - query.sphere.center[1],
      z = bounds[corner & 4 ? "max" : "min"][2] - query.sphere.center[2];
    const axial = x * direction[0] + y * direction[1] + z * direction[2];
    if (!(axial < -query.sphere.radius - epsilon)) return false;
    const lateral = Math.hypot(
      y * direction[2] - z * direction[1],
      z * direction[0] - x * direction[2],
      x * direction[1] - y * direction[0],
    );
    if (!(lateral + epsilon < radius)) return false;
  }
  return true;
}
