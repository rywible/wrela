import type { RenderSurface } from "@wrela/model";

import { packVertices, VERTEX_FLOATS } from "./packing";

/** Validate before culling/upload: an understated envelope could hide visible corrective geometry. */
export function validateCreatureDeformation(surface: RenderSurface): string | undefined {
  const deformation = surface.deformation;
  if (!deformation) return;
  const count = surface.mesh.positions.length / 3;
  const entries = deformation.vertexIndices?.length ?? count;
  if (
    !Number.isInteger(count) ||
    deformation.positionDeltas.length !== entries * 3 ||
    (deformation.normalDeltas && deformation.normalDeltas.length !== entries * 3)
  )
    return "Deformation delta count does not match its vertex mapping";
  if (!Number.isFinite(deformation.maxDisplacement) || deformation.maxDisplacement < 0)
    return "Deformation displacement envelope must be finite and nonnegative";
  const used = deformation.vertexIndices ? new Set<number>() : undefined;
  for (let i = 0; i < entries; i++) {
    const vertex = deformation.vertexIndices?.[i] ?? i;
    if (vertex >= count || used?.has(vertex)) return "Deformation vertex mapping is invalid or duplicated";
    used?.add(vertex);
    const offset = i * 3;
    const displacement = Math.hypot(
      deformation.positionDeltas[offset],
      deformation.positionDeltas[offset + 1],
      deformation.positionDeltas[offset + 2],
    );
    if (!Number.isFinite(displacement) || displacement > deformation.maxDisplacement + 1e-6)
      return "Deformation exceeds its declared displacement envelope";
    if (
      deformation.normalDeltas &&
      (!Number.isFinite(deformation.normalDeltas[offset]) ||
        !Number.isFinite(deformation.normalDeltas[offset + 1]) ||
        !Number.isFinite(deformation.normalDeltas[offset + 2]))
    )
      return "Deformation normal deltas must be finite";
  }
}

/** Reuses staging memory across poses; immutable topology, colors and binding slots stay unchanged. */
export function packCreatureDeformation(surface: RenderSurface, target?: Float32Array): Float32Array {
  const failure = validateCreatureDeformation(surface);
  if (failure) throw new Error(failure);
  const count = surface.mesh.positions.length / 3;
  const out = target?.length === count * VERTEX_FLOATS ? target : packVertices(surface.mesh, surface.skin);
  // Reset every position/normal before applying sparse deltas, including vertices active only in an earlier pose.
  for (let vertex = 0; vertex < count; vertex++) {
    for (let axis = 0; axis < 3; axis++) {
      out[vertex * VERTEX_FLOATS + axis] = surface.mesh.positions[vertex * 3 + axis];
      out[vertex * VERTEX_FLOATS + 3 + axis] = surface.mesh.normals[vertex * 3 + axis];
    }
  }
  const deformation = surface.deformation;
  if (deformation) {
    const entries = deformation.vertexIndices?.length ?? count;
    for (let i = 0; i < entries; i++) {
      const vertex = deformation.vertexIndices?.[i] ?? i;
      const slot = vertex * VERTEX_FLOATS;
      for (let axis = 0; axis < 3; axis++) {
        out[slot + axis] += deformation.positionDeltas[i * 3 + axis];
        out[slot + 3 + axis] += deformation.normalDeltas?.[i * 3 + axis] ?? 0;
      }
      const length = Math.hypot(out[slot + 3], out[slot + 4], out[slot + 5]);
      if (length > 1e-8) for (let axis = 0; axis < 3; axis++) out[slot + 3 + axis] /= length;
      else
        for (let axis = 0; axis < 3; axis++) out[slot + 3 + axis] = surface.mesh.normals[vertex * 3 + axis];
    }
  }
  return out;
}
