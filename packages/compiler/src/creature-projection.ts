import {
  type CharacterDefinition,
  type CompiledCharacter,
  type CreatureGroom,
  contentKey,
  cross,
  dot,
  normalize,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import type { CreatureGeometry } from "./creature";
import { cachedCreatureProduct } from "./creature-cache";
import type { GroomChartSample } from "./groom";

export type CreatureRootProjection = {
  status: "resolved" | "ambiguous" | "no-hit" | "wrong-region";
  domain: "compiled-body";
  position?: Vec3;
  normal?: Vec3;
  distance?: number;
  reason?: string;
  sourceNode?: string;
  interval?: [number, number];
};
type Bounds = { min: Vec3; max: Vec3 };
type BVHNode = Bounds & { start: number; count: number; left: number; right: number };
type BodyProjectionProduct = {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  sourceIds: string[];
  regions: (string | null)[];
  order: Uint32Array;
  nodes: BVHNode[];
  epsilon: number;
};
const point = (positions: Float32Array, vertex: number): Vec3 => [
  positions[vertex * 3],
  positions[vertex * 3 + 1],
  positions[vertex * 3 + 2],
];

function canonicalTriangles(source: Uint32Array): Uint32Array {
  if (source.length % 3 || source.length > 1_500_000)
    throw new Error("Body projection exceeds the 500000 triangle budget or has invalid topology");
  const triangles: [number, number, number][] = [];
  for (let i = 0; i < source.length; i += 3) {
    const corners = [source[i], source[i + 1], source[i + 2]];
    const start = corners.indexOf(Math.min(...corners));
    triangles.push([corners[start], corners[(start + 1) % 3], corners[(start + 2) % 3]]);
  }
  triangles.sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]);
  const result = new Uint32Array(source.length);
  for (let i = 0; i < triangles.length; i++) result.set(triangles[i], i * 3);
  return result;
}

function buildBodyProjection(body: CreatureGeometry): BodyProjectionProduct {
  const mesh = body.mesh,
    count = mesh.indices.length / 3;
  if (count > 500_000 || !Number.isInteger(count))
    throw new Error("Body projection exceeds the 500000 triangle budget");
  const indices = canonicalTriangles(mesh.indices),
    order = Array.from({ length: count }, (_, index) => index),
    nodes: BVHNode[] = [],
    centers = new Float64Array(count * 3),
    bounds: Bounds[] = [];
  for (let triangle = 0; triangle < count; triangle++) {
    const min: Vec3 = [Infinity, Infinity, Infinity],
      max: Vec3 = [-Infinity, -Infinity, -Infinity];
    for (let corner = 0; corner < 3; corner++) {
      const p = point(mesh.positions, indices[triangle * 3 + corner]);
      for (let axis = 0; axis < 3; axis++) {
        if (!Number.isFinite(p[axis])) throw new Error("Projection body contains a non-finite vertex");
        min[axis] = Math.min(min[axis], p[axis]);
        max[axis] = Math.max(max[axis], p[axis]);
      }
    }
    bounds.push({ min, max });
    for (let axis = 0; axis < 3; axis++) centers[triangle * 3 + axis] = (min[axis] + max[axis]) * 0.5;
  }
  const build = (start: number, end: number): number => {
    const min: Vec3 = [Infinity, Infinity, Infinity],
      max: Vec3 = [-Infinity, -Infinity, -Infinity],
      centerMin: Vec3 = [Infinity, Infinity, Infinity],
      centerMax: Vec3 = [-Infinity, -Infinity, -Infinity];
    for (let i = start; i < end; i++)
      for (let axis = 0; axis < 3; axis++) {
        const triangle = order[i];
        min[axis] = Math.min(min[axis], bounds[triangle].min[axis]);
        max[axis] = Math.max(max[axis], bounds[triangle].max[axis]);
        centerMin[axis] = Math.min(centerMin[axis], centers[triangle * 3 + axis]);
        centerMax[axis] = Math.max(centerMax[axis], centers[triangle * 3 + axis]);
      }
    const index = nodes.length;
    nodes.push({ min, max, start, count: end - start, left: -1, right: -1 });
    if (end - start > 12) {
      const extents = centerMax.map((value, axis) => value - centerMin[axis]),
        axis = extents.indexOf(Math.max(...extents));
      const sorted = order
        .slice(start, end)
        .sort((a, b) => centers[a * 3 + axis] - centers[b * 3 + axis] || a - b);
      for (let i = 0; i < sorted.length; i++) order[start + i] = sorted[i];
      const middle = start + Math.floor((end - start) / 2);
      nodes[index].left = build(start, middle);
      nodes[index].right = build(middle, end);
      nodes[index].count = 0;
    }
    return index;
  };
  if (count) build(0, count);
  const diagonal = Math.hypot(...sub(mesh.bounds.max, mesh.bounds.min));
  return {
    positions: mesh.positions.slice(),
    normals: mesh.normals.slice(),
    indices,
    sourceIds: mesh.sourceIds
      ? [...mesh.sourceIds]
      : Array.from({ length: mesh.positions.length / 3 }, () => ""),
    regions: [...body.regions],
    order: new Uint32Array(order),
    nodes,
    epsilon: Math.max(1e-8, diagonal * 1e-7),
  };
}
function rayBounds(
  origin: Vec3,
  direction: Vec3,
  maxDistance: number,
  bounds: Bounds,
  epsilon: number,
): boolean {
  let lower = -epsilon,
    upper = maxDistance + epsilon;
  for (let axis = 0; axis < 3; axis++) {
    if (Math.abs(direction[axis]) < 1e-14) {
      if (origin[axis] < bounds.min[axis] - epsilon || origin[axis] > bounds.max[axis] + epsilon)
        return false;
      continue;
    }
    const a = (bounds.min[axis] - origin[axis] - epsilon) / direction[axis],
      b = (bounds.max[axis] - origin[axis] + epsilon) / direction[axis];
    lower = Math.max(lower, Math.min(a, b));
    upper = Math.min(upper, Math.max(a, b));
    if (lower > upper) return false;
  }
  return true;
}
function rayTriangle(
  product: BodyProjectionProduct,
  triangle: number,
  origin: Vec3,
  direction: Vec3,
  maxDistance: number,
) {
  const ia = product.indices[triangle * 3],
    ib = product.indices[triangle * 3 + 1],
    ic = product.indices[triangle * 3 + 2],
    a = point(product.positions, ia),
    b = point(product.positions, ib),
    c = point(product.positions, ic),
    ab = sub(b, a),
    ac = sub(c, a);
  const h = cross(direction, ac),
    det = dot(ab, h),
    scaleProduct = Math.hypot(...ab) * Math.hypot(...ac);
  if (scaleProduct < 1e-20 || Math.abs(det) < scaleProduct * 1e-10) return undefined;
  const inverse = 1 / det,
    s = sub(origin, a),
    u = dot(s, h) * inverse,
    q = cross(s, ab),
    v = dot(direction, q) * inverse;
  if (u < -1e-7 || v < -1e-7 || u + v > 1 + 1e-7) return undefined;
  const distance = dot(ac, q) * inverse;
  if (distance < -product.epsilon || distance > maxDistance) return undefined;
  const weights = [1 - u - v, u, v],
    vertices = [ia, ib, ic],
    normal: Vec3 = [0, 0, 0];
  for (let i = 0; i < 3; i++)
    for (let axis = 0; axis < 3; axis++) normal[axis] += product.normals[vertices[i] * 3 + axis] * weights[i];
  const normalized = normalize(normal),
    fallback = normalize(cross(ab, ac));
  return {
    distance: Math.max(0, distance),
    vertices,
    weights,
    normal: Math.hypot(...normalized) > 0.5 ? normalized : fallback,
  };
}

/** Reusable, byte-bounded BVH over the actual compiled, sculpted body. Rays use
 * physical metre lengths and triangle intersections; no field value is treated
 * as a distance. Projection stays inside authored anatomical/source ownership. */
export function creatureBodyProjectionKey(body: CreatureGeometry): string {
  // Material regrouping changes index order, but not the physical triangle set.
  // Preserve winding and normalize cyclic corner order before sorting identities.
  return contentKey({
    version: 2,
    positions: body.mesh.positions,
    normals: body.mesh.normals,
    triangles: canonicalTriangles(body.mesh.indices),
    sources: body.mesh.sourceIds,
    regions: body.regions,
    bounds: body.mesh.bounds,
  });
}

/** Public projection/review view of an artifact's body, without compiled coat. */
export function compiledCreatureBodyGeometry(artifact: CompiledCharacter): CreatureGeometry {
  const count = artifact.creatureBodyVertexCount;
  if (count === undefined || count < 0 || count > artifact.mesh.positions.length / 3)
    throw new Error("Artifact does not declare a valid compiled body prefix");
  const positions = artifact.mesh.positions.slice(0, count * 3),
    indices: number[] = [];
  for (let i = 0; i < artifact.mesh.indices.length; i += 3) {
    const a = artifact.mesh.indices[i],
      b = artifact.mesh.indices[i + 1],
      c = artifact.mesh.indices[i + 2];
    if (a < count && b < count && c < count) indices.push(a, b, c);
  }
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < positions.length; i++) {
    min[i % 3] = Math.min(min[i % 3], positions[i]);
    max[i % 3] = Math.max(max[i % 3], positions[i]);
  }
  if (!positions.length) {
    min.fill(0);
    max.fill(0);
  }
  return {
    key: `${artifact.key}-body`,
    mesh: {
      positions,
      normals: artifact.mesh.normals.slice(0, count * 3),
      indices: new Uint32Array(indices),
      sourceIds: artifact.mesh.sourceIds?.slice(0, count),
      bounds: { min, max },
    },
    coordinates: artifact.creatureCoordinates?.slice(0, count) ?? Array.from({ length: count }, () => null),
    regions: artifact.creatureRegions?.slice(0, count) ?? Array.from({ length: count }, () => null),
    diagnostics: [],
  };
}

export function createCreatureBodyProjector(document: CharacterDefinition, body: CreatureGeometry) {
  const geometryKey = creatureBodyProjectionKey(body);
  const product = cachedCreatureProduct("projection", geometryKey, () => buildBodyProjection(body));
  const metrics = {
    queries: 0,
    nodeVisits: 0,
    triangleTests: 0,
    resolved: 0,
    ambiguous: 0,
    noHit: 0,
    wrongRegion: 0,
  };
  const nodes = new Map(document.field.nodes.map((node) => [node.id, node])),
    fieldSources = new Set(
      document.field.nodes.filter((node) => node.children.length === 0).map((node) => node.id),
    );
  const expand = (ids: string[]) => {
    const result = new Set<string>(),
      stack = [...ids];
    let visits = 0;
    while (stack.length) {
      const id = stack.pop();
      if (!id || result.has(id)) continue;
      if (++visits > 512) throw new Error("Projection source scope exceeds field node budget");
      result.add(id);
      for (const child of nodes.get(id)?.children ?? []) stack.push(child);
    }
    return result;
  };
  const projectRoot = (groom: CreatureGroom, sample: GroomChartSample): CreatureRootProjection => {
    metrics.queries++;
    const finish = (status: CreatureRootProjection["status"], reason: string): CreatureRootProjection => {
      if (status === "ambiguous") metrics.ambiguous++;
      else if (status === "wrong-region") metrics.wrongRegion++;
      else metrics.noHit++;
      return { status, domain: "compiled-body", reason };
    };
    const settings = groom.rootProjection,
      region = document.creature?.regions.find((item) => item.id === groom.region);
    if (!settings || !region || sample.region !== groom.region)
      return finish(
        "wrong-region",
        "Projection requires an explicit existing anatomical region and matching chart ownership.",
      );
    if (
      !Number.isFinite(settings.maxDistance) ||
      settings.maxDistance <= 0 ||
      sample.position.some((value) => !Number.isFinite(value))
    )
      return finish("no-hit", "Projection requires finite positions and a positive physical distance bound.");
    const normal = normalize(sample.normal);
    if (normal.some((value) => !Number.isFinite(value)) || Math.hypot(...normal) < 0.5)
      return finish("no-hit", "Chart normal does not define a finite projection direction.");
    const allowed = expand(settings.nodeIds ?? region.nodeIds),
      signs = settings.direction === "both" ? [1, -1] : [1];
    type Hit = { distance: number; position: Vec3; normal: Vec3; sourceNode: string; eligible: boolean };
    const hits: Hit[] = [];
    let nodeVisits = 0,
      triangleTests = 0;
    for (const sign of signs) {
      const direction = scale(normal, sign),
        stack = product.nodes.length ? [0] : [];
      while (stack.length) {
        const index = stack.pop();
        if (index === undefined) break;
        metrics.nodeVisits++;
        if (++nodeVisits > 8192)
          return finish(
            "ambiguous",
            "Projection exhausted its 8192-node work budget; narrow the target scope or simplify overlapping geometry.",
          );
        const node = product.nodes[index];
        if (!rayBounds(sample.position, direction, settings.maxDistance, node, product.epsilon)) continue;
        if (node.left >= 0) {
          stack.push(node.left, node.right);
          continue;
        }
        for (let i = node.start; i < node.start + node.count; i++) {
          metrics.triangleTests++;
          if (++triangleTests > 8192)
            return finish(
              "ambiguous",
              "Projection exhausted its 8192-triangle work budget; no partial result was accepted.",
            );
          const hit = rayTriangle(
            product,
            product.order[i],
            sample.position,
            direction,
            settings.maxDistance,
          );
          if (!hit) continue;
          const sources = hit.vertices.map((vertex) => product.sourceIds[vertex]);
          // Chart-only support geometry is not an authored field-node target.
          if (!sources.every((source) => fieldSources.has(source))) continue;
          const eligible =
            hit.vertices.every((vertex) => product.regions[vertex] === groom.region) &&
            sources.every((source) => allowed.has(source));
          const sourceNode = sources[hit.weights.indexOf(Math.max(...hit.weights))];
          hits.push({
            distance: hit.distance,
            position: [
              sample.position[0] + direction[0] * hit.distance,
              sample.position[1] + direction[1] * hit.distance,
              sample.position[2] + direction[2] * hit.distance,
            ],
            normal: hit.normal,
            sourceNode,
            eligible,
          });
          if (hits.length > 256)
            return finish(
              "ambiguous",
              "Projection intersects more than 256 surfaces; explicit source repair is required.",
            );
        }
      }
    }
    hits.sort((a, b) => a.distance - b.distance);
    const eligible = hits.filter((hit) => hit.eligible),
      closest = eligible[0];
    if (!closest)
      return finish(
        hits.length ? "wrong-region" : "no-hit",
        hits.length
          ? "Intersected body triangles belong to a different region or undeclared source node."
          : "No declared anatomical body surface intersects the bounded chart-normal ray.",
      );
    if (hits.some((hit) => !hit.eligible && hit.distance < closest.distance - product.epsilon * 4))
      return finish(
        "wrong-region",
        "A different anatomical surface blocks the declared target; projection cannot cross it.",
      );
    const distinct = eligible.filter(
      (hit) =>
        Math.hypot(...sub(hit.position, closest.position)) > product.epsilon * 4 ||
        dot(hit.normal, closest.normal) < 0.95,
    );
    if (distinct.length)
      return finish(
        "ambiguous",
        "Multiple distinct body surfaces satisfy the declared ray and anatomical scope; choose a tighter bound or repair the chart.",
      );
    metrics.resolved++;
    return {
      status: "resolved",
      domain: "compiled-body",
      position: closest.position,
      normal: closest.normal,
      distance: closest.distance,
      sourceNode: closest.sourceNode,
      interval: [Math.max(0, closest.distance - product.epsilon), closest.distance + product.epsilon],
    };
  };
  return { key: geometryKey, projectRoot, metrics };
}
