import type { SurfaceAppearance, Vec3 } from "@wrela/model";

export const SURFACE_APPEARANCE_FLOATS = 124;
export const SURFACE_LAYER_FLOATS = 24;
export const SURFACE_FAMILY = { generic: 1, skin: 2, foliage: 3, fabric: 4, metal: 5, glass: 6 } as const;
const kinds = { uniform: 0, noise: 1, slope: 2, height: 3, combined: 4 } as const;
const originAt = (origin: Vec3, frequency: number) =>
  origin.map((value) => (((value * frequency) % 1024) + 1024) % 1024);
/** Seven header vec4s and four six-vec4 layers, appended after the legacy water/creature layout. */
export function packSurfaceAppearance(
  appearance?: SurfaceAppearance,
  origin: Vec3 = [0, 0, 0],
  world = false,
): Float32Array {
  const out = new Float32Array(SURFACE_APPEARANCE_FLOATS);
  if (!appearance) return out;
  out.set([SURFACE_FAMILY[appearance.family], appearance.wetness, appearance.dirt, appearance.damage]);
  out.set(
    [appearance.weathering, appearance.historyScale, appearance.transmission, appearance.indexOfRefraction],
    4,
  );
  out.set([...appearance.dirtColor, appearance.layers.length], 8);
  out.set([...appearance.damageColor, 0], 12);
  if (world) out.set(originAt(origin, appearance.historyScale), 16);
  out.set(
    [
      { none: 0, wood: 1, bark: 2, mineral: 3, soil: 4, "birch-bark": 5 }[appearance.detail.kind],
      appearance.detail.scale,
      appearance.detail.strength,
      0,
    ],
    20,
  );
  // Every detail frequency is an integer multiple of 1/20. Reduce that shared
  // lattice first: reducing each coordinate before fractional anisotropy loses phase.
  if (world) {
    const detailOrigin = originAt(origin, appearance.detail.scale / 20);
    out.set(detailOrigin.map(Math.floor), 24);
    // Keep fractions separate: multiplying a large float32 remainder by an
    // anisotropic frequency otherwise loses visible grain phase. These header
    // w slots were reserved; no object-buffer growth is required.
    out[15] = detailOrigin[0] - Math.floor(detailOrigin[0]);
    out[19] = detailOrigin[1] - Math.floor(detailOrigin[1]);
    out[23] = detailOrigin[2] - Math.floor(detailOrigin[2]);
    const tau = Math.PI * 2;
    out[27] = (((origin[1] * appearance.detail.scale * 13.82) % tau) + tau) % tau;
  }
  for (const [index, layer] of appearance.layers.entries()) {
    const offset = 28 + index * SURFACE_LAYER_FLOATS;
    out.set([...layer.color, layer.roughness], offset);
    out.set(
      [layer.metallic, layer.enabled ? layer.coverage : 0, layer.relief, kinds[layer.mask.kind]],
      offset + 4,
    );
    out.set(
      [layer.mask.scale, layer.mask.threshold, layer.mask.softness, Number(layer.mask.invert)],
      offset + 8,
    );
    // Height bounds are relative to the current camera origin; noise receives a periodic origin.
    out.set(
      [
        layer.mask.minimumHeight - (world ? origin[1] : 0),
        layer.mask.maximumHeight - (world ? origin[1] : 0),
        0,
        0,
      ],
      offset + 12,
    );
    if (world) out.set(originAt(origin, layer.mask.scale), offset + 16);
    out.set(
      [
        layer.mask.slopeInfluence ?? 1,
        layer.mask.heightInfluence ?? 1,
        layer.mask.slopeThreshold ?? 0.5,
        layer.mask.slopeSoftness ?? 0.15,
      ],
      offset + 20,
    );
  }
  return out;
}
