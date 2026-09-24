import type { CompiledCharacter, MeshData, RenderSurface, Vec3 } from "@wrela/model";

import type { Pose } from "./animation";
import { creatureEulerFromQuaternion } from "./creature-runtime";

export type CreatureDeformation = NonNullable<RenderSurface["deformation"]>;
export type CreatureDeformationCache = { key: string; deformation?: CreatureDeformation };

/** Pose correctives remain sparse source products. The current CPU realization
 * recomputes accurate rest normals and uploads reusable per-instance offset data.
 * It never mutates the cooked mesh or caches an unbounded sequence of pose meshes.
 */
export function evaluateCreatureDeformation(
  artifact: CompiledCharacter,
  pose: Pose,
  previous?: CreatureDeformationCache,
  includeNormals = true,
): CreatureDeformationCache {
  const products = artifact.creatureCorrectives ?? [];
  if (!products.length) return { key: artifact.key };
  const activation = products.map((product) => {
    const local = pose.get(product.joint);
    const angles = local ? creatureEulerFromQuaternion(local.rotation) : ([0, 0, 0] as Vec3);
    const axis = product.axis === "x" ? 0 : product.axis === "y" ? 1 : 2;
    return Math.abs(product.angle) > 1e-8 ? Math.max(0, Math.min(1, angles[axis] / product.angle)) : 0;
  });
  const key = `${artifact.key}:${includeNormals ? "normals" : "offsets"}:${JSON.stringify(activation)}`;
  if (previous?.key === key) return previous;
  if (!activation.some((value) => value > 0)) return { key };
  const positionDeltas = new Float32Array(artifact.mesh.positions.length);
  for (let productIndex = 0; productIndex < products.length; productIndex++) {
    const weight = activation[productIndex],
      product = products[productIndex];
    if (weight === 0) continue;
    for (let i = 0; i < product.vertices.length; i++) {
      const vertex = product.vertices[i] * 3,
        source = i * 3;
      positionDeltas[vertex] += product.displacements[source] * weight;
      positionDeltas[vertex + 1] += product.displacements[source + 1] * weight;
      positionDeltas[vertex + 2] += product.displacements[source + 2] * weight;
    }
  }
  if (includeNormals)
    return { key, deformation: mergeCreatureDeformation(artifact, undefined, [positionDeltas], key) };
  let maxSquared = 0;
  for (let i = 0; i < positionDeltas.length; i += 3)
    maxSquared = Math.max(
      maxSquared,
      positionDeltas[i] ** 2 + positionDeltas[i + 1] ** 2 + positionDeltas[i + 2] ** 2,
    );
  return { key, deformation: { revision: key, positionDeltas, maxDisplacement: Math.sqrt(maxSquared) } };
}

const normalTopologies = new WeakMap<MeshData, { offsets: Uint32Array; triangles: Uint32Array }>();
/** Compile the reverse triangle dependency once; a moving mane must not traverse
 * every unchanged body triangle just to update its own normals. */
function normalTopology(mesh: MeshData) {
  const cached = normalTopologies.get(mesh);
  if (cached) return cached;
  const offsets = new Uint32Array(mesh.positions.length / 3 + 1);
  for (const vertex of mesh.indices) offsets[vertex + 1]++;
  for (let i = 1; i < offsets.length; i++) offsets[i] += offsets[i - 1];
  const cursors = offsets.slice(),
    triangles = new Uint32Array(mesh.indices.length);
  for (let i = 0; i < mesh.indices.length; i++) triangles[cursors[mesh.indices[i]]++] = Math.floor(i / 3);
  const result = { offsets, triangles };
  normalTopologies.set(mesh, result);
  return result;
}

/** Combine independent rest-space systems before skinning. Recompute normals only
 * for vertices touched by changed triangles; untouched authored normals survive. */
export function mergeCreatureDeformation(
  artifact: CompiledCharacter,
  corrective: CreatureDeformation | undefined,
  additions: readonly (Float32Array | undefined)[],
  revision: string,
): CreatureDeformation | undefined {
  const channels = additions.filter((value): value is Float32Array => value !== undefined);
  if (!channels.length) return corrective;
  const mesh = artifact.mesh,
    length = mesh.positions.length;
  if (channels.some((channel) => channel.length !== length))
    throw new Error("Creature deformation topology mismatch");
  const positionDeltas = corrective?.positionDeltas.slice() ?? new Float32Array(length);
  for (const channel of channels) for (let i = 0; i < length; i++) positionDeltas[i] += channel[i];
  const touched = new Uint8Array(length / 3),
    normals = new Float32Array(length),
    topology = normalTopology(mesh);
  const touchedVertices: number[] = [];
  // A changed vertex changes each adjacent triangle's normal, and therefore
  // each vertex touched by those triangles (including stationary neighbors).
  for (let vertex = 0; vertex < length / 3; vertex++) {
    const index = vertex * 3;
    if (!positionDeltas[index] && !positionDeltas[index + 1] && !positionDeltas[index + 2]) continue;
    for (let incident = topology.offsets[vertex]; incident < topology.offsets[vertex + 1]; incident++) {
      const triangle = topology.triangles[incident] * 3;
      for (let corner = 0; corner < 3; corner++) {
        const neighbor = mesh.indices[triangle + corner];
        if (!touched[neighbor]) {
          touched[neighbor] = 1;
          touchedVertices.push(neighbor);
        }
      }
    }
  }
  const selected = new Uint8Array(mesh.indices.length / 3),
    triangles: number[] = [];
  for (const vertex of touchedVertices)
    for (let incident = topology.offsets[vertex]; incident < topology.offsets[vertex + 1]; incident++) {
      const triangle = topology.triangles[incident];
      if (!selected[triangle]) {
        selected[triangle] = 1;
        triangles.push(triangle);
      }
    }
  for (const triangle of triangles) {
    const i = triangle * 3;
    const a = mesh.indices[i] * 3,
      b = mesh.indices[i + 1] * 3,
      c = mesh.indices[i + 2] * 3;
    const ax = mesh.positions[a] + positionDeltas[a],
      ay = mesh.positions[a + 1] + positionDeltas[a + 1],
      az = mesh.positions[a + 2] + positionDeltas[a + 2];
    const abx = mesh.positions[b] + positionDeltas[b] - ax,
      aby = mesh.positions[b + 1] + positionDeltas[b + 1] - ay,
      abz = mesh.positions[b + 2] + positionDeltas[b + 2] - az;
    const acx = mesh.positions[c] + positionDeltas[c] - ax,
      acy = mesh.positions[c + 1] + positionDeltas[c + 1] - ay,
      acz = mesh.positions[c + 2] + positionDeltas[c + 2] - az;
    const nx = aby * acz - abz * acy,
      ny = abz * acx - abx * acz,
      nz = abx * acy - aby * acx;
    normals[a] += nx;
    normals[a + 1] += ny;
    normals[a + 2] += nz;
    normals[b] += nx;
    normals[b + 1] += ny;
    normals[b + 2] += nz;
    normals[c] += nx;
    normals[c + 1] += ny;
    normals[c + 2] += nz;
  }
  const normalDeltas = new Float32Array(length);
  let maxDisplacement = 0;
  for (let i = 0; i < length; i += 3) {
    maxDisplacement = Math.max(
      maxDisplacement,
      Math.hypot(positionDeltas[i], positionDeltas[i + 1], positionDeltas[i + 2]),
    );
    if (touched[i / 3]) {
      const magnitude = Math.hypot(normals[i], normals[i + 1], normals[i + 2]);
      if (magnitude > 1e-10)
        for (let axis = 0; axis < 3; axis++)
          normalDeltas[i + axis] = normals[i + axis] / magnitude - mesh.normals[i + axis];
    }
  }
  return { revision, positionDeltas, normalDeltas, maxDisplacement };
}
