import { identityColor, type RenderSurface } from "@wrela/model";
import { packSurface } from "./packing";
import type { SurfaceVisibility } from "./visibility";
export const MAX_BATCH_INSTANCES = 256;
export const INSTANCE_FLOATS = 20;
export type SurfaceBatch = { key: string; surfaces: RenderSurface[]; visibility: SurfaceVisibility };
/** Equal GPU uniforms and a shared immutable mesh permit instancing. Poses and waves stay isolated. */
export function batchSurfaces(
  surfaces: RenderSurface[],
  meshId: (surface: RenderSurface) => number,
  visibility: (surface: RenderSurface) => SurfaceVisibility = (surface) => ({
    camera: true,
    shadow: !surface.water,
  }),
): SurfaceBatch[] {
  const batches: SurfaceBatch[] = [];
  const active = new Map<string, SurfaceBatch>();
  const chunks = new Map<string, number>();
  for (const surface of surfaces) {
    const visible = visibility(surface);
    const range = surface.drawRange ?? { start: 0, count: surface.mesh.indices.length };
    const packed = packSurface(surface);
    const material = [...packed.subarray(16, 32), ...packed.subarray(96)];
    const identity =
      surface.skin || surface.water ? `single:${surface.id}` : `${meshId(surface)}:${material.join(",")}`;
    const prefix = `${identity}:${range.start}:${range.count}:${Number(visible.camera)}:${Number(visible.shadow)}`;
    let batch = active.get(prefix);
    if (!batch || batch.surfaces.length === MAX_BATCH_INSTANCES) {
      const chunk = chunks.get(prefix) ?? 0;
      chunks.set(prefix, chunk + 1);
      batch = { key: `${prefix}:${chunk}`, surfaces: [], visibility: visible };
      active.set(prefix, batch);
      batches.push(batch);
    }
    batch.surfaces.push(surface);
  }
  return batches;
}
export function packInstances(surfaces: RenderSurface[]): Float32Array {
  const out = new Float32Array(surfaces.length * INSTANCE_FLOATS);
  for (let i = 0; i < surfaces.length; i++) {
    const surface = surfaces[i];
    out.set(surface.matrix, i * INSTANCE_FLOATS);
    out.set(identityColor(surface.instanceId ?? surface.id), i * INSTANCE_FLOATS + 16);
  }
  return out;
}
