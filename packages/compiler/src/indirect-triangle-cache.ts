import type { IndirectLightingField, MeshData, RenderSurface, Vec3 } from "@wrela/model";

import { indirectProbePosition } from "./indirect-probes";
import {
  indirectDot as dot,
  type IndirectGeometry,
  indirectNormalize,
  indirectSurfaceExclusion,
} from "./indirect-query";
import { shadowCone } from "./indirect-regions";

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
// Shader admits a slightly smaller cone, leaving room for octahedral/FP32 error.
const cosine = 0.5;
const signedDistance = (p: readonly number[], v: Vec3) => p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3];
type ProofContext = {
  origin: Vec3;
  epsilon: number;
  cones: Map<number, { cone: ReturnType<typeof shadowCone> }>;
  coneKeys: Float64Array;
  nextCone: number;
  hits: number;
  misses: number;
};

/** Support of a spherical cap. Bounds EVERY allowed material normal, not just
 * the three vertex normals. The ray-origin offset remains on the surface's
 * outward side, unlike an enclosing sphere that crosses its own triangle. */
function capMaximum(direction: number[], normal: Vec3, sign = 1): number {
  const length = Math.hypot(direction[0], direction[1], direction[2]);
  if (length === 0) return 0;
  const axial = sign * dot(direction as Vec3, normal);
  if (axial >= length * cosine) return length;
  return (
    axial * cosine + Math.sqrt(Math.max(0, length * length - axial * axial)) * Math.sqrt(1 - cosine * cosine)
  );
}

/** A certificate is only clear/blocked if the whole receiver triangle, with
 * every offset in its normal cone, lies outside/inside a triangle shadow cone.
 * Broad-phase exhaustion is uncertainty. No sampled clear ray proves visibility. */
function triangleProbeCandidates(
  geometry: IndirectGeometry,
  points: Vec3[],
  normal: Vec3,
  probe: Vec3,
  context?: ProofContext,
  probeIndex = 0,
): { visibility?: boolean; candidates?: number[] } {
  const origin = context?.origin ?? points[0],
    p = sub(probe, origin),
    vertices = points.map((v) => sub(v, origin));
  const span = Math.hypot(...geometry.bounds.max.map((v, a) => v - geometry.bounds.min[a]));
  const epsilon = context?.epsilon ?? Math.max(0.000002, span * 4e-7, Math.hypot(...p) * 4e-7);
  const pad = epsilon + span * 2e-6 + 0.0002;
  const lo = [0, 1, 2].map((a) => Math.min(p[a], ...vertices.map((v) => v[a])) - pad);
  const hi = [0, 1, 2].map((a) => Math.max(p[a], ...vertices.map((v) => v[a])) + pad);
  const pending = geometry.nodes.length ? [0] : [],
    candidates: number[] = [];
  let visits = 0;
  while (pending.length) {
    if (++visits > 512) return {};
    const node = geometry.nodes[pending.pop() ?? 0];
    if (node.min.some((v, a) => v - origin[a] > hi[a]) || node.max.some((v, a) => v - origin[a] < lo[a]))
      continue;
    if (node.count) {
      for (let i = 0; i < node.count; i++) candidates.push(geometry.order[node.start + i]);
      if (candidates.length > 256) return {};
    } else pending.push(node.left, node.right);
  }
  const uncertain: number[] = [];
  for (const index of candidates) {
    const t = geometry.triangles[index];
    if (t.min.some((v, a) => v - origin[a] > hi[a]) || t.max.some((v, a) => v - origin[a] < lo[a])) continue;
    const a = sub(t.a, origin);
    const key = probeIndex * geometry.triangles.length + index;
    let entry = context?.cones.get(key);
    if (entry && context) context.hits++;
    if (!entry) {
      entry = {
        // Segment queries ignore the final 0.1 mm next to the probe. A blocker
        // there cannot certify a blocked cone, even if the receiver is far away.
        // shadowCone rejects probe/plane distances below eight times its margin.
        cone: shadowCone(
          [a, a.map((v, i) => v + t.ab[i]), a.map((v, i) => v + t.ac[i])],
          p,
          Math.max(epsilon, (0.0001 + epsilon) / 8),
        ),
      };
      if (context) {
        context.misses++;
        // A ring keeps FIFO eviction constant-time. Starting a Map iterator at
        // each eviction repeatedly scans its deleted prefix on large worlds.
        if (context.cones.size >= context.coneKeys.length)
          context.cones.delete(context.coneKeys[context.nextCone]);
        context.coneKeys[context.nextCone] = key;
        context.nextCone = (context.nextCone + 1) % context.coneKeys.length;
        context.cones.set(key, entry);
      }
    }
    const cone = entry.cone;
    if (!cone) {
      uncertain.push(index);
      continue;
    }
    let clear = false;
    for (const plane of cone.outer) {
      const offset = 0.0002 * capMaximum(plane, normal);
      if (vertices.every((v) => signedDistance(plane, v) + offset < -epsilon)) {
        clear = true;
        break;
      }
    }
    if (clear) continue;
    let blocked = true;
    for (const plane of cone.inner) {
      const offset = 0.0002 * capMaximum(plane, normal, -1);
      if (vertices.some((v) => signedDistance(plane, v) - offset <= epsilon + 0.0005)) {
        blocked = false;
        break;
      }
    }
    if (blocked) return { visibility: false, candidates: [] };
    uncertain.push(index);
  }
  return { visibility: uncertain.length ? undefined : true, candidates: uncertain };
}

export function triangleProbeVisibility(
  geometry: IndirectGeometry,
  points: Vec3[],
  normal: Vec3,
  probe: Vec3,
): boolean | undefined {
  return triangleProbeCandidates(geometry, points, normal, probe).visibility;
}

function encodeNormal(n: Vec3): [number, number] {
  const l = Math.abs(n[0]) + Math.abs(n[1]) + Math.abs(n[2]);
  const x = n[0] / l,
    y = n[1] / l;
  return n[2] >= 0 ? [x, y] : [(1 - Math.abs(y)) * (x < 0 ? -1 : 1), (1 - Math.abs(x)) * (y < 0 ? -1 : 1)];
}

/** Scheduling estimate only. The certificate remains responsible for correctness;
 * no estimate may replace a visibility test. Avoid replacing already-cheap leaves. */
function existingQueryWork(
  field: IndirectLightingField,
  cell: number[],
  fraction: number[],
  active: number,
): number {
  const data = field.visibility?.cells;
  if (!data) return 64;
  const dims = field.dimensions.map((v) => v - 1),
    header = (cell[0] + dims[0] * (cell[1] + dims[1] * cell[2])) * 4;
  let region = data[header + 2];
  const f = fraction.slice();
  for (let level = 0; region > 0 && level < 3; level++) {
    const r = region * 4,
      next = data[r + 2],
      count = data[r + 3];
    active &= ~(data[r] | data[r + 1]);
    if (!active) return 0;
    if (next > 0) {
      let child = 0;
      for (let a = 0; a < 3; a++) {
        const b = f[a] >= 0.5 ? 1 : 0;
        child |= b << a;
        f[a] = f[a] * 2 - b;
      }
      region = next + child;
    } else {
      if (count >= 0) {
        let work = 0;
        const start = (-next - 1) * 4;
        for (let i = 0; i < count; i++) work += popcount(data[start + i * 2 + 1] & active);
        return work;
      }
      break;
    }
  }
  return popcount(active) * (data[header + 1] >= 0 ? data[header + 1] : 64);
}
function popcount(value: number): number {
  let count = 0;
  for (; value; value &= value - 1) count++;
  return count;
}

/** Static mesh charts represented by exact receiver triangles. Their visibility
 * masks occupy unused rigid skin-weight slots; shading still evaluates the real
 * material normal, moment weights and SH. This product does not bake irradiance. */
export function* indirectTriangleCacheSteps(
  geometry: IndirectGeometry,
  field: IndirectLightingField,
  surfaces: readonly RenderSurface[],
  renderOrigin: Vec3 = [0, 0, 0],
): Generator<void, NonNullable<IndirectLightingField["triangleCache"]>> {
  const started = performance.now(),
    sources: NonNullable<IndirectLightingField["triangleCache"]>["sources"][number][] = [];
  const report = {
    triangles: 0,
    admitted: 0,
    clear: 0,
    blocked: 0,
    regionBytes: 0,
    candidates: 0,
    avoidedBuilds: 0,
    rejectedCost: 0,
    coneReuse: 0,
    coneBuilds: 0,
    bytes: 0,
    buildMs: 0,
    excluded: [] as { id: string; reason: string }[],
  };
  const cells = field.visibility?.cells;
  const context: ProofContext = {
    origin: field.origin,
    epsilon: Math.max(
      0.000002,
      Math.hypot(...geometry.bounds.max.map((v, a) => v - geometry.bounds.min[a])) * 4e-7,
      Math.hypot(...field.spacing.map((v, a) => v * field.dimensions[a])) * 4e-7,
    ),
    cones: new Map(),
    coneKeys: new Float64Array(16384),
    nextCone: 0,
    hits: 0,
    misses: 0,
  };
  const regions: number[] = [];
  const queryIndex = new Uint32Array(geometry.order.length);
  geometry.order.forEach((index, packed) => {
    queryIndex[index] = packed;
  });
  const rectangles = new Set(field.surfaceCache?.sources.map((s) => s.id));
  for (const surface of surfaces) {
    const reason =
      indirectSurfaceExclusion(surface) ??
      (surface.reliefAppearance || surface.mesh.reliefCoordinates
        ? "displaced realization"
        : surface.selectedRenderProduct && surface.selectedRenderProduct.kind !== "direct-mesh"
          ? "alternate realization"
          : undefined);
    if (reason || rectangles.has(surface.id)) {
      report.excluded.push({ id: surface.id, reason: reason ?? "resolved rectangular cache" });
      continue;
    }
    const { mesh, matrix: m } = surface,
      start = surface.drawRange?.start ?? 0,
      count = surface.drawRange?.count ?? mesh.indices.length;
    if (report.triangles + count / 3 > 65536) {
      report.excluded.push({ id: surface.id, reason: "receiver triangle budget" });
      continue;
    }
    // A small material range may reference a much larger shared vertex array.
    // Bound that retained stream independently of the receiver triangle count.
    if (report.bytes + (mesh.positions.length / 3) * 16 > 4 * 1024 * 1024) {
      report.excluded.push({ id: surface.id, reason: "receiver vertex budget" });
      continue;
    }
    const proofs = new Float32Array((mesh.positions.length / 3) * 4);
    // Flat interpolation uses the first (provoking) vertex. Certify the union
    // of every triangle that shares it; preserve all original indexed geometry.
    const groups = new Map<number, number[]>();
    for (let i = start; i < start + count; i += 3) {
      const vertex = mesh.indices[i];
      const list = groups.get(vertex) ?? [];
      list.push(i);
      groups.set(vertex, list);
      if ((i - start) % 3072 === 0) yield;
    }
    let admitted = 0;
    const point = (i: number): Vec3 =>
      [0, 1, 2].map(
        (a) =>
          m[a] * mesh.positions[i * 3] +
          m[4 + a] * mesh.positions[i * 3 + 1] +
          m[8 + a] * mesh.positions[i * 3 + 2] +
          m[12 + a] +
          renderOrigin[a],
      ) as Vec3;
    for (const [vertex, group] of groups) {
      report.triangles += group.length;
      if (group.length > 32) {
        yield;
        continue;
      }
      const points: Vec3[] = [],
        edge: Vec3 = [0, 0, 0];
      for (const i of group) {
        const p = [0, 1, 2].map((c) => point(mesh.indices[i + c]));
        points.push(...p);
        const n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        for (let a = 0; a < 3; a++) edge[a] += n[a];
      }
      if (Math.hypot(...edge) < 1e-12) {
        yield;
        continue;
      }
      const normal = indirectNormalize(edge),
        center = [0, 1, 2].map((a) => points.reduce((sum, p) => sum + p[a], 0) / points.length) as Vec3;
      // Only this cell consumes the proof; pixels in another interpolation cell
      // fall back. Visibility itself is proved over the whole triangle.
      const q = center.map(
        (v, a) => (v - field.origin[a] + normal[a] * Math.min(...field.spacing) * 0.02) / field.spacing[a],
      );
      if (q.some((v, a) => v < 0 || v > field.dimensions[a] - 1)) {
        yield;
        continue;
      }
      const cell = q.map((v, a) => Math.min(field.dimensions[a] - 2, Math.floor(v)));
      const first = cell[0] + field.dimensions[0] * (cell[1] + field.dimensions[1] * cell[2]);
      let active = 0;
      for (let corner = 0; corner < 8; corner++) {
        const index =
          first +
          (corner & 1) +
          field.dimensions[0] * ((corner >> 1) & 1) +
          field.dimensions[0] * field.dimensions[1] * ((corner >> 2) & 1);
        if (
          field.data[index * 60 + 3] >= 0.5 &&
          dot(sub(indirectProbePosition(field, index), center), normal) > 0
        )
          active |= 1 << corner;
      }
      const oldWork = existingQueryWork(
        field,
        cell,
        q.map((v, a) => v - cell[a]),
        active,
      );
      if (oldWork < 16) {
        report.avoidedBuilds++;
        yield;
        continue;
      }
      let clear = 0,
        blocked = 0,
        complete = !!cells;
      const unresolved = new Map<number, number>();
      for (let corner = 0; corner < 8; corner++) {
        const index =
          first +
          (corner & 1) +
          field.dimensions[0] * ((corner >> 1) & 1) +
          field.dimensions[0] * field.dimensions[1] * ((corner >> 2) & 1);
        if (field.data[index * 60 + 3] >= 0.5) {
          const result = triangleProbeCandidates(
            geometry,
            points,
            normal,
            indirectProbePosition(field, index),
            context,
            index,
          );
          const visible = result.visibility;
          if (result.candidates === undefined) complete = false;
          for (const triangle of result.candidates ?? [])
            unresolved.set(queryIndex[triangle], (unresolved.get(queryIndex[triangle]) ?? 0) | (1 << corner));
          if (visible === true) {
            clear |= 1 << corner;
            report.clear++;
          }
          if (visible === false) {
            blocked |= 1 << corner;
            report.blocked++;
          }
        }
        yield;
      }
      // Retain a short exact list at uncertain edges instead of searching the
      // volume hierarchy. Overflow preserves the existing general query.
      const newWork = Array.from(unresolved.values()).reduce(
        (sum, mask) => sum + popcount(mask & active & ~(clear | blocked)),
        0,
      );
      if (newWork >= oldWork * 0.7) {
        report.rejectedCost++;
        continue;
      }
      if (
        complete &&
        unresolved.size <= 48 &&
        regions.length + 4 + Math.ceil(unresolved.size / 2) * 4 <= 1024 * 1024
      ) {
        const address = (cells?.length ?? 0) / 4 + regions.length / 4;
        regions.push(clear, blocked, address + 1, unresolved.size);
        for (const [triangle, mask] of unresolved) regions.push(triangle, mask);
        while (regions.length % 4) regions.push(0);
        const oct = encodeNormal(normal);
        proofs.set([first + 1, address + 1, ...oct], vertex * 4);
        report.candidates += unresolved.size;
        admitted += group.length;
      }
    }
    if (!admitted) continue;
    const cached: MeshData = { ...mesh, indirectProofs: proofs };
    report.bytes += proofs.byteLength;
    report.admitted += admitted;
    sources.push({
      id: surface.id,
      positions: mesh.positions,
      indices: mesh.indices,
      normals: mesh.normals,
      colors: mesh.colors,
      sourceIds: mesh.sourceIds,
      materialCoordinates: mesh.materialCoordinates,
      matrix: Array.from(m, (v, i) => v + (i >= 12 && i < 15 ? renderOrigin[i - 12] : 0)),
      start,
      count,
      mesh: cached,
    });
  }
  if (regions.length && field.visibility && cells) {
    const combined = new Float32Array(cells.length + regions.length);
    combined.set(cells);
    combined.set(regions, cells.length);
    field.visibility = { ...field.visibility, cells: combined };
    report.regionBytes = regions.length * 4;
  }
  report.buildMs = performance.now() - started;
  report.coneReuse = context.hits;
  report.coneBuilds = context.misses;
  return { sources, report };
}
