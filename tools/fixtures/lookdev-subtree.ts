import type { RenderSurface } from "@wrela/model";

/** Review-only isolation retains source vertex attributes and excludes every unrelated triangle. */
export function isolateLookdevSubtree(surfaces: RenderSurface[], prefix: string): RenderSurface[] {
  const isolated = surfaces.flatMap<RenderSurface>((surface) => {
    if (surface.mesh.shoots) {
      const indices = surface.mesh.shoots.sourceIds.flatMap((id, i) =>
        id === prefix || id.startsWith(`${prefix}/`) ? [i] : [],
      );
      return indices.length
        ? [
            {
              ...surface,
              id: `${surface.id}/review/${prefix}`,
              shootSelection: {
                indices: new Uint32Array(indices),
                masks: new Uint8Array(indices.length).fill(3),
              },
            },
          ]
        : [];
    }
    const indices: number[] = [];
    for (let i = 0; i < surface.mesh.indices.length; i += 3) {
      const source = surface.mesh.sourceIds?.[surface.mesh.indices[i]];
      if (source === prefix || source?.startsWith(`${prefix}/`))
        indices.push(surface.mesh.indices[i], surface.mesh.indices[i + 1], surface.mesh.indices[i + 2]);
    }
    return indices.length
      ? [
          {
            ...surface,
            id: `${surface.id}/review/${prefix}`,
            mesh: { ...surface.mesh, indices: new Uint32Array(indices) },
            details: undefined,
            drawRange: undefined,
            shadowDrawRange: undefined,
            renderProducts: undefined,
            selectedRenderProduct: undefined,
            opaqueVisibility: undefined,
          },
        ]
      : [];
  });
  if (!isolated.length) throw Error(`No source subtree ${prefix}`);
  return isolated;
}
