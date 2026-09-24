/** Uniform distant sun and opaque spherical blockers. All angles are radians.
 * Geometric identities hold in real arithmetic; floating-point error is not
 * certified, so these results must never authorize removal of scene content. */
export type SunVector = readonly [number, number, number];
export interface SunCapMoments {
  area: number;
  vector: [number, number, number];
}
const dot = (a: SunVector, b: SunVector) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const clamp = (x: number, low = 0, high = 1) => Math.max(low, Math.min(high, x));
function normalized(v: SunVector): [number, number, number] {
  const length = Math.hypot(...v);
  if (!v.every(Number.isFinite) || !Number.isFinite(length) || length <= 1e-15)
    throw new RangeError("Sun directions must be finite and nonzero");
  return [v[0] / length, v[1] / length, v[2] / length];
}
function validateRadius(radius: number) {
  if (!Number.isFinite(radius) || radius < 0 || radius > Math.PI / 2)
    throw new RangeError("Cap radius must be in [0, pi/2]");
}
export function sphericalCapArea(radius: number): number {
  validateRadius(radius);
  return 4 * Math.PI * Math.sin(radius / 2) ** 2;
}
function fullMoment(center: SunVector, radius: number): SunCapMoments {
  const scale = Math.PI * Math.sin(radius) ** 2;
  return {
    area: sphericalCapArea(radius),
    vector: [center[0] * scale, center[1] * scale, center[2] * scale],
  };
}
/** Shared-chord cancellation is performed symbolically before evaluation. */
function segmentMoment(x: number): number {
  if (Math.abs(x) >= 0.25) return x - Math.sin(x) * Math.cos(x);
  const q = x * x;
  return x * q * (2 / 3 + q * (-2 / 15 + q * (4 / 315 + q * (-2 / 2835 + (q * 4) / 155925))));
}
export function sphericalCapIntersection(
  first: SunVector,
  r1: number,
  second: SunVector,
  r2: number,
): SunCapMoments {
  validateRadius(r1);
  validateRadius(r2);
  const c1 = normalized(first),
    c2 = normalized(second);
  const cross = [c1[1] * c2[2] - c1[2] * c2[1], c1[2] * c2[0] - c1[0] * c2[2], c1[0] * c2[1] - c1[1] * c2[0]];
  const sinD = Math.hypot(...cross),
    d = Math.atan2(sinD, dot(c1, c2));
  if (r1 === 0 || r2 === 0 || d >= r1 + r2) return { area: 0, vector: [0, 0, 0] };
  if (d + r1 <= r2) return fullMoment(c1, r1);
  if (d + r2 <= r1) return fullMoment(c2, r2);
  const semi = (r1 + r2 + d) / 2;
  const angle = (r: number) =>
    2 * Math.asin(Math.sqrt(clamp((Math.sin(semi - d) * Math.sin(semi - r)) / (Math.sin(r) * sinD))));
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
  const area = clamp(
    4 * alpha * Math.sin(r1 / 2) ** 2 + 4 * beta * Math.sin(r2 / 2) ** 2 - 2 * excess,
    0,
    Math.min(sphericalCapArea(r1), sphericalCapArea(r2)),
  );
  const a = Math.sin(r1) ** 2 * segmentMoment(alpha),
    b = Math.sin(r2) ** 2 * segmentMoment(beta);
  return { area, vector: [a * c1[0] + b * c2[0], a * c1[1] + b * c2[1], a * c1[2] + b * c2[2]] };
}
export interface OpaqueSunSphere {
  center: SunVector;
  radius: number;
  opaque: boolean;
  rigid: boolean;
}
export type SphereSunResult =
  | {
      kind: "fallback";
      reason: "invalid-domain" | "unsupported-blocker" | "inside-blocker" | "horizon-clipping";
    }
  | {
      kind: "evaluated";
      fraction: number;
      vector: [number, number, number];
      diffuseIntegral: number;
      evidence: "real-arithmetic";
      numericError: "unknown";
    };
/** Single actual sphere only. Do not multiply the result into separately
 * filtered specular shading or treat a proxy as the actual geometry. */
export function evaluateSphereSun(
  receiver: SunVector,
  normal: SunVector,
  sun: SunVector,
  sunRadius: number,
  sphere: OpaqueSunSphere,
): SphereSunResult {
  if (!sphere.opaque || !sphere.rigid) return { kind: "fallback", reason: "unsupported-blocker" };
  if (
    !receiver.every(Number.isFinite) ||
    !sphere.center.every(Number.isFinite) ||
    !Number.isFinite(sphere.radius) ||
    sphere.radius <= 0 ||
    sunRadius < 1e-6
  )
    return { kind: "fallback", reason: "invalid-domain" };
  try {
    validateRadius(sunRadius);
    const direction = normalized(sun),
      n = normalized(normal);
    if (dot(n, direction) <= Math.sin(sunRadius) + 1e-12)
      return { kind: "fallback", reason: "horizon-clipping" };
    const delta: SunVector = [
      sphere.center[0] - receiver[0],
      sphere.center[1] - receiver[1],
      sphere.center[2] - receiver[2],
    ];
    const distance = Math.hypot(...delta);
    if (distance <= sphere.radius * (1 + 1e-12)) return { kind: "fallback", reason: "inside-blocker" };
    const blocked = sphericalCapIntersection(
      direction,
      sunRadius,
      delta,
      Math.asin(sphere.radius / distance),
    );
    const whole = fullMoment(direction, sunRadius);
    const vector: [number, number, number] = [
      whole.vector[0] - blocked.vector[0],
      whole.vector[1] - blocked.vector[1],
      whole.vector[2] - blocked.vector[2],
    ];
    return {
      kind: "evaluated",
      fraction: clamp(1 - blocked.area / whole.area),
      vector,
      diffuseIntegral: Math.max(0, dot(n, vector)),
      evidence: "real-arithmetic",
      numericError: "unknown",
    };
  } catch {
    return { kind: "fallback", reason: "invalid-domain" };
  }
}
export interface SunProxyCap {
  direction: SunVector;
  innerRadius: number;
  outerRadius: number;
}
/** Real-arithmetic union bounds. Overlapping interiors use MAX, never SUM.
 * Numeric margins have not been proved; ordinary shadows remain the fallback. */
export function proxySunVisibility(sun: SunVector, sunRadius: number, proxies: readonly SunProxyCap[]) {
  if (proxies.length > 64 || sunRadius < 1e-6) throw new RangeError("Sun proxy domain exceeds limits");
  const total = sphericalCapArea(sunRadius);
  normalized(sun);
  let minimumBlocked = 0,
    maximumBlocked = 0;
  for (const proxy of proxies) {
    if (proxy.innerRadius > proxy.outerRadius) throw new RangeError("Interior cap must fit exterior cap");
    minimumBlocked = Math.max(
      minimumBlocked,
      sphericalCapIntersection(sun, sunRadius, proxy.direction, proxy.innerRadius).area,
    );
    maximumBlocked += sphericalCapIntersection(sun, sunRadius, proxy.direction, proxy.outerRadius).area;
  }
  return {
    lower: clamp(1 - maximumBlocked / total),
    upper: clamp(1 - minimumBlocked / total),
    evidence: "real-arithmetic" as const,
    numericError: "unknown" as const,
    fallbackRequired: true as const,
  };
}
