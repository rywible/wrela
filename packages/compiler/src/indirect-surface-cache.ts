import type { IndirectLightingField, RenderSurface, Vec3 } from "@wrela/model";

import { indirectProbePosition, indirectProbeWeights } from "./indirect-probes";
import {
  indirectDot as dot,
  type IndirectGeometry,
  indirectNormalize,
  indirectSurfaceExclusion,
} from "./indirect-query";
import { shadowCone } from "./indirect-regions";

export type IndirectSurfaceCacheOptions = {
  spacing?: number;
  /** Resolve irradiance and reflected SH after relighting; false retains the
   * exact per-probe angular clamp reference and caches only blend weights. */
  radiance?: boolean;
  maxSamples?: number;
  maxPatches?: number;
  /** Sampled L1 blend-weight error, independent of lighting intensity/color.
   * This is an interpolation quality check, not a visibility certificate. */
  maxWeightError?: number;
};
const sub = (a: Vec3, b: Vec3) => a.map((v, i) => v - b[i]) as Vec3;
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
type Chart = { origin: Vec3; u: Vec3; v: Vec3; normal: Vec3; width: number; height: number };

/** Deliberately narrow first product: a rectangular two-triangle receiver,
 * constant geometric shading normal, no displaced or procedural normal field.
 * Decomposing arbitrary curved field products into charts is separate work. */
function rectangle(surface: RenderSurface, renderOrigin: Vec3): Chart | undefined {
  if (
    indirectSurfaceExclusion(surface) ||
    surface.reliefAppearance ||
    surface.material.appearance ||
    surface.material.creature ||
    surface.material.normalStrength !== 0 ||
    surface.material.layers?.length ||
    (surface.selectedRenderProduct && surface.selectedRenderProduct.kind !== "direct-mesh")
  )
    return;
  const { mesh, matrix: m } = surface;
  const start = surface.drawRange?.start ?? 0,
    count = surface.drawRange?.count ?? mesh.indices.length;
  if (count !== 6) return;
  const vertices: Vec3[] = [],
    ids: number[] = [],
    localNormals: Vec3[] = [];
  for (let i = start; i < start + count; i++) {
    const index = mesh.indices[i] * 3;
    const local = Array.from(mesh.positions.subarray(index, index + 3)) as Vec3;
    const p = [0, 1, 2].map(
      (a) => m[a] * local[0] + m[4 + a] * local[1] + m[8 + a] * local[2] + m[12 + a] + renderOrigin[a],
    ) as Vec3;
    if (!p.every(Number.isFinite)) return;
    let id = vertices.findIndex((q) => Math.hypot(...sub(p, q)) < 1e-8);
    if (id < 0) {
      id = vertices.length;
      vertices.push(p);
    }
    ids.push(id);
    localNormals.push(Array.from(mesh.normals.subarray(index, index + 3)) as Vec3);
  }
  if (vertices.length !== 4 || new Set(ids.slice(0, 3)).size !== 3 || new Set(ids.slice(3)).size !== 3)
    return;
  const a = vertices[0],
    first = sub(vertices[ids[1]], a),
    second = sub(vertices[ids[2]], a);
  const normal = indirectNormalize(cross(first, second));
  const n0 = localNormals[0];
  if (localNormals.some((n) => n.length !== 3 || Math.hypot(...sub(n, n0)) > 1e-6)) return;
  // Inverse-transpose normal via transformed local tangent cross product. It
  // preserves the shader's orientation even for reflected affine transforms.
  const tangent = indirectNormalize(cross(n0, Math.abs(n0[0]) < 0.8 ? [1, 0, 0] : [0, 1, 0]));
  const bitangent = cross(n0, tangent);
  const transform = (v: Vec3) =>
    [0, 1, 2].map((i) => m[i] * v[0] + m[4 + i] * v[1] + m[8 + i] * v[2]) as Vec3;
  const determinant = dot(transform([1, 0, 0]), cross(transform([0, 1, 0]), transform([0, 0, 1])));
  const shading = indirectNormalize(cross(transform(tangent), transform(bitangent))).map(
    (v) => v * Math.sign(determinant),
  ) as Vec3;
  if (Math.abs(determinant) < 1e-10 || dot(normal, shading) < 0.999999) return;
  const adjacent = vertices.slice(1).map((p) => sub(p, a));
  for (let i = 0; i < 3; i++)
    for (let j = i + 1; j < 3; j++) {
      const edgeU = adjacent[i],
        edgeV = adjacent[j],
        width = Math.hypot(...edgeU),
        height = Math.hypot(...edgeV);
      if (width < 1e-5 || height < 1e-5 || Math.abs(dot(edgeU, edgeV)) > width * height * 1e-7) continue;
      const diagonal = adjacent[3 - i - j];
      if (Math.hypot(...diagonal.map((v, a) => v - edgeU[a] - edgeV[a])) > Math.max(width, height) * 1e-7)
        continue;
      const u = indirectNormalize(edgeU),
        v = indirectNormalize(edgeV);
      const area = [0, 3].map((k) => {
        const c = cross(
          sub(vertices[ids[k + 1]], vertices[ids[k]]),
          sub(vertices[ids[k + 2]], vertices[ids[k]]),
        );
        return dot(c, normal) * 0.5;
      });
      if (area.some((v) => v <= 0) || Math.abs(area[0] + area[1] - width * height) > width * height * 1e-7)
        return;
      return { origin: a, u, v, normal, width, height };
    }
}

/** Whole-tile segment classification. Each triangle's shadow cone is tested
 * against all four receiver corners, not visibility samples. Uncertain edges,
 * coplanarity and numerical margins retain the existing exact shader queries. */
export function surfaceTileVisibility(
  geometry: IndirectGeometry,
  corners: Vec3[],
  normal: Vec3,
  probe: Vec3,
): boolean | undefined {
  // Local coordinates keep 10 Mm worlds from losing plane precision.
  const origin = corners[0];
  const p = sub(probe, origin);
  const receivers = corners.map((c) => sub(c, origin).map((v, a) => v + normal[a] * 0.0002) as Vec3);
  const extent = Math.max(...receivers.map((r) => Math.hypot(...r)), Math.hypot(...p));
  const geometrySpan = Math.hypot(...geometry.bounds.max.map((v, a) => v - geometry.bounds.min[a]));
  const epsilon = Math.max(0.000002, extent * 4e-7, geometrySpan * 4e-7);
  const broadPad = epsilon + geometrySpan * 2e-6;
  const lo = [0, 1, 2].map((a) => Math.min(p[a], ...receivers.map((r) => r[a])) - broadPad);
  const hi = [0, 1, 2].map((a) => Math.max(p[a], ...receivers.map((r) => r[a])) + broadPad);
  // Bounded broad phase. Exhaustion is uncertainty, never a visibility proof.
  const pending = geometry.nodes.length ? [0] : [],
    candidates: number[] = [];
  let visits = 0;
  while (pending.length) {
    if (++visits > 512) return;
    const node = geometry.nodes[pending.pop() ?? 0];
    if (node.min.some((v, a) => v - origin[a] > hi[a]) || node.max.some((v, a) => v - origin[a] < lo[a]))
      continue;
    if (node.count) {
      for (let i = 0; i < node.count; i++) candidates.push(geometry.order[node.start + i]);
      if (candidates.length > 256) return;
    } else pending.push(node.left, node.right);
  }
  let uncertain = false;
  for (const index of candidates) {
    const t = geometry.triangles[index];
    if (t.min.some((v, a) => v - origin[a] > hi[a]) || t.max.some((v, a) => v - origin[a] < lo[a])) continue;
    const a = sub(t.a, origin);
    const cone = shadowCone([a, a.map((v, i) => v + t.ab[i]), a.map((v, i) => v + t.ac[i])], p, epsilon);
    if (!cone) {
      uncertain = true;
      continue;
    }
    const signed = (plane: number[], r: Vec3) =>
      plane[0] * r[0] + plane[1] * r[1] + plane[2] * r[2] + plane[3];
    if (cone.outer.some((plane) => receivers.every((r) => signed(plane, r) < -epsilon))) continue;
    if (cone.inner.every((plane) => receivers.every((r) => signed(plane, r) > epsilon + 0.0005)))
      return false;
    uncertain = true;
  }
  return uncertain ? undefined : true;
}

/** Compile geometry-only weights and optional GPU-resolved radiance storage.
 * Source intensity/color edits reuse the surface product and relight it; the
 * weights-only control keeps the original per-probe reflection angular clamp. */
export function* indirectSurfaceCacheSteps(
  geometry: IndirectGeometry,
  field: IndirectLightingField,
  surfaces: readonly RenderSurface[],
  options: IndirectSurfaceCacheOptions = {},
  renderOrigin: Vec3 = [0, 0, 0],
): Generator<void, NonNullable<IndirectLightingField["surfaceCache"]>> {
  const started = performance.now(),
    spacing = options.spacing ?? Math.min(...field.spacing) * 0.125,
    limit = options.maxSamples ?? 16384;
  const patchLimit = options.maxPatches ?? 64,
    errorLimit = options.maxWeightError ?? 0.04;
  if (
    !Number.isFinite(spacing) ||
    spacing <= 0 ||
    !Number.isInteger(limit) ||
    limit < 4 ||
    limit > 65536 ||
    !Number.isInteger(patchLimit) ||
    patchLimit < 1 ||
    patchLimit > 256 ||
    !Number.isFinite(errorLimit) ||
    errorLimit < 0 ||
    errorLimit > 0.25
  )
    throw Error("Invalid surface lighting cache budget");
  const sampleMetadata: number[] = [];
  const headers: number[] = [],
    weights: number[] = [],
    flags: number[] = [],
    sources: NonNullable<IndirectLightingField["surfaceCache"]>["sources"][number][] = [];
  const excluded: { id: string; reason: string }[] = [];
  let admitted = 0;
  const rejected = { boundary: 0, visibility: 0, interpolation: 0 };
  for (const surface of surfaces) {
    const chart = rectangle(surface, renderOrigin);
    if (!chart) {
      excluded.push({
        id: surface.id,
        reason: "requires a static rectangular receiver with constant geometric normal",
      });
      continue;
    }
    const nx = Math.ceil(chart.width / spacing) + 1,
      ny = Math.ceil(chart.height / spacing) + 1;
    if (weights.length / 8 + nx * ny > limit || sources.length >= patchLimit) {
      excluded.push({ id: surface.id, reason: "surface cache allocation budget" });
      continue;
    }
    const dx = chart.width / (nx - 1),
      dy = chart.height / (ny - 1),
      first = weights.length / 8,
      firstTile = flags.length;
    const point = (x: number, y: number) =>
      chart.origin.map((v, a) => v + chart.u[a] * x * dx + chart.v[a] * y * dy) as Vec3;
    const blends: ReturnType<typeof indirectProbeWeights>[] = [];
    for (let y = 0; y < ny; y++)
      for (let x = 0; x < nx; x++) {
        const blend = indirectProbeWeights(field, point(x, y), chart.normal);
        const c = blend?.cell ?? [0, 0, 0];
        sampleMetadata.push(
          ...chart.normal,
          c[0] + field.dimensions[0] * (c[1] + field.dimensions[1] * c[2]),
        );
        blends.push(blend);
        weights.push(...(blend?.weights ?? new Array(8).fill(0)));
        yield;
      }
    for (let y = 0; y < ny - 1; y++)
      for (let x = 0; x < nx - 1; x++) {
        const corners = [point(x, y), point(x + 1, y), point(x, y + 1), point(x + 1, y + 1)];
        const samples = [
          blends[x + y * nx],
          blends[x + 1 + y * nx],
          blends[x + (y + 1) * nx],
          blends[x + 1 + (y + 1) * nx],
        ];
        const base = samples[0]?.cell;
        let valid = !!base && samples.every((s) => s?.cell.every((v, a) => v === base[a]));
        if (valid && base)
          valid = corners.every((p) =>
            p.every((v, a) => {
              const q = Math.max(
                0,
                Math.min(
                  field.dimensions[a] - 1,
                  (v - field.origin[a] + chart.normal[a] * Math.min(...field.spacing) * 0.02) /
                    field.spacing[a],
                ),
              );
              return q - base[a] > 0.00001 && q - base[a] < 0.99999;
            }),
          );
        if (!valid) rejected.boundary++;
        if (valid && base)
          for (let corner = 0; corner < 8; corner++) {
            const c = base.map((v, a) => v + ((corner >> a) & 1));
            const index = c[0] + field.dimensions[0] * (c[1] + field.dimensions[1] * c[2]);
            if (field.data[index * 60 + 3] < 0.5) continue;
            const probe = indirectProbePosition(field, index);
            // Entire plane faces away from this probe; weight is identically zero.
            if (dot(sub(probe, chart.origin), chart.normal) < -0.00002) continue;
            if (surfaceTileVisibility(geometry, corners, chart.normal, probe) === undefined) {
              rejected.visibility++;
              valid = false;
              break;
            }
            yield;
          }
        // Geometry proves visibility constancy, while these independent interior
        // samples reject rapidly changing normalized moment/facing weights.
        if (valid)
          for (const [u, v] of [
            [0.5, 0],
            [0, 0.5],
            [1, 0.5],
            [0.5, 1],
            [0.5, 0.5],
          ]) {
            const truth = indirectProbeWeights(field, point(x + u, y + v), chart.normal);
            const factors = [(1 - u) * (1 - v), u * (1 - v), (1 - u) * v, u * v];
            const error =
              truth?.weights.reduce(
                (sum, w, i) =>
                  sum + Math.abs(w - samples.reduce((s, b, k) => s + (b?.weights[i] ?? 0) * factors[k], 0)),
                0,
              ) ?? Infinity;
            if (error > errorLimit) {
              rejected.interpolation++;
              valid = false;
              break;
            }
          }
        flags.push(valid ? 1 : 0);
        if (valid) admitted++;
        yield;
      }
    headers.push(
      ...sub(chart.origin, field.origin),
      nx,
      ...chart.u.map((v) => v / dx),
      ny,
      ...chart.v.map((v) => v / dy),
      first,
      ...chart.normal,
      firstTile,
    );
    sources.push({
      id: surface.id,
      positions: surface.mesh.positions,
      normals: surface.mesh.normals,
      indices: surface.mesh.indices,
      matrix: Array.from(surface.matrix, (v, i) => v + (i >= 12 && i < 15 ? renderOrigin[i - 12] : 0)),
      start: surface.drawRange?.start ?? 0,
      count: surface.drawRange?.count ?? surface.mesh.indices.length,
    });
  }
  const weightOffset = 2 + headers.length / 4,
    flagOffset = weightOffset + weights.length / 4;
  const metadataOffset = flagOffset + Math.ceil(flags.length / 4);
  const outputOffset = options.radiance === false ? 0 : metadataOffset + sampleMetadata.length / 4;
  const data = new Float32Array(
    (outputOffset ? outputOffset + (weights.length / 8) * 10 : metadataOffset) * 4,
  );
  data.set([sources.length, weights.length / 8, weightOffset, flagOffset]);
  data.set([metadataOffset, outputOffset, 0, 0], 4);
  data.set(headers, 8);
  data.set(weights, weightOffset * 4);
  data.set(flags, flagOffset * 4);
  if (outputOffset) data.set(sampleMetadata, metadataOffset * 4);
  return {
    data,
    sources,
    report: {
      patches: sources.length,
      samples: weights.length / 8,
      tiles: flags.length,
      admitted,
      rejected,
      buildMs: performance.now() - started,
      excluded,
    },
  };
}
