/** Experimental importance integration over a coherent, complete wave orbit.
 * The sampled domain is authored wave phase, not the GGX visible-normal domain.
 * Bounded rejection + an explicitly normalized fallback mixture preserve the
 * estimator expectation in real arithmetic. Float32 needs independent validation. */
import { type Pair, tau } from "./spectral";

export type Vector = [number, number, number];
export interface WaveOrbit {
  mean: Pair;
  a: Pair;
  b: Pair;
}
export interface Lighting {
  view: Vector;
  light: Vector;
  roughness: number;
  f0: number;
}
export interface OrbitPlan {
  orbit: WaveOrbit;
  lighting: Lighting;
  matrix: [number, number, number];
  center: Pair;
  delta: number;
  metric: number;
  axis: Pair;
  qRadius: number;
  low: number;
  high: number;
  normalization: number;
  acceptance: number;
  condition: number;
  control?: number;
  refined?: boolean;
}
export const normalized = (v: Vector): Vector => {
  const n = Math.hypot(...v);
  return v.map((x) => x / n) as Vector;
};
const dot = (a: Vector, b: Vector) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export function coherentOrbit(slopes: Pair[], offsets: number[], mean: Pair = [0, 0]): WaveOrbit {
  const a: Pair = [0, 0],
    b: Pair = [0, 0];
  slopes.forEach((v, i) => {
    for (let j = 0; j < 2; j++) {
      a[j] += v[j] * Math.cos(offsets[i]);
      b[j] -= v[j] * Math.sin(offsets[i]);
    }
  });
  return { mean, a, b };
}
export function slopeAt(orbit: WaveOrbit, c: number, s: number): Pair {
  return [orbit.mean[0] + orbit.a[0] * c + orbit.b[0] * s, orbit.mean[1] + orbit.a[1] * c + orbit.b[1] * s];
}
/** Independent reference: conventional normalized-vector GGX/Smith shading. */
export function directResponse(slope: Pair, lighting: Lighting) {
  const n = normalized([-slope[0], 1, -slope[1]]),
    v = lighting.view,
    l = lighting.light;
  const h = normalized([v[0] + l[0], v[1] + l[1], v[2] + l[2]]);
  const nv = dot(n, v),
    nl = dot(n, l),
    nh = Math.max(0, dot(n, h)),
    vh = dot(v, h);
  if (nv <= 0 || nl <= 0) return 0;
  const alpha2 = lighting.roughness ** 4,
    denominator = nh * nh * (alpha2 - 1) + 1;
  const D = alpha2 / (Math.PI * denominator * denominator);
  const lambda = (x: number) => (Math.sqrt(1 + (alpha2 * (1 - x * x)) / (x * x)) - 1) / 2;
  const G = 1 / (1 + lambda(nv) + lambda(nl));
  const F = lighting.f0 + (1 - lighting.f0) * (1 - vh) ** 5;
  return (D * G * F) / (4 * nv);
}
export function prepareOrbit(orbit: WaveOrbit, lighting: Lighting): OrbitPlan | null {
  const { a, b } = orbit,
    determinant = a[0] * b[1] - a[1] * b[0];
  const alpha2 = lighting.roughness ** 4;
  if (Math.abs(determinant) < 1e-9 || !(alpha2 > 0)) return null;
  const h = normalized(lighting.view.map((v, i) => v + lighting.light[i]) as Vector);
  const beta = 1 - alpha2,
    detA = h[1] ** 2 + alpha2 * (h[0] ** 2 + h[2] ** 2);
  if (detA <= 1e-10) return null;
  const matrix: [number, number, number] = [1 - beta * h[0] ** 2, -beta * h[0] * h[2], 1 - beta * h[2] ** 2];
  const center: Pair = [(-beta * h[1] * h[0]) / detA, (-beta * h[1] * h[2]) / detA];
  const delta = alpha2 / detA,
    metric = Math.abs(determinant) * Math.sqrt(detA);
  const tx = center[0] - orbit.mean[0],
    ty = center[1] - orbit.mean[1];
  const x: Pair = [(b[1] * tx - b[0] * ty) / determinant, (-a[1] * tx + a[0] * ty) / determinant];
  const qRadius = Math.hypot(...x),
    gamma2 = delta / metric;
  const low = gamma2 + (qRadius - 1) ** 2,
    high = gamma2 + (qRadius + 1) ** 2;
  const A = (low + high) / 2,
    normalization = A / (metric * metric * (low * high) ** 1.5);
  const quadratic = (v: Pair) => matrix[0] * v[0] ** 2 + 2 * matrix[1] * v[0] * v[1] + matrix[2] * v[1] ** 2;
  const trace = quadratic(a) + quadratic(b),
    largest = (trace + Math.sqrt(Math.max(0, trace * trace - 4 * metric * metric))) / 2;
  return {
    orbit,
    lighting,
    matrix,
    center,
    delta,
    metric,
    axis: qRadius > 0 ? [x[0] / qRadius, x[1] / qRadius] : [1, 0],
    qRadius,
    low,
    high,
    normalization,
    acceptance: A / high,
    condition: largest / metric,
  };
}
/** Stable numerator after factoring the sharp GGX quadratic denominator. */
export function smoothNumerator(slope: Pair, lighting: Lighting) {
  const norm2 = 1 + slope[0] ** 2 + slope[1] ** 2,
    inv = 1 / Math.sqrt(norm2),
    v = lighting.view,
    l = lighting.light;
  const nv = (-slope[0] * v[0] + v[1] - slope[1] * v[2]) * inv;
  const nl = (-slope[0] * l[0] + l[1] - slope[1] * l[2]) * inv;
  if (nv <= 0 || nl <= 0) return 0;
  const h = normalized([v[0] + l[0], v[1] + l[1], v[2] + l[2]]),
    vh = dot(v, h),
    alpha2 = lighting.roughness ** 4;
  const lambda = (c: number) => (Math.sqrt(1 + (alpha2 * (1 - c * c)) / (c * c)) - 1) / 2;
  const G = 1 / (1 + lambda(nv) + lambda(nl)),
    F = lighting.f0 + (1 - lighting.f0) * (1 - vh) ** 5;
  return (alpha2 * norm2 * norm2 * G * F) / (4 * Math.PI * nv);
}
export function quadraticAt(plan: OrbitPlan, slope: Pair) {
  const x = slope[0] - plan.center[0],
    y = slope[1] - plan.center[1];
  return plan.delta + plan.matrix[0] * x * x + 2 * plan.matrix[1] * x * y + plan.matrix[2] * y * y;
}
/** Match the value and curvature of the dominant GGX pole. This changes only
 * the importance proposal; the residual remains exact even when this fit is poor. */
export function refineOrbit(plan: OrbitPlan, useControl = true): OrbitPlan {
  let axis: Pair = [...plan.axis];
  const A = plan.matrix;
  const bilinear = (a: Pair, b: Pair) =>
    A[0] * a[0] * b[0] + A[1] * (a[0] * b[1] + a[1] * b[0]) + A[2] * a[1] * b[1];
  let curvature = 0,
    gradient = 0;
  for (let i = 0; i < 6; i++) {
    const slope = slopeAt(plan.orbit, ...axis),
      delta: Pair = [slope[0] - plan.center[0], slope[1] - plan.center[1]];
    const tangent: Pair = [
      -plan.orbit.a[0] * axis[1] + plan.orbit.b[0] * axis[0],
      -plan.orbit.a[1] * axis[1] + plan.orbit.b[1] * axis[0],
    ];
    const second: Pair = [plan.orbit.mean[0] - slope[0], plan.orbit.mean[1] - slope[1]];
    gradient = 2 * bilinear(tangent, delta);
    curvature = 2 * (bilinear(tangent, tangent) + bilinear(second, delta));
    if (i === 5 || curvature <= 0) break;
    const step = Math.max(-0.5, Math.min(0.5, -gradient / curvature));
    const inv = 1 / Math.sqrt(1 + step * step);
    axis = [(axis[0] - step * axis[1]) * inv, (axis[1] + step * axis[0]) * inv];
  }
  let result = { ...plan, refined: false };
  if (
    curvature > 0 &&
    Math.abs(gradient) < 1e-5 * Math.max(curvature, 1e-8) &&
    plan.qRadius > 0.55 &&
    plan.qRadius < 1.6
  ) {
    const low = quadraticAt(plan, slopeAt(plan.orbit, ...axis)),
      high = low + 2 * curvature,
      average = (low + high) / 2;
    result = {
      ...plan,
      axis,
      low,
      high,
      metric: 1,
      qRadius: curvature / 2,
      normalization: average / (low * high) ** 1.5,
      acceptance: average / high,
      refined: true,
    };
  }
  if (useControl) {
    const slope = slopeAt(result.orbit, ...result.axis),
      proposal = result.metric * result.low;
    result.control = smoothNumerator(slope, result.lighting) * (proposal / quadraticAt(result, slope)) ** 2;
  }
  return result;
}
export function orbitSample(plan: OrbitPlan, random: () => number, maxAttempts = 4) {
  let c = 0,
    s = 0,
    accepted = false,
    attempts = 0;
  const ratio = plan.low / plan.high;
  for (; attempts < maxAttempts; attempts++) {
    const tangent = Math.tan(Math.PI * (random() - 0.5)),
      t2 = tangent * tangent,
      z2 = ratio * t2;
    c = (1 - z2) / (1 + z2);
    s = (2 * Math.sqrt(ratio) * tangent) / (1 + z2);
    if (random() < (1 + z2) / (1 + t2)) {
      accepted = true;
      attempts++;
      break;
    }
  }
  if (!accepted) {
    const theta = tau * random();
    c = Math.cos(theta);
    s = Math.sin(theta);
  }
  const x = c * plan.axis[0] - s * plan.axis[1],
    y = s * plan.axis[0] + c * plan.axis[1];
  const slope = slopeAt(plan.orbit, x, y);
  const qProposal = plan.metric * (plan.low + 2 * plan.qRadius * (1 - c));
  const q = quadraticAt(plan, slope),
    missed = (1 - plan.acceptance) ** maxAttempts;
  const mixture = 1 - missed + missed * plan.normalization * qProposal * qProposal;
  const control = plan.control ?? 0;
  return {
    value:
      plan.normalization *
      (control + (smoothNumerator(slope, plan.lighting) * (qProposal / q) ** 2 - control) / mixture),
    attempts,
    fallback: !accepted,
    slope,
    densityRelativeToUniform: (1 - missed) / (plan.normalization * qProposal * qProposal) + missed,
  };
}
/** Möbius phase warp: a uniform angle maps to the normalized reciprocal-
 * quadratic (wrapped Cauchy) density. The transformed GGX pole is smooth.
 * A randomly shifted N-point lattice is unbiased; a fixed shift is quadrature.
 * N >= 2 integrates the analytic constant control exactly and stays positive. */
export function warpedOrbit(plan: OrbitPlan, count: number, shift = 0.5) {
  const ratio = plan.low / plan.high;
  let sum = 0;
  for (let i = 0; i < count; i++) {
    const psi = (tau * (i + shift)) / count,
      cp = Math.cos(psi),
      sp = Math.sin(psi);
    const denominator = 1 + cp + ratio * (1 - cp);
    const c = (1 + cp - ratio * (1 - cp)) / denominator;
    const s = (2 * Math.sqrt(ratio) * sp) / denominator;
    const axis: Pair = [c * plan.axis[0] - s * plan.axis[1], s * plan.axis[0] + c * plan.axis[1]];
    const slope = slopeAt(plan.orbit, ...axis),
      qProposal = (plan.metric * 2 * plan.low) / denominator;
    const weight = qProposal / quadraticAt(plan, slope),
      control = count === 1 ? (plan.control ?? 0) : 0;
    sum +=
      control +
      ((smoothNumerator(slope, plan.lighting) * weight * weight - control) * denominator) / (1 + ratio);
  }
  return (plan.normalization * sum) / count;
}
export function integrateOrbit(orbit: WaveOrbit, lighting: Lighting, steps = 16384) {
  let sum = 0;
  for (let i = 0; i < steps; i++) {
    const theta = (tau * (i + 0.5)) / steps;
    sum += directResponse(slopeAt(orbit, Math.cos(theta), Math.sin(theta)), lighting);
  }
  return sum / steps;
}
export function ringDensity(radius: number, alpha: number, slope: Pair) {
  const x = Math.hypot(...slope),
    low = alpha * alpha + (x - radius) ** 2,
    high = alpha * alpha + (x + radius) ** 2;
  return (alpha * alpha * (alpha * alpha + x * x + radius * radius)) / (Math.PI * (low * high) ** 1.5);
}
export function seededRandom(initial: number) {
  let state = initial >>> 0;
  return () => {
    state = (state + 0x9e3779b9) >>> 0;
    let x = Math.imul(state ^ (state >>> 16), 0x21f0aaad);
    x = Math.imul(x ^ (x >>> 15), 0x735a2d97);
    return ((x ^ (x >>> 15)) >>> 0) / 4294967296;
  };
}
export function orbitFixture(
  x: number,
  y: number,
  roughness = 0.08,
): { orbit: WaveOrbit; lighting: Lighting } {
  const v = normalized([-0.22 + 0.45 * x, 0.9, -0.3 + 0.3 * y]);
  // Both directions vary, moving a narrow highlight across the slope orbit.
  const l = normalized([0.12 + 0.4 * y, 0.9, 0.04 + 0.35 * x]);
  const orbit: WaveOrbit = { mean: [0, 0], a: [0.095, 0.022], b: [0.008, 0.115] };
  return { orbit, lighting: { view: v, light: l, roughness, f0: 0.02037 } };
}
