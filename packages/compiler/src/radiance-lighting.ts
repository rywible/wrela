import {
  inverseMatrix,
  type MeshData,
  normalize,
  type RadianceLightingField,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";
import {
  type IndirectGeometry,
  indirectDot,
  indirectGeometrySteps,
  traceSkyDistance,
} from "./indirect-query";
import {
  insideLightingEnclosure,
  type LightingEnclosures,
  lightingEnclosureSteps,
} from "./lighting-enclosures";
import { radianceEmitterSteps } from "./radiance-emission";
import { localEmissionSteps } from "./radiance-local-emission";
import { radianceSamplePositions } from "./radiance-placement";
import { compactRadianceReceiverTable } from "./radiance-receiver-table";
import { type RadianceReceiver, radianceFlatNormals, radianceReceiverMesh } from "./radiance-receivers";
import { RadianceSampleIndex } from "./radiance-sample-index";
import { compileRadianceSurfaceCacheSteps } from "./radiance-surface-cache";
import { traceRadianceSampleSteps } from "./radiance-transport";
import { skyReceiverExclusion } from "./sky-visibility";

export type RadianceLightingProduct = {
  field: RadianceLightingField;
  meshes: Map<string, MeshData>;
  geometry: IndirectGeometry;
  enclosures: LightingEnclosures;
  raysPerSample: number;
  receivers: Map<string, RadianceReceiver>;
  fallbacks: Map<string, [number, number, number, number]>;
};
const add = (a: Vec3, b: Vec3, scale = 1): Vec3 => a.map((v, i) => v + b[i] * scale) as Vec3;
const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));

/** Exact segments reject samples on the other side of a wall. The spatial
 * radius bounds interpolation error as well as the runtime query budget. */
function visibleRadianceSamples(
  geometry: IndirectGeometry,
  positions: readonly Vec3[],
  position: Vec3,
  wanted: number,
  index?: RadianceSampleIndex,
): number[] {
  if (index && index.positions !== positions) throw Error("Radiance index belongs to different samples");
  if (index?.visibility && index.visibility.geometry !== geometry)
    throw Error("Radiance index belongs to different geometry");
  const visible = (p: Vec3, ray: Vec3, limit: number, id: number) =>
    index?.visibility
      ? index.visibility.visible(p, ray, limit, id)
      : traceSkyDistance(geometry, p, ray, limit) >= limit;
  // Keep only the bounded nearest set. Avoid allocating and sorting every
  // probe for every receiver vertex (millions of temporary objects in a world).
  const candidates: { i: number; d: number }[] = index?.nearest(position, 16) ?? [];
  for (let i = 0; !index && i < positions.length; i++) {
    const p = positions[i],
      x = p[0] - position[0],
      y = p[1] - position[1],
      z = p[2] - position[2];
    const squared = x * x + y * y + z * z;
    if (squared > 576 || (candidates.length === 16 && squared >= candidates[15].d)) continue;
    let at = candidates.length;
    while (at > 0 && squared < candidates[at - 1].d) at--;
    candidates.splice(at, 0, { i, d: squared });
    if (candidates.length > 16) candidates.pop();
  }
  const result: number[] = [];
  for (const c of candidates) {
    const d = Math.sqrt(c.d),
      p = positions[c.i];
    const ray: Vec3 = [
      (p[0] - position[0]) / Math.max(d, 1e-9),
      (p[1] - position[1]) / Math.max(d, 1e-9),
      (p[2] - position[2]) / Math.max(d, 1e-9),
    ];
    // Receiver admission needs only a blocked/clear segment, not the nearest
    // hit position, normal and material. The visibility query can terminate early.
    if (d < 0.004 || visible(position, ray, d - 0.002, c.i)) result.push(c.i + 1);
    if (result.length === wanted) break;
  }
  // Dense neighboring solids can occupy every entry in the short list. A
  // blocked near set is not proof that this receiver has no visible sample.
  // Keep the common fast path; only a miss gets the expanded 64-sample search.
  // This remains a bounded coverage heuristic, not a no-visible-sample proof.
  if (!result.length && candidates.length === 16) {
    const tested = new Set(candidates.map((c) => c.i));
    const remaining = index
      ? index.nearest(position, 64).slice(16)
      : positions
          .flatMap((p, i) => {
            if (tested.has(i)) return [];
            const d = p.reduce((sum, v, a) => sum + (v - position[a]) ** 2, 0);
            return d <= 576 ? [{ i, d }] : [];
          })
          .sort((a, b) => a.d - b.d || a.i - b.i)
          .slice(0, 48);
    for (const c of remaining) {
      const d = Math.sqrt(c.d),
        limit = d - 0.002;
      const ray = positions[c.i].map((v, a) => (v - position[a]) / Math.max(d, 1e-9)) as Vec3;
      if (d < 0.004 || visible(position, ray, limit, c.i)) result.push(c.i + 1);
      if (result.length === wanted) break;
    }
  }
  return result;
}

export function radianceReceiverSamples(
  geometry: IndirectGeometry,
  positions: readonly Vec3[],
  position: Vec3,
  index?: RadianceSampleIndex,
): [number, number] {
  const result = visibleRadianceSamples(geometry, positions, position, 2, index);
  return [result[0] ?? 0, result[1] ?? result[0] ?? 0];
}

/** Compactly supported weights approach zero where a neighboring sample enters
 * or leaves the nearest set. Every contributing segment remains visibility-tested. */
export function radianceReceiverMixture(
  geometry: IndirectGeometry,
  positions: readonly Vec3[],
  position: Vec3,
  index?: RadianceSampleIndex,
): number[] {
  const visible = visibleRadianceSamples(geometry, positions, position, 5, index);
  if (!visible.length) return [0, 0, 0, 0, 0, 0, 0, 0];
  const lengths = visible.map((id) => distance(positions[id - 1], position));
  const support = Math.max(0.01, lengths[lengths.length - 1] * 1.0001);
  const weights = lengths
    .slice(0, 4)
    .map((d) => (1 - Math.min(1, d / support)) ** 2 / Math.max(0.0001, d * d));
  let total = weights.reduce((sum, w) => sum + w, 0);
  if (total < 1e-15) {
    weights.fill(1);
    total = weights.length;
  }
  return [
    ...Array.from({ length: 4 }, (_, i) => visible[i] ?? 0),
    ...Array.from({ length: 4 }, (_, i) => (weights[i] ?? 0) / total),
  ];
}

/** The second ID's fractional lane carries the first sample's inverse-distance
 * weight. Zero fractional part retains the historical equal-weight encoding. */
export function radianceReceiverBinding(
  geometry: IndirectGeometry,
  positions: readonly Vec3[],
  position: Vec3,
  index?: RadianceSampleIndex,
): [number, number] {
  const pair = radianceReceiverSamples(geometry, positions, position, index);
  if (!pair[0] || pair[0] === pair[1]) return pair;
  const squared = (id: number) => positions[id - 1].reduce((sum, v, a) => sum + (v - position[a]) ** 2, 0);
  const a = squared(pair[0]),
    b = squared(pair[1]);
  return [pair[0], pair[1] + 0.125 + (0.25 * b) / Math.max(a + b, 1e-12)];
}

/** Compile diffuse transport into a radiometric basis. No sky color or light
 * intensity is baked. Sky rays use the entire resident geometry, so a large
 * closed cave cannot become sky-lit at an arbitrary short ray distance. */
export function* compileRadianceLightingSteps(
  surfaces: readonly RenderSurface[],
  options: {
    origin?: Vec3;
    center: Vec3;
    lights?: RadianceLightingField["lights"];
    key: string;
    maxSamples?: number;
    raysPerSample?: number;
    /** Caller certifies identical immutable source geometry/materials in absolute coordinates. */
    reuse?: RadianceLightingProduct;
  },
): Generator<void, RadianceLightingProduct> {
  const started = performance.now(),
    origin = options.origin ?? [0, 0, 0];
  const geometry =
    options.reuse?.geometry ?? (yield* indirectGeometrySteps(surfaces, { origin, maxTriangles: 200000 }));
  const enclosures = options.reuse?.enclosures ?? (yield* lightingEnclosureSteps(geometry));
  const budget = options.maxSamples ?? 192;
  let rayCount = options.raysPerSample ?? 64;
  if (
    !Number.isInteger(budget) ||
    budget < 2 ||
    budget > 512 ||
    !Number.isInteger(rayCount) ||
    rayCount < 16 ||
    rayCount > 1024
  )
    throw Error("Invalid radiance sampling budget");
  const positions = yield* radianceSamplePositions(geometry, options.center, budget);
  const sampleIndex = new RadianceSampleIndex(positions, geometry);
  if (options.raysPerSample === undefined && positions.length)
    rayCount = Math.min(256, Math.max(64, Math.floor((192 * 64) / positions.length / 16) * 16));
  const enclosed = new Uint32Array(positions.length);
  for (let i = 0; i < positions.length; i++) {
    enclosed[i] = insideLightingEnclosure(geometry, enclosures, positions[i]);
    if (i % 16 === 15) yield;
  }
  const lights = (options.lights ?? []).slice(0, 8).map((l) => ({ ...l, position: [...l.position] as Vec3 }));
  const transfer = new Float32Array(positions.length * 9 * 27 * 4),
    skyVisibility = new Float32Array(positions.length * 9),
    directEmission = new Float32Array(positions.length * 9 * 3);
  const reusable =
    options.reuse?.field.directEmission &&
    options.reuse.raysPerSample === rayCount &&
    JSON.stringify(options.reuse.field.lights) === JSON.stringify(lights)
      ? options.reuse
      : undefined;
  const oldSamples = new Map(reusable?.field.positions.map((p, i) => [p.join(","), i]));
  let rays = 0,
    reusedSamples = 0;
  for (let sample = 0; sample < positions.length; sample++) {
    const old = oldSamples.get(positions[sample].join(","));
    if (reusable?.field.directEmission && old !== undefined) {
      transfer.set(
        reusable.field.transfer.subarray(old * 9 * 27 * 4, (old + 1) * 9 * 27 * 4),
        sample * 9 * 27 * 4,
      );
      skyVisibility.set(reusable.field.skyVisibility.subarray(old * 9, (old + 1) * 9), sample * 9);
      directEmission.set(reusable.field.directEmission.subarray(old * 27, (old + 1) * 27), sample * 27);
      reusedSamples++;
      yield;
      continue;
    }
    rays += yield* traceRadianceSampleSteps(
      geometry,
      positions[sample],
      sample,
      rayCount,
      lights,
      transfer,
      skyVisibility,
      undefined,
      directEmission,
    );
  }

  const meshes = new Map<string, MeshData>(),
    receivers = new Map<string, RadianceReceiver>();
  const fallbacks = new Map<string, [number, number, number, number]>();
  const mixtures: number[] = [];
  const emitters = yield* radianceEmitterSteps(geometry);
  const hasEmission = emitters.entries.length > 0;
  const receiverEmission: number[] = [];
  let reusedEmissionReceivers = 0;
  function* binding(
    position: Vec3,
    normal: Vec3,
    pairOnly: boolean,
    reusableEmission?: Vec3,
  ): Generator<void, [number, number]> {
    let record: number[];
    if (pairOnly) {
      const pair = radianceReceiverBinding(geometry, positions, position, sampleIndex),
        fraction = pair[1] % 1;
      const weight = fraction >= 0.125 ? (fraction - 0.125) * 4 : 0.5;
      record = [pair[0], Math.floor(pair[1]), 0, 0, weight, 1 - weight, 0, 0];
    } else record = radianceReceiverMixture(geometry, positions, position, sampleIndex);
    if (!record[0]) return [0, 0];
    const id = 1024 + mixtures.length / 8;
    mixtures.push(...record);
    if (hasEmission) {
      const local = reusableEmission
        ? { value: reusableEmission, rays: 0 }
        : yield* localEmissionSteps(geometry, emitters, position, normal);
      if (reusableEmission) reusedEmissionReceivers++;
      rays += local.rays;
      receiverEmission.push(...local.value);
    }
    return [id, 0];
  }
  let vertices = 0,
    unmappedVertices = 0,
    vertexBytes = 0;
  for (const s of surfaces) {
    if (skyReceiverExclusion(s)) continue;
    // Alternate realizations do not have the source vertex stream. Bind them
    // from exterior free space, never an arbitrary (often underside) vertex.
    const minimum = [Infinity, Infinity, Infinity],
      maximum = [-Infinity, -Infinity, -Infinity];
    for (let corner = 0; corner < 8; corner++) {
      const p = [0, 1, 2].map((a) => (corner & (1 << a) ? s.mesh.bounds.max[a] : s.mesh.bounds.min[a]));
      for (let a = 0; a < 3; a++) {
        const v =
          s.matrix[a] * p[0] + s.matrix[4 + a] * p[1] + s.matrix[8 + a] * p[2] + s.matrix[12 + a] + origin[a];
        minimum[a] = Math.min(minimum[a], v);
        maximum[a] = Math.max(maximum[a], v);
      }
    }
    const pair = radianceReceiverBinding(
      geometry,
      positions,
      [(minimum[0] + maximum[0]) * 0.5, maximum[1] + 0.025, (minimum[2] + maximum[2]) * 0.5],
      sampleIndex,
    );
    fallbacks.set(s.id, [...pair, ...pair]);
    const priorReceiver = options.reuse?.receivers.get(s.id);
    const receiver =
      priorReceiver?.sources && priorReceiver.mesh.positions.length / 3 <= 65536 - vertices
        ? priorReceiver
        : yield* radianceReceiverMesh(
            s,
            65536 - vertices,
            hasEmission
              ? {
                  min: emitters.min.map((v, a) => v - origin[a]) as Vec3,
                  max: emitters.max.map((v, a) => v - origin[a]) as Vec3,
                }
              : undefined,
          );
    const mesh = receiver.mesh,
      count = mesh.positions.length / 3,
      inverse = inverseMatrix(s.matrix),
      m = s.matrix;
    if (!inverse || vertices + count > 65536) continue;
    const previousMesh = options.reuse?.meshes.get(s.id);
    const previousEmission = options.reuse?.field.receiverEmission;
    // Camera regions change probe mixtures, not direct source-to-surface
    // transport. Reuse only an identical carrier under the caller's immutable
    // absolute geometry/material certificate, including normals and topology.
    const reuseLocal =
      previousMesh?.radianceMixtures &&
      previousEmission &&
      (["positions", "normals", "indices"] as const).every((name) => {
        const before = previousMesh[name],
          after = mesh[name];
        return before === after || (before.length === after.length && before.every((v, i) => v === after[i]));
      });
    const localAt = (vertex: number, side: number): Vec3 | undefined => {
      if (!reuseLocal || !previousEmission || !previousMesh?.radianceProbes) return;
      const id = Math.floor(previousMesh.radianceProbes[vertex * 4 + side]);
      if (id < 1024) return;
      const offset = (id - 1024) * 3;
      if (offset + 3 > previousEmission.length) return;
      return Array.from(previousEmission.subarray(offset, offset + 3)) as Vec3;
    };
    const ids = new Float32Array(count * 4),
      centers = new Float64Array(count * 3),
      neighbors = new Uint32Array(count);
    const start = receiver.sources ? 0 : (s.drawRange?.start ?? 0),
      end = start + (receiver.sources ? mesh.indices.length : (s.drawRange?.count ?? mesh.indices.length));
    for (let t = start; t < end; t += 3) {
      for (let corner = 0; corner < 3; corner++) {
        const v = mesh.indices[t + corner];
        neighbors[v]++;
        for (let a = 0; a < 3; a++)
          centers[v * 3 + a] +=
            (mesh.positions[mesh.indices[t] * 3 + a] +
              mesh.positions[mesh.indices[t + 1] * 3 + a] +
              mesh.positions[mesh.indices[t + 2] * 3 + a]) /
            3;
      }
      if ((t - start) % 3072 === 0) yield;
    }
    for (let i = 0; i < count; i++) {
      if (!neighbors[i]) continue;
      const local = mesh.positions.subarray(i * 3, i * 3 + 3),
        ln = mesh.normals.subarray(i * 3, i * 3 + 3);
      const p = [0, 1, 2].map(
        (a) => m[a] * local[0] + m[4 + a] * local[1] + m[8 + a] * local[2] + m[12 + a] + origin[a],
      ) as Vec3;
      const n = normalize(
        [0, 1, 2].map(
          (a) => inverse[a * 4] * ln[0] + inverse[a * 4 + 1] * ln[1] + inverse[a * 4 + 2] * ln[2],
        ) as Vec3,
      );
      const toward = [0, 1, 2].map((a) => centers[i * 3 + a] / neighbors[i] - local[a]);
      const delta = [0, 1, 2].map(
        (a) => m[a] * toward[0] + m[4 + a] * toward[1] + m[8 + a] * toward[2],
      ) as Vec3;
      const axial = indirectDot(delta, n),
        tangent = delta.map((v, a) => v - axial * n[a]) as Vec3;
      const inset = add(p, tangent, Math.min(0.01, 0.004 / Math.max(1e-9, Math.hypot(...tangent))));
      const frontPosition = add(inset, n, 0.003),
        backPosition = add(inset, n, -0.003);
      const front =
        receiver.sources || hasEmission
          ? yield* binding(frontPosition, n, !receiver.sources, localAt(i, 0))
          : radianceReceiverBinding(geometry, positions, frontPosition, sampleIndex);
      const back =
        receiver.sources || hasEmission
          ? yield* binding(backPosition, n.map((v) => -v) as Vec3, !receiver.sources, localAt(i, 2))
          : radianceReceiverBinding(geometry, positions, backPosition, sampleIndex);
      ids.set([...front, ...back], i * 4);
      if (!front[0]) unmappedVertices++;
      if (i % 16 === 15) yield;
    }
    // A shared vertex carries a closed-region certificate only when every
    // incident triangle has all corners in the same convex opaque enclosure.
    for (const side of [0, 2]) {
      const certified = new Uint8Array(count).fill(1);
      for (let t = start; t < end; t += 3) {
        const corners = [mesh.indices[t], mesh.indices[t + 1], mesh.indices[t + 2]];
        const labels = corners.flatMap((i) => {
          const id = Math.floor(ids[i * 4 + side]);
          if (id < 1024) return [enclosed[id - 1] ?? 0, enclosed[Math.floor(ids[i * 4 + side + 1]) - 1] ?? 0];
          const offset = (id - 1024) * 8;
          return [0, 1, 2, 3]
            .filter((j) => mixtures[offset + 4 + j] > 0)
            .map((j) => enclosed[mixtures[offset + j] - 1] ?? 0);
        });
        if (!labels[0] || labels.some((label) => label !== labels[0]))
          for (const i of corners) certified[i] = 0;
        if ((t - start) % 768 === 0) yield;
      }
      for (let i = 0; i < count; i++) if (certified[i] && ids[i * 4 + side] > 0) ids[i * 4 + side] += 0.5;
      yield;
    }
    meshes.set(s.id, {
      ...mesh,
      radianceProbes: ids,
      // Refinement copies each source triangle's constant normal. Reuse that
      // proof instead of losing it when tessellation exceeds the scan budget.
      radianceFlatNormals: radianceFlatNormals(receiver.sources ? s.mesh : mesh),
      radianceMixtures: !!receiver.sources || hasEmission,
    });
    receivers.set(s.id, receiver);
    vertices += count;
    vertexBytes +=
      ids.byteLength +
      (receiver.sources?.byteLength ?? 0) +
      (receiver.weights?.byteLength ?? 0) +
      (receiver.sources
        ? mesh.positions.byteLength +
          mesh.normals.byteLength +
          mesh.indices.byteLength +
          (mesh.colors?.byteLength ?? 0) +
          (mesh.materialCoordinates?.byteLength ?? 0)
        : 0);
  }
  const compact = yield* compactRadianceReceiverTable(
    new Float32Array(mixtures),
    hasEmission ? new Float32Array(receiverEmission) : undefined,
    meshes,
  );
  const field: RadianceLightingField = {
    key: options.key,
    revision: 1,
    positions,
    enclosed,
    transfer,
    directEmission,
    receivers: compact.receivers,
    receiverEmission: compact.emission,
    skyVisibility,
    lights,
    report: {
      samples: positions.length,
      reusedSamples,
      reusedGeometry: !!options.reuse,
      reusedEmissionReceivers,
      triangles: geometry.triangles.length,
      rays,
      vertices,
      unmappedVertices,
      bytes:
        transfer.byteLength +
        directEmission.byteLength +
        skyVisibility.byteLength +
        compact.receivers.byteLength +
        (compact.emission?.byteLength ?? 0) +
        vertexBytes +
        geometry.report.bytes,
      buildMs: performance.now() - started,
      excluded: geometry.report.excluded,
    },
  };
  const product = { field, meshes, geometry, enclosures, raysPerSample: rayCount, receivers, fallbacks };
  yield* compileRadianceSurfaceCacheSteps(product, surfaces, origin, options.center, reusable);
  field.report.buildMs = performance.now() - started;
  return product;
}
