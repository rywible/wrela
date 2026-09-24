import type { IndirectLightingField, Vec3 } from "@wrela/model";

import {
  type IndirectGeometry,
  type IndirectLighting,
  indirectDot,
  indirectHitTransport,
  indirectNormalize,
  traceIndirectRay,
} from "./indirect-query";
import { indirectProbeVisible } from "./indirect-visibility";

export const INDIRECT_PROBE_FLOATS = 60;
export const indirectSH = ([x, y, z]: Vec3) => [
  0.2820947918,
  0.4886025119 * y,
  0.4886025119 * z,
  0.4886025119 * x,
  1.0925484306 * x * y,
  1.0925484306 * y * z,
  0.3153915653 * (3 * z * z - 1),
  1.0925484306 * x * z,
  0.5462742153 * (x * x - y * y),
];
const axes: Vec3[] = [
  [1, 0, 0],
  [-1, 0, 0],
  [0, 1, 0],
  [0, -1, 0],
  [0, 0, 1],
  [0, 0, -1],
];
export function indirectProbePosition(
  field: Pick<IndirectLightingField, "origin" | "spacing" | "dimensions"> & { data?: Float32Array },
  index: number,
): Vec3 {
  const [nx, ny] = field.dimensions,
    x = index % nx,
    y = Math.floor(index / nx) % ny,
    z = Math.floor(index / (nx * ny));
  return [x, y, z].map(
    (v, a) => field.origin[a] + v * field.spacing[a] + (field.data?.[index * 60 + [38, 39, 42][a]] ?? 0),
  ) as Vec3;
}
/** Bounded placement heuristic, never a visibility certificate. Move a probe
 * whose nearest axis hit is a back face just outside that surface. Exact
 * receiver visibility still guards all interpolation, including thin walls. */
export function relocateIndirectProbe(geometry: IndirectGeometry, position: Vec3, spacing: Vec3): Vec3 {
  const unit = Math.min(...spacing),
    limit = unit * 0.48,
    clearance = Math.max(0.002, unit * 0.025);
  let closest: ReturnType<typeof traceIndirectRay>,
    back = false;
  for (const direction of axes) {
    const hit = traceIndirectRay(geometry, position, direction, limit);
    if (hit && (!closest || hit.distance < closest.distance)) {
      closest = hit;
      back = indirectDot(geometry.triangles[hit.triangle].normal, direction) > 0;
    }
  }
  if (!closest || !back) return [0, 0, 0];
  const normal = geometry.triangles[closest.triangle].normal;
  const offset = closest.position.map((v, a) => v + normal[a] * clearance - position[a]) as Vec3;
  if (Math.hypot(...offset) > limit) return [0, 0, 0];
  const relocated = position.map((v, a) => v + offset[a]) as Vec3;
  for (const direction of axes) {
    const hit = traceIndirectRay(geometry, relocated, direction, clearance * 0.5);
    if (hit && indirectDot(geometry.triangles[hit.triangle].normal, direction) > 0) return [0, 0, 0];
  }
  return offset;
}
/** Uniform-sphere rays integrate radiance into diffuse SH. Visibility moments
 * follow the irradiance-field approach, with six coarse directional lobes.
 * They are a leak-reduction approximation, not certified visibility. */
export function* indirectProbeSteps(
  geometry: IndirectGeometry,
  position: Vec3,
  lighting: IndirectLighting,
  options: {
    samples?: number;
    skySamples?: number;
    maxDistance?: number;
    seed?: number;
    transfer?: Float32Array;
    reflections?: { data: Float32Array; transfer?: Float32Array };
    bounces?: 1 | 2 | 3;
  } = {},
): Generator<void, Float32Array> {
  const samples = options.samples ?? 128,
    skySamples = options.skySamples ?? 8,
    maxDistance = options.maxDistance ?? 1000,
    bounces = options.bounces ?? 1;
  if (
    !Number.isInteger(samples) ||
    samples < 16 ||
    samples > 4096 ||
    !Number.isInteger(skySamples) ||
    skySamples < 1 ||
    skySamples > 128 ||
    !Number.isFinite(maxDistance) ||
    maxDistance <= 0 ||
    ![1, 2, 3].includes(bounces)
  )
    throw Error("Invalid indirect probe sampling budget");
  if (options.transfer && options.transfer.length !== 360) throw Error("Invalid sky transfer allocation");
  if (
    options.reflections &&
    (options.reflections.data.length !== 36 ||
      (options.reflections.transfer && options.reflections.transfer.length !== 360))
  )
    throw Error("Invalid reflection transfer allocation");
  const transfer = options.transfer ? new Float64Array(360) : undefined;
  const reflected = options.reflections ? new Float64Array(36) : undefined;
  const reflectedTransfer = options.reflections?.transfer ? new Float64Array(360) : undefined;
  const sums = new Float64Array(60),
    moments = axes.map(() => [0, 0, 0]);
  const rotation = ((options.seed ?? 0) * 0.754877666) % 1;
  let backfaces = 0;
  for (let i = 0; i < samples; i++) {
    const y = 1 - (2 * (i + 0.5)) / samples,
      radius = Math.sqrt(1 - y * y),
      angle = 2 * Math.PI * ((i * 0.61803398875 + rotation) % 1);
    const direction: Vec3 = [radius * Math.cos(angle), y, radius * Math.sin(angle)];
    const hit = traceIndirectRay(geometry, position, direction);
    if (hit && indirectDot(geometry.triangles[hit.triangle].normal, direction) > 0) backfaces++;
    const incident = new Float64Array(40);
    const record =
      transfer || reflectedTransfer
        ? {
            sky: (ray: Vec3, weight: Vec3) => {
              indirectSH(ray).forEach((v, k) => {
                for (let c = 0; c < 3; c++) incident[k * 4 + c] += v * weight[c];
              });
            },
            sun: (weight: Vec3) => {
              for (let c = 0; c < 3; c++) incident[36 + c] += weight[c];
            },
          }
        : undefined;
    const radiance = hit
      ? indirectHitTransport(
          geometry,
          hit,
          lighting,
          skySamples,
          (i * 0.381966011 + rotation) % 1,
          bounces,
          record,
        )
      : lighting.skyRadiance;
    if (!hit && record) record.sky(direction, [1, 1, 1]);
    const basis = indirectSH(direction);
    if (reflected)
      for (let band = 0; band < 9; band++) {
        const weight = basis[band] * ((4 * Math.PI) / samples);
        if (hit) {
          for (let channel = 0; channel < 3; channel++)
            reflected[band * 4 + channel] += radiance[channel] * weight;
          if (reflectedTransfer)
            for (let input = 0; input < 10; input++)
              for (let channel = 0; channel < 3; channel++)
                reflectedTransfer[(band * 10 + input) * 4 + channel] +=
                  weight * incident[input * 4 + channel];
        } else reflected[band * 4 + 3] += weight;
      }
    for (let band = 0; band < 9; band++)
      for (let channel = 0; channel < 3; channel++)
        sums[band * 4 + channel] +=
          radiance[channel] *
          basis[band] *
          ((4 * Math.PI) / samples) *
          (band === 0 ? 1 : band < 4 ? 2 / 3 : 1 / 4);
    if (transfer) {
      for (let output = 0; output < 9; output++) {
        const weight =
          basis[output] * ((4 * Math.PI) / samples) * (output === 0 ? 1 : output < 4 ? 2 / 3 : 1 / 4);
        for (let input = 0; input < 10; input++)
          for (let channel = 0; channel < 3; channel++)
            transfer[(output * 10 + input) * 4 + channel] += weight * incident[input * 4 + channel];
      }
    }
    const distance = Math.min(hit?.distance ?? maxDistance, maxDistance);
    for (let axis = 0; axis < 6; axis++) {
      const weight = Math.max(0, indirectDot(direction, axes[axis])) ** 16;
      moments[axis][0] += weight * distance;
      moments[axis][1] += weight * distance * distance;
      moments[axis][2] += weight;
    }
    // One expensive ray includes its bounded secondary rays; yield frequently.
    if (i % 4 === 3) yield;
  }
  sums[3] = backfaces > samples * 0.8 ? 0 : 1;
  for (let axis = 0; axis < 6; axis++) {
    sums[36 + axis * 4] = moments[axis][0] / Math.max(moments[axis][2], 1e-12);
    sums[37 + axis * 4] = moments[axis][1] / Math.max(moments[axis][2], 1e-12);
  }
  if (options.transfer && transfer) options.transfer.set(transfer);
  if (options.reflections && reflected) options.reflections.data.set(reflected);
  if (options.reflections?.transfer && reflectedTransfer) options.reflections.transfer.set(reflectedTransfer);
  return Float32Array.from(sums);
}
export function compileIndirectProbe(
  geometry: IndirectGeometry,
  position: Vec3,
  lighting: IndirectLighting,
  options: Parameters<typeof indirectProbeSteps>[3] = {},
): Float32Array {
  const steps = indirectProbeSteps(geometry, position, lighting, options);
  let result = steps.next();
  while (!result.done) result = steps.next();
  return result.value;
}
/** Geometry-only blend from the production interpolation. Visibility and moments
 * are independent of source intensity/color; incomplete/outside probes cannot
 * contribute. Zero weights in a ready field represent valid darkness. */
export function indirectProbeWeights(
  field: IndirectLightingField,
  world: Vec3,
  normal: Vec3,
): { cell: Vec3; weights: number[] } | undefined {
  const q = world.map((v, i) => (v - field.origin[i]) / field.spacing[i]);
  if (q.some((v, i) => v < 0 || v > field.dimensions[i] - 1)) return;
  // A surface can coincide with a probe plane. Normal-facing weights would then
  // normalize numerical dust from the next plane differently in FP32/FP64.
  // Bias interpolation coordinates, while tracing visibility from the receiver.
  const biased = q.map((v, i) =>
    Math.max(
      0,
      Math.min(
        field.dimensions[i] - 1,
        v + (normal[i] * Math.min(...field.spacing) * 0.02) / field.spacing[i],
      ),
    ),
  );
  const base = biased.map((v, i) => Math.min(field.dimensions[i] - 2, Math.floor(v))),
    fract = biased.map((v, i) => v - base[i]);
  const weights = new Array<number>(8).fill(0);
  let total = 0;
  for (let corner = 0; corner < 8; corner++) {
    const cell = base.map((v, i) => v + ((corner >> i) & 1));
    const index = cell[0] + field.dimensions[0] * (cell[1] + field.dimensions[1] * cell[2]),
      offset = index * INDIRECT_PROBE_FLOATS;
    if (field.data[offset + 3] < 0.5) continue;
    const probe = indirectProbePosition(field, index),
      delta = world.map((v, i) => v - probe[i]) as Vec3;
    const distance = Math.hypot(...delta),
      direction = indirectNormalize(delta);
    let mean = 0,
      second = 0,
      directionalWeight = 0;
    for (let axis = 0; axis < 3; axis++) {
      const weight = Math.abs(direction[axis]) ** 8,
        lobe = axis * 2 + (direction[axis] < 0 ? 1 : 0);
      mean += field.data[offset + 36 + lobe * 4] * weight;
      second += field.data[offset + 37 + lobe * 4] * weight;
      directionalWeight += weight;
    }
    mean /= Math.max(directionalWeight, 1e-12);
    second /= Math.max(directionalWeight, 1e-12);
    const variance = Math.max(1e-6, second - mean * mean),
      excess = Math.max(0, distance - mean - Math.min(...field.spacing) * 0.04);
    const visibility = excess === 0 ? 1 : (variance / (variance + excess * excess)) ** 3;
    const facing = distance < 1e-5 ? 1 : Math.max(0, -indirectDot(direction, normal)) ** 2;
    let weight = visibility * facing;
    for (let axis = 0; axis < 3; axis++) weight *= (corner >> axis) & 1 ? fract[axis] : 1 - fract[axis];
    if (weight <= 1e-10 || !indirectProbeVisible(field, world, normal, probe)) continue;
    weights[corner] = weight;
    total += weight;
  }
  return { cell: base as Vec3, weights: weights.map((w) => (total > 1e-8 ? w / total : 0)) };
}

/** CPU mirror of the production diffuse gather. */
export function sampleIndirectField(
  field: IndirectLightingField,
  world: Vec3,
  normal: Vec3,
): [number, number, number, number] {
  const blend = indirectProbeWeights(field, world, normal);
  if (!blend) return [0, 0, 0, 0];
  const basis = indirectSH(normal),
    result: Vec3 = [0, 0, 0];
  for (let corner = 0; corner < 8; corner++) {
    const c = blend.cell.map((v, a) => v + ((corner >> a) & 1));
    const offset = (c[0] + field.dimensions[0] * (c[1] + field.dimensions[1] * c[2])) * 60;
    const irradiance: Vec3 = [0, 0, 0];
    for (let band = 0; band < 9; band++)
      for (let channel = 0; channel < 3; channel++)
        irradiance[channel] += field.data[offset + band * 4 + channel] * basis[band];
    for (let channel = 0; channel < 3; channel++)
      result[channel] += Math.max(0, irradiance[channel]) * blend.weights[corner];
  }
  return [...result, blend.weights.some((w) => w > 0) || field.completedProbes === field.totalProbes ? 1 : 0];
}
