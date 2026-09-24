import type { Bounds, MeshData, RenderSurface, Vec3 } from "@wrela/model";
export type RadianceReceiver = { mesh: MeshData; sources?: Uint32Array; weights?: Float32Array };
/** Refine only coarse rigid carriers. Lighting interpolation then has a spatial
 * resolution independent of whether the author used two triangles for a wall.
 * The mathematical surface and all authored interpolants remain unchanged. */
export function* radianceReceiverMesh(
  s: RenderSurface,
  budget: number,
  emissionBounds?: Bounds,
): Generator<void, RadianceReceiver> {
  const mesh = s.mesh,
    start = s.drawRange?.start ?? 0,
    end = start + (s.drawRange?.count ?? mesh.indices.length),
    m = s.matrix;
  if (mesh.positions.length / 3 > 2048 || end - start > 768) return { mesh };
  const divisions: number[] = [];
  let count = 0,
    indexCount = 0;
  const point = (i: number): Vec3 =>
    [0, 1, 2].map(
      (a) =>
        m[a] * mesh.positions[i * 3] +
        m[4 + a] * mesh.positions[i * 3 + 1] +
        m[8 + a] * mesh.positions[i * 3 + 2],
    ) as Vec3;
  for (let t = start; t < end; t += 3) {
    // Barycentric picking votes cannot be represented by a single new source ID
    // across an authored semantic boundary; retain that carrier unchanged.
    if (
      mesh.sourceIds &&
      [1, 2].some((j) => mesh.sourceIds?.[mesh.indices[t + j]] !== mesh.sourceIds?.[mesh.indices[t]])
    )
      return { mesh };
    const points = [0, 1, 2].map((j) => point(mesh.indices[t + j]));
    const edge = Math.max(
      ...points.map((p, i) => Math.hypot(...p.map((v, a) => v - points[(i + 1) % 3][a]))),
    );
    let spacing = 0.5,
      limit = 12;
    if (emissionBounds) {
      const gap = Math.hypot(
        ...[0, 1, 2].map((a) =>
          Math.max(
            0,
            Math.min(...points.map((p) => p[a])) - emissionBounds.max[a],
            emissionBounds.min[a] - Math.max(...points.map((p) => p[a])),
          ),
        ),
      );
      if (gap < 1) {
        spacing = Math.max(0.08, gap * 0.35);
        limit = 48;
      }
    }
    const n = Math.min(limit, Math.max(1, Math.ceil(edge / spacing)));
    divisions.push(n);
    count += ((n + 1) * (n + 2)) / 2;
    indexCount += n * n * 3;
  }
  if (count > Math.min(budget, 12288)) {
    // Extra local detail cannot evict the carrier's ordinary refinement.
    if (emissionBounds) return yield* radianceReceiverMesh(s, budget);
    return { mesh };
  }
  if (!divisions.some((n) => n > 1)) return { mesh };
  const positions = new Float32Array(count * 3),
    normals = new Float32Array(count * 3),
    sources = new Uint32Array(count * 3),
    weights = new Float32Array(count * 3);
  const colors = mesh.colors ? new Float32Array(count * 3) : undefined,
    coordinates = mesh.materialCoordinates ? new Float32Array(count * 3) : undefined;
  const sourceIds = mesh.sourceIds ? new Array<string>(count) : undefined;
  const indices = new Uint32Array(indexCount);
  let vertex = 0,
    index = 0;
  for (let t = start; t < end; t += 3) {
    const n = divisions[(t - start) / 3],
      ids = [mesh.indices[t], mesh.indices[t + 1], mesh.indices[t + 2]];
    const shared = new Map<string, number>();
    const emit = (x: number, y: number) => {
      const key = `${x},${y}`,
        cached = shared.get(key);
      if (cached !== undefined) {
        indices[index++] = cached;
        return;
      }
      shared.set(key, vertex);
      indices[index++] = vertex;
      const w = [1 - x / n - y / n, x / n, y / n];
      sources.set(ids, vertex * 3);
      weights.set(w, vertex * 3);
      for (const [source, output] of [
        [mesh.positions, positions],
        [mesh.normals, normals],
        [mesh.colors, colors],
        [mesh.materialCoordinates, coordinates],
      ] as const) {
        if (!source || !output) continue;
        for (let a = 0; a < 3; a++)
          output[vertex * 3 + a] = ids.reduce((sum, i, j) => sum + source[i * 3 + a] * w[j], 0);
      }
      if (sourceIds && mesh.sourceIds) sourceIds[vertex] = mesh.sourceIds[ids[0]];
      vertex++;
    };
    for (let y = 0; y < n; y++) {
      for (let x = 0; x < n - y; x++) {
        emit(x, y);
        emit(x + 1, y);
        emit(x, y + 1);
        if (x + y < n - 1) {
          emit(x + 1, y);
          emit(x + 1, y + 1);
          emit(x, y + 1);
        }
      }
      if (y % 4 === 3) yield;
    }
    yield;
  }
  return {
    mesh: {
      positions,
      normals,
      colors,
      materialCoordinates: coordinates,
      sourceIds,
      fidelity: mesh.fidelity,
      indices,
      bounds: mesh.bounds,
    },
    sources,
    weights,
  };
}

/** A camera is on one side of an entire planar triangle. Only geometric,
 * constant normals certify that side for vertex-cached rough reflections. */
export function radianceFlatNormals(mesh: MeshData): boolean {
  if (mesh.indices.length > 1536) return false;
  for (let t = 0; t < mesh.indices.length; t += 3) {
    const [a, b, c] = [0, 1, 2].map((j) => mesh.indices[t + j] * 3);
    const n = mesh.normals.subarray(a, a + 3);
    for (const i of [b, c])
      for (let k = 0; k < 3; k++) if (Math.abs(mesh.normals[i + k] - n[k]) > 1e-6) return false;
    const u = [0, 1, 2].map((k) => mesh.positions[b + k] - mesh.positions[a + k]);
    const v = [0, 1, 2].map((k) => mesh.positions[c + k] - mesh.positions[a + k]);
    const g = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    const length = Math.hypot(...g) * Math.hypot(...n);
    if (length > 1e-12 && g.reduce((s, x, k) => s + x * n[k], 0) < length * (1 - 1e-6)) return false;
  }
  return true;
}
