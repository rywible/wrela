import { z } from "zod";
import { geologicalProfileSchema } from "./geology-profiles";
import { geologicalRockSchema } from "./geology-rock";

export * from "./geology-profiles";
export * from "./geology-rock";

const finite = z.number().finite();
const coordinate = finite.min(-1_000_000).max(1_000_000);
const point = z.tuple([coordinate, coordinate]);
const id = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[a-zA-Z0-9_-]+$/);
/** Local, bounded geological features; world coordinates are metres. */
export const landformSchema = z
  .object({
    id,
    kind: z.enum(["ridge", "drainage", "cliff"]),
    points: z.array(point).min(2).max(32),
    width: finite.min(0.5).max(500),
    height: finite.min(0).max(100),
    falloff: finite.min(0.25).max(8),
    profile: geologicalProfileSchema.optional(),
  })
  .superRefine((value, context) => {
    if (value.profile && value.kind !== (value.profile.kind === "river" ? "drainage" : "cliff"))
      context.addIssue({
        code: "custom",
        path: ["profile"],
        message: "Cross-section kind must match the landform",
      });
  });
export const geologyFormationSchema = z.object({
  id,
  kind: z.enum(["cave", "overhang"]),
  material: z
    .string()
    .min(1)
    .max(100)
    .regex(/^[a-zA-Z0-9_-]+$/)
    .optional(),
  position: z.tuple([coordinate, finite.min(-200).max(500), coordinate]),
  /** Full exterior dimensions. Finite formations sit on top of heightfield terrain. */
  size: z.tuple([finite.min(4).max(100), finite.min(4).max(100), finite.min(4).max(100)]),
  opening: finite.min(0.2).max(0.8),
  /** Heading rotates the complete solid and opening together, in radians. */
  heading: finite.min(-Math.PI).max(Math.PI).optional(),
  /** Stratified, fractured exteriors stay within the authored dimensions. */
  rock: geologicalRockSchema.optional(),
  resolution: z.number().int().min(24).max(64),
});
/** An explicit elevation profile with a flat-width core and blended shoulders. */
export const geologyCorridorSchema = z
  .object({
    id,
    points: z
      .array(z.tuple([coordinate, finite.min(-5_000).max(5_000), coordinate]))
      .min(2)
      .max(32),
    halfWidth: finite.min(0.2).max(50),
    shoulder: finite.min(0.2).max(100),
  })
  .superRefine((value, context) => {
    for (let index = 1; index < value.points.length; index++) {
      const a = value.points[index - 1],
        b = value.points[index];
      if (a[0] === b[0] && a[2] === b[2] && a[1] !== b[1])
        context.addIssue({
          code: "custom",
          path: ["points", index],
          message: "A zero-length corridor segment cannot change elevation",
        });
    }
  });
export const terrainGeologySchema = z
  .object({
    landforms: z.array(landformSchema).max(24),
    erosion: z.object({
      strength: finite.min(0).max(1),
      radius: finite.min(0.25).max(20),
      talusAngle: finite.min(5).max(80),
    }),
    strata: z.object({ thickness: finite.min(0.25).max(20), strength: finite.min(0).max(1) }),
    /** A waterway may tint its adjacent terrain without changing collision or height. */
    bankWetness: z
      .object({
        drainageId: id,
        waterHalfWidth: finite.min(0.1).max(250),
        fadeWidth: finite.min(0.1).max(50),
        darkening: finite.min(0).max(0.8),
      })
      .optional(),
    formations: z.array(geologyFormationSchema).max(8),
    /** Grade-preserving routes apply after geological filters and before local interventions. */
    corridors: z.array(geologyCorridorSchema).max(16).optional(),
    review: z.object({
      route: z.array(point).min(2).max(32),
      maxSlope: finite.min(0).max(80),
      eyeHeight: finite.min(0.1).max(5),
      clearance: finite.min(0.1).max(10),
      bodyRadius: finite.min(0).max(2).optional(),
    }),
  })
  .superRefine((value, context) => {
    for (const key of ["landforms", "formations", "corridors"] as const) {
      const seen = new Set<string>();
      (value[key] ?? []).forEach((entry, index) => {
        if (seen.has(entry.id))
          context.addIssue({
            code: "custom",
            path: [key, index, "id"],
            message: "Feature IDs must be unique",
          });
        seen.add(entry.id);
      });
    }
    if (
      value.bankWetness &&
      !value.landforms.some((form) => form.id === value.bankWetness?.drainageId && form.kind === "drainage")
    )
      context.addIssue({
        code: "custom",
        path: ["bankWetness", "drainageId"],
        message: "Wet bank must reference a drainage landform",
      });
  });
export type TerrainGeology = z.infer<typeof terrainGeologySchema>;
export type Landform = z.infer<typeof landformSchema>;
export type GeologyFormation = z.infer<typeof geologyFormationSchema>;
export type GeologyCorridor = z.infer<typeof geologyCorridorSchema>;
export function defaultTerrainGeology(): TerrainGeology {
  return {
    landforms: [],
    erosion: { strength: 0, radius: 2, talusAngle: 35 },
    strata: { thickness: 2, strength: 0 },
    formations: [],
    review: {
      route: [
        [-10, 0],
        [10, 0],
      ],
      maxSlope: 35,
      eyeHeight: 1.7,
      clearance: 2,
      bodyRadius: 0.35,
    },
  };
}
