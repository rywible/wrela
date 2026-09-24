import type { Vec3 } from "@wrela/model";

import type { IndirectGeometry } from "./indirect-query";

type Plane = [number, number, number, number];
type Cone = { outer: Plane[]; inner: Plane[] } | undefined;
const dot = (a: number[], b: number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a: number[], b: number[]) => a.map((v, i) => v - b[i]);
const cross = (a: number[], b: number[]) => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];

/** Shadow half-spaces of a triangle as seen from a fixed probe. The enlarged
 * triangle bounds the shader's tolerant barycentric test. Only the original,
 * contracted half-spaces may establish blocked receivers. Degeneracy falls back. */
export function shadowCone(vertices: number[][], probe: number[], epsilon: number): Cone {
  const center = vertices[0].map((_, a) => (vertices[0][a] + vertices[1][a] + vertices[2][a]) / 3);
  const planes = (points: number[][]): Plane[] | undefined => {
    const face = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    const length = Math.hypot(...face);
    if (length < 1e-12) return;
    const signed = dot(face, sub(probe, points[0])) / length;
    if (Math.abs(signed) < epsilon * 8) return;
    const n = face.map((v) => (v / length) * -Math.sign(signed));
    const result: Plane[] = [[n[0], n[1], n[2], -dot(n, points[0])]];
    for (let edge = 0; edge < 3; edge++) {
      const a = sub(points[edge], probe),
        b = sub(points[(edge + 1) % 3], probe);
      const side = cross(a, b),
        size = Math.hypot(...side);
      if (size < 1e-12) return;
      const sign = Math.sign(dot(side, sub(points[(edge + 2) % 3], probe)));
      const normal = side.map((v) => (v * sign) / size);
      result.push([normal[0], normal[1], normal[2], -dot(normal, probe)]);
    }
    return result;
  };
  const inner = planes(vertices);
  const outer = planes(vertices.map((v) => v.map((x, a) => center[a] + (x - center[a]) * 1.000012)));
  return inner && outer ? { inner, outer } : undefined;
}

/** 0: wholly clear; 2: wholly blocked; 1: uncertain. The receiver box includes
 * every normal-dependent interpolation/ray offset, not just its centre sample. */
function classify(
  cone: Cone,
  center: number[],
  half: number[],
  epsilon: number,
  support?: number[][],
): number {
  if (!cone) return 1;
  // Classification only needs to know whether every vertex is on one side.
  // Stop at the first counterexample instead of finding every extremum. The
  // enclosing box also resolves planes that never touch the clipped support.
  for (const p of cone.outer) {
    const radius = Math.abs(p[0]) * half[0] + Math.abs(p[1]) * half[1] + Math.abs(p[2]) * half[2];
    const distance = dot(p, center) + p[3];
    if (distance + radius < -epsilon) return 0;
    if (
      support?.length &&
      distance - radius < -epsilon &&
      support.every((v) => dot(p, v) + p[3] + 0.0002 < -epsilon)
    )
      return 0;
  }
  for (const p of cone.inner) {
    const radius = Math.abs(p[0]) * half[0] + Math.abs(p[1]) * half[1] + Math.abs(p[2]) * half[2];
    const distance = dot(p, center) + p[3];
    if (distance - radius > epsilon + 0.0005) continue;
    if (!support?.length || support.some((v) => dot(p, v) + p[3] - 0.0002 <= epsilon + 0.0005)) return 1;
  }
  return 2;
}

function clippedSupport(triangles: number[][][], lo: number[], hi: number[]): number[][] {
  const points: number[][] = [];
  for (const triangle of triangles) {
    let polygon = triangle;
    for (let a = 0; a < 3 && polygon.length; a++)
      for (const sign of [-1, 1]) {
        const bound = sign < 0 ? lo[a] : hi[a],
          clipped: number[][] = [];
        for (let i = 0; i < polygon.length; i++) {
          const p = polygon[i],
            q = polygon[(i + 1) % polygon.length];
          const dp = (p[a] - bound) * sign,
            dq = (q[a] - bound) * sign;
          if (dp <= 0) clipped.push(p);
          if (dp <= 0 !== dq <= 0) clipped.push(p.map((v, axis) => v + ((q[axis] - v) * dp) / (dp - dq)));
        }
        polygon = clipped;
      }
    points.push(...polygon);
  }
  return points;
}

/** Compile a bounded receiver-space tree for one interpolation cell. A node
 * stores clear/blocked corner masks and eight children, or a short list of
 * (triangle, cornerMask) pairs. Ambiguous leaves retain exact segment tests;
 * budget overflow retains the existing BVH. No sampled visibility is trusted.
 * Offsets are vec4 addresses relative to the eventual cell payload. */
export function* indirectRegionSteps(
  geometry: IndirectGeometry,
  origin: Vec3,
  spacing: Vec3,
  candidates: number[],
  address: number,
  floatBudget: number,
  positions?: Vec3[],
  surfaceSupport = false,
): Generator<void, Float32Array | undefined> {
  const epsilon = Math.max(
    0.00002,
    Math.hypot(...geometry.bounds.max.map((v, a) => v - geometry.bounds.min[a])) * 4e-6,
  );
  const pad = Math.min(...spacing) * 0.02 + 0.0002 + epsilon;
  const probes =
    positions?.map((p) => p.map((v, a) => v - origin[a])) ??
    Array.from({ length: 8 }, (_, c) => spacing.map((v, a) => v * ((c >> a) & 1)));
  const cones: Cone[][] = [];
  const receiverTriangles: number[][][] = [];
  for (const index of candidates) {
    const t = geometry.triangles[geometry.order[index]];
    const vertices = [t.a, t.a.map((v, a) => v + t.ab[a]), t.a.map((v, a) => v + t.ac[a])].map((v) =>
      v.map((x, a) => x - origin[a]),
    );
    receiverTriangles.push(vertices);
    cones.push(probes.map((p) => shadowCone(vertices, p, epsilon)));
    if (cones.length % 32 === 0) yield;
  }
  const data = [0, 0, 0, 0];
  let overflow = false,
    visits = 0;
  function* node(
    index: number,
    lo: number[],
    size: number[],
    depth: number,
    visible: number,
    blocked: number,
    entries: [number, number][],
  ): Generator<void> {
    if (overflow) return;
    let center = lo.map((v, a) => v + size[a] / 2),
      half = size.map((v) => v / 2 + pad);
    // Static receivers lie on these clipped triangles, not anywhere in the
    // surrounding air. Keep the full normal-dependent interpolation padding;
    // only the much smaller ray-origin offset expands this surface support.
    const support =
      surfaceSupport && depth === 2
        ? clippedSupport(
            receiverTriangles,
            lo.map((v) => v - pad),
            lo.map((v, a) => v + size[a] + pad),
          )
        : undefined;
    if (support?.length) {
      const low = [Infinity, Infinity, Infinity],
        high = [-Infinity, -Infinity, -Infinity];
      for (const v of support)
        for (let a = 0; a < 3; a++) {
          low[a] = Math.min(low[a], v[a]);
          high[a] = Math.max(high[a], v[a]);
        }
      center = low.map((v, a) => (v + high[a]) / 2);
      half = low.map((v, a) => (high[a] - v) / 2 + 0.0002);
    }
    const uncertain: [number, number][] = [];
    let remaining = 0;
    for (const [triangle, mask] of entries) {
      let bits = mask & ~(visible | blocked),
        keep = 0;
      for (let corner = 0; corner < 8; corner++) {
        const bit = 1 << corner;
        if (!(bits & bit)) continue;
        const state = classify(cones[triangle][corner], center, half, epsilon, support);
        if (state === 2) blocked |= bit;
        else if (state === 1) keep |= bit;
      }
      if (keep) {
        uncertain.push([triangle, keep]);
        remaining |= keep;
      }
      if (++visits % 256 === 0) yield;
    }
    visible |= 255 & ~(blocked | remaining);
    const list = uncertain
      .map(([i, mask]) => [i, mask & ~blocked] as [number, number])
      .filter(([, mask]) => mask);
    data[index * 4] = visible;
    data[index * 4 + 1] = blocked;
    if (list.length > 8 && depth < 2) {
      const child = data.length / 4;
      data.push(...new Array(32).fill(0));
      data[index * 4 + 2] = address + child;
      for (let c = 0; c < 8; c++) {
        yield* node(
          child + c,
          lo.map((v, a) => v + (((c >> a) & 1) * size[a]) / 2),
          size.map((v) => v / 2),
          depth + 1,
          visible,
          blocked,
          list,
        );
      }
    } else if (list.length <= 32) {
      data[index * 4 + 2] = -(address + data.length / 4) - 1;
      data[index * 4 + 3] = list.length;
      for (const [i, mask] of list) data.push(candidates[i], mask);
      while (data.length % 4) data.push(0);
    } else data[index * 4 + 3] = -1;
    if (data.length > floatBudget) overflow = true;
  }
  yield* node(
    0,
    [0, 0, 0],
    spacing,
    0,
    0,
    0,
    candidates.map((_, i) => [i, 255]),
  );
  return overflow ? undefined : Float32Array.from(data);
}
