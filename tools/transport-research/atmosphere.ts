/** Spherical exponential atmosphere. Units: kilometres and inverse kilometres.
 * Bounds use concavity of log density; no sampled curvature estimate is needed.
 * The proof is real arithmetic. JS/WGSL results do not provide directed rounding. */
export interface AtmosphereRay {
  height: number;
  cosine: number;
  length: number;
  scale: number;
}
export const planetRadius = 6371;
export function altitude(ray: AtmosphereRay, s: number) {
  const r = planetRadius + ray.height;
  // Rationalization avoids subtracting two nearly equal planetary radii.
  const delta = s * (2 * r * ray.cosine + s);
  return ray.height + delta / (Math.sqrt(r * r + delta) + r);
}
export function logDensity(ray: AtmosphereRay, s: number) {
  return -altitude(ray, s) / ray.scale;
}
export function topDistance(height: number, cosine: number, top = 100) {
  const r = planetRadius + height,
    d = (top - height) * (2 * planetRadius + top + height);
  return d / (Math.sqrt(r * r * cosine * cosine + d) + r * cosine);
}
/** Integral of exp(linear interpolation of endpoint log values), stably. */
export function exponentialIntegral(logA: number, logB: number, length: number) {
  const delta = Math.abs(logB - logA);
  const meanFactor = delta < 1e-5 ? 1 - delta / 2 + (delta * delta) / 6 : -Math.expm1(-delta) / delta;
  return length * Math.exp(Math.max(logA, logB)) * meanFactor;
}
export interface SegmentBound {
  start: number;
  end: number;
  lower: number;
  upper: number;
}
export function segmentBound(ray: AtmosphereRay, start: number, end: number): SegmentBound {
  const mid = (start + end) / 2,
    r = planetRadius + ray.height;
  const rm = Math.sqrt(r * r + mid * (2 * r * ray.cosine + mid));
  const lm = logDensity(ray, mid),
    slope = -(r * ray.cosine + mid) / (ray.scale * rm);
  const half = (end - start) / 2;
  return {
    start,
    end,
    lower: exponentialIntegral(logDensity(ray, start), logDensity(ray, end), end - start),
    upper: exponentialIntegral(lm - slope * half, lm + slope * half, end - start),
  };
}
export function boundedOpticalDepth(
  ray: AtmosphereRay,
  extinction: number,
  transmissionBudget = 1e-5,
  maxSegments = 256,
) {
  const segments = [segmentBound(ray, 0, ray.length)];
  let lower = segments[0].lower,
    upper = segments[0].upper;
  while (
    Math.exp(-extinction * lower) - Math.exp(-extinction * upper) > transmissionBudget &&
    segments.length < maxSegments
  ) {
    let worst = 0;
    for (let i = 1; i < segments.length; i++)
      if (segments[i].upper - segments[i].lower > segments[worst].upper - segments[worst].lower) worst = i;
    const old = segments[worst],
      mid = (old.start + old.end) / 2;
    const a = segmentBound(ray, old.start, mid),
      b = segmentBound(ray, mid, old.end);
    lower += a.lower + b.lower - old.lower;
    upper += a.upper + b.upper - old.upper;
    segments[worst] = a;
    segments.push(b);
  }
  return {
    lower,
    upper,
    transmissionLower: Math.exp(-extinction * upper),
    transmissionUpper: Math.exp(-extinction * lower),
    segments,
  };
}
export function quadratureOpticalDepth(ray: AtmosphereRay, steps = 16384) {
  // Composite Simpson with even step count, independently evaluated.
  let sum = Math.exp(logDensity(ray, 0)) + Math.exp(logDensity(ray, ray.length));
  for (let i = 1; i < steps; i++)
    sum += (i % 2 ? 4 : 2) * Math.exp(logDensity(ray, (ray.length * i) / steps));
  return (sum * ray.length) / (3 * steps);
}
export function fixedOpticalDepth(ray: AtmosphereRay, segments: number) {
  let lower = 0,
    upper = 0;
  for (let i = 0; i < segments; i++) {
    // Concentrate work near the camera where low scale-height media are densest.
    const bound = segmentBound(ray, ray.length * (i / segments) ** 2, ray.length * ((i + 1) / segments) ** 2);
    lower += bound.lower;
    upper += bound.upper;
  }
  return { lower, upper, estimate: (lower + upper) / 2 };
}
