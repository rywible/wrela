import { type MeshData, type RenderSurface, type Vec3, validRadianceSurfaceCache } from "@wrela/model";
import { yieldCompilation } from "./cooperative";
import { type IndirectGeometry, indirectSurfaceExclusion } from "./indirect-query";
import type { RadianceLightingProduct } from "./radiance-lighting";
import { skyReceiverExclusion } from "./sky-visibility";

/** Bump whenever transport, placement, receiver encoding or enclosure semantics change. */
export const RADIANCE_COOK_VERSION = 12;
type Receiver = {
  id: string;
  probes: Float32Array;
  flat: boolean;
  mixtures: boolean;
  sources?: Uint32Array;
  weights?: Float32Array;
  mesh?: Pick<
    MeshData,
    | "positions"
    | "normals"
    | "indices"
    | "colors"
    | "materialCoordinates"
    | "sourceIds"
    | "bounds"
    | "fidelity"
  >;
};
export type CookedRadiance = {
  version: typeof RADIANCE_COOK_VERSION;
  source: string;
  field: RadianceLightingProduct["field"];
  triangles: Float64Array;
  nodes: Float64Array;
  order: Uint32Array;
  components: Int32Array;
  closed: Uint32Array;
  receivers: Receiver[];
  fallbacks: [string, [number, number, number, number]][];
  raysPerSample: number;
  bytes: number;
};
const triangleFields = ["a", "ab", "ac", "normal", "albedo", "min", "max", "emission"] as const;
const hashes = new WeakMap<object, Promise<string>>();
async function digest(bytes: Uint8Array): Promise<string> {
  const value = await crypto.subtle.digest("SHA-256", bytes as Uint8Array<ArrayBuffer>);
  return Array.from(new Uint8Array(value), (n) => n.toString(16).padStart(2, "0")).join("");
}
function hashView(view: Float32Array | Uint32Array | undefined) {
  if (!view) return Promise.resolve(null);
  let hash = hashes.get(view);
  if (!hash) {
    hash = digest(new Uint8Array(view.buffer, view.byteOffset, view.byteLength));
    hashes.set(view, hash);
  }
  return hash;
}
async function hashSourceIds(ids: string[] | undefined): Promise<string | null> {
  if (!ids) return null;
  let hash = hashes.get(ids);
  if (!hash) {
    hash = (async () => {
      const parts: string[] = [];
      for (let start = 0; start < ids.length; start += 4096) {
        parts.push(await digest(new TextEncoder().encode(JSON.stringify(ids.slice(start, start + 4096)))));
        if (start) await yieldCompilation();
      }
      return digest(new TextEncoder().encode(JSON.stringify(parts)));
    })();
    hashes.set(ids, hash);
  }
  return hash;
}
/** Immutable source buffers are hashed once. Radiometric inputs and render-origin
 * rebases do not invalidate persisted transport; absolute geometry and emitters do. */
export async function radianceSourceKey(
  surfaces: readonly RenderSurface[],
  origin: Vec3,
  center: Vec3,
  lights: RadianceLightingProduct["field"]["lights"],
): Promise<string> {
  const sources: unknown[] = [];
  let started = performance.now();
  for (const s of surfaces) {
    if (indirectSurfaceExclusion(s)) continue;
    const m = s.mesh;
    sources.push([
      s.id,
      await Promise.all([m.positions, m.normals, m.indices, m.colors, m.materialCoordinates].map(hashView)),
      await hashSourceIds(m.sourceIds),
      m.bounds,
      m.fidelity,
      s.drawRange,
      skyReceiverExclusion(s),
      Array.from(s.matrix, (v, i) => v + (i >= 12 && i < 15 ? origin[i - 12] : 0)),
      s.material.color,
      s.material.metallic,
      s.material.emission,
    ]);
    if (performance.now() - started > 2) {
      await yieldCompilation();
      started = performance.now();
    }
  }
  return digest(new TextEncoder().encode(JSON.stringify([RADIANCE_COOK_VERSION, sources, center, lights])));
}

/** Flatten object-heavy query geometry before IndexedDB's synchronous clone.
 * Float64 storage preserves absolute world coordinates and exact visibility. */
export function* cookRadianceSteps(
  product: RadianceLightingProduct,
  source: string,
): Generator<void, CookedRadiance> {
  const geometry = product.geometry;
  if (geometry.layers?.length)
    throw Error("Transient dynamic transport cannot be persisted as static lighting");
  const triangles = new Float64Array(geometry.triangles.length * 24);
  for (let i = 0; i < geometry.triangles.length; i++) {
    const t = geometry.triangles[i];
    triangleFields.forEach((name, j) => {
      triangles.set(t[name] ?? [0, 0, 0], i * 24 + j * 3);
    });
    if (i % 256 === 255) yield;
  }
  const nodes = new Float64Array(geometry.nodes.length * 10);
  for (let i = 0; i < geometry.nodes.length; i++) {
    const n = geometry.nodes[i];
    nodes.set([...n.min, ...n.max, n.start, n.count, n.left, n.right], i * 10);
    if (i % 256 === 255) yield;
  }
  const receivers: Receiver[] = [];
  for (const [id, mesh] of product.meshes) {
    const receiver = product.receivers.get(id);
    if (!mesh.radianceProbes || !receiver) throw Error("Incomplete radiance receiver");
    const { positions, normals, indices, colors, materialCoordinates, sourceIds, bounds, fidelity } = mesh;
    receivers.push({
      id,
      probes: mesh.radianceProbes,
      flat: !!mesh.radianceFlatNormals,
      mixtures: !!mesh.radianceMixtures,
      sources: receiver.sources,
      weights: receiver.weights,
      mesh: receiver.sources
        ? { positions, normals, indices, colors, materialCoordinates, sourceIds, bounds, fidelity }
        : undefined,
    });
    yield;
  }
  const result: CookedRadiance = {
    version: RADIANCE_COOK_VERSION,
    source,
    field: product.field,
    triangles,
    nodes,
    order: Uint32Array.from(geometry.order),
    components: product.enclosures.components,
    closed: Uint32Array.from(product.enclosures.closed),
    receivers,
    fallbacks: [...product.fallbacks],
    raysPerSample: product.raysPerSample,
    bytes: 0,
  };
  // Binary storage plus conservative metadata allowance; duplicate array views
  // are counted repeatedly so budget accounting never relies on clone aliasing.
  const bytes = (v: unknown): number => {
    if (ArrayBuffer.isView(v)) return v.byteLength;
    if (typeof v === "string") return v.length * 2 + 16;
    if (Array.isArray(v)) return 32 + v.reduce((n, x) => n + bytes(x), 0);
    if (v && typeof v === "object")
      return 64 + Object.entries(v).reduce((n, [k, x]) => n + k.length * 2 + bytes(x), 0);
    return 8;
  };
  result.bytes = bytes(result);
  return result;
}

/** Cached data never crosses into GPU/query evaluation before structural checks.
 * Invalid or obsolete entries are disposable acceleration data, never source. */
export function* restoreRadianceSteps(
  input: CookedRadiance,
  source: string,
  surfaces: readonly RenderSurface[],
  key: string,
): Generator<void, RadianceLightingProduct> {
  if (!input || input.version !== RADIANCE_COOK_VERSION || input.source !== source)
    throw Error("Stale radiance product");
  const { field, triangles, nodes, order, components, closed } = input;
  const samples = field.positions.length,
    count = triangles.length / 24,
    nodeCount = nodes.length / 10;
  if (
    !(triangles instanceof Float64Array) ||
    !(nodes instanceof Float64Array) ||
    !(order instanceof Uint32Array) ||
    !(components instanceof Int32Array) ||
    !(closed instanceof Uint32Array) ||
    !Number.isInteger(count) ||
    count > 200000 ||
    !Number.isInteger(nodeCount) ||
    nodeCount < 1 ||
    nodeCount > Math.max(1, count * 2) ||
    order.length !== count ||
    components.length !== count ||
    samples > 512 ||
    !validRadianceSurfaceCache(field.surfaceDiffuse) ||
    field.surfaceDiffuse?.patches?.some((p) => p.triangle >= count) ||
    !(field.transfer instanceof Float32Array) ||
    field.transfer.length !== samples * 9 * 27 * 4 ||
    !(field.directEmission instanceof Float32Array) ||
    field.directEmission.length !== samples * 27 ||
    (field.emissionScale !== undefined &&
      (!Number.isFinite(Math.fround(field.emissionScale)) || field.emissionScale < 0)) ||
    (field.receiverEmission !== undefined &&
      (!(field.receiverEmission instanceof Float32Array) ||
        field.receiverEmission.length !== ((field.receivers?.length ?? 0) / 8) * 3 ||
        !field.directEmission)) ||
    !(field.skyVisibility instanceof Float32Array) ||
    field.skyVisibility.length !== samples * 9 ||
    !(field.enclosed instanceof Uint32Array) ||
    field.enclosed.length !== samples ||
    field.lights.length > 8 ||
    input.receivers.length > 2048 ||
    input.fallbacks.length > 2048 ||
    !Number.isInteger(input.raysPerSample) ||
    input.raysPerSample < 16 ||
    input.raysPerSample > 1024 ||
    (field.receivers &&
      (!(field.receivers instanceof Float32Array) ||
        field.receivers.length % 8 ||
        field.receivers.length > 131072 * 8))
  )
    throw Error("Malformed radiance product");
  for (const array of [
    triangles,
    field.transfer,
    field.directEmission,
    field.receiverEmission ?? [],
    field.skyVisibility,
    field.receivers ?? [],
  ]) {
    for (let i = 0; i < array.length; i++) {
      if (!Number.isFinite(array[i])) throw Error("Nonfinite radiance product");
      if (i % 8192 === 8191) yield;
    }
  }
  if (
    field.positions.some((p) => p.length !== 3 || !p.every(Number.isFinite)) ||
    field.lights.some(
      (l) =>
        l.position.length !== 3 ||
        !l.position.every(Number.isFinite) ||
        (l.range !== undefined && (!Number.isFinite(l.range) || l.range <= 0)),
    )
  )
    throw Error("Invalid radiance positions");
  const geometry: IndirectGeometry = {
    triangles: [],
    nodes: [],
    order: Array.from(order),
    bounds: { min: [0, 0, 0], max: [0, 0, 0] },
    report: {
      triangles: count,
      nodes: nodeCount,
      bytes: 0,
      excluded: surfaces.flatMap((s) => {
        const reason = indirectSurfaceExclusion(s);
        return reason ? [{ id: s.id, reason }] : [];
      }),
    },
  };
  for (let i = 0; i < count; i++) {
    if (order[i] >= count || components[i] < 0 || components[i] >= count)
      throw Error("Invalid radiance query index");
    const vector = (at: number) => Array.from(triangles.subarray(i * 24 + at, i * 24 + at + 3)) as Vec3;
    geometry.triangles.push({
      a: vector(0),
      ab: vector(3),
      ac: vector(6),
      normal: vector(9),
      albedo: vector(12),
      min: vector(15),
      max: vector(18),
      emission: vector(21),
    });
    if (i % 256 === 255) yield;
  }
  for (let i = 0; i < nodeCount; i++) {
    const v = Array.from(nodes.subarray(i * 10, i * 10 + 10));
    const [start, length, left, right] = v.slice(6);
    if (
      v.some((x, a) => !Number.isFinite(x) && !(count === 0 && a < 6)) ||
      ![start, length, left, right].every(Number.isInteger) ||
      start < 0 ||
      length < 0 ||
      start + length > count ||
      (length > 0
        ? left !== -1 || right !== -1
        : count > 0 && (left <= i || right <= i || left >= nodeCount || right >= nodeCount))
    )
      throw Error("Invalid radiance query topology");
    geometry.nodes.push({
      min: v.slice(0, 3) as Vec3,
      max: v.slice(3, 6) as Vec3,
      start,
      count: length,
      left,
      right,
    });
    if (i % 256 === 255) yield;
  }
  if (count) geometry.bounds = { min: geometry.nodes[0].min, max: geometry.nodes[0].max };
  geometry.report.bytes = count * 24 * 8 + nodeCount * 10 * 8 + order.length * 8;
  if (closed.some((id) => id >= count)) throw Error("Invalid radiance enclosure");
  if (field.enclosed.some((id) => id > count)) throw Error("Invalid enclosure binding");
  const mixtures = field.receivers ?? new Float32Array();
  for (let at = 0; at < mixtures.length; at += 8) {
    let total = 0;
    for (let j = 0; j < 4; j++) {
      const id = mixtures[at + j],
        weight = mixtures[at + j + 4];
      if (!Number.isInteger(id) || id < 0 || id > samples || weight < 0 || weight > 1 || (weight > 0 && !id))
        throw Error("Invalid mixture binding");
      total += weight;
    }
    if (Math.abs(total - 1) > 1e-5) throw Error("Invalid mixture normalization");
    if (at % 8192 === 0) yield;
  }
  const validBindings = (values: Float32Array | number[]) => {
    for (let i = 0; i < values.length; i += 2) {
      const a = values[i],
        b = values[i + 1],
        id = Math.floor(a),
        fraction = a - id;
      if (
        !Number.isFinite(a) ||
        !Number.isFinite(b) ||
        a < 0 ||
        b < 0 ||
        (fraction !== 0 && fraction !== 0.5)
      )
        return false;
      if (id >= 1024) {
        if (
          id - 1024 >= mixtures.length / 8 ||
          !Number.isInteger(b) ||
          b > (field.surfaceDiffuse?.receivers.length ?? 0) / 12
        )
          return false;
      } else {
        const other = Math.floor(b),
          weight = b - other;
        if (
          id > samples ||
          other > samples ||
          (!id && a !== 0) ||
          (!other && b !== 0) ||
          (weight !== 0 && (weight < 0.125 || weight > 0.375))
        )
          return false;
      }
    }
    return true;
  };
  const originals = new Map(surfaces.map((s) => [s.id, s.mesh]));
  let vertexCount = 0;
  const meshes: RadianceLightingProduct["meshes"] = new Map(),
    receivers: RadianceLightingProduct["receivers"] = new Map();
  for (const entry of input.receivers) {
    const original = originals.get(entry.id),
      mesh = entry.mesh ?? original;
    if (
      !original ||
      !mesh ||
      typeof entry.mixtures !== "boolean" ||
      meshes.has(entry.id) ||
      !(mesh.positions instanceof Float32Array) ||
      !(mesh.normals instanceof Float32Array) ||
      !(mesh.indices instanceof Uint32Array) ||
      mesh.positions.length % 3 !== 0 ||
      mesh.indices.length % 3 !== 0 ||
      vertexCount + mesh.positions.length / 3 > 65536 ||
      !!entry.mesh !== !!entry.sources ||
      !!entry.sources !== !!entry.weights ||
      (mesh.colors && mesh.colors.length !== mesh.positions.length) ||
      (mesh.materialCoordinates && mesh.materialCoordinates.length !== mesh.positions.length) ||
      !(entry.probes instanceof Float32Array) ||
      entry.probes.length !== (mesh.positions.length / 3) * 4 ||
      (entry.mesh &&
        (!(entry.sources instanceof Uint32Array) ||
          !(entry.weights instanceof Float32Array) ||
          entry.sources.length !== mesh.positions.length ||
          entry.weights.length !== mesh.positions.length ||
          entry.sources.some((id) => id >= original.positions.length / 3))) ||
      mesh.normals.length !== mesh.positions.length ||
      mesh.indices.some((id) => id >= mesh.positions.length / 3)
    )
      throw Error("Invalid radiance receiver layout");
    vertexCount += mesh.positions.length / 3;
    if (!validBindings(entry.probes)) throw Error("Invalid persisted sample binding");
    if (entry.mesh) {
      for (const values of [
        mesh.positions,
        mesh.normals,
        mesh.colors ?? [],
        mesh.materialCoordinates ?? [],
        entry.weights ?? [],
      ]) {
        for (let i = 0; i < values.length; i++) {
          if (!Number.isFinite(values[i])) throw Error("Nonfinite persisted receiver");
          if (i % 8192 === 8191) yield;
        }
      }
      const weights = entry.weights ?? [];
      for (let i = 0; i < weights.length; i += 3)
        if (
          weights[i] < -1e-6 ||
          weights[i + 1] < -1e-6 ||
          weights[i + 2] < -1e-6 ||
          Math.abs(weights[i] + weights[i + 1] + weights[i + 2] - 1) > 1e-5
        )
          throw Error("Invalid receiver interpolation");
    }
    meshes.set(entry.id, {
      ...mesh,
      radianceProbes: entry.probes,
      radianceFlatNormals: entry.flat,
      radianceMixtures: entry.mixtures,
    });
    receivers.set(entry.id, { mesh, sources: entry.sources, weights: entry.weights });
    yield;
  }
  for (const [id, values] of input.fallbacks)
    if (!originals.has(id) || values.length !== 4 || !validBindings(values))
      throw Error("Invalid persisted fallback");
  return {
    field: { ...field, key, report: { ...field.report, excluded: geometry.report.excluded } },
    geometry,
    meshes,
    receivers,
    enclosures: { components, closed: new Set(closed) },
    fallbacks: new Map(input.fallbacks),
    raysPerSample: input.raysPerSample,
  };
}
