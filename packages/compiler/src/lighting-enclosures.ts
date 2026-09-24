import type { Vec3 } from "@wrela/model";
import { type IndirectGeometry, traceIndirectRay } from "./indirect-query";
export type LightingEnclosures = { components: Int32Array; closed: Set<number> };
/** Only edge-closed components qualify. Exact coordinate welding deliberately
 * declines nearly closed meshes: a crack must not become a sealed-room claim. */
export function* lightingEnclosureSteps(geometry: IndirectGeometry): Generator<void, LightingEnclosures> {
  const vertices = new Map<string, number>(),
    edges = new Map<string, number[]>();
  const parents = Int32Array.from({ length: geometry.triangles.length }, (_, i) => i);
  const root = (i: number): number => {
    while (parents[i] !== i) {
      parents[i] = parents[parents[i]];
      i = parents[i];
    }
    return i;
  };
  const vertex = (p: Vec3) => {
    const key = p.join(",");
    let id = vertices.get(key);
    if (id === undefined) {
      id = vertices.size;
      vertices.set(key, id);
    }
    return id;
  };
  for (let i = 0; i < geometry.triangles.length; i++) {
    const t = geometry.triangles[i],
      ids = [
        vertex(t.a),
        vertex(t.a.map((v, a) => v + t.ab[a]) as Vec3),
        vertex(t.a.map((v, a) => v + t.ac[a]) as Vec3),
      ];
    for (let j = 0; j < 3; j++) {
      const a = ids[j],
        b = ids[(j + 1) % 3],
        key = a < b ? `${a},${b}` : `${b},${a}`;
      const owners = edges.get(key);
      if (owners) {
        parents[root(i)] = root(owners[0]);
        owners.push(i);
      } else edges.set(key, [i]);
    }
    if (i % 512 === 511) yield;
  }
  const components = parents.map((_, i) => root(i)),
    closed = new Set(components);
  let work = 0;
  for (const owners of edges.values()) {
    if (owners.length !== 2) closed.delete(components[owners[0]]);
    if (++work % 1024 === 0) yield;
  }
  // Convexity makes a triangle-wide certificate possible from its corners.
  // Complex/non-convex shells keep ordinary shadow-map visibility.
  const groups = new Map<number, number[]>();
  for (let i = 0; i < components.length; i++)
    if (closed.has(components[i])) {
      const group = groups.get(components[i]) ?? [];
      group.push(i);
      groups.set(components[i], group);
    }
  for (const [component, group] of groups) {
    if (group.length > 256) {
      closed.delete(component);
      continue;
    }
    const points = group.flatMap((i) => {
      const t = geometry.triangles[i];
      return [t.a, t.a.map((v, a) => v + t.ab[a]) as Vec3, t.a.map((v, a) => v + t.ac[a]) as Vec3];
    });
    const center = [0, 1, 2].map((a) => points.reduce((sum, p) => sum + p[a], 0) / points.length);
    for (const i of group) {
      const t = geometry.triangles[i];
      const side = Math.sign(t.normal.reduce((sum, v, a) => sum + v * (center[a] - t.a[a]), 0));
      if (
        !side ||
        points.some((p) => t.normal.reduce((sum, v, a) => sum + v * (p[a] - t.a[a]), 0) * side < -1e-9)
      ) {
        closed.delete(component);
        break;
      }
      yield;
    }
  }
  return { components, closed };
}
/** Odd crossings of a watertight component place a point inside an opaque
 * enclosure. Ambiguous or excessive crossings decline the certificate. */
export function insideLightingEnclosure(
  geometry: IndirectGeometry,
  enclosures: LightingEnclosures,
  position: Vec3,
): number {
  if (!enclosures.closed.size) return 0;
  const direction: Vec3 = [0.8111071057, 0.3244428423, 0.4866642634];
  const counts = new Set<number>();
  let start = position,
    ignore = -1;
  for (let step = 0; step < 128; step++) {
    const hit = traceIndirectRay(geometry, start, direction, Infinity, ignore);
    if (!hit) return counts.size ? Math.min(...counts) + 1 : 0;
    const component = enclosures.components[hit.triangle];
    if (enclosures.closed.has(component)) {
      if (counts.has(component)) counts.delete(component);
      else counts.add(component);
    }
    start = hit.position.map((v, a) => v + direction[a] * 0.00001) as Vec3;
    ignore = hit.triangle;
  }
  return 0;
}
