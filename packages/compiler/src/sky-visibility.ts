import { inverseMatrix, type MeshData, normalize, type RenderSurface, type Vec3 } from "@wrela/model";
import { indirectGeometrySteps, indirectSurfaceExclusion, traceSkyDistance } from "./indirect-query";

const samples = 24;
const hemisphereSamples = Array.from({ length: samples }, (_, j) => {
  const u = (j + 0.5) / samples,
    phi = ((j * 0.6180339887498949) % 1) * Math.PI * 2;
  return [Math.sqrt(u) * Math.cos(phi), Math.sqrt(u) * Math.sin(phi), Math.sqrt(1 - u)];
});
function directions(normal: Vec3): Vec3[] {
  const helper: Vec3 = Math.abs(normal[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const t = normalize([
    helper[1] * normal[2] - helper[2] * normal[1],
    helper[2] * normal[0] - helper[0] * normal[2],
    helper[0] * normal[1] - helper[1] * normal[0],
  ]);
  const b = [
    normal[1] * t[2] - normal[2] * t[1],
    normal[2] * t[0] - normal[0] * t[2],
    normal[0] * t[1] - normal[1] * t[0],
  ];
  return hemisphereSamples.map(([x, y, z]) => [
    t[0] * x + b[0] * y + normal[0] * z,
    t[1] * x + b[1] * y + normal[1] * z,
    t[2] * x + b[2] * y + normal[2] * z,
  ]);
}

export type SkyVisibilityProduct = {
  meshes: Map<string, MeshData>;
  report: {
    vertices: number;
    reusedVertices: number;
    rays: number;
    bytes: number;
    buildMs: number;
    activeMs?: number;
    maxSliceMs?: number;
    excluded: { id: string; reason: string }[];
  };
};

export function skyReceiverExclusion(s: RenderSurface): string | undefined {
  return (
    indirectSurfaceExclusion(s) ??
    (s.reliefAppearance || s.mesh.reliefCoordinates
      ? "displaced receiver"
      : s.selectedRenderProduct && s.selectedRenderProduct.kind !== "direct-mesh"
        ? "alternate realization"
        : undefined)
  );
}

/** Local sky visibility, not bounced light. Surface samples have no probe-volume
 * interpolation or per-pixel geometry query. Both sides are sampled; the front
 * also stores a bent direction. Rays are cosine distributed over each normal.
 * Full resident visibility keeps large enclosed spaces dark, regardless of size. */
export function* compileSkyVisibilitySteps(
  surfaces: readonly RenderSurface[],
  origin: Vec3 = [0, 0, 0],
  reuse: ReadonlyMap<string, MeshData> = new Map(),
): Generator<void, SkyVisibilityProduct> {
  const started = performance.now(),
    meshes = new Map<string, MeshData>(),
    report: SkyVisibilityProduct["report"] = {
      vertices: 0,
      reusedVertices: 0,
      rays: 0,
      bytes: 0,
      buildMs: 0,
      excluded: [],
    };
  const geometry = yield* indirectGeometrySteps(surfaces, { origin, maxTriangles: 200000 });
  const radius = Infinity;
  // Shared terrain borders and faceted duplicate vertices reuse identical work.
  const values = new Map<string, readonly number[]>();
  for (const s of surfaces) {
    const reason = skyReceiverExclusion(s);
    if (reason) {
      report.excluded.push({ id: s.id, reason });
      continue;
    }
    const mesh = s.mesh,
      count = mesh.positions.length / 3;
    if (report.bytes + count * 16 > 4 * 1024 * 1024 || report.vertices + count > 65536) {
      report.excluded.push({ id: s.id, reason: "sky receiver budget" });
      continue;
    }
    const cached = reuse.get(s.id);
    if (cached?.skyVisibility) {
      meshes.set(s.id, cached);
      report.vertices += count;
      report.reusedVertices += count;
      report.bytes += cached.skyVisibility.byteLength;
      yield;
      continue;
    }
    const inverse = inverseMatrix(s.matrix);
    if (!inverse) {
      report.excluded.push({ id: s.id, reason: "singular receiver" });
      continue;
    }
    const output = new Float32Array(count * 4),
      m = s.matrix;
    const used = new Set(
      mesh.indices.subarray(
        s.drawRange?.start ?? 0,
        (s.drawRange?.start ?? 0) + (s.drawRange?.count ?? mesh.indices.length),
      ),
    );
    const centers = new Float64Array(count * 3),
      neighbors = new Uint32Array(count);
    const first = s.drawRange?.start ?? 0,
      end = first + (s.drawRange?.count ?? mesh.indices.length);
    for (let t = first; t < end; t += 3) {
      for (let corner = 0; corner < 3; corner++) {
        const vertex = mesh.indices[t + corner];
        for (let a = 0; a < 3; a++)
          centers[vertex * 3 + a] +=
            (mesh.positions[mesh.indices[t] * 3 + a] +
              mesh.positions[mesh.indices[t + 1] * 3 + a] +
              mesh.positions[mesh.indices[t + 2] * 3 + a]) /
            3;
        neighbors[vertex]++;
      }
      if ((t - first) % 3072 === 0) yield;
    }
    for (const i of used) {
      const local = mesh.positions.subarray(i * 3, i * 3 + 3),
        ln = mesh.normals.subarray(i * 3, i * 3 + 3);
      const p: Vec3 = [0, 1, 2].map(
        (a) => m[a] * local[0] + m[4 + a] * local[1] + m[8 + a] * local[2] + m[12 + a] + origin[a],
      ) as Vec3;
      const n = normalize(
        [0, 1, 2].map(
          (a) => inverse[a * 4] * ln[0] + inverse[a * 4 + 1] * ln[1] + inverse[a * 4 + 2] * ln[2],
        ) as Vec3,
      );
      // At a joined wall/floor edge an exactly coincident sample starts on
      // the other wall's plane. Move at most 2 mm into the receiver support
      // before applying the ray bias, so shared room corners do not leak.
      const toward = [0, 1, 2].map((a) => centers[i * 3 + a] / neighbors[i] - local[a]);
      const offset = [0, 1, 2].map((a) => m[a] * toward[0] + m[4 + a] * toward[1] + m[8 + a] * toward[2]);
      const axial = offset[0] * n[0] + offset[1] * n[1] + offset[2] * n[2];
      const tangent = offset.map((v, a) => v - axial * n[a]);
      const scale = Math.min(0.01, 0.002 / Math.max(0.000001, Math.hypot(...tangent)));
      const inset = p.map((v, a) => v + tangent[a] * scale) as Vec3;
      const key = [...inset, ...n].join(",");
      let value = values.get(key);
      if (!value) {
        let front = 0,
          back = 0;
        const bent: Vec3 = [0, 0, 0],
          original: Vec3 = [0, 0, 0];
        for (let side = 0; side < 2; side++) {
          const normal = n.map((v) => (side ? -v : v)) as Vec3;
          const start = inset.map((v, a) => v + normal[a] * 0.002) as Vec3;
          for (const direction of directions(normal)) {
            const distance = geometry.triangles.length
              ? traceSkyDistance(geometry, start, direction, radius)
              : radius;
            const visible = distance === Infinity ? 1 : 0;
            if (side) back += visible;
            else {
              front += visible;
              for (let a = 0; a < 3; a++) {
                bent[a] += direction[a] * visible;
                original[a] += direction[a];
              }
            }
            report.rays++;
          }
          yield;
        }
        const visibility = front / samples;
        // Subtract the finite sample set's own first-moment bias. Fully open
        // surfaces then preserve the authored normal exactly.
        const direction = normalize(
          n.map((v, a) => v + (bent[a] - original[a] * visibility) / samples) as Vec3,
        );
        value = [...direction.map((v) => v * visibility), back / samples];
        values.set(key, value);
      }
      output.set(value, i * 4);
      yield;
    }
    report.vertices += count;
    meshes.set(s.id, { ...mesh, skyVisibility: output });
    report.bytes += output.byteLength;
  }
  report.buildMs = performance.now() - started;
  return { meshes, report };
}
