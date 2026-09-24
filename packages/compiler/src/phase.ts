import { contentKey, type WaterDefinition, type WaterPhaseProduct, waterSchema } from "@wrela/model";

import { resolveWaterWaves } from "./water";
import { compileGlintMap } from "./water-glints";

const TAU = 2 * Math.PI;
export type PhasePair = [number, number];
export type PhaseVector = [number, number, number];
/** Full phase changes across each pixel axis and the shutter, not independent phase marginals. */
export interface PhaseFootprint {
  origin: number[];
  dx: number[];
  dy: number[];
  shutter: number[];
}
const finite = (xs: readonly number[]) => xs.every(Number.isFinite);
function checkFootprint(f: PhaseFootprint) {
  const n = f.origin.length;
  if (n > 8 || ![f.origin, f.dx, f.dy, f.shutter].every((xs) => xs.length === n && finite(xs)))
    throw new Error("Invalid phase footprint (at most eight finite, equally sized carriers)");
}
export function compileWaterPhases(definition: WaterDefinition): WaterPhaseProduct {
  const water = waterSchema.parse(definition);
  const waves = resolveWaterWaves(water);
  const product: WaterPhaseProduct = {
    sourceKey: contentKey({ algorithm: "water-phase-2", waves }),
    algorithmVersion: 2,
    coordinateFrame: "world-xz",
    carriers: waves.map((w, waveIndex) => {
      const k = TAU / w.wavelength;
      const spatial: PhasePair = [k * Math.cos(w.direction), k * Math.sin(w.direction)];
      return {
        waveIndex,
        spatial,
        temporal: -k * w.speed,
        phase: w.phase,
        slope: [w.amplitude * spatial[0], w.amplitude * spatial[1]],
        amplitude: w.amplitude,
      };
    }),
  };
  product.glints = compileGlintMap(product.carriers);
  return product;
}
export function waterPhaseFootprint(
  product: WaterPhaseProduct,
  position: PhasePair,
  time: number,
  dx: PhasePair,
  dy: PhasePair,
  shutterSeconds = 0,
): PhaseFootprint {
  if (!finite([...position, time, ...dx, ...dy, shutterSeconds]) || shutterSeconds < 0)
    throw new Error("Invalid water footprint coordinates or shutter");
  const f: PhaseFootprint = { origin: [], dx: [], dy: [], shutter: [] };
  for (const w of product.carriers) {
    f.origin.push(w.spatial[0] * position[0] + w.spatial[1] * position[1] + w.temporal * time + w.phase);
    f.dx.push(w.spatial[0] * dx[0] + w.spatial[1] * dx[1]);
    f.dy.push(w.spatial[0] * dy[0] + w.spatial[1] * dy[1]);
    f.shutter.push(w.temporal * shutterSeconds);
  }
  checkFootprint(f);
  return f;
}
export function phaseSinc(x: number): number {
  if (!Number.isFinite(x)) throw new Error("Nonfinite sinc argument");
  return Math.abs(x) < 1e-4 ? 1 - (x * x) / 6 + x ** 4 / 120 : Math.sin(x) / x;
}
/** Exact in real arithmetic for the locally affine pixel/shutter box. */
export function phaseFootprintFactor(mode: readonly number[], f: PhaseFootprint): number {
  checkFootprint(f);
  if (mode.length !== f.origin.length || !mode.every(Number.isSafeInteger))
    throw new Error("Invalid phase mode");
  const dot = (xs: number[]) => mode.reduce((sum, k, i) => sum + k * xs[i], 0);
  return phaseSinc(dot(f.dx) / 2) * phaseSinc(dot(f.dy) / 2) * phaseSinc(dot(f.shutter) / 2);
}
export interface PhaseTerm {
  mode: number[];
  real: number;
  imaginary: number;
}
export interface PhasePolynomial {
  constant: number;
  /** One representative of each conjugate pair; the evaluator includes its conjugate. */
  terms: PhaseTerm[];
  sourceKey: string;
  parameterKey: string;
  sourceFit:
    | { kind: "unknown"; reason: string }
    | { kind: "measured"; rms: number; maximum: number; reference: string };
}
export function evaluatePhasePolynomial(polynomial: PhasePolynomial, f: PhaseFootprint, maximumTerms = 4096) {
  checkFootprint(f);
  if (
    !Number.isSafeInteger(maximumTerms) ||
    maximumTerms < 0 ||
    maximumTerms > 4096 ||
    polynomial.terms.length > 4096
  )
    throw new Error("Phase program allocation limit exceeded");
  if (!Number.isFinite(polynomial.constant)) throw new Error("Nonfinite phase coefficient");
  const terms = polynomial.terms
    .map((term) => {
      if (!finite([term.real, term.imaginary])) throw new Error("Nonfinite phase coefficient");
      const factor = phaseFootprintFactor(term.mode, f);
      return { term, factor, bound: 2 * Math.hypot(term.real, term.imaginary) * Math.abs(factor) };
    })
    .sort((a, b) => b.bound - a.bound);
  let value = polynomial.constant;
  let omittedPolynomialBound = 0;
  for (let i = 0; i < terms.length; i++) {
    const { term, factor, bound } = terms[i];
    if (i >= maximumTerms) {
      omittedPolynomialBound += bound;
      continue;
    }
    const phase = term.mode.reduce((sum, k, j) => sum + k * f.origin[j], 0);
    value += 2 * factor * (term.real * Math.cos(phase) - term.imaginary * Math.sin(phase));
  }
  return { value, omittedPolynomialBound, sourceFit: polynomial.sourceFit };
}
/** Correlated quadrature of the complete callback response. Does not multiply separate averages. */
export function integratePhaseBox(
  f: PhaseFootprint,
  response: (phases: number[]) => number,
  nodesPerAxis = 4,
): number {
  checkFootprint(f);
  if (!Number.isSafeInteger(nodesPerAxis) || nodesPerAxis < 1 || nodesPerAxis > 32)
    throw new Error("Phase quadrature requires 1–32 nodes per active axis");
  const axes = [f.dx, f.dy, f.shutter];
  const counts = axes.map((axis) => (axis.some((x) => x !== 0) ? nodesPerAxis : 1));
  let sum = 0;
  for (let x = 0; x < counts[0]; x++)
    for (let y = 0; y < counts[1]; y++)
      for (let t = 0; t < counts[2]; t++) {
        const offsets = [x, y, t].map((index, axis) => (index + 0.5) / counts[axis] - 0.5);
        const phases = f.origin.map(
          (origin, i) => origin + axes.reduce((s, axis, j) => s + axis[i] * offsets[j], 0),
        );
        const value = response(phases);
        if (!Number.isFinite(value)) throw new Error("Nonfinite phase response");
        sum += value;
      }
  return sum / (counts[0] * counts[1] * counts[2]);
}
export interface CoherentPhaseGroup {
  rate: number;
  indices: number[];
}
/** Exact equal-rate groups only. Nearly equal rates keep their authored beat frequency. */
export function coherentPhaseGroups(rates: readonly number[]): CoherentPhaseGroup[] {
  if (rates.length > 8 || !finite(rates)) throw new Error("Invalid coherent phase rates");
  const groups = new Map<number, number[]>();
  rates.forEach((rate, index) => {
    const indices = groups.get(rate) ?? [];
    indices.push(index);
    groups.set(rate, indices);
  });
  return [...groups].map(([rate, indices]) => ({ rate, indices }));
}
export interface PhaseOrbit {
  mean: PhasePair;
  a: PhasePair;
  b: PhasePair;
}
export function coherentSlopeOrbit(
  slopes: PhasePair[],
  offsets: number[],
  mean: PhasePair = [0, 0],
): PhaseOrbit {
  if (
    slopes.length !== offsets.length ||
    slopes.length > 8 ||
    !finite([...slopes.flat(), ...offsets, ...mean])
  )
    throw new Error("Invalid coherent slope orbit");
  const a: PhasePair = [0, 0],
    b: PhasePair = [0, 0];
  slopes.forEach((s, i) => {
    for (let j = 0; j < 2; j++) {
      a[j] += s[j] * Math.cos(offsets[i]);
      b[j] -= s[j] * Math.sin(offsets[i]);
    }
  });
  return { mean: [...mean], a, b };
}
export function phaseOrbitSlope(orbit: PhaseOrbit, angle: number): PhasePair {
  return [0, 1].map(
    (i) => orbit.mean[i] + orbit.a[i] * Math.cos(angle) + orbit.b[i] * Math.sin(angle),
  ) as PhasePair;
}
export function splitPhaseInterval(length: number) {
  if (!Number.isFinite(length) || length < 0 || length > TAU * 1e9) throw new Error("Invalid phase interval");
  const periods = Math.floor(length / TAU);
  const remainder = length - periods * TAU;
  return {
    periods,
    remainder,
    periodWeight: length === 0 ? 0 : (periods * TAU) / length,
    remainderWeight: length === 0 ? 0 : remainder / length,
  };
}
/** Full periods and the actual trailing interval retain their physical lengths. */
export function integratePhaseInterval(
  start: number,
  length: number,
  response: (phase: number) => number,
  nodes = 256,
  completePeriod?: () => number,
) {
  if (!Number.isFinite(start) || !Number.isSafeInteger(nodes) || nodes < 1 || nodes > 65536)
    throw new Error("Invalid phase interval integration");
  const split = splitPhaseInterval(length);
  const quadrature = (origin: number, width: number) => {
    let sum = 0;
    for (let i = 0; i < nodes; i++) sum += response(origin + (width * (i + 0.5)) / nodes);
    return sum / nodes;
  };
  if (length === 0) return response(start);
  const full = split.periods > 0 ? (completePeriod?.() ?? quadrature(0, TAU)) : 0;
  // Removing integer periods here avoids large arguments while retaining the authored phase origin.
  const tail = split.remainder > 0 ? quadrature(start % TAU, split.remainder) : 0;
  const result = split.periodWeight * full + split.remainderWeight * tail;
  if (!Number.isFinite(result)) throw new Error("Nonfinite phase response");
  return result;
}

export interface PhaseLighting {
  view: PhaseVector;
  light: PhaseVector;
  roughness: number;
  f0: number;
}
export interface GGXWarpPlan {
  orbit: PhaseOrbit;
  lighting: PhaseLighting;
  matrix: [number, number, number];
  center: PhasePair;
  delta: number;
  metric: number;
  axis: PhasePair;
  low: number;
  high: number;
  normalization: number;
  condition: number;
  model: "balanced" | "stationary-pole";
}
const unit = (v: PhaseVector): PhaseVector => {
  const length = Math.hypot(...v);
  if (!finite(v) || length < 1e-12) throw new Error("Invalid lighting direction");
  return v.map((x) => x / length) as PhaseVector;
};
const dot3 = (a: PhaseVector, b: PhaseVector) => a.reduce((sum, x, i) => sum + x * b[i], 0);
function validLighting(lighting: PhaseLighting): PhaseLighting {
  if (
    !finite([lighting.roughness, lighting.f0]) ||
    lighting.roughness < 0.02 ||
    lighting.roughness > 1 ||
    lighting.f0 < 0 ||
    lighting.f0 > 1
  )
    throw new Error("Invalid GGX parameters");
  const view = unit(lighting.view),
    light = unit(lighting.light);
  unit(view.map((x, i) => x + light[i]) as PhaseVector);
  return { ...lighting, view, light };
}
/** Conventional normalized-vector GGX is retained as the ordinary integration control. */
export function phaseGGXResponse(slope: PhasePair, parameters: PhaseLighting): number {
  const lighting = validLighting(parameters);
  const n = unit([-slope[0], 1, -slope[1]]);
  const h = unit(lighting.view.map((x, i) => x + lighting.light[i]) as PhaseVector);
  const nv = dot3(n, lighting.view),
    nl = dot3(n, lighting.light);
  if (nv <= 0 || nl <= 0) return 0;
  const nh = Math.max(0, dot3(n, h)),
    vh = Math.max(0, dot3(lighting.view, h));
  const alpha2 = lighting.roughness ** 4;
  const denominator = 1 + nh * nh * (alpha2 - 1);
  const lambda = (c: number) => (Math.sqrt(1 + (alpha2 * Math.max(0, 1 - c * c)) / (c * c)) - 1) / 2;
  const fresnel = lighting.f0 + (1 - lighting.f0) * (1 - vh) ** 5;
  return (alpha2 * fresnel) / (Math.PI * denominator ** 2 * 4 * nv * (1 + lambda(nv) + lambda(nl)));
}
/** Balanced positive denominator model. Its condition is a denominator-ratio fact, not a radiance error bound. */
export function prepareGGXWarp(orbit: PhaseOrbit, parameters: PhaseLighting): GGXWarpPlan | null {
  const lighting = validLighting(parameters);
  if (!finite([...orbit.mean, ...orbit.a, ...orbit.b])) throw new Error("Nonfinite orbit");
  const { a, b } = orbit;
  const determinant = a[0] * b[1] - a[1] * b[0];
  const scale = Math.hypot(...a) * Math.hypot(...b);
  if (scale < 1e-12 || Math.abs(determinant) <= 1e-6 * scale) return null;
  const h = unit(lighting.view.map((x, i) => x + lighting.light[i]) as PhaseVector);
  const alpha2 = lighting.roughness ** 4,
    beta = 1 - alpha2;
  const detA = h[1] ** 2 + alpha2 * (h[0] ** 2 + h[2] ** 2);
  const matrix: [number, number, number] = [1 - beta * h[0] ** 2, -beta * h[0] * h[2], 1 - beta * h[2] ** 2];
  const center: PhasePair = [(-beta * h[1] * h[0]) / detA, (-beta * h[1] * h[2]) / detA];
  const metric = Math.abs(determinant) * Math.sqrt(detA),
    delta = alpha2 / detA;
  const tx = center[0] - orbit.mean[0],
    ty = center[1] - orbit.mean[1];
  const x: PhasePair = [(b[1] * tx - b[0] * ty) / determinant, (-a[1] * tx + a[0] * ty) / determinant];
  const radius = Math.hypot(...x),
    gamma2 = delta / metric;
  const low = gamma2 + (radius - 1) ** 2,
    high = gamma2 + (radius + 1) ** 2;
  const quadratic = (v: PhasePair) =>
    matrix[0] * v[0] ** 2 + 2 * matrix[1] * v[0] * v[1] + matrix[2] * v[1] ** 2;
  const trace = quadratic(a) + quadratic(b);
  const largest = (trace + Math.sqrt(Math.max(0, trace * trace - 4 * metric * metric))) / 2;
  const plan: GGXWarpPlan = {
    orbit,
    lighting,
    matrix,
    center,
    delta,
    metric,
    axis: radius > 0 ? [x[0] / radius, x[1] / radius] : [1, 0],
    low,
    high,
    normalization: (low + high) / (2 * metric * metric * (low * high) ** 1.5),
    condition: largest / metric,
    model: "balanced",
  };
  if (!finite([low, high, plan.normalization, plan.condition])) return null;
  return refineGGXWarp(plan, radius);
}
/** Five bounded stationary updates; a failed local fit retains the balanced model. */
function refineGGXWarp(plan: GGXWarpPlan, radius: number): GGXWarpPlan {
  if (radius <= 0.55 || radius >= 1.6) return plan;
  let axis: PhasePair = [...plan.axis];
  const matrix = plan.matrix;
  const bilinear = (a: PhasePair, b: PhasePair) =>
    matrix[0] * a[0] * b[0] + matrix[1] * (a[0] * b[1] + a[1] * b[0]) + matrix[2] * a[1] * b[1];
  let curvature = 0,
    gradient = 0;
  for (let i = 0; i <= 5; i++) {
    const slope: PhasePair = [0, 1].map(
      (j) => plan.orbit.mean[j] + plan.orbit.a[j] * axis[0] + plan.orbit.b[j] * axis[1],
    ) as PhasePair;
    const offset: PhasePair = [slope[0] - plan.center[0], slope[1] - plan.center[1]];
    const tangent: PhasePair = [0, 1].map(
      (j) => -plan.orbit.a[j] * axis[1] + plan.orbit.b[j] * axis[0],
    ) as PhasePair;
    const second: PhasePair = [plan.orbit.mean[0] - slope[0], plan.orbit.mean[1] - slope[1]];
    gradient = 2 * bilinear(tangent, offset);
    curvature = 2 * (bilinear(tangent, tangent) + bilinear(second, offset));
    if (i === 5 || curvature <= 0) break;
    const step = Math.max(-0.5, Math.min(0.5, -gradient / curvature));
    const inverseLength = 1 / Math.sqrt(1 + step * step);
    axis = [(axis[0] - step * axis[1]) * inverseLength, (axis[1] + step * axis[0]) * inverseLength];
  }
  if (!(curvature > 0) || Math.abs(gradient) >= 1e-5 * Math.max(curvature, 1e-8)) return plan;
  const slope: PhasePair = [0, 1].map(
    (j) => plan.orbit.mean[j] + plan.orbit.a[j] * axis[0] + plan.orbit.b[j] * axis[1],
  ) as PhasePair;
  const low = warpQuadratic(plan, slope),
    high = low + 2 * curvature;
  const normalization = (low + high) / (2 * (low * high) ** 1.5);
  if (!(low > 0) || !finite([low, high, normalization])) return plan;
  return { ...plan, axis, low, high, metric: 1, normalization, model: "stationary-pole" };
}
function warpQuadratic(plan: GGXWarpPlan, slope: PhasePair) {
  const x = slope[0] - plan.center[0],
    z = slope[1] - plan.center[1];
  return plan.delta + plan.matrix[0] * x * x + 2 * plan.matrix[1] * x * z + plan.matrix[2] * z * z;
}
function warpNumerator(slope: PhasePair, lighting: PhaseLighting) {
  const norm2 = 1 + slope[0] ** 2 + slope[1] ** 2;
  const n: PhaseVector = [-slope[0] / Math.sqrt(norm2), 1 / Math.sqrt(norm2), -slope[1] / Math.sqrt(norm2)];
  const nv = dot3(n, lighting.view),
    nl = dot3(n, lighting.light);
  if (nv <= 0 || nl <= 0) return 0;
  const h = unit(lighting.view.map((x, i) => x + lighting.light[i]) as PhaseVector);
  const alpha2 = lighting.roughness ** 4;
  const lambda = (c: number) => (Math.sqrt(1 + (alpha2 * Math.max(0, 1 - c * c)) / (c * c)) - 1) / 2;
  const fresnel = lighting.f0 + (1 - lighting.f0) * (1 - Math.max(0, dot3(lighting.view, h))) ** 5;
  return (alpha2 * norm2 * norm2 * fresnel) / (4 * Math.PI * nv * (1 + lambda(nv) + lambda(nl)));
}
/** A fixed shift is biased quadrature; random uniform shifts give unbiased expectation in real arithmetic. */
export function evaluateGGXWarp(plan: GGXWarpPlan, nodes: 4 | 8, shift = 0.5): number {
  if ((nodes !== 4 && nodes !== 8) || !Number.isFinite(shift) || shift < 0 || shift >= 1)
    throw new Error("Invalid phase warp lattice");
  const ratio = plan.low / plan.high;
  let sum = 0;
  for (let i = 0; i < nodes; i++) {
    const psi = (TAU * (i + shift)) / nodes,
      cp = Math.cos(psi),
      sp = Math.sin(psi);
    const denominator = 1 + cp + ratio * (1 - cp);
    const c = (1 + cp - ratio * (1 - cp)) / denominator,
      s = (2 * Math.sqrt(ratio) * sp) / denominator;
    const cosine = c * plan.axis[0] - s * plan.axis[1],
      sine = s * plan.axis[0] + c * plan.axis[1];
    const slope: PhasePair = [0, 1].map(
      (j) => plan.orbit.mean[j] + plan.orbit.a[j] * cosine + plan.orbit.b[j] * sine,
    ) as PhasePair;
    const proposal = (plan.metric * 2 * plan.low) / denominator;
    sum +=
      (warpNumerator(slope, plan.lighting) * (proposal / warpQuadratic(plan, slope)) ** 2 * denominator) /
      (1 + ratio);
  }
  const result = (plan.normalization * sum) / nodes;
  if (!Number.isFinite(result)) throw new Error("Nonfinite phase warp response");
  return result;
}
export interface WarpDomainEvidence {
  kind: "measured";
  sourceKey: string;
  parameterKey: string;
  reference: string;
  nodes: 4 | 8;
  maximumCondition: number;
  roughness: [number, number];
  rms: number;
  maximum: number;
}
export type WaterResponseRejection =
  | "no-complete-period"
  | "different-carrier-rates"
  | "degenerate-orbit"
  | "ill-conditioned-orbit"
  | "broad-lobe"
  | "unvalidated-domain";
/** Conditional one-axis integral. Other pixel/shutter axes must remain in an outer correlated integral. */
export function integrateCoherentGGX(options: {
  orbit: PhaseOrbit;
  lighting: PhaseLighting;
  start: number;
  length: number;
  rates: number[];
  sourceKey: string;
  parameterKey: string;
  nodes: 4 | 8;
  evidence?: WarpDomainEvidence;
  maximumRms: number;
  maximumError: number;
  /** Required for unequal carriers: their response is not a periodic ellipse. */
  regularResponse?: (phase: number) => number;
  regularNodes?: number;
  shift?: number;
}): { value: number; selected: "regular" | "warped-4" | "warped-8"; rejection?: WaterResponseRejection } {
  const { orbit, lighting, evidence } = options;
  const period = splitPhaseInterval(options.length);
  const groups = coherentPhaseGroups(options.rates);
  const plan = prepareGGXWarp(orbit, lighting);
  let rejection: WaterResponseRejection | undefined;
  if (groups.length !== 1 || groups[0].rate === 0) rejection = "different-carrier-rates";
  else if (!period.periods) rejection = "no-complete-period";
  else if (!plan) rejection = "degenerate-orbit";
  else if (plan.condition > Math.min(evidence?.maximumCondition ?? 8, 8)) rejection = "ill-conditioned-orbit";
  else if (lighting.roughness > 0.12) rejection = "broad-lobe";
  else if (
    !evidence ||
    evidence.sourceKey !== options.sourceKey ||
    evidence.parameterKey !== options.parameterKey ||
    !evidence.reference ||
    evidence.nodes !== options.nodes ||
    !finite([
      evidence.rms,
      evidence.maximum,
      evidence.maximumCondition,
      ...evidence.roughness,
      options.maximumRms,
      options.maximumError,
    ]) ||
    evidence.rms < 0 ||
    evidence.maximum < evidence.rms ||
    evidence.rms > options.maximumRms ||
    evidence.maximum > options.maximumError ||
    lighting.roughness < evidence.roughness[0] ||
    lighting.roughness > evidence.roughness[1]
  )
    rejection = "unvalidated-domain";
  if (rejection === "different-carrier-rates") {
    if (!options.regularResponse)
      throw new Error("Unequal carriers require their original response for regular fallback");
    const nodes = options.regularNodes ?? 256;
    if (!Number.isSafeInteger(nodes) || nodes < 1 || nodes > 65536 || !Number.isFinite(options.start))
      throw new Error("Invalid regular quadrature");
    let value = 0;
    for (let i = 0; i < nodes; i++)
      value += options.regularResponse(options.start + (options.length * (i + 0.5)) / nodes);
    value /= nodes;
    if (!Number.isFinite(value)) throw new Error("Nonfinite phase response");
    return { value, selected: "regular", rejection };
  }
  const value = integratePhaseInterval(
    options.start,
    options.length,
    (angle) => phaseGGXResponse(phaseOrbitSlope(orbit, angle), lighting),
    options.regularNodes ?? 256,
    !rejection && plan ? () => evaluateGGXWarp(plan, options.nodes, options.shift) : undefined,
  );
  return {
    value,
    selected: rejection ? "regular" : options.nodes === 4 ? "warped-4" : "warped-8",
    ...(rejection ? { rejection } : {}),
  };
}

/** Bounded tensor fit of a complete response. DFT aliasing is explicitly unknown source-fit error;
 * a separately sampled validation set must supply evidence before this program is selectable. */
export function fitPhasePolynomial(options: {
  dimensions: number;
  nodesPerDimension: number;
  modes: number[][];
  sourceKey: string;
  parameterKey: string;
  response: (phases: number[]) => number;
}): { polynomial: PhasePolynomial; evaluations: number; compileMilliseconds: number } {
  const start = performance.now();
  const { dimensions, nodesPerDimension: nodes, modes } = options;
  const count = nodes ** dimensions;
  if (
    !Number.isSafeInteger(dimensions) ||
    dimensions < 1 ||
    dimensions > 8 ||
    !Number.isSafeInteger(nodes) ||
    nodes < 2 ||
    !Number.isSafeInteger(count) ||
    count > 65536 ||
    modes.length > 4096 ||
    count * Math.max(1, modes.length) > 4194304
  )
    throw new Error("Phase fit allocation/work limit exceeded");
  const seen = new Set<string>();
  for (const mode of modes) {
    const first = mode.find((k) => k !== 0);
    if (
      mode.length !== dimensions ||
      !mode.every((k) => Number.isSafeInteger(k) && Math.abs(k) * 2 < nodes) ||
      !first ||
      first < 0 ||
      seen.has(mode.join(","))
    )
      throw new Error("Phase fit needs unique, nonzero conjugate representatives below Nyquist");
    seen.add(mode.join(","));
  }
  const terms = modes.map((mode) => ({ mode: [...mode], real: 0, imaginary: 0 }));
  let constant = 0;
  for (let index = 0; index < count; index++) {
    let remainder = index;
    const phases = Array.from({ length: dimensions }, () => {
      const digit = remainder % nodes;
      remainder = Math.floor(remainder / nodes);
      return (TAU * (digit + 0.5)) / nodes;
    });
    const response = options.response(phases);
    if (!Number.isFinite(response)) throw new Error("Nonfinite phase fit sample");
    constant += response / count;
    for (const term of terms) {
      const angle = term.mode.reduce((sum, k, i) => sum + k * phases[i], 0);
      term.real += (response * Math.cos(angle)) / count;
      term.imaginary -= (response * Math.sin(angle)) / count;
    }
  }
  return {
    polynomial: {
      constant,
      terms,
      sourceKey: options.sourceKey,
      parameterKey: options.parameterKey,
      sourceFit: {
        kind: "unknown",
        reason: "Finite DFT fit requires independent source-response validation",
      },
    },
    evaluations: count,
    compileMilliseconds: performance.now() - start,
  };
}
export function selectPhasePolynomial(
  polynomial: PhasePolynomial,
  f: PhaseFootprint,
  sourceKey: string,
  parameterKey: string,
  maximumError: number,
  maximumTerms = 4096,
) {
  if (!Number.isFinite(maximumError) || maximumError < 0) throw new Error("Invalid phase error budget");
  if (polynomial.sourceKey !== sourceKey || polynomial.parameterKey !== parameterKey)
    return { selected: false as const, rejection: "stale-parameters" as const };
  if (
    polynomial.sourceFit.kind !== "measured" ||
    !polynomial.sourceFit.reference ||
    !finite([polynomial.sourceFit.rms, polynomial.sourceFit.maximum]) ||
    polynomial.sourceFit.rms < 0 ||
    polynomial.sourceFit.maximum < polynomial.sourceFit.rms
  )
    return { selected: false as const, rejection: "unvalidated-source-fit" as const };
  const result = evaluatePhasePolynomial(polynomial, f, maximumTerms);
  if (polynomial.sourceFit.maximum + result.omittedPolynomialBound > maximumError)
    return { selected: false as const, rejection: "error-budget" as const };
  return {
    selected: true as const,
    ...result,
    evidenceKind: "measured-source-fit-and-real-polynomial-bound" as const,
  };
}

/** Finite-interval version of the same Möbius map. A partial period is never
 * assigned the complete-period mean, even when the highlight crosses its edge. */
export function evaluateGGXWarpInterval(plan: GGXWarpPlan, start: number, width: number, nodes = 32): number {
  if (
    !Number.isFinite(start) ||
    !Number.isFinite(width) ||
    width < 0 ||
    width > TAU ||
    !Number.isSafeInteger(nodes) ||
    nodes < 1 ||
    nodes > 65536
  )
    throw new Error("Invalid finite warped phase interval");
  if (width === 0) return phaseGGXResponse(phaseOrbitSlope(plan.orbit, start), plan.lighting);
  const relative = start - Math.atan2(plan.axis[1], plan.axis[0]);
  const theta = Math.atan2(Math.sin(relative), Math.cos(relative));
  const ratio = plan.low / plan.high;
  const inverse = (angle: number) =>
    2 * Math.atan2(Math.sin(angle / 2), Math.sqrt(ratio) * Math.cos(angle / 2));
  const begin = inverse(theta);
  let end = inverse(theta + width);
  let span = end - begin;
  span -= Math.floor(span / TAU) * TAU;
  if (width === TAU) span = TAU;
  end = begin + span;
  let sum = 0;
  for (let i = 0; i < nodes; i++) {
    const psi = begin + ((end - begin) * (i + 0.5)) / nodes;
    const cp = Math.cos(psi),
      sp = Math.sin(psi);
    const denominator = 1 + cp + ratio * (1 - cp);
    const c = (1 + cp - ratio * (1 - cp)) / denominator,
      s = (2 * Math.sqrt(ratio) * sp) / denominator;
    const cosine = c * plan.axis[0] - s * plan.axis[1],
      sine = s * plan.axis[0] + c * plan.axis[1];
    const slope: PhasePair = [0, 1].map(
      (j) => plan.orbit.mean[j] + plan.orbit.a[j] * cosine + plan.orbit.b[j] * sine,
    ) as PhasePair;
    const proposal = (plan.metric * 2 * plan.low) / denominator;
    sum +=
      (warpNumerator(slope, plan.lighting) * (proposal / warpQuadratic(plan, slope)) ** 2 * denominator) /
      (1 + ratio);
  }
  return (((plan.normalization * sum) / nodes) * (end - begin)) / width;
}

/** Query-local GGX denominator scale over a slope cone. This is a feature-width
 * fact used by adaptive quadrature, not a complete radiance error certificate. */
export function phaseGGXFeatureWidth(slope: PhasePair, parameters: PhaseLighting, cone: number): number {
  if (!Number.isFinite(cone) || cone < 0) throw new Error("Invalid slope cone");
  const lighting = validLighting(parameters),
    n = unit([-slope[0], 1, -slope[1]]);
  const h = unit(lighting.view.map((x, i) => x + lighting.light[i]) as PhaseVector);
  const minimumAngle = Math.max(0, Math.acos(Math.max(-1, Math.min(1, dot3(n, h)))) - cone);
  const alpha2 = lighting.roughness ** 4;
  const denominatorWidth =
    dot3(n, lighting.light) + cone <= 0
      ? 1
      : Math.min(
          Math.sqrt(alpha2 + (1 - alpha2) * Math.sin(Math.min(minimumAngle, Math.PI / 2)) ** 2),
          Math.max(0.05, dot3(n, lighting.light) - cone),
        );
  let width = Math.min(denominatorWidth, Math.max(0.05, dot3(n, lighting.view) - cone));
  const nv = dot3(n, lighting.view);
  const reflectedY = -lighting.view[1] + 2 * nv * n[1];
  if (Math.abs(reflectedY) <= 0.05 + 2 * cone) width = Math.min(width, 0.025);
  return Math.max(width, lighting.roughness ** 2 * 0.01);
}
