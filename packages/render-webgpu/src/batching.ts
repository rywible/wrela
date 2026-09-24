import { identityColor, type RenderSurface } from "@wrela/model";

import type { SurfaceVisibility } from "./visibility";
export const MAX_BATCH_INSTANCES = 256;
export const INSTANCE_FLOATS = 64;
export const surfaceInstanceCount = (surface: RenderSurface) =>
  surface.mesh.shoots ? (surface.shootSelection?.indices.length ?? surface.mesh.shoots.sourceIds.length) : 1;
export type SurfaceBatch = { key: string; surfaces: RenderSurface[]; visibility: SurfaceVisibility };
/** Equal GPU uniforms and a shared immutable mesh permit instancing. Poses and waves stay isolated. */
export function batchSurfaces(
  surfaces: RenderSurface[],
  meshId: (surface: RenderSurface) => number,
  visibility: (surface: RenderSurface) => SurfaceVisibility = (surface) => ({
    camera: true,
    shadow: !surface.water,
  }),
  meshShadows = false,
): SurfaceBatch[] {
  const batches: SurfaceBatch[] = [];
  const active = new Map<string, SurfaceBatch>();
  const chunks = new Map<string, number>();
  const counts = new Map<SurfaceBatch, number>();
  // Share packed material signatures within this frame. A local cache also sees
  // in-place authoring edits on the next frame and never retains scene objects.
  const materialSources = new WeakMap<object, string>();
  for (const surface of surfaces) {
    const visible = visibility(surface);
    const range =
      surface.selectedRenderProduct?.kind === "analytic-quadric"
        ? { start: 0, count: 6 }
        : (surface.drawRange ?? { start: 0, count: surface.mesh.indices.length });
    let identity: string;
    if (surface.skin || surface.water || surface.deformation) {
      identity = `single:${surface.id}:${surface.selectedRenderProduct?.key ?? "mesh"}`;
    } else {
      let materialSource = materialSources.get(surface.material);
      if (materialSource === undefined) {
        materialSource = JSON.stringify(surface.material);
        materialSources.set(surface.material, materialSource);
      }
      const signature = JSON.stringify([
        materialSource,
        surface.wind ?? 0,
        !!surface.selected,
        !!surface.creatureInspection,
        surface.reliefAppearance,
        surface.localLightMask,
        surface.staticIndirectReceiver,
        surface.indirectSurfaceChart,
        !!surface.mesh.indirectProofs,
        surface.skyVisibilityWeight,
        surface.radianceProbes,
        surface.radianceWeights,
        surface.radianceWeight,
        !!surface.mesh.radianceProbes,
        !!surface.mesh.radianceFlatNormals,
        !!surface.mesh.radianceMixtures,
        surface.mesh.thinCoverage?.key,
        !!surface.mesh.materialCoordinates,
      ]);
      // The complete source signature is conservative: identical sources pack
      // identically, while different sources may decline an otherwise valid batch.
      // Avoid formatting hundreds of packed floats for every material each frame.
      identity = `${surface.selectedRenderProduct?.kind === "analytic-quadric" && !meshShadows ? "quadric" : meshId(surface)}:${signature}`;
    }
    const shadowRange = surface.shadowDrawRange ?? range;
    const prefix = `${identity}:${range.start}:${range.count}:${shadowRange.start}:${shadowRange.count}:${Number(visible.camera)}:${Number(visible.shadow)}`;
    let batch = active.get(prefix);
    if (
      !batch ||
      batch.surfaces.length === MAX_BATCH_INSTANCES ||
      surface.material.appearance?.family === "glass" ||
      (counts.get(batch) ?? 0) + surfaceInstanceCount(surface) > 65536
    ) {
      const chunk = chunks.get(prefix) ?? 0;
      chunks.set(prefix, chunk + 1);
      batch = { key: `${prefix}:${chunk}`, surfaces: [], visibility: visible };
      active.set(prefix, batch);
      batches.push(batch);
    }
    batch.surfaces.push(surface);
    counts.set(batch, (counts.get(batch) ?? 0) + surfaceInstanceCount(surface));
  }
  return batches;
}
export function packInstances(
  surfaces: RenderSurface[],
  previous?: (surface: RenderSurface) => Float32Array | undefined,
  previousSelection?: (surface: RenderSurface) => RenderSurface["shootSelection"],
): Float32Array {
  const out = new Float32Array(
    surfaces.reduce((n, surface) => n + surfaceInstanceCount(surface), 0) * INSTANCE_FLOATS,
  );
  let record = 0;
  for (const surface of surfaces) {
    const prior = previous?.(surface);
    const identity = identityColor(surface.instanceId ?? surface.id);
    const shoots = surface.mesh.shoots;
    const priorSelection = previousSelection?.(surface);
    const priorMasks =
      priorSelection && priorSelection !== surface.shootSelection
        ? new Map(Array.from(priorSelection.indices, (id, i) => [id, priorSelection.masks[i]]))
        : undefined;
    for (let i = 0; i < surfaceInstanceCount(surface); i++) {
      const offset = record++ * INSTANCE_FLOATS;
      out.set(identity, offset + 16);
      out[offset + 19] = Number(!!prior);
      if (!shoots) {
        out.set(surface.matrix, offset);
        out.set(prior ?? surface.matrix, offset + 20);
        continue;
      }
      const index = surface.shootSelection?.indices[i] ?? i,
        local = index * 16;
      if (priorMasks) out[offset + 19] = Number(!!prior && !!((priorMasks.get(index) ?? 0) & 1));
      for (const [matrix, destination] of [
        [surface.matrix, offset],
        [prior ?? surface.matrix, offset + 20],
      ] as const)
        for (let column = 0; column < 4; column++)
          for (let row = 0; row < 4; row++) {
            let value = 0;
            for (let k = 0; k < 4; k++)
              value += matrix[k * 4 + row] * shoots.transforms[local + column * 4 + k];
            out[destination + column * 4 + row] = value;
          }
      out.set(shoots.transforms.subarray(local, local + 16), offset + 36);
      out.set(shoots.anchors.subarray(index * 4, index * 4 + 4), offset + 52);
      out.set(shoots.motion.subarray(index * 4, index * 4 + 4), offset + 56);
      out[offset + 60] = 4 + (surface.shootSelection?.masks[i] ?? 3);
      out[offset + 61] = shoots.skyVisibility?.[index] ?? 1;
    }
  }
  return out;
}

/** Immutable shoot products and selections permit reusing a GPU packet while
 * camera and wind change. Only root poses and history validity are dynamic. */
export class InstancePacketCache {
  private sources: {
    mesh: RenderSurface["mesh"];
    selection: RenderSurface["shootSelection"];
    identity: string;
    matrix: Float32Array;
    prior?: Float32Array;
    priorSelection: RenderSurface["shootSelection"];
  }[] = [];
  private packet?: Float32Array;
  pack(
    surfaces: RenderSurface[],
    previous: (surface: RenderSurface) => Float32Array | undefined,
    previousSelection?: (surface: RenderSurface) => RenderSurface["shootSelection"],
  ): Float32Array {
    const priors = surfaces.map(previous);
    const selections = surfaces.map((s) => previousSelection?.(s));
    const same = (a: Float32Array | undefined, b: Float32Array | undefined) =>
      a === b || (!!a && !!b && a.length === b.length && a.every((v, i) => v === b[i]));
    if (
      this.packet &&
      this.sources.length === surfaces.length &&
      surfaces.every((s, i) => {
        const old = this.sources[i];
        return (
          old.mesh === s.mesh &&
          old.selection === s.shootSelection &&
          old.priorSelection === selections[i] &&
          old.identity === (s.instanceId ?? s.id) &&
          same(old.matrix, s.matrix) &&
          same(old.prior, priors[i])
        );
      })
    )
      return this.packet;
    this.sources = surfaces.map((s, i) => ({
      mesh: s.mesh,
      selection: s.shootSelection,
      identity: s.instanceId ?? s.id,
      matrix: s.matrix.slice(),
      prior: priors[i]?.slice(),
      priorSelection: selections[i],
    }));
    const priorMap = new Map(surfaces.map((s, i) => [s, priors[i]]));
    this.packet = packInstances(surfaces, (s) => priorMap.get(s), previousSelection);
    return this.packet;
  }
}
