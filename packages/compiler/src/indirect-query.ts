import { type Bounds, hash32, type RenderSurface, type Vec3 } from "@wrela/model";

/** Delta-sun normal-incidence irradiance and constant incoming sky radiance,
 * both scene-linear. This source deliberately excludes material/specular transport. */
export type IndirectLighting = { sunDirection: Vec3; sunRadiance: Vec3; skyRadiance: Vec3 };
type Triangle = {
  emission?: Vec3;
  a: Vec3;
  ab: Vec3;
  ac: Vec3;
  normal: Vec3;
  albedo: Vec3;
  min: Vec3;
  max: Vec3;
};
type Node = { min: Vec3; max: Vec3; start: number; count: number; left: number; right: number };
export type IndirectGeometry = {
  triangles: Triangle[];
  nodes: Node[];
  order: number[];
  bounds: Bounds;
  /** Transient query overlays share the immutable static BVH. Triangle IDs in
   * each layer are rebased into this geometry's concatenated triangle table. */
  layers?: { geometry: IndirectGeometry; offset: number }[];
  /** Immutable prefix shared by a transient layered query product. */
  staticGeometry?: IndirectGeometry;
  report: { triangles: number; nodes: number; excluded: { id: string; reason: string }[]; bytes: number };
};
export type IndirectHit = {
  emission?: Vec3;
  distance: number;
  position: Vec3;
  normal: Vec3;
  albedo: Vec3;
  triangle: number;
};
export const indirectDot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const indirectNormalize = (v: Vec3): Vec3 => {
  const l = Math.hypot(...v);
  return l > 1e-20 ? [v[0] / l, v[1] / l, v[2] / l] : [0, 1, 0];
};
export function indirectSurfaceExclusion(surface: RenderSurface): string | undefined {
  if (
    surface.lightingMobility === "dynamic" ||
    surface.skin ||
    surface.deformation ||
    surface.wind ||
    surface.mesh.wind
  )
    return "dynamic geometry excluded";
  if (surface.castsShadow === false) return "non-occluding surface excluded";
  if (surface.water) return "water transport excluded";
  // Alpha coverage/transmitting objects cannot silently become opaque blockers.
  if (
    surface.mesh.shoots ||
    "thinCoverage" in surface.mesh ||
    surface.material.appearance?.family === "foliage" ||
    surface.material.appearance?.family === "glass"
  )
    return "thin or transmitting geometry excluded";
  return;
}
/** Cooperative compiler: triangle extraction and BVH partitioning yield every
 * 1024 primitive visits. No source triangles are silently dropped at the budget. */
export function* indirectGeometrySteps(
  surfaces: readonly RenderSurface[],
  options: { maxTriangles?: number; origin?: Vec3 } = {},
): Generator<void, IndirectGeometry> {
  const limit = options.maxTriangles ?? 100_000;
  if (!Number.isInteger(limit) || limit < 1 || limit > 1_000_000)
    throw Error("Invalid indirect triangle budget");
  const triangles: Triangle[] = [],
    excluded: IndirectGeometry["report"]["excluded"] = [];
  let work = 0;
  for (const surface of surfaces) {
    const reason = indirectSurfaceExclusion(surface);
    if (reason) {
      excluded.push({ id: surface.id, reason });
      continue;
    }
    const { mesh, matrix: m, material } = surface;
    if (!m.every(Number.isFinite) || m[3] !== 0 || m[7] !== 0 || m[11] !== 0 || m[15] !== 1)
      throw Error("Indirect query requires finite affine transforms");
    const start = surface.drawRange?.start ?? 0,
      count = surface.drawRange?.count ?? mesh.indices.length;
    if (start % 3 || count % 3 || start < 0 || start + count > mesh.indices.length)
      throw Error("Invalid indirect material range");
    if (triangles.length + count / 3 > limit)
      throw Error(`Indirect geometry exceeds ${limit} triangle budget; whole build refused`);
    const point = (i: number): Vec3 => {
      const x = mesh.positions[i * 3],
        y = mesh.positions[i * 3 + 1],
        z = mesh.positions[i * 3 + 2];
      return [
        m[0] * x + m[4] * y + m[8] * z + m[12],
        m[1] * x + m[5] * y + m[9] * z + m[13],
        m[2] * x + m[6] * y + m[10] * z + m[14],
      ].map((v, axis) => v + (options.origin?.[axis] ?? 0)) as Vec3;
    };
    for (let i = start; i < start + count; i += 3) {
      const ids = [mesh.indices[i], mesh.indices[i + 1], mesh.indices[i + 2]],
        [a, b, c] = ids.map(point);
      if (![...a, ...b, ...c].every(Number.isFinite)) throw Error("Nonfinite indirect triangle");
      const ab = sub(b, a),
        ac = sub(c, a),
        n = cross(ab, ac);
      if (Math.hypot(...n) > 1e-12) {
        const colors = mesh.colors;
        const albedo = material.color.map((v, channel) =>
          Math.max(
            0,
            Math.min(
              0.95,
              v *
                (1 - material.metallic) *
                (colors ? ids.reduce((sum, id) => sum + colors[id * 3 + channel], 0) / 3 : 1),
            ),
          ),
        ) as Vec3;
        triangles.push({
          a,
          ab,
          ac,
          normal: indirectNormalize(n),
          albedo,
          emission: material.emission?.color.map((v) => v * material.emission!.intensity) as Vec3 | undefined,
          min: [0, 1, 2].map((axis) => Math.min(a[axis], b[axis], c[axis])) as Vec3,
          max: [0, 1, 2].map((axis) => Math.max(a[axis], b[axis], c[axis])) as Vec3,
        });
      }
      if (++work % 1024 === 0) yield;
    }
  }
  const nodes: Node[] = [],
    order = Array.from({ length: triangles.length }, (_, i) => i);
  const empty = (): Node => ({
    min: [Infinity, Infinity, Infinity],
    max: [-Infinity, -Infinity, -Infinity],
    start: 0,
    count: 0,
    left: -1,
    right: -1,
  });
  nodes.push(empty());
  const pending = [{ node: 0, start: 0, end: order.length }];
  while (pending.length) {
    const task = pending.pop();
    if (!task) break;
    const node = nodes[task.node];
    const centroidMin: Vec3 = [Infinity, Infinity, Infinity],
      centroidMax: Vec3 = [-Infinity, -Infinity, -Infinity];
    for (let i = task.start; i < task.end; i++) {
      const t = triangles[order[i]];
      for (let axis = 0; axis < 3; axis++) {
        node.min[axis] = Math.min(node.min[axis], t.min[axis]);
        node.max[axis] = Math.max(node.max[axis], t.max[axis]);
        const center = (t.min[axis] + t.max[axis]) * 0.5;
        centroidMin[axis] = Math.min(centroidMin[axis], center);
        centroidMax[axis] = Math.max(centroidMax[axis], center);
      }
      if (++work % 1024 === 0) yield;
    }
    node.start = task.start;
    node.count = task.end - task.start;
    if (node.count <= 6) continue;
    let axis = 0;
    for (let a = 1; a < 3; a++)
      if (centroidMax[a] - centroidMin[a] > centroidMax[axis] - centroidMin[axis]) axis = a;
    const split = (centroidMin[axis] + centroidMax[axis]) * 0.5;
    let low = task.start,
      high = task.end - 1;
    while (low <= high) {
      const t = triangles[order[low]];
      if ((t.min[axis] + t.max[axis]) * 0.5 < split) low++;
      else {
        [order[low], order[high]] = [order[high], order[low]];
        high--;
      }
      if (++work % 1024 === 0) yield;
    }
    const middle = low === task.start || low === task.end ? Math.floor((task.start + task.end) / 2) : low;
    node.left = nodes.length;
    nodes.push(empty());
    node.right = nodes.length;
    nodes.push(empty());
    node.count = 0;
    pending.push(
      { node: node.right, start: middle, end: task.end },
      { node: node.left, start: task.start, end: middle },
    );
  }
  const bounds = triangles.length
    ? { min: nodes[0].min, max: nodes[0].max }
    : { min: [0, 0, 0] as Vec3, max: [0, 0, 0] as Vec3 };
  return {
    triangles,
    nodes,
    order,
    bounds,
    report: {
      triangles: triangles.length,
      nodes: nodes.length,
      excluded,
      // Reserve all eight vec3 lanes, including optional emitted radiance.
      // Restoration uses the same numeric-storage estimate.
      bytes: triangles.length * 24 * 8 + nodes.length * 10 * 8 + order.length * 8,
    },
  };
}
export function compileIndirectGeometry(
  surfaces: readonly RenderSurface[],
  options: { maxTriangles?: number; origin?: Vec3 } = {},
): IndirectGeometry {
  const steps = indirectGeometrySteps(surfaces, options);
  let item = steps.next();
  while (!item.done) item = steps.next();
  return item.value;
}
export function traceIndirectRay(
  geometry: IndirectGeometry,
  origin: Vec3,
  direction: Vec3,
  maxDistance = Infinity,
  ignoreTriangle = -1,
): IndirectHit | undefined {
  let closest = maxDistance,
    triangle = -1;
  const stack = [0];
  while (stack.length) {
    const nodeIndex = stack.pop();
    if (nodeIndex === undefined) break;
    const node = geometry.nodes[nodeIndex];
    let lo = 1e-5,
      hi = closest;
    for (let axis = 0; axis < 3; axis++) {
      if (Math.abs(direction[axis]) < 1e-15) {
        if (origin[axis] < node.min[axis] || origin[axis] > node.max[axis]) hi = -1;
      } else {
        const a = (node.min[axis] - origin[axis]) / direction[axis],
          b = (node.max[axis] - origin[axis]) / direction[axis];
        lo = Math.max(lo, Math.min(a, b));
        hi = Math.min(hi, Math.max(a, b));
      }
    }
    if (hi < lo) continue;
    if (node.count === 0 && node.left >= 0) {
      stack.push(node.left, node.right);
      continue;
    }
    for (let i = node.start; i < node.start + node.count; i++) {
      const index = geometry.order[i];
      if (index === ignoreTriangle) continue;
      const t = geometry.triangles[index],
        b = t.ab,
        c = t.ac;
      // Millions of compiler transport rays visit this loop. Scalar
      // intersection keeps the same arithmetic without per-triangle vectors.
      const px = direction[1] * c[2] - direction[2] * c[1],
        py = direction[2] * c[0] - direction[0] * c[2],
        pz = direction[0] * c[1] - direction[1] * c[0];
      const determinant = b[0] * px + b[1] * py + b[2] * pz;
      if (Math.abs(determinant) < 1e-14) continue;
      const x = origin[0] - t.a[0],
        y = origin[1] - t.a[1],
        z = origin[2] - t.a[2];
      const u = (x * px + y * py + z * pz) / determinant;
      if (u < -1e-9 || u > 1 + 1e-9) continue;
      const qx = y * b[2] - z * b[1],
        qy = z * b[0] - x * b[2],
        qz = x * b[1] - y * b[0];
      const v = (direction[0] * qx + direction[1] * qy + direction[2] * qz) / determinant;
      if (v < -1e-9 || u + v > 1 + 1e-9) continue;
      const distance = (c[0] * qx + c[1] * qy + c[2] * qz) / determinant;
      if (distance > 1e-5 && distance < closest) {
        closest = distance;
        triangle = index;
      }
    }
  }
  let layeredHit: IndirectHit | undefined;
  if (geometry.layers)
    for (const layer of geometry.layers) {
      const hit = traceIndirectRay(layer.geometry, origin, direction, closest, ignoreTriangle - layer.offset);
      if (hit) {
        closest = hit.distance;
        layeredHit = { ...hit, triangle: hit.triangle + layer.offset };
      }
    }
  if (layeredHit) return layeredHit;
  if (triangle < 0) return;
  const t = geometry.triangles[triangle],
    sign = indirectDot(t.normal, direction) > 0 ? -1 : 1;
  return {
    distance: closest,
    triangle,
    position: origin.map((v, i) => v + direction[i] * closest) as Vec3,
    normal: t.normal.map((v) => v * sign) as Vec3,
    albedo: t.albedo,
    emission: t.emission,
  };
}
/** Visibility-only query: no hit shading data, and any blocker inside the
 * opaque radius is sufficient. The fade annulus still uses the closest hit. */
export function traceSkyDistance(
  geometry: IndirectGeometry,
  origin: Vec3,
  direction: Vec3,
  radius: number,
): number {
  let closest = radius;
  const stack = [0];
  while (stack.length) {
    const index = stack.pop();
    if (index === undefined) break;
    const node = geometry.nodes[index];
    let lo = 1e-5,
      hi = closest;
    for (let a = 0; a < 3; a++) {
      if (Math.abs(direction[a]) < 1e-15) {
        if (origin[a] < node.min[a] || origin[a] > node.max[a]) hi = -1;
      } else {
        const x = (node.min[a] - origin[a]) / direction[a],
          y = (node.max[a] - origin[a]) / direction[a];
        lo = Math.max(lo, Math.min(x, y));
        hi = Math.min(hi, Math.max(x, y));
      }
    }
    if (hi < lo) continue;
    if (!node.count && node.left >= 0) {
      stack.push(node.left, node.right);
      continue;
    }
    for (let i = node.start; i < node.start + node.count; i++) {
      const t = geometry.triangles[geometry.order[i]],
        b = t.ab,
        c = t.ac;
      const px = direction[1] * c[2] - direction[2] * c[1],
        py = direction[2] * c[0] - direction[0] * c[2],
        pz = direction[0] * c[1] - direction[1] * c[0];
      const determinant = b[0] * px + b[1] * py + b[2] * pz;
      if (Math.abs(determinant) < 1e-14) continue;
      const x = origin[0] - t.a[0],
        y = origin[1] - t.a[1],
        z = origin[2] - t.a[2];
      const u = (x * px + y * py + z * pz) / determinant;
      if (u < -1e-9 || u > 1 + 1e-9) continue;
      const qx = y * b[2] - z * b[1],
        qy = z * b[0] - x * b[2],
        qz = x * b[1] - y * b[0];
      const v = (direction[0] * qx + direction[1] * qy + direction[2] * qz) / determinant;
      if (v < -1e-9 || u + v > 1 + 1e-9) continue;
      const distance = (c[0] * qx + c[1] * qy + c[2] * qz) / determinant;
      if (distance > 1e-5 && distance < closest) {
        if (distance <= radius * 0.75) return 0;
        closest = distance;
      }
    }
  }
  if (geometry.layers)
    for (const layer of geometry.layers)
      closest = Math.min(closest, traceSkyDistance(layer.geometry, origin, direction, radius));
  return closest;
}
export function indirectHemisphere(normal: Vec3, u: number, v: number): Vec3 {
  const tangent = indirectNormalize(cross(Math.abs(normal[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0], normal)),
    bitangent = cross(normal, tangent);
  const r = Math.sqrt(u),
    phi = 2 * Math.PI * v,
    z = Math.sqrt(1 - u);
  return [0, 1, 2].map(
    (i) => tangent[i] * r * Math.cos(phi) + bitangent[i] * r * Math.sin(phi) + normal[i] * z,
  ) as Vec3;
}
export function indirectOffset(position: Vec3, normal: Vec3): Vec3 {
  return position.map((v, i) => v + normal[i] * 0.0002) as Vec3;
}
/** Radiance leaving the first diffuse hit. Secondary visibility rays terminate
 * at sky/sun; there is no recursive geometry bounce. */
export function indirectHitRadiance(
  geometry: IndirectGeometry,
  hit: IndirectHit,
  lighting: IndirectLighting,
  skySamples = 8,
  phase = 0,
  transfer?: { sky: (direction: Vec3, weight: number) => void; sun: (weight: number) => void },
): Vec3 {
  const origin = indirectOffset(hit.position, hit.normal),
    nl = Math.max(0, indirectDot(hit.normal, lighting.sunDirection));
  const sun =
    nl > 0 && !traceIndirectRay(geometry, origin, lighting.sunDirection, Infinity, hit.triangle)
      ? nl / Math.PI
      : 0;
  let sky = 0;
  transfer?.sun(sun);
  if (transfer || lighting.skyRadiance.some((value) => value > 0))
    for (let i = 0; i < skySamples; i++) {
      const direction = indirectHemisphere(
        hit.normal,
        (i + 0.5) / skySamples,
        (i * 0.61803398875 + phase) % 1,
      );
      if (!traceIndirectRay(geometry, origin, direction, Infinity, hit.triangle)) {
        sky += 1 / skySamples;
        transfer?.sky(direction, 1 / skySamples);
      }
    }
  return hit.albedo.map(
    (albedo, i) => albedo * (lighting.sunRadiance[i] * sun + lighting.skyRadiance[i] * sky),
  ) as Vec3;
}

/** Bounded diffuse path transport. Secondary paths have one continuation per
 * depth, so build work grows linearly with the bounce budget. RGB throughput is
 * recorded into the incident-light basis, including every reflecting material.
 * Runtime relighting does not trace these paths again. */
export function indirectHitTransport(
  geometry: IndirectGeometry,
  first: IndirectHit,
  lighting: IndirectLighting,
  samples: number,
  phase: number,
  bounces: 1 | 2 | 3,
  transfer?: { sky: (direction: Vec3, weight: Vec3) => void; sun: (weight: Vec3) => void },
): Vec3 {
  const result: Vec3 = [0, 0, 0];
  const sun = (hit: IndirectHit, throughput: Vec3) => {
    const nl = Math.max(0, indirectDot(hit.normal, lighting.sunDirection));
    if (
      nl === 0 ||
      traceIndirectRay(
        geometry,
        indirectOffset(hit.position, hit.normal),
        lighting.sunDirection,
        Infinity,
        hit.triangle,
      )
    )
      return;
    const weight = throughput.map((v) => (v * nl) / Math.PI) as Vec3;
    transfer?.sun(weight);
    for (let c = 0; c < 3; c++) result[c] += weight[c] * lighting.sunRadiance[c];
  };
  sun(first, first.albedo);
  for (let path = 0; path < samples; path++) {
    let hit = first;
    const throughput = first.albedo.map((v) => v / samples) as Vec3;
    let random = hash32(Math.floor(phase * 4294967296) ^ path);
    for (let depth = 0; depth < bounces; depth++) {
      const u = depth === 0 ? (path + 0.5) / samples : (random + 0.5) / 4294967296;
      random = hash32(random + 0x9e3779b9);
      const v = depth === 0 ? (path * 0.61803398875 + phase) % 1 : (random + 0.5) / 4294967296;
      random = hash32(random + 0x9e3779b9);
      const ray = indirectHemisphere(hit.normal, u, v);
      const next = traceIndirectRay(
        geometry,
        indirectOffset(hit.position, hit.normal),
        ray,
        Infinity,
        hit.triangle,
      );
      if (!next) {
        transfer?.sky(ray, throughput);
        for (let c = 0; c < 3; c++) result[c] += throughput[c] * lighting.skyRadiance[c];
        break;
      }
      if (depth + 1 === bounces) break;
      hit = next;
      for (let c = 0; c < 3; c++) throughput[c] *= hit.albedo[c];
      sun(hit, throughput);
    }
  }
  return result;
}
