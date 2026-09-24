import type { Vec3 } from "@wrela/model";
import type { IndirectGeometry } from "./indirect-query";
import type { RadianceEmitters } from "./radiance-emission";
import { emissionBoxOccluded } from "./radiance-local-emission";

type Plane = [number, number, number, number];
const dot = (a: readonly number[], b: readonly number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];

function clip(polygon: Vec3[], plane: Plane, sign = 1): Vec3[] {
  const result: Vec3[] = [];
  for (let i = 0; i < polygon.length; i++) {
    const a = polygon[i],
      b = polygon[(i + 1) % polygon.length];
    const da = (dot(a, plane) + plane[3]) * sign,
      db = (dot(b, plane) + plane[3]) * sign;
    if (da >= 0) result.push(a);
    if ((da > 0 && db < 0) || (da < 0 && db > 0)) {
      const t = da / (da - db);
      result.push(a.map((v, axis) => v + (b[axis] - v) * t) as Vec3);
    }
  }
  return result;
}

/** Irradiance / pi of a unit-radiance convex polygon. The boundary integral
 * of the projected solid angle is exact, including clipping at the horizon.
 * Vertices are relative to the receiver; winding does not change emission. */
export function polygonIrradiance(vertices: Vec3[], normal: Vec3): number | undefined {
  const polygon = clip(vertices, [...normal, 0]);
  if (polygon.length < 3) return 0;
  const directions: Vec3[] = [];
  for (const p of polygon) {
    const length = Math.hypot(...p);
    if (length < 1e-10) return;
    directions.push(p.map((v) => v / length) as Vec3);
  }
  let sum = 0;
  for (let i = 0; i < directions.length; i++) {
    const a = directions[i],
      b = directions[(i + 1) % directions.length];
    const axis = cross(a, b),
      sine = Math.hypot(...axis),
      cosine = dot(a, b);
    if (sine < 1e-12) {
      if (cosine < 0) return;
      continue;
    }
    sum += (dot(normal, axis) * Math.atan2(sine, cosine)) / sine;
  }
  return Math.min(1, Math.abs(sum) / (2 * Math.PI));
}

/** Four half-spaces beyond a triangle, in receiver-relative coordinates. */
function cone(vertices: Vec3[]): Plane[] | undefined {
  const [a, b, c] = vertices;
  const face = cross(b.map((v, i) => v - a[i]) as Vec3, c.map((v, i) => v - a[i]) as Vec3);
  const size = Math.hypot(...face);
  if (size < 1e-14) return;
  const distance = dot(face, a) / size;
  if (Math.abs(distance) < 1e-9) return;
  const n = face.map((v) => (v * Math.sign(distance)) / size) as Vec3;
  const result: Plane[] = [[...n, -Math.abs(distance)]];
  for (let i = 0; i < 3; i++) {
    const edge = cross(vertices[i], vertices[(i + 1) % 3]);
    const length = Math.hypot(...edge),
      direction = Math.sign(dot(edge, vertices[(i + 2) % 3]));
    if (length < 1e-14 || !direction) return;
    result.push([
      (edge[0] * direction) / length,
      (edge[1] * direction) / length,
      (edge[2] * direction) / length,
      0,
    ]);
  }
  return result;
}

/** Research candidate: integrate only after subtracting every opaque triangle's
 * shadow from each source polygon. Complexity exhaustion is undefined, never
 * an unshadowed answer. No result from a partially processed source is returned. */
export function polygonEmission(
  geometry: IndirectGeometry,
  emitters: RadianceEmitters,
  position: Vec3,
  normal: Vec3,
): { value: Vec3; candidates: number; polygons: number } | undefined {
  if (geometry.layers?.length || emitters.entries.length > 32) return;
  const value: Vec3 = [0, 0, 0];
  if (!emitters.entries.length) return { value, candidates: 0, polygons: 0 };
  const farthest = position.map((v, a) => (normal[a] >= 0 ? emitters.max[a] : emitters.min[a]) - v) as Vec3;
  if (dot(farthest, normal) <= 1e-7 || emissionBoxOccluded(geometry, emitters, position).blocked)
    return { value, candidates: 0, polygons: 0 };
  let candidates = 0,
    polygons = 0;
  const vertices = (id: number): Vec3[] => {
    const t = geometry.triangles[id];
    const a = t.a.map((v, i) => v - position[i]) as Vec3;
    return [a, a.map((v, i) => v + t.ab[i]) as Vec3, a.map((v, i) => v + t.ac[i]) as Vec3];
  };
  for (const entry of emitters.entries) {
    const source = vertices(entry.triangle);
    const horizon = clip(source, [...normal, 0]);
    if (horizon.length < 3) continue;
    const sourceCone = cone(source);
    if (!sourceCone) return;
    // The rays occupy the cone before, rather than beyond, the emitting plane.
    const volume = [sourceCone[0].map((v) => -v) as Plane, ...sourceCone.slice(1)];
    const intersects = (min: Vec3, max: Vec3) =>
      volume.every(
        (p) =>
          p[3] +
            p.slice(0, 3).reduce((sum, n, a) => sum + n * ((n >= 0 ? max[a] : min[a]) - position[a]), 0) >=
          -1e-9,
      );
    const pending = geometry.nodes.length ? [0] : [];
    let pieces = [horizon],
      visits = 0,
      sourceCandidates = 0;
    while (pending.length && pieces.length) {
      if (++visits > 512) return;
      const node = geometry.nodes[pending.pop() ?? 0];
      if (!intersects(node.min, node.max)) continue;
      if (!node.count && node.left >= 0) {
        pending.push(node.left, node.right);
        continue;
      }
      for (let j = node.start; j < node.start + node.count && pieces.length; j++) {
        const id = geometry.order[j],
          triangle = geometry.triangles[id];
        if (id === entry.triangle || !intersects(triangle.min, triangle.max)) continue;
        if (++sourceCandidates > 256) return;
        candidates++;
        const shadow = cone(vertices(id));
        if (!shadow) return;
        // Coincident source faces do not shadow one another. This also avoids
        // subtracting emitters from themselves through floating-point residue.
        if (horizon.every((p) => dot(shadow[0], p) + shadow[0][3] <= 1e-9)) continue;
        const next: Vec3[][] = [];
        for (const piece of pieces) {
          if (shadow.some((plane) => piece.every((p) => dot(plane, p) + plane[3] <= 0))) {
            next.push(piece);
            continue;
          }
          let remaining = piece;
          for (const plane of shadow) {
            const outside = clip(remaining, plane, -1);
            if (outside.length >= 3) next.push(outside);
            remaining = clip(remaining, plane);
            if (remaining.length < 3) break;
          }
        }
        if (next.length > 64 || next.some((p) => p.length > 32)) return;
        pieces = next;
      }
    }
    for (const piece of pieces) {
      const factor = polygonIrradiance(piece, normal);
      if (factor === undefined) return;
      const emission = geometry.triangles[entry.triangle].emission;
      if (!emission) return;
      for (let c = 0; c < 3; c++) value[c] += emission[c] * factor;
    }
    polygons += pieces.length;
  }
  return { value, candidates, polygons };
}
