/** Exact spherical-cap intersection moments in real arithmetic.
 * Used for uniform radiance at infinity and opaque spherical occluders.
 * The first moment integrates affine angular shading, including un-clipped
 * Lambertian irradiance. It does not authorize multiplying filtered specular
 * radiance by mean visibility. */
import { normalized, type Vector } from "./coherent-ggx";

const dot = (a: Vector, b: Vector) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const scaled = (v: Vector, s: number) => v.map((x) => x * s) as Vector;
export const capArea = (radius: number) => 4 * Math.PI * Math.sin(radius / 2) ** 2;
const fullMoment = (center: Vector, radius: number) => scaled(center, Math.PI * Math.sin(radius) ** 2);
export interface CapMoments {
  area: number;
  vector: Vector;
}
/** x - sin(x)cos(x), evaluated without cancellation at a small cap arc. */
export function segmentMoment(x: number) {
  if (Math.abs(x) >= 0.25) return x - Math.sin(x) * Math.cos(x);
  const q = x * x;
  return x * q * (2 / 3 + q * (-2 / 15 + q * (4 / 315 + q * (-2 / 2835 + (q * 4) / 155925))));
}
export function capIntersection(first: Vector, r1: number, second: Vector, r2: number): CapMoments {
  const c1 = normalized(first),
    c2 = normalized(second);
  const cross: Vector = [
    c1[1] * c2[2] - c1[2] * c2[1],
    c1[2] * c2[0] - c1[0] * c2[2],
    c1[0] * c2[1] - c1[1] * c2[0],
  ];
  const sinD = Math.hypot(...cross),
    cosD = dot(c1, c2),
    d = Math.atan2(sinD, cosD);
  if (r1 <= 0 || r2 <= 0 || d >= r1 + r2) return { area: 0, vector: [0, 0, 0] };
  if (d + r1 <= r2) return { area: capArea(r1), vector: fullMoment(c1, r1) };
  if (d + r2 <= r1) return { area: capArea(r2), vector: fullMoment(c2, r2) };
  const semi = (r1 + r2 + d) / 2;
  // Half-angle form avoids subtracting almost equal cosines for a small sun.
  const angle = (a: number) =>
    2 *
    Math.asin(
      Math.sqrt(Math.max(0, Math.min(1, (Math.sin(semi - d) * Math.sin(semi - a)) / (Math.sin(a) * sinD)))),
    );
  const alpha = angle(r1),
    beta = angle(r2);
  const excess =
    4 *
    Math.atan(
      Math.sqrt(
        Math.max(
          0,
          Math.tan(semi / 2) *
            Math.tan((semi - r1) / 2) *
            Math.tan((semi - r2) / 2) *
            Math.tan((semi - d) / 2),
        ),
      ),
    );
  const area = 4 * alpha * Math.sin(r1 / 2) ** 2 + 4 * beta * Math.sin(r2 / 2) ** 2 - 2 * excess;
  // The boundary's shared chord cancels analytically, before floating point.
  const firstMoment = Math.sin(r1) ** 2 * segmentMoment(alpha),
    secondMoment = Math.sin(r2) ** 2 * segmentMoment(beta);
  const vector = c1.map((v, i) => firstMoment * v + secondMoment * c2[i]) as Vector;
  return { area, vector };
}
export function visibleSun(sun: Vector, radius: number, occluder: Vector, angularRadius: number) {
  const overlap = capIntersection(sun, radius, occluder, angularRadius),
    total = capArea(radius),
    whole = fullMoment(sun, radius);
  return { fraction: 1 - overlap.area / total, vector: whole.map((v, i) => v - overlap.vector[i]) as Vector };
}
/** Conservative area bounds for a UNION of opaque shapes with inner/outer caps.
 * The outer union bound is a sum; inner overlap is only a max unless disjointness
 * has separately been proved. Alpha blending is outside this contract. */
export function visibilityInterval(
  sun: Vector,
  radius: number,
  occluders: { center: Vector; inner: number; outer: number }[],
) {
  const total = capArea(radius);
  let minimumBlocked = 0,
    maximumBlocked = 0;
  for (const c of occluders) {
    minimumBlocked = Math.max(minimumBlocked, capIntersection(sun, radius, c.center, c.inner).area);
    maximumBlocked += capIntersection(sun, radius, c.center, c.outer).area;
  }
  return {
    lo: Math.max(0, 1 - maximumBlocked / total),
    hi: Math.max(0, Math.min(1, 1 - minimumBlocked / total)),
  };
}
export function integrateCapReference(r1: number, r2: number, distance: number, steps = 32768): CapMoments {
  // Independent 1D quadrature: integrate azimuthally covered arcs of the second
  // cap, centered at +Z; the first cap center lies in its XZ meridian.
  const cosA = Math.cos(r1),
    cosD = Math.cos(distance),
    sinD = Math.sin(distance),
    height = 2 * Math.sin(r2 / 2) ** 2;
  let area = 0,
    x = 0,
    z = 0;
  for (let i = 0; i < steps; i++) {
    const q = (height * (i + 0.5)) / steps,
      cz = 1 - q,
      sz = Math.sqrt(q * (2 - q));
    const cut = (cosA - cosD * cz) / (sinD * sz);
    const phi = Math.acos(Math.max(-1, Math.min(1, cut)));
    area += 2 * phi;
    x += 2 * sz * Math.sin(phi);
    z += 2 * cz * phi;
  }
  return { area: (area * height) / steps, vector: [(x * height) / steps, 0, (z * height) / steps] };
}
