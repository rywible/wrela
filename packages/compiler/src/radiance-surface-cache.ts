import {
  inverseMatrix,
  normalize,
  type RadianceLightingField,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";
import { indirectSH } from "./indirect-probes";
import { indirectDot, traceIndirectRay } from "./indirect-query";
import type { RadianceLightingProduct } from "./radiance-lighting";
import { compileSurfaceLatticeSteps, surfaceLatticeBinding } from "./radiance-surface-lattice";

/** Bounded specialization for rigid, constant-normal source triangles. The
 * lattice divides the refined carrier exactly, so no admitted carrier triangle
 * can interpolate across an uncertified cell. Reflection bindings are retained. */
export function* compileRadianceSurfaceCacheSteps(
  product: RadianceLightingProduct,
  surfaces: readonly RenderSurface[],
  origin: Vec3,
  center: Vec3,
  reuse?: RadianceLightingProduct,
): Generator<void> {
  const candidates: {
    id: string;
    vertices: number[];
    triangle: number;
    normal: Vec3;
    resolution: number;
    score: number;
  }[] = [];
  for (const surface of surfaces) {
    const receiver = product.receivers.get(surface.id);
    if (!receiver?.sources || !receiver.weights) continue;
    const m = surface.matrix,
      inverse = inverseMatrix(m);
    if (!inverse) continue;
    const groups = new Map<string, number[]>();
    for (let v = 0; v < receiver.sources.length / 3; v++) {
      const key = receiver.sources.subarray(v * 3, v * 3 + 3).join(",");
      const group = groups.get(key) ?? [];
      group.push(v);
      groups.set(key, group);
    }
    for (const vertices of groups.values()) {
      const source = Array.from(receiver.sources.subarray(vertices[0] * 3, vertices[0] * 3 + 3));
      const normals = source.map((i) => surface.mesh.normals.subarray(i * 3, i * 3 + 3));
      if (normals.some((n) => n.some((v, a) => Math.abs(v - normals[0][a]) > 1e-6))) continue;
      const normal = normalize(
        [0, 1, 2].map(
          (a) =>
            inverse[a * 4] * normals[0][0] +
            inverse[a * 4 + 1] * normals[0][1] +
            inverse[a * 4 + 2] * normals[0][2],
        ) as Vec3,
      );
      const points = source.map(
        (i) =>
          [0, 1, 2].map(
            (a) =>
              m[a] * surface.mesh.positions[i * 3] +
              m[4 + a] * surface.mesh.positions[i * 3 + 1] +
              m[8 + a] * surface.mesh.positions[i * 3 + 2] +
              m[12 + a] +
              origin[a],
          ) as Vec3,
      );
      const centroid = [0, 1, 2].map((a) => points.reduce((sum, p) => sum + p[a] / 3, 0)) as Vec3;
      const distance = Math.hypot(...centroid.map((v, a) => v - center[a]));
      if (distance > 32) continue;
      const hit = traceIndirectRay(
        product.geometry,
        centroid.map((v, a) => v + normal[a] * 0.003) as Vec3,
        normal.map((v) => -v) as Vec3,
        0.006,
      );
      if (!hit || Math.abs(indirectDot(hit.normal, normal)) < 1 - 1e-6) continue;
      const t = product.geometry.triangles[hit.triangle];
      const expected = [t.a, t.a.map((v, a) => v + t.ab[a]), t.a.map((v, a) => v + t.ac[a])];
      // Preserve source orientation/order, not just a nearby plane hit.
      if (points.some((p, i) => p.some((v, a) => Math.abs(v - expected[i][a]) > 1e-6))) continue;
      const edge = Math.max(
        ...points.map((p, i) => Math.hypot(...p.map((v, a) => v - points[(i + 1) % 3][a]))),
      );
      const area =
        0.5 *
        Math.sqrt(
          Math.max(0, indirectDot(t.ab, t.ab) * indirectDot(t.ac, t.ac) - indirectDot(t.ab, t.ac) ** 2),
        );
      if (area < 0.25 || area / (edge * edge) < 0.015) continue;
      const carrierResolution = Math.round((Math.sqrt(8 * vertices.length + 1) - 3) / 2);
      if (((carrierResolution + 1) * (carrierResolution + 2)) / 2 !== vertices.length) continue;
      let resolution = 1;
      while (
        resolution < carrierResolution &&
        (resolution < Math.ceil(edge) || carrierResolution % resolution !== 0)
      )
        resolution++;
      const count = ((resolution + 1) * (resolution + 2)) / 2;
      candidates.push({
        id: surface.id,
        vertices,
        triangle: hit.triangle,
        normal,
        resolution,
        score: vertices.length / count / (1 + (distance * distance) / 256),
      });
      yield;
    }
  }
  candidates.sort((a, b) => b.score - a.score || a.triangle - b.triangle);
  const positions: Vec3[] = [],
    transfer: number[] = [],
    records: number[] = [];
  const patches: NonNullable<NonNullable<RadianceLightingField["surfaceDiffuse"]>["patches"]> = [];
  const old = reuse?.field.surfaceDiffuse;
  const patchKey = (p: { triangle: number; normal: Vec3; resolution: number }) =>
    `${p.triangle}/${p.normal.join(",")}/${p.resolution}`;
  const oldPatches = new Map(old?.patches?.map((p) => [patchKey(p), p]));
  let reusedSamples = 0;
  let rays = 0;
  for (const candidate of candidates) {
    const count = ((candidate.resolution + 1) * (candidate.resolution + 2)) / 2;
    if (positions.length + count > 2048) continue;
    const receiver = product.receivers.get(candidate.id),
      mesh = product.meshes.get(candidate.id);
    if (!receiver?.weights || !mesh?.radianceProbes) continue;
    const previous = oldPatches.get(patchKey(candidate));
    const lattice =
      old && previous
        ? {
            ...previous,
            positions: old.positions.slice(previous.offset, previous.offset + count),
            transfer: old.transfer.subarray(previous.offset * 108, (previous.offset + count) * 108),
            rays: 0,
          }
        : yield* compileSurfaceLatticeSteps(
            product.geometry,
            candidate.triangle,
            candidate.normal,
            product.field.lights,
            { spacing: 1, samples: 64, resolution: candidate.resolution },
          );
    rays += lattice.rays;
    const weights = receiver.weights;
    const coordinates = (v: number): Vec3 => Array.from(weights.subarray(v * 3, v * 3 + 3)) as Vec3;
    const rejected = new Set<number>();
    const owned = new Set(candidate.vertices);
    for (let at = 0; at < mesh.indices.length; at += 3) {
      const ids = Array.from(mesh.indices.subarray(at, at + 3));
      if (!owned.has(ids[0])) continue;
      const midpoint = [0, 1, 2].map((a) => ids.reduce((sum, v) => sum + weights[v * 3 + a] / 3, 0)) as Vec3;
      if (!lattice.validCells[surfaceLatticeBinding(lattice.resolution, midpoint).cell])
        for (const id of ids) rejected.add(id);
    }
    const admitted = candidate.vertices.filter(
      (v) => !rejected.has(v) && mesh.radianceProbes![v * 4] >= 1024,
    );
    if (!admitted.length) continue;
    if (previous) reusedSamples += count;
    const sampleOffset = positions.length;
    patches.push({
      triangle: lattice.triangle,
      normal: lattice.normal,
      resolution: lattice.resolution,
      offset: sampleOffset,
      validCells: lattice.validCells,
      directEmission: lattice.directEmission,
    });
    positions.push(...lattice.positions);
    for (const value of lattice.transfer) transfer.push(value);
    const basis = indirectSH(candidate.normal);
    for (const v of admitted) {
      const mixture = Math.floor(mesh.radianceProbes[v * 4]) - 1024;
      const binding = surfaceLatticeBinding(lattice.resolution, coordinates(v));
      const emission: Vec3 = [0, 0, 0];
      // Keep the position-correct direct emitter term and existing emitted
      // bounce for this first sky/local-light specialization.
      if (product.field.receiverEmission && product.field.directEmission && product.field.receivers) {
        for (let c = 0; c < 3; c++) emission[c] = product.field.receiverEmission[mixture * 3 + c];
        for (let j = 0; j < 4; j++) {
          const probe = product.field.receivers[mixture * 8 + j] - 1;
          const weight = product.field.receivers[mixture * 8 + j + 4];
          if (!weight || probe < 0) continue;
          for (let c = 0; c < 3; c++) {
            let e = 0;
            for (let k = 0; k < 9; k++)
              e +=
                (product.field.transfer[((probe * 9 + k) * 27 + 26) * 4 + c] -
                  product.field.directEmission[(probe * 9 + k) * 3 + c]) *
                basis[k] *
                (k === 0 ? 1 : k < 4 ? 2 / 3 : 1 / 4);
            emission[c] += Math.max(0, e) * weight;
          }
        }
      }
      mesh.radianceProbes[v * 4 + 1] = records.length / 12 + 1;
      records.push(...binding.ids.map((id) => id + sampleOffset), 0, ...binding.weights, 0, ...emission, 0);
      if (records.length % (12 * 64) === 0) yield;
    }
    yield;
  }
  if (!positions.length) return;
  const cache: NonNullable<RadianceLightingField["surfaceDiffuse"]> = {
    positions,
    transfer: new Float32Array(transfer),
    receivers: new Float32Array(records),
    patches,
  };
  product.field.surfaceDiffuse = cache;
  product.field.report.surfaceSamples = positions.length;
  product.field.report.surfaceReceivers = records.length / 12;
  product.field.report.surfaceRays = rays;
  product.field.report.reusedSurfaceSamples = reusedSamples;
  product.field.report.rays += rays;
  product.field.report.bytes +=
    positions.length * 3 * 8 +
    cache.transfer.byteLength +
    cache.receivers.byteLength +
    patches.reduce((sum, p) => sum + p.validCells.byteLength + p.directEmission.byteLength + 64, 0);
}
