import { z } from "zod";
import { surfaceReliefSchema } from "./surface-relief";

const unit = z.number().finite().min(0).max(1);
const color = z.tuple([unit, unit, unit]);
/** Bounded, ordered surface recipes. Distances are metres in the material's coordinate domain. */
export const surfaceMaskSchema = z
  .object({
    kind: z.enum(["uniform", "noise", "slope", "height", "combined"]).default("noise"),
    scale: z.number().finite().min(0.01).max(100).default(1),
    threshold: unit.default(0.5),
    softness: z.number().finite().min(0.001).max(1).default(0.15),
    invert: z.boolean().default(false),
    minimumHeight: z.number().finite().min(-1e6).max(1e6).default(0),
    maximumHeight: z.number().finite().min(-1e6).max(1e6).default(2),
    // Combined masks intersect noise with independently weighted spatial restrictions.
    // Defaults remain optional in source so existing authored mask objects stay valid.
    slopeInfluence: unit.optional(),
    heightInfluence: unit.optional(),
    slopeThreshold: unit.optional(),
    slopeSoftness: z.number().finite().min(0.001).max(1).optional(),
  })
  .refine((mask) => mask.maximumHeight >= mask.minimumHeight, {
    message: "Maximum height must be at least minimum height",
  });
export const surfaceLayerSchema = z.object({
  id: z.string().min(1).max(100),
  name: z.string().min(1).max(120),
  enabled: z.boolean().default(true),
  color: color.default([0.3, 0.25, 0.16]),
  roughness: z.number().finite().min(0.04).max(1).default(0.8),
  metallic: unit.default(0),
  coverage: unit.default(1),
  relief: z.number().finite().min(-0.02).max(0.02).default(0),
  mask: surfaceMaskSchema.default(() => surfaceMaskSchema.parse({})),
});
export const surfaceAppearanceSchema = z
  .object({
    family: z.enum(["generic", "skin", "foliage", "fabric", "metal", "glass"]).default("generic"),
    /** Local-space geometry relief; independent of shader-only microdetail. */
    relief: surfaceReliefSchema.optional(),
    weathering: unit.default(0),
    wetness: unit.default(0),
    dirt: unit.default(0),
    damage: unit.default(0),
    historyScale: z.number().finite().min(0.01).max(100).default(1),
    dirtColor: color.default([0.16, 0.11, 0.06]),
    damageColor: color.default([0.45, 0.42, 0.35]),
    transmission: unit.default(0.4),
    indexOfRefraction: z.number().finite().min(1).max(2.5).default(1.5),
    response: z
      .object({
        subsurface: unit.optional(),
        thickness: z.number().finite().min(0).max(1).optional(),
        scatterColor: color.optional(),
        sheen: unit.optional(),
        anisotropy: z.number().finite().min(-0.95).max(0.95).optional(),
        clearcoat: unit.optional(),
        clearcoatRoughness: z.number().finite().min(0.04).max(1).optional(),
      })
      .optional(),
    detail: z
      .object({
        kind: z.enum(["none", "wood", "bark", "mineral", "soil", "birch-bark"]).default("none"),
        scale: z.number().finite().min(0.01).max(100).default(1),
        strength: unit.default(0.5),
      })
      .default({ kind: "none", scale: 1, strength: 0.5 }),
    layers: z.array(surfaceLayerSchema).max(4).default([]),
  })
  .refine(
    (appearance) => new Set(appearance.layers.map((layer) => layer.id)).size === appearance.layers.length,
    { message: "Surface layer identifiers must be unique" },
  );
export type SurfaceAppearance = z.infer<typeof surfaceAppearanceSchema>;
export type SurfaceLayer = z.infer<typeof surfaceLayerSchema>;
export type SurfaceMask = z.infer<typeof surfaceMaskSchema>;
/** Defaults stay in one place so the authoring controls display the response actually rendered. */
export function surfaceResponseDefaults(family: SurfaceAppearance["family"]) {
  return {
    subsurface: family === "skin" ? 0.35 : family === "foliage" ? 0.08 : 0,
    thickness: family === "foliage" ? 0.0005 : 0.005,
    scatterColor: (family === "foliage" ? [0.72, 1, 0.45] : [1, 0.4, 0.25]) as [number, number, number],
    sheen: family === "fabric" ? 0.4 : 0,
    anisotropy: 0,
    clearcoat: 0,
    clearcoatRoughness: 0.12,
  };
}
export function createSurfaceAppearance(family: SurfaceAppearance["family"] = "generic"): SurfaceAppearance {
  return surfaceAppearanceSchema.parse({ family });
}
export function createSurfaceLayer(id: string): SurfaceLayer {
  return surfaceLayerSchema.parse({ id, name: "Surface layer" });
}
