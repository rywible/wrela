import type { RadianceLightingProduct } from "@wrela/compiler";
import { indirectSH } from "@wrela/compiler/indirect-probes";
import { inverseMatrix, normalize, type RenderSurface, type Vec3 } from "@wrela/model";

type Channels = { sky: Vec3; local: Vec3; emission: Vec3 };
type Light = { color: Vec3; intensity: number };
const zero = (): Channels => ({ sky: [0, 0, 0], local: [0, 0, 0], emission: [0, 0, 0] });
const dot = (a: Vec3, b: Vec3) => a.reduce((sum, v, i) => sum + v * b[i], 0);
const sub = (a: Vec3, b: Vec3) => a.map((v, i) => v - b[i]) as Vec3;

/** CPU diagnostic of the actual compiled carrier, including its vertex
 * interpolation and local emission. Evaluates channels separately under the
 * same isolated inputs as lighting-reference-check; this is not GPU readback. */
export function nativeRadianceReference(
  product: RadianceLightingProduct,
  surfaces: readonly RenderSurface[],
  point: { position: Vec3; normal: Vec3 },
  lights: readonly Light[],
) {
  const field = product.field;
  const accumulate = (out: Channels, transfer: Float32Array, at: number, weight: number) => {
    for (let c = 0; c < 3; c++) {
      out.sky[c] += (transfer[at + c] + transfer[at + 36 + c]) / 0.2820947918 * weight;
      out.emission[c] += transfer[at + 104 + c] * weight;
      for (let l = 0; l < lights.length; l++)
        out.local[c] += transfer[at + (18 + l) * 4 + c] * lights[l].color[c] * lights[l].intensity * weight;
    }
  };
  const vertex = (id: number, other: number, normal: Vec3) => {
    const value = zero();
    const mixture = Math.floor(id) - 1024;
    const cache = field.surfaceDiffuse;
    if (mixture >= 0 && other >= 1 && cache) {
      const at = (other - 1) * 12;
      for (let c = 0; c < 3; c++) value.emission[c] = cache.receivers[at + 8 + c];
      for (let j = 0; j < 3; j++) {
        const sample = zero();
        accumulate(sample, cache.transfer, cache.receivers[at + j] * 108, 1);
        for (const channel of ["sky", "local"] as const)
          for (let c = 0; c < 3; c++) value[channel][c] += Math.max(0, sample[channel][c]) * cache.receivers[at + 4 + j];
      }
      return { value, coverage: 1, surface: 1 };
    }
    const fraction = other % 1;
    const pairWeight = fraction >= 0.125 ? (fraction - 0.125) * 4 : 0.5;
    const ids = mixture >= 0 ? field.receivers?.subarray(mixture * 8, mixture * 8 + 4) : [Math.floor(id), Math.floor(other)];
    const weights = mixture >= 0 ? field.receivers?.subarray(mixture * 8 + 4, mixture * 8 + 8) : [pairWeight, 1 - pairWeight];
    const basis = indirectSH(normal);
    let total = 0;
    const local = mixture >= 0 && field.receiverEmission;
    for (let j = 0; j < (ids?.length ?? 0); j++) {
      const probe = (ids?.[j] ?? 0) - 1, weight = weights?.[j] ?? 0;
      if (probe < 0 || !weight) continue;
      const sample = zero();
      for (let k = 0; k < 9; k++) {
        const angular = basis[k] * (k === 0 ? 1 : k < 4 ? 2 / 3 : 1 / 4);
        accumulate(sample, field.transfer, (probe * 9 + k) * 108, angular);
        if (local && field.directEmission) for (let c = 0; c < 3; c++)
          sample.emission[c] -= field.directEmission[(probe * 9 + k) * 3 + c] * angular;
      }
      for (const channel of ["sky", "local", "emission"] as const)
        for (let c = 0; c < 3; c++) value[channel][c] += Math.max(0, sample[channel][c]) * weight;
      total += weight;
    }
    if (total) for (const channel of ["sky", "local", "emission"] as const)
      for (let c = 0; c < 3; c++) value[channel][c] /= total;
    if (total && local) for (let c = 0; c < 3; c++) value.emission[c] += local[mixture * 3 + c];
    return { value, coverage: total ? 1 : 0, surface: 0 };
  };
  const target = point.position.map((v, a) => v - point.normal[a] * 0.003) as Vec3;
  const matches = [];
  for (const source of surfaces) {
    const mesh = product.meshes.get(source.id), m = source.matrix, inv = inverseMatrix(m);
    if (!mesh?.radianceProbes || !inv) continue;
    const local = [0, 1, 2].map((a) => inv[a] * target[0] + inv[4 + a] * target[1] + inv[8 + a] * target[2] + inv[12 + a]) as Vec3;
    const position = (id: number) => Array.from(mesh.positions.subarray(id * 3, id * 3 + 3)) as Vec3;
    const refined = product.receivers.get(source.id)?.sources;
    const start = refined ? 0 : source.drawRange?.start ?? 0;
    const end = start + (refined ? mesh.indices.length : source.drawRange?.count ?? mesh.indices.length);
    for (let at = start; at < end; at += 3) {
      const ids = Array.from(mesh.indices.subarray(at, at + 3));
      const a = position(ids[0]), u = sub(position(ids[1]), a), v = sub(position(ids[2]), a), p = sub(local, a);
      const uu = dot(u, u), uv = dot(u, v), vv = dot(v, v), pu = dot(p, u), pv = dot(p, v);
      const denominator = uu * vv - uv * uv;
      if (denominator < 1e-18) continue;
      const x = (pu * vv - pv * uv) / denominator, y = (pv * uu - pu * uv) / denominator;
      if (x < -1e-6 || y < -1e-6 || x + y > 1 + 1e-6) continue;
      const delta = p.map((w, i) => w - u[i] * x - v[i] * y) as Vec3;
      const worldDelta = [0, 1, 2].map((i) => m[i] * delta[0] + m[4 + i] * delta[1] + m[8 + i] * delta[2]);
      if (Math.hypot(...worldDelta) > 1e-5) continue;
      const weights = [1 - x - y, x, y], value = zero();
      let coverage = 0, surfaceCoverage = 0;
      for (let j = 0; j < 3; j++) {
        const n = mesh.normals.subarray(ids[j] * 3, ids[j] * 3 + 3);
        const transformed = normalize([0, 1, 2].map((i) => inv[i * 4] * n[0] + inv[i * 4 + 1] * n[1] + inv[i * 4 + 2] * n[2]) as Vec3);
        const sign = dot(transformed, point.normal) >= 0 ? 1 : -1;
        const offset = ids[j] * 4 + (sign > 0 ? 0 : 2);
        const result = vertex(mesh.radianceProbes[offset], mesh.radianceProbes[offset + 1], transformed.map((v) => v * sign) as Vec3);
        coverage += result.coverage * weights[j];
        surfaceCoverage += result.surface * weights[j];
        for (const channel of ["sky", "local", "emission"] as const)
          for (let c = 0; c < 3; c++) value[channel][c] += result.value[channel][c] * weights[j];
      }
      if (coverage > 0) for (const channel of ["sky", "local", "emission"] as const)
        for (let c = 0; c < 3; c++) value[channel][c] /= coverage;
      matches.push({ source: source.id, triangle: at / 3, weights, coverage, surfaceCoverage, value });
    }
  }
  // Shared edges may have multiple triangles. Keep all answers so discontinuity
  // is visible in the evidence rather than hidden by whichever triangle won.
  return matches;
}
