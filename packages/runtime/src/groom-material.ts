import type { RenderMaterial, Vec3 } from "@wrela/model";

/** Groom vertex colors are authored absolute linear albedo. An optional material
 * contributes optics and relative pattern tint, never a second base-albedo factor. */
export function resolveGroomMaterial(generated: RenderMaterial, source?: RenderMaterial): RenderMaterial {
  if (!source) return generated;
  const relative = (color: Vec3): Vec3 =>
    color.map((value, axis) => (source.color[axis] > 1e-6 ? value / source.color[axis] : 1)) as Vec3;
  return {
    ...source,
    color: [1, 1, 1],
    secondary: relative(source.secondary),
    layers: source.layers?.map((layer) => ({ ...layer, color: relative(layer.color) })),
    creature: {
      ...generated.creature,
      ...source.creature,
      family: "fiber",
      fiberDirection: generated.creature?.fiberDirection,
    },
  };
}
